//! Explicit persistent raster relationships, independent of painting selection.
use crate::application::{ActionError, ActionResult, Session};
use serde_json::{json, Value};

pub(crate) fn ids_schema() -> Value {
    json!({"type":"array","minItems":1,"maxItems":16,
        "items":{"type":"integer","minimum":1,"maximum":4294967295u64},
        "description":"Move exactly these whole rasters and their owned linked masks; IDs are not implicitly expanded. Omitted/auto/layer target only; mutually exclusive with mask_id. Overrides target preferences."})
}

pub(super) fn ids(value: &Value, code: &str) -> Result<Vec<u64>, ActionError> {
    let invalid = || ActionError::new(code, "Supply one to sixteen unique positive layer IDs");
    let values = value.as_array().ok_or_else(invalid)?;
    if values.is_empty() || values.len() > 16 {
        return Err(invalid());
    }
    let mut ids = Vec::with_capacity(values.len());
    for value in values {
        let id = super::bounded_integer(value)
            .filter(|id| *id != 0)
            .ok_or_else(invalid)?;
        if ids.contains(&id) {
            return Err(invalid());
        }
        ids.push(id);
    }
    Ok(ids)
}

pub(crate) fn validate(args: &Value) -> Result<(), ActionError> {
    let targets = ids(&args["layer_ids"], "invalid_layer_links")?;
    let linked = args["linked"]
        .as_bool()
        .ok_or_else(|| ActionError::new("invalid_layer_links", "linked requires a boolean"))?;
    if linked && targets.len() < 2 {
        return Err(ActionError::new(
            "invalid_layer_links",
            "Link at least two raster layers",
        ));
    }
    Ok(())
}

pub(crate) fn query(s: &Session) -> Value {
    json!({
        "version":1,"revision":s.revision,"layer_links":s.doc.layer_link_sets(),
        "active_layer_id":s.doc.layer_id(s.doc.active),
        "active_linked_layer_ids":s.doc.linked_layer_ids(s.doc.layer_id(s.doc.active)).unwrap_or_default(),
        "selected_layer_ids":s.doc.selected_layer_ids(),
        "drawing":s.is_drawing()
    })
}

pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    validate(&args)?;
    // A metadata operation must not turn a rejected command into a synthetic UP.
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before changing layer links",
        ));
    }
    let targets = ids(&args["layer_ids"], "invalid_layer_links")?;
    s.doc
        .set_layer_links(&targets, args["linked"].as_bool().unwrap())
        .map_err(|error| ActionError::new("layer_links", error))?;
    Ok(s.state())
}
