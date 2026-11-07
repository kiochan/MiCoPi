//! Shared affine planning and atomic publication. Pixel algorithms belong to the DLL.
use crate::application::{ActionError, ActionResult, MoveTarget, Session};
use crate::engine::homography::{self, Mapping};
pub(crate) use crate::engine::transform::Method;
use crate::engine::transform::{Recipe, Target};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Parameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<MoveTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quad: Option<[[f64; 2]; 4]>,
    pub translate_x: f64,
    pub translate_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub angle: f64,
    #[serde(skip_serializing_if = "zero")]
    pub skew_x: f64,
    #[serde(skip_serializing_if = "zero")]
    pub skew_y: f64,
    pub flip_x: bool,
    pub flip_y: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pivot_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pivot_y: Option<f64>,
    pub method: Method,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            target: None,
            quad: None,
            translate_x: 0.,
            translate_y: 0.,
            scale_x: 1.,
            scale_y: 1.,
            angle: 0.,
            skew_x: 0.,
            skew_y: 0.,
            flip_x: false,
            flip_y: false,
            pivot_x: None,
            pivot_y: None,
            method: Method::Bilinear,
        }
    }
}
fn zero(v: &f64) -> bool {
    *v == 0.
}
impl Parameters {
    pub(crate) fn matrix(&self, bounds: [usize; 4]) -> Result<([f64; 6], [f64; 2]), String> {
        if ![
            self.translate_x,
            self.translate_y,
            self.scale_x,
            self.scale_y,
            self.angle,
            self.skew_x,
            self.skew_y,
        ]
        .iter()
        .all(|v| v.is_finite())
            || self.translate_x.abs() > 8192.
            || self.translate_y.abs() > 8192.
            || !(0.01..=100.).contains(&self.scale_x)
            || !(0.01..=100.).contains(&self.scale_y)
            || self.angle.abs() > 360.
            || self.skew_x.abs() > 80.
            || self.skew_y.abs() > 80.
            || self.pivot_x.is_some() != self.pivot_y.is_some()
            || [self.pivot_x, self.pivot_y]
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 8192.)
        {
            return Err(
                "Invalid transform parameters; supply both pivot coordinates or neither".into(),
            );
        }
        let pivot = [
            self.pivot_x.unwrap_or((bounds[0] + bounds[2]) as f64 / 2.),
            self.pivot_y.unwrap_or((bounds[1] + bounds[3]) as f64 / 2.),
        ];
        let (sin, cos) = match self.angle {
            0. | 360. | -360. => (0., 1.),
            90. | -270. => (1., 0.),
            180. | -180. => (0., -1.),
            270. | -90. => (-1., 0.),
            _ => self.angle.to_radians().sin_cos(),
        };
        let sx = self.scale_x * if self.flip_x { -1. } else { 1. };
        let sy = self.scale_y * if self.flip_y { -1. } else { 1. };
        // Local scale/flip, horizontal shear, vertical shear, then rotation.
        let (hx, hy) = self.shears();
        let (a, b, d, e) = (
            (cos - sin * hy) * sx,
            (cos * hx - sin * (1. + hx * hy)) * sy,
            (sin + cos * hy) * sx,
            (sin * hx + cos * (1. + hx * hy)) * sy,
        );
        let matrix = [
            a,
            b,
            pivot[0] + self.translate_x - a * pivot[0] - b * pivot[1],
            d,
            e,
            pivot[1] + self.translate_y - d * pivot[0] - e * pivot[1],
        ];
        Recipe {
            matrix,
            denominator: [0., 0., 1.],
            target: Target::Layer,
            method: self.method,
            owners: None,
        }
        .validate()?;
        Ok((matrix, pivot))
    }
    pub(crate) fn mapping(&self, bounds: [usize; 4]) -> Result<(Mapping, [f64; 2]), String> {
        let (matrix, pivot) = self.matrix(bounds)?;
        let mapping = if let Some(quad) = self.quad {
            if self.translate_x != 0.
                || self.translate_y != 0.
                || self.scale_x != 1.
                || self.scale_y != 1.
                || self.angle != 0.
                || self.skew_x != 0.
                || self.skew_y != 0.
                || self.flip_x
                || self.flip_y
                || self.pivot_x.is_some()
                || self.pivot_y.is_some()
            {
                return Err("Perspective corners cannot be combined with affine controls".into());
            }
            homography::from_quad(bounds, quad)?
        } else {
            Mapping {
                matrix,
                denominator: [0., 0., 1.],
            }
        };
        Ok((mapping, pivot))
    }
    pub(crate) fn shears(&self) -> (f64, f64) {
        (
            self.skew_x.to_radians().tan(),
            self.skew_y.to_radians().tan(),
        )
    }
    /// Undo local rotation/shears without dividing out scale or flips.
    #[cfg(feature = "desktop")]
    pub(crate) fn unskew(&self, v: [f64; 2]) -> [f64; 2] {
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let (hx, hy) = self.shears();
        let x = cos * v[0] + sin * v[1];
        let y = -sin * v[0] + cos * v[1] - hy * x;
        [x - hx * y, y]
    }
}
pub(crate) struct Plan {
    pub denominator: [f64; 3],
    pub options: abi::Affine,
    pub pivot: [f64; 2],
    pub target: MoveTarget,
    pub bounds: [usize; 4],
}
pub(crate) fn staged(
    s: &Session,
    o: &abi::Affine,
    denominator: [f64; 3],
) -> Result<(Option<Vec<u32>>, crate::engine::selection::SelectionState), String> {
    let mut selection = s.doc.selection().clone();
    if (Mapping {
        matrix: o.matrix,
        denominator,
    })
    .identity()
    {
        return Ok((None, selection));
    }
    if o.kind != 0 {
        selection.current = Some(sample_mask(s, o, denominator)?);
    }
    let pixels = if o.kind == 2 {
        None
    } else {
        Some(sample_pixels(
            s,
            o,
            denominator,
            &s.doc.layers[s.doc.active].pixels,
        )?)
    };
    Ok((pixels, selection))
}
/// Cold, isolated render for the desktop draft. Never mutates Session/history/journal.
#[cfg(feature = "desktop")]
pub(crate) fn preview(
    s: &Session,
    p: Parameters,
) -> Result<(Vec<u32>, crate::engine::selection::SelectionState), String> {
    let plan = plan(s, p)?;
    let (pixels, selection) = staged(s, &plan.options, plan.denominator)?;
    Ok((
        s.doc
            .preview_pixel_transform(pixels.as_deref(), &selection)?,
        selection,
    ))
}
pub(crate) fn plan(s: &Session, p: Parameters) -> Result<Plan, String> {
    let target =
        super::movement::resolved_target(s, p.target.unwrap_or(s.movement_options().target));
    let bounds = source_bounds(s, target)?;
    let (mapping, pivot) = p.mapping(bounds)?;
    mapping.corners(bounds)?;
    let matrix = mapping.matrix;
    Ok(Plan {
        denominator: mapping.denominator,
        options: abi::Affine {
            kind: match target {
                MoveTarget::Layer => 0,
                MoveTarget::Selection => 1,
                _ => 2,
            },
            method: if p.method == Method::Nearest { 0 } else { 1 },
            matrix,
        },
        pivot,
        target,
        bounds,
    })
}
pub(crate) fn source_bounds(s: &Session, target: MoveTarget) -> Result<[usize; 4], String> {
    let target = super::movement::resolved_target(s, target);
    let mask_bounds = super::movement::preview_bounds(s, target)?;
    if target == MoveTarget::Layer {
        super::native::affine_bounds(s)
    } else {
        Ok(mask_bounds)
    }
}
pub(crate) fn describe(s: &Session, args: Value) -> ActionResult {
    let (p, ids, mask) = super::owner_affine::decode(args)?;
    if let Some(scope) = super::owner_affine::resolve_parameters(s, p, ids.as_deref(), mask)
        .map_err(|e| ActionError::new("transform", e))?
    {
        let plan = super::owner_affine::plan(s, &scope, p)
            .map_err(|e| ActionError::new("transform", e))?;
        return Ok(super::owner_affine::describe(&plan, s));
    }
    let plan = plan(s, p).map_err(|e| ActionError::new("transform", e))?;
    let m = plan.options.matrix;
    let b = plan.bounds;
    let mapping = Mapping {
        matrix: m,
        denominator: plan.denominator,
    };
    let corners = mapping
        .corners(b)
        .map_err(|e| ActionError::new("transform", e))?;
    Ok(
        json!({"target":plan.target,"source_bounds":b,"matrix":m,"pivot_x":plan.pivot[0],"pivot_y":plan.pivot[1],"corners":corners,"denominator":plan.denominator,"identity_matrix":mapping.identity(),"width":s.doc.width,"height":s.doc.height,"revision":s.revision,"drawing":s.is_drawing(),"layer_ids":if plan.target==MoveTarget::SelectionMask{Vec::<u64>::new()}else{vec![s.doc.layer_id(s.doc.active)]},"mask_ids":Vec::<u64>::new()}),
    )
}
fn run(s: &mut Session, p: Parameters) -> Result<(), String> {
    s.transform_event = None;
    let plan = plan(s, p)?;
    s.finish_stroke();
    let o = &plan.options;
    if !(Mapping {
        matrix: o.matrix,
        denominator: plan.denominator,
    })
    .identity()
    {
        let (pixels, selection) = staged(s, o, plan.denominator)?;
        let recipe = Recipe {
            matrix: o.matrix,
            denominator: plan.denominator,
            target: match plan.target {
                MoveTarget::Layer => Target::Layer,
                MoveTarget::Selection => Target::Selection,
                _ => Target::SelectionMask,
            },
            method: p.method,
            owners: None,
        };
        s.doc
            .apply_affine(pixels.as_deref(), selection, Some(recipe))?;
    }
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":plan.target,"matrix":o.matrix,"pivot":plan.pivot,"source_bounds":plan.bounds,"method":p.method}}),
        );
        if plan.denominator != [0., 0., 1.] {
            s.transform_event.as_mut().unwrap()["transform"]["denominator"] =
                json!(plan.denominator);
        }
    }
    Ok(())
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    if args.get("quad").is_some() {
        require_idle(s)?;
    }
    let (parameters, ids, mask) = super::owner_affine::decode(args)?;
    if let Some(scope) =
        super::owner_affine::resolve_parameters(s, parameters, ids.as_deref(), mask)
            .map_err(|e| ActionError::new("transform", e))?
    {
        return super::owner_affine::execute(s, scope, parameters);
    }
    let target = super::movement::resolved_target(
        s,
        parameters.target.unwrap_or(s.movement_options().target),
    );
    if target != MoveTarget::SelectionMask {
        super::PixelAccess::Geometry.require(&s.doc, s.doc.active)?;
    }
    run(s, parameters).map_err(|e| ActionError::new("transform", e))?;
    Ok(s.state())
}
pub(crate) fn request_preview(s: &mut Session, args: Value) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new("ui_unavailable","Free Transform controls require the desktop; use canvas.transform for headless editing"));
    }
    if s.ui_effects.len() >= 128 {
        return Err(ActionError::new(
            "presentation_queue_full",
            "Wait for queued presentation commands before retrying",
        ));
    }
    let (p, ids, mask) = super::owner_affine::decode(args["parameters"].clone())?;
    let scope = super::owner_affine::resolve_parameters(s, p, ids.as_deref(), mask)
        .map_err(|e| ActionError::new("transform", e))?;
    if let Some(scope) = scope {
        super::owner_affine::plan(s, &scope, p).map_err(|e| ActionError::new("transform", e))?;
    } else if p.quad.is_some() {
        plan(s, p).map_err(|e| ActionError::new("invalid_arguments", e))?;
    } else {
        p.matrix([0, 0, 1, 1])
            .map_err(|e| ActionError::new("invalid_arguments", e))?;
    }
    let value = scope
        .as_ref()
        .map(|scope| super::owner_affine::action_args(scope, &p))
        .unwrap_or_else(|| json!(p));
    let mut effect = json!({"id":"transform.parameters","value":value});
    if scope.is_some() {
        effect["document_identity"] = json!(s.doc.identity);
        effect["revision"] = json!(s.revision);
    }
    s.ui_effects.push_back(effect);
    Ok(json!({"queued":true}))
}

