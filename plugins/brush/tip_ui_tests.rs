//! Real tip controls and command bindings share the parameter action contract.
use super::*;
use crate::application::TipShape;
use crate::desktop::tests::studio;
use serde_json::json;

fn fixture() -> Studio {
    let mut app = studio();
    app.editor.settings.ui.language = crate::i18n::Language::English;
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    app.editor.register_commands();
    assert!(app
        .editor
        .core("canvas.new", json!({"width":12,"height":8})));
    assert!(app.editor.core(
        "brush.set",
        json!({"radius":3,"flow":0.25,"opacity_mode":"stroke"})
    ));
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1600.))),
            events,
            modifiers,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.brush_tip_ui(ui));
        },
    )
}
fn warm(app: &mut Studio, ctx: &Context, width: f32, time: f64) -> FullOutput {
    frame(app, ctx, width, vec![], time);
    frame(app, ctx, width, vec![], time + 0.01);
    frame(app, ctx, width, vec![], time + 0.02)
}
fn control(ctx: &Context, id: impl std::hash::Hash) -> (Rect, bool) {
    ctx.data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new(id)))
        .unwrap()
}
fn click(app: &mut Studio, ctx: &Context, width: f32, p: Pos2, time: f64) {
    for (n, pressed) in [true, false].into_iter().enumerate() {
        frame(
            app,
            ctx,
            width,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            time + n as f64 / 100.,
        );
    }
    warm(app, ctx, width, time + 0.1);
}
fn number(app: &mut Studio, ctx: &Context, id: &str, text: &str, time: f64) {
    let rect = control(ctx, id).0;
    let point = pos2(
        rect.right() - 29.,
        rect.top() + app.editor.settings.ui.control_height * 0.5,
    );
    click(app, ctx, 360., point, time);
    click(app, ctx, 360., point, time + 0.2);
    let modifiers = Modifiers {
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
                modifiers,
            },
            Event::Text(text.into()),
        ],
        time + 0.4,
    );
    frame(
        app,
        ctx,
        360.,
        vec![Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        time + 0.5,
    );
}
fn label(shape: &Shape, title: &str) -> Option<Rect> {
    match shape {
        Shape::Text(t) if t.galley.text() == title => {
            Some(Rect::from_min_size(t.pos, t.galley.size()))
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, title)),
        _ => None,
    }
}

#[test]
fn brush_tip_actual_shape_roundness_angle_controls_publish_shared_parameters_without_pixel_edits() {
    let mut app = fixture();
    let ctx = context(&app);
    let pixels = app.editor.session.doc.layers[0].pixels.clone();
    let history = app.editor.session.doc.history_bytes();
    warm(&mut app, &ctx, 360., 0.);
    click(
        &mut app,
        &ctx,
        360.,
        control(&ctx, ("brush-tip-shape", "Square")).0.center(),
        1.,
    );
    assert_eq!(app.editor.session.parameters.tip_shape, TipShape::Square);
    number(&mut app, &ctx, "brush-tip-roundness-control", "40", 2.);
    number(&mut app, &ctx, "brush-tip-angle-control", "-45", 3.);
    assert!((app.editor.session.parameters.tip_roundness - 0.4).abs() < 1e-6);
    assert_eq!(app.editor.session.parameters.tip_angle, -45.);
    app.editor.radius = 7.;
    app.editor.push_gui_parameters();
    assert_eq!(app.editor.session.parameters.tip_shape, TipShape::Square);
    assert_eq!(app.editor.session.parameters.tip_angle, -45.);
    assert_eq!(app.editor.session.parameters.flow, 0.25);
    assert_eq!(
        app.editor.session.parameters.opacity_mode,
        crate::application::OpacityMode::Stroke
    );
    assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
    click(
        &mut app,
        &ctx,
        360.,
        control(&ctx, ("brush-tip-shape", "Round")).0.center(),
        4.,
    );
    assert_eq!(app.editor.session.parameters.tip_shape, TipShape::Round);
    assert!((app.editor.session.parameters.tip_roundness - 0.4).abs() < 1e-6);
    assert_eq!(app.editor.session.parameters.tip_angle, -45.);
}

#[test]
fn brush_tip_area_and_navigation_controls_are_disabled_without_losing_settings() {
    let mut app = fixture();
    let ctx = context(&app);
    assert!(app.editor.core(
        "brush.set",
        json!({"tip_shape":"square","tip_roundness":0.5,"tip_angle":30})
    ));
    for (n, area) in [1, 2, 8, 0].into_iter().enumerate() {
        app.editor.area_mode = area;
        if area == 0 {
            app.editor.select_tool("hand");
        }
        warm(&mut app, &ctx, 240., n as f64 * 2.);
        let before = app.editor.session.state();
        let (rect, enabled) = control(&ctx, ("brush-tip-shape", "Round"));
        assert!(!enabled);
        click(&mut app, &ctx, 240., rect.center(), n as f64 * 2. + 0.3);
        assert!(!control(&ctx, "brush-tip-angle-control").1);
        assert!(!control(&ctx, "brush-tip-roundness-control").1);
        assert_eq!(app.editor.session.state(), before);
    }
}

