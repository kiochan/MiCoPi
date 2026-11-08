//! Nominal document-space symmetry feedback, independent of native pixel work.
use super::*;
impl Editor {
    pub(super) fn paint_symmetry_guides(&self, painter: &Painter, color: Color32) {
        let p = &self.session.parameters;
        if p.symmetry.is_off()
            || self.area_mode != 0
            || !matches!(self.tool, Tool::Brush | Tool::Eraser)
        {
            return;
        }
        let width = self.session.doc.width as f32;
        let height = self.session.doc.height as f32;
        let clip = painter.with_clip_rect(painter.clip_rect().intersect(self.image_rect));
        for (bit, a, b) in [
            (
                1,
                pos2(p.symmetry_x * width, 0.),
                pos2(p.symmetry_x * width, height),
            ),
            (
                2,
                pos2(0., p.symmetry_y * height),
                pos2(width, p.symmetry_y * height),
            ),
        ] {
            if p.symmetry.axes() & bit != 0 {
                let points = [
                    self.document_screen_position(a),
                    self.document_screen_position(b),
                ];
                clip.line_segment(points, Stroke::new(1., color.gamma_multiply(0.55)));
            }
        }
    }
    pub(super) fn symmetry_cursor_centers(&self, p: Pos2) -> ([(Pos2, f32); 4], usize) {
        let parameters = &self.session.parameters;
        let original = self.document_position(p);
        let axes = parameters.symmetry.axes();
        let mut output = [(p, 0.); 4];
        let mut count = 0;
        let symmetric_tip = parameters.tip_shape == crate::application::TipShape::Round
            && parameters.tip_roundness == 1.
            || parameters.tip_angle.rem_euclid(90.) == 0.
            || parameters.tip_shape == crate::application::TipShape::Square
                && parameters.tip_roundness == 1.
                && parameters.tip_angle.rem_euclid(45.) == 0.;
        for arm in 0u32..4 {
            if arm & !axes != 0 {
                continue;
            }
            let q = pos2(
                if arm & 1 != 0 {
                    2. * parameters.symmetry_x * self.session.doc.width as f32 - original.x
                } else {
                    original.x
                },
                if arm & 2 != 0 {
                    2. * parameters.symmetry_y * self.session.doc.height as f32 - original.y
                } else {
                    original.y
                },
            );
            let native_odd = arm == 1 || arm == 2;
            let angle = if native_odd && !symmetric_tip {
                -parameters.tip_angle
            } else {
                parameters.tip_angle
            };
            let angle = if self.session.view.mirror_x != self.session.view.mirror_y {
                -angle
            } else {
                angle
            };
            let value = (self.document_screen_position(q), angle);
            if !output[..count].contains(&value) {
                output[count] = value;
                count += 1;
            }
        }
        (output, count)
    }
    pub(super) fn paint_symmetry_cursors(
        &self,
        ctx: &Context,
        painter: &Painter,
        p: Pos2,
        radius: f32,
    ) {
        let parameters = &self.session.parameters;
        let (centers, count) = self.symmetry_cursor_centers(p);
        for (center, angle) in &centers[..count] {
            let contour = super::super::brush::tip_geometry::cached(
                ctx,
                if *angle == parameters.tip_angle {
                    "brush-tip-cursor"
                } else {
                    "brush-tip-cursor-reflected"
                },
                parameters.tip_shape,
                parameters.tip_roundness,
                *angle,
                radius,
            );
            contour.outline(
                painter,
                *center,
                radius,
                Stroke::new(2.5, Color32::from_white_alpha(230)),
            );
            contour.outline(
                painter,
                *center,
                radius,
                Stroke::new(1., Color32::from_black_alpha(220)),
            );
        }
    }
}
