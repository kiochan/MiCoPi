//! One cold, explicitly targeted raster/mask set. Typed painting stays single-target.
use crate::application::{ActionError, ActionResult, Operation, Session};
use micopi_plugin_api as abi;
use serde_json::{json, Value};

pub(super) fn validate(args: &Value) -> Result<(), ActionError> {
    if let Some(value) = args.get("layer_ids") {
        super::layer_links::ids(value, "invalid_arguments")?;
        if args.get("mask_id").is_some()
            || args
                .get("target")
                .is_some_and(|target| target != "auto" && target != "layer")
        {
            return Err(ActionError::new(
                "invalid_arguments",
                "layer_ids requires a whole-layer target and cannot be combined with mask_id",
            ));
        }
    }
    Ok(())
}

pub(crate) fn admit(s: &Session, args: &Value) -> Result<(), ActionError> {
    validate(args)?;
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before moving multiple layers",
        ));
    }
    Ok(())
}

fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("movement", message)
}

pub(crate) fn preflight(s: &Session, ids: &[u64]) -> Result<Vec<(u64, Option<u64>)>, ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before moving multiple layers",
        ));
    }
    let pairs = s.doc.validate_owner_moves(ids).map_err(error)?;
    super::native::require_movement(s, true, false).map_err(error)?;
    if pairs.iter().any(|(_, mask)| mask.is_some()) && !super::mask_geometry::available(s) {
        return Err(error(
            "Painting DLL does not support raw raster-mask geometry",
        ));
    }
    Ok(pairs)
}

pub(super) fn execute(s: &mut Session, op: Operation, args: &Value) -> ActionResult {
    admit(s, args)?;
    let ids = super::layer_links::ids(&args["layer_ids"], "invalid_arguments")?;
    let pairs = preflight(s, &ids)?;
    let (dx, dy) = super::owner_movement::offsets(s, op, args);
    if (dx, dy) != (0, 0) {
        // All sources, callbacks and aggregate output costs were checked before
        // the first allocation. No result is published until every pair exists.
        let options = abi::Move { kind: 0, dx, dy };
        let mut outputs = Vec::with_capacity(pairs.len());
        for (id, mask) in pairs {
            let index = s
                .doc
                .layer_index(id)
                .ok_or_else(|| error("Layer does not exist"))?;
            let pixels = super::native::move_layer_pixels(s, index, &options).map_err(error)?;
            let mask = mask
                .map(|id| super::owner_movement::stage_mask(s, id, dx, dy).map(|plane| (id, plane)))
                .transpose()
                .map_err(error)?;
            outputs.push((id, pixels, mask));
        }
        s.doc.apply_owner_moves(outputs).map_err(error)?;
    }
    Ok(s.state())
}

pub(super) fn context(s: &Session, op: Operation, args: &Value) -> Value {
    let (dx, dy) = super::owner_movement::offsets(s, op, args);
    let ids = super::layer_links::ids(&args["layer_ids"], "invalid_arguments").unwrap_or_default();
    let masks: Vec<_> = ids
        .iter()
        .filter_map(|id| {
            s.doc
                .mask_state()
                .items
                .iter()
                .find(|mask| {
                    mask.linked
                        && mask.owner == (crate::engine::raster_masks::MaskOwner::Layer { id: *id })
                })
                .map(|mask| mask.id)
        })
        .collect();
    json!({"target":"layers","layer_ids":ids,"linked_mask_ids":masks,"dx":dx,"dy":dy})
}
