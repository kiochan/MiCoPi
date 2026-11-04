//! egui workbench. Painting and platform input remain independent of widget layout.
mod area;
#[path = "../../plugins/brush/panel.rs"]
mod brush;
#[path = "../../plugins/canvas/panel.rs"]
mod canvas;
#[path = "../../plugins/color/panel.rs"]
mod color;
mod commands;
mod document_texture;
mod group_bar;
mod group_drop;
#[path = "../../plugins/layers/panel.rs"]
mod layers;
mod native_windows;
#[path = "../../plugins/navigator/panel.rs"]
mod navigator;
#[path = "../../plugins/compositing/panel.rs"]
mod nodes;
mod panels;
#[path = "../../plugins/workbench/command_panel.rs"]
mod plugin_commands;
#[path = "../../plugins/workbench/preferences.rs"]
mod preferences;
#[path = "../../plugins/replay/panel.rs"]
mod replay;
mod schema;
pub mod settings;
mod shell;
#[cfg(test)]
mod tests;
mod theme;
#[path = "../../plugins/toolbox/panel.rs"]
mod toolbox;
mod window_groups;
mod windows;
#[path = "../../plugins/workflows/panel.rs"]
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
#[cfg(test)]
use egui_dock::SurfaceIndex;
use egui_dock::{DockArea, DockState, NodeIndex};
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
    Tools,
    Color,
    Brush,
    Navigator,
    Layers,
    Nodes,
    Workflow,
    Plugin(u32),
}
impl Panel {
    fn title(self) -> &'static str {
        match self {
            Self::Canvas => "Canvas",
            Self::Tools => "Tools",
            Self::Color => "Color",
            Self::Brush => "Brush",
            Self::Navigator => "Navigator",
            Self::Layers => "Layers",
            Self::Nodes => "Node Editor",
            Self::Workflow => "Plugin Workflow",
            Self::Plugin(_) => "Plugins",
        }
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
    headers: std::collections::HashMap<Panel, group_bar::HeaderLayout>,
    command_form: Option<plugin_commands::CommandForm>,
    area_mode: u8,
    area_tolerance: u32,
    area_radial: bool,
    area_drag: Option<(Pos2, Pos2)>,
    area_target: Option<(u64, u64, u64)>,
    plugin_nodes: std::collections::BTreeMap<String, crate::plugins::nodes::Resolved>,
    workflow: workflow::WorkflowEditor,
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
            headers: Default::default(),
            command_form: None,
            plugin_nodes,
            workflow: Default::default(),
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
        editor.register_commands();
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
    pending_group_drag: Option<Panel>,
    window_capture_results: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    pending_window_captures: std::collections::HashMap<ViewportId, PathBuf>,
    native_windows: std::collections::BTreeMap<usize, native_windows::NativeWindow>,
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
    let total = width.max(a.right_panel_width + 500.);
    let [center, right] = dock.main_surface_mut().split_right(
        NodeIndex::root(),
        1. - (a.right_panel_width / total).clamp(0.16, 0.38),
        vec![Panel::Navigator],
    );
    dock.main_surface_mut().split_below(
        right,
        0.32,
        vec![Panel::Layers, Panel::Brush, Panel::Color],
    );
    dock.main_surface_mut().split_above(
        center,
        (a.toolbox_height / 820.).clamp(0.10, 0.25),
        vec![Panel::Tools],
    );
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
        let saved_text = std::fs::read_to_string(&layout_path).ok();
        let saved = saved_text.as_deref().and_then(decode_workspace);
        if saved
            .as_ref()
            .is_some_and(|(_, known)| !known.iter().any(|id| id == "toolbox"))
        {
            if let Some(text) = &saved_text {
                use std::io::Write;
                if let Ok(mut backup) = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(layout_path.with_file_name("workspace.before-plugin-windows.json"))
                {
                    let _ = backup.write_all(text.as_bytes());
                }
            }
        }
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
            pending_group_drag: None,
            window_capture_results: Default::default(),
            pending_window_captures: Default::default(),
            native_windows: Default::default(),
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
                Some("application.window") => match effect["value"].as_str() {
                    Some("minimize") => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                    Some("maximize") => ctx.send_viewport_cmd(ViewportCommand::Maximized(true)),
                    Some("restore") => {
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(false));
                    }
                    _ => {}
                },
                Some("window.drop") => {
                    let panel = effect["value"]["panel"]
                        .as_str()
                        .and_then(|id| self.editor.session.registry.panels.get(id))
                        .map(contributed_panel);
                    if let Some((surface, _, _)) = panel
                        .and_then(|p| self.dock.find_tab(&p))
                        .filter(|(s, _, _)| !s.is_main())
                    {
                        self.request_group_drop_hint(
                            ctx,
                            surface.0,
                            (
                                effect["value"]["x"].as_i64().unwrap() as i32,
                                effect["value"]["y"].as_i64().unwrap() as i32,
                            ),
                            effect["value"]["target"]
                                .as_str()
                                .and_then(|id| self.editor.session.registry.panels.get(id))
                                .map(contributed_panel),
                        );
                    } else {
                        self.editor.session.ui["windows.drop"] =
                            serde_json::json!({"status":"no_source"});
                    }
                }
                Some("window.capture") => {
                    let panel = effect["value"]["panel"]
                        .as_str()
                        .and_then(|id| self.editor.session.registry.panels.get(id))
                        .map(contributed_panel);
                    let viewport = panel
                        .and_then(|p| self.dock.find_tab(&p))
                        .and_then(|(s, _, _)| self.native_windows.get(&s.0))
                        .map(|w| w.viewport);
                    if let (Some(viewport), Some(path)) =
                        (viewport, effect["value"]["path"].as_str())
                    {
                        self.pending_window_captures
                            .insert(viewport, PathBuf::from(path));
                    } else {
                        self.editor.status = "Float the window before capturing it".into();
                    }
                }
                Some("window.layout") => {
                    let result = decode_workspace(&effect["value"].to_string());
                    if let Some((mut dock, _)) = result {
                        let known = self
                            .editor
                            .session
                            .registry
                            .panels
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>();
                        reconcile_plugins(&mut dock, &self.editor.session.registry, Some(&known));
                        self.reset_window_input();
                        self.dock = dock;
                        self.dock_before_focus = None;
                        self.layout_changed = Some(Instant::now());
                        self.editor.session.ui["windows.result"] =
                            serde_json::json!({"status":"applied","operation":"layout"});
                        self.report_windows();
                    } else {
                        self.editor.session.ui["windows.result"] =
                            serde_json::json!({"status":"failed","error":"Invalid window layout"});
                    }
                }
                Some("window.command") => {
                    let result = self.apply_window_command(ctx, &effect["value"]);
                    self.editor.session.ui["windows.result"] = match result {
                        Ok(()) => serde_json::json!({"status":"applied","request":effect["value"]}),
                        Err(error) => {
                            self.editor.status = error.clone();
                            serde_json::json!({"status":"failed","error":error,"request":effect["value"]})
                        }
                    };
                    self.report_windows();
                }
                Some("nodes.open") => {
                    self.focus_panel(if effect["value"]["mode"] == "workflow" {
                        Panel::Workflow
                    } else {
                        Panel::Nodes
                    });
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
                                self.editor.register_commands();
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
        self.editor
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
        self.layout_changed = Some(Instant::now());
        if let Some(before) = self.dock_before_focus.take() {
            self.dock = before;
        }
        if let Some((s, n, t)) = self.dock.find_tab(&panel) {
            self.dock.set_active_tab((s, n, t));
            self.dock.set_focused_node_and_surface((s, n));
        } else {
            if matches!(panel, Panel::Nodes | Panel::Workflow) {
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
            if self
                .dock
                .find_tab(&Panel::Canvas)
                .is_some_and(|(s, _, _)| !s.is_main())
            {
                self.editor.packets.clear();
            }
        }
        self.consume_ui_effects(ctx);
        if self.dock.find_tab(&Panel::Canvas).is_none() {
            self.editor.packets.clear();
            self.editor.canvas = Rect::NOTHING;
            self.editor.image_rect = Rect::NOTHING;
        }
        self.editor.shortcuts(ctx);
        self.run_actions(ctx);
        self.editor.blocked = self.settings_open
            || self.editor.palette.is_some()
            || self.editor.new_open
            || self.editor.command_form.is_some()
            || self.replay.open;
        self.editor.headers.clear();
        self.draw_shell(ctx);
        let a = self.editor.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        if self.dock.translations.tab_context_menu.close_button != crate::i18n::text("Hide Window")
        {
            self.dock.translations.tab_context_menu.close_button =
                localize("Hide Window").into_owned();
        }
        if self.dock.translations.tab_context_menu.eject_button != crate::i18n::text("Float Window")
        {
            self.dock.translations.tab_context_menu.eject_button =
                localize("Float Window").into_owned();
        }
        let main_docking_bounds = ctx.available_rect();
        ctx.data_mut(|d| d.insert_temp(Id::new("native-group-main-bounds"), main_docking_bounds));
        // Render the main tree independently; detached trees belong to real OS viewports.
        let separate = !ctx.embed_viewports();
        let mut main = DockState::new(vec![]);
        if separate {
            std::mem::swap(main.main_surface_mut(), self.dock.main_surface_mut());
            main.translations = self.dock.translations.clone();
        }
        CentralPanel::default()
            .frame(
                Frame::new()
                    .fill(colors.background)
                    .inner_margin(Margin::same(a.outer_padding as i8)),
            )
            .show(ctx, |ui| {
                let style = native_windows::dock_style(ctx, &a);
                let can_float = self.dock.iter_surfaces().filter(|s| !s.is_empty()).count() < 32;
                DockArea::new(if separate { &mut main } else { &mut self.dock })
                    .id(Id::new("micopi-workspace"))
                    .style(style)
                    .draggable_tabs(false)
                    .show_close_buttons(false)
                    .show_leaf_close_all_buttons(false)
                    .show_leaf_collapse_buttons(false)
                    .show_inside(
                        ui,
                        &mut panels::Viewer {
                            editor: &mut self.editor,
                            can_float,
                            native: None,
                            single_leaf: false,
                            focused: self.dock_before_focus.is_some(),
                        },
                    );
            });
        if separate {
            std::mem::swap(main.main_surface_mut(), self.dock.main_surface_mut());
            self.absorb_native_surfaces(main);
            self.render_native_windows(ctx);
        }
        let area = ctx.content_rect();
        let area_changed = ctx.data_mut(|data| {
            let key = Id::new("reported-plugin-window-area");
            let changed = data.get_temp::<Rect>(key) != Some(area);
            if changed {
                data.insert_temp(key, area);
            }
            changed
        });
        if self.editor.session.ui.get("windows.actual").is_none()
            || self.layout_changed.is_some()
            || area_changed
        {
            self.report_windows();
        }
        self.settings_window(ctx);
        self.editor.plugin_command_window(ctx);
        self.replay_window(ctx);
        self.editor.palette_ui(ctx);
        self.new_document_window(ctx);
        self.run_actions(ctx);
        if !ctx.input(|i| i.focused)
            && self.editor.input_owned
            && self
                .dock
                .find_tab(&Panel::Canvas)
                .is_some_and(|(s, _, _)| s.is_main())
        {
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
    saved.translations = egui_dock::Translations::english();
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
        Some("canvas") => Panel::Canvas,
        Some("tools") => Panel::Tools,
        Some("color") => Panel::Color,
        Some("brush") => Panel::Brush,
        Some("navigator") => Panel::Navigator,
        Some("layers") => Panel::Layers,
        Some("nodes") => Panel::Nodes,
        Some("workflow") => Panel::Workflow,
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
    if known.is_some_and(|ids| !ids.iter().any(|id| id == "toolbox"))
        && registry.panels.contains_key("toolbox")
    {
        // Upgrade the former broad left sidebar once; preserve closed/floating panels.
        for panel in [Panel::Color, Panel::Brush] {
            if let Some(location) = dock.find_tab(&panel).filter(|(s, _, _)| s.is_main()) {
                dock.remove_tab(location);
                if let Some((s, n, _)) = dock.find_tab(&Panel::Layers) {
                    dock.set_focused_node_and_surface((s, n));
                    dock.push_to_focused_leaf(panel);
                } else if dock.main_surface().is_empty() {
                    dock.push_to_first_leaf(panel);
                } else {
                    dock.main_surface_mut()
                        .split_right(NodeIndex::root(), 0.75, vec![panel]);
                }
            }
        }
        if dock.find_tab(&Panel::Tools).is_none() {
            if dock.main_surface().is_empty() {
                dock.push_to_first_leaf(Panel::Tools);
            } else {
                dock.main_surface_mut()
                    .split_above(NodeIndex::root(), 0.15, vec![Panel::Tools]);
            }
        }
    }
    let panels: Vec<_> = registry.panels.values().map(contributed_panel).collect();
    let remove: Vec<_> = dock
        .iter_all_tabs()
        .map(|(_, p)| *p)
        .filter(|p| !panels.contains(p))
        .collect();
    for p in remove {
        if let Some(location) = dock.find_tab(&p) {
            dock.remove_tab(location);
        }
    }
    for definition in registry.panels.values() {
        let panel = contributed_panel(definition);
        let new = known.is_none_or(|ids| !ids.contains(&definition.id));
        if new
            && !matches!(panel, Panel::Nodes | Panel::Workflow)
            && dock.find_tab(&panel).is_none()
        {
            if let Some((surface, node, _)) = dock.find_tab(&Panel::Layers) {
                let active = match &dock[surface][node] {
                    egui_dock::Node::Leaf(leaf) => Some(leaf.active),
                    _ => None,
                };
                dock.set_focused_node_and_surface((surface, node));
                dock.push_to_focused_leaf(panel);
                if let Some(active) = active {
                    dock.set_active_tab((surface, node, active));
                }
            } else {
                dock.push_to_focused_leaf(panel);
            }
        }
    }
}
fn valid_dock(dock: &DockState<Panel>) -> bool {
    if !matches!(
        dock.iter_surfaces().next(),
        Some(egui_dock::Surface::Main(_))
    ) || dock.iter_surfaces().count() > 32
    {
        return false;
    }
    for surface in dock.iter_surfaces() {
        if surface.is_empty() {
            continue;
        }
        let nodes: Vec<_> = surface.iter_nodes().collect();
        if nodes.is_empty() && matches!(surface, egui_dock::Surface::Main(_)) {
            continue;
        }
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
    tabs.iter().filter(|p| **p == Panel::Canvas).count() <= 1
        && tabs.len() <= 128
        && tabs.iter().enumerate().all(|(i, p)| !tabs[..i].contains(p))
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
            if then.elapsed() > Duration::from_millis(750)
                && !ctx.input(|i| i.pointer.any_down())
                && !self
                    .native_windows
                    .values()
                    .any(|w| w.native.as_ref().is_some_and(Native::is_sizing))
            {
                // Coalesce splitter/window drags into one registered presentation command.
                let layout = encode_workspace(
                    self.dock_before_focus.as_ref().unwrap_or(&self.dock),
                    &self.editor.session.registry,
                );
                if let Ok(layout) = layout {
                    if self.editor.core("workspace.layout.apply",serde_json::json!({"layout":serde_json::from_str::<serde_json::Value>(&layout).unwrap()})) {
                        if self.editor.session.ui_effects.back().is_some_and(|v|v["id"]=="window.layout") { self.editor.session.ui_effects.pop_back(); }
                        self.editor.session.ui["windows.result"]=serde_json::json!({"status":"applied","operation":"layout"});
                        self.report_windows();
                    }
                }
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
