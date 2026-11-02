//! Canvas gestures adapt to the same plugin actions used by CLI and MCP.
use super::*;
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
                ui.label(tr("Click the canvas to fill a connected area."));
                ui.horizontal(|ui| {
                    ui.label(tr("Fill Tolerance"));
                    ui.add(DragValue::new(&mut self.area_tolerance).range(0..=255));
                });
            }
            2 => {
                ui.label(tr(
                    "Drag on the canvas from foreground to background color.",
                ));
                ui.checkbox(&mut self.area_radial, tr("Radial Gradient"));
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
            self.core("layer.fill",serde_json::json!({"x":q.x.floor() as u32,"y":q.y.floor() as u32,"color":self.session.parameters.color,"opacity":self.opacity,"tolerance":self.area_tolerance}));
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
                self.core("layer.gradient",serde_json::json!({"x0":a.x,"y0":a.y,"x1":b.x,"y1":b.y,"start_color":self.session.parameters.color,"end_color":self.session.parameters.background_color,"opacity":self.opacity,"kind":if self.area_radial {"radial"} else {"linear"}}));
            }
        }
    }
}
