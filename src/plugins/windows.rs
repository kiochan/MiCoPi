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
    "minimize",
    "maximize",
    "restore",
];
pub fn commands(panel: &PanelDefinition) -> Vec<ActionDefinition> {
    let mut commands:Vec<_>=OPERATIONS.iter().map(|operation| {
        let mut args = json!({"panel":panel.id,"operation":operation});
        let schema = if *operation == "tab" {
            args["target"] = json!({"$arg":"target"});
            json!({"type":"object","required":["target"],"properties":{"target":{"type":"string"}},"additionalProperties":false})
        } else { json!({"type":"object","additionalProperties":false}) };
        let title=match *operation {"show"=>"Show Window","hide"=>"Hide Window","focus"=>"Focus Window","float"=>"Float Window","dock_left"=>"Dock Left","dock_right"=>"Dock Right","dock_top"=>"Dock Top","dock_bottom"=>"Dock Bottom","minimize"=>"Minimize Window","maximize"=>"Maximize Window","restore"=>"Restore Window",_=>"Group as Tabs"};
        ActionDefinition { id:format!("window.{}.{}",panel.id,operation), title:format!("{title}: {}",panel.title), operation:None,native:None,pixel_access:None,
            steps:vec![Action::new("window.command",args)], input_schema:Some(schema),read_only:false,document_atomic:false,requires_idle:false,output_schema:None }
    }).collect();
    for (operation, title) in [
        ("float", "Float Panel Group"),
        ("hide", "Hide Panel Group"),
        ("dock_left", "Dock Group Left"),
        ("dock_right", "Dock Group Right"),
        ("dock_top", "Dock Group Top"),
        ("dock_bottom", "Dock Group Bottom"),
        ("tab", "Merge Panel Group as Tabs"),
    ] {
        let mut command = commands
            .iter()
            .find(|c| c.id == format!("window.{}.{operation}", panel.id))
            .unwrap()
            .clone();
        command.id.push_str("_group");
        command.title = format!("{title}: {}", panel.title);
        command.steps[0].args["scope"] = json!("group");
        commands.push(command);
    }
    commands
}
pub fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::WorkspaceList {
        return Ok(json!({"runtime":s.runtime,"available":s.runtime=="desktop",
            "actual":s.ui.get("workspaces.actual"),"last_result":s.ui.get("workspaces.result")}));
    }
    if matches!(
        op,
        Operation::WorkspaceSave | Operation::WorkspaceSwitch | Operation::WorkspaceDelete
    ) {
        let name = args["name"].as_str().unwrap().to_owned();
        if name.trim() != name
            || name.is_empty()
            || name.chars().count() > 64
            || name.chars().any(char::is_control)
        {
            return Err(ActionError::new(
                "invalid_workspace_name",
                "Use 1-64 characters without surrounding whitespace or control characters",
            ));
        }
        if op == Operation::WorkspaceSwitch && (s.stroke.active() || s.pointer_checkpoint.is_some())
        {
            return Err(ActionError::new(
                "stroke_active",
                "Finish the current contact before switching workspace layouts",
            ));
        }
        if s.runtime != "desktop" {
            return Ok(
                json!({"queued":false,"applied":false,"status":"presentation_unavailable","runtime":s.runtime}),
            );
        }
        if s.ui_effects.len() >= 128 {
            return Err(ActionError::new(
                "presentation_queue_full",
                "Wait for queued presentation commands before retrying",
            ));
        }
        let operation = match op {
            Operation::WorkspaceSave => "save",
            Operation::WorkspaceSwitch => "switch",
            _ => "delete",
        };
        s.ui_effects
            .push_back(json!({"id":"workspace.named","operation":operation,"value":args}));
        s.ui["workspaces.result"] = json!({"status":"queued","operation":operation,"name":name});
        return Ok(json!({"queued":true,"applied":false,"runtime":s.runtime}));
    }
    if op == Operation::WindowState {
        let captures = s.capture_diagnostics();
        let last_capture = if captures["last"].is_null() {
            s.ui.get("windows.capture").cloned().unwrap_or(Value::Null)
        } else {
            captures["last"].clone()
        };
        return Ok(
            json!({"runtime":s.runtime,"panels":s.registry.panels.values().collect::<Vec<_>>(),"actual":s.ui.get("windows.actual"),"last_drop":s.ui.get("windows.drop"),"last_capture":last_capture,"capture_requests":captures,"last_request":s.ui.get("windows.request"),"last_result":s.ui.get("windows.result"),"diagnostics_available":s.runtime=="desktop" && s.desktop_diagnostics.is_some(),"diagnostics":s.desktop_diagnostics}),
        );
    }
    if s.runtime == "desktop" && s.ui_effects.len() >= 128 {
        return Err(ActionError::new(
            "presentation_queue_full",
            "Wait for queued presentation commands before retrying",
        ));
    }
    if op == Operation::WindowDrop {
        let panel = args["panel"].as_str().unwrap();
        if !s.registry.panels.contains_key(panel) {
            return Err(ActionError::new("unknown_window", panel));
        }
        if let Some(target) = args["target"].as_str() {
            if !s.registry.panels.contains_key(target) {
                return Err(ActionError::new("unknown_window", target));
            }
        }
        let queued = s.runtime == "desktop";
        s.ui["windows.drop"] =
            json!({"status":if queued {"queued"} else {"presentation_unavailable"}});
        if queued {
            s.ui_effects
                .push_back(json!({"id":"window.drop","value":args}));
        }
        return Ok(json!({"queued":queued,"applied":false,"runtime":s.runtime}));
    }
    if op == Operation::WindowCapture {
        let panel = args["panel"].as_str().unwrap();
        if !s.registry.panels.contains_key(panel) {
            return Err(ActionError::new("unknown_window", panel));
        }
        let queued = s.runtime == "desktop";
        if queued {
            let request_id = s.queue_capture(Some(panel), args["path"].as_str().unwrap())?;
            let mut effect = args;
            effect["request_id"] = json!(request_id);
            s.ui_effects
                .push_back(json!({"id":"window.capture","value":effect}));
            return Ok(
                json!({"queued":true,"applied":false,"runtime":s.runtime,"request_id":request_id}),
            );
        }
        return Ok(json!({"queued":queued,"applied":false,"runtime":s.runtime}));
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
    fn diagnostics_are_discoverable_read_only_and_explicit_when_unavailable() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        assert!(s.registry.states.contains_key("workbench.debug"));
        let before = s.state();
        let history = s.doc.history_bytes();
        let revision = s.revision;
        assert!(s.journal.is_none());
        let headless = s
            .apply(Action::new("workbench.diagnostics", json!({})))
            .unwrap();
        assert_eq!(headless["result"]["diagnostics_available"], false);
        assert!(headless["result"]["diagnostics"].is_null());
        assert!(
            s.journal.is_none(),
            "a read-only query must not start recording"
        );
        s.runtime = "desktop".into();
        s.desktop_diagnostics =
            Some(json!({"root_material":{"reason":"system_transparency_disabled"}}));
        s.replay_ensure().unwrap();
        let events = s.replay_journal(false).unwrap().entries.len();
        let actual = s
            .apply(Action::new("workbench.diagnostics", json!({})))
            .unwrap();
        assert_eq!(actual["result"]["diagnostics_available"], true);
        assert_eq!(
            actual["result"]["diagnostics"]["root_material"]["reason"],
            "system_transparency_disabled"
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.replay_journal(false).unwrap().entries.len(), events);
        assert!(s.ui_effects.is_empty());
    }
    #[test]
    fn workspace_layout_actions_are_discoverable_headless_explicit_and_revision_neutral() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        let revision = s.revision;
        let history = s.doc.history_bytes();
        for id in [
            "workspace.layout.save",
            "workspace.layout.switch",
            "workspace.layout.delete",
        ] {
            assert_eq!(s.registry.owners[id], "workbench");
            let result = s.apply(Action::new(id, json!({"name":"绘画"}))).unwrap();
            assert_eq!(result["result"]["status"], "presentation_unavailable");
            assert_eq!(result["result"]["queued"], false);
            assert_eq!(s.revision, revision);
            assert!(s.ui_effects.is_empty());
        }
        assert!(s.registry.actions["workspace.layout.list"].is_read_only());
        assert_eq!(
            s.apply(Action::new("workspace.layout.list", json!({})))
                .unwrap()["result"]["available"],
            false
        );
        s.runtime = "desktop";
        s.apply(Action::new("workspace.layout.save", json!({"name":"One"})))
            .unwrap();
        assert_eq!(s.ui_effects.len(), 1);
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.history_bytes(), history);
        for invalid in [
            json!({"name":""}),
            json!({"name":" space "}),
            json!({"name":"a\nb"}),
            json!({"name":"a".repeat(65)}),
            json!({"name":"One","extra":true}),
        ] {
            assert!(s
                .apply(Action::new("workspace.layout.save", invalid))
                .is_err());
        }
        assert_eq!(s.ui_effects.len(), 1);
        s.ui_effects.clear();
        assert!(s.apply(Action::new("pipeline.run", json!({"atomic":true,"steps":[{"id":"save","action":"workspace.layout.save","args":{"name":"One"}}]}))).is_err());
        assert!(s.ui_effects.is_empty());
    }

    #[test]
    fn workspace_layout_switch_rejects_live_backend_contacts_and_full_queue() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        s.runtime = "desktop";
        s.apply(Action::new(
            "canvas.pointer",
            json!({"phase":"down","x":2,"y":2,"pressure":0.5,"pen":true,"time":0}),
        ))
        .unwrap();
        assert!(s.stroke.active() || s.pointer_checkpoint.is_some());
        let revision = s.revision;
        s.apply(Action::new(
            "workspace.layout.save",
            json!({"name":"During contact"}),
        ))
        .unwrap();
        assert!(s.stroke.active() || s.pointer_checkpoint.is_some());
        assert_eq!(s.revision, revision);
        assert_eq!(s.ui_effects.len(), 1);
        s.ui_effects.clear();
        assert_eq!(
            s.apply(Action::new(
                "workspace.layout.switch",
                json!({"name":"One"})
            ))
            .unwrap_err()
            .code,
            "stroke_active"
        );
        assert!(s.stroke.active() || s.pointer_checkpoint.is_some());
        assert_eq!(s.revision, revision);
        assert!(s.ui_effects.is_empty());
        s.apply(Action::new(
            "canvas.pointer",
            json!({"phase":"up","x":2,"y":2,"pressure":0.5,"pen":true,"time":1}),
        ))
        .unwrap();
        for _ in 0..128 {
            s.ui_effects.push_back(json!({"id":"test"}));
        }
        assert_eq!(
            s.apply(Action::new(
                "workspace.layout.switch",
                json!({"name":"One"})
            ))
            .unwrap_err()
            .code,
            "presentation_queue_full"
        );
        assert_eq!(s.ui_effects.len(), 128);
        assert!(s
            .apply(Action::new("workspace.layout.list", json!({})))
            .is_ok());
    }
    #[test]
    fn desktop_point_drop_has_explicit_headless_results_and_validates_target_ids() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        let args = json!({"panel":"layers","target":"navigator","x":-1400,"y":240});
        assert_eq!(
            s.apply(Action::new("window.drop", args.clone())).unwrap()["result"]["queued"],
            false
        );
        assert_eq!(s.ui["windows.drop"]["status"], "presentation_unavailable");
        s.runtime = "desktop".into();
        assert_eq!(
            s.apply(Action::new("window.drop", args)).unwrap()["result"]["queued"],
            true
        );
        assert_eq!(s.ui["windows.drop"]["status"], "queued");
        assert_eq!(s.ui_effects.len(), 1);
        assert!(s
            .apply(Action::new(
                "window.drop",
                json!({"panel":"layers","target":"missing","x":0,"y":0})
            ))
            .is_err());
        assert_eq!(s.ui_effects.len(), 1);
    }
    #[test]
    fn viewport_capture_is_explicitly_unavailable_headlessly_and_queued_on_desktop() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        let args = json!({"panel":"canvas","path":"target/test-output/window.png"});
        assert_eq!(
            s.apply(Action::new("window.capture", args.clone()))
                .unwrap()["result"]["queued"],
            false
        );
        assert!(s.ui_effects.is_empty());
        s.runtime = "desktop".into();
        assert_eq!(
            s.apply(Action::new("window.capture", args)).unwrap()["result"]["queued"],
            true
        );
        assert_eq!(s.ui_effects.back().unwrap()["id"], "window.capture");
    }
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
