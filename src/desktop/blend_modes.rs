//! Shared artist-facing blend selection; wire order and capability checks stay host-owned.
use super::*;
use crate::engine::graph::BlendMode;

#[cfg(test)]
#[derive(Clone)]
struct MenuLayout {
    rows: Vec<(BlendMode, Rect, bool)>,
    clip: Rect,
    viewport_size: Vec2,
    paint_margin: f32,
    height_limit: f32,
}

pub(super) fn supported(document: &crate::engine::Document) -> u32 {
    BlendMode::ALL.into_iter().fold(1, |mask, mode| {
        if document
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.supports_blend_mode(mode as u32))
        {
            mask | 1 << mode as u32
        } else {
            mask
        }
    })
}
pub(super) fn allows(mask: u32, mode: BlendMode) -> bool {
    mode == BlendMode::Normal || mask & (1 << mode as u32) != 0
}
pub(super) fn combo(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    current: BlendMode,
    mixed: bool,
    width: f32,
    supported: u32,
) -> InnerResponse<Option<BlendMode>> {
    let id = Id::new(id);
    let width = width.min(ui.available_width()).max(1.);
    let row_height = ui.spacing().interact_size.y;
    let height = row_height * 9. + ui.spacing().item_spacing.y * 8.;
    let title = if mixed {
        "Mixed Values"
    } else {
        current.title()
    };
    let enabled = allows(supported, current);
    let mut selected = None;
    let response = ui
        .allocate_ui_with_layout(
            vec2(width, row_height),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.set_max_width(width);
                // ComboBox hashes its salt once before deriving its persistent widget ID.
                let was_open = ComboBox::is_open(ui.ctx(), ui.make_persistent_id(Id::new(id)));
                ComboBox::from_id_salt(id)
                    .width(width)
                    .height(height)
                    .truncate()
                    .selected_text(if enabled || mixed {
                        tr(title)
                    } else {
                        tr(title).color(ui.visuals().warn_fg_color)
                    })
                    .show_ui(ui, |ui| {
                        // Capture ScrollArea's viewport before its content expands max_rect.
                        #[cfg(test)]
                        let viewport_size = ui.max_rect().size();
                        ui.set_width(width);
                        ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                        #[cfg(test)]
                        let mut rows = vec![];
                        for (index, mode) in BlendMode::MENU.into_iter().enumerate() {
                            if index != 0 && mode.starts_menu_group() {
                                ui.separator();
                            }
                            let available = allows(supported, mode);
                            let response = ui.add_enabled(
                                available,
                                Button::selectable(!mixed && current == mode, tr(mode.title()))
                                    .truncate()
                                    .min_size(vec2(width, row_height)),
                            );
                            #[cfg(test)]
                            rows.push((mode, response.rect, response.enabled()));
                            if !was_open && !mixed && current == mode {
                                response.scroll_to_me(Some(Align::Center));
                            }
                            let response = if available {
                                response.on_hover_text(tr(mode.title()))
                            } else {
                                response.on_disabled_hover_text(format!(
                                    "{}\n{}",
                                    localize(mode.title()),
                                    localize("This blend mode requires a compatible compositor.")
                                ))
                            };
                            if response.clicked() {
                                selected = Some(mode);
                                ui.close();
                            }
                        }
                        #[cfg(test)]
                        ui.data_mut(|data| {
                            data.insert_temp(
                                Id::new("blend-menu-layout"),
                                MenuLayout {
                                    rows,
                                    clip: ui.clip_rect(),
                                    viewport_size,
                                    paint_margin: ui.visuals().clip_rect_margin,
                                    height_limit: height,
                                },
                            )
                        });
                    })
                    .response
            },
        )
        .inner;
    let response = if enabled || mixed {
        response.on_hover_text(tr(title))
    } else {
        response.on_hover_text(format!(
            "{}\n{}",
            localize(title),
            localize("This blend mode requires a compatible compositor.")
        ))
    };
    InnerResponse {
        inner: selected,
        response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn click(point: Pos2) -> Vec<Event> {
        vec![
            Event::PointerMoved(point),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ]
    }
    fn frame(
        ctx: &Context,
        width: f32,
        current: &mut BlendMode,
        mask: u32,
        events: Vec<Event>,
        time: f64,
    ) -> Response {
        let mut response = None;
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 640.))),
                events,
                time: Some(time),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| {
                    let result = combo(
                        ui,
                        "blend-test",
                        *current,
                        false,
                        ui.available_width(),
                        mask,
                    );
                    if let Some(mode) = result.inner {
                        *current = mode;
                    }
                    response = Some(result.response);
                });
            },
        );
        response.unwrap()
    }
    #[test]
    fn blend_dropdown_is_bounded_ordered_and_selectable_in_narrow_cjk_panels() {
        crate::i18n::load_test_catalogs();
        let previous = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for (width, font_size) in [(176., 13.), (176., 20.), (320., 13.), (320., 20.)] {
                let ctx = Context::default();
                let appearance = Appearance {
                    control_height: 30.,
                    item_spacing: 5.,
                    font_size,
                    ..Default::default()
                };
                theme::fonts(&ctx);
                theme::apply(&ctx, &appearance);
                let mut mode = BlendMode::Normal;
                let mut button = frame(&ctx, width, &mut mode, u32::MAX, vec![], 0.);
                for n in 1..3 {
                    button = frame(&ctx, width, &mut mode, u32::MAX, vec![], n as f64);
                }
                assert!(
                    button.rect.left() >= 0. && button.rect.right() <= width,
                    "{language:?}: width={width} font={font_size} button={:?}",
                    button.rect
                );
                frame(
                    &ctx,
                    width,
                    &mut mode,
                    u32::MAX,
                    click(button.rect.center()),
                    3.,
                );
                frame(&ctx, width, &mut mode, u32::MAX, vec![], 4.);
                let layout = ctx
                    .data_mut(|data| data.get_temp::<MenuLayout>(Id::new("blend-menu-layout")))
                    .unwrap();
                assert_eq!(
                    layout.rows.iter().map(|row| row.0).collect::<Vec<_>>(),
                    BlendMode::MENU
                );
                assert_eq!(layout.rows.len(), 26);
                let configured_height =
                    appearance.control_height * 9. + appearance.item_spacing * 8.;
                assert_eq!(layout.height_limit, configured_height);
                assert!(layout.viewport_size.y<=configured_height+1.,"{language:?}: width={width} font={font_size} viewport={:?} configured={configured_height} clip={:?} paint_margin={}",layout.viewport_size,layout.clip,layout.paint_margin);
                // ScrollArea deliberately adds clip_rect_margin on both vertical sides for
                // hover/stroke painting. It is not part of the bounded scroll viewport.
                assert!(
                    layout.clip.height() <= layout.viewport_size.y + 2. * layout.paint_margin + 1.,
                    "{language:?}: viewport={:?} clip={:?} paint_margin={}",
                    layout.viewport_size,
                    layout.clip,
                    layout.paint_margin
                );
                assert!(
                    layout.clip.left() >= 0. && layout.clip.right() <= width,
                    "{language:?}: width={width} font={font_size} clip={:?}",
                    layout.clip
                );
                assert!(
                    layout
                        .rows
                        .iter()
                        .all(|row| row.1.left() >= 0. && row.1.right() <= width),
                    "{language:?}: width={width} font={font_size} rows={:?}",
                    layout.rows
                );
                let choice = layout
                    .rows
                    .iter()
                    .find(|row| row.0 == BlendMode::ColorBurn)
                    .unwrap();
                assert!(choice.2 && layout.clip.contains(choice.1.center()));
                for pair in layout.rows.windows(2) {
                    assert!(pair[0].1.bottom() <= pair[1].1.top());
                }
                frame(
                    &ctx,
                    width,
                    &mut mode,
                    u32::MAX,
                    click(choice.1.center()),
                    5.,
                );
                assert_eq!(mode, BlendMode::ColorBurn);
                let button = frame(&ctx, width, &mut mode, u32::MAX, vec![], 6.);
                assert_eq!(mode.title(), "Color Burn");
                assert!(button.rect.right() <= width);
                mode = BlendMode::Luminosity;
                let button = frame(&ctx, width, &mut mode, u32::MAX, vec![], 7.);
                frame(
                    &ctx,
                    width,
                    &mut mode,
                    u32::MAX,
                    click(button.rect.center()),
                    8.,
                );
                for n in 9..12 {
                    frame(&ctx, width, &mut mode, u32::MAX, vec![], n as f64);
                }
                let layout = ctx
                    .data_mut(|data| data.get_temp::<MenuLayout>(Id::new("blend-menu-layout")))
                    .unwrap();
                let active = layout
                    .rows
                    .iter()
                    .find(|row| row.0 == BlendMode::Luminosity)
                    .unwrap();
                assert!(
                    active.2 && layout.clip.contains(active.1.center()),
                    "{language:?}: active last mode must be reachable in the scrolling popup"
                );
            }
        }
        crate::i18n::set_language(previous);
    }
    #[test]
    fn blend_dropdown_keeps_unsupported_modes_visible_without_changing_selection() {
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &Appearance::default());
        let mut mode = BlendMode::Normal;
        let button = frame(&ctx, 244., &mut mode, 0xff, vec![], 0.);
        frame(&ctx, 244., &mut mode, 0xff, click(button.rect.center()), 1.);
        frame(&ctx, 244., &mut mode, 0xff, vec![], 2.);
        let layout = ctx
            .data_mut(|data| data.get_temp::<MenuLayout>(Id::new("blend-menu-layout")))
            .unwrap();
        let unsupported = layout
            .rows
            .iter()
            .find(|row| row.0 == BlendMode::ColorBurn)
            .unwrap();
        assert!(!unsupported.2 && layout.clip.contains(unsupported.1.center()));
        frame(
            &ctx,
            244.,
            &mut mode,
            0xff,
            click(unsupported.1.center()),
            3.,
        );
        assert_eq!(mode, BlendMode::Normal);
        assert_eq!(layout.rows.iter().filter(|row| row.2).count(), 8);
        assert!(allows(0, BlendMode::Normal));
    }
}
