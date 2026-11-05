//! Shared affine planning and atomic publication. Pixel algorithms belong to the DLL.
use crate::application::{ActionError, ActionResult, MoveTarget, Session};
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
            target: Target::Layer,
            method: self.method,
        }
        .validate()?;
        Ok((matrix, pivot))
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
    pub options: abi::Affine,
    pub pivot: [f64; 2],
    pub target: MoveTarget,
    pub bounds: [usize; 4],
}
pub(crate) fn staged(
    s: &Session,
    o: &abi::Affine,
) -> Result<(Option<Vec<u32>>, crate::engine::selection::SelectionState), String> {
    let mut selection = s.doc.selection().clone();
    if o.matrix == [1., 0., 0., 0., 1., 0.] {
        return Ok((None, selection));
    }
    if o.kind != 0 {
        selection.current = Some(super::native::affine_mask(s, o)?);
    }
    let pixels = if o.kind == 2 {
        None
    } else {
        Some(super::native::affine_pixels(s, o)?)
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
    let (pixels, selection) = staged(s, &plan.options)?;
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
    let (matrix, pivot) = p.matrix(bounds)?;
    Ok(Plan {
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
    let p: Parameters = serde_json::from_value(args)?;
    let plan = plan(s, p).map_err(|e| ActionError::new("transform", e))?;
    let m = plan.options.matrix;
    let b = plan.bounds;
    let corners = [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]].map(|[x, y]| {
        [
            m[0] * x as f64 + m[1] * y as f64 + m[2],
            m[3] * x as f64 + m[4] * y as f64 + m[5],
        ]
    });
    Ok(
        json!({"target":plan.target,"source_bounds":b,"matrix":m,"pivot_x":plan.pivot[0],"pivot_y":plan.pivot[1],"corners":corners,"identity_matrix":m==[1.,0.,0.,0.,1.,0.],"width":s.doc.width,"height":s.doc.height,"revision":s.revision,"drawing":s.is_drawing()}),
    )
}
fn run(s: &mut Session, args: Value) -> Result<(), String> {
    s.transform_event = None;
    let p: Parameters = serde_json::from_value(args).map_err(|e| e.to_string())?;
    let plan = plan(s, p)?;
    s.finish_stroke();
    let o = &plan.options;
    if o.matrix != [1., 0., 0., 0., 1., 0.] {
        let (pixels, selection) = staged(s, o)?;
        let recipe = Recipe {
            matrix: o.matrix,
            target: match plan.target {
                MoveTarget::Layer => Target::Layer,
                MoveTarget::Selection => Target::Selection,
                _ => Target::SelectionMask,
            },
            method: p.method,
        };
        s.doc
            .apply_affine(pixels.as_deref(), selection, Some(recipe))?;
    }
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":plan.target,"matrix":o.matrix,"pivot":plan.pivot,"source_bounds":plan.bounds,"method":p.method}}),
        );
    }
    Ok(())
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    run(s, args).map_err(|e| ActionError::new("transform", e))?;
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
    let p: Parameters = serde_json::from_value(args["parameters"].clone())?;
    p.matrix([0, 0, 1, 1])
        .map_err(|e| ActionError::new("invalid_arguments", e))?;
    s.ui_effects
        .push_back(json!({"id":"transform.parameters","value":p}));
    Ok(json!({"queued":true}))
}

/// Read-only, stable node/state contract. No stroke completion or native invocation.
pub(crate) fn repeat_state(s: &Session) -> Value {
    if let Some(recipe) = s.doc.last_transform() {
        let mut result = json!(recipe);
        result["available"] = json!(true);
        result
    } else {
        json!({"available":false})
    }
}
pub(crate) fn repeat(s: &mut Session, duplicate: bool) -> ActionResult {
    repeat_run(s, duplicate).map_err(|e| ActionError::new("transform", e))?;
    Ok(s.state())
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
    };
    if duplicate && target == MoveTarget::SelectionMask {
        return Err("Cannot duplicate a selection outline; repeat without duplication".into());
    }
    let bounds = source_bounds(s, target)?;
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
            selection.current = Some(super::native::affine_mask(s, &options)?);
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
            super::native::affine_source(s, &options, &extracted)?
        } else {
            super::native::affine_pixels(s, &options)?
        };
        s.doc
            .duplicate_affine_layer(s.doc.active, Some((pixels, selection)))?;
    } else {
        let (pixels, selection) = staged(s, &options)?;
        s.doc
            .apply_affine(pixels.as_deref(), selection, Some(recipe))?;
    }
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":target,"matrix":recipe.matrix,"source_bounds":bounds,"method":recipe.method,"duplicate":duplicate}}),
        );
    }
    Ok(())
}
