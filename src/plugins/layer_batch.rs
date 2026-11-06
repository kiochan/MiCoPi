//! Bounded multi-layer orchestration through the existing shared action handlers.
use crate::application::{Action, ActionError, ActionResult, Operation, Session};
use crate::engine::graph::BlendMode;
use serde::Deserialize;
use serde_json::{json, Value};

const ACTIONS: &[&str] = &[
    "layer.set",
    "layer.duplicate",
    "layer.delete",
    "layer.move",
    "layer.clear",
    "layer.flip_horizontal",
    "layer.flip_vertical",
    "layer.rotate_180",
    "layer.translate",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    action: String,
    layer_ids: Vec<u64>,
    #[serde(default = "crate::application::actions_empty_object")]
    args: Value,
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("invalid_layer_batch", message)
}
fn target_schema(action: &str) -> Value {
    let mut schema = match action {
        "layer.set" => Operation::LayerProperties.schema(),
        "layer.move" => Operation::MoveLayer.schema(),
        "layer.translate" => json!({"type":"object","properties":{
            "dx":{"type":"integer","minimum":-4096,"maximum":4096},
            "dy":{"type":"integer","minimum":-4096,"maximum":4096}},
            "required":["dx","dy"],"additionalProperties":false}),
        _ => json!({"type":"object","properties":{},"additionalProperties":false}),
    };
    if let Some(properties) = schema["properties"].as_object_mut() {
        properties.remove("index");
        properties.remove("layer_id");
    }
    schema
}
fn parse(args: &Value) -> Result<Batch, ActionError> {
    let batch: Batch = serde_json::from_value(args.clone()).map_err(|e| invalid(e.to_string()))?;
    if !ACTIONS.contains(&batch.action.as_str()) {
        return Err(invalid(
            "The requested action is not a supported layer batch operation",
        ));
    }
    if batch.layer_ids.is_empty()
        || batch.layer_ids.len() > 16
        || batch
            .layer_ids
            .iter()
            .enumerate()
            .any(|(i, id)| *id == 0 || *id > u32::MAX as u64 || batch.layer_ids[..i].contains(id))
    {
        return Err(invalid("Supply between one and sixteen unique layer IDs"));
    }
    super::validate(&target_schema(&batch.action), &batch.args)?;
    Ok(batch)
}
/// Source-independent literal validation; earlier pipeline steps may create IDs.
pub(crate) fn validate(args: &Value) -> Result<(), ActionError> {
    parse(args).map(|_| ())
}
pub(crate) fn schema() -> Value {
    let mut arguments = target_schema("layer.set");
    arguments["properties"]["to"] = json!({"type":"integer","minimum":0,"maximum":15});
    for key in ["dx", "dy"] {
        arguments["properties"][key] = json!({"type":"integer","minimum":-4096,"maximum":4096});
    }
    arguments["default"] = json!({});
    json!({"type":"object","properties":{
        "action":{"type":"string","enum":ACTIONS},
        "layer_ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4294967295u64},"minItems":1,"maxItems":16},
        "args":arguments},"required":["action","layer_ids"],"additionalProperties":false})
}
fn targeted(action: &str) -> bool {
    matches!(
        action,
        "layer.set" | "layer.duplicate" | "layer.delete" | "layer.move"
    )
}
fn child_args(batch: &Batch, id: u64) -> Value {
    let mut args = batch.args.clone();
    if targeted(&batch.action) {
        args["layer_id"] = json!(id);
    }
    args
}
fn alpha_transform_valid(s: &Session, index: usize, batch: &Batch) -> bool {
    let (w, h) = (s.doc.width, s.doc.height);
    let pixels = &s.doc.layers[index].pixels;
    let (dx, dy) = (
        batch.args["dx"].as_i64().unwrap_or(0),
        batch.args["dy"].as_i64().unwrap_or(0),
    );
    pixels.iter().enumerate().all(|(i, before)| {
        let (x, y) = (i % w, i / w);
        let (sx, sy) = match batch.action.as_str() {
            "layer.flip_horizontal" => ((w - 1 - x) as i64, y as i64),
            "layer.flip_vertical" => (x as i64, (h - 1 - y) as i64),
            "layer.rotate_180" => ((w - 1 - x) as i64, (h - 1 - y) as i64),
            _ => (x as i64 - dx, y as i64 - dy),
        };
        let after = if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
            0
        } else {
            pixels[sy as usize * w + sx as usize]
        };
        before >> 24 == after >> 24 && (before >> 24 != 0 || *before == after)
    })
}
fn preflight(s: &Session, batch: &Batch) -> Result<Vec<u64>, ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before a layer batch",
        ));
    }
    if batch
        .layer_ids
        .iter()
        .any(|id| s.doc.layer_index(*id).is_none())
    {
        return Err(ActionError::new(
            "invalid_layer",
            "A batch layer ID does not exist",
        ));
    }
    let ids: Vec<_> = (0..s.doc.layers.len())
        .map(|i| s.doc.layer_id(i))
        .filter(|id| batch.layer_ids.contains(id))
        .collect();
    let definition = s
        .registry
        .actions
        .get(&batch.action)
        .ok_or_else(|| ActionError::new("unknown_action", &batch.action))?;
    for id in &ids {
        super::validate(&definition.schema(), &child_args(batch, *id))?;
    }
    match batch.action.as_str() {
        "layer.duplicate" => s
            .doc
            .preflight_layer_duplicates(&ids)
            .map_err(|e| ActionError::new("layer_limit", e))?,
        "layer.delete" => s
            .doc
            .preflight_layer_deletions(&ids)
            .map_err(|e| ActionError::new("layer_edit", e))?,
        "layer.move"
            if batch.args["to"].as_u64().unwrap() as usize > s.doc.layers.len() - ids.len() =>
        {
            return Err(ActionError::new(
                "invalid_layer",
                "Batch destination exceeds the remaining layer stack",
            ));
        }
        "layer.set" => {
            let mode: Option<BlendMode> = batch
                .args
                .get("blend_mode")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?;
            if mode.is_some_and(|m| m != BlendMode::Normal) && s.doc.renderer.is_none() {
                return Err(ActionError::new(
                    "layer_properties",
                    "Layer compositor DLL is not loaded",
                ));
            }
        }
        _ => {}
    }
    if batch.action == "layer.move" && s.doc.has_groups() {
        let mut order: Vec<_> = (0..s.doc.layers.len())
            .map(|i| s.doc.layer_id(i))
            .filter(|id| !ids.contains(id))
            .collect();
        order.splice(
            batch.args["to"].as_u64().unwrap() as usize
                ..batch.args["to"].as_u64().unwrap() as usize,
            ids.iter().copied(),
        );
        s.doc
            .preflight_layer_order(&order)
            .map_err(|e| ActionError::new("group_boundary", e))?;
    }
    if !targeted(&batch.action) {
        for id in &ids {
            let i = s.doc.layer_index(*id).unwrap();
            let geometry = batch.action != "layer.clear";
            let access = if geometry {
                super::PixelAccess::Geometry
            } else {
                super::PixelAccess::Pixels
            };
            access.require(&s.doc, i)?;
            if geometry && !s.doc.layers[i].visible {
                return Err(ActionError::new(
                    "hidden_layer",
                    "Make batch targets visible before transforming pixels",
                ));
            }
            if s.doc.alpha_locked(i) && (!geometry || !alpha_transform_valid(s, i, batch)) {
                return Err(ActionError::new(
                    "alpha_locked",
                    "The batch would change locked layer transparency",
                ));
            }
        }
    }
    Ok(ids)
}
fn apply(s: &mut Session, batch: &Batch, id: u64, args: Value) -> Result<(), ActionError> {
    if !targeted(&batch.action) {
        s.doc.active = s.doc.layer_index(id).unwrap();
    }
    let result = s.apply(Action::new(&batch.action, args))?;
    if result["result"].get("ok") == Some(&Value::Bool(false)) {
        return Err(serde_json::from_value(result["result"]["error"].clone())
            .unwrap_or_else(|_| invalid("A layer batch child did not complete")));
    }
    Ok(())
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    let batch = parse(&args)?;
    let ids = preflight(s, &batch)?;
    let original: Vec<_> = (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect();
    let active = s.doc.layer_id(s.doc.active);
    let selected = s.doc.selected_layer_ids();
    let revision = s.revision;
    let depth = s.atomic_depth;
    s.doc.begin_history_group().map_err(invalid)?;
    let events = std::mem::take(&mut s.events);
    let sequence = s.sequence;
    s.atomic_depth += 1;
    let outcome = (|| -> Result<(), ActionError> {
        match batch.action.as_str() {
            "layer.move" => {
                let to = batch.args["to"].as_u64().unwrap() as usize;
                let mut expected: Vec<_> = original
                    .iter()
                    .copied()
                    .filter(|id| !ids.contains(id))
                    .collect();
                expected.splice(to..to, ids.iter().copied());
                if expected != original {
                    if s.doc.has_groups() {
                        s.doc.reorder_layers(&expected).map_err(invalid)?;
                    } else {
                        // Gather at the top, then place the block. Each move touches
                        // only a requested layer and moves its existing pixel buffer.
                        for (pass, destination) in [(0, original.len() - 1), (1, to)] {
                            for (offset, id) in ids.iter().enumerate() {
                                let mut args = child_args(&batch, *id);
                                args["to"] =
                                    json!(destination + if pass == 0 { 0 } else { offset });
                                apply(s, &batch, *id, args)?;
                            }
                        }
                    }
                }
                s.doc
                    .set_layer_selection(&selected, Some(active))
                    .map_err(invalid)?;
            }
            "layer.duplicate" => {
                let mut copies = Vec::with_capacity(ids.len());
                let mut active_copy = None;
                for id in &ids {
                    apply(s, &batch, *id, child_args(&batch, *id))?;
                    let copy = s.doc.layer_id(s.doc.active);
                    copies.push(copy);
                    if *id == active {
                        active_copy = Some(copy);
                    }
                }
                s.doc
                    .set_layer_selection(
                        &copies,
                        Some(active_copy.unwrap_or(*copies.last().unwrap())),
                    )
                    .map_err(invalid)?;
            }
            "layer.delete" => {
                for id in ids.iter().rev() {
                    apply(s, &batch, *id, child_args(&batch, *id))?;
                }
                let survivors: Vec<_> = selected
                    .iter()
                    .copied()
                    .filter(|id| !ids.contains(id))
                    .collect();
                let old_active = original.iter().position(|id| *id == active).unwrap();
                let next = if survivors.contains(&active) {
                    active
                } else if !survivors.is_empty() {
                    *survivors
                        .iter()
                        .min_by_key(|id| {
                            let index = original.iter().position(|v| v == *id).unwrap();
                            (index.abs_diff(old_active), index)
                        })
                        .unwrap()
                } else {
                    original
                        .iter()
                        .skip(old_active)
                        .find(|id| !ids.contains(id))
                        .or_else(|| original.iter().rev().find(|id| !ids.contains(id)))
                        .copied()
                        .unwrap()
                };
                let selection = if survivors.is_empty() {
                    vec![next]
                } else {
                    survivors
                };
                s.doc
                    .set_layer_selection(&selection, Some(next))
                    .map_err(invalid)?;
            }
            _ => {
                for id in &ids {
                    apply(s, &batch, *id, child_args(&batch, *id))?;
                }
                s.doc
                    .set_layer_selection(&selected, Some(active))
                    .map_err(invalid)?;
            }
        }
        s.doc.end_history_group().map_err(invalid)
    })();
    s.atomic_depth = depth;
    s.revision = revision;
    s.events = events;
    s.sequence = sequence;
    if let Err(error) = outcome {
        s.doc.cancel_history_group();
        return Err(error);
    }
    Ok(s.state())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_soft_failure_restores_partial_children_and_closes_the_history_group() {
        let mut s = Session::new(super::super::Registry::bundled()).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":4,"height":3})))
            .unwrap();
        s.apply(Action::new("layer.add", json!({}))).unwrap();
        s.apply(Action::new(
            "layer.selection.set",
            json!({"layer_ids":[1,2],"active_layer_id":1}),
        ))
        .unwrap();
        // A shared action can return a successful transport envelope whose
        // result reports a partial failure, as native extensions can do too.
        let action = s.registry.actions.get_mut("layer.clear").unwrap();
        action.operation = None;
        action.native = None;
        action.steps = vec![Action::new(
            "pipeline.run",
            json!({"steps":[
                {"id":"partial","action":"layer.set","args":{"color_label":"red"}},
                {"id":"reject","action":"layer.select","args":{"layer_id":4294967295u64}}
            ]}),
        )];
        action.input_schema = Some(json!({"type":"object","additionalProperties":false}));
        let composition = s.doc.composition();
        let active = s.doc.active;
        let history = s.doc.history_bytes();
        let revision = s.revision;
        let sequence = s.sequence;
        let result = s.apply(Action::new(
            "layer.batch",
            json!({
                "action":"layer.clear","layer_ids":[1,2]
            }),
        ));
        assert!(result.is_err());
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.active, active);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.revision, revision);
        assert_eq!(s.sequence, sequence + 1);
        assert_eq!(s.atomic_depth, 0);
        assert!(!s.doc.history_group_active());
    }
}
