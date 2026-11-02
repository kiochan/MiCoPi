use super::*;
use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
pub(super) fn studio() -> Studio {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/egui");
    std::fs::create_dir_all(&root).unwrap();
    let mut editor = Editor::new(root.join("nonexistent-test-settings.json"), None);
    editor.workspace = Workspace::default();
    let applied = editor.settings.ui.clone();
    Studio {
        replay: replay::ReplayWindow::default(),
        action_server: None,
        from_core_ui: false,
        pending_capture: None,
        dock: default_dock(&applied, 1360.),
        editor,
        dock_before_focus: None,
        settings_open: false,
        settings_before: None,
        preferences: preferences::Preferences::default(),
        applied,
        layout_path: root.join("layout.json"),
        layout_snapshot: String::new(),
        layout_changed: None,
        preview: None,
        preview_frames: 0,
        screenshot_requested: false,
        allow_close: false,
        title: String::new(),
    }
}

#[test]
fn configured_theme_survives_opposite_system_theme_on_first_frame() {
    for (name, system, dark) in [
        ("graphite", egui::Theme::Light, true),
        ("light", egui::Theme::Dark, false),
    ] {
        let ctx = Context::default();
        let mut appearance = Appearance::default();
        appearance.theme = name.into();
        theme::apply(&ctx, &appearance);
        let _ = ctx.run(
            RawInput {
                system_theme: Some(system),
                ..Default::default()
            },
            |ctx| {
                assert_eq!(ctx.style().visuals.dark_mode, dark);
                assert_eq!(
                    ctx.style().visuals.override_text_color,
                    Some(theme::Colors::new(&appearance).text)
                );
            },
        );
    }
}
pub(super) fn frame(
    app: &mut Studio,
    ctx: &Context,
    size: Vec2,
    events: Vec<Event>,
    time: f64,
) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    if !ctx.data_mut(|d| d.get_temp::<bool>(Id::new("test-fonts")).unwrap_or(false)) {
        theme::fonts(ctx);
        theme::apply(ctx, &app.editor.settings.ui);
        ctx.data_mut(|d| d.insert_temp(Id::new("test-fonts"), true));
    }
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.frame_ui(ctx),
    )
}
#[test]
fn all_languages_render_localized_menus_and_settings_without_changing_the_document() {
    fn contains(shape: &Shape, message: &str) -> bool {
        match shape {
            Shape::Text(text) => text.galley.job.text == message,
            Shape::Vec(shapes) => shapes.iter().any(|s| contains(s, message)),
            _ => false,
        }
    }
    let mut app = studio();
    let before = app.editor.session.doc.layers[0].pixels.clone();
    for locale in crate::i18n::Language::ALL {
        let ctx = Context::default();
        app.editor.settings.ui.language = locale;
        for (i, size) in [vec2(960., 680.), vec2(1700., 1150.)]
            .into_iter()
            .enumerate()
        {
            let out = frame(&mut app, &ctx, size, vec![], i as f64);
            assert!(
                out.shapes
                    .iter()
                    .any(|s| contains(&s.shape, crate::i18n::text_in(locale, "File"))),
                "{}",
                locale.code()
            );
            assert!(out.shapes.iter().all(|s| s.clip_rect.is_finite()));
            assert!(app.editor.canvas.width() > 200. && app.editor.canvas.height() > 100.);
        }
        app.open_settings(Some(preferences::Category::Appearance));
        app.preferences.query = "ui.language".into();
        frame(&mut app, &ctx, vec2(1700., 1150.), vec![], 3.);
        let out = frame(&mut app, &ctx, vec2(1700., 1150.), vec![], 4.);
        assert!(out
            .shapes
            .iter()
            .any(|s| contains(&s.shape, crate::i18n::text_in(locale, "Display Language"))));
        app.cancel_settings();
        assert_eq!(app.editor.session.doc.layers[0].pixels, before);
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
#[test]
fn dock_layout_has_one_canvas_and_roundtrips_groups_and_floating_panels() {
    let mut dock = default_dock(&Appearance::default(), 1360.);
    assert!(valid_dock(&dock));
    let brush = dock.find_tab(&Panel::Brush).unwrap();
    dock.remove_tab(brush);
    let (surface, node, _) = dock.find_tab(&Panel::Color).unwrap();
    dock.set_focused_node_and_surface((surface, node));
    dock.push_to_focused_leaf(Panel::Brush);
    assert_eq!(
        dock.find_tab(&Panel::Color).unwrap().1,
        dock.find_tab(&Panel::Brush).unwrap().1
    );
    let nav = dock.find_tab(&Panel::Navigator).unwrap();
    dock.remove_tab(nav);
    dock.add_window(vec![Panel::Navigator]);
    let restored: DockState<Panel> = serde_json::from_str(&encode_dock(&dock).unwrap()).unwrap();
    assert!(valid_dock(&restored));
    assert_ne!(
        restored.find_tab(&Panel::Navigator).unwrap().0,
        SurfaceIndex::main()
    );
    let mut duplicate = DockState::new(vec![Panel::Canvas, Panel::Canvas]);
    assert!(!valid_dock(&duplicate));
    duplicate
        .main_surface_mut()
        .push_to_first_leaf(Panel::Brush);
    assert!(!valid_dock(&duplicate));
}
#[test]
fn workbench_keeps_a_visible_canvas_across_sizes_themes_and_density() {
    let mut app = studio();
    let ctx = Context::default();
    theme::fonts(&ctx);
    for (index, (size, compact, theme_name)) in [
        (vec2(960., 680.), true, "graphite"),
        (vec2(1360., 920.), false, "light"),
        (vec2(1920., 1080.), false, "warm"),
    ]
    .into_iter()
    .enumerate()
    {
        app.editor.settings.ui = Appearance::default();
        if compact {
            app.editor.settings.ui.compact();
        }
        app.editor.settings.ui.theme = theme_name.into();
        app.dock = default_dock(&app.editor.settings.ui, size.x - 64.);
        for n in 0..3 {
            let output = frame(&mut app, &ctx, size, vec![], (index * 3 + n) as f64);
            assert!(output.shapes.iter().all(|s| s.clip_rect.is_finite()));
        }
        assert!(
            app.editor.canvas.width() > 280.,
            "{:?}: {:?}",
            size,
            app.editor.canvas
        );
        assert!(app.editor.canvas.height() > 350.);
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(app.editor.canvas));
        app.settings_open = true;
        let _ = frame(&mut app, &ctx, size, vec![], 10. + index as f64);
        assert!(app.editor.blocked);
        app.settings_open = false;
    }
}
#[test]
fn mouse_strokes_use_canvas_coordinates_and_upload_only_changed_pixels() {
    let mut app = studio();
    let ctx = Context::default();
    theme::apply(&ctx, &app.editor.settings.ui);
    let size = vec2(1360., 920.);
    for t in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![], t as f64);
    }
    app.editor.workspace.stabilizer = 0.;
    let center = app.editor.image_rect.center();
    let down = Event::PointerButton {
        pos: center,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    let output = frame(
        &mut app,
        &ctx,
        size,
        vec![Event::PointerMoved(center), down],
        3.,
    );
    assert!(app.editor.drawing);
    let id = app.editor.texture.as_ref().unwrap().id();
    let delta = &output
        .textures_delta
        .set
        .iter()
        .find(|(texture, _)| *texture == id)
        .unwrap()
        .1;
    assert!(delta.pos.is_some());
    assert!(delta.image.size()[0] < app.editor.session.doc.width);
    let end = center + vec2(70., 0.);
    let _ = frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerMoved(end),
            Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        3.1,
    );
    assert!(!app.editor.drawing);
    assert!(app.editor.session.doc.history_bytes() > 0);
    assert_ne!(app.editor.session.doc.layers[0].pixels[450 * 1200 + 600], 0);
    app.editor.session.doc.undo();
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
}
#[test]
fn ink_packets_keep_pressure_and_do_not_double_paint_promoted_mouse_events() {
    let mut app = studio();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for t in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![], t as f64);
    }
    app.editor.workspace.stabilizer = 0.;
    app.editor.radius = 30.;
    let center = app.editor.image_rect.center();
    let ppp = ctx.pixels_per_point();
    let sample = PenSample {
        phase: PenPhase::Down,
        x: center.x * ppp,
        y: center.y * ppp,
        pressure: 0.2,
        eraser: false,
        time: 3000,
    };
    app.editor.packets = vec![sample];
    let _ = frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerMoved(center),
            Event::PointerButton {
                pos: center,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        3.,
    );
    assert!(app.editor.pen_active && app.editor.drawing);
    assert!((app.editor.pressure - 0.2).abs() < 0.001);
    let count = app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .filter(|p| **p != 0)
        .count();
    assert!(count > 0 && count < 400, "pressure not applied: {count}");
    app.editor.packets = vec![PenSample {
        phase: PenPhase::Up,
        pressure: 0.,
        time: 3010,
        ..sample
    }];
    let _ = frame(&mut app, &ctx, size, vec![], 3.01);
    assert!(!app.editor.pen_active && !app.editor.drawing);
    assert_eq!(
        app.editor.session.doc.layers[0]
            .pixels
            .iter()
            .filter(|p| **p != 0)
            .count(),
        count
    );
}
#[test]
fn configured_shortcuts_dispatch_and_text_fields_block_global_tool_changes() {
    let mut app = studio();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    app.editor.settings.keys = Keymap::decode(r#"{"keybindings":{"tool.eraser":["F8"]}}"#).unwrap();
    for t in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![], t as f64);
    }
    let _ = frame(
        &mut app,
        &ctx,
        size,
        vec![Event::Key {
            key: Key::F8,
            physical_key: Some(Key::F8),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        3.,
    );
    assert!(app.editor.tool == Tool::Eraser);
    app.editor.open_palette(crate::ui::palette::Mode::Commands);
    let _ = frame(&mut app, &ctx, size, vec![Event::Text("b".into())], 4.);
    assert!(app.editor.tool == Tool::Eraser);
    assert!(app.editor.palette.is_some());
}

#[test]
fn layer_name_entry_commits_and_duplicate_shortcut_uses_the_shared_action() {
    let mut app = studio();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for t in 0..3 {
        frame(&mut app, &ctx, size, vec![], t as f64);
    }
    let id = app.editor.session.doc.layer_id(0);
    let edit = Id::new(("layer-name", app.editor.session.doc.identity, id));
    ctx.memory_mut(|m| m.request_focus(edit));
    ctx.data_mut(|d| d.insert_temp(edit, "Ink 墨".to_string()));
    let key = |key, modifiers| Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        &mut app,
        &ctx,
        size,
        vec![key(Key::Enter, Modifiers::NONE)],
        3.,
    );
    assert_eq!(app.editor.session.doc.layer_name(0), "Ink 墨");
    ctx.memory_mut(|m| m.surrender_focus(edit));
    frame(&mut app, &ctx, size, vec![], 4.);
    frame(&mut app, &ctx, size, vec![key(Key::J, Modifiers::CTRL)], 5.);
    assert_eq!(app.editor.session.doc.layers.len(), 2);
    assert_eq!(app.editor.session.doc.layer_name(1), "Ink 墨");
    assert_ne!(app.editor.session.doc.layer_id(1), id);
    app.editor.core("history.undo", serde_json::json!({}));
    assert_eq!(app.editor.session.doc.layers.len(), 1);
    assert_eq!(app.editor.session.doc.layer_id(0), id);
}

