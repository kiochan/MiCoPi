//! Custom title bar; Windows still owns moving, resizing and system commands.
use super::*;
pub(super) const MINIMIZE: u32 = 10001;
pub(super) const MAXIMIZE: u32 = 10002;
pub(super) const CLOSE: u32 = 10003;
pub(super) unsafe fn native_metadata(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    // DefWindowProc must still store taskbar/Alt+Tab metadata, but must not draw
    // a classic caption over our client area. Keep the suppression synchronous;
    // never hide/show the window or disable repainting across the message loop.
    const STYLE: i32 = -16;
    const VISIBLE: isize = 0x10000000;
    let style = GetWindowLongPtrW(hwnd, STYLE);
    if style & VISIBLE != 0 {
        SetWindowLongPtrW(hwnd, STYLE, style & !VISIBLE);
    }
    let result = DefWindowProcW(hwnd, msg, w, l);
    if style & VISIBLE != 0 {
        let current = GetWindowLongPtrW(hwnd, STYLE);
        SetWindowLongPtrW(hwnd, STYLE, current | VISIBLE);
    }
    result
}
pub(super) unsafe fn set_title(hwnd: Hwnd, title: &str) -> bool {
    let title = wide(title);
    let length = GetWindowTextLengthW(hwnd).max(0) as usize;
    if length + 1 == title.len() {
        let mut current = vec![0u16; length + 1];
        GetWindowTextW(hwnd, current.as_mut_ptr(), current.len() as i32);
        if current == title {
            return false;
        }
    }
    SetWindowTextW(hwnd, title.as_ptr()) != 0
}
unsafe fn attribute(hwnd: Hwnd, id: u32, value: u32) {
    // Unsupported attributes are ignored by older Windows versions.
    DwmSetWindowAttribute(
        hwnd,
        id,
        &value as *const _ as _,
        mem::size_of::<u32>() as u32,
    );
}
pub(super) unsafe fn shape(hwnd: Hwnd, maximized: bool) {
    // Keep a tiny compositor-owned edge so a client-drawn window retains DWM shadow.
    // Maximized windows must meet the work area without transparent insets.
    let edge = if maximized { 0 } else { 1 };
    DwmExtendFrameIntoClientArea(
        hwnd,
        &Margins {
            left: edge,
            right: edge,
            top: edge,
            bottom: edge,
        },
    );
    attribute(hwnd, 33, if maximized { 1 } else { 3 }); // DONOTROUND / ROUNDSMALL (Windows 11).
}
pub(super) unsafe fn appearance(hwnd: Hwnd, theme: crate::ui::Theme, dark: bool) {
    attribute(hwnd, 2, 2); // DWMWA_NCRENDERING_POLICY = DWMNCRP_ENABLED.
    attribute(hwnd, 20, dark as u32);
    attribute(hwnd, 34, color(theme.selected)); // Native edge matches the app theme.
    shape(hwnd, IsZoomed(hwnd) != 0);
}
// Keep the workbench away from the screen edge after Windows removes its frame.
pub(super) fn content_padding(maximized: bool) -> i32 {
    if maximized {
        8
    } else {
        0
    }
}
pub(super) fn fit_maximized_client(proposed: Rect, work: Rect) -> Rect {
    let fitted = Rect {
        left: proposed.left.max(work.left),
        top: proposed.top.max(work.top),
        right: proposed.right.min(work.right),
        bottom: proposed.bottom.min(work.bottom),
    };
    if fitted.right > fitted.left && fitted.bottom > fitted.top {
        fitted
    } else {
        proposed
    }
}
pub(super) unsafe fn calculate_client(hwnd: Hwnd, l: isize) {
    // NCCALCSIZE_PARAMS starts with the proposed RECT, as does the wParam=0 form.
    // A maximized thick-frame window can extend beyond the visible monitor area.
    if l == 0 || IsZoomed(hwnd) == 0 {
        return;
    }
    let proposed = &mut *(l as *mut Rect);
    let mut info = MonitorInfo {
        size: mem::size_of::<MonitorInfo>() as u32,
        monitor: Rect::default(),
        work: Rect::default(),
        flags: 0,
    };
    if GetMonitorInfoW(MonitorFromRect(proposed, 2), &mut info) != 0 {
        *proposed = fit_maximized_client(*proposed, info.work);
    }
}
pub(super) fn button(width: i32, id: u32) -> Rect {
    rect(width - 138 + (id - MINIMIZE) as i32 * 46, 0, 46, TOP)
}
pub(super) fn hit(width: i32, height: i32, x: i32, y: i32, border: i32, maximized: bool) -> isize {
    if x < 0 || y < 0 || x >= width || y >= height {
        return 0;
    }
    if !maximized {
        let left = x < border;
        let right = x >= width - border;
        let top = y < border;
        let bottom = y >= height - border;
        match (left, right, top, bottom) {
            (true, _, true, _) => return 13,
            (_, true, true, _) => return 14,
            (true, _, _, true) => return 16,
            (_, true, _, true) => return 17,
            (true, _, _, _) => return 10,
            (_, true, _, _) => return 11,
            (_, _, true, _) => return 12,
            (_, _, _, true) => return 15,
            _ => {}
        }
    }
    if y < TOP {
        if (x >= 56 && x < 284) || inside(search(width), x, y) || x >= width - 218 {
            1
        } else {
            2
        }
    } else {
        1
    }
}
pub(super) fn search(width: i32) -> Rect {
    let w = (width - 540).max(160).min(520);
    rect(((width - w) / 2).max(300), 6, w, 24)
}

