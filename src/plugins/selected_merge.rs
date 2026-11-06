//! Explicit selected-only layer baking and its read-only metadata plan.
use crate::application::{ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};

pub(crate) fn schema(op: Operation) -> Value {
    if op == Operation::MergeLayers {
        return json!({"type":"object","properties":{},"additionalProperties":false,
            "description":"Merge the current explicit layer selection, or Merge Down for a singleton. Requires an idle contact. Use layer.merge_selected with stable IDs for deterministic automation targets."});
    }
    json!({"type":"object","properties":{
        "layer_ids":{"type":"array","minItems":2,"maxItems":16,
            "items":{"type":"integer","minimum":1,"maximum":4294967295u64},
            "description":"Unique stable raster IDs. Targets may be noncontiguous or in different groups; baking samples only selected content and intermediate group gates over transparency, excluding the common parent/ancestors. Outside-backdrop and stacking interactions can change. Use the read-only plan to inspect placement and hidden consumption."}
    },"required":["layer_ids"],"additionalProperties":false})
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("selected_merge", message)
}
pub(crate) fn validate(args: &Value) -> Result<Vec<u64>, ActionError> {
    let ids: Vec<u64> = serde_json::from_value(args["layer_ids"].clone())?;
    if !(2..=16).contains(&ids.len())
        || ids
            .iter()
            .enumerate()
            .any(|(i, id)| *id == 0 || *id > u32::MAX as u64 || ids[..i].contains(id))
    {
        return Err(invalid("Supply between two and sixteen unique layer IDs"));
    }
    Ok(ids)
}
fn idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before merging selected layers",
        ));
    }
    Ok(())
}
fn plan(s: &Session, ids: &[u64]) -> ActionResult {
    let mut value = serde_json::to_value(s.doc.selected_merge_plan(ids).map_err(invalid)?)?;
    value["revision"] = json!(s.revision);
    value["drawing"] = json!(s.is_drawing());
    value["contact_active"] = json!(s.stroke.active() || s.pointer_checkpoint.is_some());
    Ok(value)
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::MergeLayers {
        idle(s)?;
        let ids = s.doc.selected_layer_ids();
        if ids.len() == 1 {
            let mut value = super::layers::execute(s, Operation::MergeDown, json!({}))?;
            value["layer_id"] = json!(s.doc.layer_id(s.doc.active));
            return Ok(value);
        }
        return execute(s, Operation::MergeSelectedLayers, json!({"layer_ids":ids}));
    }
    let ids = validate(&args)?;
    if op == Operation::MergeSelectedLayersPlan {
        return plan(s, &ids);
    }
    if op == Operation::OpenSelectedMerge {
        if s.runtime != "desktop" {
            return Err(ActionError::new("ui_unavailable", "Selected merge review requires the desktop; use layer.merge_selected.plan and layer.merge_selected for headless editing"));
        }
        if s.ui_effects.len() >= 128 {
            return Err(ActionError::new(
                "presentation_queue_full",
                "Wait for queued presentation commands before retrying",
            ));
        }
        let value = plan(s, &ids)?;
        s.ui_effects.push_back(json!({"id":"layer.merge_selected.open","value":value,"document_identity":s.doc.identity,"revision":s.revision}));
        return Ok(json!({"queued":true,"applied":false,"runtime":s.runtime}));
    }
    idle(s)?;
    let id = s.doc.merge_selected(&ids).map_err(invalid)?;
    s.doc.compose();
    if let Some(error) = &s.doc.render_error {
        return Err(ActionError::new("selected_merge_render", error));
    }
    let mut state = s.state();
    state["layer_id"] = json!(id);
    Ok(state)
}