#[test]
fn alpha_lock_toggle_keeps_pending_names_and_color_shortcuts_keep_coverage() {
    fn label(shape: &Shape) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.job.text == "Lock Layer Transparency" => {
                Some(t.pos + t.galley.rect.center().to_vec2())
            }
            Shape::Vec(shapes) => shapes.iter().find_map(label),
            _ => None,
        }
    }
    let mut app = studio();
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor
        .core("canvas.new", serde_json::json!({"width":12,"height":8}));
    app.editor
        .core("layer.fill_color", serde_json::json!({"color":"#33669960"}));
    app.editor.core(
        "brush.set",
        serde_json::json!({"color":"#ff0000","background_color":"#00ff00","opacity":1}),
    );
    app.editor.sync_from_session();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for t in 0..3 {
        frame(&mut app, &ctx, size, vec![], t as f64);
    }
    let id = Id::new((
        "layer-name",
        app.editor.session.doc.identity,
        app.editor.session.doc.layer_id(0),
    ));
    ctx.memory_mut(|m| m.request_focus(id));
    ctx.data_mut(|d| d.insert_temp(id, "Cutout".to_string()));
    let output = frame(&mut app, &ctx, size, vec![], 3.);
    let point = output.shapes.iter().find_map(|s| label(&s.shape)).unwrap();
    let pointer = |pressed| Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    frame(
        &mut app,
        &ctx,
        size,
        vec![Event::PointerMoved(point), pointer(true), pointer(false)],
        4.,
    );
    assert_eq!(app.editor.session.doc.layer_name(0), "Cutout");
    assert!(app.editor.session.doc.alpha_locked(0));
    ctx.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    frame(&mut app, &ctx, size, vec![], 5.);
    let key = |pressed, modifiers| Event::Key {
        key: Key::Backspace,
        physical_key: Some(Key::Backspace),
        pressed,
        repeat: false,
        modifiers,
    };
    frame(&mut app, &ctx, size, vec![key(true, Modifiers::ALT)], 6.);
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0x60ff0000));
    frame(&mut app, &ctx, size, vec![key(false, Modifiers::ALT)], 7.);
    frame(&mut app, &ctx, size, vec![key(true, Modifiers::CTRL)], 8.);
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0x6000ff00));
}