/// Read-only, stable node/state contract. No stroke completion or native invocation.
pub(crate) fn repeat_state(s: &Session) -> Value {
    if let Some(recipe) = s.doc.last_transform() {
        let mut result = json!(recipe);
        result["denominator"] = json!(recipe.denominator);
        result["available"] = json!(true);
        result
    } else {
        json!({"available":false})
    }
}
pub(crate) fn repeat(s: &mut Session, duplicate: bool) -> ActionResult {
    if let Some(recipe) = s
        .doc
        .last_transform()
        .filter(|recipe| recipe.target == Target::Owners)
    {
        return super::owner_affine::repeat(s, recipe, duplicate);
    }
    if !duplicate
        && s.doc
            .last_transform()
            .is_some_and(|r| r.target != Target::SelectionMask)
    {
        super::PixelAccess::Geometry.require(&s.doc, s.doc.active)?;
    }
    if s.doc
        .last_transform()
        .is_some_and(|r| r.denominator != [0., 0., 1.])
    {
        require_idle(s)?;
    }
    repeat_run(s, duplicate).map_err(|e| ActionError::new("transform", e))?;
    Ok(s.state())
}
fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before perspective editing",
        ));
    }
    Ok(())
}
fn repeat_run(s: &mut Session, duplicate: bool) -> Result<(), String> {
    s.transform_event = None;
    let recipe = s
        .doc
        .last_transform()
        .ok_or("No saved transform to repeat")?;
    recipe.validate()?;
    let target = match recipe.target {
        Target::Layer => MoveTarget::Layer,
        Target::Selection => MoveTarget::Selection,
        Target::SelectionMask => MoveTarget::SelectionMask,
        Target::Owners => return Err("Owner transforms require their captured scope".into()),
    };
    if duplicate && target == MoveTarget::SelectionMask {
        return Err("Cannot duplicate a selection outline; repeat without duplication".into());
    }
    let bounds = source_bounds(s, target)?;
    recipe.mapping().corners(bounds)?;
    if duplicate
        && (s.doc.layers.len() >= crate::engine::MAX_LAYERS
            || s.doc.width * s.doc.height * (s.doc.layers.len() + 1) > 64 * 1024 * 1024)
    {
        return Err("Layer count or document pixel limit reached".into());
    }
    s.finish_stroke();
    let mut options = abi::Affine {
        matrix: recipe.matrix,
        kind: match target {
            MoveTarget::Layer => 0,
            MoveTarget::Selection => 1,
            _ => 2,
        },
        method: if recipe.method == Method::Nearest {
            0
        } else {
            1
        },
    };
    if duplicate {
        let mut selection = s.doc.selection().clone();
        let pixels = if target == MoveTarget::Selection {
            selection.current = Some(sample_mask(s, &options, recipe.denominator)?);
            let mut extracted = vec![0; s.doc.width * s.doc.height];
            super::native::transfer(
                s,
                &abi::SelectionTransfer {
                    kind: 0,
                    x: 0,
                    y: 0,
                },
                &s.doc.layers[s.doc.active].pixels,
                [s.doc.width, s.doc.height],
                s.doc.selection().current.as_ref(),
                &mut extracted,
                [s.doc.width, s.doc.height],
            )?;
            options.kind = 0;
            sample_pixels(s, &options, recipe.denominator, &extracted)?
        } else {
            sample_pixels(
                s,
                &options,
                recipe.denominator,
                &s.doc.layers[s.doc.active].pixels,
            )?
        };
        s.doc
            .duplicate_affine_layer(s.doc.active, Some((pixels, selection)))?;
    } else {
        let (pixels, selection) = staged(s, &options, recipe.denominator)?;
        s.doc
            .apply_affine(pixels.as_deref(), selection, Some(recipe))?;
    }
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":target,"matrix":recipe.matrix,"source_bounds":bounds,"method":recipe.method,"duplicate":duplicate}}),
        );
        if recipe.denominator != [0., 0., 1.] {
            s.transform_event.as_mut().unwrap()["transform"]["denominator"] =
                json!(recipe.denominator);
        }
    }
    Ok(())
}

fn projective(o: &abi::Affine, denominator: [f64; 3]) -> abi::Projective {
    abi::Projective {
        kind: o.kind,
        method: o.method,
        matrix: Mapping {
            matrix: o.matrix,
            denominator,
        }
        .full(),
    }
}
fn sample_pixels(
    s: &Session,
    o: &abi::Affine,
    denominator: [f64; 3],
    source: &[u32],
) -> Result<Vec<u32>, String> {
    if denominator == [0., 0., 1.] {
        super::native::affine_source(s, o, source)
    } else {
        super::native::projective_source(s, &projective(o, denominator), source)
    }
}
fn sample_mask(
    s: &Session,
    o: &abi::Affine,
    denominator: [f64; 3],
) -> Result<crate::engine::selection::Mask, String> {
    if denominator == [0., 0., 1.] {
        super::native::affine_mask(s, o)
    } else {
        super::native::projective_mask(s, &projective(o, denominator))
    }
}