pub(super) unsafe fn hit_window(hwnd: Hwnd, l: isize) -> isize {
    let mut r = Rect::default();
    GetClientRect(hwnd, &mut r);
    let mut p = Point {
        x: l as i16 as i32,
        y: (l >> 16) as i16 as i32,
    };
    ScreenToClient(hwnd, &mut p);
    let dpi = dpi::for_window(hwnd);
    let scale = |n| dpi::logical(n, dpi);
    let maximized = IsZoomed(hwnd) != 0;
    let padding = content_padding(maximized);
    if p.x < 0 || p.y < 0 || p.x >= r.right || p.y >= r.bottom {
        return 0;
    }
    if maximized {
        if scale(p.y) < padding {
            return 2;
        }
        if scale(p.x) < padding
            || scale(p.x) >= scale(r.right) - padding
            || scale(p.y) >= scale(r.bottom) - padding
        {
            return 1;
        }
    }
    hit(
        scale(r.right) - padding * 2,
        scale(r.bottom) - padding * 2,
        scale(p.x) - padding,
        scale(p.y) - padding,
        5,
        maximized,
    )
}
pub(super) unsafe fn minmax(hwnd: Hwnd, l: isize) {
    if l == 0 {
        return;
    }
    let points = l as *mut Point;
    let dpi = dpi::for_window(hwnd);
    (*points.add(3)).x = dpi::physical(900, dpi);
    (*points.add(3)).y = dpi::physical(600, dpi);
    let monitor = MonitorFromWindow(hwnd, 2);
    let mut info = MonitorInfo {
        size: mem::size_of::<MonitorInfo>() as u32,
        monitor: Rect::default(),
        work: Rect::default(),
        flags: 0,
    };
    if GetMonitorInfoW(monitor, &mut info) != 0 {
        *points.add(1) = Point {
            x: info.work.right - info.work.left,
            y: info.work.bottom - info.work.top,
        };
        *points.add(2) = Point {
            x: info.work.left - info.monitor.left,
            y: info.work.top - info.monitor.top,
        };
    }
}
impl App {
    pub(super) unsafe fn draw_chrome(&mut self, dc: Dc) {
        let t = self.workspace.theme();
        let search = search(self.size.0);
        self.rounded(dc, search, t.button, 5);
        self.frame(
            dc,
            search,
            if self.workspace.theme == 2 {
                0x364149
            } else {
                0xd1d7df
            },
        );
        self.icons.draw(
            dc,
            rect(search.left + 6, search.top + 2, 22, 20),
            icons::SEARCH,
            16,
            t.muted,
        );
        SelectObject(dc, self.small_font);
        Self::text(
            dc,
            rect(
                search.left + 34,
                search.top,
                (search.right
                    - search.left
                    - 34
                    - if search.right - search.left > 400 {
                        154
                    } else {
                        8
                    })
                .max(1),
                24,
            ),
            "Search Commands, Tools and Settings…",
            t.muted,
        );
        self.hit(search, 95);
        if search.right - search.left > 400 {
            self.icons.keycaps(
                dc,
                rect(search.right - 154, search.top, 148, 24),
                &crate::ui::keymap::COMMANDS
                    .iter()
                    .position(|c| c.ui == 95)
                    .map(|i| self.keymap.label(i))
                    .unwrap_or_default(),
                self.small_font,
                t,
                96,
            );
        }
        for (offset, id) in [(218, 60), (180, 61)] {
            let r = rect(self.size.0 - offset, 5, 30, 26);
            if self.hovered.map_or(false, |h| h.0 == id) {
                self.rounded(dc, r, t.selected, 4);
            }
            self.icons.draw(
                dc,
                r,
                if id == 60 {
                    icons::DOCK_LEFT
                } else {
                    icons::DOCK_RIGHT
                },
                20,
                t.muted,
            );
            self.hit(r, id);
        }
        for id in MINIMIZE..=CLOSE {
            let r = button(self.size.0, id);
            let hovered = self.hovered.map_or(false, |h| h.0 == id);
            let bg = if hovered && id == CLOSE {
                0xd44c58
            } else if hovered {
                t.selected
            } else {
                t.panel
            };
            Self::fill(dc, r, bg);
            let ink = if hovered && id == CLOSE {
                0xffffff
            } else {
                t.muted
            };
            let saved = SaveDC(dc);
            let pen = CreatePen(0, 1, color(ink));
            SelectObject(dc, pen);
            let x = r.left + 18;
            let y = 13;
            match id {
                MINIMIZE => {
                    MoveToEx(dc, x, y + 6, null_mut());
                    LineTo(dc, x + 10, y + 6);
                }
                MAXIMIZE => {
                    if self.maximized {
                        self.frame(dc, rect(x + 2, y, 9, 8), ink);
                        Self::fill(dc, rect(x, y + 3, 9, 8), bg);
                        self.frame(dc, rect(x, y + 3, 9, 8), ink);
                    } else {
                        self.frame(dc, rect(x, y, 10, 10), ink);
                    }
                }
                _ => {
                    MoveToEx(dc, x, y, null_mut());
                    LineTo(dc, x + 10, y + 10);
                    MoveToEx(dc, x + 9, y, null_mut());
                    LineTo(dc, x - 1, y + 10);
                }
            }
            RestoreDC(dc, saved);
            DeleteObject(pen);
            self.hit(r, id);
        }
        SelectObject(dc, self.font);
    }
}
