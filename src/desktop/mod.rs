//! egui workbench. Painting and platform input remain independent of widget layout.
mod area;
mod canvas;
mod commands;
mod layers;
mod nodes;
mod panels;
mod preferences;
mod replay;
mod schema;
pub mod settings;
mod shell;
#[cfg(test)]
mod tests;
mod theme;
mod workflow;

use crate::i18n::localize;
use crate::{
    engine::Brush,
    platform::windows::desktop_bridge::{Native, PenSample},
    ui::{
        keymap::Keymap,
        palette::Palette,
        shortcuts::{Action, OpacityInput},
        Workspace,
    },
};
use eframe::egui::{self, *};
use egui_dock::{DockArea, DockState, NodeIndex, SurfaceIndex};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde::{Deserialize, Serialize};
use settings::{Appearance, Settings};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Panel {
    Canvas,
    Color,
    Brush,
    Navigator,
    Layers,
    Nodes,
    Plugin(u32),
}
impl Panel {
    fn title(self) -> &'static str {
        match self {
            Self::Canvas => "Canvas",
            Self::Color => "Color",
            Self::Brush => "Brush",
            Self::Navigator => "Navigator",
            Self::Layers => "Layers",
            Self::Nodes => "Node Editor",
            Self::Plugin(_) => "Plugins",
        }
    }
    fn icon(self) -> char {
        theme::action_icon(match self {
            Self::Canvas => 14,
            Self::Color => 10110,
            Self::Brush => 10,
            Self::Navigator => 95,
            Self::Layers => 61,
            Self::Nodes => 50,
            Self::Plugin(_) => 50,
        })
    }
}
type Tool = crate::application::ToolKind;
fn tr(value: impl Into<WidgetText>) -> RichText {
    let value = value.into();
    if let WidgetText::RichText(ref rich) = value {
        let translated = crate::i18n::translated(rich.text());
        if translated == rich.text() {
            return (**rich).clone();
        }
        return RichText::new(translated);
    }
    RichText::new(crate::i18n::translated(value.text()))
}

