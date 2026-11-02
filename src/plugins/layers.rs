//! Built-in layers plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::MAX_LAYERS;
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let index = || -> Result<usize, ActionError> {
        let by_index = args["index"].as_u64().map(|v| v as usize);
        let by_id = args["layer_id"]
            .as_u64()
            .map(|id| {
                s.doc
                    .layer_index(id)
                    .ok_or_else(|| ActionError::new("invalid_layer", "Layer ID does not exist"))
            })
            .transpose()?;
        if by_index.is_some() && by_id.is_some() && by_index != by_id {
            return Err(ActionError::new(
                "invalid_layer",
                "Layer index and ID refer to different layers",
            ));
        }
        let i = by_id.or(by_index).unwrap_or(s.doc.active);
        if i >= s.doc.layers.len() {
            return Err(ActionError::new("invalid_layer", "Layer does not exist"));
        }
        Ok(i)
    };
    match op {
        AddLayer => {
            if s.doc.layers.len() >= MAX_LAYERS
                || s.doc.width * s.doc.height * (s.doc.layers.len() + 1) > 64 * 1024 * 1024
            {
                return Err(ActionError::new(
                    "layer_limit",
                    "Layer count or document pixel limit reached",
                ));
            }
            s.finish_stroke();
            let count = s.doc.layers.len();
            s.doc.add_layer();
            if s.doc.layers.len() == count {
                return Err(ActionError::new(
                    "layer_limit",
                    "Layer identity limit reached",
                ));
            }
            Ok(json!({"active_layer":s.doc.active,"layer_id":s.doc.layer_id(s.doc.active)}))
        }
        RenameLayer | DuplicateLayer | MoveLayer | DeleteLayer => {
            let i = index()?;
            s.finish_stroke();
            let result = match op {
                RenameLayer => s
                    .doc
                    .rename_layer(i, args["name"].as_str().unwrap().to_owned()),
                DuplicateLayer => s.doc.duplicate_layer(i),
                MoveLayer => s.doc.move_layer(i, args["to"].as_u64().unwrap() as usize),
                DeleteLayer => s.doc.delete_layer(i),
                _ => unreachable!(),
            };
            result.map_err(|e| ActionError::new("layer_edit", e))?;
            Ok(s.state())
        }
        SelectLayer | LayerProperties => {
            let i = index()?;
            if i >= s.doc.layers.len() {
                return Err(ActionError::new("invalid_layer", "Layer does not exist"));
            }
            s.finish_stroke();
            if op == SelectLayer {
                s.doc.active = i;
            } else {
                let l = &s.doc.layers[i];
                let visible = args["visible"].as_bool().unwrap_or(l.visible);
                let opacity = args["opacity"]
                    .as_f64()
                    .map(|v| (v * 255.).round() as u8)
                    .unwrap_or(l.opacity);
                let mode = if args.get("blend_mode").is_some() {
                    serde_json::from_value(args["blend_mode"].clone())?
                } else {
                    l.mode
                };
                s.doc
                    .set_layer_properties_with_lock(
                        i,
                        visible,
                        opacity,
                        mode,
                        args["alpha_locked"]
                            .as_bool()
                            .unwrap_or(s.doc.alpha_locked(i)),
                    )
                    .map_err(|e| ActionError::new("layer_properties", e))?;
            }
            Ok(s.state())
        }
        ClearLayer => {
            if s.doc.alpha_locked(s.doc.active) {
                return Err(ActionError::new(
                    "alpha_locked",
                    "Unlock layer transparency before clearing it",
                ));
            }
            s.finish_stroke();
            s.doc.clear_layer();
            Ok(s.state())
        }

        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}
