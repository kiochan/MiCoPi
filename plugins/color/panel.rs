//! Feature-owned presentation adapter; document changes use shared actions.
use super::panels::section;
use super::*;
impl Editor {
    pub(super) fn color_ui(&mut self, ui: &mut Ui) {
        let gap = self.settings.ui.section_spacing;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(38., 30.), Sense::hover());
            ui.painter().rect_filled(
                r.translate(vec2(7., 4.)),
                4.,
                Color32::from_rgb(
                    self.background_ink[0],
                    self.background_ink[1],
                    self.background_ink[2],
                ),
            );
            ui.painter().rect_stroke(
                r.translate(vec2(7., 4.)),
                4.,
                Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color),
                StrokeKind::Inside,
            );
            ui.painter().rect_filled(
                r,
                4.,
                Color32::from_rgb(self.ink[0], self.ink[1], self.ink[2]),
            );
            ui.add_space(8.);
            ui.label(tr(RichText::new(localize(format!(
                "#{:02X}{:02X}{:02X}",
                self.ink[0], self.ink[1], self.ink[2]
            )))
            .monospace()
            .small()));
        });
        ui.add_space(gap * 0.5);
        self.color_wheel(ui);
        section(ui, "Swatches", gap);
        let colors = [
            0x252830, 0xffffff, 0x929aa8, 0xefe3cf, 0xe66b72, 0xed9b62, 0xe8ca76, 0xb1c978,
            0x75b89b, 0x71bcc5, 0x7699d4, 0x9990d3, 0xc68cc2, 0x8c5854, 0x6d7b62, 0x465e77,
        ];
        let swatch_gap = self.settings.ui.item_spacing * 0.5;
        let side = ((ui.available_width() - 7. * swatch_gap) / 8.)
            .clamp(14., self.settings.ui.control_height);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(swatch_gap);
            for row in colors.chunks(8) {
                ui.horizontal(|ui| {
                    for &c in row {
                        let (r, response) =
                            ui.allocate_exact_size(Vec2::splat(side), Sense::click());
                        ui.painter().rect_filled(r, 3., theme::rgb(c));
                        if response.hovered() {
                            ui.painter().rect_stroke(
                                r,
                                3.,
                                Stroke::new(1.5, ui.visuals().text_color()),
                                StrokeKind::Inside,
                            );
                        }
                        if response.clicked() {
                            self.ink = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
                        }
                        response.on_hover_text(tr(format!("#{c:06X}")));
                    }
                });
            }
        });
        CollapsingHeader::new(tr("RGB Values"))
            .id_salt("rgb-channels")
            .show(ui, |ui| {
                for (i, name) in ["R", "G", "B"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(tr(name));
                        ui.add(Slider::new(&mut self.ink[i], 0..=255));
                    });
                }
            });
    }
    fn color_wheel(&mut self, ui: &mut Ui) {
        use crate::engine::color::{hsv, square, to_hsv, wheel};
        let color = u32::from_be_bytes([0, self.ink[0], self.ink[1], self.ink[2]]);
        let (h, mut saturation, mut value) = to_hsv(color);
        if self.color_last != self.ink && saturation > 0.001 {
            self.color_hue = h;
        }
        let size = ui.available_width().min(160.).max(128.).round() as usize;
        let (slot, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), size as f32), Sense::hover());
        let r = Rect::from_center_size(slot.center(), Vec2::splat(size as f32));
        let response = ui.interact(r, ui.id().with("hsv-wheel"), Sense::click_and_drag());
        let (offset, side) = square(size as i32);
        if let Some(p) = response
            .interact_pointer_pos()
            .filter(|_| response.is_pointer_button_down_on() || response.clicked())
        {
            let d = p - r.center();
            let ring = *self
                .wheel_drag
                .get_or_insert(d.length() > size as f32 * 0.5 - 20.);
            if ring {
                self.color_hue = (d.y.atan2(d.x) / std::f32::consts::TAU).rem_euclid(1.);
            } else {
                saturation = ((p.x - r.left() - offset as f32) / (side - 1) as f32).clamp(0., 1.);
                value = 1. - ((p.y - r.top() - offset as f32) / (side - 1) as f32).clamp(0., 1.);
            }
            let rgb = hsv(self.color_hue, saturation, value);
            self.ink = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
        }
        if !response.is_pointer_button_down_on() {
            self.wheel_drag = None;
        }
        self.color_last = self.ink;
        let bg = theme::Colors::new(&self.settings.ui).panel;
        let background = u32::from_be_bytes([0, bg.r(), bg.g(), bg.b()]);
        let key = (size, (self.color_hue * 65535.) as u32, background);
        if self.wheel_key != Some(key) {
            let mut pixels = vec![];
            wheel(&mut pixels, size, self.color_hue, background);
            let image = ColorImage::new(
                [size, size],
                pixels.into_iter().map(|p| theme::rgb(p)).collect(),
            );
            if let Some(texture) = &mut self.wheel_texture {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                self.wheel_texture = Some(ui.ctx().load_texture(
                    "color-wheel",
                    image,
                    TextureOptions::LINEAR,
                ));
            }
            self.wheel_key = Some(key);
        }
        ui.painter().image(
            self.wheel_texture.as_ref().unwrap().id(),
            r,
            Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
            Color32::WHITE,
        );
        let angle = self.color_hue * std::f32::consts::TAU;
        let hue_point = r.center() + vec2(angle.cos(), angle.sin()) * (size as f32 * 0.5 - 9.);
        let sv_point = r.min
            + vec2(
                offset as f32 + saturation * (side - 1) as f32,
                offset as f32 + (1. - value) * (side - 1) as f32,
            );
        for p in [hue_point, sv_point] {
            ui.painter()
                .circle_stroke(p, 4., Stroke::new(2., Color32::WHITE));
            ui.painter()
                .circle_stroke(p, 5., Stroke::new(1., Color32::BLACK));
        }
    }
}
