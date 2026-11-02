//! Native Windows shell; no external crates or embedded browser runtime.
mod chrome;
mod command_palette;
#[cfg(feature = "desktop")]
pub mod desktop_bridge;
mod docking;
mod dpi;
mod ffi;
mod icons;
mod menu;
mod shell;
mod workbench;
use super::replace_file;
use crate::engine::input::{Sample, Stabilizer};
use crate::engine::{Brush, Document, MAX_SIDE};
use crate::ui::{Box2, Workspace};
use ffi::*;
use std::{
    cell::RefCell,
    mem,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
const TOP: i32 = crate::ui::CHROME_TOP;
const BOTTOM: i32 = crate::ui::CHROME_BOTTOM;
use crate::ui::{DOCUMENT_TABS, PANEL_HEADER};
const PALETTE: [u32; 20] = [
    0x252830, 0xffffff, 0x929aa8, 0xefe3cf, 0xe66b72, 0xed9b62, 0xe8ca76, 0xb1c978, 0x75b89b,
    0x71bcc5, 0x7699d4, 0x9990d3, 0xc68cc2, 0x8c5854, 0x6d7b62, 0x465e77, 0x3b393e, 0xbe7960,
    0xddd1bb, 0x536c65,
];
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn path_wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn color(c: u32) -> u32 {
    (c & 0xff00) | ((c >> 16) & 255) | ((c & 255) << 16)
}
fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn inside(r: Rect, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Brush,
    Eraser,
    Picker,
    Hand,
    Zoom,
}
struct Hit {
    r: Rect,
    id: u32,
}
struct Backbuffer {
    dc: Dc,
    bitmap: Handle,
    old: Handle,
    w: i32,
    h: i32,
}
impl Backbuffer {
    unsafe fn new(dc: Dc, w: i32, h: i32) -> Self {
        let m = CreateCompatibleDC(dc);
        let b = CreateCompatibleBitmap(dc, w.max(1), h.max(1));
        let old = SelectObject(m, b);
        Self {
            dc: m,
            bitmap: b,
            old,
            w,
            h,
        }
    }
}
impl Drop for Backbuffer {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}
struct App {
    keymap: crate::ui::keymap::Keymap,
    palette: Option<crate::ui::palette::Palette>,
    palette_edit: Hwnd,
    palette_font: Handle,
    maximized: bool,
    pressed_window: Option<u32>,
    dock_drag: Option<docking::Drag>,
    render_panel: Option<crate::ui::docking::Panel>,
    panel_scroll: [i32; 4],
    doc: Document,
    pixels: Vec<u32>,
    tool: Tool,
    ink: u32,
    background_ink: u32,
    opacity_input: crate::ui::shortcuts::OpacityInput,
    radius: f32,
    opacity: f32,
    hardness: f32,
    zoom: f32,
    pan: (f32, f32),
    size: (i32, i32),
    physical_size: (i32, i32),
    dpi: u32,
    fit_to_window: bool,
    hits: Vec<Hit>,
    drawing: bool,
    panning: bool,
    last_mouse: (i32, i32),
    slider: Option<u32>,
    file: Option<PathBuf>,
    status: String,
    icons: icons::Icons,
    font: Handle,
    title_font: Handle,
    small_font: Handle,
    hovered: Option<(u32, i32, i32)>,
    back: Option<Backbuffer>,
    viewport: Option<Backbuffer>,
    custom: [u32; 16],
    new_size: usize,
    layer_scroll: usize,
    workspace: Workspace,
    workspace_path: PathBuf,
    focus: bool,
    widget_offset: (i32, i32),
    widget_clip: Option<Rect>,
    ui_drag: Option<u8>,
    tools_scroll: i32,
    stabilizer: Stabilizer,
    pressure: f32,
    pen_id: Option<u32>,
    pen_eraser: bool,
    sample_time: u32,
    cursor_visible: bool,
    mouse_tracking: bool,
    cursor_override: bool,
    cursor_dark: Handle,
    cursor_light: Handle,
    scene_dirty: bool,
    wheel_pixels: Vec<u32>,
    wheel_key: (i32, i32, u32),
    color_hue: f32,
    color_last_ink: u32,
    control_drag: Option<u32>,
    nav_pixels: Vec<u32>,
}
impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            if !self.palette_edit.is_null() {
                DestroyWindow(self.palette_edit);
            }
            DeleteObject(self.palette_font);
            self.back.take();
            self.viewport.take();
            DeleteObject(self.font);
            DeleteObject(self.title_font);
            DeleteObject(self.small_font);
            DeleteObject(self.cursor_dark);
            DeleteObject(self.cursor_light);
        }
    }
}
impl App {
    unsafe fn new() -> Self {
        let face = wide("Microsoft YaHei UI");
        let font = CreateFontW(-14, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
        let title_font = CreateFontW(-14, 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
        let small_font = CreateFontW(-12, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
        let loaded = crate::ui::keymap::Keymap::load(&crate::ui::keymap::Keymap::path());
        let status = loaded
            .as_ref()
            .err()
            .map(|e| format!("Shortcut configuration error; using defaults: {}", e))
            .unwrap_or_else(|| "Ready · Start drawing".into());
        Self {
            keymap: loaded.unwrap_or_default(),
            palette: None,
            palette_edit: null_mut(),
            palette_font: null_mut(),
            maximized: false,
            pressed_window: None,
            dock_drag: None,
            render_panel: None,
            panel_scroll: [0; 4],
            doc: Document::new(1200, 900),
            pixels: vec![],
            tool: Tool::Brush,
            ink: 0x333743,
            background_ink: 0xffffff,
            opacity_input: Default::default(),
            radius: 6.,
            opacity: 1.,
            hardness: 0.85,
            zoom: 0.7,
            pan: (0., 0.),
            size: (1280, 900),
            physical_size: (1280, 900),
            dpi: 96,
            fit_to_window: true,
            hits: vec![],
            drawing: false,
            panning: false,
            last_mouse: (0, 0),
            slider: None,
            file: None,
            status,
            icons: icons::Icons::new(96),
            font,
            title_font,
            small_font,
            hovered: None,
            back: None,
            viewport: None,
            custom: [0; 16],
            new_size: 1200,
            layer_scroll: 0,
            workspace: Workspace::load(&Workspace::path()),
            workspace_path: Workspace::path(),
            focus: false,
            widget_offset: (0, 0),
            widget_clip: None,
            ui_drag: None,
            tools_scroll: 0,
            stabilizer: Stabilizer::default(),
            pressure: 1.,
            pen_id: None,
            pen_eraser: false,
            sample_time: 0,
            cursor_visible: false,
            mouse_tracking: false,
            cursor_override: false,
            cursor_dark: CreatePen(0, 3, 0),
            cursor_light: CreatePen(0, 1, 0xffffff),
            scene_dirty: true,
            wheel_pixels: vec![],
            wheel_key: (-1, -1, 0),
            color_hue: 0.,
            color_last_ink: 0,
            control_drag: None,
            nav_pixels: vec![],
        }
    }
    fn brush(&self) -> Brush {
        Brush {
            radius: self.radius,
            opacity: self.opacity,
            hardness: self.hardness,
            color: self.ink,
            erase: self.tool == Tool::Eraser || self.pen_eraser,
        }
    }
    fn view(&self) -> Rect {
        let v = self
            .workspace
            .layout(self.size.0, self.size.1, self.focus)
            .canvas;
        rect(v.x, v.y, v.w, v.h)
    }
    fn origin(&self) -> (f32, f32) {
        let v = self.view();
        (
            (v.left + v.right) as f32 / 2. - self.doc.width as f32 * self.zoom / 2. + self.pan.0,
            (v.top + v.bottom) as f32 / 2. - self.doc.height as f32 * self.zoom / 2. + self.pan.1,
        )
    }
    fn canvas_pos(&self, x: i32, y: i32) -> (f32, f32) {
        let o = self.origin();
        ((x as f32 - o.0) / self.zoom, (y as f32 - o.1) / self.zoom)
    }
    fn fit(&mut self) {
        self.fit_to_window = true;
        let v = self.view();
        self.zoom = (((v.right - v.left - 64) as f32 / self.doc.width as f32)
            .min((v.bottom - v.top - 64) as f32 / self.doc.height as f32))
        .max(0.05)
        .min(4.);
        self.pan = (0., 0.);
    }
    fn zoom_percent(&self) -> f32 {
        self.zoom * self.dpi as f32 / 96. * 100.
    }
    fn resize(&mut self, width: i32, height: i32, dpi: u32) {
        if width <= 0 || height <= 0 {
            return;
        }
        let old_dpi = self.dpi;
        self.dpi = dpi.max(96);
        if !self.fit_to_window && old_dpi != self.dpi {
            let f = old_dpi as f32 / self.dpi as f32;
            self.zoom *= f;
            self.pan.0 *= f;
            self.pan.1 *= f;
        }
        self.physical_size = (width, height);
        self.size = (
            (dpi::logical(width, self.dpi) - self.content_padding() * 2).max(1),
            (dpi::logical(height, self.dpi) - self.content_padding() * 2).max(1),
        );
        unsafe {
            self.position_palette();
        }
        self.hovered = None;
        self.hits.clear();
        self.scene_dirty = true;
        if self.fit_to_window {
            self.fit();
        }
    }
    fn zoom_by(&mut self, factor: f32) {
        self.fit_to_window = false;
        self.zoom = ((self.zoom * self.dpi as f32 / 96. * factor)
            .max(0.05)
            .min(16.))
            * 96.
            / self.dpi as f32;
    }
    fn zoom_at(&mut self, x: i32, y: i32, factor: f32) {
        let before = self.canvas_pos(x, y);
        self.zoom_by(factor);
        let after = self.canvas_pos(x, y);
        self.pan.0 += (after.0 - before.0) * self.zoom;
        self.pan.1 += (after.1 - before.1) * self.zoom;
    }
    fn select_tool(&mut self, tool: Tool) {
        self.tool = tool;
        let id = match tool {
            Tool::Brush => 10,
            Tool::Eraser => 11,
            Tool::Picker => 12,
            Tool::Hand => 13,
            Tool::Zoom => 14,
        };
        self.status = self.keymap.ui_hint(id).unwrap_or_default();
    }
    fn canvas_wheel(&mut self, x: i32, y: i32, delta: f32, alt: bool, shift: bool) {
        if alt {
            self.zoom_at(x, y, 1.15f32.powf(delta / 120.));
        } else {
            self.fit_to_window = false;
            if shift {
                self.pan.0 += delta / 120. * 48.;
            } else {
                self.pan.1 += delta / 120. * 48.;
            }
        }
    }
    fn content_padding(&self) -> i32 {
        chrome::content_padding(self.maximized)
    }
    fn client_point(&self, p: Point) -> Point {
        Point {
            x: dpi::physical(p.x + self.content_padding(), self.dpi),
            y: dpi::physical(p.y + self.content_padding(), self.dpi),
        }
    }
    fn client_rect(&self, r: Rect) -> Rect {
        let pad = self.content_padding();
        dpi::device_rect(
            Rect {
                left: r.left + pad,
                top: r.top + pad,
                right: r.right + pad,
                bottom: r.bottom + pad,
            },
            self.dpi,
        )
    }
    unsafe fn map_content(&self, dc: Dc) {
        dpi::map(dc, self.dpi);
        let pad = dpi::physical(self.content_padding(), self.dpi);
        SetViewportOrgEx(dc, pad, pad, null_mut());
    }
    fn logical_point(&self, p: Point) -> Point {
        Point {
            x: dpi::logical(p.x, self.dpi) - self.content_padding(),
            y: dpi::logical(p.y, self.dpi) - self.content_padding(),
        }
    }
    unsafe fn render_scene(&mut self, dc: Dc) {
        let saved = SaveDC(dc);
        Self::fill(
            dc,
            rect(0, 0, self.physical_size.0, self.physical_size.1),
            self.workspace.theme().panel,
        );
        self.map_content(dc);
        IntersectClipRect(dc, 0, 0, self.size.0, self.size.1);
        self.draw_frame(dc);
        RestoreDC(dc, saved);
    }
    unsafe fn fill(dc: Dc, r: Rect, c: u32) {
        let b = CreateSolidBrush(color(c));
        FillRect(dc, &r, b);
        DeleteObject(b);
    }
    unsafe fn text(dc: Dc, r: Rect, s: &str, c: u32) {
        SetTextColor(dc, color(c));
        let s = wide(&crate::i18n::translated(s));
        let mut r = r;
        DrawTextW(dc, s.as_ptr(), -1, &mut r, 0x24 | 0x8000);
    }
    fn hit(&mut self, r: Rect, id: u32) {
        let mut r = rect(
            r.left + self.widget_offset.0,
            r.top + self.widget_offset.1,
            r.right - r.left,
            r.bottom - r.top,
        );
        if let Some(c) = self.widget_clip {
            r.left = r.left.max(c.left);
            r.top = r.top.max(c.top);
            r.right = r.right.min(c.right);
            r.bottom = r.bottom.min(c.bottom);
        }
        if r.left < r.right && r.top < r.bottom {
            self.hits.push(Hit { r, id });
        }
    }
    unsafe fn paint(&mut self, hwnd: Hwnd) {
        let mut ps: Paint = mem::zeroed();
        let target = BeginPaint(hwnd, &mut ps);
        let new_back = self.back.as_ref().map_or(true, |b| {
            b.w != self.physical_size.0 || b.h != self.physical_size.1
        });
        if new_back {
            self.back = Some(Backbuffer::new(
                target,
                self.physical_size.0,
                self.physical_size.1,
            ));
        }
        let dc = self.back.as_ref().unwrap().dc;
        // Hover invalidates only the old/new cursor bounds. Reuse the clean scene;
        // all edits, layout changes and view changes invalidate the full client.
        if new_back || self.scene_dirty {
            self.scene_dirty = false;
            self.render_scene(dc);
        }
        BitBlt(
            target,
            ps.rect.left,
            ps.rect.top,
            ps.rect.right - ps.rect.left,
            ps.rect.bottom - ps.rect.top,
            dc,
            ps.rect.left,
            ps.rect.top,
            0xcc0020,
        );
        let saved = SaveDC(target);
        self.map_content(target);
        self.draw_brush_cursor(target);
        RestoreDC(target, saved);
        EndPaint(hwnd, &ps);
    }
    fn brush_cursor_rect(&self) -> Option<Rect> {
        if self.dock_drag.is_some()
            || (self.workspace.docking.enabled
                && self.workspace.docking.floating_at(
                    self.last_mouse.0,
                    self.last_mouse.1,
                    self.size.0,
                    self.size.1,
                    self.focus,
                    self.workspace.tools_visible,
                    self.workspace.layers_visible,
                ))
        {
            return None;
        }
        if self.palette.is_some() {
            return None;
        }
        let (x, y) = self.last_mouse;
        if !self.cursor_visible
            || self.cursor_override
            || !matches!(self.tool, Tool::Brush | Tool::Eraser)
            || self.panning
            || self.ui_drag.is_some()
            || self.dock_drag.is_some()
            || self.slider.is_some()
            || self.control_drag.is_some()
            || self
                .workspace
                .splitter_at(self.size.0, self.size.1, self.focus, x, y)
                .is_some()
            || !inside(self.view(), x, y)
        {
            return None;
        }
        let radius = if self.pen_id.is_some() && self.drawing {
            self.dynamic_brush(self.pressure).radius
        } else {
            self.radius
        };
        let r = (radius * self.zoom).max(0.5);
        Some(Rect {
            left: (x as f32 - r).floor() as i32,
            top: (y as f32 - r).floor() as i32,
            right: (x as f32 + r).ceil() as i32 + 1,
            bottom: (y as f32 + r).ceil() as i32 + 1,
        })
    }
    unsafe fn invalidate_scene(&mut self, hwnd: Hwnd) {
        self.scene_dirty = true;
        if !hwnd.is_null() {
            InvalidateRect(hwnd, null(), 0);
        }
    }
    fn cursor_damage(&self) -> Option<Rect> {
        self.brush_cursor_rect().map(|r| {
            rect(
                r.left - 5,
                r.top - 5,
                r.right - r.left + 10,
                r.bottom - r.top + 10,
            )
        })
    }
    unsafe fn invalidate_cursor(&self, hwnd: Hwnd, old: Option<Rect>) {
        let v = self.view();
        for r in [old, self.cursor_damage()].iter().flatten() {
            let r = Rect {
                left: r.left.max(v.left),
                top: r.top.max(v.top),
                right: r.right.min(v.right),
                bottom: r.bottom.min(v.bottom),
            };
            if r.left < r.right && r.top < r.bottom {
                InvalidateRect(hwnd, &self.client_rect(r), 0);
            }
        }
    }
    unsafe fn draw_brush_cursor(&self, dc: Dc) {
        if let Some(r) = self.brush_cursor_rect() {
            let saved = SaveDC(dc);
            let v = self.view();
            IntersectClipRect(dc, v.left, v.top, v.right, v.bottom);
            SelectObject(dc, GetStockObject(5)); // NULL_BRUSH: never fill the preview.
            for pen in [self.cursor_dark, self.cursor_light] {
                SelectObject(dc, pen);
                Ellipse(dc, r.left, r.top, r.right, r.bottom);
                {
                    let (x, y) = self.last_mouse;
                    let size = if r.right - r.left <= 7 { 4 } else { 2 };
                    MoveToEx(dc, x - size, y, null_mut());
                    LineTo(dc, x + size + 1, y);
                    MoveToEx(dc, x, y - size, null_mut());
                    LineTo(dc, x, y + size + 1);
                }
            }
            RestoreDC(dc, saved);
        }
    }
    fn cursor_at(&self, x: i32, y: i32, space: bool, alt: bool, ctrl: bool) -> usize {
        if self.palette.is_some() {
            return 32512;
        }
        if let Some(cursor) = self.dock_cursor(x, y) {
            return cursor;
        }
        if self.hits.iter().any(|h| {
            ((500..=503).contains(&h.id) || (4000..=4003).contains(&h.id)) && inside(h.r, x, y)
        }) {
            return 32646;
        }
        if matches!(self.ui_drag, Some(0) | Some(1))
            || self
                .workspace
                .splitter_at(self.size.0, self.size.1, self.focus, x, y)
                .is_some()
        {
            return 32644;
        }
        if matches!(self.ui_drag, Some(2) | Some(3)) {
            return 32646;
        }
        if self.panning {
            return 32649;
        }
        let l = self.workspace.layout(self.size.0, self.size.1, self.focus);
        if [l.tools, l.layers]
            .iter()
            .any(|p| p.w > 0 && x >= p.x && x < p.x + p.w && y >= p.y && y < p.y + PANEL_HEADER)
        {
            return 32646;
        }
        if inside(self.view(), x, y) && self.slider.is_none() && self.control_drag.is_none() {
            if (space && ctrl) || self.tool == Tool::Zoom {
                32515
            } else if space || self.tool == Tool::Hand {
                32649
            } else if self.tool == Tool::Picker || alt {
                32515
            } else {
                0
            }
        } else {
            32512
        }
    }
    unsafe fn set_cursor_at(&self, x: i32, y: i32) {
        let state = self.input_state();
        let cursor = self.cursor_at(
            x,
            y,
            state.pan || state.zoom || state.zoom_out,
            state.pick,
            state.zoom || state.zoom_out,
        );
        SetCursor(if cursor == 0 {
            null_mut()
        } else {
            LoadCursorW(null_mut(), cursor as *const u16)
        });
    }
    unsafe fn sync_cursor(&mut self, hwnd: Hwnd) {
        let old = self.cursor_damage();
        let state = self.input_state();
        self.cursor_override = state.pan || state.pick || state.zoom || state.zoom_out;
        self.invalidate_cursor(hwnd, old);
        if !self.cursor_visible {
            return;
        }
        self.set_cursor_at(self.last_mouse.0, self.last_mouse.1);
    }
    unsafe fn title(&self, hwnd: Hwnd) {
        let name = self
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        chrome::set_title(
            hwnd,
            &format!(
                "{}{} — MiCoPi",
                if self.doc.dirty { "* " } else { "" },
                name
            ),
        );
    }
    unsafe fn error(&mut self, hwnd: Hwnd, e: impl std::fmt::Display) {
        self.status = format!("Operation failed: {}", e);
        MessageBoxW(
            hwnd,
            wide(&self.status).as_ptr(),
            wide("MiCoPi").as_ptr(),
            0x10,
        );
    }
    unsafe fn confirm(&mut self, hwnd: Hwnd) -> bool {
        self.doc.finish();
        if !self.doc.dirty {
            return true;
        }
        match MessageBoxW(
            hwnd,
            wide("This artwork has unsaved changes. Save the project first?").as_ptr(),
            wide("MiCoPi").as_ptr(),
            3 | 0x20,
        ) {
            6 => self.save(hwnd, false),
            7 => true,
            _ => false,
        }
    }
    unsafe fn dialog(&self, hwnd: Hwnd, save: bool, png: bool) -> Option<PathBuf> {
        file_dialog(hwnd, save, png)
    }
    unsafe fn save(&mut self, hwnd: Hwnd, save_as: bool) -> bool {
        let path = if save_as || self.file.is_none() {
            self.dialog(hwnd, true, false)
        } else {
            self.file.clone()
        };
        if let Some(p) = path {
            match self.doc.save(&p) {
                Ok(()) => {
                    self.status = format!("Project saved: {}", p.display());
                    self.file = Some(p);
                    self.title(hwnd);
                    return true;
                }
                Err(e) => self.error(hwnd, e),
            }
        }
        false
    }
    unsafe fn command(&mut self, hwnd: Hwnd, id: u32) {
        self.opacity_input.reset();
        // UI commands may open nested modal loops before pen-up arrives. End ownership
        // first, so a swallowed pen-up cannot leave mouse input permanently blocked.
        if self.pen_id.is_some() {
            self.cancel_input(hwnd);
        }
        self.doc.finish();
        match id {
            7 => {
                self.save(hwnd, true);
            }
            85 => self.zoom_by(1.25),
            86 => self.zoom_by(1. / 1.25),
            90..=92 => {
                self.workbench_menu(hwnd, id);
                return;
            }
            820..=825 => self.apply_preset((id - 820) as usize),
            840..=849 => self.size_preset((id - 840) as usize),
            50 => {
                self.settings_menu(hwnd);
                return;
            }
            51 => self.workspace.dynamics.size = !self.workspace.dynamics.size,
            52 => self.workspace.dynamics.opacity = !self.workspace.dynamics.opacity,
            53 => {
                self.workspace.dynamics.gamma = if self.workspace.dynamics.gamma < 0.9 {
                    1.
                } else if self.workspace.dynamics.gamma < 1.3 {
                    1.7
                } else {
                    0.65
                }
            }
            60 => self.workspace.tools_visible = !self.workspace.tools_visible,
            61 => self.workspace.layers_visible = !self.workspace.layers_visible,
            62 => {
                self.workspace.swapped = !self.workspace.swapped;
                if self.workspace.docking.enabled {
                    self.workspace.docking.swap();
                }
            }
            63..=65 => {
                self.workspace.theme = (id - 63) as u8;
                self.workspace.accent = None;
            }
            66 => {
                let mut cc: ChooseColor = mem::zeroed();
                cc.size = mem::size_of::<ChooseColor>() as u32;
                cc.owner = hwnd;
                cc.result = color(self.workspace.theme().accent);
                cc.custom = self.custom.as_mut_ptr();
                cc.flags = 3;
                if ChooseColorW(&mut cc) != 0 {
                    self.workspace.accent = Some(color(cc.result));
                }
            }
            67 => {
                self.workspace = Workspace::default();
                self.workspace.enable_docking();
                self.focus = false;
                self.tools_scroll = 0;
            }
            68 => {
                self.workspace = Workspace::load(&self.workspace_path);
                self.tools_scroll = 0;
            }
            69 => self.focus = !self.focus,
            10110 | 10111 => {
                self.focus = false;
                self.workspace.tools_visible = true;
                self.workspace.tools_page = if id == 10111 { 1 } else { 0 };
                if self.workspace.docking.enabled {
                    let p = if id == 10111 {
                        crate::ui::docking::Panel::Brush
                    } else {
                        crate::ui::docking::Panel::Color
                    };
                    self.workspace.docking.select(p);
                    self.workspace.docking.raise(p);
                }
                self.tools_scroll = 0;
                self.save_workspace();
            }
            10120 => {
                if self.workspace.docking.enabled {
                    self.workspace.docking.enabled = false
                } else {
                    self.workspace.enable_docking();
                }
                self.save_workspace();
            }
            10122 | 10123 => {
                self.workspace.enable_docking();
                let (panel, with) = if id == 10122 {
                    (
                        crate::ui::docking::Panel::Brush,
                        crate::ui::docking::Panel::Color,
                    )
                } else {
                    (
                        crate::ui::docking::Panel::Layers,
                        crate::ui::docking::Panel::Navigator,
                    )
                };
                self.workspace.docking.move_group(
                    panel,
                    crate::ui::docking::Target::Tab {
                        with,
                        index: 1,
                        preview: self.panel_box(with),
                    },
                );
                self.save_workspace();
            }
            10121 => {
                self.workspace.docking = Default::default();
                self.workspace.enable_docking();
                self.panel_scroll = [0; 4];
                self.save_workspace();
            }
            72 => self.workspace.dynamics.gamma = 0.65,
            73 => self.workspace.dynamics.gamma = 1.,
            74 => self.workspace.dynamics.gamma = 1.7,
            75 => self.workspace.stabilizer = 0.,
            76 => self.workspace.stabilizer = 0.35,
            77 => self.workspace.stabilizer = 0.7,
            78 => self.workspace.stabilizer = 1.,
            79 => {
                self.save_workspace();
                MessageBoxW(
                    hwnd,
                    wide(&format!(
                        "Workspace configuration:\n{}\n\nReload from the Workspace menu after editing.",
                        self.workspace_path.display()
                    ))
                    .as_ptr(),
                    wide("MiCoPi Workspace Configuration").as_ptr(),
                    0,
                );
            }
            1 => {
                if self.confirm(hwnd) {
                    self.doc = Document::new(self.new_size, self.new_size);
                    self.layer_scroll = 0;
                    self.pixels.clear();
                    self.file = None;
                    self.fit();
                    self.status = "New Canvas Ready".into();
                }
            }
            2 => {
                if self.confirm(hwnd) {
                    if let Some(p) = self.dialog(hwnd, false, false) {
                        let project = p
                            .extension()
                            .map_or(false, |s| s.to_string_lossy().eq_ignore_ascii_case("mcp"));
                        let result = if project {
                            Document::load(&p)
                        } else {
                            load_image(&p)
                        };
                        match result {
                            Ok(d) => {
                                self.doc = d;
                                self.layer_scroll = 0;
                                self.pixels.clear();
                                self.file = if project { Some(p) } else { None };
                                self.fit();
                                self.status = "Artwork Opened".into();
                            }
                            Err(e) => self.error(hwnd, e),
                        }
                    }
                }
            }
            3 => {
                self.save(hwnd, false);
            }
            4 => {
                if let Some(p) = self.dialog(hwnd, true, true) {
                    self.doc.compose();
                    match save_png(&p, &mut self.doc.composite, self.doc.width, self.doc.height) {
                        Ok(()) => self.status = format!("PNG exported: {}", p.display()),
                        Err(e) => self.error(hwnd, e),
                    }
                }
            }
            5 => self.doc.undo(),
            6 => self.doc.redo(),
            10 => self.select_tool(Tool::Brush),
            11 => self.select_tool(Tool::Eraser),
            12 => self.select_tool(Tool::Picker),
            13 => self.select_tool(Tool::Hand),
            14 => self.select_tool(Tool::Zoom),
            93 => self.open_palette(hwnd, crate::ui::palette::Mode::Bindings),
            95 => self.open_palette(hwnd, crate::ui::palette::Mode::Commands),
            98 => {
                MessageBoxW(
                    hwnd,
                    wide(&crate::ui::keymap::Keymap::path().display().to_string()).as_ptr(),
                    wide("settings.json Location").as_ptr(),
                    0,
                );
            }
            96 => match crate::ui::keymap::Keymap::load(&crate::ui::keymap::Keymap::path()) {
                Ok(map) => {
                    self.keymap = map;
                    self.status = "Shortcuts Reloaded".into();
                }
                Err(e) => self.error(hwnd, e),
            },
            97 => {
                if MessageBoxW(
                    hwnd,
                    wide("Restore and Save All Default Shortcuts?").as_ptr(),
                    wide("Shortcut").as_ptr(),
                    0x24,
                ) == 6
                {
                    match crate::ui::keymap::Keymap::default()
                        .save(&crate::ui::keymap::Keymap::path())
                    {
                        Ok(()) => {
                            self.keymap = Default::default();
                            self.status = "Default Shortcuts Restored".into();
                        }
                        Err(e) => self.error(hwnd, e),
                    }
                }
            }
            30 => {
                let mut cc: ChooseColor = mem::zeroed();
                cc.size = mem::size_of::<ChooseColor>() as u32;
                cc.owner = hwnd;
                cc.result = color(self.ink);
                cc.custom = self.custom.as_mut_ptr();
                cc.flags = 1 | 2;
                if ChooseColorW(&mut cc) != 0 {
                    self.ink = color(cc.result);
                }
            }
            40 => self.fit(),
            41 => {
                self.fit_to_window = false;
                self.zoom = 96. / self.dpi as f32;
                self.pan = (0., 0.);
            }
            42 => {
                self.doc.add_layer();
                self.layer_scroll = 0;
            }
            43 => {
                if MessageBoxW(
                    hwnd,
                    wide("Clear the Current Layer? This Can Be Undone.").as_ptr(),
                    wide("MiCoPi").as_ptr(),
                    1 | 0x20,
                ) == 1
                {
                    self.doc.clear_layer();
                }
            }
            44 | 45 => {
                let l = &mut self.doc.layers[self.doc.active];
                l.opacity = if id == 44 {
                    l.opacity.saturating_sub(26)
                } else {
                    l.opacity.saturating_add(26)
                };
                self.doc.invalidate();
                self.doc.dirty = true;
            }
            46 => {
                self.new_size = match self.new_size {
                    512 => 1024,
                    1024 => 1200,
                    1200 => 2048,
                    _ => 512,
                };
            }
            100..=119 => self.ink = PALETTE[(id - 100) as usize],
            200..=215 => {
                let i = (id - 200) as usize;
                if i < self.doc.layers.len() {
                    self.doc.active = i;
                }
            }
            300..=315 => {
                let i = (id - 300) as usize;
                if i < self.doc.layers.len() {
                    self.doc.layers[i].visible = !self.doc.layers[i].visible;
                    self.doc.invalidate();
                    self.doc.dirty = true;
                }
            }
            _ => {}
        }
        if (51..=78).contains(&id) {
            self.save_workspace();
        }
        if matches!(id, 63..=68) {
            chrome::appearance(hwnd, self.workspace.theme(), self.workspace.theme == 2);
        }
        if self.fit_to_window {
            self.fit();
        }
        self.sync_cursor(hwnd);
        self.title(hwnd);
        self.invalidate_scene(hwnd);
    }
    fn dynamic_brush(&self, pressure: f32) -> Brush {
        if self.pen_id.is_some() {
            self.workspace.dynamics.apply(self.brush(), pressure)
        } else {
            self.brush()
        }
    }
    fn save_workspace(&mut self) {
        if let Err(e) = self.workspace.save(&self.workspace_path) {
            self.status = format!("Workspace configuration save failed: {}", e);
        }
    }
    unsafe fn settings_menu(&mut self, hwnd: Hwnd) {
        let mut popup = menu::Popup::new(self.workspace.theme(), self.dpi);
        let menu = popup.root;
        let panels = popup.submenu(menu, "Panels and Layout");
        let themes = popup.submenu(menu, "UI Theme");
        let pen = popup.submenu(menu, "Pen Pressure");
        let smooth = popup.submenu(menu, "Brush Stabilizer");
        let mut item = |parent: Handle, id: u32, label: &str, checked: bool| {
            popup.item(parent, id, label, checked, &self.keymap)
        };
        item(panels, 60, "Show Tools Panel", self.workspace.tools_visible);
        item(
            panels,
            61,
            "Show Layers Panel",
            self.workspace.layers_visible,
        );
        item(panels, 62, "Swap Sides", self.workspace.swapped);
        item(panels, 69, &self.keymap.ui_hint(69).unwrap(), self.focus);
        item(panels, 67, "Reset UI and Brush Settings", false);
        item(panels, 68, "Reload Configuration", false);
        item(panels, 79, "Configuration Location…", false);
        item(
            panels,
            10120,
            "Free Panel Layout",
            self.workspace.docking.enabled,
        );
        item(panels, 10121, "Reset Panel Docking", false);
        item(panels, 10122, "Group Color and Brush Tabs", false);
        item(panels, 10123, "Group Navigator and Layer Tabs", false);
        for (i, label) in ["Mist", "Warm Paper", "Graphite"].iter().enumerate() {
            item(
                themes,
                63 + i as u32,
                label,
                self.workspace.theme == i as u8,
            );
        }
        item(
            themes,
            66,
            "Custom Accent Color…",
            self.workspace.accent.is_some(),
        );
        item(
            pen,
            51,
            "Pressure Controls Size",
            self.workspace.dynamics.size,
        );
        item(pen, 52, "Pressure Opacity", self.workspace.dynamics.opacity);
        for (id, label, gamma) in [
            (72, "Soft Curve · Thicker at Light Pressure", 0.65),
            (73, "Linear Curve", 1.),
            (74, "Firm Curve · Thicker at High Pressure", 1.7),
        ] {
            item(
                pen,
                id,
                label,
                (self.workspace.dynamics.gamma - gamma).abs() < 0.01,
            );
        }
        for (id, label, strength) in [
            (75, "Off · 0%", 0.),
            (76, "Light · 35%", 0.35),
            (77, "Medium · 70%", 0.7),
            (78, "Strong · 100%", 1.),
        ] {
            item(
                smooth,
                id,
                label,
                (self.workspace.stabilizer - strength).abs() < 0.01,
            );
        }
        item(menu, 93, "Shortcut Reference…", false);
        let mut point = Point {
            x: if self.last_mouse.1 < TOP {
                self.last_mouse.0.min(self.size.0 - 220).max(8)
            } else {
                288
            },
            y: TOP,
        };
        point = self.client_point(point);
        ClientToScreen(hwnd, &mut point);
        let id = popup.track(hwnd, point);
        drop(popup);
        if id > 0 {
            self.command(hwnd, id as u32);
        }
    }
    unsafe fn cancel_input(&mut self, hwnd: Hwnd) {
        if let Some(drag) = self.dock_drag.take() {
            self.workspace.docking = drag.before;
            if self.fit_to_window {
                self.fit();
            }
        }
        self.pressed_window = None;
        self.hovered = None;
        self.cursor_visible = false;
        self.stabilizer.cancel();
        self.doc.finish();
        self.drawing = false;
        self.panning = false;
        let save = self.slider.take() == Some(23);
        self.control_drag = None;
        if self.ui_drag.take().is_some() || save {
            self.save_workspace();
        }
        let mouse = self.pen_id.is_none();
        self.pen_id = None;
        self.pen_eraser = false;
        self.pressure = 1.;
        if mouse {
            ReleaseCapture();
        }
        self.title(hwnd);
        self.invalidate_scene(hwnd);
    }
    unsafe fn pen_sample(&mut self, hwnd: Hwnd, info: PenInfo, move_input: bool) {
        let mut p = info.pointer.pixel;
        ScreenToClient(hwnd, &mut p);
        let p = self.logical_point(p);
        self.pressure = if info.mask & 1 != 0 {
            info.pressure.min(1024) as f32 / 1024.
        } else {
            1.
        };
        self.sample_time = info.pointer.time;
        self.pen_eraser = info.flags & (2 | 4) != 0;
        if move_input {
            self.motion(hwnd, p.x, p.y);
        } else {
            self.down(hwnd, p.x, p.y, false);
        }
    }
    unsafe fn pointer(&mut self, hwnd: Hwnd, msg: u32, id: u32) -> bool {
        let mut info: PenInfo = mem::zeroed();
        if GetPointerPenInfo(id, &mut info) == 0 {
            return false;
        }
        if info.pointer.flags & 0x8000 != 0 {
            if self.pen_id == Some(id) {
                self.cancel_input(hwnd);
            }
            return true;
        }
        match msg {
            0x246 => {
                if self.pen_id.is_none() && !self.drawing && !self.panning {
                    self.pen_id = Some(id);
                    self.pen_sample(hwnd, info, false);
                }
            }
            0x245 => {
                if self.pen_id == Some(id) {
                    // The OS returns newest first. Replay in chronological order; fixed stack buffer.
                    let mut history: [PenInfo; 64] = mem::zeroed();
                    let mut count = history.len() as u32;
                    if GetPointerPenInfoHistory(id, &mut count, history.as_mut_ptr()) != 0 {
                        for s in history[..(count as usize).min(64)].iter().rev() {
                            if s.pointer.flags & 4 != 0 {
                                self.pen_sample(hwnd, *s, true);
                            }
                        }
                    } else if info.pointer.flags & 4 != 0 {
                        self.pen_sample(hwnd, info, true);
                    }
                } else if self.pen_id.is_none() && !self.drawing && !self.panning {
                    let mut p = info.pointer.pixel;
                    ScreenToClient(hwnd, &mut p);
                    let p = self.logical_point(p);
                    self.pen_eraser = info.flags & (2 | 4) != 0;
                    self.motion(hwnd, p.x, p.y);
                }
            }
            0x247 => {
                if self.pen_id == Some(id) {
                    // Lift samples commonly report pressure zero; retain the last contact's pressure.
                    let mut p = info.pointer.pixel;
                    ScreenToClient(hwnd, &mut p);
                    let p = self.logical_point(p);
                    self.sample_time = info.pointer.time;
                    if self.drawing
                        || self.panning
                        || self.ui_drag.is_some()
                        || self.slider.is_some()
                        || self.control_drag.is_some()
                    {
                        self.motion(hwnd, p.x, p.y);
                    }
                    self.up(hwnd);
                    self.pen_id = None;
                    self.pen_eraser = false;
                    self.pressure = 1.;
                }
            }
            0x249 => {
                if self.pen_id.is_none() {
                    let mut p = info.pointer.pixel;
                    ScreenToClient(hwnd, &mut p);
                    let p = self.logical_point(p);
                    self.motion(hwnd, p.x, p.y);
                }
            }
            0x24a => {
                if self.pen_id.is_none() {
                    if self.hovered.take().is_some() {
                        self.invalidate_scene(hwnd);
                    }
                    let old = self.cursor_damage();
                    self.cursor_visible = false;
                    self.invalidate_cursor(hwnd, old);
                }
            }
            _ => {}
        }
        true
    }
    fn adjust(&mut self, id: u32, x: i32) {
        let p = self.panel_box(crate::ui::docking::Panel::Brush);
        let t = ((x - p.x - 76) as f32 / (p.w - 126) as f32).max(0.).min(1.);
        match id {
            20 => self.radius = (1. + t * 199.) / 2.,
            21 => self.opacity = t,
            22 => self.hardness = t,
            23 => self.workspace.stabilizer = t,
            _ => {}
        }
    }
    unsafe fn down(&mut self, hwnd: Hwnd, x: i32, y: i32, middle: bool) {
        if !middle && y >= 0 && y < TOP {
            if let Some(id) = (chrome::MINIMIZE..=chrome::CLOSE)
                .find(|id| inside(chrome::button(self.size.0, *id), x, y))
            {
                self.pressed_window = Some(id);
                self.last_mouse = (x, y);
                SetCapture(hwnd);
                return;
            }
        }

        if self.palette.is_some() {
            self.palette_click(hwnd, x, y);
            return;
        }
        self.opacity_input.reset();
        let state = self.input_state();
        let old = self.cursor_damage();
        self.cursor_visible = true;
        self.last_mouse = (x, y);
        self.invalidate_cursor(hwnd, old);
        if self.drawing || self.panning || self.ui_drag.is_some() {
            return;
        }
        if middle && self.workspace.docking.enabled && self.dock_layout().top_at(x, y).is_some() {
            return;
        }
        if !middle && self.dock_down(hwnd, x, y) {
            return;
        }
        if !middle && !self.workspace.docking.enabled {
            if let Some(i) = self
                .workspace
                .splitter_at(self.size.0, self.size.1, self.focus, x, y)
            {
                self.ui_drag = Some(i);
                if self.pen_id.is_none() {
                    SetCapture(hwnd);
                }
                self.sync_cursor(hwnd);
                return;
            }
        }
        if !middle {
            if let Some(id) = self
                .hits
                .iter()
                .rev()
                .find(|h| inside(h.r, x, y))
                .map(|h| h.id)
            {
                if (800..=805).contains(&id) || id == 810 {
                    self.control_drag = Some(id);
                    self.adjust_control(id, x, y);
                    if self.pen_id.is_none() {
                        SetCapture(hwnd);
                    }
                    self.invalidate_scene(hwnd);
                    return;
                }
                if (500..=503).contains(&id) {
                    let panel = match id {
                        500 => {
                            if self.workspace.tools_page == 1 {
                                crate::ui::docking::Panel::Brush
                            } else {
                                crate::ui::docking::Panel::Color
                            }
                        }
                        501 => crate::ui::docking::Panel::Navigator,
                        502 => crate::ui::docking::Panel::Brush,
                        _ => crate::ui::docking::Panel::Layers,
                    };
                    self.start_panel_drag(hwnd, panel, x, y);
                    return;
                }
                if (20..=23).contains(&id) {
                    self.slider = Some(id);
                    self.adjust(id, x);
                    if self.pen_id.is_none() {
                        SetCapture(hwnd);
                    }
                    self.invalidate_scene(hwnd);
                } else {
                    PostMessageW(hwnd, 0x8001, id as usize, 0);
                }
                return;
            }
        }
        if !inside(self.view(), x, y) {
            return;
        }
        if !middle && (self.tool == Tool::Zoom || state.zoom || state.zoom_out) {
            self.zoom_at(
                x,
                y,
                if state.zoom_out || (self.tool == Tool::Zoom && state.tool_zoom_out) {
                    1. / 1.25
                } else {
                    1.25
                },
            );
            self.invalidate_scene(hwnd);
            self.sync_cursor(hwnd);
            return;
        }
        if middle || state.pan || self.tool == Tool::Hand {
            self.panning = true;
            if self.pen_id.is_none() {
                SetCapture(hwnd);
            }
            return;
        }
        let (cx, cy) = self.canvas_pos(x, y);
        if cx < 0. || cy < 0. || cx >= self.doc.width as f32 || cy >= self.doc.height as f32 {
            return;
        }
        if self.tool == Tool::Picker || state.pick {
            self.doc.compose();
            let c = self.doc.composite[cy as usize * self.doc.width + cx as usize];
            if c >> 24 > 0 {
                self.ink = c & 0xffffff;
                self.status = format!("Picked Color  #{:06X}", self.ink);
            }
            self.invalidate_scene(hwnd);
            return;
        }
        if !self.doc.layers[self.doc.active].visible {
            self.status = "Current Layer Is Hidden; Enable Visibility First".into();
            self.invalidate_scene(hwnd);
            return;
        }
        self.drawing = true;
        if self.pen_id.is_none() {
            SetCapture(hwnd);
            self.pressure = 1.;
        }
        let sample = Sample {
            x: cx,
            y: cy,
            pressure: self.pressure,
            time: if self.pen_id.is_some() {
                self.sample_time
            } else {
                GetMessageTime() as u32
            },
        };
        let sample = self.stabilizer.reset(sample);
        self.doc
            .begin(sample.x, sample.y, self.dynamic_brush(sample.pressure));
        self.invalidate_scene(hwnd);
    }
    unsafe fn motion(&mut self, hwnd: Hwnd, x: i32, y: i32) {
        if self.dock_motion(hwnd, x, y) {
            return;
        }
        if self.palette.is_some() {
            return;
        }
        let hovered = self
            .hits
            .iter()
            .rev()
            .find(|h| inside(h.r, x, y))
            .map(|h| (h.id, h.r.left, h.r.top));
        if self.hovered != hovered {
            self.hovered = hovered;
            self.invalidate_scene(hwnd);
        }
        let old = self.cursor_damage();
        self.cursor_visible = true;
        if let Some(id) = self.control_drag {
            self.adjust_control(id, x, y);
            self.invalidate_scene(hwnd);
        } else if let Some(drag) = self.ui_drag {
            if drag < 2 {
                let p = if drag == 0 {
                    self.workspace
                        .layout(self.size.0, self.size.1, self.focus)
                        .tools
                } else {
                    self.workspace
                        .layout(self.size.0, self.size.1, self.focus)
                        .layers
                };
                let width = if p.x == crate::ui::ACTIVITY_WIDTH {
                    x - p.x
                } else {
                    self.size.0 - x
                };
                if drag == 0 {
                    self.workspace.tools_width = width.max(224).min(360);
                } else {
                    self.workspace.layers_width = width.max(208).min(360);
                }
                if self.fit_to_window {
                    self.fit();
                }
                self.invalidate_scene(hwnd);
            }
        } else if let Some(id) = self.slider {
            self.adjust(id, x);
            self.invalidate_scene(hwnd);
        } else if self.panning {
            self.fit_to_window = false;
            self.pan.0 += (x - self.last_mouse.0) as f32;
            self.pan.1 += (y - self.last_mouse.1) as f32;
            self.invalidate_scene(hwnd);
        } else if self.drawing {
            let (cx, cy) = self.canvas_pos(x, y);
            let sample = self.stabilizer.push(
                Sample {
                    x: cx,
                    y: cy,
                    pressure: self.pressure,
                    time: if self.pen_id.is_some() {
                        self.sample_time
                    } else {
                        GetMessageTime() as u32
                    },
                },
                self.workspace.stabilizer,
            );
            self.doc
                .line(sample.x, sample.y, self.dynamic_brush(sample.pressure));
            self.invalidate_scene(hwnd);
        }
        self.last_mouse = (x, y);
        self.invalidate_cursor(hwnd, old);
        self.sync_cursor(hwnd);
    }
    unsafe fn up(&mut self, hwnd: Hwnd) {
        if self.dock_up(hwnd) {
            return;
        }
        if let Some(id) = self.pressed_window.take() {
            ReleaseCapture();
            if inside(
                chrome::button(self.size.0, id),
                self.last_mouse.0,
                self.last_mouse.1,
            ) {
                PostMessageW(
                    hwnd,
                    0x112,
                    match id {
                        chrome::MINIMIZE => 0xf020,
                        chrome::MAXIMIZE => {
                            if self.maximized {
                                0xf120
                            } else {
                                0xf030
                            }
                        }
                        _ => 0xf060,
                    },
                    0,
                );
            }
            return;
        }

        if self.drawing {
            if let Some(s) = self.stabilizer.finish() {
                self.doc.line(s.x, s.y, self.dynamic_brush(s.pressure));
            }
        }
        if let Some(drag) = self.ui_drag {
            if drag >= 2 {
                self.workspace.swapped = if drag == 2 {
                    self.last_mouse.0 > self.size.0 / 2
                } else {
                    self.last_mouse.0 < self.size.0 / 2
                };
            }
        }
        if self.ui_drag.is_some() || self.slider == Some(23) {
            self.save_workspace();
        }
        if self.ui_drag.is_some() && self.fit_to_window {
            self.fit();
        }
        self.ui_drag = None;
        self.doc.finish();
        self.drawing = false;
        self.panning = false;
        self.slider = None;
        self.control_drag = None;
        if self.pen_id.is_none() {
            ReleaseCapture();
        }
        self.title(hwnd);
        self.invalidate_scene(hwnd);
    }
    unsafe fn key(&mut self, hwnd: Hwnd, key: usize) -> bool {
        if self.palette.is_some() {
            self.palette_key(hwnd, key);
            return true;
        }
        let mods = command_palette::modifiers();
        if self.keymap.hold_event(key, mods) {
            return true;
        }
        let action = match self.keymap.resolve(key, mods) {
            Some(a) => a,
            None => return false,
        };
        if self.drawing
            || self.panning
            || self.ui_drag.is_some()
            || self.dock_drag.is_some()
            || self.slider.is_some()
            || self.control_drag.is_some()
        {
            if action == crate::ui::shortcuts::Action::Cancel {
                self.cancel_input(hwnd);
                return true;
            }
            return false;
        }
        self.execute_action(hwnd, action);
        true
    }
    unsafe fn execute_action(&mut self, hwnd: Hwnd, action: crate::ui::shortcuts::Action) {
        use crate::ui::shortcuts::{self, Action::*};
        if !matches!(action, OpacityDigit(_)) {
            self.opacity_input.reset();
        }
        match action {
            Brush => self.select_tool(Tool::Brush),
            Eraser => self.select_tool(Tool::Eraser),
            Picker => self.select_tool(Tool::Picker),
            Hand => self.select_tool(Tool::Hand),
            Zoom => self.select_tool(Tool::Zoom),
            Undo => self.command(hwnd, 5),
            Redo => self.command(hwnd, 6),
            New => self.command(hwnd, 1),
            Open => self.command(hwnd, 2),
            Save => self.command(hwnd, 3),
            SaveAs => self.command(hwnd, 7),
            Export => self.command(hwnd, 4),
            NewLayer => self.command(hwnd, 42),
            Fit => self.command(hwnd, 40),
            ActualPixels => self.command(hwnd, 41),
            ZoomIn => self.command(hwnd, 85),
            ZoomOut => self.command(hwnd, 86),
            Preferences => self.command(hwnd, 50),
            ShowShortcuts => self.command(hwnd, 93),
            TogglePanels => self.command(hwnd, 69),
            ToggleLayers => self.command(hwnd, 61),
            SmallerBrush => self.radius = shortcuts::brush_diameter(self.radius * 2., false) / 2.,
            LargerBrush => self.radius = shortcuts::brush_diameter(self.radius * 2., true) / 2.,
            SofterBrush => self.hardness = (self.hardness - 0.25).max(0.),
            HarderBrush => self.hardness = (self.hardness + 0.25).min(1.),
            OpacityDigit(n) => {
                if matches!(self.tool, Tool::Brush | Tool::Eraser) {
                    self.opacity = self.opacity_input.digit(n, GetMessageTime() as u32);
                }
            }
            DefaultColors => {
                self.ink = 0;
                self.background_ink = 0xffffff;
            }
            SwapColors => std::mem::swap(&mut self.ink, &mut self.background_ink),
            OpenPalette => self.command(hwnd, 95),
            ReloadBindings => self.command(hwnd, 96),
            ResetBindings => self.command(hwnd, 97),
            Cancel => self.cancel_input(hwnd),
            UiCommand(id) => self.command(hwnd, id),
        }
        self.invalidate_scene(hwnd);
    }
}
use std::os::windows::ffi::OsStringExt;
thread_local! {static APP:RefCell<Option<App>>=RefCell::new(None);}
unsafe extern "system" fn window_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    if matches!(msg, 0x2b | 0x2c) {
        if let Some(result) = menu::message(msg, l) {
            return result;
        }
    }
    // Handle frame messages without borrowing App: system move/resize may reenter.
    if matches!(msg, 0x0c | 0x80) {
        // WM_SETTEXT / WM_SETICON also draw legacy captions.
        return chrome::native_metadata(hwnd, msg, w, l);
    }
    if msg == 0x83 {
        chrome::calculate_client(hwnd, l);
        return 0;
    } // WM_NCCALCSIZE: keep maximized clients inside the visible work area.
    if msg == 0x84 {
        return chrome::hit_window(hwnd, l);
    }
    if matches!(msg, 0x85 | 0xae | 0xaf) {
        // WM_NCPAINT and undocumented UAH caption/frame paints belong to the
        // custom shell. DWM composition remains enabled independently.
        return 0;
    }
    if msg == 0x86 {
        return 1; // Accept activation without invoking the native title painter.
    }
    if msg == 0x24 {
        chrome::minmax(hwnd, l);
        return 0;
    }
    if matches!(msg, 0x31a | 0x31e) {
        // Theme/composition changed: restore compositor settings.
        PostMessageW(hwnd, 0x8014, 0, 0);
        return DefWindowProcW(hwnd, msg, w, l);
    }
    if msg == 0x111 && l != 0 && (w >> 16) == 0x300 {
        PostMessageW(hwnd, 0x8012, 0, 0);
        return 0;
    }
    // Per-window tablet gestures can delay ink; leave global OS settings untouched.
    if msg == 0x02cc {
        return 0x1 | 0x8 | 0x10 | 0x10000 | 0x80000 | 0x100000;
    }
    // DefWindowProc synchronously emits WM_SIZE from WM_WINDOWPOSCHANGED.
    // Forward system-owned messages before borrowing App, or the nested WM_SIZE
    // would be rejected by try_borrow_mut and silently lose the layout update.
    if !matches!(
        msg,
        0x245
            | 0x246
            | 0x247
            | 0x249
            | 0x24a
            | 0x24c
            | 0x0f
            | 0x14
            | 0x05
            | 0x02e0
            | 0x24
            | 0x201
            | 0x203
            | 0x207
            | 0x200
            | 0x2a3
            | 0x202
            | 0x208
            | 0x215
            | 0x08
            | 0x20
            | 0x20a
            | 0x100
            | 0x101
            | 0x104
            | 0x105
            | 0x8010
            | 0x8011
            | 0x8012
            | 0x8013
            | 0x8014
            | 0x8001
            | 0x10
    ) {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    // Non-client resize cursors and system keys are also owned by Windows.
    if msg == 0x20 && (l & 0xffff) != 1 {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    if msg == 0x104 || msg == 0x105 {
        let handled = APP.with(|slot| {
            if let Ok(mut state) = slot.try_borrow_mut() {
                if let Some(app) = state.as_mut() {
                    let handled = msg == 0x104 && app.key(hwnd, w);
                    app.sync_cursor(hwnd);
                    return handled;
                }
            }
            false
        });
        if handled {
            return 0;
        }
        return DefWindowProcW(hwnd, msg, w, l);
    }
    APP.with(|slot| {
        let mut slot = match slot.try_borrow_mut() {
            Ok(s) => s,
            Err(_) => return DefWindowProcW(hwnd, msg, w, l),
        };
        let app = match slot.as_mut() {
            Some(a) => a,
            None => return DefWindowProcW(hwnd, msg, w, l),
        };
        let x = l as i16 as i32;
        let y = (l >> 16) as i16 as i32;
        let input = app.logical_point(Point { x, y });
        // Ink can also synthesize mouse messages. Consuming both would double every stroke.
        if (0x200..=0x209).contains(&msg)
            && GetMessageExtraInfo() as usize & 0xffffff00 == 0xff515700
        {
            return 0;
        }
        if (0x200..=0x209).contains(&msg) && app.pen_id.is_some() {
            return 0;
        }
        match msg {
            0x245 | 0x246 | 0x247 | 0x249 | 0x24a => {
                if app.pointer(hwnd, msg, (w & 0xffff) as u32) {
                    0
                } else {
                    DefWindowProcW(hwnd, msg, w, l)
                }
            }
            0x24c => {
                if app.pen_id == Some((w & 0xffff) as u32) {
                    app.cancel_input(hwnd);
                }
                0
            }
            0x0f => {
                app.paint(hwnd);
                0
            }
            0x14 => 1,
            0x05 => {
                let maximized = IsZoomed(hwnd) != 0;
                if maximized != app.maximized {
                    chrome::shape(hwnd, maximized);
                }
                app.maximized = maximized;
                if w == 1 {
                    return 0;
                }
                app.resize(
                    (l & 0xffff) as i32,
                    ((l >> 16) & 0xffff) as i32,
                    dpi::for_window(hwnd),
                );
                app.invalidate_scene(hwnd);
                0
            }
            0x02e0 => {
                app.cancel_input(hwnd);
                if l != 0 {
                    let r = *(l as *const Rect);
                    SetWindowPos(
                        hwnd,
                        null_mut(),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        0x4 | 0x10,
                    );
                }
                let mut r = Rect::default();
                GetClientRect(hwnd, &mut r);
                app.resize(r.right, r.bottom, (w & 0xffff) as u32);
                chrome::shape(hwnd, app.maximized);
                app.invalidate_scene(hwnd);
                0
            }
            0x201 | 0x203 => {
                app.down(hwnd, input.x, input.y, false);
                0
            }
            0x207 => {
                app.down(hwnd, input.x, input.y, true);
                0
            }
            0x200 => {
                if !app.mouse_tracking {
                    let mut track = MouseTrack {
                        size: mem::size_of::<MouseTrack>() as u32,
                        flags: 2,
                        hwnd,
                        hover_time: 0,
                    };
                    app.mouse_tracking = TrackMouseEvent(&mut track) != 0;
                }
                app.motion(hwnd, input.x, input.y);
                0
            }
            0x2a3 => {
                if app.hovered.take().is_some() {
                    app.invalidate_scene(hwnd);
                }
                app.mouse_tracking = false;
                if app.pen_id.is_none() {
                    let old = app.cursor_damage();
                    app.cursor_visible = false;
                    app.invalidate_cursor(hwnd, old);
                }
                0
            }
            0x202 | 0x208 => {
                // Mouse-up can carry a newer location than the last coalesced move.
                if app.drawing
                    || app.panning
                    || app.ui_drag.is_some()
                    || app.dock_drag.is_some()
                    || app.slider.is_some()
                    || app.control_drag.is_some()
                {
                    app.motion(hwnd, input.x, input.y);
                }
                app.last_mouse = (input.x, input.y);
                app.up(hwnd);
                0
            }
            0x215 | 0x08 => {
                // Mouse capture notifications must not interrupt implicit pen capture.
                if msg == 0x08 || app.pen_id.is_none() {
                    app.cancel_input(hwnd);
                }
                0
            }
            0x20 => {
                if (l & 0xffff) == 1 {
                    let mut p = Point::default();
                    if GetCursorPos(&mut p) != 0 {
                        ScreenToClient(hwnd, &mut p);
                        let p = app.logical_point(p);
                        if !app.drawing
                            && !app.panning
                            && app.ui_drag.is_none()
                            && app.slider.is_none()
                            && app.control_drag.is_none()
                        {
                            let old = app.cursor_damage();
                            app.last_mouse = (p.x, p.y);
                            app.cursor_visible = true;
                            app.invalidate_cursor(hwnd, old);
                        }
                        app.set_cursor_at(p.x, p.y);
                    }
                    1
                } else {
                    DefWindowProcW(hwnd, msg, w, l)
                }
            }
            0x20a => {
                if let Some(p) = app.palette.as_mut() {
                    p.step(if (w >> 16) as i16 > 0 { -1 } else { 1 });
                    app.invalidate_scene(hwnd);
                    return 0;
                }
                let mut p = Point { x, y };
                ScreenToClient(hwnd, &mut p);
                let p = app.logical_point(p);
                if app.workspace.docking.enabled {
                    if let Some(region) = app.dock_layout().top_at(p.x, p.y) {
                        use crate::ui::docking::Panel;
                        let delta = ((w >> 16) as i16 as i32 / 120) * 45;
                        match region.panel {
                            Panel::Color | Panel::Brush => {
                                let i = region.panel.index();
                                let max = app.panel_scroll_limit(region.panel, region.content());
                                app.panel_scroll[i] = (app.panel_scroll[i] - delta).max(0).min(max);
                            }
                            Panel::Layers => {
                                let rows = crate::ui::layer_rows(region.content().h + 215);
                                let max = app.doc.layers.len().saturating_sub(rows);
                                app.layer_scroll = if delta > 0 {
                                    app.layer_scroll.saturating_sub(1)
                                } else {
                                    (app.layer_scroll + 1).min(max)
                                };
                            }
                            _ => {}
                        }
                        app.invalidate_scene(hwnd);
                        return 0;
                    }
                }
                let layout = app.workspace.layout(app.size.0, app.size.1, app.focus);
                if !app.workspace.docking.enabled && layout.tools.contains(p.x, p.y) {
                    let geometry = crate::ui::ToolsGeometry::new(layout.tools.w);
                    let base = if app.workspace.tools_page == 1 {
                        geometry.tools
                    } else {
                        0
                    };
                    let max = (geometry.height - base - (layout.tools.h - PANEL_HEADER)).max(0);
                    app.tools_scroll = (app.tools_scroll - ((w >> 16) as i16 as i32 / 120) * 45)
                        .max(0)
                        .min(max);
                    app.invalidate_scene(hwnd);
                }
                if !app.workspace.docking.enabled
                    && layout.layers.contains(p.x, p.y)
                    && p.y >= layout.layers.y + PANEL_HEADER + 304
                {
                    let rows = crate::ui::layer_rows(layout.layers.h);
                    let max = app.doc.layers.len().saturating_sub(rows);
                    app.layer_scroll = if (w >> 16) as i16 > 0 {
                        app.layer_scroll.saturating_sub(1)
                    } else {
                        (app.layer_scroll + 1).min(max)
                    };
                    app.invalidate_scene(hwnd);
                }
                if inside(app.view(), p.x, p.y) && !app.drawing {
                    if let Some(gesture) = app.keymap.wheel(command_palette::modifiers()) {
                        app.canvas_wheel(
                            p.x,
                            p.y,
                            ((w >> 16) as i16) as f32,
                            gesture == crate::ui::keymap::Wheel::Zoom,
                            gesture == crate::ui::keymap::Wheel::Horizontal,
                        );
                    }
                    app.invalidate_scene(hwnd);
                }
                0
            }
            0x100 => {
                app.key(hwnd, w);
                app.sync_cursor(hwnd);
                0
            }
            0x101 => {
                app.sync_cursor(hwnd);
                0
            }
            0x104 | 0x105 => {
                app.sync_cursor(hwnd);
                DefWindowProcW(hwnd, msg, w, l)
            }
            0x8010 => {
                app.palette_key(hwnd, w);
                0
            }
            0x8011 => {
                if let Some(c) = crate::ui::keymap::COMMANDS.get(w) {
                    if let crate::ui::keymap::Kind::Press(a) = c.kind {
                        app.execute_action(hwnd, a);
                    }
                }
                0
            }
            0x8013 => {
                app.captured_binding(hwnd, w as u16, l as u8);
                0
            }
            0x8014 => {
                chrome::appearance(hwnd, app.workspace.theme(), app.workspace.theme == 2);
                0
            }
            0x8012 => {
                app.palette_query(hwnd);
                0
            }
            0x8001 => {
                app.command(hwnd, w as u32);
                0
            }
            0x10 => {
                if app.confirm(hwnd) {
                    app.close_palette(hwnd);
                    DestroyWindow(hwnd);
                    PostQuitMessage(0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    })
}
fn gdip_error(code: i32) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("Windows image codec error {}", code),
    )
}
unsafe fn save_png(path: &Path, pixels: &mut [u32], w: usize, h: usize) -> std::io::Result<()> {
    let mut bitmap = null_mut();
    let status = GdipCreateBitmapFromScan0(
        w as i32,
        h as i32,
        (w * 4) as i32,
        0x26200a,
        pixels.as_mut_ptr() as _,
        &mut bitmap,
    );
    if status != 0 {
        return Err(gdip_error(status));
    }
    let png = Guid {
        a: 0x557cf406,
        b: 0x1a04,
        c: 0x11d3,
        d: [0x9a, 0x73, 0, 0, 0xf8, 0x1e, 0xf3, 0x2e],
    };
    let tmp = path.with_extension("png.tmp");
    let status = GdipSaveImageToFile(bitmap, path_wide(&tmp).as_ptr(), &png, null_mut());
    GdipDisposeImage(bitmap);
    if status != 0 {
        return Err(gdip_error(status));
    }
    replace_file(&tmp, path)
}
unsafe fn load_image(path: &Path) -> std::io::Result<Document> {
    let mut image = null_mut();
    let status = GdipCreateBitmapFromFile(path_wide(path).as_ptr(), &mut image);
    if status != 0 {
        return Err(gdip_error(status));
    }
    let result = (|| {
        let mut w = 0;
        let mut h = 0;
        GdipGetImageWidth(image, &mut w);
        GdipGetImageHeight(image, &mut h);
        if w == 0 || h == 0 || w as usize > MAX_SIDE || h as usize > MAX_SIDE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Image Dimensions Must Be between 1 and 4096 Pixels",
            ));
        }
        let mut data: BitmapData = mem::zeroed();
        let r = GpRect {
            x: 0,
            y: 0,
            width: w as i32,
            height: h as i32,
        };
        let status = GdipBitmapLockBits(image, &r, 1, 0x26200a, &mut data);
        if status != 0 {
            return Err(gdip_error(status));
        }
        let mut doc = Document::new(w as usize, h as usize);
        for y in 0..h as usize {
            let row =
                (data.scan as *const u8).offset(y as isize * data.stride as isize) as *const u32;
            std::ptr::copy_nonoverlapping(
                row,
                doc.layers[0].pixels.as_mut_ptr().add(y * w as usize),
                w as usize,
            );
        }
        GdipBitmapUnlockBits(image, &mut data);
        doc.dirty = true;
        Ok(doc)
    })();
    GdipDisposeImage(image);
    result
}
pub fn run() {
    unsafe {
        dpi::enable();
        let mut token = 0;
        let startup = GdiStartup {
            version: 1,
            callback: null_mut(),
            suppress_thread: 0,
            suppress_codecs: 0,
        };
        let status = GdiplusStartup(&mut token, &startup, null_mut());
        if status != 0 {
            MessageBoxW(
                null_mut(),
                wide("Failed to Start the Windows Image Encoder").as_ptr(),
                wide("MiCoPi").as_ptr(),
                0x10,
            );
            return;
        }
        APP.with(|s| *s.borrow_mut() = Some(App::new()));
        let instance = GetModuleHandleW(null());
        let name = wide("MiCoPi.Native.Canvas");
        let class = WndClass {
            style: 3 | 8,
            proc: window_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512 as _),
            background: null_mut(),
            menu: null(),
            name: name.as_ptr(),
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("Untitled — MiCoPi").as_ptr(),
            0x02cf0000,
            0x80000000u32 as i32,
            0x80000000u32 as i32,
            1380,
            960,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        if hwnd.is_null() {
            MessageBoxW(
                null_mut(),
                wide("Failed to Create Window").as_ptr(),
                wide("MiCoPi").as_ptr(),
                0x10,
            );
        } else {
            APP.with(|s| {
                let mut state = s.borrow_mut();
                let app = state.as_mut().unwrap();
                app.fit();
                chrome::appearance(hwnd, app.workspace.theme(), app.workspace.theme == 2);
            });
            ShowWindow(hwnd, 5);
            let mut msg: Msg = mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        APP.with(|s| *s.borrow_mut() = None);
        GdiplusShutdown(token);
    }
}

#[cfg(test)]
mod tests;

unsafe fn file_dialog(hwnd: Hwnd, save: bool, png: bool) -> Option<PathBuf> {
    let mut file = [0u16; 32768];
    let name = if png { "Untitled.png" } else { "Untitled.mcp" };
    if save {
        let n = wide(name);
        file[..n.len()].copy_from_slice(&n);
    }
    let filter = wide(if save && png {
        "PNG image\0*.png\0\0"
    } else if save {
        "MiCoPi project\0*.mcp\0\0"
    } else {
        "MiCoPi / Images\0*.mcp;*.png;*.jpg;*.jpeg;*.bmp\0MiCoPi project\0*.mcp\0PNG image\0*.png\0All files\0*.*\0\0"
    });
    let ext = wide(if png { "png" } else { "mcp" });
    let mut of: OpenFile = mem::zeroed();
    of.size = mem::size_of::<OpenFile>() as u32;
    of.owner = hwnd;
    of.file = file.as_mut_ptr();
    of.max_file = file.len() as u32;
    of.filter = filter.as_ptr();
    of.filter_index = 1;
    of.default_ext = ext.as_ptr();
    of.flags = 0x80000 | 0x800 | 8 | if save { 2 } else { 0x1000 };
    let ok = if save {
        GetSaveFileNameW(&mut of)
    } else {
        GetOpenFileNameW(&mut of)
    };
    if ok == 0 {
        None
    } else {
        let len = file.iter().position(|c| *c == 0).unwrap_or(file.len());
        Some(PathBuf::from(std::ffi::OsString::from_wide(&file[..len])))
    }
}
