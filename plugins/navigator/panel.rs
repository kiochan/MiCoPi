//! Feature-owned presentation adapter; document changes use shared actions.
use super::panels::icon_button;
use super::*;
impl Editor {
    pub(super) fn navigator_ui(&mut self, ui: &mut Ui) {
        self.refresh_texture(ui.ctx());
        let width = ui.available_width();
        let reserve = self.settings.ui.control_height
            + ui.text_style_height(&TextStyle::Body)
            + self.settings.ui.item_spacing * 4.;
        let height = (width * self.session.doc.height as f32 / self.session.doc.width as f32)
            .min(150.)
            .min((ui.available_height() - reserve).max(40.));
        let (r, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let inner = Rect::from_center_size(
            r.center(),
            vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * ((r.width() - 4.) / self.session.doc.width as f32)
                .min((r.height() - 4.) / self.session.doc.height as f32),
        );
        ui.painter()
            .rect_filled(r, 4., theme::Colors::new(&self.settings.ui).background);
        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                inner,
                Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        let scale = inner.width() / self.session.doc.width as f32;
        if self.canvas.is_positive() {
            let min = self.document_position(self.canvas.min);
            let max = self.document_position(self.canvas.max);
            let view = Rect::from_min_max(
                inner.min + min.to_vec2() * scale,
                inner.min + max.to_vec2() * scale,
            )
            .intersect(inner);
            ui.painter().rect_stroke(
                view,
                0.,
                Stroke::new(1., theme::Colors::new(&self.settings.ui).accent),
                StrokeKind::Inside,
            );
        }
        if let Some(p) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() || response.dragged())
        {
            let q = (p - inner.min) / scale;
            self.fit = false;
            self.pan = (vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * 0.5
                - q)
                * self.zoom;
        }
        ui.add_space(self.settings.ui.item_spacing);
        ui.horizontal(|ui| {
            for id in [86, 40, 85] {
                if icon_button(ui, self, id, false).clicked() {
                    self.actions.push(match id {
                        86 => Action::ZoomOut,
                        40 => Action::Fit,
                        _ => Action::ZoomIn,
                    });
                }
            }
            ui.label(tr(RichText::new(localize(format!(
                "{:.1}%",
                self.zoom * 100.
            )))
            .small()));
        });
        ui.weak(tr(format!(
            "{} × {} px",
            self.session.doc.width, self.session.doc.height
        )));
    }
}