#[test]
fn canvas_area_gestures_commit_once_cancel_and_guard_the_document_target() {
    let mut app = studio();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    assert!(app
        .editor
        .core("canvas.new", serde_json::json!({"width":4,"height":3})));
    assert!(app.editor.core(
        "brush.set",
        serde_json::json!({"color":"#ff0000","background_color":"#0000ff","opacity":1})
    ));
    app.editor.sync_from_session();
    for t in 0..3 {
        frame(&mut app, &ctx, size, vec![], t as f64);
    }
    let position =
        |editor: &Editor, x: f32, y: f32| editor.image_rect.min + vec2(x, y) * editor.zoom;
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    app.editor.area_mode = 1;
    let a = position(&app.editor, 0.5, 1.5);
    let b = position(&app.editor, 3.5, 1.5);
    frame(
        &mut app,
        &ctx,
        size,
        vec![Event::PointerMoved(a), button(a, true), button(a, false)],
        3.,
    );
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0xffff0000));
    assert!(app.editor.core("history.undo", serde_json::json!({})));
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    app.editor.area_mode = 2;
    frame(
        &mut app,
        &ctx,
        size,
        vec![button(a, true), Event::PointerMoved(b)],
        4.,
    );
    assert!(app.editor.area_drag.is_some());
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    frame(&mut app, &ctx, size, vec![button(b, false)], 5.);
    assert_eq!(app.editor.session.doc.layers[0].pixels[0], 0xffff0000);
    assert_eq!(app.editor.session.doc.layers[0].pixels[3], 0xff0000ff);
    app.editor.core("history.undo", serde_json::json!({}));
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            button(a, true),
            Event::PointerMoved(b),
            Event::WindowFocused(false),
        ],
        6.,
    );
    frame(&mut app, &ctx, size, vec![button(b, false)], 7.);
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    frame(
        &mut app,
        &ctx,
        size,
        vec![button(a, true), Event::PointerMoved(b)],
        8.,
    );
    app.editor.core("layer.add", serde_json::json!({}));
    frame(&mut app, &ctx, size, vec![button(b, false)], 9.);
    assert!(app
        .editor
        .session
        .doc
        .layers
        .iter()
        .all(|l| l.pixels.iter().all(|p| *p == 0)));
}

