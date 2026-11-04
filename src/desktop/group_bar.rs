//! One compact group row for docked and native panels: grip, tabs, maximize, close.
use super::*;
use serde_json::json;
#[derive(Clone)]
struct DraggedTab(Panel);
#[derive(Clone, Debug)]
pub(super) struct HeaderLayout {
    pub bar: Rect,
    pub grip: Rect,
    pub tabs: Rect,
    pub maximize: Rect,
    pub close: Rect,
    /// The whole floating card: group row plus body.
    pub card: Rect,
}
pub(super) fn layout(body: Rect, padding: f32, a: &Appearance) -> HeaderLayout {
    let bar = Rect::from_min_max(
        pos2(body.left() - padding, body.top() - padding - a.tab_height),
        pos2(body.right() + padding, body.top() - padding),
    );
    let button = a
        .control_height
        .min(a.tab_height)
        .max(18.)
        .min((bar.width() - a.item_spacing * 2.).max(4.) / 4.);
    let grip = Rect::from_min_size(bar.min, vec2(button, bar.height()));
    let close = Rect::from_min_size(
        pos2(bar.right() - button, bar.top()),
        vec2(button, bar.height()),
    );
    let maximize = close.translate(vec2(-button, 0.));
    let tabs = Rect::from_min_max(
        pos2(grip.right() + a.item_spacing, bar.top()),
        pos2(maximize.left() - a.item_spacing, bar.bottom()),
    );
    HeaderLayout {
        bar,
        grip,
        tabs,
        maximize,
        close,
        card: Rect::from_min_max(bar.min, pos2(bar.right(), body.bottom() + padding)),
    }
}
/// Gaps between neighbouring cards, merged per splitter line. `vertical` gaps
/// separate side-by-side cards and are dragged horizontally.
pub(super) fn splitter_gaps(cards: &[Rect], limit: f32) -> Vec<(bool, Rect)> {
    let mut gaps: Vec<(bool, Rect)> = vec![];
    let mut add = |vertical: bool, gap: Rect| {
        let along = |r: &Rect| if vertical { r.center().x } else { r.center().y };
        if let Some((_, merged)) = gaps
            .iter_mut()
            .find(|(v, r)| *v == vertical && (along(r) - along(&gap)).abs() < 1.)
        {
            *merged = merged.union(gap);
        } else {
            gaps.push((vertical, gap));
        }
    };
    for a in cards {
        for b in cards {
            let x = b.left() - a.right();
            let y = a.y_range().intersection(b.y_range());
            if x > 0. && x <= limit && y.span() > 0. {
                add(true, Rect::from_x_y_ranges(a.right()..=b.left(), y));
            }
            let y = b.top() - a.bottom();
            let x = a.x_range().intersection(b.x_range());
            if y > 0. && y <= limit && x.span() > 0. {
                add(false, Rect::from_x_y_ranges(x, a.bottom()..=b.top()));
            }
        }
    }
    gaps
}
/// Rounded grab handles centred in splitter gaps; they lengthen and take the
/// accent while hovered or dragged so resizing is discoverable.
pub(super) fn splitter_handles(ui: &Ui, cards: &[Rect], a: &Appearance) {
    let colors = theme::Colors::new(a);
    let pointer = ui.ctx().pointer_latest_pos();
    let (down, origin) = ui.input(|i| (i.pointer.primary_down(), i.pointer.press_origin()));
    for (vertical, gap) in splitter_gaps(cards, a.outer_padding * 2.) {
        let zone = if vertical {
            gap.expand2(vec2(4., 0.))
        } else {
            gap.expand2(vec2(0., 4.))
        };
        let active = pointer.is_some_and(|p| zone.contains(p))
            || down && origin.is_some_and(|p| zone.contains(p));
        let key = Id::new((
            "splitter-handle",
            vertical,
            (gap.center().x + gap.center().y) as i32,
        ));
        let t = ui.ctx().animate_bool_with_time(key, active, 0.12);
        let thick = 4.;
        let length = (32. + 24. * t).min(if vertical { gap.height() } else { gap.width() } - 8.);
        if length <= 0. {
            continue;
        }
        let size = if vertical {
            vec2(thick, length)
        } else {
            vec2(length, thick)
        };
        let idle = colors
            .muted
            .gamma_multiply(if colors.dark { 0.6 } else { 0.55 });
        ui.painter().rect_filled(
            Rect::from_center_size(gap.center(), size),
            thick * 0.5,
            idle.lerp_to_gamma(colors.accent, t),
        );
    }
}
impl panels::Viewer<'_> {
    pub(super) fn group_bar(&mut self, ui: &Ui, panel: Panel) {
        let a = self.editor.settings.ui.clone();
        let padding = if panel == Panel::Canvas {
            0.
        } else {
            a.panel_padding
        };
        let h = layout(ui.max_rect(), padding, &a);
        if !h.bar.is_positive() || !h.bar.is_finite() || h.tabs.width() <= 0. {
            return;
        }
        self.editor.headers.insert(panel, h.clone());
        // The docking library reserves this row; paint controls outside the body clip
        // without advancing its layout. Its own tab widgets stay passive underneath.
        let mut row = Ui::new(
            ui.ctx().clone(),
            Id::new(("panel-group-controls", panel)),
            UiBuilder::new().max_rect(h.bar).layer_id(ui.layer_id()),
        );
        row.set_clip_rect(h.bar.intersect(ui.ctx().content_rect()));
        let colors = theme::Colors::new(&a);
        let radius = theme::card_radius(&a);
        row.painter().rect_filled(
            h.bar,
            CornerRadius {
                nw: radius,
                ne: radius,
                sw: 0,
                se: 0,
            },
            colors.header,
        );
        // A hairline divides the tab row from the panel content.
        row.painter().hline(
            h.bar.x_range(),
            h.bar.bottom() - 0.5,
            Stroke::new(
                1.,
                if colors.dark {
                    colors.border.gamma_multiply(0.8)
                } else {
                    colors.border
                },
            ),
        );
        // Hairline card outline, painted on the body layer so content stays on top.
        ui.ctx()
            .layer_painter(ui.layer_id())
            .with_clip_rect(h.card.expand(1.).intersect(ui.ctx().content_rect()))
            .rect_stroke(
                h.card,
                radius,
                Stroke::new(
                    1.,
                    if colors.dark {
                        colors.border.gamma_multiply(0.7)
                    } else {
                        colors.border
                    },
                ),
                StrokeKind::Inside,
            );
        // Window chrome stays quiet until the pointer reaches the group row.
        let near = row.rect_contains_pointer(h.bar.expand(4.));
        let reveal = row
            .ctx()
            .animate_bool_with_time(Id::new(("group-chrome", panel)), near, 0.15);
        let chrome = colors.muted.gamma_multiply(0.35 + 0.65 * reveal);
        row.interact(
            h.bar,
            Id::new(("group-row-background", panel)),
            Sense::click(),
        );
        let members = self
            .editor
            .panel_id(panel)
            .and_then(|id| {
                self.editor.session.ui["windows.actual"]
                    .as_array()
                    .and_then(|list| list.iter().find(|v| v["id"] == id))
                    .and_then(|v| v["tabs"].as_array())
                    .cloned()
            })
            .unwrap_or_default();
        let mut tabs: Vec<_> = members
            .iter()
            .filter_map(|v| v.as_str())
            .filter_map(|id| self.editor.session.registry.panels.get(id))
            .map(contributed_panel)
            .collect();
        if tabs.is_empty() {
            tabs.push(panel);
        }
        let group = tabs[0];
        let drag_id = Id::new("dragged-panel-tab");
        // Tabs rise from the bottom of the row; the active one shares the body
        // surface and flares into it. Their surfaces are painted beneath the labels.
        let lift = (a.tab_height * 0.16).round().clamp(3., 8.);
        let flare = (a.corner_radius * 0.75).clamp(3., 8.);
        let strip = Rect::from_x_y_ranges(h.tabs.x_range(), h.bar.top() + lift..=h.bar.bottom());
        let surfaces = row
            .painter()
            .with_clip_rect(h.tabs.expand2(vec2(flare, 0.)).intersect(h.bar));
        let slot = surfaces.add(Shape::Noop);
        let mut shapes: Vec<Shape> = vec![];
        let mut tab_rects: Vec<(Rect, bool, bool)> = vec![];
        let mut tab_ui = Ui::new(
            ui.ctx().clone(),
            Id::new(("group-tabs", group)),
            UiBuilder::new().max_rect(strip).layer_id(ui.layer_id()),
        );
        tab_ui.set_clip_rect(h.tabs.intersect(ui.ctx().content_rect()));
        tab_ui.spacing_mut().button_padding = vec2(a.panel_padding + 2., 0.);
        tab_ui.spacing_mut().interact_size.y = strip.height();
        ScrollArea::horizontal()
            .id_salt(("group-tab-scroll", group))
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() })
            .show(&mut tab_ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 1.;
                for (index, mut tab) in tabs.iter().copied().enumerate() {
                    let title = self.tab_title(&mut tab);
                    let active = tab == panel;
                    let title = RichText::new(title.text())
                        .font(if active { theme::strong(ui.ctx(), a.font_size) } else { FontId::proportional(a.font_size) })
                        .color(if active { colors.text } else { colors.muted });
                    // A transparent frame keeps button padding; surfaces are painted separately.
                    let response = ui.add(Button::new(title)
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::NONE)
                        .sense(Sense::click_and_drag())
                        .min_size(vec2(0., strip.height())));
                    tab_rects.push((response.rect, active, response.hovered()));
                    if let Some(id) = self.editor.panel_id(tab) {
                        if response.clicked() {
                            self.editor.core("window.command", json!({"panel":id,"operation":"focus"}));
                        }
                        if response.double_clicked() {
                            self.editor.core("window.command", json!({"panel":id,"operation":"float","scope":"tabs"}));
                        }
                        if response.drag_started() {
                            ui.ctx().data_mut(|d| d.insert_temp(drag_id, DraggedTab(tab)));
                        }
                        if response.dragged()
                            && ui.ctx().data(|d| d.get_temp::<DraggedTab>(drag_id)).is_some_and(|d| d.0 == tab)
                            && response.interact_pointer_pos().is_some_and(|p| !h.bar.expand(8.).contains(p))
                        {
                            if let Some(p) = response.interact_pointer_pos() {
                                ui.ctx().data_mut(|d| d.remove::<DraggedTab>(drag_id));
                                let origin = ui.input(|i| i.viewport().outer_rect.map(|r| r.min)).unwrap_or_default();
                                let position = origin + p.to_vec2()
                                    - vec2(a.outer_padding + a.control_height * 0.5, a.outer_padding + a.tab_height * 0.5);
                                self.editor.core("window.command", json!({"panel":id,"operation":"float","drag":true,"x":position.x,"y":position.y}));
                            }
                        }
                        if ui.input(|i| i.pointer.primary_released()) && response.contains_pointer() {
                            let dragged = ui.ctx().data_mut(|d| {
                                let value = d.get_temp::<DraggedTab>(drag_id);
                                d.remove::<DraggedTab>(drag_id);
                                value
                            });
                            if let Some(DraggedTab(source)) = dragged {
                                if source != tab {
                                    if let Some(source_id) = self.editor.panel_id(source) {
                                        self.editor.core("window.command", json!({"panel":source_id,"operation":"tab","target":id,"index":index}));
                                    }
                                }
                            }
                        }
                    }
                    response.context_menu(|ui| self.editor.window_menu(tab, ui));
                }
            });
        });
        let radius = (a.corner_radius * 0.9).min(strip.height() * 0.5);
        let top = CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        };
        for (index, &(rect, active, hovered)) in tab_rects.iter().enumerate() {
            let body = Rect::from_x_y_ranges(rect.x_range(), strip.y_range());
            if active {
                shapes.push(Shape::rect_filled(
                    body.expand2(vec2(0., 1.)).translate(vec2(0., 0.5)),
                    top,
                    colors.panel,
                ));
                for (x, side) in [(body.left(), -1.), (body.right(), 1.)] {
                    let corner = Rect::from_min_max(
                        pos2(x.min(x + side * flare), body.bottom() - flare),
                        pos2(x.max(x + side * flare), body.bottom() + 1.),
                    );
                    shapes.push(Shape::rect_filled(corner, 0, colors.panel));
                    shapes.push(Shape::circle_filled(
                        pos2(x + side * flare, body.bottom() - flare),
                        flare,
                        colors.header,
                    ));
                }
            } else if hovered {
                shapes.push(Shape::rect_filled(
                    body.shrink2(vec2(2., 0.)).with_max_y(body.bottom() - lift),
                    radius,
                    colors.header.lerp_to_gamma(colors.border, 0.7),
                ));
            }
            // Thin dividers sit between neighbouring tabs that are both at rest.
            if let Some(&(next, next_active, next_hovered)) = tab_rects.get(index + 1) {
                if !(active || hovered || next_active || next_hovered) {
                    let x = (rect.right() + next.left()) * 0.5;
                    let y = body.center().y - lift * 0.5;
                    shapes.push(Shape::line_segment(
                        [pos2(x, y - 7.), pos2(x, y + 7.)],
                        Stroke::new(1., colors.border),
                    ));
                }
            }
        }
        surfaces.set(slot, Shape::Vec(shapes));
        if row.input(|i| i.pointer.primary_released() || i.key_pressed(Key::Escape)) {
            row.ctx().data_mut(|d| d.remove::<DraggedTab>(drag_id));
        }
        let grip = row
            .interact(
                h.grip,
                Id::new(("group-grip", panel)),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(CursorIcon::Grab)
            .on_hover_text(tr("Drag This Title Bar to Move the Entire Group"));
        for x in 0..2 {
            for y in 0..3 {
                row.painter().circle_filled(
                    h.grip.center() + vec2(x as f32 * 5. - 2.5, y as f32 * 5. - 5.),
                    1.2,
                    chrome,
                );
            }
        }
        let Some(id) = self.editor.panel_id(panel) else {
            return;
        };
        if grip.drag_started() {
            if let Some(native) = self.native.filter(|_| self.single_leaf) {
                if let Some(p) = grip.interact_pointer_pos() {
                    native.begin_group_drag(p * row.ctx().pixels_per_point());
                }
                row.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            } else if let Some(p) = grip.interact_pointer_pos() {
                let origin = row
                    .input(|i| i.viewport().outer_rect.map(|r| r.min))
                    .unwrap_or_default();
                let position = origin + p.to_vec2()
                    - vec2(
                        a.outer_padding + h.grip.width() * 0.5,
                        a.outer_padding + a.tab_height * 0.5,
                    );
                self.editor.core("window.command",json!({"panel":id,"scope":"tabs","operation":"float","drag":true,"x":position.x,"y":position.y,
                    "width":(h.bar.width()+a.outer_padding*2.).max(200.),"height":(ui.max_rect().height()+padding*2.+a.tab_height+a.outer_padding*2.).max(160.)}));
            }
        }
        grip.context_menu(|ui| self.editor.window_menu(panel, ui));
        let restore = self.focused
            || self.native.is_some() && row.input(|i| i.viewport().maximized.unwrap_or(false));
        let max = theme::chrome_button(&mut row, h.maximize.shrink(2.));
        let r = Rect::from_center_size(
            max.rect.center(),
            Vec2::splat(max.rect.width().min(max.rect.height()) * 0.45),
        );
        if restore {
            row.painter().rect_stroke(
                r.translate(vec2(2., -2.)),
                1.5,
                Stroke::new(1.2, chrome),
                StrokeKind::Inside,
            );
        }
        row.painter()
            .rect_stroke(r, 1.5, Stroke::new(1.2, chrome), StrokeKind::Inside);
        if max
            .on_hover_text(tr(if restore {
                "Restore Window"
            } else {
                "Maximize Window"
            }))
            .clicked()
        {
            self.editor.core("window.command",json!({"panel":id,"scope":"tabs","operation":if restore {"restore"} else {"maximize"}}));
        }
        let close = theme::chrome_button(&mut row, h.close.shrink(2.));
        let r = Rect::from_center_size(
            close.rect.center(),
            Vec2::splat(close.rect.width().min(close.rect.height()) * 0.34),
        );
        row.painter()
            .line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.3, chrome));
        row.painter()
            .line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.3, chrome));
        if close.on_hover_text(tr("Hide Panel Group")).clicked() {
            self.editor.core(
                "window.command",
                json!({"panel":id,"scope":"tabs","operation":"hide"}),
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    #[test]
    fn canvas_row_maximize_button_receives_pointer_input() {
        let mut app = studio();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        let p = app.editor.headers[&Panel::Canvas].maximize.center();
        for (i, pressed) in [true, false].into_iter().enumerate() {
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                4. + i as f64 * 0.1,
            );
        }
        frame(&mut app, &ctx, size, vec![], 6.);
        assert!(app.dock_before_focus.is_some());
    }
    #[test]
    fn row_buttons_maximize_restore_and_close_only_their_tab_group() {
        let mut app = studio();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        let before = encode_dock(&app.dock).unwrap();
        let click = |app: &mut Studio, p: Pos2, time| {
            frame(
                app,
                &ctx,
                size,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                time,
            );
            frame(
                app,
                &ctx,
                size,
                vec![Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                time + 0.1,
            );
            frame(app, &ctx, size, vec![], time + 0.2);
        };
        let active = app
            .dock
            .find_tab(&Panel::Layers)
            .map(|(s, n, _)| match &app.dock[s][n] {
                egui_dock::Node::Leaf(l) => l.tabs[l.active.0],
                _ => unreachable!(),
            })
            .unwrap();
        let p = app.editor.headers[&active].maximize.center();
        click(&mut app, p, 4.);
        assert!(app.dock_before_focus.is_some());
        for tab in [Panel::Layers, Panel::Brush, Panel::Color] {
            assert!(app.dock.find_tab(&tab).is_some());
        }
        let p = app.editor.headers[&active].maximize.center();
        click(&mut app, p, 5.);
        assert!(app.dock_before_focus.is_none());
        // Layout fractions and tabs are restored; viewport rectangles are refreshed by rendering.
        let restored = encode_dock(&app.dock).unwrap();
        assert_eq!(restored, before);
        let p = app.editor.headers[&active].close.center();
        click(&mut app, p, 6.);
        for tab in [Panel::Layers, Panel::Brush, Panel::Color] {
            assert!(app.dock.find_tab(&tab).is_none());
        }
        assert!(app.dock.find_tab(&Panel::Canvas).is_some());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
    }
    #[test]
    fn splitter_gaps_merge_one_line_per_splitter() {
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(pos2(x0, y0), pos2(x1, y1));
        // Tools over Canvas on the left; Navigator over Layers on the right.
        let cards = [
            r(10., 10., 600., 100.),
            r(10., 110., 600., 800.),
            r(610., 10., 900., 300.),
            r(610., 310., 900., 800.),
        ];
        let gaps = splitter_gaps(&cards, 20.);
        let vertical: Vec<_> = gaps.iter().filter(|(v, _)| *v).collect();
        let horizontal: Vec<_> = gaps.iter().filter(|(v, _)| !*v).collect();
        assert_eq!(vertical.len(), 1);
        assert_eq!(vertical[0].1, r(600., 10., 610., 800.));
        assert_eq!(horizontal.len(), 2);
        assert!(splitter_gaps(&cards, 5.).is_empty());
    }
    #[test]
    fn grip_tabs_and_group_buttons_share_one_non_overlapping_row() {
        for width in [180., 320., 720.] {
            for padding in [0., 8.] {
                let a = Appearance::default();
                let h = layout(
                    Rect::from_min_size(pos2(20., 100.), vec2(width, 400.)),
                    padding,
                    &a,
                );
                assert_eq!(h.grip.center().y, h.maximize.center().y);
                assert_eq!(h.tabs.center().y, h.close.center().y);
                assert!(h.grip.right() < h.tabs.left());
                assert!(h.tabs.right() < h.maximize.left());
                assert_eq!(h.maximize.right(), h.close.left());
                assert!(h.tabs.width() > 0.);
            }
        }
    }
}
