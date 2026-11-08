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
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(Id::new("navigator-image"), inner));
        if self.canvas.is_positive() {
            let min = self.document_position(self.canvas.min);
            let max = self.document_position(self.canvas.max);
            let view = Rect::from_two_pos(
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
            let mut q = (p - inner.min) / scale;
            if self.session.view.mirror_x {
                q.x = self.session.doc.width as f32 - q.x;
            }
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

#[cfg(test)]
mod mirror_tests {
    use super::*;
    #[test]
    fn navigator_click_centers_the_same_document_point_in_both_view_orientations() {
        for mirrored in [false, true] {
            let mut app = crate::desktop::tests::studio();
            let ctx = Context::default();
            app.editor
                .core("canvas.new", serde_json::json!({"width":12,"height":8}));
            app.editor.zoom = 3.;
            app.editor.fit = false;
            app.editor.session.view.mirror_x = mirrored;
            app.editor.canvas = Rect::from_min_size(pos2(0., 0.), vec2(1000., 800.));
            app.editor.image_rect =
                Rect::from_center_size(app.editor.canvas.center(), vec2(36., 24.));
            let mut frame = |events| {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(320., 360.))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| app.editor.navigator_ui(ui));
                    },
                );
            };
            frame(vec![]);
            frame(vec![]);
            let image =
                ctx.data_mut(|data| data.get_temp::<Rect>(Id::new("navigator-image")).unwrap());
            let p = pos2(image.left() + image.width() * 0.25, image.center().y);
            for pressed in [true, false] {
                frame(vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ]);
            }
            assert!((app.editor.pan.x - if mirrored { -9. } else { 9. }).abs() < 0.01);
            assert!(app.editor.pan.y.abs() < 0.01);
        }
    }
}