pub(super) struct Editor {
    area_mode: u8,
    area_tolerance: u32,
    area_radial: bool,
    area_contiguous: bool,
    area_expand: i32,
    area_feather: u32,
    area_spread: &'static str,
    area_reverse: bool,
    area_transparent: bool,
    area_drag: Option<(Pos2, Pos2)>,
    area_target: Option<(u64, u64, u64)>,
    plugin_nodes: std::collections::BTreeMap<String, crate::plugins::nodes::Resolved>,
    workflow: workflow::WorkflowEditor,
    nodes_workflow: bool,
    nodes: nodes::GraphEditor,
    settings: Settings,
    workspace: Workspace,
    session: crate::application::Session,
    tool: Tool,
    ink: [u8; 3],
    background_ink: [u8; 3],
    radius: f32,
    opacity: f32,
    hardness: f32,
    zoom: f32,
    pan: Vec2,
    fit: bool,
    canvas: Rect,
    image_rect: Rect,
    texture: Option<TextureHandle>,
    wheel_texture: Option<TextureHandle>,
    wheel_key: Option<(usize, u32, u32)>,
    color_hue: f32,
    color_last: [u8; 3],
    wheel_drag: Option<bool>,
    display: Vec<u32>,
    drawing: bool,
    input_owned: bool,
    pen_active: bool,
    pen_eraser: bool,
    pressure: f32,
    pen_hover: Option<Pos2>,
    panning: Option<Pos2>,
    mouse_position: Option<Pos2>,
    packets: Vec<PenSample>,
    actions: Vec<Action>,
    status: String,
    palette: Option<Palette>,
    palette_focus: bool,
    palette_bounds: Rect,
    binding_draft: String,
    binding_index: Option<usize>,
    capture_binding: bool,
    opacity_input: OpacityInput,
    native: Option<Native>,
    blocked: bool,
    new_width: usize,
    new_height: usize,
    new_open: bool,
}
impl Editor {
    fn new(path: PathBuf, native: Option<Native>) -> Self {
        let loaded = Settings::load(path.clone());
        let (settings, mut status) = match loaded {
            Ok(s) => (s, "Ready · Start drawing".into()),
            Err(e) => (
                Settings::defaults(path.clone()),
                format!("Settings Not Loaded: {e}"),
            ),
        };
        #[cfg(not(test))]
        let registry = {
            let args: Vec<_> = std::env::args().collect();
            let root = args
                .iter()
                .position(|a| a == "--plugins")
                .and_then(|i| args.get(i + 1))
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::current_exe()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .join("plugins")
                });
            crate::plugins::Registry::load_with_disabled(&root, &settings.plugins.disabled)
                .unwrap_or_else(|e| {
                    status = format!("Plugin Directory Unavailable; Using Built-in Manifests: {e}");
                    crate::plugins::Registry::bundled()
                })
        };
        #[cfg(test)]
        let registry = crate::plugins::Registry::bundled();
        let mut session = crate::application::Session::new(registry).unwrap_or_else(|e| {
            status = format!("Plugin Runtime Unavailable; Recovering Built-in Manifests: {e}");
            crate::application::Session::new(crate::plugins::Registry::bundled()).unwrap()
        });
        session.runtime = "desktop";
        let plugin_nodes = session.registry.node_catalog();
        let mut editor = Self {
            plugin_nodes,
            workflow: Default::default(),
            nodes_workflow: false,
            nodes: Default::default(),
            settings,
            workspace: Workspace::load(&path.with_file_name("MiCoPi.workspace.ini")),
            session,
            tool: Tool::Brush,
            ink: [51, 55, 67],
            background_ink: [255; 3],
            radius: 6.,
            opacity: 1.,
            hardness: 0.85,
            zoom: 1.,
            pan: Vec2::ZERO,
            fit: true,
            canvas: Rect::NOTHING,
            image_rect: Rect::NOTHING,
            texture: None,
            wheel_texture: None,
            wheel_key: None,
            color_hue: 0.,
            color_last: [0; 3],
            wheel_drag: None,
            display: vec![],
            drawing: false,
            input_owned: false,
            area_mode: 0,
            area_tolerance: 0,
            area_radial: false,
            area_contiguous: true,
            area_expand: 0,
            area_feather: 0,
            area_spread: "pad",
            area_reverse: false,
            area_transparent: false,
            area_drag: None,
            area_target: None,
            pen_active: false,
            pen_eraser: false,
            pressure: 1.,
            pen_hover: None,
            panning: None,
            mouse_position: None,
            packets: Vec::with_capacity(128),
            actions: vec![],
            status,
            palette: None,
            palette_focus: false,
            palette_bounds: Rect::NOTHING,
            binding_draft: String::new(),
            binding_index: None,
            capture_binding: false,
            opacity_input: OpacityInput::default(),
            native,
            blocked: false,
            new_width: 1200,
            new_height: 900,
            new_open: false,
        };
        editor.session.parameters.stabilizer = editor.workspace.stabilizer;
        editor.session.parameters.pressure_size = editor.workspace.dynamics.size;
        editor.session.parameters.pressure_opacity = editor.workspace.dynamics.opacity;
        editor.session.parameters.pressure_gamma = editor.workspace.dynamics.gamma;
        editor.session.parameters.minimum_radius = editor.workspace.dynamics.minimum;
        if editor.settings.has_painting_preferences() {
            editor
                .settings
                .painting
                .apply(&mut editor.session.parameters);
        } else {
            editor.settings.painting = crate::configuration::PaintingPreferences {
                stabilizer: editor.workspace.stabilizer,
                pressure_size: editor.workspace.dynamics.size,
                pressure_opacity: editor.workspace.dynamics.opacity,
                pressure_gamma: editor.workspace.dynamics.gamma,
                minimum_radius: editor.workspace.dynamics.minimum,
                ..Default::default()
            };
        }
        editor.sync_from_session();
        editor
    }
    fn brush(&self, pressure: f32, pen: bool) -> Brush {
        let b = Brush {
            radius: self.radius,
            opacity: self.opacity,
            hardness: self.hardness,
            color: u32::from_be_bytes([0, self.ink[0], self.ink[1], self.ink[2]]),
            erase: self.tool == Tool::Eraser || self.pen_eraser,
        };
        if pen {
            self.workspace.dynamics.apply(b, pressure)
        } else {
            b
        }
    }
    fn label(&self, id: u32) -> String {
        crate::ui::keymap::COMMANDS
            .iter()
            .position(|c| c.ui == id)
            .map(|i| self.settings.keys.label(i))
            .unwrap_or_default()
    }
    fn hint(&self, id: u32) -> String {
        self.settings.keys.ui_hint(id).unwrap_or_default()
    }
    fn finish_stroke(&mut self) {
        self.area_drag = None;
        self.area_target = None;
        self.session.finish_stroke();
        self.drawing = false;
        self.panning = None;
        self.input_owned = false;
    }
    fn core(&mut self, id: &str, args: serde_json::Value) -> bool {
        match self
            .session
            .apply(crate::application::Action::new(id, args))
        {
            Ok(_) => true,
            Err(e) => {
                self.status = e.to_string();
                false
            }
        }
    }
    fn save_settings(&mut self) -> Result<(), String> {
        let encoded: serde_json::Value =
            serde_json::from_str(&self.settings.encode()?).map_err(|e| e.to_string())?;
        self.session
            .apply(crate::application::Action::new(
                "settings.update",
                serde_json::json!({"path":self.settings.path,"document":encoded}),
            ))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    fn select_tool(&mut self, id: &str) {
        self.area_mode = 0;
        self.area_drag = None;
        self.push_gui_parameters();
        if self.core("tool.select", serde_json::json!({"tool":id})) {
            self.sync_from_session();
        }
    }
    fn push_gui_parameters(&mut self) {
        let desired = crate::application::Parameters {
            spacing: self.session.parameters.spacing,
            minimum_opacity: self.session.parameters.minimum_opacity,
            radius: self.radius,
            opacity: self.opacity,
            hardness: self.hardness,
            color: format!("#{:02X}{:02X}{:02X}", self.ink[0], self.ink[1], self.ink[2]),
            background_color: format!(
                "#{:02X}{:02X}{:02X}",
                self.background_ink[0], self.background_ink[1], self.background_ink[2]
            ),
            stabilizer: self.workspace.stabilizer,
            pressure_size: self.workspace.dynamics.size,
            pressure_opacity: self.workspace.dynamics.opacity,
            pressure_gamma: self.workspace.dynamics.gamma,
            minimum_radius: self.workspace.dynamics.minimum,
        };
        if desired != self.session.parameters {
            self.core("brush.set", serde_json::to_value(desired).unwrap());
        }
    }
    fn push_gui_view(&mut self) {
        let view = crate::application::View {
            zoom: self.zoom,
            pan_x: self.pan.x,
            pan_y: self.pan.y,
            fit: self.fit,
        };
        if self.session.view != view {
            self.core("view.set", serde_json::to_value(view).unwrap());
        }
    }
    fn sync_from_session(&mut self) {
        self.tool = self.session.tool;
        let p = &self.session.parameters;
        self.radius = p.radius;
        self.opacity = p.opacity;
        self.hardness = p.hardness;
        let color = crate::application::parse_color(&p.color).unwrap_or(0);
        self.ink = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        let color = crate::application::parse_color(&p.background_color).unwrap_or(0xffffff);
        self.background_ink = [(color >> 16) as u8, (color >> 8) as u8, color as u8];
        self.workspace.stabilizer = p.stabilizer;
        self.workspace.dynamics = crate::engine::input::Dynamics {
            size: p.pressure_size,
            opacity: p.pressure_opacity,
            gamma: p.pressure_gamma,
            minimum: p.minimum_radius,
        };
        self.zoom = self.session.view.zoom;
        self.pan = vec2(self.session.view.pan_x, self.session.view.pan_y);
        self.fit = self.session.view.fit;
        self.drawing = self.session.is_drawing();
        if self
            .texture
            .as_ref()
            .is_some_and(|t| t.size() != [self.session.doc.width, self.session.doc.height])
        {
            self.texture = None;
            self.display.clear();
        }
    }
}

