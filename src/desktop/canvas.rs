use super::*;
use crate::{platform::windows::desktop_bridge::PenPhase, ui::keymap::Wheel};

impl Editor {
    pub(super) fn document_position(&self, p: Pos2) -> Pos2 {
        pos2(
            (p.x - self.image_rect.left()) / self.zoom,
            (p.y - self.image_rect.top()) / self.zoom,
        )
    }
    pub(super) fn zoom_at(&mut self, p: Pos2, factor: f32) {
        let before = self.document_position(p);
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
    fn pointer_down(
        &mut self,
        p: Pos2,
        pressure: f32,
        time: u32,
        pen: bool,
        held: crate::ui::keymap::InputState,
        middle: bool,
    ) {
        self.push_gui_parameters();
        self.push_gui_view();
        let tool_override = if middle || held.pan {
            Some(Tool::Hand)
        } else if held.pick {
            Some(Tool::Picker)
        } else if held.zoom || held.zoom_out {
            Some(Tool::Zoom)
        } else {
            None
        };
        let q = self.document_position(p);
        if tool_override.is_none() && self.area_down(q) {
            return;
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
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    fn move_pointer(&mut self, p: Pos2, pressure: f32, time: u32, pen: bool) {
        if self.area_drag.is_some() {
            let q = self.document_position(p);
            self.area_drag.as_mut().unwrap().1 = q;
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
        let c = theme::Colors::new(&self.settings.ui);
        let (bounds, response) = ui.allocate_exact_size(
            ui.available_size().max(vec2(1., 1.)),
            Sense::click_and_drag(),
        );
        self.canvas = bounds;
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
        let (events, mods, time) =
            ui.input(|i| (i.events.clone(), i.modifiers, (i.time * 1000.) as u32));
        if self.area_drag.is_some() && ui.input(|i| i.key_pressed(Key::Escape)) {
            self.finish_stroke();
        }
        let held = self.settings.keys.held(commands::mod_bits(mods), |key| {
            ui.input(|i| commands::vk_to_key(key).is_some_and(|key| i.key_down(key)))
        });
        let layer = ui.layer_id();
        let ctx = ui.ctx().clone();
        let on_canvas = |p: Pos2| bounds.contains(p) && ctx.layer_id_at(p) == Some(layer);
        let mut packets = std::mem::take(&mut self.packets);
        let pen_frame = self.pen_active
            || packets
                .iter()
                .any(|s| matches!(s.phase, PenPhase::Down | PenPhase::Move | PenPhase::Up));
        for packet in packets.iter().copied() {
            let p = pos2(packet.x, packet.y) / ctx.pixels_per_point();
            if !matches!(packet.phase, PenPhase::Cancel | PenPhase::Leave) {
                self.pen_hover = Some(p);
                self.pen_eraser = packet.eraser;
            }
            match packet.phase {
                PenPhase::Down if !self.blocked && on_canvas(p) => {
                    self.finish_stroke();
                    self.pen_active = true;
                    self.pointer_down(p, packet.pressure, packet.time, true, held, false);
                }
                PenPhase::Move if self.pen_active => {
                    self.move_pointer(p, packet.pressure, packet.time, true)
                }
                PenPhase::Up if self.pen_active => {
                    self.move_pointer(p, self.pressure, packet.time, true);
                    self.area_up();
                    self.finish_stroke();
                    self.pen_active = false;
                    self.pressure = 1.;
                }
                PenPhase::Cancel => {
                    if self.input_owned {
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
                            self.pen_eraser = false;
                            self.pointer_down(
                                pos,
                                1.,
                                time,
                                false,
                                held,
                                button == PointerButton::Middle,
                            );
                        } else if !pressed {
                            if self.area_drag.is_some() {
                                self.move_pointer(pos, 1., time, false);
                                self.area_up();
                            }
                            self.finish_stroke();
                        }
                    }
                    Event::PointerMoved(pos) => {
                        self.pen_hover = None;
                        self.mouse_position = Some(pos);
                        self.move_pointer(pos, 1., time, false);
                    }
                    Event::WindowFocused(false) => self.finish_stroke(),
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
        if self.blocked && self.input_owned {
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
        let painter = ui.painter().with_clip_rect(bounds);
        painter.rect_filled(bounds, 0, c.background);
        if let Some(texture) = &self.texture {
            painter.rect_filled(
                self.image_rect.translate(vec2(0., 3.)).expand(2.),
                2.,
                Color32::from_black_alpha(65),
            );
            painter.image(
                texture.id(),
                self.image_rect,
                Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        if !self.blocked {
            if let Some((a, b)) = self.area_drag {
                let screen = |q: Pos2| self.image_rect.min + q.to_vec2() * self.zoom;
                painter.line_segment([screen(a), screen(b)], Stroke::new(2., c.accent));
            }
            let p = self.pen_hover.or_else(|| ctx.pointer_hover_pos());
            if let Some(p) = p.filter(|p| on_canvas(*p)) {
                if self.panning.is_some() || self.tool == Tool::Hand || held.pan {
                    ctx.set_cursor_icon(CursorIcon::Grab);
                } else if self.tool == Tool::Zoom || held.zoom || held.zoom_out {
                    ctx.set_cursor_icon(CursorIcon::ZoomIn);
                } else if self.tool == Tool::Picker || held.pick || self.area_mode != 0 {
                    ctx.set_cursor_icon(CursorIcon::Crosshair);
                } else {
                    ctx.set_cursor_icon(CursorIcon::None);
                    let radius = self.brush(self.pressure, self.pen_active).radius * self.zoom;
                    painter.circle_stroke(
                        p,
                        radius.max(0.5),
                        Stroke::new(2.5, Color32::from_white_alpha(230)),
                    );
                    painter.circle_stroke(
                        p,
                        radius.max(0.5),
                        Stroke::new(1., Color32::from_black_alpha(220)),
                    );
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
    pub(super) fn refresh_texture(&mut self, ctx: &Context) {
        if let Some([x0, y0, x1, y1]) = self.session.doc.update_display(&mut self.display) {
            if let Some(texture) = &mut self.texture {
                let pixels = (y0..y1)
                    .flat_map(|y| {
                        self.display
                            [y * self.session.doc.width + x0..y * self.session.doc.width + x1]
                            .iter()
                            .map(|p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, *p as u8))
                    })
                    .collect();
                texture.set_partial(
                    [x0, y0],
                    ColorImage::new([x1 - x0, y1 - y0], pixels),
                    TextureOptions::LINEAR,
                );
            } else {
                let pixels = self
                    .display
                    .iter()
                    .map(|p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, *p as u8))
                    .collect();
                self.texture = Some(ctx.load_texture(
                    "document",
                    ColorImage::new([self.session.doc.width, self.session.doc.height], pixels),
                    TextureOptions::LINEAR,
                ));
            }
        }
    }
}
