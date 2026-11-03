//! Window contributions and presentation commands. No GUI dependency or document ownership.
use super::{ActionDefinition, PanelDefinition};
use crate::application::{Action, ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};
pub const OPERATIONS: &[&str] = &[
    "show",
    "hide",
    "focus",
    "float",
    "dock_left",
    "dock_right",
    "dock_top",
    "dock_bottom",
    "tab",
];
pub fn commands(panel: &PanelDefinition) -> Vec<ActionDefinition> {
    OPERATIONS.iter().map(|operation| {
        let mut args = json!({"panel":panel.id,"operation":operation});
        let schema = if *operation == "tab" {
            args["target"] = json!({"$arg":"target"});
            json!({"type":"object","required":["target"],"properties":{"target":{"type":"string"}},"additionalProperties":false})
        } else { json!({"type":"object","additionalProperties":false}) };
        ActionDefinition { id:format!("window.{}.{}",panel.id,operation), title:format!("{} · {}",panel.title,operation), operation:None,native:None,
            steps:vec![Action::new("window.command",args)], input_schema:Some(schema),read_only:false,document_atomic:false,output_schema:None }
    }).collect()
}
pub fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::WindowState {
        return Ok(
            json!({"runtime":s.runtime,"panels":s.registry.panels.values().collect::<Vec<_>>(),"actual":s.ui.get("windows.actual"),"last_request":s.ui.get("windows.request"),"last_result":s.ui.get("windows.result")}),
        );
    }
    let panel = args["panel"].as_str().unwrap();
    if !s.registry.panels.contains_key(panel) {
        return Err(ActionError::new("unknown_window", panel));
    }
    if let Some(target) = args["target"].as_str() {
        if target == panel || !s.registry.panels.contains_key(target) {
            return Err(ActionError::new("invalid_window_target", target));
        }
    } else if args["operation"] == "tab" {
        return Err(ActionError::new(
            "invalid_window_target",
            "Tab grouping requires a target window",
        ));
    }
    let queued = s.runtime == "desktop";
    s.ui["windows.request"] = args.clone();
    if queued {
        s.ui_effects
            .push_back(json!({"id":"window.command","value":args}));
        while s.ui_effects.len() > 128 {
            s.ui_effects.pop_front();
        }
    }
    s.ui["windows.result"] =
        json!({"status":if queued {"queued"} else {"presentation_unavailable"},"request":args});
    Ok(json!({"queued":queued,"applied":false,"runtime":s.runtime,"request":args}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_commands_are_plugin_owned_schema_checked_and_headless_is_explicit() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        assert_eq!(s.registry.owners["window.toolbox.float"], "toolbox");
        assert_eq!(s.registry.owners["window.canvas.show"], "canvas");
        let before = s.doc.composition();
        let result = s
            .apply(Action::new("window.toolbox.float", json!({})))
            .unwrap();
        assert_eq!(result["result"]["applied"], false);
        assert_eq!(result["result"]["queued"], false);
        assert!(s.ui_effects.is_empty());
        assert_eq!(s.doc.composition(), before);
        assert!(s
            .apply(Action::new(
                "window.command",
                json!({"panel":"missing","operation":"show"})
            ))
            .is_err());
        assert!(s
            .apply(Action::new("window.canvas.tab", json!({"target":"canvas"})))
            .is_err());
        s.runtime = "desktop";
        let result = s
            .apply(Action::new("window.canvas.tab", json!({"target":"layers"})))
            .unwrap();
        assert_eq!(result["result"]["queued"], true);
        assert_eq!(s.ui_effects.back().unwrap()["value"]["target"], "layers");
    }
    #[test]
    fn atomic_document_groups_reject_window_side_effects_before_queuing() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        s.runtime = "desktop";
        assert!(s
            .apply(Action::new(
                "pipeline.run",
                json!({"atomic":true,"steps":[{"id":"window","action":"window.canvas.float"}]})
            ))
            .is_err());
        assert!(s.ui_effects.is_empty());
    }
}
