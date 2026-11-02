//! Built-in layers plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::MAX_LAYERS;
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let index = || args["index"].as_f64().unwrap_or(0.) as usize;
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
            s.doc.add_layer();
            Ok(json!({"active_layer":s.doc.active}))
        }
        SelectLayer | LayerProperties => {
            let i = index();
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
                    .set_layer_properties(i, visible, opacity, mode)
                    .map_err(|e| ActionError::new("layer_properties", e))?;
            }
            Ok(s.state())
        }
        ClearLayer => {
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
