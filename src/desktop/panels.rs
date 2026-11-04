use super::*;
use egui_dock::TabViewer;
pub(super) struct Viewer<'a> {
    pub editor: &'a mut Editor,
    pub can_float: bool,
    pub native: Option<&'a Native>,
    pub single_leaf: bool,
    pub focused: bool,
}
impl Viewer<'_> {
    pub(super) fn tab_title(&mut self, p: &mut Panel) -> WidgetText {
        if let Panel::Plugin(key) = p {
            return self
                .editor
                .session
                .registry
                .panels
                .values()
                .find(|d| crate::plugins::panel_key(&d.id) == *key)
                .map(|d| {
                    crate::i18n::translated_for(
                        self.editor
                            .session
                            .registry
                            .contribution_owner("panels", &d.id),
                        &d.title,
                    )
                    .into_owned()
                })
                .unwrap_or_else(|| crate::i18n::text("Plugin Not Loaded").into_owned())
                .into();
        }
        if *p == Panel::Canvas {
            let name = self
                .editor
                .session
                .file
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| crate::i18n::text("Untitled").into());
            format!(
                "{}{}",
                if self.editor.session.doc.dirty {
                    "●  "
                } else {
                    ""
                },
                name
            )
            .into()
        } else {
            let builtin = match p {
                Panel::Color => "color",
                Panel::Brush => "brush",
                Panel::Navigator => "navigator",
                Panel::Layers => "layers",
                Panel::Nodes => "nodes",
                Panel::Workflow => "workflow",
                Panel::Tools => "tools",
                Panel::ToolOptions => "tool_options",
                _ => "",
            };
            self.editor
                .session
                .registry
                .panels
                .values()
                .find(|d| d.builtin.as_deref() == Some(builtin))
                .map(|d| {
                    crate::i18n::translated_for(
                        self.editor
                            .session
                            .registry
                            .contribution_owner("panels", &d.id),
                        &d.title,
                    )
                    .into_owned()
                })
                .unwrap_or_else(|| localize(p.title()).into_owned())
                .into()
        }
    }
}
impl TabViewer for Viewer<'_> {
    type Tab = Panel;
    fn title(&mut self, _: &mut Panel) -> WidgetText {
        "".into()
    }
    fn id(&mut self, p: &mut Panel) -> Id {
        Id::new(("panel", *p))
    }
    fn is_closeable(&self, p: &Panel) -> bool {
        let _ = p;
        true
    }
    fn on_close(&mut self, panel: &mut Panel) -> egui_dock::tab_viewer::OnCloseResponse {
        if let Some(id) = self.editor.panel_id(*panel) {
            self.editor.core(
                "window.command",
                serde_json::json!({"panel":id,"operation":"hide"}),
            );
        }
        egui_dock::tab_viewer::OnCloseResponse::Ignore
    }
    fn allowed_in_windows(&self, p: &mut Panel) -> bool {
        let _ = p;
        self.can_float
    }
    fn scroll_bars(&self, _: &Panel) -> [bool; 2] {
        [false, false]
    }
    fn context_menu(
        &mut self,
        ui: &mut Ui,
        panel: &mut Panel,
        _: egui_dock::SurfaceIndex,
        _: egui_dock::NodeIndex,
    ) {
        self.editor.window_menu(*panel, ui);
    }
    fn on_tab_button(&mut self, panel: &mut Panel, response: &Response) {
        if response.double_clicked() {
            if let Some(id) = self.editor.panel_id(*panel) {
                self.editor.core(
                    "window.command",
                    serde_json::json!({"panel":id,"operation":"float","scope":"tabs"}),
                );
            }
        }
        response
            .clone()
            .on_hover_text(tr("Double-click a Tab to Float Its Group"));
        response.clone().on_hover_text(tr(
            "Drag Tabs to Split, Group or Float Panels; Right-click for Actions",
        ));
    }
    fn tab_style_override(
        &self,
        p: &Panel,
        global: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        let mut style = global.clone();
        if *p == Panel::Canvas {
            style.tab_body.inner_margin = Margin::ZERO;
        }
        Some(style)
    }
    fn ui(&mut self, ui: &mut Ui, p: &mut Panel) {
        self.group_bar(ui, *p);
        if *p == Panel::Tools {
            self.editor.toolbox_ui(ui);
            return;
        }
        if *p == Panel::ToolOptions {
            self.editor.tool_options_ui(ui);
            return;
        }
        if *p == Panel::Canvas {
            self.editor.canvas_ui(ui);
            return;
        }
        if *p == Panel::Workflow {
            self.editor.workflow_ui(ui);
            return;
        }
        if *p == Panel::Nodes {
            self.editor.nodes_ui(ui);
            return;
        }
        ui.set_min_width(176.);
        ui.spacing_mut().slider_width = (ui.available_width() * 0.35).clamp(52., 120.);
        if *p == Panel::Navigator {
            self.editor.navigator_ui(ui);
            return;
        }
        ScrollArea::vertical()
            .id_salt(*p)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .auto_shrink([false, false])
            .show(ui, |ui| match p {
                Panel::Color => self.editor.color_ui(ui),
                Panel::Brush => self.editor.brush_ui(ui),
                Panel::Navigator => self.editor.navigator_ui(ui),
                Panel::Layers => self.editor.layers_ui(ui),
                Panel::Plugin(key) => self.editor.plugin_panel(ui, *key),
                _ => {}
            });
    }
}
pub(super) fn section(ui: &mut Ui, title: &str, gap: f32) {
    ui.add_space(gap * 0.5);
    ui.label(tr(RichText::new(localize(title)).small().strong()));
}
pub(super) fn icon_button(ui: &mut Ui, editor: &Editor, id: u32, selected: bool) -> Response {
    let a = &editor.settings.ui;
    let response = ui.add_sized(
        [a.control_height + 4., a.control_height],
        Button::new(tr(theme::icon(theme::action_icon(id), a.icon_size)))
            .frame_when_inactive(selected)
            .selected(selected),
    );
    let hint = editor.hint(id);
    let keys = editor.label(id);
    response.on_hover_ui(|ui| {
        ui.label(tr(hint.split("  ").next().unwrap_or(&hint)));
        theme::keycaps(ui, &keys);
    })
}
impl Editor {
    fn plugin_panel(&mut self, ui: &mut Ui, key: u32) {
        let definition = self
            .session
            .registry
            .panels
            .values()
            .find(|d| crate::plugins::panel_key(&d.id) == key)
            .cloned();
        if let Some(panel) = definition {
            let owner = self
                .session
                .registry
                .contribution_owner("panels", &panel.id)
                .map(str::to_owned);
            for button in panel.buttons {
                if ui
                    .add_sized(
                        [ui.available_width(), self.settings.ui.control_height],
                        Button::new(tr_for(owner.as_deref(), &button.title)),
                    )
                    .clicked()
                {
                    match self.session.apply(button.action) {
                        Ok(_) => self.sync_from_session(),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
        } else {
            ui.weak(tr("Plugin Disabled or Unavailable"));
        }
    }
}

pub(super) fn parameter(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    percent: bool,
    logarithmic: bool,
) {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    // A shared column grid keeps short and translated labels aligned.
    let label_width = (width * 0.32).clamp(64., 104.);
    let number_width = 58.;
    let narrow = width < label_width + number_width + gap * 2. + 60.;
    let mut display = if percent { *value * 100. } else { *value };
    let display_range = if percent {
        *range.start() * 100. ..=*range.end() * 100.
    } else {
        range.clone()
    };
    ui.horizontal(|ui| {
        let label_size = vec2(
            if narrow {
                width - number_width - gap
            } else {
                label_width
            },
            ui.spacing().interact_size.y,
        );
        ui.allocate_ui_with_layout(label_size, Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_size(label_size);
            ui.add(
                Label::new(tr(RichText::new(localize(label))
                    .small()
                    .color(ui.visuals().weak_text_color())))
                .truncate(),
            )
            .on_hover_text(tr(label));
        });
        if !narrow {
            ui.spacing_mut().slider_width =
                (width - label_width - number_width - gap * 2.).max(24.);
            let changed = ui
                .scope(|ui| {
                    ui.spacing_mut().interact_size.y *= 0.6;
                    ui.add(
                        Slider::new(value, range.clone())
                            .show_value(false)
                            .logarithmic(logarithmic),
                    )
                    .changed()
                })
                .inner;
            if changed {
                display = if percent { *value * 100. } else { *value };
            }
        }
        if ui
            .add_sized(
                [number_width, ui.spacing().interact_size.y],
                DragValue::new(&mut display)
                    .range(display_range)
                    .speed(if percent { 1. } else { 0.5 })
                    .max_decimals(1)
                    .suffix(suffix),
            )
            .changed()
        {
            *value = if percent { display / 100. } else { display };
        }
    });
    if narrow {
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = width;
            ui.add(
                Slider::new(value, range)
                    .show_value(false)
                    .logarithmic(logarithmic),
            );
        });
    }
}
