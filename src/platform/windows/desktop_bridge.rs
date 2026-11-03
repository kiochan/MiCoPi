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
                wide(crate::i18n::text(
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
                wide(crate::i18n::text(
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
