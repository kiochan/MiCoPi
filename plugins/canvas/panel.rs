use super::*;
#[path = "symmetry_feedback.rs"]
mod symmetry_feedback;
use crate::{platform::windows::desktop_bridge::PenPhase, ui::keymap::Wheel};

impl Editor {
    /// Zoom out, a logarithmic slider, zoom in and an editable percentage, with
    /// fit and actual-pixels shortcuts; export and document size sit on the left.
    fn zoom_strip(&mut self, ui: &mut Ui, strip: Rect, c: &theme::Colors) {
        if strip.height() < 8. {
            return;
        }
        let a = self.settings.ui.clone();
        let card = theme::card_radius(&a);
        ui.painter().rect_filled(
            strip,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: card,
                se: card,
            },
            c.panel,
        );
        ui.painter().hline(
            strip.x_range(),
            strip.top() + 0.5,
            Stroke::new(1., c.border.gamma_multiply(if c.dark { 0.8 } else { 1. })),
        );
        let inner = strip.shrink2(vec2(a.panel_padding * 0.5, 3.));
        let mut zoom = self.zoom;
        let mut fit = false;
        let mut actual = false;
        let mut export = false;
        ui.scope_builder(
            UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::left_to_right(Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 2.;
                ui.spacing_mut().interact_size.y = inner.height();
                ui.spacing_mut().button_padding = vec2(6., 0.);
                let small = |ui: &mut Ui, editor: &Editor, id: u32| {
                    let hint = editor.hint(id);
                    let keys = editor.label(id);
                    ui.add_sized(
                        [inner.height() + 2., inner.height()],
                        Button::new(tr(theme::icon(theme::action_icon(id), 16.)))
                            .frame_when_inactive(false),
                    )
                    .on_hover_ui(|ui| {
                        ui.label(tr(hint.split("  ").next().unwrap_or(&hint)));
                        theme::keycaps(ui, &keys);
                    })
                };
                export = small(ui, self, 4).clicked();
                ui.add_space(6.);
                // The workspace has no separate status bar; status, pen pressure,
                // document size and history memory live here beside export.
                let slider = if inner.width() > 300. {
                    (inner.width() * 0.22).clamp(80., 180.)
                } else {
                    0.
                };
                let controls = 58. + (inner.height() + 2.) * 4. + 32. + 13. + slider + 32.;
                let room = (ui.available_width() - controls).max(0.);
                if room > 24. {
                    let status = if self.pen_active {
                        format!("Windows Ink · Pressure {:.0}%", self.pressure * 100.)
                    } else if self.session.view.mirror_x || self.session.view.mirror_y {
                        format!("{} · {}", localize("Mirrored View"), self.status)
                    } else {
                        self.status.clone()
                    };
                    let status = if self.session.doc.selection().current.is_some() {
                        format!("{} · {}", localize(status), localize("Selection"))
                    } else {
                        status
                    };
                    let status = if matches!(
                        self.session.doc.mask_state().target,
                        crate::engine::raster_masks::PaintTarget::Mask { .. }
                    ) {
                        format!("{} · {}", localize("Mask"), localize(status))
                    } else {
                        status
                    };
                    let details = format!(
                        "{} × {} px · History {:.1} MB",
                        self.session.doc.width,
                        self.session.doc.height,
                        self.session.doc.history_bytes() as f64 / 1048576.
                    );
                    ui.allocate_ui_with_layout(
                        vec2(room, inner.height()),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            let (dot, _) = ui.allocate_exact_size(vec2(10., 8.), Sense::hover());
                            ui.painter().circle_filled(
                                pos2(dot.left() + 3., dot.center().y),
                                3.,
                                if self.pen_active { c.blush } else { c.accent },
                            );
                            ui.add(
                                Label::new(tr(RichText::new(localize(status))
                                    .small()
                                    .color(c.muted)))
                                .truncate(),
                            )
                            .on_hover_text(tr(details));
                        },
                    );
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let mut percent = zoom * 100.;
                    let field = ui
                        .add_sized(
                            [58., inner.height()],
                            DragValue::new(&mut percent)
                                .range(2. ..=3200.)
                                .speed(1.)
                                .fixed_decimals(0)
                                .suffix("%"),
                        )
                        .on_hover_text(tr("Zoom"));
                    if field.changed() {
                        zoom = percent / 100.;
                    }
                    if small(ui, self, 85).clicked() {
                        zoom *= 1.25;
                    }
                    if inner.width() > 300. {
                        ui.spacing_mut().slider_width = (inner.width() * 0.22).clamp(80., 180.);
                        // The knob scales with the row height; keep it small in the thin strip.
                        ui.spacing_mut().interact_size.y = 14.;
                        ui.add(
                            Slider::new(&mut zoom, 0.02..=32.)
                                .logarithmic(true)
                                .show_value(false),
                        )
                        .on_hover_text(tr("Zoom"));
                    }
                    ui.spacing_mut().interact_size.y = inner.height();
                    if small(ui, self, 86).clicked() {
                        zoom /= 1.25;
                    }
                    ui.add_space(6.);
                    let (line, _) =
                        ui.allocate_exact_size(vec2(1., inner.height() * 0.6), Sense::hover());
                    ui.painter()
                        .vline(line.center().x, line.y_range(), Stroke::new(1., c.border));
                    ui.add_space(6.);
                    actual = ui
                        .add_sized(
                            [32., inner.height()],
                            Button::new(tr(RichText::new(localize("1:1")).small()))
                                .frame_when_inactive(false),
                        )
                        .on_hover_text(tr(self.hint(41)))
                        .clicked();
                    fit = small(ui, self, 40).clicked();
                });
            },
        );
        if export {
            self.actions.push(Action::Export);
        }
        if fit {
            self.actions.push(Action::Fit);
        } else if actual {
            self.actions.push(Action::ActualPixels);
        } else if (zoom - self.zoom).abs() > f32::EPSILON * self.zoom.max(1.) {
            self.zoom_at(self.canvas.center(), zoom.clamp(0.02, 32.) / self.zoom);
        }
    }
    /// Reflection is its own inverse in unscaled document coordinates.
    pub(super) fn reflect_view_point(&self, p: Pos2) -> Pos2 {
        pos2(
            if self.session.view.mirror_x {
                self.session.doc.width as f32 - p.x
            } else {
                p.x
            },
            if self.session.view.mirror_y {
                self.session.doc.height as f32 - p.y
            } else {
                p.y
            },
        )
    }
    pub(super) fn document_position(&self, p: Pos2) -> Pos2 {
        let q = (p - self.image_rect.min) / self.zoom;
        self.reflect_view_point(pos2(q.x, q.y))
    }
    pub(super) fn document_screen_position(&self, p: Pos2) -> Pos2 {
        self.image_rect.min + self.reflect_view_point(p).to_vec2() * self.zoom
    }
    pub(super) fn document_screen_rect(&self, rect: Rect) -> Rect {
        Rect::from_two_pos(
            self.document_screen_position(rect.min),
            self.document_screen_position(rect.max),
        )
    }
    pub(super) fn canvas_uv(&self) -> Rect {
        let x = if self.session.view.mirror_x { 1. } else { 0. };
        let y = if self.session.view.mirror_y { 1. } else { 0. };
        Rect::from_min_max(pos2(x, y), pos2(1. - x, 1. - y))
    }
    pub(super) fn zoom_at(&mut self, p: Pos2, factor: f32) {
        if !self.canvas.is_positive() {
            return;
        }
        let before = self.reflect_view_point(self.document_position(p));
        self.fit = false;
        self.zoom = (self.zoom * factor).clamp(0.02, 32.);
        let size = vec2(
            self.session.doc.width as f32,
            self.session.doc.height as f32,
        ) * self.zoom;
        self.pan = p.to_vec2() - before.to_vec2() * self.zoom - self.canvas.center().to_vec2()
            + size * 0.5;
        self.image_rect = Rect::from_center_size(self.canvas.center() + self.pan, size);
    }
    fn finish_for_pointer_down(&mut self) {
        // Entering a new native contact intentionally commits the previous
        // one, unlike cancelling an outline that owns no native contact.
        self.session.finish_stroke();
        self.clear_local_contact();
    }
    fn pointer_down(
        &mut self,
        p: Pos2,
        pressure: f32,
        time: u32,
        pen: bool,
        held: crate::ui::keymap::InputState,
        middle: bool,
        mods: Modifiers,
    ) -> bool {
        if self.guarded_movement_input(held, middle) {
            return self.movement_start(self.document_position(p));
        }
        self.push_gui_parameters();
        self.push_gui_view();
        let tool_override = if middle || held.pan {
            Some(Tool::Hand)
        } else if held.pick && self.area_mode < 3 {
            Some(Tool::Picker)
        } else if held.zoom || held.zoom_out {
            Some(Tool::Zoom)
        } else {
            None
        };
        let q = self.document_position(p);
        if self.area_mode >= 3 {
            self.pressure = pressure;
        }
        if tool_override.is_none() && self.movement_down(q) {
            return true;
        }
        if tool_override.is_none() && self.selection_down(q, mods, time) {
            return true;
        }
        if tool_override.is_none() && self.area_down(q) {
            return true;
        }
        let sample = crate::application::Pointer {
            phase: crate::application::PointerPhase::Down,
            x: q.x,
            y: q.y,
            pressure,
            time,
            pen,
            eraser: self.pen_eraser || held.zoom_out || held.tool_zoom_out,
            view_x: Some(p.x),
            view_y: Some(p.y),
            tool_override,
        };
        self.finish_for_pointer_down();
        match self.session.pointer(sample) {
            Ok(()) => {
                self.input_owned = self.session.is_drawing() || self.session.is_panning();
                self.sync_from_session();
                self.panning = if self.session.is_panning() {
                    Some(p)
                } else {
                    None
                };
                self.pressure = pressure;
                true
            }
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }
    fn move_pointer(&mut self, p: Pos2, pressure: f32, time: u32, pen: bool, constrain: bool) {
        if self.movement_move(self.document_position(p), constrain) {
            return;
        }
        if self.selection_gesture.is_some() {
            self.pressure = pressure;
            self.selection_move(self.document_position(p), constrain);
            return;
        }
        if self.area_drag.is_some() {
            let q = self.document_position(p);
            let (start, end) = self.area_drag.as_mut().unwrap();
            *end = if constrain {
                super::area::snap_gradient(*start, q)
            } else {
                q
            };
            return;
        }
        if !self.input_owned || (!self.session.is_drawing() && !self.session.is_panning()) {
            return;
        }
        let q = self.document_position(p);
        let sample = crate::application::Pointer {
            phase: crate::application::PointerPhase::Move,
            x: q.x,
            y: q.y,
            pressure,
            time,
            pen,
            eraser: self.pen_eraser,
            view_x: Some(p.x),
            view_y: Some(p.y),
            tool_override: None,
        };
        match self.session.pointer(sample) {
            Ok(()) => {
                self.sync_from_session();
                self.panning = if self.session.is_panning() {
                    Some(p)
                } else {
                    None
                };
                self.pressure = pressure;
            }
            Err(e) => self.status = e.to_string(),
        }
        self.image_rect = Rect::from_center_size(
            self.canvas.center() + self.pan,
            vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * self.zoom,
        );
    }
    pub(super) fn canvas_ui(&mut self, ui: &mut Ui) {
        let appearance = self.settings.ui.clone();
        let c = theme::Colors::new(&self.settings.ui);
        // A thin zoom strip sits beneath the stage, like a status bar for the view.
        let strip_height = (appearance.control_height + 2.).round();
        let full = ui.available_rect_before_wrap();
        let strip = Rect::from_min_max(
            pos2(full.left(), (full.bottom() - strip_height).max(full.top())),
            full.max,
        );
        let (bounds, response) = ui.allocate_exact_size(
            vec2(full.width(), (full.height() - strip_height).max(1.)),
            Sense::click_and_drag(),
        );
        self.canvas = bounds;
        self.zoom_strip(ui, strip, &c);
        if self.fit {
            let margin = self.settings.ui.canvas_margin * 2.;
            self.zoom = ((bounds.width() - margin).max(32.) / self.session.doc.width as f32)
                .min((bounds.height() - margin).max(32.) / self.session.doc.height as f32)
                .clamp(0.02, 8.);
            self.pan = Vec2::ZERO;
        }
        self.image_rect = Rect::from_center_size(
            bounds.center() + self.pan,
            vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * self.zoom,
        );
        let (mut events, mods, time) =
            ui.input(|i| (i.events.clone(), i.modifiers, (i.time * 1000.) as u32));
        if !self.selection_preview_valid() || (self.blocked && self.selection_gesture.is_some()) {
            self.finish_stroke();
        }
        if (self.area_drag.is_some() || self.selection_gesture.is_some())
            && ui.input(|i| i.key_pressed(Key::Escape))
        {
            self.finish_stroke();
        }
        if self.polygon_pending() && ui.input(|i| i.key_pressed(Key::Enter)) {
            self.selection_commit();
        }
        let held = self.settings.keys.held(commands::mod_bits(mods), |key| {
            ui.input(|i| commands::vk_to_key(key).is_some_and(|key| i.key_down(key)))
        });
        let layer = ui.layer_id();
        let ctx = ui.ctx().clone();
        let on_canvas = |p: Pos2| bounds.contains(p) && ctx.layer_id_at(p) == Some(layer);
        if self.liquify.is_some() {
            self.liquify_input(ui, bounds, layer, &events);
            self.packets.clear();
            events.clear();
        }
        if self.transform.is_some()
            && !(self.guarded_transform_modal() && self.backend_contact_active())
        {
            self.transform_input(ui, bounds, layer, &events, mods);
            self.packets.clear();
        }
        let mut packets = std::mem::take(&mut self.packets);
        let pen_frame = self.pen_active
            || packets
                .iter()
                .any(|s| matches!(s.phase, PenPhase::Down | PenPhase::Move | PenPhase::Up));
        for packet in packets.iter().copied() {
            let p = pos2(packet.x, packet.y) / ctx.pixels_per_point();
            let guarded_move_down = matches!(packet.phase, PenPhase::Down)
                && !self.blocked
                && on_canvas(p)
                && self.guarded_movement_input(held, false);
            if !matches!(packet.phase, PenPhase::Cancel | PenPhase::Leave) && !guarded_move_down {
                self.pen_hover = Some(p);
                self.pen_eraser = packet.eraser;
            }
            match packet.phase {
                PenPhase::Down if !self.blocked && on_canvas(p) => {
                    if guarded_move_down {
                        if self.pointer_down(
                            p,
                            packet.pressure,
                            packet.time,
                            true,
                            held,
                            false,
                            mods,
                        ) {
                            self.pen_active = true;
                            self.pen_hover = Some(p);
                            self.pen_eraser = packet.eraser;
                            self.pressure = packet.pressure;
                        }
                    } else {
                        if !self.polygon_pending() {
                            self.finish_for_pointer_down();
                        }
                        self.pen_active = true;
                        self.pointer_down(p, packet.pressure, packet.time, true, held, false, mods);
                    }
                }
                PenPhase::Move if self.pen_active => {
                    self.move_pointer(p, packet.pressure, packet.time, true, mods.shift)
                }
                PenPhase::Hover if self.polygon_pending() => {
                    self.selection_move(self.document_position(p), mods.shift);
                }
                PenPhase::Up if self.pen_active => {
                    let guarded_outline = self.guarded_movement_pending();
                    self.move_pointer(p, self.pressure, packet.time, true, mods.shift);
                    self.movement_release();
                    self.area_up();
                    self.selection_release();
                    if !self.polygon_pending() && !guarded_outline {
                        self.finish_stroke();
                    }
                    self.pen_active = false;
                    self.pressure = 1.;
                }
                PenPhase::Cancel => {
                    if self.input_owned || self.selection_gesture.is_some() {
                        self.finish_stroke();
                    }
                    self.pen_active = false;
                    self.pen_hover = None;
                }
                PenPhase::Leave => self.pen_hover = None,
                _ => {}
            }
        }
        packets.clear();
        self.packets = packets;
        if !pen_frame {
            for event in events {
                match event {
                    Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        ..
                    } if button == PointerButton::Primary || button == PointerButton::Middle => {
                        if pressed && !self.blocked && on_canvas(pos) {
                            let guarded_move =
                                self.guarded_movement_input(held, button == PointerButton::Middle);
                            if !guarded_move {
                                self.pen_eraser = false;
                            }
                            let admitted = self.pointer_down(
                                pos,
                                1.,
                                time,
                                false,
                                held,
                                button == PointerButton::Middle,
                                mods,
                            );
                            if guarded_move && admitted {
                                self.pen_eraser = false;
                            }
                        } else if !pressed {
                            let guarded_outline = self.guarded_movement_pending();
                            let finish_owned =
                                self.input_owned || self.session.pointer_checkpoint.is_none();
                            if self.movement_gesture.is_some() {
                                self.move_pointer(pos, 1., time, false, mods.shift);
                                self.movement_release();
                            }
                            if self.selection_gesture.is_some() {
                                self.move_pointer(pos, 1., time, false, mods.shift);
                                self.selection_release();
                            }
                            if self.area_drag.is_some() {
                                self.move_pointer(pos, 1., time, false, mods.shift);
                                self.area_up();
                            }
                            if !self.polygon_pending() && finish_owned && !guarded_outline {
                                self.finish_stroke();
                            }
                        }
                    }
                    Event::PointerMoved(pos) => {
                        self.pen_hover = None;
                        self.mouse_position = Some(pos);
                        self.move_pointer(pos, 1., time, false, mods.shift);
                    }
                    Event::WindowFocused(false) => self.finish_stroke(),
                    Event::PointerGone
                        if (self.selection_gesture.is_some()
                            || self.movement_gesture.is_some())
                            && !self.polygon_pending() =>
                    {
                        self.finish_stroke()
                    }
                    Event::PointerButton {
                        button: PointerButton::Secondary,
                        pressed: true,
                        ..
                    } if self.selection_gesture.is_some() => self.finish_stroke(),
                    Event::MouseWheel {
                        delta, modifiers, ..
                    } if !self.blocked && response.hovered() => {
                        let amount = delta.y;
                        match self.settings.keys.wheel(commands::mod_bits(modifiers)) {
                            Some(Wheel::Zoom) => self.zoom_at(
                                ctx.pointer_hover_pos().unwrap_or(bounds.center()),
                                (amount * 0.003).exp(),
                            ),
                            Some(Wheel::Horizontal) => {
                                self.fit = false;
                                self.pan.x += amount;
                            }
                            Some(Wheel::Vertical) => {
                                self.fit = false;
                                self.pan.y += amount;
                            }
                            None => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        if self.blocked
            && self.input_owned
            && !(self.contact_draft_allowed && self.contact_draft_modal())
        {
            self.finish_stroke();
        }
        self.image_rect = Rect::from_center_size(
            bounds.center() + self.pan,
            vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * self.zoom,
        );
        self.refresh_texture(ui.ctx());
        self.refresh_selection(ui.ctx());
        let painter = ui.painter().with_clip_rect(bounds);
        // A recessed stage sits inside the card between the tab row and the zoom strip.
        painter.rect_filled(bounds, 0, c.stage);
        if let Some(texture) = self
            .color_preview_texture()
            .or_else(|| self.liquify_texture())
            .or_else(|| self.transform_texture())
            .or(self.texture.as_ref())
        {
            for (offset, blur, alpha) in [([0, 2], 6, 0.6), ([0, 14], 40, 1.)] {
                painter.add(
                    Shadow {
                        offset,
                        blur,
                        spread: 0,
                        color: c.shadow.gamma_multiply(alpha),
                    }
                    .as_shape(self.image_rect, 1),
                );
            }
            painter.image(
                texture.id(),
                self.image_rect,
                self.canvas_uv(),
                Color32::WHITE,
            );
        }
        self.paint_symmetry_guides(&painter, c.accent);
        self.movement_preview(&painter, c.accent);
        self.crop_outline(&painter, c.accent);
        self.selection_preview(&painter);
        if let Some(texture) = self.transform_mask() {
            painter.image(
                texture.id(),
                self.image_rect,
                self.canvas_uv(),
                Color32::WHITE,
            );
        }
        self.transform_outline(&painter, c.accent);
        self.liquify_cursor(&painter, c.accent);
        if !self.blocked {
            if let Some((a, b)) = self.area_drag {
                let screen = |q: Pos2| self.document_screen_position(q);
                painter.line_segment([screen(a), screen(b)], Stroke::new(2., c.accent));
            }
            let p = self.pen_hover.or_else(|| ctx.pointer_hover_pos());
            if let Some(p) = p.filter(|p| on_canvas(*p)) {
                if self.panning.is_some() || self.tool == Tool::Hand || held.pan {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                } else if self.tool == Tool::Zoom || held.zoom || held.zoom_out {
                    ctx.set_cursor_icon(CursorIcon::ZoomIn);
                } else if self.area_mode == 8 {
                    ctx.set_cursor_icon(CursorIcon::Move);
                } else if self.tool == Tool::Picker || held.pick || self.area_mode != 0 {
                    ctx.set_cursor_icon(CursorIcon::Crosshair);
                } else {
                    ctx.set_cursor_icon(CursorIcon::None);
                    // The DLL clamps the major radius in document pixels;
                    // screen-space readability marks are separate from the tip.
                    let radius =
                        self.brush(self.pressure, self.pen_active).radius.max(0.5) * self.zoom;
                    self.paint_symmetry_cursors(&ctx, &painter, p, radius);
                    if radius < 4. {
                        painter.line_segment(
                            [p - vec2(3., 0.), p + vec2(3., 0.)],
                            Stroke::new(1., c.text),
                        );
                        painter.line_segment(
                            [p - vec2(0., 3.), p + vec2(0., 3.)],
                            Stroke::new(1., c.text),
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "brush_tip_cursor_tests.rs"]
mod brush_tip_cursor_tests;

#[cfg(test)]
#[path = "geometry_contact_ui_tests.rs"]
mod geometry_contact_ui_tests;

#[cfg(test)]
#[path = "mask_footer_tests.rs"]
mod mask_footer_tests;
