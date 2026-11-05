//! Built-in workbench plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let path = || {
        args["path"]
            .as_str()
            .map(PathBuf::from)
            .ok_or_else(|| ActionError::new("invalid_arguments", "Missing path"))
    };
    match op {
        CaptureWorkspace => {
            if s.runtime != "desktop" {
                return Err(ActionError::new(
                    "ui_unavailable",
                    "Workspace capture requires the desktop",
                ));
            }
            if s.ui_effects.len() >= 128 {
                return Err(ActionError::new(
                    "presentation_queue_full",
                    "Wait for queued presentation commands before retrying",
                ));
            }
            let path = path()?;
            s.ui_effects
                .push_back(json!({"id":"screenshot","value":path}));
            Ok(json!({"queued":true,"path":path}))
        }
        SettingsRead => {
            let config = crate::configuration::Settings::load(path()?)
                .map_err(|e| ActionError::new("settings", e))?;
            Ok(serde_json::from_str(
                &config
                    .encode()
                    .map_err(|e| ActionError::new("settings", e))?,
            )?)
        }
        SettingsWrite => {
            if s.runtime == "desktop" && s.ui_effects.len() >= 128 {
                return Err(ActionError::new(
                    "presentation_queue_full",
                    "Wait for queued presentation commands before retrying",
                ));
            }
            let path = path()?;
            let mut value: Value = if let Some(document) = args.get("document") {
                document.clone()
            } else {
                let existing = crate::configuration::Settings::load(path.clone())
                    .map_err(|e| ActionError::new("settings", e))?;
                serde_json::from_str(
                    &existing
                        .encode()
                        .map_err(|e| ActionError::new("settings", e))?,
                )?
            };
            for key in [
                "ui",
                "keybindings",
                "painting",
                "selection",
                "plugins",
                "movement",
                "new_canvas",
            ] {
                if let Some(patch) = args[key].as_object() {
                    for (name, setting) in patch {
                        value[key][name] = setting.clone();
                    }
                }
            }
            let updated = crate::configuration::Settings::decode(&value.to_string(), path.clone())
                .map_err(|e| ActionError::new("settings", e))?;
            updated
                .save()
                .map_err(|e| ActionError::new("settings", e))?;
            let selection_changed = s.selection_defaults != updated.selection;
            if s.settings_path
                .as_ref()
                .is_none_or(|active| active == &path)
            {
                s.apply_selection_defaults(updated.selection);
                s.apply_movement_defaults(updated.movement);
            }
            s.ui_effects.push_back(
                json!({"id":"settings.reload","value":path,"selection_changed":selection_changed}),
            );
            while s.ui_effects.len() > 128 {
                s.ui_effects.pop_front();
            }
            Ok(json!({"path":path}))
        }
        Ui => {
            let id = args["id"].as_str().unwrap();
            if matches!(id, "transform.open" | "transform.parameters") && s.runtime != "desktop" {
                return Err(ActionError::new("ui_unavailable","Free Transform controls require the desktop; use canvas.transform for headless editing"));
            }
            if id == "transform.parameters" {
                let p: super::affine::Parameters = serde_json::from_value(args["value"].clone())
                    .map_err(|e| ActionError::new("invalid_arguments", e.to_string()))?;
                p.matrix([0, 0, 1, 1])
                    .map_err(|e| ActionError::new("invalid_arguments", e))?;
            }
            let selection_tool = id == "selection.combine"
                || id == "selection.options"
                || (id == "painting.mode"
                    && args["value"]
                        .as_str()
                        .is_some_and(|v| v.starts_with("selection_")));
            let movement_tool =
                id == "movement.options" || (id == "painting.mode" && args["value"] == "move");
            if movement_tool && s.runtime != "desktop" {
                return Err(ActionError::new("ui_unavailable","Move controls require the desktop; use canvas.move or canvas.nudge for headless editing"));
            }
            if id == "movement.options" {
                let p: crate::application::MovementPreferences =
                    serde_json::from_value(args["value"].clone())
                        .map_err(|e| ActionError::new("invalid_arguments", e.to_string()))?;
                p.validate()
                    .map_err(|e| ActionError::new("invalid_arguments", e))?;
            }
            if selection_tool && s.runtime != "desktop" {
                return Err(ActionError::new("ui_unavailable","Selection gesture tools require the desktop; use selection.shape for headless editing"));
            }
            if id == "selection.options" {
                serde_json::from_value::<crate::application::SelectionSampling>(
                    args["value"].clone(),
                )
                .map_err(|e| ActionError::new("invalid_arguments", e.to_string()))?;
            }
            if s.runtime == "desktop" && s.ui_effects.len() >= 128 {
                return Err(ActionError::new(
                    "presentation_queue_full",
                    "Wait for queued presentation commands before retrying",
                ));
            }
            let id = args["id"].as_str().unwrap();
            s.ui[id] = args.get("value").cloned().unwrap_or(Value::Null);
            s.ui_effects.push_back(args.clone());
            while s.ui_effects.len() > 128 {
                s.ui_effects.pop_front();
            }
            Ok(json!({"id":id,"value":s.ui[id]}))
        }
        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Action;
    #[test]
    fn new_canvas_settings_patch_roundtrip_and_invalid_patch_preserve_profile_and_artwork() {
        let registry = crate::plugins::Registry::bundled();
        assert!(
            registry.actions.contains_key("settings.update"),
            "{:?}",
            registry.diagnostics
        );
        let mut s = Session::new(registry).unwrap();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/new-canvas-profile-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"extension":{"keep":42},"ui":{"language":"en"}}"#).unwrap();
        let revision = s.revision;
        let composition = s.doc.composition();
        let info = s.doc.info();
        let dimensions = (s.doc.width, s.doc.height, s.doc.active, s.doc.dirty);
        let history = s.doc.history_bytes();
        let pixels = s.doc.layers[0].pixels.clone();
        s.apply(Action::new(
            "settings.update",
            json!({"path":path,"new_canvas":{"default_preset":"a4"}}),
        ))
        .unwrap();
        let read = s
            .apply(Action::new("settings.read", json!({"path":path})))
            .unwrap();
        assert_eq!(read["result"]["new_canvas"]["default_preset"], "a4");
        assert_eq!(
            read["result"]["new_canvas"]["presets"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(read["result"]["extension"]["keep"], 42);
        assert_eq!(s.revision, revision + 1);
        let original = std::fs::read(&path).unwrap();
        for invalid in [
            json!({"default_preset":"missing"}),
            json!({"presets":[]}),
            json!({"typo":true}),
        ] {
            assert!(s
                .apply(Action::new(
                    "settings.update",
                    json!({"path":path,"new_canvas":invalid})
                ))
                .is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(s.revision, revision + 1);
        }
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.info(), info);
        assert_eq!(
            (s.doc.width, s.doc.height, s.doc.active, s.doc.dirty),
            dimensions
        );
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    #[test]
    fn workspace_capture_is_presentation_only_and_keeps_revision_and_public_state() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let args = json!({"path":"target/test-output/capture.png"});
        assert_eq!(
            s.apply(Action::new("ui.capture", args.clone()))
                .unwrap_err()
                .code,
            "ui_unavailable"
        );
        s.runtime = "desktop";
        let before = s.state();
        let events = s.replay_journal(false).unwrap().entries.len();
        assert_eq!(
            s.apply(Action::new("ui.capture", args.clone())).unwrap()["result"]["queued"],
            true
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.replay_journal(false).unwrap().entries.len(), events + 1);
        assert_eq!(s.ui_effects.back().unwrap()["id"], "screenshot");
        assert!(s
            .apply(Action::new(
                "pipeline.run",
                json!({"atomic":true,"steps":[{"id":"capture","action":"ui.capture","args":args}]})
            ))
            .is_err());
        s.ui_effects.resize(128, json!({}));
        assert_eq!(
            s.apply(Action::new("ui.capture", args)).unwrap_err().code,
            "presentation_queue_full"
        );
        assert_eq!(s.state(), before);
    }
}
