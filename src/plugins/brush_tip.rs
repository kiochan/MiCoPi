//! Explicit angle intent shares the same scalar state as brush.set and presets.
use crate::application::{ActionError, ActionResult, Pointer, PointerPhase, Session, ToolKind};
use serde_json::{json, Value};
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{"delta_degrees":{"type":"number","minimum":-180,"maximum":180,"description":"Clockwise document-space change; wraps to [-180,180)."}},"required":["delta_degrees"],"additionalProperties":false})
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    let tool = s.registry.tools.get(&s.selected_tool).ok_or_else(|| {
        ActionError::new(
            "unknown_tool",
            "The selected registered tool is unavailable",
        )
    })?;
    let base = match tool.kind {
        ToolKind::Brush => "brush",
        ToolKind::Eraser => "eraser",
        _ => {
            return Err(ActionError::new(
                "brush_tip",
                "Select a Brush or Eraser to rotate its tip",
            ))
        }
    };
    let mut next = s.parameters.clone();
    next.tip_angle = ((f64::from(next.tip_angle) + args["delta_degrees"].as_f64().unwrap() + 180.)
        .rem_euclid(360.)
        - 180.) as f32;
    if next.tip_angle >= 180. {
        next.tip_angle = -180.;
    }
    let native = s
        .registry
        .native_tools
        .get(&s.selected_tool)
        .or_else(|| s.registry.native_tools.get(base))
        .and_then(|(owner, _)| s.registry.native_plugins.get(owner))
        .ok_or_else(|| {
            ActionError::new(
                "tool_plugin_missing",
                "The selected tool's native provider is unavailable",
            )
        })?;
    native.validate_preset_parameters(&next)?;
    if s.pointer_checkpoint.is_some() && !s.stroke.active() {
        return Err(ActionError::new(
            "stroke_active",
            "Wait for pending input before rotating the brush tip",
        ));
    }
    if let Some(last) = s.stroke.last_sample() {
        s.pointer(Pointer {
            phase: PointerPhase::Up,
            ..last
        })?;
    }
    next.color = s.parameters.color.clone();
    next.background_color = s.parameters.background_color.clone();
    s.parameters = next;
    Ok(json!({"parameters":s.parameter_state(),"revision":s.revision}))
}
