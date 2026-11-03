//! Every registered action is searchable/bindable; argument editing never executes it.
use super::*;
use serde_json::{json, Value};
pub(super) struct CommandForm {
    action: String,
    schema: Value,
    args: Value,
    complex: std::collections::BTreeMap<String, String>,
    revision: u64,
    message: String,
}
impl Editor {
    pub(super) fn register_commands(&mut self) {
        let actions = self
            .session
            .registry
            .actions
            .values()
            .map(|a| (a.id.clone(), a.title.clone()))
            .collect::<Vec<_>>();
        if let Err(error) = self.settings.keys.register_actions(actions) {
            self.status = error;
        }
    }
    pub(super) fn begin_plugin_command(&mut self, action: &str) {
        let Some(def) = self.session.registry.actions.get(action) else {
            return;
        };
        let schema = def.schema();
        if schema["properties"]
            .as_object()
            .is_none_or(|p| p.is_empty())
        {
            self.core(action, json!({}));
            return;
        }
        let mut args = json!({});
        if let Some(properties) = schema["properties"].as_object() {
            for (key, s) in properties {
                if schema["required"]
                    .as_array()
                    .is_some_and(|r| r.iter().any(|v| v == key))
                    || s.get("default").is_some()
                {
                    args[key] = schema::default_value(s);
                }
            }
        }
        self.command_form = Some(CommandForm {
            action: action.into(),
            schema,
            args,
            complex: Default::default(),
            revision: self.session.revision,
            message: String::new(),
        });
    }
    pub(super) fn plugin_command_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.command_form.take() else {
            return;
        };
        let mut open = true;
        let mut run = false;
        let mut invalid = false;
        let definition = self.session.registry.actions[&form.action].clone();
        Window::new(tr(&definition.title))
            .id(Id::new("plugin-command-arguments"))
            .open(&mut open)
            .default_width(480.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.weak(&form.action);
                ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.55)
                    .show(ui, |ui| {
                        if let Some(properties) = form.schema["properties"].as_object() {
                            for (key, s) in properties {
                                let required = form.schema["required"]
                                    .as_array()
                                    .is_some_and(|r| r.iter().any(|v| v == key));
                                let mut enabled = form.args.get(key).is_some();
                                ui.horizontal(|ui| {
                                    if required {
                                        ui.label(tr(s["title"].as_str().unwrap_or(key)));
                                    } else if ui
                                        .checkbox(
                                            &mut enabled,
                                            tr(s["title"].as_str().unwrap_or(key)),
                                        )
                                        .changed()
                                    {
                                        if enabled {
                                            form.args[key] = schema::default_value(s);
                                        } else {
                                            form.args.as_object_mut().unwrap().remove(key);
                                            form.complex.remove(key);
                                        }
                                    }
                                    if enabled
                                        && !matches!(
                                            s["type"].as_str(),
                                            Some("object" | "array") | None
                                        )
                                    {
                                        schema::edit(
                                            ui,
                                            (&form.action, key),
                                            s,
                                            &mut form.args[key],
                                        );
                                    }
                                });
                                if enabled
                                    && matches!(s["type"].as_str(), Some("object" | "array") | None)
                                {
                                    let text =
                                        form.complex.entry(key.clone()).or_insert_with(|| {
                                            serde_json::to_string_pretty(&form.args[key]).unwrap()
                                        });
                                    ui.add(
                                        TextEdit::multiline(text)
                                            .font(TextStyle::Monospace)
                                            .desired_rows(3)
                                            .desired_width(f32::INFINITY),
                                    );
                                    match serde_json::from_str::<Value>(text) {
                                        Ok(v) => form.args[key] = v,
                                        Err(_) => {
                                            invalid = true;
                                            ui.weak(tr("Incomplete JSON"));
                                        }
                                    }
                                }
                            }
                        }
                    });
                if let Err(error) = crate::plugins::validate(&form.schema, &form.args) {
                    invalid = true;
                    ui.weak(error.to_string());
                }
                if !definition.is_read_only() && form.revision != self.session.revision {
                    invalid = true;
                    ui.weak(tr("Document Changed; Refresh Revision Before Running"));
                    if ui.button(tr("Refresh Revision")).clicked() {
                        form.revision = self.session.revision;
                    }
                }
                if ui
                    .add_enabled(!invalid, Button::new(tr("Run Command")))
                    .clicked()
                {
                    run = true;
                }
                if !form.message.is_empty() {
                    ui.label(&form.message);
                }
            });
        if run {
            let mut action = crate::application::Action::new(&form.action, form.args.clone());
            if !definition.is_read_only() {
                action.expected_revision = Some(form.revision);
            }
            match self.session.apply(action) {
                Ok(result) => {
                    self.sync_from_session();
                    form.message = serde_json::to_string_pretty(&result["result"]).unwrap();
                    form.revision = self.session.revision;
                }
                Err(error) => form.message = error.to_string(),
            }
        }
        if open {
            self.command_form = Some(form);
        }
    }
}
