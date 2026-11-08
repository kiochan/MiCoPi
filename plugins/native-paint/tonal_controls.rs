//! Local percent controls; they never execute a document action themselves.
use super::*;
pub(super) fn edit(ui: &mut Ui, id: Id, value: &mut tonal_preview::Parameters) -> bool {
    let before = *value;
    ui.push_id(id, |ui| {
        for (title, number) in [
            ("Brightness", &mut value.brightness),
            ("Contrast", &mut value.contrast),
        ] {
            ui.add(Label::new(tr_for(Some("native_paint"), title)).wrap());
            ui.scope(|ui| {
                ui.spacing_mut().slider_width = ui.available_width().max(1.);
                ui.add(Slider::new(&mut *number, -100. ..=100.).show_value(false));
            });
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(number)
                    .range(-100. ..=100.)
                    .speed(1.)
                    .suffix("%"),
            );
        }
        let reset = ui.button(tr("Reset"));
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(Id::new("tonal-reset"), reset.rect));
        if reset.clicked() {
            *value = tonal_preview::Parameters::default();
        }
    });
    before != *value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tonal_slider_and_numeric_rows_fit_narrow_wide_and_large_metrics() {
        crate::i18n::load_test_catalogs();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for width in [176., 360., 680.] {
                for large in [false, true] {
                    let ctx = Context::default();
                    theme::fonts(&ctx);
                    let mut a = crate::configuration::Appearance::default();
                    if large {
                        a.font_size = 22.;
                        a.control_height = 48.;
                    }
                    theme::apply(&ctx, &a);
                    let original = tonal_preview::Parameters {
                        brightness: 25.,
                        contrast: 30.,
                    };
                    let mut value = original;
                    let _ = ctx.run(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1000.))),
                            ..Default::default()
                        },
                        |ctx| {
                            CentralPanel::default().show(ctx, |ui| {
                                let bounds = ui.max_rect();
                                let result =
                                    ui.scope(|ui| edit(ui, Id::new("tonal-layout"), &mut value));
                                assert!(
                                    result.response.rect.right() <= bounds.right() + 1.,
                                    "{language:?}/{width}/{large}: {:?}",
                                    result.response.rect
                                );
                            });
                        },
                    );
                    assert_eq!(value, original);
                }
            }
        }
    }
}
