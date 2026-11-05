//! Temporary gesture previews. Only completion publishes a shared selection action.
use super::*;
pub(super) struct Gesture {
    kind: u8,
    target: (u64, u64),
    mode: &'static str,
    points: Vec<Pos2>,
    hover: Pos2,
    last_click: u32,
    step: f32,
    shift_eligible: bool,
}
impl Editor {
    pub(super) fn polygon_pending(&self) -> bool {
        self.selection_gesture.as_ref().is_some_and(|g| g.kind == 6)
    }
    pub(super) fn selection_down(&mut self, q: Pos2, mods: Modifiers, time: u32) -> bool {
        if !(3..=7).contains(&self.area_mode) {
            return false;
        }
        let q = bounded(q);
        if let Some(g) = self.selection_gesture.as_mut().filter(|g| g.kind == 6) {
            let close = g.points.len() >= 3
                && (q.distance(g.points[0]) * self.zoom <= 6.
                    || (q.distance(*g.points.last().unwrap()) * self.zoom <= 6.
                        && time.wrapping_sub(g.last_click) < 400));
            if close {
                self.selection_commit();
            } else if g.points.len() < 4096 {
                g.points.push(q);
                g.hover = q;
                g.last_click = time;
            } else {
                self.status = "Polygon selection is limited to 4096 points".into();
            }
            self.input_owned = true;
            return true;
        }
        let mode = match (mods.shift, mods.alt) {
            (true, true) => "intersect",
            (true, false) => "add",
            (false, true) => "subtract",
            _ => self.selection_combine,
        };
        if self.area_mode == 7 {
            // A contact selects once; movement and release must never paint.
            self.input_owned = true;
            if q.x >= 0.
                && q.y >= 0.
                && q.x < self.session.doc.width as f32
                && q.y < self.session.doc.height as f32
            {
                let options = self.session.selection_sampling();
                self.core("selection.wand", serde_json::json!({
                    "x":q.x.floor() as u32,"y":q.y.floor() as u32,"mode":mode,
                    "tolerance":options.tolerance,"contiguous":options.contiguous,"sample_merged":options.sample_merged
                }));
            }
            return true;
        }
        self.selection_gesture = Some(Gesture {
            kind: self.area_mode,
            target: (self.session.doc.identity, self.session.revision),
            mode,
            points: vec![q],
            hover: q,
            last_click: time,
            step: 1.,
            shift_eligible: !mods.shift,
        });
        self.input_owned = true;
        true
    }
    pub(super) fn selection_move(&mut self, q: Pos2, constrain: bool) -> bool {
        let Some(g) = self.selection_gesture.as_mut() else {
            return false;
        };
        let mut q = bounded(q);
        if !constrain {
            g.shift_eligible = true;
        }
        if g.kind <= 4 && constrain && g.shift_eligible {
            let delta = q - g.points[0];
            let side = delta.x.abs().max(delta.y.abs());
            q = g.points[0]
                + vec2(
                    side * if delta.x < 0. { -1. } else { 1. },
                    side * if delta.y < 0. { -1. } else { 1. },
                );
        }
        g.hover = q;
        if g.kind == 5 && q.distance(*g.points.last().unwrap()) * self.zoom >= g.step {
            if g.points.len() == 4095 {
                // Bounded, progressively sampled trace; preview and submitted
                // polygon use exactly the same retained vertices.
                let mut i = 0;
                g.points.retain(|_| {
                    let keep = i % 2 == 0;
                    i += 1;
                    keep
                });
                g.step *= 2.;
            }
            g.points.push(q);
        }
        true
    }
    pub(super) fn selection_release(&mut self) {
        if !self.polygon_pending() {
            self.selection_commit();
        }
        self.input_owned = false;
    }
    pub(super) fn selection_commit(&mut self) {
        let Some(mut g) = self.selection_gesture.take() else {
            return;
        };
        self.input_owned = false;
        if g.target != (self.session.doc.identity, self.session.revision) {
            return;
        }
        let args = if g.kind <= 4 {
            let r = Rect::from_two_pos(g.points[0], g.hover);
            if r.width() < 0.001 || r.height() < 0.001 {
                if g.mode == "replace" {
                    self.core("selection.deselect", serde_json::json!({}));
                }
                return;
            }
            serde_json::json!({"kind":if g.kind==3 {"rectangle"}else{"ellipse"},"mode":g.mode,"x":r.left(),"y":r.top(),"width":r.width(),"height":r.height()})
        } else {
            if g.kind == 5
                && g.points.len() < 4096
                && g.hover.distance(*g.points.last().unwrap()) > 0.001
            {
                g.points.push(g.hover);
            }
            if g.points.len() < 3 {
                if g.mode == "replace" {
                    self.core("selection.deselect", serde_json::json!({}));
                }
                return;
            }
            let points: Vec<_> = g.points.iter().map(|p| [p.x, p.y]).collect();
            serde_json::json!({"kind":"polygon","mode":g.mode,"points":points})
        };
        let mut action = crate::application::Action::new("selection.shape", args);
        action.expected_revision = Some(g.target.1);
        if let Err(e) = self.session.apply(action) {
            self.status = e.to_string();
        }
    }
    pub(super) fn selection_preview(&self, painter: &Painter) {
        let Some(g) = &self.selection_gesture else {
            return;
        };
        let screen = |q: Pos2| self.image_rect.min + q.to_vec2() * self.zoom;
        for stroke in [
            Stroke::new(3., Color32::BLACK),
            Stroke::new(1., Color32::WHITE),
        ] {
            if g.kind <= 4 {
                let rect = Rect::from_two_pos(screen(g.points[0]), screen(g.hover));
                if g.kind == 3 {
                    painter.rect_stroke(rect, 0., stroke, StrokeKind::Inside);
                } else {
                    let center = rect.center();
                    let radius = rect.size() * 0.5;
                    let points = (0..64)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::TAU / 64.;
                            center + vec2(a.cos() * radius.x, a.sin() * radius.y)
                        })
                        .collect();
                    painter.add(egui::epaint::PathShape::closed_line(points, stroke));
                }
            } else {
                let mut points: Vec<_> = g.points.iter().map(|p| screen(*p)).collect();
                points.push(screen(g.hover));
                painter.add(egui::epaint::PathShape::closed_line(points, stroke));
            }
        }
    }
    pub(super) fn selection_preview_valid(&self) -> bool {
        self.selection_gesture
            .as_ref()
            .is_none_or(|g| g.target == (self.session.doc.identity, self.session.revision))
    }
}
fn bounded(q: Pos2) -> Pos2 {
    pos2(q.x.clamp(-4096., 8192.), q.y.clamp(-4096., 8192.))
}

#[cfg(test)]
#[path = "selection_gesture_tests.rs"]
mod tests;
