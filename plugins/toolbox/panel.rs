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
fn selection_mode(id: &str) -> Option<u8> {
    match id {
        "selection_rectangle" => Some(3),
        "selection_ellipse" => Some(4),
        "selection_lasso" => Some(5),
        "selection_polygon" => Some(6),
        "selection_wand" => Some(7),
        _ => None,
    }
}

fn selection_icon(painter: &Painter, r: Rect, mode: u8, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    match mode {
        3 => {
            painter.rect_stroke(r, 0., stroke, StrokeKind::Inside);
        }
        7 => {
            let c = r.min + r.size() * vec2(0.75, 0.25);
            painter.line_segment([r.left_bottom(), c], stroke);
            for d in [vec2(0., 3.), vec2(3., 0.)] {
                painter.line_segment([c - d, c + d], stroke);
            }
        }
        4 => {
            painter.circle_stroke(r.center(), r.width().min(r.height()) * 0.5, stroke);
        }
        _ => {
            let points: &[[f32; 2]] = if mode == 5 {
                &[
                    [0., 0.5],
                    [0.15, 0.1],
                    [0.7, 0.],
                    [1., 0.4],
                    [0.7, 0.8],
                    [0.2, 0.85],
                    [0.15, 0.6],
                    [0.5, 0.55],
                    [0.4, 1.],
                ]
            } else {
                &[
                    [0., 0.],
                    [1., 0.2],
                    [0.8, 0.85],
                    [0.15, 0.7],
                    [0., 0.],
                    [0.4, 1.],
                ]
            };
            painter.add(egui::epaint::PathShape::line(
                points
                    .iter()
                    .map(|q| r.min + vec2(q[0] * r.width(), q[1] * r.height()))
                    .collect(),
                stroke,
            ));
        }
    }
}

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
            Tool::Picker => 9,
            Tool::Hand => 10,
            Tool::Zoom => 11,
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
            (7, FILL, "layer.fill", "Flood Fill", '\u{e23a}'),
            (8, GRADIENT, "layer.gradient", "Gradient", '\u{e3e9}'),
            (
                2,
                "selection_rectangle",
                "selection.shape",
                "Rectangular Marquee",
                '\u{e3c2}',
            ),
            (
                3,
                "selection_ellipse",
                "selection.shape",
                "Elliptical Marquee",
                '\u{e3c2}',
            ),
            (
                6,
                "selection_wand",
                "selection.wand",
                "Magic Wand",
                '\u{e3c2}',
            ),
            (4, "selection_lasso", "selection.shape", "Lasso", '\u{e3c2}'),
            (
                5,
                "selection_polygon",
                "selection.shape",
                "Polygonal Lasso",
                '\u{e3c2}',
            ),
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
        if let Some(mode) = selection_mode(id) {
            return self.area_mode == mode;
        }
        match id {
            FILL => self.area_mode == 1,
            GRADIENT => self.area_mode == 2,
            _ => self.area_mode == 0 && self.session.selected_tool == id,
        }
    }
    fn toolbar_activate(&mut self, id: &str) {
        if selection_mode(id).is_some() {
            self.core(&format!("toolbox.mode.{id}"), serde_json::json!({}));
            return;
        }
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
                let selection = selection_mode(&item.id);
                let response = ui
                    .add_sized(
                        [size, size],
                        Button::new(if selection.is_some() {
                            RichText::new("")
                        } else {
                            tr(theme::icon(item.icon, a.icon_size))
                        })
                        .frame_when_inactive(selected)
                        .corner_radius(a.corner_radius)
                        .selected(selected),
                    )
                    .on_hover_text(tr_for(item.owner.as_deref(), &item.title));
                if let Some(mode) = selection {
                    let inset = (a.item_spacing * 0.5).clamp(2., 6.);
                    let r = Rect::from_center_size(
                        response.rect.center(),
                        Vec2::splat(a.icon_size.min((size - inset * 2.).max(1.))),
                    );
                    selection_icon(ui.painter(), r, mode, ui.visuals().text_color());
                }
                if response.clicked() {
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
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            ui.spacing_mut().item_spacing.x = a.item_spacing * 1.5;
            if let Some(item) = &active {
                if let Some(mode) = selection_mode(&item.id) {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(a.icon_size), Sense::hover());
                    selection_icon(ui.painter(), r, mode, c.accent_text);
                } else {
                    ui.label(tr(theme::icon(item.icon, a.icon_size).color(c.accent_text)));
                }
                ui.label(
                    tr_for(item.owner.as_deref(), item.title.as_str())
                        .font(theme::strong(ui.ctx(), a.font_size)),
                );
                ui.separator();
            }
            if editor.area_mode >= 3 {
                for (mode, title) in [
                    ("replace", "New Selection"),
                    ("add", "Add to Selection"),
                    ("subtract", "Subtract from Selection"),
                    ("intersect", "Intersect Selection"),
                ] {
                    let selected = editor.selection_combine == mode;
                    let response = ui
                        .add_sized(
                            Vec2::splat(a.control_height),
                            Button::new("").selected(selected),
                        )
                        .on_hover_text(tr(title));
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::SelectableLabel,
                            true,
                            selected,
                            crate::i18n::text(title),
                        )
                    });
                    let r = response.rect.shrink(6.);
                    let color = if selected {
                        c.accent_text
                    } else {
                        ui.visuals().text_color()
                    };
                    let stroke = Stroke::new(1., color);
                    if mode == "replace" {
                        ui.painter().rect_stroke(r, 0., stroke, StrokeKind::Inside);
                    } else {
                        let first = Rect::from_min_size(r.min, r.size() * 0.7);
                        let second = Rect::from_min_max(r.max - r.size() * 0.7, r.max);
                        ui.painter()
                            .rect_stroke(first, 0., stroke, StrokeKind::Inside);
                        ui.painter()
                            .rect_stroke(second, 0., stroke, StrokeKind::Inside);
                        if mode == "intersect" {
                            ui.painter().rect_filled(first.intersect(second), 0., color);
                        } else {
                            let center = second.right_top();
                            ui.painter().line_segment(
                                [center - vec2(2., 0.), center + vec2(2., 0.)],
                                stroke,
                            );
                            if mode == "add" {
                                ui.painter().line_segment(
                                    [center - vec2(0., 2.), center + vec2(0., 2.)],
                                    stroke,
                                );
                            }
                        }
                    }
                    if response.clicked() {
                        editor.core(&format!("toolbox.selection.{mode}"), serde_json::json!({}));
                    }
                }
                if editor.area_mode == 7 {
                    let mut options = editor.session.selection_sampling();
                    let before = options;
                    ui.separator();
                    ui.weak(tr("Tolerance"))
                        .on_hover_text(tr("Selection Tolerance"));
                    ui.add(DragValue::new(&mut options.tolerance).range(0..=255));
                    ui.checkbox(&mut options.contiguous, tr("Contiguous"));
                    ui.checkbox(&mut options.sample_merged, tr("Sample Merged"));
                    if options != before {
                        editor.core(
                            "toolbox.selection.options",
                            serde_json::to_value(options).unwrap(),
                        );
                    }
                }
                ui.menu_button(RichText::new("…").size(a.icon_size), |ui| {
                    if editor.area_mode == 7 {
                        let options=editor.session.selection_sampling();
                        for (id,title) in [("selection.grow","Grow Selection"),("selection.similar","Select Similar")] {
                            if ui.add_enabled(editor.session.doc.selection().current.is_some(),Button::new(tr(title))).clicked() {
                                editor.core(id,serde_json::json!({"tolerance":options.tolerance,"sample_merged":options.sample_merged}));
                                ui.close();
                            }
                        }
                        ui.separator();
                    }
                    for (id,title) in [("selection.expand","Expand Selection"),("selection.contract","Contract Selection"),("selection.feather","Feather Selection"),("selection.smooth","Smooth Selection")] {
                        if ui.add_enabled(editor.session.doc.selection().current.is_some(),Button::new(tr(title))).clicked() {
                            editor.begin_plugin_command(id);
                            ui.close();
                        }
                    }
                }).response.on_hover_text(tr("Selection Actions"));

                ui.label(tr(theme::icon('\u{e312}', a.icon_size).color(c.muted)))
                    .on_hover_text(tr(if editor.area_mode == 7 {
                        "Click to select a color. Shift adds, Alt subtracts, Shift+Alt intersects."
                    } else if editor.area_mode == 6 {
                        "Click vertices; Enter or double-click to finish. Esc cancels."
                    } else {
                        "Drag to select. Shift adds, Alt subtracts, Shift+Alt intersects."
                    }));
                return;
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
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| ui.horizontal_wrapped(|ui| body(ui, self)));
    }
}
