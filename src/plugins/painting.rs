//! Built-in painting plugin action handlers; callable without a GUI.
use crate::application::PointerPhase;
use crate::application::{ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    match op {
        SelectTool => {
            let id = args["tool"].as_str().unwrap();
            let tool = s
                .registry
                .tools
                .get(id)
                .ok_or_else(|| ActionError::new("unknown_tool", id))?
                .clone();
            let parameters = if tool.parameters.is_null() {
                s.parameters.clone()
            } else {
                s.parameters.patched(&tool.parameters)?
            };
            s.finish_stroke();
            s.selected_tool = id.into();
            s.tool = tool.kind;
            s.parameters = parameters;
            Ok(json!({"selected_tool":id}))
        }
        Parameters => {
            let next = s.parameters.patched(&args)?;
            s.finish_stroke();
            s.parameters = next;
            Ok(serde_json::to_value(&s.parameters)?)
        }
        Pointer => {
            let sample: crate::application::Pointer = serde_json::from_value(args)?;
            s.pointer_action(sample)?;
            Ok(json!({"drawing":s.stroke.drawing()}))
        }
        Stroke => {
            let points = args["points"].as_array().unwrap();
            let mut samples = Vec::with_capacity(points.len());
            for (i, p) in points.iter().enumerate() {
                let mut p = p.clone();
                p["phase"] = json!(if i == 0 { "down" } else { "move" });
                if p.get("time").is_none() {
                    p["time"] = json!((i as u64 * 8) as u32);
                }
                let sample: crate::application::Pointer = serde_json::from_value(p)?;
                sample.validate()?;
                samples.push(sample);
            }
            if s.stroke.active() {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current stroke first",
                ));
            }
            let selected_kind = if let Some(id) = args["tool"].as_str() {
                s.registry
                    .tools
                    .get(id)
                    .ok_or_else(|| ActionError::new("unknown_tool", id))?
                    .kind
            } else {
                s.tool
            };
            let kind = samples[0].tool_override.unwrap_or(selected_kind);
            if matches!(
                kind,
                crate::application::ToolKind::Brush | crate::application::ToolKind::Eraser
            ) && s.doc.mask_state().target
                == crate::engine::raster_masks::PaintTarget::LayerPixels
            {
                super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
            }
            if let Some(tool) = args["tool"].as_str() {
                super::execute(s, SelectTool, json!({"tool":tool}))?;
            }
            for sample in &samples {
                s.pointer(*sample)?;
            }
            let last = *samples.last().unwrap();
            s.pointer(crate::application::Pointer {
                phase: PointerPhase::Up,
                ..last
            })?;
            Ok(json!({"points":samples.len()}))
        }
        Pick => {
            let x = args["x"].as_f64().unwrap() as f32;
            let y = args["y"].as_f64().unwrap() as f32;
            s.pick(x, y)?;
            Ok(json!({"color":s.parameters.color}))
        }
        View => {
            for (key, dst) in [
                ("zoom", &mut s.view.zoom),
                ("pan_x", &mut s.view.pan_x),
                ("pan_y", &mut s.view.pan_y),
            ] {
                if let Some(n) = args[key].as_f64() {
                    *dst = n as f32;
                }
            }
            if let Some(fit) = args["fit"].as_bool() {
                s.view.fit = fit;
            }
            Ok(serde_json::to_value(s.view)?)
        }

        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}
