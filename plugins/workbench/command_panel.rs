//! Every registered action is searchable/bindable; argument editing never executes it.
use super::*;
use serde_json::{json, Value};
pub(super) struct CommandForm {
    action: String,
    contact_policy: crate::plugins::CommandContactPolicy,
    title: String,
    owner: Option<String>,
    document_resolution: bool,
    schema: Value,
    args: Value,
    complex: std::collections::BTreeMap<String, String>,
    revision: u64,
    message: String,
    pub(super) hsl: Option<hsl_preview_ui::Draft>,
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
        self.session.registry.rebuild_command_contact_policies();
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
        self.begin_plugin_command_with_args(action, json!({}));
    }
    /// Capture explicit selectors for a reviewable form without executing them.
    pub(super) fn begin_plugin_command_with_args(&mut self, action: &str, initial: Value) {
        let policy = self.session.registry.command_contact_policy(action);
        if !policy.preserves_contact() {
            self.discard_command_sync();
        }
        self.command_sync_suspended = policy.preserves_contact();
        self.command_form = None;
        let Some(def) = self.session.registry.actions.get(action).cloned() else {
            self.core(action, json!({}));
            self.sync_from_session();
            self.release_command_sync();
            return;
        };
        if matches!(
            def.operation,
            Some(
                crate::application::Operation::HslDialog
                    | crate::application::Operation::TonalDialog
                    | crate::application::Operation::GrayscaleDialog
                    | crate::application::Operation::ThresholdDialog
                    | crate::application::Operation::PosterizeDialog
            )
        ) {
            self.core(action, initial);
            self.sync_from_session();
            self.release_command_sync();
            return;
        }
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
            self.sync_from_session();
            self.release_command_sync();
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
        if let Some(initial) = initial.as_object() {
            for (key, value) in initial {
                args[key] = value.clone();
            }
        }
        let hsl = hsl_preview_ui::Draft::new(&self.session, action, &args);
        self.command_form = Some(CommandForm {
            action: action.into(),
            contact_policy: policy,
            title: def.title,
            owner: self
                .session
                .registry
                .contribution_owner("actions", action)
                .map(str::to_owned),
            document_resolution: def.operation
                == Some(crate::application::Operation::DocumentResolution),
            schema,
            args,
            complex: Default::default(),
            revision: self.session.revision,
            message: String::new(),
            hsl,
        });
    }
    pub(super) fn color_preview_texture(&self) -> Option<&TextureHandle> {
        self.command_form
            .as_ref()?
            .hsl
            .as_ref()?
            .texture(&self.session)
    }
    pub(super) fn guarded_command_form(&self) -> bool {
        self.command_form
            .as_ref()
            .is_some_and(|form| form.contact_policy.preserves_contact())
    }
    pub(super) fn plugin_command_window(&mut self, ctx: &Context) {
        let Some(mut form) = self.command_form.take() else {
            return;
        };
        // Canvas fitting may settle after the command opens in this frame.
        // Bind that pending local state before rendering the form, while keeping
        // a revision mismatch caused by an intervening external edit stale.
        let policy = form.contact_policy;
        let revision_guarded = matches!(
            policy,
            crate::plugins::CommandContactPolicy::SharedAdmission
                | crate::plugins::CommandContactPolicy::LegacyCleanup
        );
        let before_sync = self.session.revision;
        if policy.preserves_contact() {
            // The form is temporarily removed while it renders. Retain the
            // central frame-sync fence until it is restored or closed.
            self.command_sync_suspended = true;
        } else {
            self.push_gui_parameters();
            self.push_gui_view();
        }
        if form.revision == before_sync {
            form.revision = self.session.revision;
        }
        if let Some(mut draft) = form.hsl.take() {
            if self.color_command_window(ctx, &mut draft) {
                form.hsl = Some(draft);
                self.command_form = Some(form);
            }
            self.release_command_sync();
            return;
        }
        let mut open = true;
        let mut run = false;
        let mut invalid = false;
        let title = tr_for(form.owner.as_deref(), &form.title);
        Window::new(title)
            .id(Id::new("plugin-command-arguments"))
            .open(&mut open)
            .default_width(480.)
            .resizable(true)
            .show(ctx, |ui| {
                ui.weak(&form.action);
                if form.document_resolution {
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
                if revision_guarded && form.revision != self.session.revision {
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
            if revision_guarded {
                action.expected_revision = Some(form.revision);
            }
            let previous = self.backend_contact_stamp();
            let result = self.session.apply(action);
            self.reconcile_command_contact(previous);
            match result {
                Ok(result) => {
                    self.sync_from_session();
                    form.message = serde_json::to_string_pretty(&result["result"]).unwrap();
                    form.revision = self.session.revision;
                }
                Err(error) => {
                    self.sync_from_session();
                    form.message = error.to_string();
                }
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        if open {
            self.command_form = Some(form);
        }
        self.release_command_sync();
    }
}

#[cfg(test)]
mod brush_preset_form_tests {
    use super::*;
    #[test]
    fn brush_presets_export_form_prefills_captured_explicit_ids_and_token_without_execution() {
        let mut app = crate::desktop::tests::studio();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/brush-presets/prefill/{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let mut settings = crate::configuration::Settings::decode(
            r#"{"version":1}"#,
            directory.join("settings.json"),
        )
        .unwrap();
        let p = crate::configuration::brush_presets::PresetParameters::capture(
            &crate::application::Parameters::default(),
        )
        .unwrap();
        for name in ["One", "Two"] {
            settings
                .brush_presets
                .insert(name.into(), "painting".into(), "brush".into(), p.clone())
                .unwrap();
        }
        settings.save().unwrap();
        app.editor.settings = settings;
        app.editor.settings.ui.language = crate::i18n::Language::English;
        crate::i18n::set_language(crate::i18n::Language::English);
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        app.editor.session.settings_path = Some(app.editor.settings.path.clone());
        app.editor
            .session
            .load_profile_presets(&app.editor.settings)
            .unwrap();
        app.editor.register_commands();
        assert!(app.editor.core("canvas.new", json!({"width":1,"height":1})));
        app.editor.sync_from_session();
        let token = app.editor.session.preset_library_token();
        let before = app.editor.session.state();
        let target = directory.join("captured.json");
        if target.exists() {
            std::fs::remove_file(&target).unwrap();
        }
        app.editor.begin_plugin_command_with_args(
            "brush.presets.export",
            json!({"preset_ids":[2,1],"expected_library_token":token}),
        );
        let form = app.editor.command_form.as_mut().unwrap();
        assert_eq!(form.args["preset_ids"], json!([2, 1]));
        assert_eq!(form.args["expected_library_token"], token);
        assert_eq!(form.args["path"], "");
        assert_eq!(app.editor.session.state(), before);
        assert!(!target.exists());
        // The ordinary path field is edited before the actual shared Run button.
        app.editor.command_form.as_mut().unwrap().args["path"] = json!(target);
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let mut output = None;
        for n in 0..3 {
            output = Some(ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 1000.))),
                    time: Some(n as f64 / 10.),
                    focused: true,
                    ..Default::default()
                },
                |ctx| app.editor.plugin_command_window(ctx),
            ));
        }
        fn run_button(shape: &Shape) -> Option<Pos2> {
            match shape {
                Shape::Text(text) if text.galley.text() == "Run Command" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                Shape::Vec(shapes) => shapes.iter().find_map(run_button),
                _ => None,
            }
        }
        let position = output
            .unwrap()
            .shapes
            .iter()
            .find_map(|shape| run_button(&shape.shape))
            .unwrap();
        for (n, pressed) in [true, false].into_iter().enumerate() {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 1000.))),
                    time: Some(1. + n as f64 / 10.),
                    focused: true,
                    events: vec![
                        Event::PointerMoved(position),
                        Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ctx| app.editor.plugin_command_window(ctx),
            );
        }
        let bundle = crate::configuration::brush_presets::BrushPresetBundle::decode(
            &std::fs::read_to_string(&target).unwrap(),
        )
        .unwrap();
        assert_eq!(
            bundle.items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![2, 1]
        );
        assert_eq!(
            bundle
                .items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Two", "One"]
        );
        assert_eq!(app.editor.session.preset_library_token(), token);
        let mut expected = before;
        expected["revision"] = json!(expected["revision"].as_u64().unwrap() + 1);
        assert_eq!(app.editor.session.state(), expected);
    }
}
