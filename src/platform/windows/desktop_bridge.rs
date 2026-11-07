//! Small native adapter: file codecs, system frame and full-resolution Windows Ink.
//! The egui shell never borrows the legacy App or creates a second message loop.
use super::*;
use eframe::egui;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PenPhase {
    Down,
    Move,
    Up,
    Hover,
    Cancel,
    Leave,
}
#[derive(Clone, Copy, Debug)]
pub struct PenSample {
    pub phase: PenPhase,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub eraser: bool,
    pub time: u32,
}
struct Hook {
    hwnd: Hwnd,
    previous: isize,
    context: egui::Context,
    samples: VecDeque<PenSample>,
    sizing: bool,
    moving: bool,
    move_cancelled: bool,
    group_drop: Option<(i32, i32)>,
    drag_offset: Option<(i32, i32)>,
    drag_point: Option<(i32, i32)>,
}
impl Hook {
    fn observe_group_move(&mut self, rect: Option<&Rect>, cursor: Option<(i32, i32)>) {
        if self.sizing {
            self.moving = true;
            self.drag_point = cursor.or_else(|| {
                rect.and_then(|r| self.drag_offset.map(|(x, y)| (r.left + x, r.top + y)))
            });
        }
    }
    fn finish_group_move(&mut self, cursor: Option<(i32, i32)>, escape: bool) {
        if self.moving && !self.move_cancelled && !escape {
            self.group_drop = cursor.or(self.drag_point);
        }
        self.sizing = false;
        self.moving = false;
        self.drag_offset = None;
        self.drag_point = None;
    }
}
thread_local! { static HOOKS: RefCell<BTreeMap<usize, Hook>> = const { RefCell::new(BTreeMap::new()) }; }

