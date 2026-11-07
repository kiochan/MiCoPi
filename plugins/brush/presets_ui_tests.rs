//! Actual library controls keep artist swatches and native contacts independent.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::configuration::Settings;
use crate::desktop::tests::studio;

fn fixture(tag: &str) -> Studio {
    let mut app = studio();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-presets/ui-{tag}/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let settings = Settings::decode(
        r##"{"version":1,"custom":{"keep":42},"keybindings":{"file.save":"Ctrl+F12"}}"##,
        directory.join("settings.json"),
    )
    .unwrap();
    settings.save().unwrap();
    app.editor.settings = settings;
    app.editor.settings.ui.language = crate::i18n::Language::English;
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
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
    assert!(
        app.editor
            .core("canvas.new", json!({"width":12,"height":8})),
        "{}",
        app.editor.status
    );
    assert!(app.editor.core("brush.set",json!({"color":"#B64D7A","background_color":"#DCEAF1","radius":3,"stabilizer":0.55,"pressure_size":true,"pressure_opacity":true,"flow":0.25,"opacity":0.4,"opacity_mode":"stroke"})),"{}",app.editor.status);
    app.editor.sync_from_session();
    app
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
}
fn frame(app: &mut Studio, ctx: &Context, width: f32, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    let modifiers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 900.))),
            events,
            modifiers,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.user_brush_presets_ui(ui));
        },
    )
}
fn warm(app: &mut Studio, ctx: &Context, width: f32, time: f64) -> FullOutput {
    frame(app, ctx, width, vec![], time);
    frame(app, ctx, width, vec![], time + 0.01);
    frame(app, ctx, width, vec![], time + 0.02)
}
fn click(app: &mut Studio, ctx: &Context, width: f32, rect: Rect, time: f64) -> FullOutput {
    for (n, pressed) in [true, false].into_iter().enumerate() {
        frame(
            app,
            ctx,
            width,
            vec![
                Event::PointerMoved(rect.center()),
                Event::PointerButton {
                    pos: rect.center(),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            time + n as f64 / 100.,
        );
    }
    frame(app, ctx, width, vec![], time + 0.02);
    frame(app, ctx, width, vec![], time + 0.03)
}
fn rect(ctx: &Context, id: impl std::hash::Hash) -> Rect {
    ctx.data_mut(|data| data.get_temp::<Rect>(Id::new(id)))
        .unwrap()
}
fn control(ctx: &Context, id: impl std::hash::Hash) -> (Rect, bool) {
    ctx.data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new(id)))
        .unwrap()
}
fn replace(app: &mut Studio, ctx: &Context, id: &str, text: &str, time: f64) {
    click(app, ctx, 360., rect(ctx, id), time);
    let mods = Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(
        app,
        ctx,
        360.,
        vec![
            Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: mods,
            },
            Event::Text(text.into()),
        ],
        time + 0.1,
    );
}
fn save_current(app: &mut Studio, ctx: &Context, name: &str, time: f64) -> u64 {
    warm(app, ctx, 360., time);
    click(
        app,
        ctx,
        360.,
        control(ctx, "user-brush-save-current").0,
        time + 0.1,
    );
    replace(app, ctx, "user-brush-preset-name-input", name, time + 0.2);
    click(
        app,
        ctx,
        360.,
        control(ctx, "user-brush-preset-save").0,
        time + 0.4,
    );
    assert!(
        ctx.data_mut(|data| data.get_temp::<NameDraft>(Id::new(DRAFT)))
            .is_none(),
        "{}",
        app.editor.status
    );
    let item = app.editor.session.preset_library().items.last().unwrap();
    assert_eq!(item.name, name);
    item.id
}
fn menu(app: &mut Studio, ctx: &Context, id: u64, action: &str, time: f64) {
    warm(app, ctx, 360., time);
    click(
        app,
        ctx,
        360.,
        rect(ctx, ("user-brush-preset-menu", id)),
        time + 0.1,
    );
    click(
        app,
        ctx,
        360.,
        rect(ctx, ("user-brush-preset-action", id, action)),
        time + 0.2,
    );
}
fn raw(app: &Studio) -> (Vec<Vec<u32>>, usize, Value) {
    (
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect(),
        app.editor.session.doc.history_bytes(),
        serde_json::to_value(app.editor.session.doc.composition()).unwrap(),
    )
}
fn samples(app: &mut Studio) -> Vec<Value> {
    let journal = app.editor.session.replay_journal(false).unwrap();
    let entries = journal.events(0, journal.entries.len()).unwrap();
    let mut samples = entries["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["source"]["kind"] == "pointer")
        .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
        .collect::<Vec<_>>();
    samples.extend(
        serde_json::to_value(&app.editor.session.replay_samples)
            .unwrap()
            .as_array()
            .unwrap()
            .clone(),
    );
    samples
}
fn down(app: &mut Studio, kind: ToolKind, local: bool) {
    app.editor
        .session
        .pointer(Pointer {
            phase: PointerPhase::Down,
            x: 5.5,
            y: 4.5,
            pressure: 0.7,
            pen: true,
            eraser: false,
            time: 100,
            tool_override: Some(kind),
            view_x: Some(50.),
            view_y: Some(40.),
        })
        .unwrap();
    app.editor.sync_from_session();
    app.editor.input_owned = local;
    app.editor.pen_active = local;
    app.editor.panning = (local && kind == ToolKind::Hand).then_some(Pos2::ZERO);
}
fn label(shape: &Shape, title: &str) -> Option<Rect> {
    match shape {
        Shape::Text(text) if text.galley.text() == title => {
            Some(Rect::from_min_size(text.pos, text.galley.size()))
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, title)),
        _ => None,
    }
}

