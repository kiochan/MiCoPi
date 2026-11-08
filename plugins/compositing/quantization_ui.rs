//! Local integer controls shared by source-node and adjustment drafts.
use super::*;
use crate::engine::quantization::{Effect, Quantization};
pub(crate) fn estimated_height(a: &crate::configuration::Appearance) -> f32 {
    3. * a.control_height.max(1.5 * a.font_size)
        + 2. * a.font_size
        + 4. * a.item_spacing
        + a.section_spacing
}
pub(crate) fn edit(ui: &mut Ui, id: Id, value: &mut Quantization) -> bool {
    let before = *value;
    ui.push_id(id, |ui| {
        let (title, min, max, default) = match value.effect {
            Effect::Threshold => ("Threshold Level", 0, 255, 128),
            Effect::Posterize => ("Posterize Levels", 2, 256, 4),
        };
        ui.add(Label::new(tr_for(Some("compositing"), title)).wrap());
        let width = ui.available_width().max(1.);
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = width;
            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                Slider::new(&mut value.value, min..=max).show_value(false),
            );
        });
        ui.add_sized(
            [width, ui.spacing().interact_size.y],
            DragValue::new(&mut value.value).range(min..=max).speed(1.),
        );
        let reset = ui.button(tr_for(Some("compositing"), "Reset"));
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(id.with("reset"), reset.rect));
        if reset.clicked() {
            value.value = default;
        }
    });
    before != *value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quantization_controls_fit_four_languages_and_reset_only_local_parameters() {
        crate::i18n::load_test_catalogs();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for effect in [Effect::Threshold, Effect::Posterize] {
                for width in [176., 680.] {
                    for large in [false, true] {
                        let ctx = Context::default();
                        theme::fonts(&ctx);
                        let mut appearance = crate::configuration::Appearance::default();
                        if large {
                            appearance.font_size = 22.;
                            appearance.control_height = 48.;
                        }
                        theme::apply(&ctx, &appearance);
                        let original = Quantization { effect, value: 17 };
                        let mut value = original;
                        let mut frame = |events| {
                            let _ = ctx.run(
                                RawInput {
                                    screen_rect: Some(Rect::from_min_size(
                                        Pos2::ZERO,
                                        vec2(width, 900.),
                                    )),
                                    events,
                                    ..Default::default()
                                },
                                |ctx| {
                                    CentralPanel::default().show(ctx, |ui| {
                                        let bounds = ui.max_rect();
                                        let response = ui.scope(|ui| {
                                            edit(ui, Id::new("quantization-test"), &mut value)
                                        });
                                        assert!(
                                            response.response.rect.right() <= bounds.right() + 1.,
                                            "{language:?}/{width}/{large}"
                                        );
                                    });
                                },
                            );
                        };
                        frame(vec![]);
                        frame(vec![]);
                        let position = ctx
                            .data_mut(|d| {
                                d.get_temp::<Rect>(Id::new("quantization-test").with("reset"))
                                    .unwrap()
                            })
                            .center();
                        for pressed in [true, false] {
                            frame(vec![
                                Event::PointerMoved(position),
                                Event::PointerButton {
                                    pos: position,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Modifiers::NONE,
                                },
                            ]);
                        }
                        assert_eq!(
                            value,
                            Quantization {
                                effect,
                                value: if effect == Effect::Threshold { 128 } else { 4 }
                            }
                        );
                    }
                }
            }
        }
    }
}
