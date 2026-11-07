//! Brush controls preserve command/preset choices and expose stroke application.
use super::*;
use crate::application::OpacityMode;
use crate::desktop::tests::studio;
use serde_json::json;

fn fixture() -> Studio {
    let mut app = studio();
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    assert!(app
        .editor
        .core("canvas.new", json!({"width":12,"height":8})));
    app.editor.sync_from_session();
    app
}
fn frame(app: &mut Studio, ctx: &Context, width: f32, events: Vec<Event>, time: f64) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1500.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.brush_ui(ui));
        },
    )
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
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
fn click(pos: Pos2) -> Vec<Event> {
    let mut events = vec![Event::PointerMoved(pos)];
    events.extend(
        [true, false]
            .into_iter()
            .map(|pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }),
    );
    events
}

#[test]
fn brush_opacity_real_toggle_and_gui_parameter_sync_preserve_independent_flow() {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut app = fixture();
    let ctx = context(&app);
    assert!(app
        .editor
        .core("brush.set", json!({"flow":0.25,"opacity_mode":"stroke"})));
    app.editor.sync_from_session();
    app.editor.radius += 1.;
    app.editor.push_gui_parameters();
    assert_eq!(app.editor.session.parameters.flow, 0.25);
    assert_eq!(
        app.editor.session.parameters.opacity_mode,
        OpacityMode::Stroke
    );
    for t in 0..3 {
        frame(&mut app, &ctx, 300., vec![], t as f64);
    }
    let before = app.editor.session.doc.layers[0].pixels.clone();
    let history = app.editor.session.doc.history_bytes();
    let position = ctx
        .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-stroke-opacity-control")))
        .unwrap()
        .center();
    frame(&mut app, &ctx, 300., click(position), 3.);
    assert_eq!(app.editor.session.parameters.opacity_mode, OpacityMode::Dab);
    assert_eq!(app.editor.session.parameters.flow, 0.25);
    app.editor.push_gui_parameters();
    assert_eq!(app.editor.session.parameters.opacity_mode, OpacityMode::Dab);
    assert_eq!(app.editor.session.parameters.flow, 0.25);
    assert_eq!(app.editor.session.doc.layers[0].pixels, before);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
}

#[test]
fn brush_opacity_preset_cards_dispatch_glazing_and_restore_existing_marker_behavior() {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut app = fixture();
    let ctx = context(&app);
    let before = app.editor.session.doc.layers[0].pixels.clone();
    for t in 0..3 {
        frame(&mut app, &ctx, 300., vec![], t as f64);
    }
    for (time, title, id, mode, flow) in [
        (
            3.,
            "Glazing Marker",
            "glaze_marker",
            OpacityMode::Stroke,
            0.25,
        ),
        (5., "Marker", "marker", OpacityMode::Dab, 1.),
    ] {
        let output = frame(&mut app, &ctx, 300., vec![], time);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, title))
            .unwrap()
            .center();
        frame(&mut app, &ctx, 300., click(position), time + 0.1);
        assert_eq!(app.editor.session.selected_tool, id);
        assert_eq!(app.editor.session.parameters.opacity_mode, mode);
        assert_eq!(app.editor.session.parameters.flow, flow);
        app.editor.push_gui_parameters();
        assert_eq!(app.editor.session.parameters.opacity_mode, mode);
        assert_eq!(app.editor.session.parameters.flow, flow);
    }
    assert_eq!(app.editor.session.doc.layers[0].pixels, before);
}

#[test]
fn brush_opacity_controls_and_help_fit_narrow_wide_panels_in_four_languages() {
    crate::i18n::load_test_catalogs();
    let mut app = fixture();
    let ctx = context(&app);
    for language in crate::i18n::Language::ALL {
        crate::i18n::set_language(language);
        for mode in [OpacityMode::Dab, OpacityMode::Stroke] {
            assert!(app
                .editor
                .core("brush.set", json!({"opacity_mode":mode,"flow":0.25})));
            for width in [244., 680.] {
                let before = app.editor.session.state();
                let mut output = frame(&mut app, &ctx, width, vec![], 0.);
                for t in 1..3 {
                    output = frame(&mut app, &ctx, width, vec![], t as f64);
                }
                let note = if mode == OpacityMode::Stroke {
                    "Keep each stroke within Opacity; lift the pen to build another coat."
                } else {
                    "Paint builds up as you pass over the same area."
                };
                for title in ["Flow", "Limit opacity per stroke", note] {
                    let text = localize(title);
                    let bounds = output
                        .shapes
                        .iter()
                        .find_map(|shape| label(&shape.shape, &text))
                        .unwrap_or_else(|| panic!("{language:?} {title}"));
                    assert!(
                        bounds.right() <= width + 0.5,
                        "{language:?} {width} {title}: {bounds:?}"
                    );
                }
                let rect = ctx
                    .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-stroke-opacity-control")))
                    .unwrap();
                assert!(rect.right() <= width + 0.5, "{language:?}: {rect:?}");
                assert_eq!(app.editor.session.state(), before);
            }
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}

#[test]
fn brush_opacity_area_tools_and_temporary_navigation_cannot_change_disabled_stroke_controls() {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut app = fixture();
    assert!(app
        .editor
        .core("brush.set", json!({"opacity_mode":"stroke","flow":0.25})));
    app.editor.sync_from_session();
    let ctx = context(&app);
    for (step, area) in [1, 2, 0].into_iter().enumerate() {
        if area == 0 {
            app.editor.select_tool("hand");
        } else {
            app.editor.area_mode = area;
        }
        let base = step as f64 * 10.;
        for offset in 0..3 {
            frame(&mut app, &ctx, 300., vec![], base + offset as f64 * 0.1);
        }
        let before = app.editor.session.state();
        let mode = ctx
            .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-stroke-opacity-control")))
            .unwrap()
            .center();
        frame(&mut app, &ctx, 300., click(mode), base + 0.3);
        let flow = ctx
            .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-flow-control")))
            .unwrap();
        let number = pos2(flow.right() - 29., flow.center().y);
        frame(&mut app, &ctx, 300., click(number), base + 0.4);
        frame(&mut app, &ctx, 300., click(number), base + 0.5);
        frame(
            &mut app,
            &ctx,
            300.,
            vec![
                Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                },
                Event::Text("80".into()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            base + 0.6,
        );
        assert_eq!(app.editor.session.state(), before);
        assert_eq!(app.editor.session.parameters.flow, 0.25);
        assert_eq!(
            app.editor.session.parameters.opacity_mode,
            OpacityMode::Stroke
        );
    }
}