#[test]
fn brush_presets_ui_save_and_apply_capture_dynamics_preserve_both_swatches_and_real_tool() {
    let mut app = fixture("save-apply");
    let ctx = context(&app);
    let before = raw(&app);
    let id = save_current(&mut app, &ctx, "Soft", 0.);
    let record = app.editor.session.preset_library().get(id).unwrap().clone();
    assert_eq!(record.owner, "painting");
    assert_eq!(record.tool_id, "brush");
    assert_eq!(record.parameters.flow, 0.25);
    assert_eq!(
        record.parameters.opacity_mode,
        crate::application::OpacityMode::Stroke
    );
    assert_eq!(record.parameters.stabilizer, 0.55);
    assert!(record.parameters.pressure_size && record.parameters.pressure_opacity);
    let value = record.parameters.value().unwrap();
    assert!(value.get("color").is_none() && value.get("background_color").is_none());
    assert_eq!(raw(&app), before);
    assert!(app.editor.core("tool.select", json!({"tool":"eraser"})));
    assert!(app.editor.core(
        "brush.set",
        json!({"radius":9,"flow":1,"color":"#224466","background_color":"#F1E3C8"})
    ));
    app.editor.sync_from_session();
    warm(&mut app, &ctx, 360., 1.);
    down(&mut app, ToolKind::Brush, true);
    let painted = raw(&app);
    let before_samples = samples(&mut app);
    assert_eq!(before_samples.len(), 1);
    click(
        &mut app,
        &ctx,
        360.,
        control(&ctx, ("user-brush-preset-card", id)).0,
        2.,
    );
    assert!(!app.editor.session.stroke.active());
    assert!(!app.editor.input_owned && !app.editor.pen_active);
    let after = samples(&mut app);
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before_samples[0]);
    assert_eq!(after[1]["phase"], "up");
    assert_eq!(app.editor.session.selected_tool, "brush");
    assert_eq!(app.editor.radius, 3.);
    assert_eq!(app.editor.session.parameters.flow, 0.25);
    assert_eq!(app.editor.session.parameters.color, "#224466");
    assert_eq!(app.editor.session.parameters.background_color, "#F1E3C8");
    assert_eq!(
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        painted.0
    );
    assert!(app
        .editor
        .session
        .registry
        .tools
        .contains_key(&app.editor.session.selected_tool));
}