#[test]
fn area_controls_and_shift_gesture_match_public_native_actions() {
    use crate::application::{Action as CoreAction, Session};
    let registry =
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    let mut expected = Session::new(registry.clone()).unwrap();
    let mut app = studio();
    app.editor.session = Session::new(registry).unwrap();
    for s in [&mut expected, &mut app.editor.session] {
        s.apply(CoreAction::new(
            "canvas.new",
            serde_json::json!({"width":8,"height":8}),
        ))
        .unwrap();
        s.apply(CoreAction::new(
            "brush.set",
            serde_json::json!({"color":"#ff0000","background_color":"#0000ff","opacity":1}),
        ))
        .unwrap();
    }
    app.editor.sync_from_session();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for t in 0..3 {
        frame(&mut app, &ctx, size, vec![], t as f64);
    }
    app.editor.area_mode = 2;
    app.editor.area_reverse = true;
    app.editor.area_transparent = true;
    app.editor.area_spread = "reflect";
    let start = pos2(1.5, 1.5);
    let end = pos2(6.5, 2.5);
    let screen = |p: Pos2| app.editor.image_rect.min + p.to_vec2() * app.editor.zoom;
    let (a, b) = (screen(start), screen(end));
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::SHIFT,
    };
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(3.),
            modifiers: Modifiers::SHIFT,
            events: vec![button(a, true), Event::PointerMoved(b)],
            focused: true,
            ..Default::default()
        },
        |ctx| app.frame_ui(ctx),
    );
    let (_, snapped) = app.editor.area_drag.unwrap();
    assert!((snapped.y - start.y).abs() < 0.0001);
    assert!((snapped.distance(start) - end.distance(start)).abs() < 0.0001);
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            time: Some(4.),
            modifiers: Modifiers::SHIFT,
            events: vec![button(b, false)],
            focused: true,
            ..Default::default()
        },
        |ctx| app.frame_ui(ctx),
    );
    expected.apply(CoreAction::new("layer.gradient",serde_json::json!({"x0":start.x,"y0":start.y,"x1":snapped.x,"y1":snapped.y,"start_color":"#ff0000","end_color":"#0000ff00","spread":"reflect","reverse":true}))).unwrap();
    assert_eq!(
        app.editor.session.doc.layers[0].pixels,
        expected.doc.layers[0].pixels
    );
    assert!(app.editor.area_drag.is_none());
    assert!(app
        .editor
        .core("native.checkerboard", serde_json::json!({"cell":2})));
    let original = app.editor.session.doc.layers[0].pixels.clone();
    app.editor.area_mode = 1;
    app.editor.area_contiguous = false;
    let p = app.editor.image_rect.min + vec2(0.5, 0.5) * app.editor.zoom;
    frame(
        &mut app,
        &ctx,
        size,
        vec![button(p, true), button(p, false)],
        5.,
    );
    for (actual, old) in app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .zip(&original)
    {
        assert_eq!(
            *actual,
            if *old == original[0] {
                0xffff0000
            } else {
                *old
            }
        );
    }
    app.editor
        .core("canvas.new", serde_json::json!({"width":8,"height":8}));
    app.editor.sync_from_session();
    app.editor.area_expand = -1;
    app.editor.area_feather = 1;
    frame(&mut app, &ctx, size, vec![], 6.);
    let p = app.editor.image_rect.center();
    frame(
        &mut app,
        &ctx,
        size,
        vec![button(p, true), button(p, false)],
        7.,
    );
    expected
        .apply(CoreAction::new(
            "canvas.new",
            serde_json::json!({"width":8,"height":8}),
        ))
        .unwrap();
    expected.apply(CoreAction::new("layer.fill",serde_json::json!({"x":4,"y":4,"color":"#ff0000","contiguous":false,"expand":-1,"feather":1}))).unwrap();
    assert_eq!(
        app.editor.session.doc.layers[0].pixels,
        expected.doc.layers[0].pixels
    );
    assert_eq!(app.editor.session.doc.layers[0].pixels[0], 0x40ff0000);
}

