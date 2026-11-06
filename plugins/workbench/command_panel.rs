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
#[cfg(test)]
mod color_selection_tests {
    use super::*;
    #[test]
    fn color_commands_have_searchable_translations_and_nonexecuting_parameter_forms() {
        crate::i18n::load_test_catalogs();
        let mut app = crate::desktop::tests::studio();
        app.editor.register_commands();
        let before = app.editor.session.state();
        for (action, queries) in [
            (
                "selection.wand",
                ["Magic Wand", "魔棒", "自動選択", "자동 선택"],
            ),
            (
                "selection.grow",
                [
                    "Grow Selection",
                    "扩大选区",
                    "選択範囲を拡張",
                    "선택 영역 확대",
                ],
            ),
            (
                "selection.similar",
                [
                    "Select Similar",
                    "选取相似颜色",
                    "類似色を選択",
                    "유사 색상 선택",
                ],
            ),
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette = Palette::new(crate::ui::palette::Mode::Commands, keys);
            for query in queries {
                palette.query(query.into(), keys);
                assert!(
                    palette
                        .results
                        .iter()
                        .any(|&i| keys.command(i).keywords == action),
                    "{action}: {query}"
                );
            }
            app.editor.begin_plugin_command(action);
            let form = app.editor.command_form.as_ref().unwrap();
            assert_eq!(form.action, action);
            assert_eq!(form.args["tolerance"], 32);
            assert_eq!(form.args["sample_merged"], false);
            if action == "selection.wand" {
                assert_eq!(form.args["contiguous"], true);
                assert_eq!(form.args["x"], 0);
                assert_eq!(form.args["y"], 0);
            }
            assert_eq!(app.editor.session.state(), before);
        }
        app.editor
            .session
            .apply_selection_defaults(crate::application::SelectionSampling {
                tolerance: 7,
                contiguous: false,
                sample_merged: true,
            });
        app.editor.begin_plugin_command("selection.wand");
        let form = app.editor.command_form.as_ref().unwrap();
        assert_eq!(form.args["tolerance"], 7);
        assert_eq!(form.args["contiguous"], false);
        assert_eq!(form.args["sample_merged"], true);
    }
    #[test]
    fn edge_parameter_forms_are_searchable_and_do_not_edit_the_selection() {
        crate::i18n::load_test_catalogs();
        let mut app = crate::desktop::tests::studio();
        app.editor.register_commands();
        let before = app.editor.session.state();
        for (id, query) in [
            ("selection.expand", "Expand Selection"),
            ("selection.contract", "收缩选区"),
            ("selection.feather", "選択範囲をぼかす"),
            ("selection.smooth", "선택 영역 매끄럽게"),
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette = Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(query.into(), keys);
            assert!(palette
                .results
                .iter()
                .any(|&i| keys.command(i).keywords == id));
            app.editor.begin_plugin_command(id);
            let form = app.editor.command_form.as_ref().unwrap();
            assert_eq!(form.args["radius"], 4);
            assert_eq!(form.args["edge"], "extend");
            assert_eq!(app.editor.session.state(), before);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&Context::default());
            assert!(app.editor.command_form.is_none());
            assert_eq!(app.editor.session.state(), before);
        }
    }
    #[test]
    fn forms_bind_pending_local_view_state_but_keep_external_edits_stale() {
        let mut app = crate::desktop::tests::studio();
        app.editor.begin_plugin_command("selection.feather");
        app.editor.zoom = 2.;
        let ctx = Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.editor.plugin_command_window(ctx)
        });
        assert_eq!(
            app.editor.command_form.as_ref().unwrap().revision,
            app.editor.session.revision
        );
        app.editor.core("brush.set", json!({"radius":17}));
        app.editor.sync_from_session();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            app.editor.plugin_command_window(ctx)
        });
        assert_ne!(
            app.editor.command_form.as_ref().unwrap().revision,
            app.editor.session.revision
        );
    }
    #[test]
    fn resolution_form_uses_current_metadata_and_four_language_search_without_execution() {
        crate::i18n::load_test_catalogs();
        let mut app = crate::desktop::tests::studio();
        app.editor.register_commands();
        for query in [
            "Set Document Resolution",
            "设置文档分辨率",
            "ドキュメントの解像度を設定",
            "문서 해상도 설정",
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette = Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(query.into(), keys);
            assert!(
                palette
                    .results
                    .iter()
                    .any(|&i| keys.command(i).keywords == "canvas.resolution"),
                "{query}"
            );
        }
        assert!(app.editor.core(
            "canvas.new",
            json!({"width":12,"height":8,"resolution_ppi":300})
        ));
        for resolution in [300, 72, 600] {
            assert!(app
                .editor
                .core("canvas.resolution", json!({"resolution_ppi":resolution})));
            let before = app.editor.session.state();
            let pixels = app.editor.session.doc.layers[0].pixels.clone();
            let events = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            app.editor.begin_plugin_command("canvas.resolution");
            let form = app.editor.command_form.as_mut().unwrap();
            assert_eq!(form.action, "canvas.resolution");
            assert_eq!(form.args["resolution_ppi"], resolution);
            assert_eq!(
                form.schema["properties"]["resolution_ppi"]["default"],
                resolution
            );
            form.args["resolution_ppi"] = json!(1200);
            assert_eq!(app.editor.session.state(), before);
            assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
            assert_eq!(
                app.editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len(),
                events
            );
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&Context::default());
            assert!(app.editor.command_form.is_none());
            assert_eq!(app.editor.session.state(), before);
        }
    }
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
        self.settings
            .keys
            .register_action_owners(&self.session.registry.owners);
    }
    pub(super) fn begin_plugin_command(&mut self, action: &str) {
        let Some(def) = self.session.registry.actions.get(action) else {
            return;
        };
        let mut schema = def.schema();
        if matches!(
            def.operation,
            Some(crate::application::Operation::DocumentResolution)
        ) {
            schema["properties"]["resolution_ppi"]["default"] =
                json!(self.session.doc.info().resolution_ppi);
        }
        if matches!(
            def.operation,
            Some(
                crate::application::Operation::SelectColor
                    | crate::application::Operation::GrowSelection
                    | crate::application::Operation::SimilarSelection
            )
        ) {
            // Parameter forms show current configured defaults; running a form
            // then submits visible, explicit values, just like a saved workflow.
            for (key, value) in serde_json::to_value(self.session.selection_defaults)
                .unwrap()
                .as_object()
                .unwrap()
            {
                if let Some(property) = schema["properties"].get_mut(key) {
                    property["default"] = value.clone();
                }
            }
        }
        if !def.is_read_only()
            && schema["type"] == "object"
            && schema["properties"]
                .as_object()
                .is_none_or(|p| p.is_empty())
        {
            self.core(action, json!({}));
            return;
        }
        let mut args = schema::default_value(&schema);
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
    pub(super) fn geometry_command_form(&self) -> bool {
        self.command_form.as_ref().is_some_and(|form| {
            self.session
                .registry
                .actions
                .get(&form.action)
                .and_then(|definition| definition.operation)
                .is_some_and(crate::application::Operation::is_canvas_geometry_command)
        })
    }
    pub(super) fn plugin_command_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.command_form.take() else {
            return;
        };
        // Canvas fitting may settle after the command opens in this frame.
        // Bind that pending local state before rendering the form, while keeping
        // a revision mismatch caused by an intervening external edit stale.
        let definition = self.session.registry.actions[&form.action].clone();
        let before_sync = self.session.revision;
        if !definition
            .operation
            .is_some_and(crate::application::Operation::is_canvas_geometry_command)
        {
            self.push_gui_parameters();
            self.push_gui_view();
        }
        if form.revision == before_sync {
            form.revision = self.session.revision;
        }
        let mut open = true;
        let mut run = false;
        let mut invalid = false;
        let title = tr_for(
            self.session
                .registry
                .contribution_owner("actions", &form.action),
            &definition.title,
        );
        Window::new(title)
            .id(Id::new("plugin-command-arguments"))
            .open(&mut open)
            .default_width(480.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.weak(&form.action);
                if definition.operation == Some(crate::application::Operation::DocumentResolution) {
                    if let Some(density) = self.session.doc.info().png_density {
                        ui.label(format!("{}: {} × {} {}",localize("Imported PNG Density"),density.xppu,density.yppu,
                            localize(if density.unit == crate::engine::document_info::DensityUnit::Meter {"pixels per meter"} else {"pixels per unspecified unit"})));
                        ui.weak(tr("Applying resolution replaces imported density with square metric pixels. No pixels are resampled."));
                        if !density.metric() { ui.weak(tr("Physical size is unknown for this PNG density.")); }
                    }
                }
                ScrollArea::vertical()
                    .max_height(ctx.content_rect().height() * 0.55)
                    .show(ui, |ui| {
                        if form.schema["type"] != "object" {
                            if matches!(form.schema["type"].as_str(), Some("array") | None) {
                                let text = form
                                    .complex
                                    .entry("$".into())
                                    .or_insert_with(|| form.args.to_string());
                                ui.add(
                                    TextEdit::multiline(text)
                                        .font(TextStyle::Monospace)
                                        .desired_width(f32::INFINITY),
                                );
                                match serde_json::from_str::<Value>(text) {
                                    Ok(v) => form.args = v,
                                    Err(_) => {
                                        invalid = true;
                                        ui.weak(tr("Incomplete JSON"));
                                    }
                                }
                            } else {
                                schema::edit(ui, form.action.clone(), &form.schema, &mut form.args);
                            }
                        }
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
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        if open {
            self.command_form = Some(form);
        }
    }
}
