//! Icon-only Tools strip and a separate Tool Options bar. All controls dispatch
//! registered actions; order and visibility come from `ui.tool_order` and
//! `ui.hidden_tools`.
use super::*;

/// One entry in the Tools strip: a registered tool or an area mode.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ToolbarItem {
    pub id: String,
    pub title: String,
    pub icon: char,
    /// Plugin whose locale scope translates the title; `None` for built-in modes.
    pub owner: Option<String>,
}
const FILL: &str = "fill";
const GRADIENT: &str = "gradient";

impl Editor {
    /// Every available Tools-strip entry in its default order, ignoring settings.
    pub(crate) fn toolbar_catalog(&self) -> Vec<ToolbarItem> {
        let mut tools: Vec<_> = self
            .session
            .registry
            .tools
            .values()
            .filter(|t| !t.preset)
            .cloned()
            .collect();
        let rank = |kind: Tool| match kind {
            Tool::Brush => 0,
            Tool::Eraser => 1,
            Tool::Picker => 4,
            Tool::Hand => 5,
            Tool::Zoom => 6,
        };
        tools.sort_by_key(|t| (rank(t.kind), t.id.clone()));
        let mut items: Vec<(u8, ToolbarItem)> = tools
            .into_iter()
            .map(|t| {
                (
                    rank(t.kind),
                    ToolbarItem {
                        icon: theme::action_icon(match t.kind {
                            Tool::Brush => 10,
                            Tool::Eraser => 11,
                            Tool::Picker => 12,
                            Tool::Hand => 13,
                            Tool::Zoom => 14,
                        }),
                        owner: self
                            .session
                            .registry
                            .contribution_owner("tools", &t.id)
                            .map(str::to_owned),
                        id: t.id,
                        title: t.title,
                    },
                )
            })
            .collect();
        for (rank, id, action, title, icon) in [
            (2, FILL, "layer.fill", "Flood Fill", '\u{e23a}'),
            (3, GRADIENT, "layer.gradient", "Gradient", '\u{e3e9}'),
        ] {
            if self.session.registry.actions.contains_key(action) {
                items.push((
                    rank,
                    ToolbarItem {
                        id: id.into(),
                        title: title.into(),
                        icon,
                        owner: None,
                    },
                ));
            }
        }
        items.sort_by_key(|(rank, _)| *rank);
        items.into_iter().map(|(_, item)| item).collect()
    }
    /// The catalog arranged by `ui.tool_order`; hidden entries are kept so the
    /// settings editor can list them.
    pub(crate) fn toolbar_ordered(&self) -> Vec<ToolbarItem> {
        let mut rest = self.toolbar_catalog();
        let mut ordered = vec![];
        for id in &self.settings.ui.tool_order {
            if let Some(index) = rest.iter().position(|item| &item.id == id) {
                ordered.push(rest.remove(index));
            }
        }
        ordered.extend(rest);
        ordered
    }
    fn toolbar_selected(&self, id: &str) -> bool {
        match id {
            FILL => self.area_mode == 1,
            GRADIENT => self.area_mode == 2,
            _ => self.area_mode == 0 && self.session.selected_tool == id,
        }
    }
    fn toolbar_activate(&mut self, id: &str) {
        match id {
            FILL => {
                self.core("toolbox.mode.fill", serde_json::json!({}));
            }
            GRADIENT => {
                self.core("toolbox.mode.gradient", serde_json::json!({}));
            }
            _ => self.select_tool(id),
        }
    }
    pub(super) fn toolbox_ui(&mut self, ui: &mut Ui) {
        let a = self.settings.ui.clone();
        let items: Vec<_> = self
            .toolbar_ordered()
            .into_iter()
            .filter(|item| !a.hidden_tools.contains(&item.id))
            .collect();
        let size = a.control_height + 2.;
        let horizontal = ui.available_width() >= ui.available_height();
        let buttons = |ui: &mut Ui, editor: &mut Editor| {
            ui.spacing_mut().item_spacing = Vec2::splat(a.item_spacing);
            for item in &items {
                let selected = editor.toolbar_selected(&item.id);
                if ui
                    .add_sized(
                        [size, size],
                        Button::new(tr(theme::icon(item.icon, a.icon_size)))
                            .frame_when_inactive(selected)
                            .corner_radius(a.corner_radius)
                            .selected(selected),
                    )
                    .on_hover_text(tr_for(item.owner.as_deref(), &item.title))
                    .clicked()
                {
                    editor.toolbar_activate(&item.id);
                }
            }
        };
        // Scroll only along the strip so the cross axis stays bounded for centring.
        let hidden = egui::scroll_area::ScrollBarVisibility::AlwaysHidden;
        if horizontal {
            ScrollArea::horizontal()
                .auto_shrink([false, false])
                .scroll_bar_visibility(hidden)
                .show(ui, |ui| ui.horizontal_centered(|ui| buttons(ui, self)));
        } else {
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(hidden)
                .show(ui, |ui| ui.vertical_centered(|ui| buttons(ui, self)));
        }
    }
    /// Options for the active tool: its identity, then size, opacity and stabilizer.
    pub(super) fn tool_options_ui(&mut self, ui: &mut Ui) {
        let a = self.settings.ui.clone();
        let c = theme::Colors::new(&a);
        let active = self
            .toolbar_catalog()
            .into_iter()
            .find(|item| self.toolbar_selected(&item.id));
        let body = |ui: &mut Ui, editor: &mut Editor| {
            ui.spacing_mut().item_spacing.x = a.item_spacing * 1.5;
            if let Some(item) = &active {
                ui.label(tr(theme::icon(item.icon, a.icon_size).color(c.accent_text)));
                ui.label(
                    tr_for(item.owner.as_deref(), item.title.as_str())
                        .font(theme::strong(ui.ctx(), a.font_size)),
                );
                ui.separator();
            }
            let mut size = editor.radius * 2.;
            ui.weak(tr("Size"));
            if ui
                .add(DragValue::new(&mut size).range(1. ..=400.).suffix(" px"))
                .changed()
            {
                editor.core("brush.set", serde_json::json!({"radius":size/2.}));
            }
            let mut opacity = editor.opacity * 100.;
            ui.weak(tr("Opacity"));
            if ui
                .add(DragValue::new(&mut opacity).range(0. ..=100.).suffix("%"))
                .changed()
            {
                editor.core("brush.set", serde_json::json!({"opacity":opacity/100.}));
            }
            let mut stabilizer = editor.workspace.stabilizer * 100.;
            ui.weak(tr("Stabilizer"));
            if ui
                .add(
                    DragValue::new(&mut stabilizer)
                        .range(0. ..=100.)
                        .suffix("%"),
                )
                .changed()
            {
                editor.core(
                    "brush.set",
                    serde_json::json!({"stabilizer":stabilizer/100.}),
                );
            }
        };
        ScrollArea::horizontal()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| ui.horizontal_centered(|ui| body(ui, self)));
    }
}
