//! Native IME-capable search input with a lightweight GDI command overlay.
use super::*;
use crate::ui::keymap::{Chord, InputState, Keymap, Kind, COMMANDS};
use crate::ui::palette::{Mode, Palette};
use std::cell::Cell;
thread_local! {static CAPTURE:Cell<u8>=Cell::new(0);static USED_KEY:Cell<bool>=Cell::new(false);}

pub(super) unsafe fn modifiers() -> u8 {
    (GetKeyState(17) < 0) as u8
        | (((GetKeyState(16) < 0) as u8) << 1)
        | (((GetKeyState(18) < 0) as u8) << 2)
}
unsafe extern "system" fn edit_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    let capture = CAPTURE.with(|c| c.get());
    if capture != 0 {
        if matches!(msg, 0x100 | 0x104) {
            if w == 27 {
                CAPTURE.with(|c| c.set(0));
                PostMessageW(GetParent(hwnd), 0x8010, 27, 0);
                return 0;
            }
            if !matches!(w, 16 | 17 | 18 | 160..=165) {
                USED_KEY.with(|c| c.set(true));
                PostMessageW(GetParent(hwnd), 0x8013, w, modifiers() as isize);
            }
            return 0;
        }
        if matches!(msg, 0x101 | 0x105)
            && capture == 2
            && matches!(w, 16 | 17 | 18 | 160..=165)
            && !USED_KEY.with(|c| c.get())
        {
            let released = match w {
                17 | 162 | 163 => 1,
                16 | 160 | 161 => 2,
                _ => 4,
            };
            PostMessageW(
                GetParent(hwnd),
                0x8013,
                0,
                (modifiers() | released) as isize,
            );
            return 0;
        }
        if msg == 0x20a {
            PostMessageW(
                GetParent(hwnd),
                0x8013,
                crate::ui::keymap::WHEEL as usize,
                modifiers() as isize,
            );
            return 0;
        }
        if matches!(msg, 0x102 | 0x106) {
            return 0;
        }
    }
    if msg == 0x20a {
        PostMessageW(
            GetParent(hwnd),
            0x8010,
            if (w >> 16) as i16 > 0 { 38 } else { 40 },
            0,
        );
        return 0;
    }
    if msg == 0x100 && matches!(w, 13 | 27 | 38 | 40 | 33 | 34 | 113 | 114 | 9) {
        PostMessageW(GetParent(hwnd), 0x8010, w, 0);
        return 0;
    }
    if msg == 0x102 && matches!(w, 9 | 13 | 27) {
        return 0;
    }
    if msg == 0x100 && w == 65 && GetKeyState(17) < 0 {
        SendMessageW(hwnd, 0xb1, 0, -1);
        return 0;
    }
    CallWindowProcW(GetWindowLongPtrW(hwnd, -21), hwnd, msg, w, l)
}
impl App {
    pub(super) unsafe fn input_state(&self) -> InputState {
        self.keymap
            .held(modifiers(), |key| GetKeyState(key as i32) < 0)
    }
    pub(super) fn palette_geometry(&self) -> (Rect, usize) {
        let width = (self.size.0 - 48).min(760).max(280);
        let rows = ((self.size.1 - TOP - 214) / 36).max(2).min(9) as usize;
        (
            rect(
                (self.size.0 - width) / 2,
                TOP + 12,
                width,
                174 + rows as i32 * 36,
            ),
            rows,
        )
    }
    pub(super) unsafe fn position_palette(&mut self) {
        if self.palette_edit.is_null() {
            return;
        }
        let (r, _) = self.palette_geometry();
        let font = CreateFontW(
            -dpi::physical(15, self.dpi),
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
            5,
            0,
            wide("Microsoft YaHei UI").as_ptr(),
        );
        SendMessageW(self.palette_edit, 0x30, font as usize, 1);
        DeleteObject(self.palette_font);
        self.palette_font = font;
        SetWindowPos(
            self.palette_edit,
            null_mut(),
            dpi::physical(r.left + 20 + self.content_padding(), self.dpi),
            dpi::physical(r.top + 44 + self.content_padding(), self.dpi),
            dpi::physical(r.right - r.left - 40, self.dpi),
            dpi::physical(28, self.dpi),
            0x14,
        );
    }
    pub(super) unsafe fn open_palette(&mut self, hwnd: Hwnd, mode: Mode) {
        self.cancel_input(hwnd);
        self.close_palette(hwnd);
        self.palette = Some(Palette::new(mode, &self.keymap));
        self.palette_edit = CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            wide("").as_ptr(),
            0x50010080,
            0,
            0,
            1,
            1,
            hwnd,
            null_mut(),
            GetModuleHandleW(null()),
            null_mut(),
        );
        if self.palette_edit.is_null() {
            self.palette = None;
            self.error(hwnd, "Failed to Create Search Field");
            return;
        }
        let old = SetWindowLongPtrW(self.palette_edit, -4, edit_proc as usize as isize);
        SetWindowLongPtrW(self.palette_edit, -21, old);
        SendMessageW(self.palette_edit, 0xc5, 512, 0);
        self.position_palette();
        SetFocus(self.palette_edit);
        self.hits.clear();
        self.hovered = None;
        self.invalidate_scene(hwnd);
    }
    pub(super) unsafe fn close_palette(&mut self, hwnd: Hwnd) {
        CAPTURE.with(|c| c.set(0));
        if !self.palette_edit.is_null() {
            DestroyWindow(self.palette_edit);
            self.palette_edit = null_mut();
        }
        DeleteObject(self.palette_font);
        self.palette_font = null_mut();
        self.palette = None;
        self.hits.clear();
        self.hovered = None;
        SetFocus(hwnd);
        self.invalidate_scene(hwnd);
    }
    fn edit_text(&self) -> String {
        unsafe {
            let n = GetWindowTextLengthW(self.palette_edit).max(0) as usize;
            let mut text = vec![0u16; n + 1];
            let len = GetWindowTextW(self.palette_edit, text.as_mut_ptr(), text.len() as i32);
            String::from_utf16_lossy(&text[..len.max(0) as usize])
        }
    }
    pub(super) unsafe fn palette_query(&mut self, hwnd: Hwnd) {
        if self.palette_edit.is_null() {
            return;
        }
        let text = self.edit_text();
        if let Some(p) = self.palette.as_mut() {
            if p.recording.is_none() && p.query != text {
                p.query(text, &self.keymap);
                p.message.clear();
            }
        }
        self.invalidate_scene(hwnd);
    }
    unsafe fn begin_binding(&mut self, hwnd: Hwnd) {
        if let Some(p) = self.palette.as_mut() {
            if let Some(index) = p.current() {
                p.recording = Some(index);
                p.message = format!(
                    "{}: Enter comma-separated shortcuts; Enter to save, Esc to return",
                    COMMANDS[index].title
                );
                SetWindowTextW(self.palette_edit, wide(&self.keymap.label(index)).as_ptr());
                SendMessageW(self.palette_edit, 0xb1, 0, -1);
                SetFocus(self.palette_edit);
            }
        }
        self.invalidate_scene(hwnd);
    }
    unsafe fn change_binding(&mut self, hwnd: Hwnd, index: usize, keys: Vec<Chord>) {
        let changed = self.keymap.changed(index, keys).and_then(|map| {
            map.save(&Keymap::path())?;
            Ok(map)
        });
        match changed {
            Ok(map) => {
                CAPTURE.with(|c| c.set(0));
                self.keymap = map;
                if let Some(p) = self.palette.as_mut() {
                    p.recording = None;
                    p.message = "Saved and Applied".into();
                    p.refresh(&self.keymap);
                    SetWindowTextW(self.palette_edit, wide(&p.query).as_ptr());
                }
            }
            Err(e) => {
                if let Some(p) = self.palette.as_mut() {
                    p.message = e;
                }
            }
        }
        self.invalidate_scene(hwnd);
    }
    unsafe fn record_binding(&mut self, hwnd: Hwnd) {
        self.begin_binding(hwnd);
        if let Some(i) = self.palette.as_ref().and_then(|p| p.recording) {
            CAPTURE.with(|c| {
                c.set(if matches!(COMMANDS[i].kind, Kind::Hold(_)) {
                    2
                } else {
                    1
                })
            });
            USED_KEY.with(|c| c.set(false));
            self.palette.as_mut().unwrap().message =
                "Press a shortcut; hold gestures accept modifiers alone; Esc to cancel".into();
            SetWindowTextW(self.palette_edit, wide("Waiting for Input…").as_ptr());
            self.invalidate_scene(hwnd);
        }
    }
    pub(super) unsafe fn captured_binding(&mut self, hwnd: Hwnd, key: u16, mods: u8) {
        if let Some(i) = self.palette.as_ref().and_then(|p| p.recording) {
            let chord = Chord { key, mods };
            if Chord::parse(&chord.text()) != Ok(chord) {
                self.palette.as_mut().unwrap().message =
                    "Unsupported key; try another shortcut".into();
                self.invalidate_scene(hwnd);
                return;
            }
            self.change_binding(hwnd, i, vec![chord]);
        }
    }
    pub(super) unsafe fn palette_key(&mut self, hwnd: Hwnd, key: usize) {
        let p = match self.palette.as_ref() {
            Some(p) => p,
            None => return,
        };
        if let Some(index) = p.recording {
            if key == 27 {
                CAPTURE.with(|c| c.set(0));
                let p = self.palette.as_mut().unwrap();
                p.recording = None;
                p.message.clear();
                SetWindowTextW(self.palette_edit, wide(&p.query).as_ptr());
            } else if key == 13 {
                let text = self.edit_text();
                let result: Result<Vec<_>, _> = if text.trim().is_empty() {
                    Ok(vec![])
                } else {
                    text.split(',').map(Chord::parse).collect()
                };
                match result {
                    Ok(keys) => self.change_binding(hwnd, index, keys),
                    Err(e) => self.palette.as_mut().unwrap().message = e,
                }
            }
        } else {
            match key {
                27 => self.close_palette(hwnd),
                38 | 40 | 33 | 34 => self.palette.as_mut().unwrap().step(match key {
                    38 => -1,
                    40 => 1,
                    33 => -7,
                    _ => 7,
                }),
                114 => {
                    if p.mode == Mode::Bindings {
                        self.record_binding(hwnd);
                    }
                }
                113 => {
                    if p.mode == Mode::Bindings {
                        self.begin_binding(hwnd);
                    }
                }
                13 => {
                    if p.mode == Mode::Bindings {
                        self.begin_binding(hwnd);
                    } else if let Some(i) = p.current() {
                        self.close_palette(hwnd);
                        PostMessageW(hwnd, 0x8011, i, 0);
                    }
                }
                9 => {
                    let mode = if p.mode == Mode::Bindings {
                        Mode::Commands
                    } else {
                        Mode::Bindings
                    };
                    self.open_palette(hwnd, mode);
                }
                _ => {}
            }
        }
        self.invalidate_scene(hwnd);
    }
    pub(super) unsafe fn palette_click(&mut self, hwnd: Hwnd, x: i32, y: i32) {
        let id = self.hits.iter().find(|h| inside(h.r, x, y)).map(|h| h.id);
        match id {
            Some(id) if id >= 20000 => {
                let index = (id - 20000) as usize;
                let p = self.palette.as_mut().unwrap();
                if p.recording.is_none() {
                    p.selected = index;
                    if p.mode == Mode::Commands {
                        self.palette_key(hwnd, 13);
                    }
                }
            }
            Some(12005) => self.record_binding(hwnd),
            Some(12001) => self.begin_binding(hwnd),
            Some(12002) | Some(12003) => {
                let p = self.palette.as_ref().unwrap();
                if p.recording.is_none() {
                    if let Some(i) = p.current() {
                        let keys = if id == Some(12002) {
                            vec![]
                        } else {
                            Keymap::default().bindings[i].clone()
                        };
                        self.change_binding(hwnd, i, keys);
                    }
                }
            }
            Some(12004) => self.palette_key(hwnd, 9),
            _ => {
                let (r, _) = self.palette_geometry();
                if !inside(r, x, y) {
                    self.close_palette(hwnd);
                } else {
                    SetFocus(self.palette_edit);
                }
            }
        }
        self.invalidate_scene(hwnd);
    }
    pub(super) unsafe fn draw_palette(&mut self, dc: Dc) {
        let p = match self.palette.as_ref() {
            Some(p) => p,
            None => return,
        };
        let (r, rows) = self.palette_geometry();
        let t = self.workspace.theme();
        let mode = p.mode;
        let selected = p.selected;
        let recording = p.recording;
        let message = p.message.clone();
        let start = selected.saturating_sub(rows - 1);
        let results: Vec<_> = p
            .results
            .iter()
            .copied()
            .enumerate()
            .skip(start)
            .take(rows)
            .collect();
        self.hits.clear();
        SelectObject(dc, self.font);
        self.rounded(
            dc,
            rect(r.left + 4, r.top + 5, r.right - r.left, r.bottom - r.top),
            0x282b33,
            10,
        );
        self.rounded(dc, r, t.panel, 10);
        self.frame(dc, r, t.muted);
        self.icons.draw(
            dc,
            rect(r.left + 16, r.top + 9, 24, 24),
            if mode == Mode::Commands {
                icons::SEARCH
            } else {
                icons::KEYBOARD
            },
            20,
            t.accent,
        );
        Self::text(
            dc,
            rect(r.left + 48, r.top + 6, r.right - r.left - 66, 30),
            if mode == Mode::Commands {
                "Command Palette"
            } else {
                "Keyboard Shortcuts"
            },
            t.text,
        );
        Self::fill(
            dc,
            rect(r.left + 16, r.top + 39, r.right - r.left - 32, 38),
            0xffffff,
        );
        self.frame(
            dc,
            rect(r.left + 16, r.top + 39, r.right - r.left - 32, 38),
            t.accent,
        );
        if self.palette_edit.is_null() {
            Self::text(
                dc,
                rect(r.left + 20, r.top + 44, r.right - r.left - 40, 28),
                &self.palette.as_ref().unwrap().query,
                0x252830,
            );
        }
        SelectObject(dc, self.small_font);
        Self::text(
            dc,
            rect(r.left + 18, r.top + 79, r.right - r.left - 36, 30),
            if recording.is_some() {
                "Examples: Ctrl+K / Shift+[ / Alt / Ctrl+Wheel; leave empty to disable"
            } else {
                "Search names, keywords or shortcuts · ↑↓ Select · Enter Confirm · Tab Switch"
            },
            t.muted,
        );
        for (pos, index) in results {
            let y = r.top + 112 + (pos - start) as i32 * 36;
            if pos == selected {
                self.rounded(
                    dc,
                    rect(r.left + 8, y, r.right - r.left - 16, 34),
                    t.selected,
                    4,
                );
            }
            let ink = t.text;
            self.icons.draw(
                dc,
                rect(r.left + 18, y + 7, 22, 22),
                icons::command(COMMANDS[index].ui).unwrap_or(icons::TUNE),
                20,
                if pos == selected { t.accent } else { t.muted },
            );
            Self::text(
                dc,
                rect(r.left + 48, y, r.right - r.left - 338, 34),
                &format!("{}：{}", COMMANDS[index].category, COMMANDS[index].title),
                ink,
            );
            let label = self.keymap.label(index);
            let keys = rect(r.right - 290, y, 272, 34);
            if label.is_empty() {
                Self::text(dc, keys, "Unbound", t.muted);
            } else {
                self.icons.keycaps(dc, keys, &label, self.small_font, t, 96);
            }
            self.hits.push(Hit {
                r: rect(r.left + 8, y, r.right - r.left - 16, 34),
                id: 20000 + pos as u32,
            });
        }
        if self.palette.as_ref().unwrap().results.is_empty() {
            Self::centered(
                dc,
                rect(r.left + 20, r.top + 120, r.right - r.left - 40, 40),
                "No Matching Commands",
                t.muted,
            );
        }
        let footer = r.top + 112 + rows as i32 * 36;
        if mode == Mode::Bindings && recording.is_none() {
            for (offset, label, id) in [
                (0, "Edit F2", 12001),
                (128, "Record F3", 12005),
                (236, "Clear Binding", 12002),
                (344, "Restore Defaults", 12003),
            ] {
                let b = rect(
                    r.left + 16 + offset,
                    footer,
                    if offset == 0 { 120 } else { 100 },
                    26,
                );
                self.rounded(dc, b, t.selected, 4);
                let icon = match id {
                    12001 => icons::EDIT,
                    12005 => icons::KEYBOARD,
                    12002 => icons::BACKSPACE,
                    _ => icons::UNDO,
                };
                self.icons
                    .draw(dc, rect(b.left + 5, b.top + 3, 20, 20), icon, 16, t.text);
                Self::centered(
                    dc,
                    rect(b.left + 26, b.top, b.right - b.left - 28, 26),
                    label,
                    t.text,
                );
                self.hits.push(Hit { r: b, id });
            }
        }
        let b = rect(r.right - 106, footer, 90, 26);
        self.icons.draw(
            dc,
            rect(b.left, b.top + 3, 20, 20),
            if mode == Mode::Commands {
                icons::KEYBOARD
            } else {
                icons::SEARCH
            },
            16,
            t.accent,
        );
        Self::centered(
            dc,
            rect(b.left + 22, b.top, 68, 26),
            if mode == Mode::Commands {
                "Shortcut"
            } else {
                "Command Palette"
            },
            t.accent,
        );
        self.hits.push(Hit { r: b, id: 12004 });
        if !message.is_empty() {
            Self::text(
                dc,
                rect(r.left + 18, r.bottom - 22, r.right - r.left - 36, 22),
                &message,
                t.accent,
            );
        }
    }
}
