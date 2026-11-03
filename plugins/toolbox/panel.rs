//! Toolbox presentation adapter. Algorithms and document edits remain registered actions.
use super::*;
impl Editor {
    pub(super) fn toolbox_ui(&mut self, ui: &mut Ui) {
        let tools: Vec<_> = self
            .session
            .registry
            .tools
            .values()
            .filter(|t| !t.preset)
            .cloned()
            .collect();
        ui.vertical_centered(|ui| {
            for tool in tools {
                let icon = match tool.kind {
                    Tool::Brush => 10,
                    Tool::Eraser => 11,
                    Tool::Picker => 12,
                    Tool::Hand => 13,
                    Tool::Zoom => 14,
                };
                if ui
                    .add_sized(
                        [
                            self.settings.ui.control_height + 8.,
                            self.settings.ui.control_height + 8.,
                        ],
                        Button::new(tr(theme::icon(
                            theme::action_icon(icon),
                            self.settings.ui.icon_size,
                        )))
                        .selected(self.session.selected_tool == tool.id),
                    )
                    .on_hover_text(tr(&tool.title))
                    .clicked()
                {
                    self.select_tool(&tool.id);
                }
            }
        });
    }
}
