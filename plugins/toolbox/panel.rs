//! Independent, responsive control bar. All controls dispatch registered actions.
use super::*;
impl Editor {
    pub(super) fn toolbox_ui(&mut self, ui: &mut Ui) {
        let mut tools: Vec<_> = self
            .session
            .registry
            .tools
            .values()
            .filter(|t| !t.preset)
            .cloned()
            .collect();
        tools.sort_by_key(|t| {
            (
                match t.kind {
                    Tool::Brush => 0,
                    Tool::Eraser => 1,
                    Tool::Picker => 2,
                    Tool::Hand => 3,
                    Tool::Zoom => 4,
                },
                t.id.clone(),
            )
        });
        let wide = ui.available_width() > 400.;
        let body = |ui: &mut Ui| {
            ui.spacing_mut().button_padding = vec2(
                self.settings.ui.item_spacing * 0.75,
                self.settings.ui.item_spacing * 0.5,
            );
            for tool in tools {
                let icon = match tool.kind {
                    Tool::Brush => 10,
                    Tool::Eraser => 11,
                    Tool::Picker => 12,
                    Tool::Hand => 13,
                    Tool::Zoom => 14,
                };
                let size = self.settings.ui.control_height + 2.;
                let selected = self.session.selected_tool == tool.id && self.area_mode == 0;
                if ui
                    .add_sized(
                        [size.min(ui.available_width().max(24.)), size],
                        Button::new(tr(theme::icon(
                            theme::action_icon(icon),
                            self.settings.ui.icon_size,
                        )))
                        .frame_when_inactive(selected)
                        .selected(selected),
                    )
                    .on_hover_text(tr(&tool.title))
                    .clicked()
                {
                    self.select_tool(&tool.id);
                }
            }
            if wide {
                let show_area_modes = ui.available_width() > 320.;
                if show_area_modes
                    && self.session.registry.actions.contains_key("layer.fill")
                    && ui
                        .selectable_label(self.area_mode == 1, tr("Flood Fill"))
                        .clicked()
                {
                    self.core("toolbox.mode.fill", serde_json::json!({}));
                }
                if show_area_modes
                    && self.session.registry.actions.contains_key("layer.gradient")
                    && ui
                        .selectable_label(self.area_mode == 2, tr("Gradient"))
                        .clicked()
                {
                    self.core("toolbox.mode.gradient", serde_json::json!({}));
                }
                ui.separator();
                let mut size = self.radius * 2.;
                ui.weak(tr("Size"));
                if ui
                    .add(DragValue::new(&mut size).range(1. ..=400.).suffix(" px"))
                    .changed()
                {
                    self.core("brush.set", serde_json::json!({"radius":size/2.}));
                }
                if ui.available_width() > 180. {
                    let mut opacity = self.opacity * 100.;
                    ui.weak(tr("Opacity"));
                    if ui
                        .add(DragValue::new(&mut opacity).range(0. ..=100.).suffix("%"))
                        .changed()
                    {
                        self.core("brush.set", serde_json::json!({"opacity":opacity/100.}));
                    }
                }
                if ui.available_width() > 240. {
                    let mut stabilizer = self.workspace.stabilizer * 100.;
                    ui.weak(tr("Stabilizer"));
                    if ui
                        .add(
                            DragValue::new(&mut stabilizer)
                                .range(0. ..=100.)
                                .suffix("%"),
                        )
                        .changed()
                    {
                        self.core(
                            "brush.set",
                            serde_json::json!({"stabilizer":stabilizer/100.}),
                        );
                    }
                }
            }
        };
        if wide {
            ui.horizontal_centered(body);
        } else {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical_centered(body);
                });
        }
    }
}