#[test]
fn brush_tip_four_language_labels_and_controls_fit_narrow_wide_without_mutating_parameters() {
    crate::i18n::load_test_catalogs();
    let mut app = fixture();
    let ctx = context(&app);
    for (n, language) in crate::i18n::Language::ALL.into_iter().enumerate() {
        app.editor.settings.ui.language = language;
        crate::i18n::set_language(language);
        for (action, title) in [
            ("brush.angle.clockwise", "Rotate Brush Tip Clockwise"),
            (
                "brush.angle.counterclockwise",
                "Rotate Brush Tip Counterclockwise",
            ),
            (
                "brush.angle.clockwise_coarse",
                "Rotate Brush Tip Clockwise (15°)",
            ),
            (
                "brush.angle.counterclockwise_coarse",
                "Rotate Brush Tip Counterclockwise (15°)",
            ),
        ] {
            let mut palette = Palette::new(
                crate::ui::palette::Mode::Commands,
                &app.editor.settings.keys,
            );
            palette.query(
                crate::i18n::text(title).into_owned(),
                &app.editor.settings.keys,
            );
            assert!(
                palette.results.iter().any(|index| app
                    .editor
                    .settings
                    .keys
                    .command(*index)
                    .keywords
                    == action),
                "{language:?}: {action}"
            );
        }
        for (j, width) in [176., 244., 680.].into_iter().enumerate() {
            let before = app.editor.session.state();
            let output = warm(&mut app, &ctx, width, n as f64 * 10. + j as f64);
            for key in [("brush-tip-shape", "Round"), ("brush-tip-shape", "Square")] {
                let (rect, enabled) = control(&ctx, key);
                assert!(enabled);
                assert!(
                    rect.right() <= width + 0.5 && rect.left() >= 0.,
                    "{language:?}/{width}: {rect:?}"
                );
            }
            for key in ["brush-tip-angle-control", "brush-tip-roundness-control"] {
                assert!(
                    control(&ctx, key).0.right() <= width + 0.5,
                    "{language:?}/{width}: {key}"
                );
            }
            for source in ["Tip Shape", "Round", "Square", "Roundness", "Angle"] {
                let text = crate::i18n::text(source);
                let rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, &text))
                    .unwrap_or_else(|| panic!("{language:?}/{width}: {source}"));
                assert!(
                    rect.right() <= width + 0.5,
                    "{language:?}/{width}: {rect:?}"
                );
            }
            assert_eq!(app.editor.session.state(), before);
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}

#[test]
fn brush_tip_configured_angle_shortcut_dispatches_registered_macro_without_touching_pixels() {
    let mut app = fixture();
    let ctx = context(&app);
    app.editor.settings.keys = crate::ui::keymap::Keymap::decode(
        r#"{"keybindings":{"plugin.brush.angle.clockwise":"Ctrl+Alt+Comma"}}"#,
    )
    .unwrap();
    app.editor.register_commands();
    assert!(app.editor.core(
        "brush.set",
        json!({"tip_shape":"square","tip_roundness":0.5,"tip_angle":179})
    ));
    let pixels = app.editor.session.doc.layers[0].pixels.clone();
    let history = app.editor.session.doc.history_bytes();
    let revision = app.editor.session.revision;
    let modifiers = Modifiers {
        ctrl: true,
        alt: true,
        ..Default::default()
    };
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640., 480.))),
            focused: true,
            modifiers,
            events: vec![Event::Key {
                key: Key::Comma,
                physical_key: Some(Key::Comma),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        },
        |ctx| {
            app.editor.shortcuts(ctx);
            app.run_actions(ctx);
        },
    );
    assert!(!app.editor.command_form.is_some());
    assert!(
        app.editor.session.parameters.tip_angle == 180.
            || app.editor.session.parameters.tip_angle == -180.
    );
    assert_eq!(app.editor.session.revision, revision + 1);
    assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
}

#[test]
fn brush_tip_invalid_geometry_preserves_contacts_and_actual_shape_change_commits_once() {
    use crate::application::{Pointer, PointerPhase, ToolKind};
    fn samples(app: &mut Studio) -> Vec<serde_json::Value> {
        let journal = app.editor.session.replay_journal(false).unwrap();
        let events = journal.events(0, journal.entries.len()).unwrap();
        let mut out = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["source"]["kind"] == "pointer")
            .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
            .collect::<Vec<_>>();
        out.extend(
            serde_json::to_value(&app.editor.session.replay_samples)
                .unwrap()
                .as_array()
                .unwrap()
                .clone(),
        );
        out
    }
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = fixture();
            let ctx = context(&app);
            warm(&mut app, &ctx, 360., 0.);
            app.editor
                .session
                .pointer(Pointer {
                    phase: PointerPhase::Down,
                    x: 5.5,
                    y: 4.5,
                    pressure: 0.6,
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
            let before = app.editor.session.state();
            let pixels = app.editor.session.doc.layers[0].pixels.clone();
            let transcript = samples(&mut app);
            assert_eq!(transcript.len(), 1);
            assert!(!app.editor.core("brush.set", json!({"tip_roundness":0.2})));
            assert_eq!(app.editor.session.state(), before);
            assert_eq!(samples(&mut app), transcript);
            assert!(app.editor.session.stroke.active());
            assert_eq!(app.editor.input_owned, local);
            assert_eq!(app.editor.pen_active, local);
            click(
                &mut app,
                &ctx,
                360.,
                control(&ctx, ("brush-tip-shape", "Square")).0.center(),
                1.,
            );
            assert_eq!(app.editor.session.parameters.tip_shape, TipShape::Square);
            assert!(!app.editor.session.stroke.active());
            assert!(!app.editor.input_owned && !app.editor.pen_active);
            assert!(app.editor.panning.is_none());
            let after = samples(&mut app);
            assert_eq!(after.len(), 2);
            assert_eq!(after[0], transcript[0]);
            let mut up = transcript[0].clone();
            up["phase"] = json!("up");
            assert_eq!(after[1], up);
            assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        }
    }
}