pub struct Native {
    hwnd: Hwnd,
    gdiplus: usize,
}
impl Native {
    /// Must be created and dropped on the native event-loop thread.
    pub fn new(hwnd: isize, context: egui::Context) -> Self {
        unsafe {
            let mut token = 0;
            GdiplusStartup(
                &mut token,
                &GdiStartup {
                    version: 1,
                    callback: null_mut(),
                    suppress_thread: 0,
                    suppress_codecs: 0,
                },
                null_mut(),
            );
            let hwnd = hwnd as Hwnd;
            if !hwnd.is_null() {
                let previous = SetWindowLongPtrW(hwnd, -4, procedure as *const () as isize);
                HOOKS.with(|slot| {
                    slot.borrow_mut().insert(
                        hwnd as usize,
                        Hook {
                            hwnd,
                            previous,
                            context,
                            samples: VecDeque::with_capacity(128),
                            sizing: false,
                            moving: false,
                            move_cancelled: false,
                            group_drop: None,
                            drag_offset: None,
                            drag_point: None,
                        },
                    )
                });
                chrome::appearance(hwnd, crate::ui::Theme::preset(2), true);
                // Recalculate the frame after subclassing, without moving or showing it.
                SetWindowPos(hwnd, null_mut(), 0, 0, 0, 0, 0x1 | 0x2 | 0x4 | 0x10 | 0x20);
            }
            Self {
                hwnd,
                gdiplus: token,
            }
        }
    }
    /// Discover a newly created viewport by its unique bootstrap title on this UI thread.
    /// Titles are never used to attach to another process or an already hooked window.
    pub fn for_viewport(title: &str, context: egui::Context) -> Option<Self> {
        unsafe extern "system" fn visit(hwnd: Hwnd, data: isize) -> i32 {
            let search = &mut *(data as *mut (String, Hwnd));
            let mut buffer = [0u16; 512];
            let n = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if n > 0 && String::from_utf16_lossy(&buffer[..n as usize]) == search.0 {
                search.1 = hwnd;
                return 0;
            }
            1
        }
        unsafe {
            let mut search: (String, Hwnd) = (title.to_owned(), null_mut());
            EnumThreadWindows(GetCurrentThreadId(), visit, &mut search as *mut _ as isize);
            if search.1.is_null() {
                None
            } else {
                Some(Self::new(search.1 as isize, context))
            }
        }
    }
    pub fn window_id(&self) -> usize {
        self.hwnd as usize
    }
    pub fn owner(&self) -> usize {
        unsafe { GetWindowLongPtrW(self.hwnd, -8) as usize }
    }
    pub fn set_owner(&self, owner: usize) {
        if owner != self.hwnd as usize && self.owner() != owner {
            unsafe {
                SetWindowLongPtrW(self.hwnd, -8, owner as isize);
                SetWindowPos(self.hwnd, null_mut(), 0, 0, 0, 0, 0x1 | 0x2 | 0x10);
            }
        }
    }
    pub fn is_sizing(&self) -> bool {
        HOOKS.with(|hooks| {
            hooks
                .borrow()
                .get(&(self.hwnd as usize))
                .is_some_and(|h| h.sizing)
        })
    }
    pub fn primary_down() -> bool {
        unsafe { GetAsyncKeyState(1) < 0 }
    }
    pub fn begin_group_drag(&self, client: egui::Pos2) {
        HOOKS.with(|hooks| {
            if let Some(h) = hooks.borrow_mut().get_mut(&(self.hwnd as usize)) {
                h.drag_offset = Some((client.x.round() as i32, client.y.round() as i32));
                h.drag_point = None;
            }
        });
    }
    pub fn group_drag_cursor(&self) -> Option<(i32, i32)> {
        HOOKS.with(|hooks| {
            hooks
                .borrow()
                .get(&(self.hwnd as usize))
                .filter(|h| h.moving && !h.move_cancelled)
                .and_then(|h| h.drag_point)
        })
    }
    pub fn take_group_drop(&self) -> Option<(i32, i32)> {
        HOOKS.with(|hooks| {
            hooks
                .borrow_mut()
                .get_mut(&(self.hwnd as usize))
                .and_then(|h| h.group_drop.take())
        })
    }
    pub fn client_point(&self, point: (i32, i32), pixels_per_point: f32) -> Option<egui::Pos2> {
        unsafe {
            let mut p = Point {
                x: point.0,
                y: point.1,
            };
            (ScreenToClient(self.hwnd, &mut p) != 0)
                .then(|| egui::pos2(p.x as f32, p.y as f32) / pixels_per_point)
        }
    }
    pub fn window_under(point: (i32, i32), except: usize) -> Option<usize> {
        unsafe {
            let hit = WindowFromPoint(Point {
                x: point.0,
                y: point.1,
            });
            if !hit.is_null() && hit as usize != except {
                let top = GetAncestor(hit, 2);
                if !top.is_null() && top as usize != except {
                    return Some(top as usize);
                }
            }
            // Start below the dragged HWND rather than a global desktop enumeration.
            // This also works on restricted desktops where GetTopWindow(NULL) is unavailable.
            let mut hwnd = GetWindow(except as Hwnd, 2);
            for _ in 0..4096 {
                if hwnd.is_null() {
                    break;
                }
                let mut r = Rect::default();
                let mut cloaked = 0u32;
                DwmGetWindowAttribute(hwnd, 14, &mut cloaked as *mut _ as *mut std::ffi::c_void, 4);
                let ex_style = GetWindowLongPtrW(hwnd, -20) as u32;
                let passthrough = ex_style & 0x80000 != 0 && ex_style & 0x20 != 0;
                if cloaked == 0
                    && !passthrough
                    && hwnd as usize != except
                    && IsWindowVisible(hwnd) != 0
                    && IsIconic(hwnd) == 0
                    && GetWindowRect(hwnd, &mut r) != 0
                    && point.0 >= r.left
                    && point.0 < r.right
                    && point.1 >= r.top
                    && point.1 < r.bottom
                {
                    return Some(hwnd as usize);
                }
                hwnd = GetWindow(hwnd, 2);
            }
        }
        None
    }
    pub fn clear_pen(&self) {
        HOOKS.with(|hooks| {
            if let Some(hook) = hooks.borrow_mut().get_mut(&(self.hwnd as usize)) {
                hook.samples.clear();
            }
        });
    }
    pub fn drain_pen(&self, into: &mut Vec<PenSample>) {
        into.clear();
        HOOKS.with(|slot| {
            if let Some(h) = slot.borrow_mut().get_mut(&(self.hwnd as usize)) {
                into.extend(h.samples.drain(..));
            }
        });
    }
    pub fn dialog(&self, save: bool, png: bool) -> Option<PathBuf> {
        unsafe { file_dialog(self.hwnd, save, png) }
    }
    pub fn preset_dialog(&self, save: bool) -> Option<PathBuf> {
        unsafe {
            super::file_dialog_custom(
                self.hwnd,
                save,
                "My Brushes.json",
                "MiCoPi brush presets\0*.json\0All files\0*.*\0\0",
                "json",
            )
        }
    }
    pub fn replay_dialog(&self, save: bool, frames: bool) -> Option<PathBuf> {
        unsafe {
            super::file_dialog_custom(
                self.hwnd,
                save,
                if frames {
                    "timelapse"
                } else {
                    "Untitled.mcreplay"
                },
                if frames {
                    "PNG frame folder\0*.*\0\0"
                } else {
                    "MiCoPi replay\0*.mcreplay\0All files\0*.*\0\0"
                },
                if frames { "" } else { "mcreplay" },
            )
        }
    }
    pub fn confirm_discard(&self) -> bool {
        unsafe {
            MessageBoxW(
                self.hwnd,
                wide(&crate::i18n::text(
                    "Discard Unsaved Changes to This Artwork?",
                ))
                .as_ptr(),
                wide("MiCoPi").as_ptr(),
                0x4 | 0x30 | 0x100,
            ) == 6
        }
    }
    pub fn confirm_clear_layer(&self) -> bool {
        unsafe {
            MessageBoxW(
                self.hwnd,
                wide(&crate::i18n::text(
                    "Clear the Current Layer? This Can Be Undone.",
                ))
                .as_ptr(),
                wide("MiCoPi").as_ptr(),
                0x4 | 0x30 | 0x100,
            ) == 6
        }
    }
    pub fn appearance(&self, theme: crate::ui::Theme, dark: bool) {
        unsafe {
            if !self.hwnd.is_null() {
                chrome::appearance(self.hwnd, theme, dark);
            }
        }
    }
    pub fn load_image(&self, path: &Path) -> std::io::Result<Document> {
        unsafe { load_image(path) }
    }
    pub fn save_png(
        &self,
        path: &Path,
        pixels: &mut [u32],
        w: usize,
        h: usize,
    ) -> std::io::Result<()> {
        unsafe { save_png(path, pixels, w, h) }
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            HOOKS.with(|slot| {
                let hook = slot.borrow_mut().remove(&(self.hwnd as usize));
                if let Some(hook) = hook {
                    if GetWindowLongPtrW(hook.hwnd, -4) == procedure as *const () as isize {
                        SetWindowLongPtrW(hook.hwnd, -4, hook.previous);
                    }
                }
            });
            if self.gdiplus != 0 {
                GdiplusShutdown(self.gdiplus);
            }
        }
    }
}
unsafe fn enqueue(hwnd: Hwnd, info: PenInfo, phase: PenPhase) {
    let mut point = info.pointer.pixel;
    ScreenToClient(hwnd, &mut point);
    let sample = PenSample {
        phase,
        x: point.x as f32,
        y: point.y as f32,
        pressure: if info.mask & 1 != 0 {
            info.pressure.min(1024) as f32 / 1024.
        } else {
            1.
        },
        eraser: info.flags & 6 != 0,
        time: info.pointer.time,
    };
    HOOKS.with(|slot| {
        if let Some(hook) = slot.borrow_mut().get_mut(&(hwnd as usize)) {
            if hook.samples.len() >= 4096 {
                hook.samples.clear();
                hook.samples.push_back(PenSample {
                    phase: PenPhase::Cancel,
                    ..sample
                });
            }
            hook.samples.push_back(sample);
            hook.context.request_repaint();
        }
    });
}
unsafe extern "system" fn procedure(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
    // Release the hook borrow before forwarding: winit and Windows can reenter.
    let previous = HOOKS
        .with(|slot| slot.borrow().get(&(hwnd as usize)).map(|h| h.previous))
        .unwrap_or(0);
    match msg {
        0x231 => {
            HOOKS.with(|hooks| {
                if let Some(h) = hooks.borrow_mut().get_mut(&(hwnd as usize)) {
                    h.sizing = true;
                    h.moving = false;
                    h.move_cancelled = false;
                    h.group_drop = None;
                    h.context.request_repaint();
                }
            });
        }
        0x216 => {
            let mut cursor = Point::default();
            let have_cursor = GetCursorPos(&mut cursor) != 0;
            HOOKS.with(|hooks| {
                if let Some(h) = hooks.borrow_mut().get_mut(&(hwnd as usize)) {
                    h.observe_group_move(
                        if l != 0 {
                            Some(&*(l as *const Rect))
                        } else {
                            None
                        },
                        have_cursor.then_some((cursor.x, cursor.y)),
                    );
                    h.context.request_repaint();
                }
            });
        }
        0x100 if w == 27 => {
            HOOKS.with(|hooks| {
                if let Some(h) = hooks.borrow_mut().get_mut(&(hwnd as usize)) {
                    if h.sizing {
                        h.move_cancelled = true;
                    }
                }
            });
        }
        0x232 => {
            let mut p = Point::default();
            let have_cursor = GetCursorPos(&mut p) != 0;
            let escape = GetAsyncKeyState(27) < 0;
            HOOKS.with(|hooks| {
                if let Some(h) = hooks.borrow_mut().get_mut(&(hwnd as usize)) {
                    h.finish_group_move(have_cursor.then_some((p.x, p.y)), escape);
                    h.context.request_repaint();
                }
            });
        }
        0x83 => {
            chrome::calculate_client(hwnd, l);
            return 0;
        }
        0x85 | 0xae | 0xaf => return 0,
        0x86 => {
            // winit needs WM_NCACTIVATE to track active focus. Swallowing it makes
            // egui consider a foreground window unfocused and end every stroke.
            // -1 suppresses DefWindowProc's non-client repaint, preserving chrome.
            return if previous != 0 {
                CallWindowProcW(previous, hwnd, msg, w, -1)
            } else {
                DefWindowProcW(hwnd, msg, w, -1)
            };
        }
        0x0c | 0x80 => return chrome::native_metadata(hwnd, msg, w, l),
        0x24 => {
            // Preserve the window library's configured minimum size, then apply
            // our work-area bounds for a maximized custom frame.
            if previous != 0 {
                CallWindowProcW(previous, hwnd, msg, w, l);
            }
            let minimum = if l != 0 {
                Some(*(l as *const Point).add(3))
            } else {
                None
            };
            chrome::minmax(hwnd, l);
            if let Some(minimum) = minimum {
                let size = (l as *mut Point).add(3);
                (*size).x = (*size).x.max(minimum.x);
                (*size).y = (*size).y.max(minimum.y);
            }
            return 0;
        }
        0x84 => {
            let mut r = Rect::default();
            GetClientRect(hwnd, &mut r);
            let mut p = Point {
                x: l as i16 as i32,
                y: (l >> 16) as i16 as i32,
            };
            ScreenToClient(hwnd, &mut p);
            if IsZoomed(hwnd) == 0 {
                let border = dpi::physical(5, dpi::for_window(hwnd));
                let result = chrome::hit(r.right, r.bottom, p.x, p.y, border, false);
                if (10..=17).contains(&result) {
                    return result;
                }
            }
            return 1;
        }
        0x05 | 0x31a | 0x31e => chrome::shape(hwnd, IsZoomed(hwnd) != 0),
        0x02cc => return 0x1 | 0x8 | 0x10 | 0x100 | 0x10000 | 0x80000 | 0x100000,
        0x245 | 0x246 | 0x247 | 0x249 => {
            let mut info: PenInfo = mem::zeroed();
            let id = (w & 0xffff) as u32;
            if GetPointerPenInfo(id, &mut info) != 0 {
                if info.pointer.flags & 0x8000 != 0 {
                    enqueue(hwnd, info, PenPhase::Cancel);
                } else if msg == 0x245 && info.pointer.flags & 4 != 0 {
                    let mut history: [PenInfo; 64] = mem::zeroed();
                    let mut count = 64;
                    if GetPointerPenInfoHistory(id, &mut count, history.as_mut_ptr()) != 0 {
                        for item in history[..(count as usize).min(64)].iter().rev() {
                            if item.pointer.flags & 4 != 0 {
                                enqueue(hwnd, *item, PenPhase::Move);
                            }
                        }
                    } else {
                        enqueue(hwnd, info, PenPhase::Move);
                    }
                } else {
                    enqueue(
                        hwnd,
                        info,
                        match msg {
                            0x246 => PenPhase::Down,
                            0x247 => PenPhase::Up,
                            _ => PenPhase::Hover,
                        },
                    );
                }
            }
        }
        0x08 | 0x1f | 0x24c => {
            if msg == 0x1f {
                HOOKS.with(|hooks| {
                    if let Some(h) = hooks.borrow_mut().get_mut(&(hwnd as usize)) {
                        if h.sizing {
                            h.move_cancelled = true;
                        }
                    }
                });
            }

            let info: PenInfo = mem::zeroed();
            enqueue(hwnd, info, PenPhase::Cancel);
        }
        0x24a => {
            let info: PenInfo = mem::zeroed();
            enqueue(hwnd, info, PenPhase::Leave);
        }
        0x82 => {
            HOOKS.with(|slot| {
                slot.borrow_mut().remove(&(hwnd as usize));
            });
        }
        _ => {}
    }
    if previous != 0 {
        CallWindowProcW(previous, hwnd, msg, w, l)
    } else {
        DefWindowProcW(hwnd, msg, w, l)
    }
}
pub fn report_error(error: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(error).as_ptr(),
            wide("MiCoPi Failed to Start").as_ptr(),
            0x10,
        );
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumThreadWindows(
        thread: u32,
        callback: unsafe extern "system" fn(Hwnd, isize) -> i32,
        data: isize,
    ) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
}

