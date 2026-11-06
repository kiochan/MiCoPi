use super::*;
use crate::desktop::tests::studio;
use serde_json::json;

fn text(shape: &Shape, prefix: &str) -> Option<String> {
    match shape {
        Shape::Text(text) if text.galley.text().starts_with(prefix) => {
            Some(text.galley.text().to_owned())
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| text(shape, prefix)),
        _ => None,
    }
}
fn frame(app: &mut Studio, ctx: &Context, width: f32, time: f64) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    if !ctx.data_mut(|data| {
        data.get_temp::<bool>(Id::new("mask-footer-fonts"))
            .unwrap_or(false)
    }) {
        theme::fonts(ctx);
        theme::apply(ctx, &app.editor.settings.ui);
        ctx.data_mut(|data| data.insert_temp(Id::new("mask-footer-fonts"), true));
    }
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 100.))),
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let strip = Rect::from_min_size(
                    ui.next_widget_position(),
                    vec2(
                        ui.available_width(),
                        app.editor.settings.ui.control_height + 2.,
                    ),
                );
                let colors = theme::Colors::new(&app.editor.settings.ui);
                app.editor.zoom_strip(ui, strip, &colors);
            });
        },
    )
}

#[test]
fn mask_footer_prefix_is_localized_and_preserves_ready_error_pressure_and_selection_status() {
    for language in crate::i18n::Language::ALL {
        for (selection, pen, error) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (true, false, true),
        ] {
            let mut app = studio();
            app.editor.session = crate::application::Session::new(
                crate::plugins::Registry::load(
                    &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
                )
                .unwrap(),
            )
            .unwrap();
            app.editor.settings.ui.language = language;
            assert!(app
                .editor
                .core("canvas.new", json!({"width":12,"height":8})));
            assert!(
                app.editor
                    .core("layer.fill_color", json!({"color":"#5477CC"})),
                "{}",
                app.editor.status
            );
            assert!(app.editor.core(
                "mask.create",
                json!({"owner":{"kind":"layer","id":1},"source":"hide_all","select":true})
            ));
            if selection {
                assert!(app.editor.core(
                    "selection.shape",
                    json!({"kind":"rectangle","x":1,"y":1,"width":6,"height":4})
                ));
            }
            app.editor.status = if error {
                "raster_mask: Injected footer error"
            } else {
                "Ready to Paint"
            }
            .into();
            app.editor.pen_active = pen;
            app.editor.pressure = 0.42;
            let state = app.editor.session.state();
            let history = app.editor.session.doc.history_bytes();
            let rgba = app.editor.session.doc.layers[0].pixels.clone();
            let surface = app.editor.session.doc.composite.clone();
            let events = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            for width in [640., 1200.] {
                let ctx = Context::default();
                let _ = frame(&mut app, &ctx, width, 0.);
                let output = frame(&mut app, &ctx, width, 1.);
                let prefix = format!("{} · ", localize("Mask"));
                let status = output
                    .shapes
                    .iter()
                    .find_map(|shape| text(&shape.shape, &prefix))
                    .expect("Mask target must be visible in the canvas footer");
                let original = if pen {
                    "Windows Ink · Pressure 42%"
                } else {
                    app.editor.status.as_str()
                };
                assert!(
                    status.contains(localize(original).as_ref()),
                    "{language:?}/{width}: {status}"
                );
                if selection {
                    assert!(
                        status.ends_with(localize("Selection").as_ref()),
                        "{language:?}/{width}: {status}"
                    );
                }
                assert_eq!(app.editor.session.state(), state);
                assert_eq!(app.editor.session.doc.history_bytes(), history);
                assert_eq!(app.editor.session.doc.layers[0].pixels, rgba);
                assert_eq!(app.editor.session.doc.composite, surface);
                assert!(matches!(
                    app.editor.session.doc.mask_plane(1).unwrap(),
                    crate::engine::raster_masks::PlaneView::Uniform(0)
                ));
                assert_eq!(
                    app.editor
                        .session
                        .replay_journal(false)
                        .unwrap()
                        .entries
                        .len(),
                    events
                );
            }
            assert!(app
                .editor
                .core("mask.target", json!({"target":{"kind":"layer_pixels"}})));
            let ctx = Context::default();
            let _ = frame(&mut app, &ctx, 1200., 0.);
            let output = frame(&mut app, &ctx, 1200., 1.);
            let prefix = format!("{} · ", localize("Mask"));
            assert!(output
                .shapes
                .iter()
                .all(|shape| text(&shape.shape, &prefix).is_none()));
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
