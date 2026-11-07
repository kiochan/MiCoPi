//! Shared local HSL drafts. No document access, native preparation or sampling.
use super::*;
use crate::engine::hsl::Hsl;
pub(crate) fn estimated_height(a: &crate::configuration::Appearance) -> f32 {
    let control = a.control_height.max(1.5 * a.font_size);
    3. * (control + 2. * a.font_size + 2. * a.item_spacing)
        + control
        + 2. * a.font_size
        + a.section_spacing
}
pub(crate) fn edit(ui: &mut Ui, id: Id, value: &mut Hsl) -> bool {
    let before = *value;
    ui.push_id(id, |ui| {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
        for (title, number, bound, unit) in [
            ("Hue Shift", &mut value.hue, 180., "°"),
            ("Saturation Adjustment", &mut value.saturation, 100., "%"),
            ("Lightness Adjustment", &mut value.lightness, 100., "%"),
        ] {
            ui.add(Label::new(tr_for(Some("compositing"), title)).wrap());
            ui.add_sized(
                [ui.available_width(), ui.spacing().interact_size.y],
                DragValue::new(number)
                    .range(-bound..=bound)
                    .speed(1.)
                    .suffix(unit),
            );
        }
        let reset = ui.button(tr_for(Some("compositing"), "Reset"));
        #[cfg(test)]
        ui.data_mut(|d| d.insert_temp(id.with("reset"), reset.rect));
        if reset.clicked() {
            *value = Hsl::default();
        }
    });
    before != *value
}
#[cfg(test)]
mod tests {
    use super::*;
    fn frame(ctx: &Context, value: &mut Hsl, events: Vec<Event>) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(240., 900.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| edit(ui, Id::new("hsl-draft"), value));
            },
        );
    }
    #[test]
    fn reset_click_changes_only_the_local_hsl_draft() {
        let ctx = Context::default();
        let original = Hsl {
            hue: 73.,
            saturation: -27.,
            lightness: 18.,
        };
        let mut value = original;
        frame(&ctx, &mut value, vec![]);
        frame(&ctx, &mut value, vec![]);
        let position = ctx
            .data_mut(|d| {
                d.get_temp::<Rect>(Id::new("hsl-draft").with("reset"))
                    .unwrap()
            })
            .center();
        assert_eq!(value, original);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut value,
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(value, Hsl::default());
    }
    #[test]
    fn local_hsl_controls_fit_four_languages_and_configured_metrics() {
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
                        a.title_height = 60.;
                        a.toolbar_height = 60.;
                        a.tab_height = 52.;
                    }
                    theme::apply(&ctx, &a);
                    let original = Hsl {
                        hue: 73.,
                        saturation: -27.,
                        lightness: 18.,
                    };
                    let mut value = original;
                    let _ = ctx.run(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1200.))),
                            ..Default::default()
                        },
                        |ctx| {
                            CentralPanel::default().show(ctx, |ui| {
                                let bounds = ui.max_rect();
                                let scope =
                                    ui.scope(|ui| edit(ui, Id::new("hsl-layout"), &mut value));
                                assert!(
                                    scope.response.rect.right() <= bounds.right() + 1.,
                                    "{language:?}/{width}/{large}"
                                );
                                assert!(
                                    scope.response.rect.height() <= estimated_height(&a),
                                    "{language:?}/{width}/{large}: {}",
                                    scope.response.rect.height()
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