#[cfg(test)]
mod viewport_input_tests {
    use super::*;
    #[test]
    fn native_move_release_keeps_physical_negative_coordinates_and_respects_cancellation() {
        let mut h = Hook {
            hwnd: null_mut(),
            previous: 0,
            context: egui::Context::default(),
            samples: VecDeque::new(),
            sizing: true,
            moving: false,
            move_cancelled: false,
            group_drop: None,
            drag_offset: Some((50, 20)),
            drag_point: None,
        };
        let rect = Rect {
            left: -1400,
            top: 240,
            right: -900,
            bottom: 900,
        };
        h.observe_group_move(Some(&rect), None);
        assert_eq!(h.drag_point, Some((-1350, 260)));
        h.finish_group_move(None, false);
        assert_eq!(h.group_drop.take(), Some((-1350, 260)));
        h.sizing = true;
        h.drag_offset = Some((50, 20));
        h.observe_group_move(Some(&rect), Some((-1320, 280)));
        h.move_cancelled = true;
        h.finish_group_move(None, false);
        assert!(h.group_drop.is_none());
        h.sizing = true;
        h.moving = false;
        h.move_cancelled = false;
        h.finish_group_move(Some((-1000, 200)), false);
        assert!(h.group_drop.is_none(), "resizing must not dock a group");
        h.sizing = true;
        h.observe_group_move(Some(&rect), Some((-1200, 220)));
        h.finish_group_move(None, true);
        assert!(h.group_drop.is_none());
    }
    #[test]
    fn pen_queues_are_window_local_and_keep_pressure_and_cancel_order() {
        let first = Native {
            hwnd: 0x7fff001usize as Hwnd,
            gdiplus: 0,
        };
        let second = Native {
            hwnd: 0x7fff002usize as Hwnd,
            gdiplus: 0,
        };
        for native in [&first, &second] {
            HOOKS.with(|hooks| {
                hooks.borrow_mut().insert(
                    native.hwnd as usize,
                    Hook {
                        hwnd: native.hwnd,
                        previous: 0,
                        context: egui::Context::default(),
                        samples: VecDeque::new(),
                        sizing: false,
                        moving: false,
                        move_cancelled: false,
                        group_drop: None,
                        drag_offset: None,
                        drag_point: None,
                    },
                );
            });
        }
        unsafe {
            let mut pen: PenInfo = mem::zeroed();
            pen.mask = 1;
            pen.pressure = 256;
            enqueue(first.hwnd, pen, PenPhase::Down);
            pen.pressure = 1024;
            enqueue(second.hwnd, pen, PenPhase::Move);
            enqueue(first.hwnd, pen, PenPhase::Cancel);
        }
        let mut packets = vec![];
        second.drain_pen(&mut packets);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].phase, PenPhase::Move);
        assert_eq!(packets[0].pressure, 1.);
        first.drain_pen(&mut packets);
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].phase, PenPhase::Down);
        assert_eq!(packets[0].pressure, 0.25);
        assert_eq!(packets[1].phase, PenPhase::Cancel);
        drop(first);
        second.drain_pen(&mut packets);
        assert!(packets.is_empty());
        HOOKS.with(|hooks| assert!(hooks.borrow().contains_key(&(second.hwnd as usize))));
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn WindowFromPoint(point: Point) -> Hwnd;
    fn GetAncestor(hwnd: Hwnd, flags: u32) -> Hwnd;
    fn GetWindow(hwnd: Hwnd, command: u32) -> Hwnd;
    fn IsWindowVisible(hwnd: Hwnd) -> i32;
    fn IsIconic(hwnd: Hwnd) -> i32;
    fn GetAsyncKeyState(key: i32) -> i16;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(
        hwnd: Hwnd,
        attribute: u32,
        value: *mut std::ffi::c_void,
        size: u32,
    ) -> i32;
}
