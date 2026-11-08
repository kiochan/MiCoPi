//! Actual symmetry controls retain configured metrics and shared actions.
use super::*;
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
            CentralPanel::default().show(ctx, |ui| app.editor.brush_symmetry_ui(ui));
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

#[test]
fn symmetry_modes_reset_and_four_language_layout_dispatch_shared_commands() {
    crate::i18n::load_test_catalogs();
    for language in [
        crate::i18n::Language::English,
        crate::i18n::Language::Chinese,
        crate::i18n::Language::Japanese,
        crate::i18n::Language::Korean,
    ] {
        for width in [280., 700.] {
            let mut app = fixture();
            app.editor.settings.ui.language = language;
            assert!(app
                .editor
                .core("brush.set", json!({"symmetry_x":0.25,"symmetry_y":0.75})));
            let ctx = context(&app);
            warm(&mut app, &ctx, width, 0.);
            let header = ctx
                .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-symmetry-header")))
                .unwrap();
            click(&mut app, &ctx, width, header.center(), 1.);
            for candidate in crate::application::SymmetryMode::ALL {
                let (rect, enabled) = control(&ctx, ("brush-symmetry-mode", candidate.title()));
                assert!(enabled);
                assert!(Rect::from_min_size(Pos2::ZERO, vec2(width, 1600.)).contains_rect(rect));
            }
            let before = app.editor.session.doc.layers[0].pixels.clone();
            let rect = control(&ctx, ("brush-symmetry-mode", "Both Axes")).0;
            click(&mut app, &ctx, width, rect.center(), 2.);
            assert_eq!(
                app.editor.session.parameters.symmetry,
                crate::application::SymmetryMode::Both
            );
            let rect = control(&ctx, "brush-symmetry-reset").0;
            click(&mut app, &ctx, width, rect.center(), 3.);
            assert_eq!(
                (
                    app.editor.session.parameters.symmetry_x,
                    app.editor.session.parameters.symmetry_y
                ),
                (0.5, 0.5)
            );
            assert_eq!(app.editor.session.doc.layers[0].pixels, before);
            assert!(app.editor.core("replay.verify", json!({})));
        }
    }
}

#[test]
fn disabled_symmetry_controls_leave_live_pressure_contact_and_parameters_intact() {
    use crate::application::{Pointer, PointerPhase};
    let mut app = fixture();
    let ctx = context(&app);
    let width = 360.;
    warm(&mut app, &ctx, width, 0.);
    let header = ctx
        .data_mut(|data| data.get_temp::<Rect>(Id::new("brush-symmetry-header")))
        .unwrap();
    click(&mut app, &ctx, width, header.center(), 1.);
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 2.5,
        y: 2.5,
        pressure: 0.4,
        time: 100,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    app.editor.session.pointer(pointer).unwrap();
    let parameters = app.editor.session.parameters.clone();
    let revision = app.editor.session.revision;
    warm(&mut app, &ctx, width, 2.);
    let (rect, enabled) = control(&ctx, ("brush-symmetry-mode", "Both Axes"));
    assert!(!enabled);
    click(&mut app, &ctx, width, rect.center(), 3.);
    assert!(app.editor.session.stroke.active());
    assert_eq!(app.editor.session.parameters, parameters);
    assert_eq!(app.editor.session.revision, revision);
    app.editor
        .session
        .pointer(Pointer {
            phase: PointerPhase::Up,
            time: 108,
            ..pointer
        })
        .unwrap();
    assert!(app.editor.core("replay.verify", json!({})));
}