#[test]
fn command_palette_stays_inside_the_window_at_both_densities() {
    let mut app = studio();
    let ctx = Context::default();
    for size in [vec2(960., 680.), vec2(1360., 920.)] {
        for mode in [
            crate::ui::palette::Mode::Commands,
            crate::ui::palette::Mode::Bindings,
        ] {
            app.editor.open_palette(mode);
            for n in 0..4 {
                let _ = frame(&mut app, &ctx, size, vec![], 20. + n as f64);
            }
            let bounds = app.editor.palette_bounds;
            assert!(
                bounds.is_finite() && bounds.width() < size.x - 32.,
                "{bounds:?}"
            );
            assert!(
                Rect::from_min_size(Pos2::ZERO, size).contains_rect(bounds),
                "{bounds:?}"
            );
        }
    }
}

#[test]
fn installing_a_panel_adds_it_once_without_reopening_closed_known_panels() {
    let mut registry = crate::plugins::Registry::bundled();
    registry.panels.insert(
        "test.panel".into(),
        crate::plugins::PanelDefinition {
            id: "test.panel".into(),
            title: "Test".into(),
            builtin: None,
            buttons: vec![],
        },
    );
    let mut dock = default_dock(&Appearance::default(), 1360.);
    let location = dock.find_tab(&Panel::Color).unwrap();
    dock.remove_tab(location);
    let known = ["color", "brush", "navigator", "layers"]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    reconcile_plugins(&mut dock, &registry, Some(&known));
    let panel = Panel::Plugin(crate::plugins::panel_key("test.panel"));
    assert!(dock.find_tab(&Panel::Color).is_none());
    assert!(dock.find_tab(&panel).is_some());
    let (mut dock, known) = decode_workspace(&encode_workspace(&dock, &registry).unwrap()).unwrap();
    let location = dock.find_tab(&panel).unwrap();
    dock.remove_tab(location);
    reconcile_plugins(&mut dock, &registry, Some(&known));
    assert!(dock.find_tab(&panel).is_none());
}
#[test]
fn settings_center_search_json_preview_cancel_and_shortcut_entry_share_one_window() {
    let mut app = studio();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    app.editor.session.parameters.stabilizer = 0.8;
    app.editor.sync_from_session();
    app.editor.actions.push(Action::ShowShortcuts);
    app.run_actions(&ctx);
    assert!(app.settings_open);
    assert!(app.editor.palette.is_none());
    assert_eq!(app.preferences.category, preferences::Category::Keys);
    app.preferences.query = "pressure_gamma".into();
    app.preferences.category = preferences::Category::All;
    for i in 0..3 {
        let _ = frame(&mut app, &ctx, size, vec![], i as f64);
    }
    assert!(app.editor.blocked);
    let original = app.editor.settings.encode().unwrap();
    let changed = app.editor.settings.keys.changed(0, vec![]).unwrap();
    app.editor.settings.keys = changed;
    app.editor.settings.ui.panel_padding = 24.;
    let _ = frame(
        &mut app,
        &ctx,
        size,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        4.,
    );
    assert!(!app.settings_open);
    assert_eq!(app.editor.session.parameters.stabilizer, 0.8);
    assert_eq!(app.editor.settings.encode().unwrap(), original);
    app.preferences.query.clear();
    for (i, size) in [vec2(580., 640.), vec2(960., 680.), vec2(1360., 920.)]
        .into_iter()
        .enumerate()
    {
        app.open_settings(None);
        for n in 0..3 {
            let output = frame(&mut app, &ctx, size, vec![], (10 + i * 3 + n) as f64);
            assert!(output.shapes.iter().all(|s| s.clip_rect.is_finite()));
            if n > 0 {
                let bounds = app.preferences.bounds.unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(bounds),
                    "settings {bounds:?} outside {size:?}"
                );
            }
        }
    }
}
