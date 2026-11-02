//! Canvas gestures adapt to the same plugin actions used by CLI and MCP.
use super::*;
pub(super) fn snap_gradient(start: Pos2, end: Pos2) -> Pos2 {
    let delta = end - start;
    let distance = delta.length();
    if distance == 0. {
        return start;
    }
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (delta.y.atan2(delta.x) / step).round() * step;
    let (y, x) = angle.sin_cos();
    start + vec2(x, y) * distance
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_angle_snapping_preserves_distance_in_every_octant() {
        let start = pos2(12., -8.);
        assert_eq!(snap_gradient(start, start), start);
        for i in 0..64 {
            let angle = i as f32 * std::f32::consts::TAU / 64.;
            let end = start + vec2(angle.cos(), angle.sin()) * 25.;
            let snapped = snap_gradient(start, end) - start;
            assert!((snapped.length() - 25.).abs() < 0.0001);
            assert!(
                snapped.x.abs() < 0.0001
                    || snapped.y.abs() < 0.0001
                    || (snapped.x.abs() - snapped.y.abs()).abs() < 0.0001
            );
        }
    }
}
impl Editor {
    pub(super) fn area_ui(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            for (mode, title, action) in [
                (0, "Brush Tool", ""),
                (1, "Flood Fill", "layer.fill"),
                (2, "Gradient", "layer.gradient"),
            ] {
                if ui
                    .add_enabled(
                        mode == 0 || self.session.registry.actions.contains_key(action),
                        Button::new(tr(title)).selected(self.area_mode == mode),
                    )
                    .clicked()
                {
                    self.finish_stroke();
                    self.select_tool("brush");
                    self.area_mode = mode;
                }
            }
        });
        match self.area_mode {
            1 => {
                ui.label(tr(if self.area_contiguous {
                    "Click the canvas to fill a connected area."
                } else {
                    "Click the canvas to fill matching colors across this layer."
                }));
                ui.horizontal(|ui| {
                    ui.label(tr("Fill Tolerance"));
                    ui.add(DragValue::new(&mut self.area_tolerance).range(0..=255));
                });
                ui.checkbox(&mut self.area_contiguous, tr("Contiguous Fill"));
            }
            2 => {
                ui.label(tr(
                    "Drag on the canvas to set gradient direction and length.",
                ));
                ui.checkbox(&mut self.area_radial, tr("Radial Gradient"));
                ComboBox::from_id_salt("gradient-spread")
                    .width(ui.available_width())
                    .selected_text(tr(match self.area_spread {
                        "repeat" => "Repeat Gradient",
                        "reflect" => "Reflect Gradient",
                        _ => "Extend End Colors",
                    }))
                    .show_ui(ui, |ui| {
                        for (id, title) in [
                            ("pad", "Extend End Colors"),
                            ("repeat", "Repeat Gradient"),
                            ("reflect", "Reflect Gradient"),
                        ] {
                            ui.selectable_value(&mut self.area_spread, id, tr(title));
                        }
                    });
                ui.checkbox(&mut self.area_reverse, tr("Reverse Gradient"));
                ui.checkbox(&mut self.area_transparent, tr("Transparent Gradient End"));
                ui.weak(tr("Hold Shift to constrain the gradient to 45° steps."));
            }
            _ => {}
        }
    }
    pub(super) fn area_down(&mut self, q: Pos2) -> bool {
        if self.area_mode == 0 {
            return false;
        }
        if q.x < 0.
            || q.y < 0.
            || q.x >= self.session.doc.width as f32
            || q.y >= self.session.doc.height as f32
        {
            return true;
        }
        if self.area_mode == 1 {
            self.core("layer.fill",serde_json::json!({"x":q.x.floor() as u32,"y":q.y.floor() as u32,"color":self.session.parameters.color,"opacity":self.opacity,"tolerance":self.area_tolerance,"contiguous":self.area_contiguous}));
        } else {
            self.area_drag = Some((q, q));
            self.area_target = Some((
                self.session.doc.identity,
                self.session.doc.layer_id(self.session.doc.active),
                self.session.revision,
            ));
            self.input_owned = true;
        }
        true
    }
    pub(super) fn area_up(&mut self) {
        if let Some((a, b)) = self.area_drag.take() {
            let target = self.area_target.take();
            if target
                != Some((
                    self.session.doc.identity,
                    self.session.doc.layer_id(self.session.doc.active),
                    self.session.revision,
                ))
            {
                return;
            }
            if a.distance(b) >= 0.001 {
                let end = if self.area_transparent {
                    format!("{}00", self.session.parameters.background_color)
                } else {
                    self.session.parameters.background_color.clone()
                };
                self.core("layer.gradient",serde_json::json!({"x0":a.x,"y0":a.y,"x1":b.x,"y1":b.y,"start_color":self.session.parameters.color,"end_color":end,"opacity":self.opacity,"kind":if self.area_radial {"radial"} else {"linear"},"spread":self.area_spread,"reverse":self.area_reverse}));
            }
        }
    }
}
