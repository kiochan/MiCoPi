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
        let title=match *operation {"show"=>"Show Window","hide"=>"Hide Window","focus"=>"Focus Window","float"=>"Float Window","dock_left"=>"Dock Left","dock_right"=>"Dock Right","dock_top"=>"Dock Top","dock_bottom"=>"Dock Bottom",_=>"Group as Tabs"};
        ActionDefinition { id:format!("window.{}.{}",panel.id,operation), title:format!("{title}: {}",panel.title), operation:None,native:None,
            steps:vec![Action::new("window.command",args)], input_schema:Some(schema),read_only:false,document_atomic:false,output_schema:None }
    }).collect()
}
pub fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::WindowState {
        return Ok(
            json!({"runtime":s.runtime,"panels":s.registry.panels.values().collect::<Vec<_>>(),"actual":s.ui.get("windows.actual"),"last_request":s.ui.get("windows.request"),"last_result":s.ui.get("windows.result")}),
        );
    }
    if s.runtime == "desktop" && s.ui_effects.len() >= 128 {
        return Err(ActionError::new(
            "presentation_queue_full",
            "Wait for queued presentation commands before retrying",
        ));
    }
    if op == Operation::WindowLayout {
        if serde_json::to_vec(&args["layout"])?.len() > 1024 * 1024 {
            return Err(ActionError::new(
                "layout_limit",
                "Window layout exceeds 1 MB",
            ));
        }
        let queued = s.runtime == "desktop";
        if queued {
            s.ui_effects
                .push_back(json!({"id":"window.layout","value":args["layout"]}));
        }
        s.ui["windows.request"] = json!({"operation":"layout"});
        s.ui["windows.result"] =
            json!({"status":if queued {"queued"} else {"presentation_unavailable"}});
        return Ok(json!({"queued":queued,"applied":false,"runtime":s.runtime}));
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
    #[test]
    fn a_full_presentation_queue_rejects_new_commands_without_dropping_old_requests() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        s.runtime = "desktop";
        for i in 0..128 {
            s.ui_effects.push_back(json!({"id":"test","sequence":i}));
        }
        let ui = s.ui.clone();
        assert_eq!(
            s.apply(Action::new("window.canvas.float", json!({})))
                .unwrap_err()
                .code,
            "presentation_queue_full"
        );
        assert_eq!(s.ui, ui);
        assert_eq!(s.ui_effects.len(), 128);
        assert_eq!(s.ui_effects.front().unwrap()["sequence"], 0);
        assert!(s.apply(Action::new("window.list", json!({}))).is_ok());
    }
    #[test]
    fn legacy_workbench_is_adapted_without_rewriting_custom_manifests() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output/legacy-window-plugins")
            .join(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
                    .to_string(),
            );
        crate::plugins::Registry::load(&root).unwrap();
        let mut legacy: Value =
            serde_json::from_slice(&std::fs::read(root.join("workbench/manifest.json")).unwrap())
                .unwrap();
        let mut panels = vec![];
        for id in ["canvas", "toolbox", "color", "brush", "navigator", "layers"] {
            let path = root.join(id).join("manifest.json");
            let module: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            if !["canvas", "toolbox"].contains(&id) {
                panels.push(module["panels"][0].clone());
            }
            std::fs::remove_file(path).unwrap();
        }
        panels.iter_mut().find(|p| p["id"] == "color").unwrap()["title"] = json!("Custom Wheel");
        legacy["panels"] = json!(panels);
        legacy["states"] = json!([]);
        legacy["actions"].as_array_mut().unwrap().retain(|a| {
            !a["id"].as_str().unwrap().starts_with("window.")
                && !a["id"].as_str().unwrap().starts_with("application.")
                && a["id"] != "workspace.layout.apply"
        });
        let path = root.join("workbench/manifest.json");
        let original = serde_json::to_vec(&legacy).unwrap();
        std::fs::write(&path, &original).unwrap();
        let loaded = crate::plugins::Registry::load(&root).unwrap();
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        assert_eq!(loaded.panels["color"].title, "Custom Wheel");
        assert!(loaded.actions.contains_key("window.canvas.float"));
        assert!(loaded.actions.contains_key("application.minimize"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!root.join("toolbox/manifest.json").exists());
        let disabled =
            crate::plugins::Registry::load_with_disabled(&root, &["color".into()]).unwrap();
        assert!(!disabled.panels.contains_key("color"));
        assert!(!disabled.actions.contains_key("window.color.show"));
    }
}
