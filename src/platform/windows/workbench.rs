//! Compact native painting workspace. All controls here drive existing editor state.
use super::*;
use crate::ui::docking::Panel;
use crate::{
    engine::color::{hsv, square, to_hsv, wheel},
    engine::over,
    ui::ToolsGeometry,
};
const BRUSH_SIZES: [f32; 10] = [1., 2., 3., 5., 8., 12., 20., 35., 60., 100.];
const PRESETS: [(&str, f32, f32, f32, bool); 6] = [
    ("Pencil", 1.5, 0.65, 1., false),
    ("Ink", 4., 1., 1., false),
    ("Brush Tool", 10., 0.85, 0.65, false),
    ("Soft", 22., 0.12, 0., false),
    ("Marker", 14., 0.3, 1., false),
    ("Eraser", 16., 1., 0.8, true),
];

impl App {
    pub(super) fn border_color(&self) -> u32 {
        if self.workspace.theme == 2 {
            0x414c59
        } else {
            0xe1e6ec
        }
    }
    pub(super) unsafe fn frame(&self, dc: Dc, r: Rect, c: u32) {
        Self::fill(dc, rect(r.left, r.top, r.right - r.left, 1), c);
        Self::fill(dc, rect(r.left, r.bottom - 1, r.right - r.left, 1), c);
        Self::fill(dc, rect(r.left, r.top, 1, r.bottom - r.top), c);
        Self::fill(dc, rect(r.right - 1, r.top, 1, r.bottom - r.top), c);
    }
    pub(super) unsafe fn rounded(&self, dc: Dc, r: Rect, c: u32, radius: i32) {
        let save = SaveDC(dc);
        let b = CreateSolidBrush(color(c));
        SelectObject(dc, b);
        SelectObject(dc, GetStockObject(8));
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
        RestoreDC(dc, save);
        DeleteObject(b);
    }
    fn is_hovered(&self, r: Rect, id: u32) -> bool {
        let mut x = r.left + self.widget_offset.0;
        let mut y = r.top + self.widget_offset.1;
        if let Some(c) = self.widget_clip {
            x = x.max(c.left);
            y = y.max(c.top);
        }
        self.hovered == Some((id, x, y))
    }
    pub(super) unsafe fn centered(dc: Dc, r: Rect, s: &str, c: u32) {
        let mut r = r;
        SetTextColor(dc, color(c));
        DrawTextW(dc, wide(s).as_ptr(), -1, &mut r, 0x25 | 0x8000 | 0x800);
    }
    pub(super) unsafe fn button(&mut self, dc: Dc, r: Rect, label: &str, id: u32, selected: bool) {
        let t = self.workspace.theme();
        let hovered = self.is_hovered(r, id);
        let primary = id == 4;
        self.rounded(
            dc,
            r,
            if primary {
                t.accent
            } else if selected || hovered {
                t.selected
            } else {
                t.button
            },
            5,
        );
        let ink = if primary && self.workspace.theme != 2 {
            0xffffff
        } else if selected {
            t.accent
        } else {
            t.text
        };
        if let Some(icon) = icons::command(id) {
            let size = if r.bottom - r.top < 24 { 16 } else { 20 };
            self.icons.draw(
                dc,
                rect(r.left, r.top, r.right - r.left, r.bottom - r.top),
                icon,
                size,
                ink,
            );
        } else {
            Self::centered(
                dc,
                rect(r.left + 3, r.top, r.right - r.left - 6, r.bottom - r.top),
                label,
                ink,
            );
        }
        self.hit(r, id);
    }
    unsafe fn section(&self, dc: Dc, y: i32, w: i32, title: &str) {
        let t = self.workspace.theme();
        SelectObject(dc, self.title_font);
        Self::text(dc, rect(14, y, w - 28, 24), title, t.text);
        SelectObject(dc, self.font);
    }
    unsafe fn begin_panel(&mut self, dc: Dc, p: Box2, title: &str, id: u32, scroll: i32) -> i32 {
        let t = self.workspace.theme();
        Self::fill(dc, rect(p.x, p.y, p.w, p.h), t.panel);
        SelectObject(dc, self.title_font);
        Self::text(
            dc,
            rect(p.x + 14, p.y, p.w - 55, PANEL_HEADER),
            title,
            t.text,
        );
        SelectObject(dc, self.font);
        if id == 500 {
            self.rounded(
                dc,
                rect(p.x + p.w - 28, p.y + 14, 16, 14),
                self.background_ink,
                3,
            );
            self.rounded(dc, rect(p.x + p.w - 36, p.y + 7, 18, 14), self.ink, 3);
        } else {
            for dy in [13, 17] {
                Self::fill(dc, rect(p.x + p.w - 27, p.y + dy, 12, 1), t.muted);
            }
        }
        self.hit(rect(p.x, p.y, p.w, PANEL_HEADER), id);
        let saved = SaveDC(dc);
        IntersectClipRect(dc, p.x, p.y + PANEL_HEADER, p.x + p.w, p.y + p.h);
        SetViewportOrgEx(
            dc,
            dpi::physical(p.x + self.content_padding(), self.dpi),
            dpi::physical(
                p.y + PANEL_HEADER - scroll + self.content_padding(),
                self.dpi,
            ),
            null_mut(),
        );
        self.widget_offset = (p.x, p.y + PANEL_HEADER - scroll);
        self.widget_clip = Some(rect(p.x, p.y + PANEL_HEADER, p.w, p.h - PANEL_HEADER));
        saved
    }
    unsafe fn end_panel(&mut self, dc: Dc, s: i32) {
        RestoreDC(dc, s);
        self.widget_offset = (0, 0);
        self.widget_clip = None;
    }
    unsafe fn compact_slider(
        &mut self,
        dc: Dc,
        y: i32,
        label: &str,
        value: f32,
        number: &str,
        id: u32,
    ) {
        let t = self.workspace.theme();
        let w = self
            .widget_clip
            .map(|r| r.right - r.left)
            .unwrap_or(self.workspace.tools_width);
        let x = 76;
        let track = w - 126;
        Self::text(dc, rect(14, y, 60, 24), label, t.muted);
        self.rounded(dc, rect(x, y + 11, track, 3), self.border_color(), 1);
        self.rounded(
            dc,
            rect(x, y + 11, ((track - 2) as f32 * value) as i32 + 2, 3),
            t.accent,
            1,
        );
        self.rounded(
            dc,
            rect(x + ((track - 4) as f32 * value) as i32 - 2, y + 8, 8, 8),
            t.accent,
            4,
        );
        SelectObject(dc, self.small_font);
        Self::centered(dc, rect(w - 48, y + 2, 42, 22), number, t.text);
        SelectObject(dc, self.font);
        self.hit(rect(x - 2, y, track + 4, 24), id);
    }
    unsafe fn marker(&self, dc: Dc, x: i32, y: i32) {
        let saved = SaveDC(dc);
        SelectObject(dc, GetStockObject(5));
        for p in [self.cursor_dark, self.cursor_light] {
            SelectObject(dc, p);
            Ellipse(dc, x - 4, y - 4, x + 5, y + 5);
        }
        RestoreDC(dc, saved);
    }
    pub(super) unsafe fn draw_tools(&mut self, dc: Dc, p: Box2) {
        let g = ToolsGeometry::new(p.w);
        let base = if self.render_panel == Some(Panel::Brush)
            || (self.render_panel.is_none() && self.workspace.tools_page == 1)
        {
            g.tools
        } else {
            0
        };
        let end = if self.render_panel == Some(Panel::Color) {
            g.tools - 24
        } else {
            g.height
        };
        self.tools_scroll = self
            .tools_scroll
            .min((end - base - (p.h - PANEL_HEADER)).max(0));
        let title = if self.render_panel == Some(Panel::Color) {
            "Color"
        } else if base > 0 {
            "Brush"
        } else {
            "Color and Tools"
        };
        let id = self
            .render_panel
            .map(|p| 4000 + p.index() as u32)
            .unwrap_or(500);
        let saved = self.begin_panel(dc, p, title, id, self.tools_scroll + base);
        if self.render_panel == Some(Panel::Color) {
            IntersectClipRect(dc, 0, 0, p.w, end);
            if let Some(r) = self.widget_clip.as_mut() {
                r.bottom = r.bottom.min(p.y + PANEL_HEADER + end - self.tools_scroll);
            }
        }
        let t = self.workspace.theme();
        let (h, s, v) = to_hsv(self.ink);
        if self.color_last_ink != self.ink {
            if s > 0.001 {
                self.color_hue = h;
            }
            self.color_last_ink = self.ink;
        }
        let key = ((self.color_hue * 3600.).round() as i32, g.wheel, t.panel);
        if self.wheel_key != key {
            wheel(
                &mut self.wheel_pixels,
                g.wheel as usize,
                self.color_hue,
                t.panel,
            );
            self.wheel_key = key;
        }
        let wx = (p.w - g.wheel) / 2;
        let wy = 6;
        StretchDIBits(
            dc,
            wx,
            wy,
            g.wheel,
            g.wheel,
            0,
            0,
            g.wheel,
            g.wheel,
            self.wheel_pixels.as_ptr() as _,
            &BitmapInfo::new(g.wheel as usize, g.wheel as usize),
            0,
            0xcc0020,
        );
        let (o, side) = square(g.wheel);
        self.frame(
            dc,
            rect(wx + o - 1, wy + o - 1, side + 2, side + 2),
            self.border_color(),
        );
        self.marker(
            dc,
            wx + o + (s * (side - 1) as f32).round() as i32,
            wy + o + ((1. - v) * (side - 1) as f32).round() as i32,
        );
        let a = self.color_hue * std::f32::consts::TAU;
        let r = g.wheel as f32 / 2. - 9.;
        self.marker(
            dc,
            p.w / 2 + (a.cos() * r) as i32,
            wy + g.wheel / 2 + (a.sin() * r) as i32,
        );
        self.hit(rect(wx + o, wy + o, side, side), 801);
        self.hit(rect(wx, wy, g.wheel, g.wheel), 800);
        for (index, label) in ["R", "G", "B"].iter().enumerate() {
            let y = g.rgb + index as i32 * 20;
            let shift = 16 - index * 8;
            let value = (self.ink >> shift) & 255;
            Self::text(dc, rect(10, y, 18, 18), label, t.muted);
            let width = p.w - 80;
            let mut strip = [0u32; 128];
            for (i, c) in strip.iter_mut().enumerate() {
                *c =
                    0xff000000 | (self.ink & !(255 << shift)) | (((i as u32 * 255) / 127) << shift);
            }
            StretchDIBits(
                dc,
                34,
                y + 3,
                width,
                12,
                0,
                0,
                128,
                1,
                strip.as_ptr() as _,
                &BitmapInfo::new(128, 1),
                0,
                0xcc0020,
            );
            self.frame(dc, rect(33, y + 2, width + 2, 14), self.border_color());
            let tx = 34 + (width as u32 * value / 255) as i32;
            Self::fill(dc, rect(tx - 1, y + 1, 3, 16), t.text);
            Self::text(
                dc,
                rect(p.w - 39, y, 34, 18),
                &format!("{:03}", value),
                t.text,
            );
            self.hit(rect(31, y, p.w - 72, 19), 802 + index as u32);
        }
        let cell = (p.w - 18) / 10;
        for (i, c) in PALETTE.iter().enumerate() {
            let r = rect(
                9 + (i % 10) as i32 * cell,
                g.palette + (i / 10) as i32 * 19,
                cell - 2,
                16,
            );
            Self::fill(dc, r, *c);
            self.frame(dc, r, self.border_color());
            self.hit(r, 100 + i as u32);
        }
        self.section(dc, g.tools - 28, p.w, "Painting Tools");
        if self.render_panel.is_none() {
            self.hit(rect(0, g.tools - 28, p.w, 24), 502);
        }
        let third = (p.w - 24) / 5;
        for (i, label, id, on) in [
            (0, "Brush Tool", 10, self.tool == Tool::Brush),
            (1, "Eraser", 11, self.tool == Tool::Eraser),
            (2, "Eyedropper", 12, self.tool == Tool::Picker),
            (3, "Hand", 13, self.tool == Tool::Hand),
            (4, "Zoom", 14, self.tool == Tool::Zoom),
        ] {
            self.button(
                dc,
                rect(8 + i * (third + 2), g.tools, third, 30),
                label,
                id,
                on,
            );
        }
        let cw = (p.w - 20) / 3;
        for (i, (name, radius, opacity, hard, erase)) in PRESETS.iter().enumerate() {
            let x = 8 + (i % 3) as i32 * (cw + 2);
            let y = g.presets + (i / 3) as i32 * 56;
            let selected = (self.radius - radius).abs() < 0.05
                && (self.opacity - opacity).abs() < 0.01
                && (self.hardness - hard).abs() < 0.01
                && (self.tool == Tool::Eraser) == *erase;
            let selected = selected && matches!(self.tool, Tool::Brush | Tool::Eraser);
            let r = rect(x, y, cw, 50);
            self.rounded(
                dc,
                r,
                if selected || self.is_hovered(r, 820 + i as u32) {
                    t.selected
                } else {
                    t.button
                },
                6,
            );
            if selected {
                self.rounded(dc, rect(x + 7, y + 6, 3, 8), t.accent, 1);
            }
            let old = SaveDC(dc);
            let pen = CreatePen(
                0,
                if i == 0 {
                    1
                } else if i == 3 {
                    7
                } else {
                    3
                },
                color(if *erase { 0xa7adae } else { t.text }),
            );
            SelectObject(dc, pen);
            for step in 0..17 {
                let u = step as f32 / 16.;
                let px = x + 10 + ((cw - 20) as f32 * u) as i32;
                let py = y + 13 + ((u * 4.2 - 0.6).sin() * 4.) as i32;
                if step == 0 {
                    MoveToEx(dc, px, py, null_mut());
                } else {
                    LineTo(dc, px, py);
                }
            }
            RestoreDC(dc, old);
            DeleteObject(pen);
            Self::centered(dc, rect(x, y + 28, cw, 19), name, t.text);
            self.hit(r, 820 + i as u32);
        }
        self.section(dc, g.parameters - 26, p.w, "Brush Settings");
        self.compact_slider(
            dc,
            g.parameters,
            "Size",
            (self.radius * 2. - 1.) / 199.,
            &format!("{:.1}", self.radius * 2.),
            20,
        );
        self.compact_slider(
            dc,
            g.parameters + 28,
            "Opacity",
            self.opacity,
            &format!("{}%", (self.opacity * 100.) as i32),
            21,
        );
        self.compact_slider(
            dc,
            g.parameters + 56,
            "Hardness",
            self.hardness,
            &format!("{}%", (self.hardness * 100.) as i32),
            22,
        );
        self.compact_slider(
            dc,
            g.parameters + 84,
            "Stabilizer",
            self.workspace.stabilizer,
            &format!("{}%", (self.workspace.stabilizer * 100.) as i32),
            23,
        );
        let half = (p.w - 20) / 2;
        self.button(
            dc,
            rect(8, g.pressure, half, 28),
            "Pressure Size",
            51,
            self.workspace.dynamics.size,
        );
        self.button(
            dc,
            rect(12 + half, g.pressure, half, 28),
            "Pressure Opacity",
            52,
            self.workspace.dynamics.opacity,
        );
        self.button(
            dc,
            rect(8, g.curve, p.w - 16, 28),
            &format!(
                "Pressure Curve   {:.2} · Cycle",
                self.workspace.dynamics.gamma
            ),
            53,
            false,
        );
        Self::text(
            dc,
            rect(9, g.sizes, p.w - 18, 18),
            "Brush Diameter / px",
            t.muted,
        );
        let cell = (p.w - 20) / 5;
        for (i, n) in BRUSH_SIZES.iter().enumerate() {
            self.button(
                dc,
                rect(
                    8 + (i % 5) as i32 * (cell + 1),
                    g.sizes + 24 + (i / 5) as i32 * 25,
                    cell,
                    23,
                ),
                &format!("{}", *n as i32),
                840 + i as u32,
                (self.radius * 2. - n).abs() < 0.1,
            );
        }
        self.end_panel(dc, saved);
        let view = p.h - PANEL_HEADER;
        if end - base > view {
            let track = view - 16;
            let thumb = (track * view / (end - base)).max(24);
            let offset = (track - thumb) * self.tools_scroll / (end - base - view);
            self.rounded(
                dc,
                rect(p.x + p.w - 4, p.y + PANEL_HEADER + 8 + offset, 3, thumb),
                self.border_color(),
                1,
            );
        }
    }
    pub(super) fn navigator_rect(&self) -> Rect {
        let p = self.panel_box(Panel::Navigator);
        let preview_height = if self.workspace.docking.enabled {
            (p.h - PANEL_HEADER - 20).max(32).min(112)
        } else {
            112
        };
        let scale = ((p.w - 20) as f32 / self.doc.width as f32)
            .min(preview_height as f32 / self.doc.height as f32);
        let w = (self.doc.width as f32 * scale).round().max(1.) as i32;
        let h = (self.doc.height as f32 * scale).round().max(1.) as i32;
        rect(
            p.x + (p.w - w) / 2,
            p.y + PANEL_HEADER + 10 + (preview_height - h) / 2,
            w,
            h,
        )
    }
    pub(super) unsafe fn draw_layers(&mut self, dc: Dc, p: Box2) {
        let layers_only = self.render_panel == Some(Panel::Layers);
        let nav_only = self.render_panel == Some(Panel::Navigator);
        let base = if layers_only { 215 } else { 0 };
        let id = self
            .render_panel
            .map(|p| 4000 + p.index() as u32)
            .unwrap_or(501);
        let saved = self.begin_panel(
            dc,
            p,
            if layers_only { "Layers" } else { "Navigator" },
            id,
            base,
        );

        let t = self.workspace.theme();
        if !layers_only {
            let n = self.navigator_rect();
            let x = n.left - p.x;
            let y = n.top - p.y - PANEL_HEADER;
            let w = n.right - n.left;
            let h = n.bottom - n.top;
            self.rounded(
                dc,
                rect(
                    8,
                    6,
                    p.w - 16,
                    if nav_only {
                        (p.h - PANEL_HEADER - 12).max(1).min(122)
                    } else {
                        122
                    },
                ),
                t.background,
                6,
            );
            self.nav_pixels.resize((w * h) as usize, 0);
            for py in 0..h {
                for px in 0..w {
                    let src = (py as usize * self.doc.height / h as usize) * self.doc.width
                        + px as usize * self.doc.width / w as usize;
                    let bg = if (px / 6 + py / 6) % 2 == 0 {
                        0xfffcfcfc
                    } else {
                        0xfff2f4f6
                    };
                    self.nav_pixels[(py * w + px) as usize] = over(bg, self.doc.composite[src]);
                }
            }
            StretchDIBits(
                dc,
                x,
                y,
                w,
                h,
                0,
                0,
                w,
                h,
                self.nav_pixels.as_ptr() as _,
                &BitmapInfo::new(w as usize, h as usize),
                0,
                0xcc0020,
            );
            let v = self.view();
            let o = self.origin();
            let clamp = |a: f32| a.max(0.).min(1.);
            let a = clamp((v.left as f32 - o.0) / self.zoom / self.doc.width as f32);
            let b = clamp((v.right as f32 - o.0) / self.zoom / self.doc.width as f32);
            let c = clamp((v.top as f32 - o.1) / self.zoom / self.doc.height as f32);
            let d = clamp((v.bottom as f32 - o.1) / self.zoom / self.doc.height as f32);
            self.frame(
                dc,
                rect(
                    x + (a * w as f32) as i32,
                    y + (c * h as f32) as i32,
                    ((b - a) * w as f32).max(1.) as i32,
                    ((d - c) * h as f32).max(1.) as i32,
                ),
                t.accent,
            );
            self.hit(rect(x, y, w, h), 810);
            if !nav_only || p.h >= PANEL_HEADER + 150 {
                self.button(dc, rect(8, 128, 28, 22), "−", 86, false);
                self.button(
                    dc,
                    rect(40, 128, p.w - 116, 22),
                    &format!("{:.1}%", self.zoom_percent()),
                    40,
                    false,
                );
                self.button(dc, rect(p.w - 72, 128, 28, 22), "+", 85, false);
                self.button(dc, rect(p.w - 40, 128, 32, 22), "1:1", 41, false);
            }
            if !nav_only || p.h >= PANEL_HEADER + 177 {
                Self::text(
                    dc,
                    rect(9, 157, p.w - 18, 20),
                    &format!("Canvas  {} × {} px", self.doc.width, self.doc.height),
                    t.muted,
                );
            }
        }
        if nav_only {
            self.end_panel(dc, saved);
            return;
        }
        if !layers_only {
            self.section(dc, 185, p.w, "Layers");
            self.hit(rect(0, 185, p.w, 24), 503);
        }

        self.button(dc, rect(8, 215, 54, 24), "+ New", 42, false);
        self.button(dc, rect(67, 215, 54, 24), "Clear", 43, false);
        Self::text(
            dc,
            rect(p.w - 78, 215, 70, 24),
            &format!("{} / 16", self.doc.layers.len()),
            t.muted,
        );
        Self::text(dc, rect(9, 247, 60, 19), "Blend Mode", t.muted);
        Self::text(dc, rect(80, 247, p.w - 88, 19), "Normal", t.text);
        let alpha = self.doc.layers[self.doc.active].opacity;
        Self::text(dc, rect(9, 272, 54, 20), "Opacity", t.muted);
        let track = p.w - 112;
        self.rounded(dc, rect(65, 281, track, 3), self.border_color(), 1);
        self.rounded(
            dc,
            rect(65, 281, track * alpha as i32 / 255 + 1, 3),
            t.accent,
            1,
        );
        self.rounded(
            dc,
            rect(62 + track * alpha as i32 / 255, 278, 8, 8),
            t.accent,
            4,
        );
        Self::text(
            dc,
            rect(p.w - 43, 272, 38, 20),
            &format!("{}%", alpha as u32 * 100 / 255),
            t.text,
        );
        self.hit(rect(63, 270, track + 4, 23), 805);
        let rows = crate::ui::layer_rows(p.h + base);
        self.layer_scroll = self
            .layer_scroll
            .min(self.doc.layers.len().saturating_sub(rows));
        for (row, i) in (0..self.doc.layers.len())
            .rev()
            .skip(self.layer_scroll)
            .take(rows)
            .enumerate()
        {
            let y = 304 + row as i32 * 56;
            let r = rect(8, y, p.w - 16, 52);
            let l = &self.doc.layers[i];
            let selected = i == self.doc.active;
            self.rounded(
                dc,
                r,
                if selected || self.is_hovered(rect(27, y, p.w - 34, 52), 200 + i as u32) {
                    t.selected
                } else {
                    t.button
                },
                6,
            );
            if selected {
                self.rounded(dc, rect(8, y + 10, 3, 32), t.accent, 1);
            }
            let saved = SaveDC(dc);
            SelectObject(dc, self.cursor_light);
            SelectObject(dc, GetStockObject(5));
            let eye = CreatePen(0, 1, color(t.muted));
            SelectObject(dc, eye);
            Ellipse(dc, 11, y + 18, 23, y + 27);
            if l.visible {
                SelectObject(dc, GetStockObject(4));
                Ellipse(dc, 15, y + 20, 20, y + 25);
            } else {
                MoveToEx(dc, 11, y + 29, null_mut());
                LineTo(dc, 24, y + 16);
            }
            RestoreDC(dc, saved);
            DeleteObject(eye);
            let mut thumb = [0u32; 42 * 33];
            for py in 0..33 {
                for px in 0..42 {
                    let sample = l.pixels
                        [(py * self.doc.height / 33) * self.doc.width + px * self.doc.width / 42];
                    let bg = if (px / 6 + py / 6) % 2 == 0 {
                        0xfffcfcfc
                    } else {
                        0xfff0f3f5
                    };
                    thumb[py * 42 + px] = over(bg, sample);
                }
            }
            StretchDIBits(
                dc,
                29,
                y + 7,
                42,
                33,
                0,
                0,
                42,
                33,
                thumb.as_ptr() as _,
                &BitmapInfo::new(42, 33),
                0,
                0xcc0020,
            );
            self.frame(dc, rect(28, y + 6, 44, 35), self.border_color());
            Self::text(
                dc,
                rect(80, y + 5, p.w - 95, 20),
                &format!("Layer {:02}", i + 1),
                t.text,
            );
            SelectObject(dc, self.small_font);
            Self::text(
                dc,
                rect(80, y + 25, p.w - 95, 18),
                &format!("Normal · {}%", l.opacity as u32 * 100 / 255),
                t.muted,
            );
            SelectObject(dc, self.font);
            self.hit(rect(8, y, 18, 52), 300 + i as u32);
            self.hit(rect(27, y, p.w - 34, 52), 200 + i as u32);
        }
        if !layers_only || p.h >= 280 {
            Self::text(
                dc,
                rect(9, p.h - PANEL_HEADER + base - 61, p.w - 18, 18),
                "New Canvas Size",
                t.muted,
            );
            self.button(
                dc,
                rect(8, p.h - PANEL_HEADER + base - 36, p.w - 16, 23),
                &format!("{} × {}   ▾", self.new_size, self.new_size),
                46,
                false,
            );
        }
        self.end_panel(dc, saved);
    }
    pub(super) unsafe fn draw_frame(&mut self, dc: Dc) {
        self.doc.display_pixels(&mut self.pixels);
        let t = self.workspace.theme();
        SelectObject(dc, self.font);
        SetBkMode(dc, 1);
        self.hits.clear();
        self.widget_offset = (0, 0);
        self.widget_clip = None;
        Self::fill(dc, rect(0, 0, self.size.0, self.size.1), t.background);
        let l = self.workspace.layout(self.size.0, self.size.1, self.focus);
        self.draw_shell(dc, &l);
        if !self.workspace.docking.enabled && l.tools.w > 0 {
            self.draw_tools(dc, l.tools);
        }
        if !self.workspace.docking.enabled && l.layers.w > 0 {
            self.draw_layers(dc, l.layers);
        }
        let v = self.view();
        let o = self.origin();
        let dw = (self.doc.width as f32 * self.zoom).round().max(1.) as i32;
        let dh = (self.doc.height as f32 * self.zoom).round().max(1.) as i32;
        let vw = v.right - v.left;
        let vh = v.bottom - v.top;
        let pw = dpi::extent(vw, self.dpi);
        let ph = dpi::extent(vh, self.dpi);
        if self
            .viewport
            .as_ref()
            .map_or(true, |b| b.w != pw || b.h != ph)
        {
            self.viewport = Some(Backbuffer::new(dc, pw, ph));
        }
        let sc = self.viewport.as_ref().unwrap();
        dpi::map(sc.dc, self.dpi);
        Self::fill(sc.dc, rect(0, 0, vw, vh), t.background);
        Self::fill(
            sc.dc,
            rect(o.0 as i32 - v.left + 3, o.1 as i32 - v.top + 3, dw, dh),
            if self.workspace.theme == 2 {
                0x292d30
            } else {
                0x909596
            },
        );
        SetStretchBltMode(sc.dc, 3);
        StretchDIBits(
            sc.dc,
            o.0.round() as i32 - v.left,
            o.1.round() as i32 - v.top,
            dw,
            dh,
            0,
            0,
            self.doc.width as i32,
            self.doc.height as i32,
            self.pixels.as_ptr() as _,
            &BitmapInfo::new(self.doc.width, self.doc.height),
            0,
            0xcc0020,
        );
        BitBlt(dc, v.left, v.top, vw, vh, sc.dc, 0, 0, 0xcc0020);
        for p in [l.tools, l.layers] {
            if p.w > 0 {
                Self::fill(
                    dc,
                    rect(
                        if p.x == l.activity.w {
                            p.x + p.w - 1
                        } else {
                            p.x
                        },
                        TOP,
                        1,
                        p.h,
                    ),
                    self.border_color(),
                );
            }
        }
        let tab_y = l.tabs.y;
        Self::fill(dc, rect(v.left, tab_y, vw, DOCUMENT_TABS), t.panel);
        self.frame(
            dc,
            rect(v.left, tab_y, vw, DOCUMENT_TABS),
            self.border_color(),
        );
        Self::fill(
            dc,
            rect(v.left, tab_y, 218.min(vw), DOCUMENT_TABS),
            t.background,
        );
        Self::fill(dc, rect(v.left, tab_y, 218.min(vw), 2), t.accent);
        self.rounded(dc, rect(v.left + 16, tab_y + 12, 4, 10), t.accent, 2);
        let name = self
            .file
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled.mcp".into());
        Self::text(
            dc,
            rect(v.left + 30, tab_y + 4, 178.min((vw - 36).max(1)), 26),
            &format!("{}{}", if self.doc.dirty { "●  " } else { "" }, name),
            t.text,
        );
        SelectObject(dc, self.small_font);
        Self::fill(
            dc,
            rect(0, self.size.1 - BOTTOM, self.size.0, BOTTOM),
            t.panel,
        );
        Self::fill(
            dc,
            rect(0, self.size.1 - BOTTOM, self.size.0, 1),
            self.border_color(),
        );
        let hover_hint = self.hovered.and_then(|h| {
            self.keymap.ui_hint(h.0).or_else(|| {
                if (500..=503).contains(&h.0) || (4000..=4403).contains(&h.0) {
                    Some(
                        "Drag tabs to move panels, headers to move groups; group on tabs, dock at edges; Esc to cancel"
                            .into(),
                    )
                } else {
                    None
                }
            })
        });
        Self::text(
            dc,
            rect(9, self.size.1 - BOTTOM, self.size.0 - 530, BOTTOM),
            hover_hint.as_deref().unwrap_or(&self.status),
            t.muted,
        );
        Self::text(
            dc,
            rect(self.size.0 - 515, self.size.1 - BOTTOM, 510, BOTTOM),
            &format!(
                "{} × {}    ·    {} {:.0}%    ·    {:.1}%    ·    History {:.1} MB",
                self.doc.width,
                self.doc.height,
                if self.pen_id.is_some() {
                    "Ink"
                } else {
                    "Mouse"
                },
                self.pressure * 100.,
                self.zoom_percent(),
                self.doc.history_bytes() as f64 / 1048576.
            ),
            t.muted,
        );
        self.draw_dock_panels(dc);
        self.draw_dock_preview(dc);
        self.draw_icon_hint(dc);
        self.draw_palette(dc);
    }
    unsafe fn draw_icon_hint(&self, dc: Dc) {
        if self.palette.is_some()
            || self.drawing
            || self.panning
            || self.ui_drag.is_some()
            || self.dock_drag.is_some()
        {
            return;
        }
        let (id, left, top) = match self.hovered {
            Some(h) => h,
            None => return,
        };
        if icons::command(id).is_none() {
            return;
        }
        let hit = match self
            .hits
            .iter()
            .find(|h| h.id == id && h.r.left == left && h.r.top == top)
        {
            Some(h) => h.r,
            None => return,
        };
        let index = match crate::ui::keymap::COMMANDS.iter().position(|c| c.ui == id) {
            Some(i) => i,
            None => return,
        };
        let t = self.workspace.theme();
        let label = self.keymap.label(index);
        let width = (self.icons.caps_width(dc, &label, self.small_font, 96) + 24)
            .max(180)
            .min(self.size.0 - 16);
        let height = if label.is_empty() { 32 } else { 58 };
        let x = hit.left.min(self.size.0 - width - 8).max(8);
        let y = if hit.bottom + height + 8 < self.size.1 - BOTTOM {
            hit.bottom + 6
        } else {
            hit.top - height - 6
        }
        .max(TOP);
        let r = rect(x, y, width, height);
        self.rounded(dc, r, t.panel, 5);
        self.frame(dc, r, t.selected);
        SelectObject(dc, self.small_font);
        Self::text(
            dc,
            rect(x + 12, y + 3, width - 24, 26),
            crate::ui::keymap::COMMANDS[index].title,
            t.text,
        );
        self.icons.keycaps(
            dc,
            rect(x + 12, y + 30, width - 24, 22),
            &label,
            self.small_font,
            t,
            96,
        );
    }
    pub(super) fn apply_preset(&mut self, index: usize) {
        if let Some((_, r, o, h, e)) = PRESETS.get(index) {
            self.radius = *r;
            self.opacity = *o;
            self.hardness = *h;
            self.select_tool(if *e { Tool::Eraser } else { Tool::Brush });
        }
    }
    pub(super) fn size_preset(&mut self, index: usize) {
        if let Some(n) = BRUSH_SIZES.get(index) {
            self.radius = *n / 2.;
        }
    }
    pub(super) fn adjust_control(&mut self, id: u32, x: i32, y: i32) {
        let layer = self.panel_box(Panel::Layers);
        let p = self.panel_box(Panel::Color);
        let g = ToolsGeometry::new(p.w);
        let px = x - p.x;
        let py = y - p.y - PANEL_HEADER
            + if self.workspace.docking.enabled {
                self.panel_scroll[Panel::Color.index()]
            } else {
                self.tools_scroll
                    + if self.workspace.tools_page == 1 {
                        g.tools
                    } else {
                        0
                    }
            };
        match id {
            800 => {
                let dx = px as f32 - p.w as f32 / 2.;
                let dy = py as f32 - 6. - g.wheel as f32 / 2.;
                let (_, s, v) = to_hsv(self.ink);
                self.color_hue = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.);
                self.ink = hsv(self.color_hue, s, v);
                self.color_last_ink = self.ink;
            }
            801 => {
                let (o, side) = square(g.wheel);
                let s = ((px - (p.w - g.wheel) / 2 - o) as f32 / (side - 1) as f32)
                    .max(0.)
                    .min(1.);
                let v = 1. - ((py - 6 - o) as f32 / (side - 1) as f32).max(0.).min(1.);
                self.ink = hsv(self.color_hue, s, v);
                self.color_last_ink = self.ink;
            }
            802..=804 => {
                let shift = 16 - (id - 802) * 8;
                let value =
                    (((px - 34) as f32 / (p.w - 80) as f32).max(0.).min(1.) * 255.).round() as u32;
                self.ink = (self.ink & !(255 << shift)) | (value << shift);
            }
            805 => {
                let alpha = (((x - layer.x - 65) as f32 / (layer.w - 112) as f32)
                    .max(0.)
                    .min(1.)
                    * 255.) as u8;
                self.doc.layers[self.doc.active].opacity = alpha;
                self.doc.dirty = true;
                self.doc.invalidate();
            }
            810 => {
                self.fit_to_window = false;
                let n = self.navigator_rect();
                let cx = ((x - n.left) as f32 / (n.right - n.left) as f32)
                    .max(0.)
                    .min(1.)
                    * self.doc.width as f32;
                let cy = ((y - n.top) as f32 / (n.bottom - n.top) as f32)
                    .max(0.)
                    .min(1.)
                    * self.doc.height as f32;
                self.pan = (
                    (self.doc.width as f32 / 2. - cx) * self.zoom,
                    (self.doc.height as f32 / 2. - cy) * self.zoom,
                );
            }
            _ => {}
        }
    }
    pub(super) unsafe fn workbench_menu(&mut self, hwnd: Hwnd, which: u32) {
        let mut popup = menu::Popup::new(self.workspace.theme(), self.dpi);
        let menu = popup.root;
        let items: &[(u32, &str)] = match which {
            90 => &[
                (1, "New Canvas"),
                (2, "Open…"),
                (3, "Save Project"),
                (7, "Save Project As…"),
                (4, "Export PNG…"),
            ],
            91 => &[
                (5, "Undo"),
                (6, "Redo"),
                (42, "New Layer"),
                (50, "Settings…"),
                (93, "Configure Shortcuts…"),
                (30, "System Color Picker…"),
                (43, "Clear Current Layer…"),
            ],
            _ => &[
                (40, "Fit to Window"),
                (41, "Actual Pixels"),
                (85, "Zoom In"),
                (86, "Zoom Out"),
                (69, "Toggle Panels"),
                (61, "Toggle Layers Panel"),
                (50, "Workspace and Brush Settings…"),
            ],
        };
        for (id, label) in items {
            popup.item(menu, *id, label, false, &self.keymap);
        }
        let mut point = Point {
            x: if which == 90 {
                56
            } else if which == 91 {
                108
            } else {
                160
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
}