#[test]
fn brush_presets_ui_rename_update_delete_do_not_change_live_brush_or_document() {
    let mut app = fixture("management");
    let ctx = context(&app);
    let id = save_current(&mut app, &ctx, "Original", 0.);
    let before = raw(&app);
    let parameters = app.editor.session.parameters.clone();
    menu(&mut app, &ctx, id, "rename", 1.);
    replace(
        &mut app,
        &ctx,
        "user-brush-preset-name-input",
        "Ink for 星",
        1.4,
    );
    click(
        &mut app,
        &ctx,
        360.,
        control(&ctx, "user-brush-preset-save").0,
        1.6,
    );
    assert_eq!(
        app.editor.session.preset_library().get(id).unwrap().name,
        "Ink for 星"
    );
    assert_eq!(app.editor.session.parameters, parameters);
    assert_eq!(raw(&app), before);
    assert!(app
        .editor
        .core("brush.set", json!({"radius":8,"flow":0.4,"stabilizer":0.2})));
    app.editor.sync_from_session();
    let parameters = app.editor.session.parameters.clone();
    menu(&mut app, &ctx, id, "update", 2.);
    let updated = app.editor.session.preset_library().get(id).unwrap();
    assert_eq!(updated.parameters.radius, 8.);
    assert_eq!(updated.parameters.flow, 0.4);
    assert_eq!(updated.name, "Ink for 星");
    assert_eq!(app.editor.session.parameters, parameters);
    assert_eq!(raw(&app), before);
    menu(&mut app, &ctx, id, "delete", 3.);
    assert!(app.editor.session.preset_library().items.is_empty());
    assert_eq!(app.editor.session.parameters, parameters);
    assert_eq!(raw(&app), before);
    let file: Value =
        serde_json::from_slice(&std::fs::read(&app.editor.settings.path).unwrap()).unwrap();
    assert_eq!(file["custom"]["keep"], 42);
}

#[test]
fn brush_presets_ui_stale_name_draft_is_retained_and_never_overwrites_new_library() {
    let mut app = fixture("stale");
    let ctx = context(&app);
    let id = save_current(&mut app, &ctx, "Original", 0.);
    menu(&mut app, &ctx, id, "rename", 1.);
    replace(
        &mut app,
        &ctx,
        "user-brush-preset-name-input",
        "Keep this draft",
        1.4,
    );
    let captured = crate::plugins::brush_presets::capture_current(&app.editor.session).unwrap();
    assert!(app.editor.core("brush.presets.upsert",json!({"name":"External change","owner":captured.owner,"tool_id":captured.tool_id,"parameters":captured.parameters.value().unwrap()})));
    let before = app.editor.session.preset_library().clone();
    warm(&mut app, &ctx, 360., 2.);
    let (save, enabled) = control(&ctx, "user-brush-preset-save");
    assert!(!enabled);
    click(&mut app, &ctx, 360., save, 2.1);
    assert_eq!(app.editor.session.preset_library(), &before);
    assert_eq!(
        ctx.data_mut(|data| data.get_temp::<NameDraft>(Id::new(DRAFT)))
            .unwrap()
            .name,
        "Keep this draft"
    );
    click(
        &mut app,
        &ctx,
        360.,
        rect(&ctx, "user-brush-preset-cancel"),
        2.3,
    );
    assert!(ctx
        .data_mut(|data| data.get_temp::<NameDraft>(Id::new(DRAFT)))
        .is_none());
}

#[test]
fn brush_presets_ui_guarded_file_writes_and_missing_provider_apply_preserve_owned_and_external_contacts(
) {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = fixture(&format!("contact-{kind:?}-{local}"));
            let ctx = context(&app);
            let id = save_current(&mut app, &ctx, "Original", 0.);
            let captured =
                crate::plugins::brush_presets::capture_current(&app.editor.session).unwrap();
            assert!(app.editor.core("brush.presets.upsert",json!({"name":"Unavailable","owner":"missing.provider","tool_id":captured.tool_id,"parameters":captured.parameters.value().unwrap()})));
            let missing = app.editor.session.preset_library().items.last().unwrap().id;
            warm(&mut app, &ctx, 360., 1.);
            let (disabled, available) = control(&ctx, ("user-brush-preset-card", missing));
            assert!(!available);
            down(&mut app, kind, local);
            let before = app.editor.session.state();
            let pixels = raw(&app);
            let transcript = samples(&mut app);
            let bytes = std::fs::read(&app.editor.settings.path).unwrap();
            click(&mut app, &ctx, 360., disabled, 2.);
            assert_eq!(app.editor.session.state(), before);
            assert_eq!(samples(&mut app), transcript);
            menu(&mut app, &ctx, id, "delete", 3.);
            assert!(
                app.editor.status.contains("stroke_active"),
                "{}",
                app.editor.status
            );
            assert_eq!(app.editor.session.state(), before);
            assert_eq!(raw(&app), pixels);
            assert_eq!(samples(&mut app), transcript);
            assert_eq!(std::fs::read(&app.editor.settings.path).unwrap(), bytes);
            assert!(
                app.editor.session.stroke.active()
                    && app.editor.session.pointer_checkpoint.is_some()
            );
            assert_eq!(app.editor.input_owned, local);
            assert_eq!(app.editor.pen_active, local);
            assert_eq!(
                app.editor.panning.is_some(),
                local && kind == ToolKind::Hand
            );
            let entry = app
                .editor
                .settings
                .keys
                .commands()
                .find(|command| command.keywords == "brush.presets.query")
                .unwrap()
                .ui;
            app.editor.actions.push(Action::UiCommand(entry));
            app.run_actions(&ctx);
            assert!(app.editor.command_form.is_some());
            assert_eq!(app.editor.session.state(), before);
            assert_eq!(samples(&mut app), transcript);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            assert!(app.editor.command_form.is_none());
            assert_eq!(app.editor.session.state(), before);
        }
    }
}

