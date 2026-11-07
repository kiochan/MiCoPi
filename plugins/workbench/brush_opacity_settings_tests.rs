//! Actual searchable setting controls preview, cancel and persist brush options.
use super::*;
use crate::{
    application::OpacityMode,
    desktop::tests::{frame, studio},
};
use serde_json::json;

fn bounds(shape: &Shape, label: &str) -> Option<Rect> {
    match shape {
        Shape::Text(text) if text.galley.text() == label => {
            Some(Rect::from_min_size(text.pos, text.galley.size()))
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| bounds(shape, label)),
        _ => None,
    }
}
fn click(app: &mut Studio, ctx: &Context, position: Pos2, time: f64) {
    for (index, pressed) in [true, false].into_iter().enumerate() {
        frame(
            app,
            ctx,
            vec2(1360., 920.),
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            time + index as f64 * 0.01,
        );
    }
}
fn locate(app: &mut Studio, ctx: &Context, title: &str, time: f64) -> Pos2 {
    let output = frame(app, ctx, vec2(1360., 920.), vec![], time);
    output
        .shapes
        .iter()
        .find_map(|shape| bounds(&shape.shape, title))
        .unwrap_or_else(|| panic!("Missing settings label {title}"))
        .center()
}

#[test]
fn brush_opacity_searchable_enum_and_flow_preview_cancel_save_preserve_pixels_and_unknown_settings()
{
    let mut app = studio();
    let ctx = Context::default();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-opacity-settings-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    app.editor.settings = Settings::decode(
        r#"{"version":1,"extension":{"keep":42}}"#,
        folder.join("settings.json"),
    )
    .unwrap();
    for time in 0..3 {
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], time as f64);
    }
    let pixels = app.editor.session.doc.layers[0].pixels.clone();
    let composition = app.editor.session.doc.composition();
    let history = app.editor.session.doc.history_bytes();
    for pass in 0..2 {
        let base = 5. + pass as f64 * 10.;
        app.open_settings(Some(Category::Painting));
        app.preferences.query = "painting.opacity_mode".into();
        for offset in 0..3 {
            frame(
                &mut app,
                &ctx,
                vec2(1360., 920.),
                vec![],
                base + offset as f64 * 0.1,
            );
        }
        let position = locate(&mut app, &ctx, "Build Up", base + 0.3);
        click(&mut app, &ctx, position, base + 0.4);
        let position = locate(&mut app, &ctx, "Per Stroke", base + 0.5);
        click(&mut app, &ctx, position, base + 0.6);
        assert_eq!(
            app.editor.settings.painting.opacity_mode,
            OpacityMode::Stroke
        );
        assert_eq!(
            app.editor.session.parameters.opacity_mode,
            OpacityMode::Stroke
        );
        app.preferences.query = "painting.flow".into();
        for offset in 0..3 {
            frame(
                &mut app,
                &ctx,
                vec2(1360., 920.),
                vec![],
                base + 0.7 + offset as f64 * 0.1,
            );
        }
        let number = ctx
            .data_mut(|data| data.get_temp::<Rect>(Id::new(("setting-number", "painting.flow"))))
            .unwrap()
            .center();
        click(&mut app, &ctx, number, base + 1.0);
        click(&mut app, &ctx, number, base + 1.1);
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![
                Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                },
                Event::Text("0.25".into()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            base + 1.2,
        );
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], base + 1.3);
        assert_eq!(app.editor.settings.painting.flow, 0.25);
        assert_eq!(app.editor.session.parameters.flow, 0.25);
        if pass == 1 {
            let save = locate(&mut app, &ctx, "Save Settings", base + 1.4);
            click(&mut app, &ctx, save, base + 1.5);
            let stored = Settings::load(app.editor.settings.path.clone()).unwrap();
            assert_eq!(stored.painting.flow, 0.25);
            assert_eq!(stored.painting.opacity_mode, OpacityMode::Stroke);
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&stored.encode().unwrap()).unwrap()
                    ["extension"]["keep"],
                42
            );
        }
        app.cancel_settings();
        assert_eq!(
            app.editor.session.parameters.flow,
            if pass == 0 { 1. } else { 0.25 }
        );
        assert_eq!(
            app.editor.session.parameters.opacity_mode,
            if pass == 0 {
                OpacityMode::Dab
            } else {
                OpacityMode::Stroke
            }
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.composition(), composition);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
    }
}

#[test]
fn brush_opacity_settings_search_translations_show_enum_choices_without_mutating_state() {
    crate::i18n::load_test_catalogs();
    for language in crate::i18n::Language::ALL {
        for width in [420., 1360.] {
            let mut app = studio();
            app.editor.settings.ui.language = language;
            let ctx = Context::default();
            theme::fonts(&ctx);
            theme::apply(&ctx, &app.editor.settings.ui);
            crate::i18n::set_language(language);
            app.open_settings(Some(Category::Painting));
            app.preferences.query = crate::i18n::text_in(language, "Opacity Mode").into_owned();
            let before = app.editor.session.state();
            let mut output = None;
            for time in 0..3 {
                output = Some(ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 920.))),
                        time: Some(time as f64),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| app.settings_window(ctx),
                ));
            }
            let output = output.unwrap();
            assert_eq!(app.preferences.result_count, 1);
            for source in ["Opacity Mode", "Build Up"] {
                let title = crate::i18n::text_in(language, source);
                let rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| bounds(&shape.shape, &title))
                    .unwrap_or_else(|| panic!("{language:?}: {source}"));
                assert!(
                    rect.right() <= width + 0.5,
                    "{language:?} {width}: {rect:?}"
                );
            }
            assert_eq!(app.editor.session.state(), before);
            app.cancel_settings();
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