pub struct Studio {
    replay: replay::ReplayWindow,
    action_server: Option<crate::automation::ipc::Server>,
    from_core_ui: bool,
    pending_capture: Option<PathBuf>,
    editor: Editor,
    dock: DockState<Panel>,
    dock_before_focus: Option<DockState<Panel>>,
    settings_open: bool,
    settings_before: Option<Settings>,
    preferences: preferences::Preferences,
    applied: Appearance,
    layout_path: PathBuf,
    layout_snapshot: String,
    layout_changed: Option<Instant>,
    preview: Option<PathBuf>,
    preview_frames: u32,
    screenshot_requested: bool,
    allow_close: bool,
    title: String,
}
pub fn default_dock(a: &Appearance, width: f32) -> DockState<Panel> {
    let mut dock = DockState::new(vec![Panel::Canvas]);
    let total = width.max(a.left_panel_width + a.right_panel_width + 380.);
    let [center, left] = dock.main_surface_mut().split_left(
        NodeIndex::root(),
        (a.left_panel_width / total).clamp(0.15, 0.35),
        vec![Panel::Color],
    );
    dock.main_surface_mut()
        .split_below(left, 0.50, vec![Panel::Brush]);
    let [_, right] = dock.main_surface_mut().split_right(
        center,
        1. - (a.right_panel_width / (total - a.left_panel_width)).clamp(0.15, 0.38),
        vec![Panel::Navigator],
    );
    dock.main_surface_mut()
        .split_below(right, 0.32, vec![Panel::Layers]);
    dock
}
impl Studio {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let hwnd = cc
            .window_handle()
            .ok()
            .and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                _ => None,
            })
            .unwrap_or(0);
        let native = Native::new(hwnd, cc.egui_ctx.clone());
        let args: Vec<_> = std::env::args().collect();
        let path = args
            .iter()
            .position(|a| a == "--config-dir")
            .and_then(|i| args.get(i + 1))
            .map(|p| PathBuf::from(p).join("settings.json"))
            .unwrap_or_else(Keymap::path);
        let mut editor = Editor::new(path, Some(native));
        crate::i18n::set_language(editor.settings.ui.language);
        theme::fonts(&cc.egui_ctx);
        if !editor.session.registry.diagnostics.is_empty() {
            editor.status = format!(
                "{} Plugins Not Loaded; See plugins.list for Details",
                editor.session.registry.diagnostics.len()
            );
        }
        theme::apply(&cc.egui_ctx, &editor.settings.ui);
        let layout_path = editor.settings.path.with_file_name("workspace.egui.json");
        let saved = std::fs::read_to_string(&layout_path)
            .ok()
            .and_then(|text| decode_workspace(&text));
        let known = saved.as_ref().map(|(_, known)| known.clone());
        let mut dock = saved.map(|(dock, _)| dock).unwrap_or_else(|| {
            default_dock(
                &editor.settings.ui,
                1360. - 48. - editor.settings.ui.outer_padding * 3.,
            )
        });
        reconcile_plugins(&mut dock, &editor.session.registry, known.as_deref());
        let layout_snapshot = encode_dock(&dock).unwrap_or_default();

        let preview = args
            .iter()
            .position(|a| a == "--preview")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from);
        let preview_keys = args.iter().any(|a| a == "--preview-bindings");
        let settings_open = args.iter().any(|a| a == "--preview-settings") || preview_keys;
        let mut preferences = preferences::Preferences::default();
        if preview_keys {
            preferences.category = preferences::Category::Keys;
        }
        let applied = editor.settings.ui.clone();
        editor.push_gui_parameters();
        let action_server = if preview.is_none() {
            let context = cc.egui_ctx.clone();
            match crate::automation::ipc::Server::start(
                editor.settings.path.with_file_name("session.json"),
                move || context.request_repaint(),
            ) {
                Ok(server) => Some(server),
                Err(error) => {
                    editor.status = format!("Local Action Service Did Not Start: {error}");
                    None
                }
            }
        } else {
            None
        };
        Self {
            action_server,
            replay: replay::ReplayWindow::default(),
            from_core_ui: false,
            pending_capture: None,
            editor,
            dock,
            dock_before_focus: None,
            settings_open,
            settings_before: None,
            preferences,
            applied,
            layout_path,
            layout_snapshot,
            layout_changed: None,
            preview,
            preview_frames: 0,
            screenshot_requested: false,
            allow_close: false,
            title: String::new(),
        }
    }
    fn consume_ui_effects(&mut self, ctx: &Context) {
        let effects = std::mem::take(&mut self.editor.session.ui_effects);
        for effect in effects {
            match effect["id"].as_str() {
                Some("nodes.open") => {
                    self.editor.nodes_workflow = effect["value"]["mode"] == "workflow";
                    self.focus_panel(Panel::Nodes);
                }
                Some("replay.open") => {
                    self.editor.finish_stroke();
                    self.replay.open = true;
                    self.replay.refresh = true;
                }
                Some("command") => {
                    match serde_json::from_value::<Action>(effect["value"].clone()) {
                        Ok(action) => {
                            self.from_core_ui = true;
                            self.editor.actions.push(action);
                            self.run_actions(ctx);
                            self.from_core_ui = false;
                        }
                        Err(e) => self.editor.status = format!("Invalid UI Action: {e}"),
                    }
                }
                Some("painting.mode") => {
                    if let Some(mode) = match effect["value"].as_str() {
                        Some("brush") => Some(0),
                        Some("fill") => Some(1),
                        Some("gradient") => Some(2),
                        _ => None,
                    } {
                        self.editor.finish_stroke();
                        self.editor.select_tool("brush");
                        self.editor.area_mode = mode;
                    }
                }
                Some("settings.search") => {
                    self.open_settings(None);
                    self.preferences.query = effect["value"].as_str().unwrap_or("").into();
                    self.preferences.category = preferences::Category::All;
                }
                Some("settings.close") if self.settings_open => self.cancel_settings(),
                Some("settings.reload") => {
                    if effect["value"]
                        .as_str()
                        .is_some_and(|p| PathBuf::from(p) == self.editor.settings.path)
                    {
                        match Settings::load(self.editor.settings.path.clone()) {
                            Ok(settings) => {
                                if self.settings_open {
                                    self.settings_before = Some(settings.clone());
                                    self.preferences.painting_before =
                                        Some(settings.painting.clone());
                                    self.preferences.json = settings.encode().unwrap_or_default();
                                }
                                settings.painting.apply(&mut self.editor.session.parameters);
                                self.editor.sync_from_session();
                                self.editor.settings = settings;
                                ctx.request_repaint();
                            }
                            Err(e) => self.editor.status = e,
                        }
                    }
                }
                Some("close") => {
                    if effect["value"] == true {
                        self.allow_close = true;
                        self.save_layout();
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    } else {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
                Some("screenshot") => {
                    if let Some(path) = effect["value"].as_str() {
                        self.pending_capture = Some(PathBuf::from(path));
                        ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                    }
                }
                _ => {}
            }
        }
    }
    fn panel_available(&self, panel: Panel) -> bool {
        panel == Panel::Canvas
            || self
                .editor
                .session
                .registry
                .panels
                .values()
                .any(|p| contributed_panel(p) == panel)
    }
    fn reset_layout(&mut self, width: f32) {
        self.dock_before_focus = None;
        self.dock = default_dock(&self.editor.settings.ui, width);
        reconcile_plugins(&mut self.dock, &self.editor.session.registry, None);
    }
    fn focus_panel(&mut self, panel: Panel) {
        if !self.panel_available(panel) {
            self.editor.status = "Panel Plugin Disabled".into();
            return;
        }
        if let Some(before) = self.dock_before_focus.take() {
            self.dock = before;
        }
        if let Some((s, n, t)) = self.dock.find_tab(&panel) {
            self.dock.set_active_tab((s, n, t));
            self.dock.set_focused_node_and_surface((s, n));
        } else {
            if panel == Panel::Nodes {
                if let Some((surface, node, _)) = self.dock.find_tab(&Panel::Canvas) {
                    self.dock[surface].split_below(node, 0.42, vec![panel]);
                    return;
                }
            }
            self.dock.main_surface_mut().push_to_focused_leaf(panel);
        }
    }
    fn save_layout(&mut self) {
        if self.preview.is_some() {
            return;
        }
        let dock = self.dock_before_focus.as_ref().unwrap_or(&self.dock);
        match encode_workspace(dock, &self.editor.session.registry)
            .and_then(|text| settings::atomic_write(&self.layout_path, &text))
        {
            Ok(()) => {
                self.layout_changed = None;
            }
            Err(e) => self.editor.status = format!("Layout Save Failed: {e}"),
        }
    }
    fn frame_ui(&mut self, ctx: &Context) {
        self.editor.push_gui_parameters();
        self.editor.push_gui_view();
        if let Some(server) = &self.action_server {
            if server.poll(&mut self.editor.session) > 0 {
                self.editor.sync_from_session();
                self.editor.status = "MCP / Action Updated the Session".into();
            }
        }
        crate::i18n::set_language(self.editor.settings.ui.language);
        if self.applied != self.editor.settings.ui {
            if self.applied.language != self.editor.settings.ui.language {
                theme::fonts(ctx);
                self.editor.workflow.fit();
                self.editor.nodes.fit();
            }
            theme::apply(ctx, &self.editor.settings.ui);
            self.applied = self.editor.settings.ui.clone();
            ctx.request_repaint();
            if let Some(native) = &self.editor.native {
                let mut t = crate::ui::Theme::preset(if self.applied.theme == "graphite" {
                    2
                } else {
                    0
                });
                t.selected = u32::from_be_bytes([
                    0,
                    theme::Colors::new(&self.applied).border.r(),
                    theme::Colors::new(&self.applied).border.g(),
                    theme::Colors::new(&self.applied).border.b(),
                ]);
                native.appearance(t, self.applied.theme == "graphite");
            }
        }
        if let Some(native) = &self.editor.native {
            native.drain_pen(&mut self.editor.packets);
        }
        self.consume_ui_effects(ctx);
        self.editor.shortcuts(ctx);
        self.run_actions(ctx);
        self.editor.blocked = self.settings_open
            || self.editor.palette.is_some()
            || self.editor.new_open
            || self.replay.open;
        self.draw_shell(ctx);
        let a = self.editor.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(colors.background)
                    .inner_margin(Margin::same(a.outer_padding as i8)),
            )
            .show(ctx, |ui| {
                let mut style = egui_dock::Style::from_egui(ui.style().as_ref());
                style.dock_area_padding = Some(Margin::ZERO);
                style.tab_bar.height = a.tab_height;
                style.tab_bar.bg_fill = colors.panel;
                style.tab_bar.inner_margin = Margin::symmetric(8, 0);
                style.tab.tab_body.inner_margin = Margin::same(a.panel_padding as i8);
                style.tab.tab_body.bg_fill = colors.panel;
                style.tab.tab_body.stroke = Stroke::new(1., colors.border);
                style.tab.tab_body.corner_radius = CornerRadius::same(a.corner_radius as u8);
                style.tab.hline_below_active_tab_name = true;
                style.tab.active.bg_fill = colors.panel;
                style.tab.focused.bg_fill = colors.panel;
                style.tab.focused.text_color = colors.accent;
                style.separator.width = a.item_spacing;
                style.separator.color_idle = colors.background;
                style.main_surface_border_stroke = Stroke::NONE;
                DockArea::new(&mut self.dock)
                    .id(Id::new("micopi-workspace"))
                    .style(style)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(
                        ui,
                        &mut panels::Viewer {
                            editor: &mut self.editor,
                        },
                    );
            });
        self.settings_window(ctx);
        self.replay_window(ctx);
        self.editor.palette_ui(ctx);
        self.new_document_window(ctx);
        self.run_actions(ctx);
        if !ctx.input(|i| i.focused) && self.editor.input_owned {
            self.editor.finish_stroke();
            self.editor.pen_active = false;
        }
        self.editor.push_gui_parameters();
        self.editor.push_gui_view();
    }
}
fn encode_dock(dock: &DockState<Panel>) -> Result<String, String> {
    // Runtime rectangles use infinity before their first layout. They are recomputed
    // every frame and must never be written as JSON nulls in a saved workspace.
    let mut saved = dock.clone();
    for (_, node) in saved.iter_all_nodes_mut() {
        match node {
            egui_dock::Node::Leaf(leaf) => {
                leaf.rect = Rect::ZERO;
                leaf.viewport = Rect::ZERO;
            }
            egui_dock::Node::Horizontal(split) | egui_dock::Node::Vertical(split) => {
                split.rect = Rect::ZERO
            }
            _ => {}
        }
    }
    serde_json::to_string_pretty(&saved).map_err(|e| e.to_string())
}
fn contributed_panel(p: &crate::plugins::PanelDefinition) -> Panel {
    match p.builtin.as_deref() {
        Some("color") => Panel::Color,
        Some("brush") => Panel::Brush,
        Some("navigator") => Panel::Navigator,
        Some("layers") => Panel::Layers,
        Some("nodes") => Panel::Nodes,
        _ => Panel::Plugin(crate::plugins::panel_key(&p.id)),
    }
}
fn decode_workspace(text: &str) -> Option<(DockState<Panel>, Vec<String>)> {
    if text.len() > 1024 * 1024 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.get("version").is_some_and(|v| v != 1) {
        return None;
    }
    let known = value
        .get("known_panels")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_else(|| {
            ["color", "brush", "navigator", "layers"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        });
    let dock: DockState<Panel> =
        serde_json::from_value(value.get("dock").unwrap_or(&value).clone()).ok()?;
    valid_dock(&dock).then_some((dock, known))
}
fn encode_workspace(
    dock: &DockState<Panel>,
    registry: &crate::plugins::Registry,
) -> Result<String, String> {
    let dock: serde_json::Value =
        serde_json::from_str(&encode_dock(dock)?).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&serde_json::json!({"version":1,"dock":dock,"known_panels":registry.panels.keys().collect::<Vec<_>>()})).map_err(|e|e.to_string())
}
fn reconcile_plugins(
    dock: &mut DockState<Panel>,
    registry: &crate::plugins::Registry,
    known: Option<&[String]>,
) {
    let panels: Vec<_> = registry.panels.values().map(contributed_panel).collect();
    let remove: Vec<_> = dock
        .iter_all_tabs()
        .map(|(_, p)| *p)
        .filter(|p| *p != Panel::Canvas && !panels.contains(p))
        .collect();
    for p in remove {
        if let Some(location) = dock.find_tab(&p) {
            dock.remove_tab(location);
        }
    }
    for definition in registry.panels.values() {
        let panel = contributed_panel(definition);
        let new = known.is_none_or(|ids| !ids.contains(&definition.id));
        if new && panel != Panel::Nodes && dock.find_tab(&panel).is_none() {
            if let Some((surface, node, _)) = dock.find_tab(&Panel::Layers) {
                dock.set_focused_node_and_surface((surface, node));
            }
            dock.push_to_focused_leaf(panel);
        }
    }
}
fn valid_dock(dock: &DockState<Panel>) -> bool {
    if !matches!(
        dock.iter_surfaces().next(),
        Some(egui_dock::Surface::Main(_))
    ) || dock.iter_surfaces().count() > 8
    {
        return false;
    }
    for surface in dock.iter_surfaces() {
        if surface.is_empty() {
            continue;
        }
        let nodes: Vec<_> = surface.iter_nodes().collect();
        if nodes.is_empty() || nodes.len() > 128 {
            return false;
        }
        let mut todo = vec![0];
        let mut visited = vec![false; nodes.len()];
        while let Some(index) = todo.pop() {
            if index >= nodes.len() || visited[index] {
                return false;
            }
            visited[index] = true;
            match nodes[index] {
                egui_dock::Node::Leaf(leaf) => {
                    if leaf.tabs.is_empty()
                        || leaf.active.0 >= leaf.tabs.len()
                        || !leaf.scroll.is_finite()
                    {
                        return false;
                    }
                }
                egui_dock::Node::Horizontal(split) | egui_dock::Node::Vertical(split) => {
                    if !split.fraction.is_finite() || !(0.0..=1.0).contains(&split.fraction) {
                        return false;
                    }
                    todo.extend([index * 2 + 1, index * 2 + 2]);
                }
                egui_dock::Node::Empty => return false,
            }
        }
        if nodes
            .iter()
            .enumerate()
            .any(|(i, node)| !visited[i] && !matches!(node, egui_dock::Node::Empty))
        {
            return false;
        }
    }
    let tabs: Vec<_> = dock.iter_all_tabs().map(|(_, p)| *p).collect();
    tabs.iter().filter(|p| **p == Panel::Canvas).count() == 1
        && tabs.len() <= 128
        && tabs.iter().enumerate().all(|(i, p)| !tabs[..i].contains(p))
        && dock
            .find_tab(&Panel::Canvas)
            .is_some_and(|(s, _, _)| s == SurfaceIndex::main())
}
impl eframe::App for Studio {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.pending_capture.is_some() {
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = shot {
                let path = self.pending_capture.take().unwrap();
                let mut pixels: Vec<u32> = image
                    .pixels
                    .iter()
                    .map(|c| u32::from_be_bytes([c.a(), c.r(), c.g(), c.b()]))
                    .collect();
                if let Some(native) = &self.editor.native {
                    match native.save_png(&path, &mut pixels, image.width(), image.height()) {
                        Ok(()) => {
                            self.editor.status = format!("Screenshot Saved: {}", path.display())
                        }
                        Err(e) => self.editor.status = e.to_string(),
                    }
                }
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.editor.session.doc.dirty {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                self.editor.actions.push(Action::UiCommand(19999));
            } else {
                self.save_layout();
                let _ = self.editor.workspace.save(
                    &self
                        .editor
                        .settings
                        .path
                        .with_file_name("MiCoPi.workspace.ini"),
                );
            }
        }
        self.frame_ui(ctx);
        let snapshot =
            encode_dock(self.dock_before_focus.as_ref().unwrap_or(&self.dock)).unwrap_or_default();
        if snapshot != self.layout_snapshot {
            self.layout_snapshot = snapshot;
            self.layout_changed = Some(Instant::now());
        }
        if let Some(then) = self.layout_changed {
            if then.elapsed() > Duration::from_millis(750) && !ctx.input(|i| i.pointer.any_down()) {
                self.save_layout();
            } else {
                ctx.request_repaint_after(Duration::from_millis(800));
            }
        }
        let name = self
            .editor
            .session
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| crate::i18n::text("Untitled").into());
        let title = format!(
            "{}{} — MiCoPi",
            if self.editor.session.doc.dirty {
                "* "
            } else {
                ""
            },
            name
        );
        if self.title != title {
            self.title = title.clone();
            ctx.send_viewport_cmd(ViewportCommand::Title(title));
        }
        if let Some(path) = &self.preview {
            self.preview_frames += 1;
            if self.preview_frames >= 8 && !self.screenshot_requested {
                ctx.send_viewport_cmd(ViewportCommand::Screenshot(UserData::default()));
                self.screenshot_requested = true;
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| {
                    if let Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = shot {
                let mut pixels: Vec<u32> = image
                    .pixels
                    .iter()
                    .map(|c| u32::from_be_bytes([c.a(), c.r(), c.g(), c.b()]))
                    .collect();
                if let Some(native) = &self.editor.native {
                    let _ = native.save_png(path, &mut pixels, image.width(), image.height());
                }
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.editor.finish_stroke();
        self.save_layout();
        let _ = self.editor.workspace.save(
            &self
                .editor
                .settings
                .path
                .with_file_name("MiCoPi.workspace.ini"),
        );
    }
}
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: ViewportBuilder::default()
            .with_title("MiCoPi")
            .with_inner_size([1360., 920.])
            .with_min_inner_size([960., 680.])
            .with_decorations(false)
            .with_transparent(false),
        ..Default::default()
    };
    eframe::run_native(
        "MiCoPi",
        options,
        Box::new(|cc| Ok(Box::new(Studio::new(cc)))),
    )
}