#[test]
fn brush_presets_ui_cards_search_and_name_controls_fit_four_languages_without_translating_names() {
    crate::i18n::load_test_catalogs();
    let mut app = fixture("locales");
    let ctx = context(&app);
    let id = save_current(&mut app, &ctx, "Soft", 0.);
    let captured = crate::plugins::brush_presets::capture_current(&app.editor.session).unwrap();
    assert!(app.editor.core("brush.presets.upsert",json!({"name":"星影 ink","owner":"missing.provider","tool_id":captured.tool_id,"parameters":captured.parameters.value().unwrap()})));
    for (n, language) in crate::i18n::Language::ALL.into_iter().enumerate() {
        app.editor.settings.ui.language = language;
        let base = 10. + n as f64 * 10.;
        menu(&mut app, &ctx, id, "rename", base);
        for (action, title) in [
            ("brush.presets.query", "Query Brush Presets"),
            ("brush.presets.upsert", "Save User Brush Preset"),
            ("brush.presets.delete", "Delete User Brush Preset"),
            ("brush.presets.apply", "Apply User Brush Preset"),
            ("brush.presets.import", "Import Brush Presets"),
            ("brush.presets.export", "Export Brush Presets"),
            ("brush.presets.reload", "Reload Brush Preset Library"),
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette = Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(crate::i18n::text(title).into_owned(), keys);
            assert!(
                palette
                    .results
                    .iter()
                    .any(|index| keys.command(*index).keywords == action),
                "{language:?}: {action}"
            );
        }
        for (j, width) in [176., 360., 680.].into_iter().enumerate() {
            let time = base + 1. + j as f64;
            let output = warm(&mut app, &ctx, width, time);
            let before = app.editor.session.state();
            let title = output
                .shapes
                .iter()
                .find_map(|shape| label(&shape.shape, "Soft"))
                .unwrap_or_else(|| {
                    panic!("{language:?}/{width}: user name was translated or hidden")
                });
            assert!(
                title.right() <= width + 0.5,
                "{language:?}/{width}: {title:?}"
            );
            for key in ["user-brush-save-current"] {
                let (rect, _) = control(&ctx, key);
                assert!(
                    rect.right() <= width + 0.5,
                    "{language:?}/{width}: {rect:?}"
                );
            }
            assert!(
                rect(&ctx, "user-brush-preset-name-input").right() <= width + 0.5,
                "{language:?}/{width}: name input"
            );
            assert!(
                control(&ctx, "user-brush-preset-save").0.right() <= width + 0.5,
                "{language:?}/{width}: save name"
            );
            for preset in &app.editor.session.preset_library().items {
                let (rect, _) = control(&ctx, ("user-brush-preset-card", preset.id));
                assert!(
                    rect.right() <= width + 0.5 && rect.left() >= 0.,
                    "{language:?}/{width}: {rect:?}"
                );
            }
            assert_eq!(app.editor.session.state(), before);
        }
        click(
            &mut app,
            &ctx,
            680.,
            rect(&ctx, "user-brush-preset-cancel"),
            base + 5.,
        );
        assert!(ctx
            .data_mut(|data| data.get_temp::<NameDraft>(Id::new(DRAFT)))
            .is_none());
    }
    app.editor.settings.ui.language = crate::i18n::Language::English;
    warm(&mut app, &ctx, 360., 60.);
    // Search uses owned names, independent of the current presentation language.
    replace(
        &mut app,
        &ctx,
        "user-brush-preset-search-input",
        "星影",
        61.,
    );
    let out = warm(&mut app, &ctx, 360., 61.2);
    assert!(out
        .shapes
        .iter()
        .any(|shape| label(&shape.shape, "星影 ink").is_some()));
    assert!(out
        .shapes
        .iter()
        .all(|shape| label(&shape.shape, "Soft").is_none()));
    ctx.data_mut(|data| data.insert_temp(Id::new(SEARCH), String::new()));
    crate::i18n::set_language(crate::i18n::Language::English);
}
