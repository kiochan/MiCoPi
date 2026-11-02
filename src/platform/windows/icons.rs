//! A private, 8 KB Material Symbols subset. No system font installation or web runtime.
use super::*;
use crate::ui::{
    keymap::{Chord, ALT, CTRL, SHIFT, WHEEL},
    Theme,
};
#[derive(Clone, Copy)]
pub(super) struct Icon {
    #[allow(dead_code)]
    pub name: &'static str,
    pub code: u16,
    pub fallback: &'static str,
}
macro_rules! icon {
    ($id:ident,$name:literal,$code:expr,$fallback:literal) => {
        #[allow(dead_code)]
        pub(super) const $id: Icon = Icon {
            name: $name,
            code: $code,
            fallback: $fallback,
        };
    };
}
icon!(PALETTE, "palette", 0xe40a, "Col");
icon!(BRUSH, "brush", 0xe3ae, "Br");
icon!(ERASER, "ink_eraser", 0xe6d0, "Er");
icon!(PICKER, "colorize", 0xe3b8, "Pick");
icon!(HAND, "back_hand", 0xe764, "Hand");
icon!(ZOOM_IN, "zoom_in", 0xe8ff, "+");
icon!(ZOOM_OUT, "zoom_out", 0xe900, "−");
icon!(LAYERS, "layers", 0xe53b, "Lay");
icon!(SEARCH, "search", 0xef7a, "Find");
icon!(KEYBOARD, "keyboard", 0xe312, "Key");
icon!(ENTER, "keyboard_return", 0xe31b, "↵");
icon!(TAB, "keyboard_tab", 0xe31c, "⇥");
icon!(SHIFT_KEY, "shift", 0xe5f2, "↑");
icon!(SPACE, "space_bar", 0xe256, "Space");
icon!(BACKSPACE, "backspace", 0xe14a, "⌫");
icon!(SETTINGS, "settings", 0xe8b8, "Set");
icon!(TUNE, "tune", 0xe429, "Adj");
icon!(EDIT, "edit", 0xf097, "Edit");
icon!(UNDO, "undo", 0xe166, "Undo");
icon!(REDO, "redo", 0xe15a, "Redo");
icon!(ADD, "add", 0xe145, "+");
icon!(REMOVE, "remove", 0xe15b, "−");
icon!(FIT, "fit_screen", 0xea10, "Fit");
icon!(CROP, "crop_free", 0xe3c2, "Box");
icon!(OPEN, "file_open", 0xeaf3, "Open");
icon!(SAVE, "save", 0xe161, "Save");
icon!(SAVE_AS, "save_as", 0xeb60, "Save");
icon!(IMAGE, "image", 0xe3f4, "Img");
icon!(UP, "arrow_upward", 0xe5d8, "↑");
icon!(DOWN, "arrow_downward", 0xe5db, "↓");
icon!(LEFT, "arrow_back", 0xe5c4, "←");
icon!(RIGHT, "arrow_forward", 0xe5c8, "→");
icon!(DOWNLOAD, "download", 0xf090, "Exp");
icon!(DOCK_LEFT, "dock_to_left", 0xf7e5, "L");
icon!(DOCK_RIGHT, "dock_to_right", 0xf7e4, "R");
icon!(CLOSE, "close", 0xe5cd, "×");
icon!(CHECK, "check", 0xe668, "✓");
icon!(MOUSE, "mouse", 0xe323, "Wheel");
icon!(NEW, "note_add", 0xe89c, "New");
icon!(DRAG, "drag_indicator", 0xe945, "Move");
#[cfg(test)]
pub(super) const ALL: [Icon; 40] = [
    PALETTE, BRUSH, ERASER, PICKER, HAND, ZOOM_IN, ZOOM_OUT, LAYERS, SEARCH, KEYBOARD, ENTER, TAB,
    SHIFT_KEY, SPACE, BACKSPACE, SETTINGS, TUNE, EDIT, UNDO, REDO, ADD, REMOVE, FIT, CROP, OPEN,
    SAVE, SAVE_AS, IMAGE, UP, DOWN, LEFT, RIGHT, DOWNLOAD, DOCK_LEFT, DOCK_RIGHT, CLOSE, CHECK,
    MOUSE, NEW, DRAG,
];
pub(super) fn command(id: u32) -> Option<Icon> {
    Some(match id {
        1 => NEW,
        2 => OPEN,
        3 => SAVE,
        7 => SAVE_AS,
        4 => DOWNLOAD,
        5 => UNDO,
        6 => REDO,
        10 => BRUSH,
        11 => ERASER,
        12 => PICKER,
        13 => HAND,
        14 | 85 => ZOOM_IN,
        86 => ZOOM_OUT,
        40 => FIT,
        42 => ADD,
        43 => BACKSPACE,
        50 => SETTINGS,
        60 => DOCK_LEFT,
        61 => LAYERS,
        93 => KEYBOARD,
        95 => SEARCH,
        10110 => PALETTE,
        10111 => BRUSH,
        _ => return None,
    })
}
pub(super) struct Icons {
    resource: Handle,
    fonts: [Handle; 3],
}
impl Icons {
    pub unsafe fn new(dpi: u32) -> Self {
        static FONT: &[u8] = include_bytes!("../../../assets/material-symbols/MiCoPiSymbols.ttf");
        let mut count = 0;
        let resource = AddFontMemResourceEx(
            FONT.as_ptr() as _,
            FONT.len() as u32,
            null_mut(),
            &mut count,
        );
        let face = wide(if resource.is_null() {
            "Microsoft YaHei UI"
        } else {
            "MiCoPi Symbols"
        });
        let make = |size| {
            CreateFontW(
                -dpi::physical(size, dpi),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                4,
                0,
                face.as_ptr(),
            )
        };
        Self {
            resource,
            fonts: [make(16), make(20), make(24)],
        }
    }
    pub unsafe fn draw(&self, dc: Dc, r: Rect, icon: Icon, size: i32, ink: u32) {
        let saved = SaveDC(dc);
        SelectObject(
            dc,
            self.fonts[if size <= 16 {
                0
            } else if size <= 20 {
                1
            } else {
                2
            }],
        );
        SetBkMode(dc, 1);
        SetTextColor(dc, color(ink));
        let mut bounds = r;
        if self.resource.is_null() {
            DrawTextW(
                dc,
                wide(icon.fallback).as_ptr(),
                -1,
                &mut bounds,
                0x25 | 0x800,
            );
        } else {
            DrawTextW(dc, [icon.code, 0].as_ptr(), 1, &mut bounds, 0x25 | 0x800);
        }
        RestoreDC(dc, saved);
    }
    pub unsafe fn caps_width(&self, dc: Dc, label: &str, font: Handle, dpi: u32) -> i32 {
        if label.contains(',') {
            return label
                .split(',')
                .map(|part| self.caps_width(dc, part.trim(), font, dpi))
                .sum::<i32>()
                + dpi::physical(16, dpi) * (label.split(',').count() - 1) as i32;
        }
        let saved = SaveDC(dc);
        SelectObject(dc, font);
        let (tokens, extra) = tokens(label);
        let s = |n| dpi::physical(n, dpi);
        let width = tokens.iter().map(|c| cap_width(dc, c, dpi)).sum::<i32>()
            + s(10) * tokens.len().saturating_sub(1) as i32
            + if extra > 0 { s(32) } else { 0 };
        RestoreDC(dc, saved);
        width
    }
    pub unsafe fn keycaps(
        &self,
        dc: Dc,
        r: Rect,
        label: &str,
        font: Handle,
        theme: Theme,
        dpi: u32,
    ) {
        if label.trim().is_empty() {
            return;
        }
        if label.contains(',') {
            let mut x = r.left;
            for (i, alias) in label.split(',').enumerate() {
                if i > 0 {
                    let mut slash = rect(x, r.top, dpi::physical(16, dpi), r.bottom - r.top);
                    let saved = SaveDC(dc);
                    SelectObject(dc, font);
                    SetTextColor(dc, color(theme.muted));
                    SetBkMode(dc, 1);
                    if slash.right <= r.right {
                        DrawTextW(dc, wide("/").as_ptr(), -1, &mut slash, 0x25 | 0x800);
                    }
                    RestoreDC(dc, saved);
                    x += dpi::physical(16, dpi);
                }
                if x >= r.right {
                    break;
                }
                let width = self.caps_width(dc, alias.trim(), font, dpi);
                self.keycaps(
                    dc,
                    rect(x, r.top, width.min(r.right - x), r.bottom - r.top),
                    alias.trim(),
                    font,
                    theme,
                    dpi,
                );
                x += width;
            }
            return;
        }
        let saved = SaveDC(dc);
        SelectObject(dc, font);
        SetBkMode(dc, 1);
        let (tokens, extra) = tokens(label);
        let s = |n| dpi::physical(n, dpi);
        let height = s(20);
        let y = r.top + ((r.bottom - r.top - height) / 2).max(0);
        let mut x = r.left;
        IntersectClipRect(dc, r.left, r.top, r.right, r.bottom);
        for (i, token) in tokens.iter().enumerate() {
            if i > 0 {
                let mut plus = rect(x, y, s(10), height);
                SetTextColor(dc, color(theme.muted));
                DrawTextW(dc, wide("+").as_ptr(), -1, &mut plus, 0x25 | 0x800);
                x += s(10);
            }
            let width = cap_width(dc, token, dpi);
            if x + width > r.right {
                App::centered(
                    dc,
                    rect(x, y, (r.right - x).max(1), height),
                    "…",
                    theme.muted,
                );
                break;
            }
            cap_frame(dc, rect(x, y, width, height), theme, dpi);
            match token {
                Cap::Text(text) => App::centered(
                    dc,
                    rect(x + s(4), y, width - s(8), height),
                    text,
                    theme.text,
                ),
                Cap::Symbol(icon) => {
                    self.draw(dc, rect(x, y, width, height), *icon, 16, theme.text)
                }
            }
            x += width;
        }
        if extra > 0 && x + s(30) <= r.right {
            App::centered(
                dc,
                rect(x + s(5), y, s(26), height),
                &format!("+{}", extra),
                theme.muted,
            );
        }
        RestoreDC(dc, saved);
    }
}
impl Drop for Icons {
    fn drop(&mut self) {
        unsafe {
            for font in self.fonts {
                DeleteObject(font);
            }
            if !self.resource.is_null() {
                RemoveFontMemResourceEx(self.resource);
            }
        }
    }
}
enum Cap {
    Text(String),
    Symbol(Icon),
}
fn tokens(label: &str) -> (Vec<Cap>, usize) {
    let mut aliases = label.split(',');
    let first = aliases.next().unwrap_or("").trim();
    let extra = aliases.count();
    let mut out = vec![];
    if let Ok(chord) = Chord::parse(first) {
        if chord.mods & CTRL != 0 {
            out.push(Cap::Text("Ctrl".into()));
        }
        if chord.mods & ALT != 0 {
            out.push(Cap::Text("Alt".into()));
        }
        if chord.mods & SHIFT != 0 {
            out.push(Cap::Symbol(SHIFT_KEY));
        }
        let symbol = match chord.key {
            13 => Some(ENTER),
            9 => Some(TAB),
            32 => Some(SPACE),
            8 => Some(BACKSPACE),
            37 => Some(LEFT),
            38 => Some(UP),
            39 => Some(RIGHT),
            40 => Some(DOWN),
            WHEEL => Some(MOUSE),
            _ => None,
        };
        if let Some(icon) = symbol {
            out.push(Cap::Symbol(icon));
        } else if chord.key != 0 {
            out.push(Cap::Text(match chord.key {
                27 => "Esc".into(),
                46 => "Del".into(),
                187 => "+".into(),
                189 => "−".into(),
                _ => Chord {
                    key: chord.key,
                    mods: 0,
                }
                .text(),
            }));
        }
    } else if !first.is_empty() {
        out.push(Cap::Text(first.into()));
    }
    (out, extra)
}
unsafe fn cap_width(dc: Dc, token: &Cap, dpi: u32) -> i32 {
    let s = |n| dpi::physical(n, dpi);
    match token {
        Cap::Symbol(_) => s(24),
        Cap::Text(text) => {
            let mut r = Rect::default();
            DrawTextW(dc, wide(text).as_ptr(), -1, &mut r, 0x400 | 0x20 | 0x800);
            (r.right + s(12)).max(s(22))
        }
    }
}
unsafe fn cap_frame(dc: Dc, r: Rect, t: Theme, dpi: u32) {
    let saved = SaveDC(dc);
    let brush = CreateSolidBrush(color(t.button));
    let pen = CreatePen(0, 1, color(t.selected));
    SelectObject(dc, brush);
    SelectObject(dc, pen);
    RoundRect(
        dc,
        r.left,
        r.top,
        r.right,
        r.bottom,
        dpi::physical(6, dpi),
        dpi::physical(6, dpi),
    );
    RestoreDC(dc, saved);
    DeleteObject(pen);
    DeleteObject(brush);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_subset_contains_every_selected_symbol_at_multiple_dpi() {
        unsafe {
            let dc = GetDC(null_mut());
            for dpi in [96, 144, 192] {
                let icons = Icons::new(dpi);
                assert!(!icons.resource.is_null());
                let saved = SaveDC(dc);
                for font in icons.fonts {
                    SelectObject(dc, font);
                    for icon in ALL {
                        let mut glyph = 0xffff;
                        assert_ne!(GetGlyphIndicesW(dc, &icon.code, 1, &mut glyph, 1), u32::MAX);
                        assert!(
                            glyph != 0xffff && glyph != 0,
                            "missing {} at {} DPI",
                            icon.name,
                            dpi
                        );
                    }
                }
                RestoreDC(dc, saved);
            }
            ReleaseDC(null_mut(), dc);
        }
    }
    #[test]
    fn keycap_symbols_preserve_configured_chords_and_alternatives() {
        let (caps, extras) = tokens("Ctrl+Shift+P, F1");
        assert_eq!(extras, 1);
        assert!(matches!(&caps[0], Cap::Text(s) if s == "Ctrl"));
        assert!(matches!(&caps[1], Cap::Symbol(s) if s.code == SHIFT_KEY.code));
        assert!(matches!(&caps[2], Cap::Text(s) if s == "P"));
        for (label, code) in [
            ("Enter", ENTER.code),
            ("Tab", TAB.code),
            ("Space", SPACE.code),
            ("Backspace", BACKSPACE.code),
            ("Up", UP.code),
            ("Alt+Wheel", MOUSE.code),
        ] {
            let (caps, _) = tokens(label);
            assert!(
                matches!(caps.last(), Some(Cap::Symbol(s)) if s.code == code),
                "{}",
                label
            );
        }
        assert_eq!(tokens("Alt").0.len(), 1);
        assert!(tokens("").0.is_empty());
        assert!(
            command(840).is_none(),
            "numeric brush presets retain their labels"
        );
    }
    #[test]
    fn keycaps_restore_gdi_state_and_scale_with_the_menu() {
        unsafe {
            let dc = GetDC(null_mut());
            let surface = Backbuffer::new(dc, 600, 180);
            ReleaseDC(null_mut(), dc);
            for dpi in [96, 144] {
                let icons = Icons::new(dpi);
                let font = GetStockObject(17);
                let saved = SaveDC(surface.dc);
                let width = icons.caps_width(surface.dc, "Ctrl+Shift+P", font, dpi);
                assert!(width > dpi::physical(70, dpi));
                for theme in [0, 1, 2] {
                    icons.keycaps(
                        surface.dc,
                        rect(0, theme * 50, width, 40),
                        "Ctrl+Shift+P",
                        font,
                        Theme::preset(theme as u8),
                        dpi,
                    );
                }
                assert_eq!(SaveDC(surface.dc), saved + 1);
                RestoreDC(surface.dc, saved);
            }
        }
    }
}
