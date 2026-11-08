//! Presentation controls dispatch the same scalar operation as automation.
use super::*;
impl Editor {
    pub(super) fn brush_symmetry_ui(&mut self, ui: &mut Ui) {
        let capable = self
            .session
            .registry
            .tool_option_capabilities(&self.session.selected_tool)["symmetry"]
            .as_bool()
            .unwrap_or(false);
        let enabled = self.area_mode == 0
            && matches!(self.tool, Tool::Brush | Tool::Eraser)
            && capable
            && !self.session.stroke.active()
            && self.session.pointer_checkpoint.is_none();
        ui.add_enabled_ui(enabled, |ui| {
            let response = ui.collapsing(tr("Brush Symmetry"), |ui| {
                let mut mode = self.session.parameters.symmetry;
                ui.horizontal_wrapped(|ui| {
                    for candidate in crate::application::SymmetryMode::ALL {
                        let response = ui.selectable_value(&mut mode, candidate, tr(candidate.title()));
                        #[cfg(test)]
                        ui.data_mut(|data| data.insert_temp(
                            Id::new(("brush-symmetry-mode", candidate.title())),
                            (response.rect, response.enabled()),
                        ));
                    }
                });
                let mut patch = serde_json::Map::new();
                if mode != self.session.parameters.symmetry {
                    patch.insert("symmetry".into(), serde_json::json!(mode));
                }
                for (id, title, current) in [
                    ("symmetry_x", "Symmetry X Anchor", self.session.parameters.symmetry_x),
                    ("symmetry_y", "Symmetry Y Anchor", self.session.parameters.symmetry_y),
                ] {
                    let mut value = current;
                    let response = ui.scope(|ui| parameter(ui, title, &mut value, 0. ..=1., "%", true, false)).response;
                    #[cfg(test)]
                    ui.data_mut(|data| data.insert_temp(
                        Id::new(("brush-symmetry-anchor", id)),
                        (response.rect, response.enabled()),
                    ));
                    if value != current { patch.insert(id.into(), serde_json::json!(value)); }
                }
                let reset = ui.button(tr("Center Symmetry Axes"));
                #[cfg(test)]
                ui.data_mut(|data| data.insert_temp(Id::new("brush-symmetry-reset"), (reset.rect, reset.enabled())));
                if reset.clicked() {
                    patch.insert("symmetry_x".into(), serde_json::json!(0.5));
                    patch.insert("symmetry_y".into(), serde_json::json!(0.5));
                }
                ui.add(Label::new(tr("Axes use document coordinates. Overlapping marks share the selected opacity mode.")).wrap());
                if !patch.is_empty() { self.core("brush.set", serde_json::Value::Object(patch)); }
            });
            #[cfg(test)]
            ui.data_mut(|data| data.insert_temp(Id::new("brush-symmetry-header"), response.header_response.rect));
        }).response.on_disabled_hover_text(tr("Symmetry needs a supporting Brush or Eraser and idle input."));
    }
}

#[cfg(test)]
#[path = "symmetry_ui_tests.rs"]
mod tests;
