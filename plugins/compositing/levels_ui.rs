//! Shared Levels draft controls. This editor never dispatches document actions.
use super::*;
use crate::engine::levels::Levels;
fn text(message: &str) -> RichText {
    tr_for(Some("compositing"), message)
}

pub(crate) fn estimated_height(a: &crate::configuration::Appearance) -> f32 {
    5. * (a.control_height + 1.5 * a.font_size + 3. * a.item_spacing)
        + 4. * a.control_height
        + 4. * a.section_spacing
        + 9. * a.font_size
        + 4. * a.control_height
        + 3. * a.font_size
        + 4. * a.item_spacing
}
fn ramp(ui: &mut Ui, black: u8, white: u8) {
    let height = (ui.spacing().interact_size.y * 0.35).max(4.);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    for step in 0..32 {
        let value = (f32::from(black) + (f32::from(white) - f32::from(black)) * step as f32 / 31.)
            .round() as u8;
        let segment = Rect::from_min_max(
            pos2(rect.left() + rect.width() * step as f32 / 32., rect.top()),
            pos2(
                rect.left() + rect.width() * (step + 1) as f32 / 32.,
                rect.bottom(),
            ),
        );
        ui.painter()
            .rect_filled(segment, 0, Color32::from_gray(value));
    }
}
fn byte(ui: &mut Ui, title: &str, value: &mut u8) {
    ui.label(text(title));
    ui.add_sized(
        [ui.available_width(), ui.spacing().interact_size.y],
        DragValue::new(value).range(0..=255),
    );
}
#[cfg(test)]
pub(crate) fn edit(ui: &mut Ui, id: Id, value: &mut Levels) -> bool {
    edit_with_histogram(ui, id, value, None)
}
pub(crate) fn edit_with_histogram(
    ui: &mut Ui,
    id: Id,
    value: &mut Levels,
    histogram: Option<&super::histogram_ui::Snapshot>,
) -> bool {
    let before = *value;
    ui.push_id(id, |ui| {
        ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
        let channel_key = id.with("levels-channel");
        let mut channel = ui
            .data_mut(|d| *d.get_temp_mut_or_default::<usize>(channel_key))
            .min(3);
        ui.label(text("Channel"));
        ComboBox::from_id_salt("levels-channel")
            .width(ui.available_width())
            .truncate()
            .selected_text(text(["Master RGB", "Red", "Green", "Blue"][channel]))
            .show_ui(ui, |ui| {
                for (index, title) in ["Master RGB", "Red", "Green", "Blue"]
                    .into_iter()
                    .enumerate()
                {
                    let response = ui.selectable_value(&mut channel, index, text(title));
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(id.with(("levels-channel-option", index)), response.rect)
                    });
                    #[cfg(not(test))]
                    let _ = response;
                }
            });
        ui.data_mut(|d| d.insert_temp(channel_key, channel));
        if let Some(snapshot) = histogram {
            super::histogram_ui::band(ui, snapshot, [3, 0, 1, 2][channel]);
        }

        let mut range = match channel {
            0 => value.master(),
            1 => value.channels.red,
            2 => value.channels.green,
            _ => value.channels.blue,
        };
        ui.strong(text("Input Levels"));
        ramp(ui, 0, 255);
        byte(ui, "Input Black", &mut range.input_black);
        ui.label(text("Midtone Gamma"));
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            DragValue::new(&mut range.gamma)
                .range(0.1..=10.)
                .speed(0.01)
                .max_decimals(2),
        );
        byte(ui, "Input White", &mut range.input_white);
        ui.add_space(ui.spacing().item_spacing.y);
        ui.strong(text("Output Levels"));
        ramp(ui, range.output_black, range.output_white);
        byte(ui, "Output Black", &mut range.output_black);
        byte(ui, "Output White", &mut range.output_white);
        if range.input_black >= range.input_white {
            ui.add(
                Label::new(
                    text("Input white must exceed input black.").color(ui.visuals().warn_fg_color),
                )
                .wrap(),
            );
        }
        match channel {
            0 => value.set_master(range),
            1 => value.channels.red = range,
            2 => value.channels.green = range,
            _ => value.channels.blue = range,
        }
        let reset_channel = ui.add_enabled(
            range != crate::engine::levels::LevelsRange::default(),
            Button::new(text("Reset Channel")),
        );
        #[cfg(test)]
        ui.data_mut(|d| d.insert_temp(id.with("levels-reset-channel"), reset_channel.rect));
        if reset_channel.clicked() {
            let identity = crate::engine::levels::LevelsRange::default();
            match channel {
                0 => value.set_master(identity),
                1 => value.channels.red = identity,
                2 => value.channels.green = identity,
                _ => value.channels.blue = identity,
            }
        }
        let reset = ui.add_enabled(
            *value != Levels::default(),
            Button::new(text("Reset Levels")),
        );
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(id.with("levels-reset"), reset.rect));
        if reset.clicked() {
            *value = Levels::default();
        }
    });
    *value != before
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_reset_preserves_master_and_other_channels_then_all_reset_clears_intent() {
        let ctx = Context::default();
        let id = Id::new("levels-test");
        ctx.data_mut(|d| d.insert_temp(id.with("levels-channel"), 1usize));
        let mut value = Levels {
            gamma: 2.,
            ..Default::default()
        };
        value.channels.red.gamma = 3.;
        value.channels.green.gamma = 4.;
        frame(&ctx, &mut value, 240., vec![]);
        frame(&ctx, &mut value, 240., vec![]);
        for key in ["levels-reset-channel", "levels-reset"] {
            let point = ctx
                .data_mut(|d| d.get_temp::<Rect>(id.with(key)))
                .unwrap()
                .center();
            for pressed in [true, false] {
                frame(
                    &ctx,
                    &mut value,
                    240.,
                    vec![
                        Event::PointerMoved(point),
                        Event::PointerButton {
                            pos: point,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
            if key == "levels-reset-channel" {
                assert_eq!(value.gamma, 2.);
                assert_eq!(value.channels.red.gamma, 1.);
                assert_eq!(value.channels.green.gamma, 4.);
            } else {
                assert_eq!(value, Levels::default());
            }
        }
    }
    fn frame(ctx: &Context, p: &mut Levels, width: f32, events: Vec<Event>) -> FullOutput {
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1200.))),
                events,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    edit(ui, Id::new("levels-test"), p);
                });
            },
        )
    }
    #[test]
    fn levels_reset_changes_only_the_local_draft_after_an_actual_click() {
        let ctx = Context::default();
        let mut value = Levels {
            gamma: 2.,
            output_black: 33,
            ..Default::default()
        };
        frame(&ctx, &mut value, 240., vec![]);
        frame(&ctx, &mut value, 240., vec![]);
        let rect = ctx
            .data_mut(|data| data.get_temp::<Rect>(Id::new("levels-test").with("levels-reset")))
            .unwrap();
        assert_eq!(value.gamma, 2.);
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut value,
                240.,
                vec![
                    Event::PointerMoved(rect.center()),
                    Event::PointerButton {
                        pos: rect.center(),
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(value, Levels::default());
    }
    #[test]
    fn levels_controls_fit_narrow_and_wide_four_language_layouts() {
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
                    let mut value = Levels::default();
                    for _ in 0..3 {
                        let _ = ctx.run(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(width, 1200.),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                CentralPanel::default().show(ctx, |ui| {
                                    let left = ui.max_rect().left();
                                    let right = ui.max_rect().right();
                                    // CentralPanel sets its minimum to the whole
                                    // viewport. Measure the editor child scope.
                                    let response = ui.scope(|ui| {
                                        let snapshot = super::super::histogram_ui::test_snapshot();
                                        edit_with_histogram(
                                            ui,
                                            Id::new("levels-test"),
                                            &mut value,
                                            Some(&snapshot),
                                        )
                                    });
                                    let rect = response.response.rect;
                                    assert!(
                                        rect.left() >= left - 1. && rect.right() <= right + 1.,
                                        "{language:?}/{width}/large={large}: {rect:?}"
                                    );
                                    assert!(
                                        rect.height() <= estimated_height(&a),
                                        "{language:?}/{width}/large={large}: height {} vs {}",
                                        rect.height(),
                                        estimated_height(&a)
                                    );
                                });
                            },
                        );
                    }
                    assert_eq!(value, Levels::default());
                }
            }
        }
    }
}
