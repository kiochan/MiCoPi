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
            for key in ["ui", "keybindings", "painting", "selection", "plugins"] {
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
            let selection_tool = id == "selection.combine"
                || id == "selection.options"
                || (id == "painting.mode"
                    && args["value"]
                        .as_str()
                        .is_some_and(|v| v.starts_with("selection_")));
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
