//! Explicit affine owner scopes; source math stays in the painting DLL.
use super::affine::{Method, Parameters};
use crate::application::{ActionError, ActionResult, MoveTarget, Operation, Session};
use crate::engine::raster_masks::{PaintTarget, PlaneView, SamplePlane};
use crate::engine::transform::{OwnerScope, Recipe, Target};
use micopi_plugin_api as abi;
use serde_json::{json, Value};

pub(crate) struct OwnerPlan {
    pub scope: OwnerScope,
    pub bounds: [usize; 4],
    pub options: abi::Affine,
    pub pivot: [f64; 2],
    pub staged_bytes: usize,
    pub preview_cost: Option<crate::engine::OwnerPreviewCost>,
    pub preview_error: Option<String>,
}
pub(crate) struct OwnerOutputs {
    pub layers: Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)>,
    pub mask_only: Option<(u64, SamplePlane)>,
}
fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("transform", message)
}

pub(crate) fn validate_literal(args: &Value) -> Result<(), ActionError> {
    if let Some(ids) = args.get("layer_ids") {
        super::layer_links::ids(ids, "invalid_arguments")?;
        if args.get("mask_id").is_some()
            || args
                .get("target")
                .is_some_and(|v| v != "auto" && v != "layer")
        {
            return Err(ActionError::new(
                "invalid_arguments",
                "layer_ids requires a whole-layer target and cannot be combined with mask_id",
            ));
        }
    }
    if let Some(id) = args.get("mask_id") {
        if super::bounded_integer(id).is_none_or(|id| id == 0)
            || args.get("target").is_some_and(|v| v != "auto")
        {
            return Err(ActionError::new(
                "invalid_arguments",
                "mask_id requires a positive stable mask ID and an omitted or auto target",
            ));
        }
    }
    if args.get("quad").is_some()
        && (args.get("layer_ids").is_some() || args.get("mask_id").is_some())
    {
        return Err(ActionError::new("invalid_arguments","Owner and linked transforms support affine controls; perspective corners are unavailable"));
    }
    Ok(())
}
pub(crate) fn decode(
    args: Value,
) -> Result<(Parameters, Option<Vec<u64>>, Option<u64>), ActionError> {
    validate_literal(&args)?;
    let ids = args
        .get("layer_ids")
        .map(|v| super::layer_links::ids(v, "invalid_arguments"))
        .transpose()?;
    let mask = args.get("mask_id").and_then(super::bounded_integer);
    let mut parameters = args;
    if let Some(object) = parameters.as_object_mut() {
        object.remove("layer_ids");
        object.remove("mask_id");
    }
    Ok((serde_json::from_value(parameters)?, ids, mask))
}
/// Explicit selectors override preferences. Omitted selectors never widen links.
pub(crate) fn resolve(
    s: &Session,
    target: MoveTarget,
    ids: Option<&[u64]>,
    mask: Option<u64>,
) -> Result<Option<OwnerScope>, String> {
    if let Some(ids) = ids {
        return s.doc.capture_owner_transform_scope(ids, None).map(Some);
    }
    let mask = mask.or_else(|| {
        if target == MoveTarget::Auto {
            match s.doc.mask_state().target {
                PaintTarget::Mask { id } => Some(id),
                _ => None,
            }
        } else {
            None
        }
    });
    if let Some(id) = mask {
        return s.doc.capture_owner_transform_scope(&[], Some(id)).map(Some);
    }
    let target = super::movement::resolved_target(s, target);
    if target == MoveTarget::Layer && s.doc.has_raster_masks() {
        s.doc
            .capture_owner_transform_scope(&[s.doc.layer_id(s.doc.active)], None)
            .map(Some)
    } else {
        Ok(None)
    }
}
pub(crate) fn resolve_parameters(
    s: &Session,
    p: Parameters,
    ids: Option<&[u64]>,
    mask: Option<u64>,
) -> Result<Option<OwnerScope>, String> {
    let target = if mask.is_some() {
        MoveTarget::Auto
    } else {
        p.target.unwrap_or(s.movement_options().target)
    };
    resolve(s, target, ids, mask)
}
pub(crate) fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before transforming raster owners",
        ))
    } else {
        Ok(())
    }
}
/// Shared admission precedes recording/checkpoint work and any local cleanup.
pub(crate) fn admit(s: &Session, op: Operation, args: &Value) -> Result<bool, ActionError> {
    if matches!(
        op,
        Operation::TransformRepeat | Operation::TransformRepeatCopy
    ) {
        if let Some(recipe) = s
            .doc
            .last_transform()
            .filter(|recipe| recipe.target == Target::Owners)
        {
            require_idle(s)?;
            if op == Operation::TransformRepeatCopy || args["duplicate"] == true {
                return Err(error("Copy Repeat is unavailable for owner-scoped transforms; repeat without duplication"));
            }
            s.doc
                .validate_owner_transform_scope(
                    &recipe
                        .owners
                        .ok_or_else(|| error("Owner recipe has no scope"))?,
                )
                .map_err(error)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let args = if op == Operation::TransformPreview {
        args["parameters"].clone()
    } else {
        args.clone()
    };
    let (p, ids, mask) = decode(args)?;
    let target = if mask.is_some() {
        MoveTarget::Auto
    } else {
        p.target.unwrap_or(s.movement_options().target)
    };
    let selected_mask = mask.or_else(|| {
        if target == MoveTarget::Auto {
            match s.doc.mask_state().target {
                PaintTarget::Mask { id } => Some(id),
                _ => None,
            }
        } else {
            None
        }
    });
    if ids.is_none()
        && selected_mask.is_some_and(|id| {
            s.doc.mask_state().items.iter().any(|item| {
                item.id == id
                    && matches!(
                        item.owner,
                        crate::engine::raster_masks::MaskOwner::Group { .. }
                    )
            })
        })
    {
        // Preserve the existing unsupported-domain boundary and its error code.
        return Ok(false);
    }
    if let Some(scope) = resolve_parameters(s, p, ids.as_deref(), mask).map_err(error)? {
        if op == Operation::TransformPixels {
            require_idle(s)?;
        }
        if p.quad.is_some() {
            return Err(error("Owner and linked transforms support affine controls; perspective corners are unavailable"));
        }
        p.matrix([0, 0, 1, 1]).map_err(error)?;
        super::native::require_owner_affine(
            s,
            !scope.layer_ids().is_empty(),
            !scope.mask_ids().is_empty(),
        )
        .map_err(error)?;
        return Ok(true);
    }
    Ok(false)
}
pub(crate) fn plan(s: &Session, scope: &OwnerScope, p: Parameters) -> Result<OwnerPlan, String> {
    if p.quad.is_some() {
        return Err("Owner and linked transforms support affine controls; perspective corners are unavailable".into());
    }
    s.doc.validate_owner_transform_scope(scope)?;
    let staged_bytes = s.doc.owner_transform_staging_bytes(scope)?;
    super::native::require_owner_affine(
        s,
        !scope.layer_ids().is_empty(),
        !scope.mask_ids().is_empty(),
    )?;
    let mut union: Option<[usize; 4]> = None;
    for bounds in scope
        .layer_ids()
        .iter()
        .map(|id| super::native::affine_layer_bounds(s, *id))
        .chain(
            scope
                .mask_ids()
                .iter()
                .map(|id| super::native::affine_mask_bounds(s, *id)),
        )
    {
        if let Some(bounds) = bounds? {
            union = Some(match union {
                None => bounds,
                Some(old) => [
                    old[0].min(bounds[0]),
                    old[1].min(bounds[1]),
                    old[2].max(bounds[2]),
                    old[3].max(bounds[3]),
                ],
            });
        }
    }
    let bounds = union.unwrap_or([0, 0, s.doc.width, s.doc.height]);
    finish_plan(s, scope, p, bounds, staged_bytes)
}
fn finish_plan(
    s: &Session,
    scope: &OwnerScope,
    p: Parameters,
    bounds: [usize; 4],
    staged_bytes: usize,
) -> Result<OwnerPlan, String> {
    if bounds[0] >= bounds[2]
        || bounds[1] >= bounds[3]
        || bounds[2] > s.doc.width
        || bounds[3] > s.doc.height
    {
        return Err("Owner transform bounds do not match the source canvas".into());
    }
    let (matrix, pivot) = p.matrix(bounds)?;
    crate::engine::homography::Mapping {
        matrix,
        denominator: [0., 0., 1.],
    }
    .corners(bounds)?;
    let (preview_cost, preview_error) = match s.doc.owner_preview_cost(scope.mask_ids()) {
        Ok(cost) => (Some(cost), None),
        Err(error) => (None, Some(error)),
    };
    Ok(OwnerPlan {
        scope: *scope,
        bounds,
        options: abi::Affine {
            kind: 0,
            method: if p.method == Method::Nearest { 0 } else { 1 },
            matrix,
        },
        pivot,
        staged_bytes,
        preview_cost,
        preview_error,
    })
}
pub(crate) fn action_args(scope: &OwnerScope, p: &Parameters) -> Value {
    let mut args = json!(p);
    if scope.is_mask_only() {
        args["mask_id"] = json!(scope.mask_ids()[0]);
        args["target"] = json!("auto");
    } else {
        args["layer_ids"] = json!(scope.layer_ids());
        args["target"] = json!("layer");
    }
    args
}
pub(crate) fn describe(plan: &OwnerPlan, s: &Session) -> Value {
    let mapping = crate::engine::homography::Mapping {
        matrix: plan.options.matrix,
        denominator: [0., 0., 1.],
    };
    json!({"target":if plan.scope.is_mask_only(){"raster_mask"}else{"layers"},"layer_ids":plan.scope.layer_ids(),"mask_ids":plan.scope.mask_ids(),"owner_scope":plan.scope,
        "source_bounds":plan.bounds,"matrix":plan.options.matrix,"denominator":[0.,0.,1.],"pivot_x":plan.pivot[0],"pivot_y":plan.pivot[1],"corners":mapping.corners(plan.bounds).unwrap(),"identity_matrix":mapping.identity(),
        "width":s.doc.width,"height":s.doc.height,"revision":s.revision,"drawing":s.is_drawing(),"contact_active":s.stroke.active()||s.pointer_checkpoint.is_some(),"staged_bytes":plan.staged_bytes,
        "preview_available":plan.preview_cost.is_some(),"preview_composite_bytes":plan.preview_cost.as_ref().map(|cost|cost.composite_bytes),"preview_effect_bytes":plan.preview_cost.as_ref().map(|cost|cost.effect_bytes),"preview_extra_bytes":plan.preview_cost.as_ref().map(|cost|cost.extra_bytes),"preview_error":plan.preview_error})
}
pub(crate) fn staged(s: &Session, plan: &OwnerPlan) -> Result<OwnerOutputs, String> {
    require_idle(s).map_err(|e| e.to_string())?;
    s.doc.validate_owner_transform_scope(&plan.scope)?;
    s.doc.owner_transform_staging_bytes(&plan.scope)?;
    super::native::require_owner_affine(
        s,
        !plan.scope.layer_ids().is_empty(),
        !plan.scope.mask_ids().is_empty(),
    )?;
    if plan.scope.is_mask_only() {
        let id = plan.scope.mask_ids()[0];
        return Ok(OwnerOutputs {
            layers: Vec::new(),
            mask_only: Some((id, super::native::affine_raw_mask(s, id, &plan.options)?)),
        });
    }
    let pairs = s.doc.validate_owner_transform_scope(&plan.scope)?;
    let mut layers = Vec::with_capacity(pairs.len());
    for (id, mask) in pairs {
        let index = s.doc.layer_index(id).ok_or("Layer does not exist")?;
        let pixels = super::native::affine_source(s, &plan.options, &s.doc.layers[index].pixels)?;
        let mask = mask
            .map(|id| super::native::affine_raw_mask(s, id, &plan.options).map(|plane| (id, plane)))
            .transpose()?;
        layers.push((id, pixels, mask));
    }
    Ok(OwnerOutputs {
        layers,
        mask_only: None,
    })
}
pub(crate) fn execute(s: &mut Session, scope: OwnerScope, p: Parameters) -> ActionResult {
    require_idle(s)?;
    s.transform_event = None;
    let plan = plan(s, &scope, p).map_err(error)?;
    let recipe = Recipe {
        matrix: plan.options.matrix,
        denominator: [0., 0., 1.],
        target: Target::Owners,
        method: p.method,
        owners: Some(scope),
    };
    if !recipe.mapping().identity() {
        let outputs = staged(s, &plan).map_err(error)?;
        s.doc
            .apply_owner_transform(outputs.layers, outputs.mask_only, recipe)
            .map_err(error)?;
    }
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":if scope.is_mask_only(){"raster_mask"}else{"layers"},"owner_scope":scope,"layer_ids":scope.layer_ids(),"mask_ids":scope.mask_ids(),"matrix":recipe.matrix,"pivot":plan.pivot,"source_bounds":plan.bounds,"method":p.method}}),
        );
    }
    Ok(s.state())
}
#[cfg(feature = "desktop")]
pub(crate) fn preview(
    s: &Session,
    scope: &OwnerScope,
    p: Parameters,
) -> Result<(Vec<u32>, crate::engine::selection::SelectionState), String> {
    require_idle(s).map_err(|e| e.to_string())?;
    let plan = plan(s, scope, p)?;
    render_preview(s, plan)
}
/// The desktop keeps source bounds with its captured document/revision. Reuse
/// them during debounced previews rather than rescanning every raw source frame.
#[cfg(feature = "desktop")]
pub(crate) fn preview_cached(
    s: &Session,
    scope: &OwnerScope,
    p: Parameters,
    source: (u64, u64),
    bounds: [usize; 4],
) -> Result<(Vec<u32>, crate::engine::selection::SelectionState), String> {
    require_idle(s).map_err(|e| e.to_string())?;
    if source != (s.doc.identity, s.revision) {
        return Err("Transform source has changed; reopen the controls".into());
    }
    if p.quad.is_some() {
        return Err("Owner perspective preview is unavailable".into());
    }
    s.doc.validate_owner_transform_scope(scope)?;
    let bytes = s.doc.owner_transform_staging_bytes(scope)?;
    super::native::require_owner_affine(
        s,
        !scope.layer_ids().is_empty(),
        !scope.mask_ids().is_empty(),
    )?;
    render_preview(s, finish_plan(s, scope, p, bounds, bytes)?)
}
#[cfg(feature = "desktop")]
fn render_preview(
    s: &Session,
    plan: OwnerPlan,
) -> Result<(Vec<u32>, crate::engine::selection::SelectionState), String> {
    if plan.preview_cost.is_none() {
        return Err(plan
            .preview_error
            .unwrap_or("Owner preview is unavailable".into()));
    }
    if (crate::engine::homography::Mapping {
        matrix: plan.options.matrix,
        denominator: [0., 0., 1.],
    })
    .identity()
    {
        return Ok((
            s.doc.preview_owner_transform(&[], &[])?,
            s.doc.selection().clone(),
        ));
    }
    let outputs = staged(s, &plan)?;
    let layers: Vec<_> = outputs
        .layers
        .iter()
        .map(|(id, pixels, _)| (*id, pixels.as_slice()))
        .collect();
    let mut masks: Vec<_> = outputs
        .layers
        .iter()
        .filter_map(|(_, _, mask)| mask.as_ref())
        .collect();
    masks.extend(outputs.mask_only.as_ref());
    let masks: Vec<_> = masks
        .into_iter()
        .map(|(id, plane)| {
            (
                *id,
                match plane {
                    SamplePlane::Uniform(v) => PlaneView::Uniform(*v),
                    SamplePlane::Dense(data) => PlaneView::Dense(data),
                },
            )
        })
        .collect();
    Ok((
        s.doc.preview_owner_transform(&layers, &masks)?,
        s.doc.selection().clone(),
    ))
}
pub(crate) fn repeat(s: &mut Session, recipe: Recipe, duplicate: bool) -> ActionResult {
    require_idle(s)?;
    if duplicate {
        return Err(error(
            "Copy Repeat is unavailable for owner-scoped transforms; repeat without duplication",
        ));
    }
    recipe.validate().map_err(error)?;
    let scope = recipe
        .owners
        .ok_or_else(|| error("Owner transform recipe has no captured scope"))?;
    let mut p = Parameters::default();
    p.method = recipe.method;
    let mut plan = plan(s, &scope, p).map_err(error)?;
    plan.options.matrix = recipe.matrix;
    recipe.mapping().corners(plan.bounds).map_err(error)?;
    let outputs = staged(s, &plan).map_err(error)?;
    s.transform_event = None;
    s.doc
        .apply_owner_transform(outputs.layers, outputs.mask_only, recipe)
        .map_err(error)?;
    if s.atomic_depth == 0 {
        s.transform_event = Some(
            json!({"transform":{"target":if scope.is_mask_only(){"raster_mask"}else{"layers"},"owner_scope":scope,"layer_ids":scope.layer_ids(),"mask_ids":scope.mask_ids(),"matrix":recipe.matrix,"source_bounds":plan.bounds,"method":recipe.method,"duplicate":false}}),
        );
    }
    Ok(s.state())
}
