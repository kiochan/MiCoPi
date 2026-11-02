//! DPI conversion stays at the native boundary; editor geometry is in logical pixels.
use super::*;
pub(super) unsafe fn enable() {
    let module = GetModuleHandleW(wide("user32.dll").as_ptr());
    let proc = GetProcAddress(module, b"SetProcessDpiAwarenessContext\0".as_ptr());
    if !proc.is_null() {
        let set: unsafe extern "system" fn(Handle) -> i32 = mem::transmute(proc);
        if set(-4isize as Handle) != 0 {
            return;
        }
    }
    SetProcessDPIAware();
}
pub(super) unsafe fn for_window(hwnd: Hwnd) -> u32 {
    let module = GetModuleHandleW(wide("user32.dll").as_ptr());
    let proc = GetProcAddress(module, b"GetDpiForWindow\0".as_ptr());
    if !proc.is_null() {
        let get: unsafe extern "system" fn(Hwnd) -> u32 = mem::transmute(proc);
        let dpi = get(hwnd);
        if dpi > 0 {
            return dpi;
        }
    }
    96
}
pub(super) fn logical(value: i32, dpi: u32) -> i32 {
    (value as f64 * 96. / dpi.max(1) as f64).floor() as i32
}
pub(super) fn physical(value: i32, dpi: u32) -> i32 {
    (value as f64 * dpi as f64 / 96.).round() as i32
}
pub(super) fn extent(value: i32, dpi: u32) -> i32 {
    (value as f64 * dpi as f64 / 96.).ceil() as i32
}
pub(super) unsafe fn map(dc: Dc, dpi: u32) {
    SetMapMode(dc, 8);
    SetWindowExtEx(dc, 96, 96, null_mut());
    SetViewportExtEx(dc, dpi as i32, dpi as i32, null_mut());
}
pub(super) fn device_rect(r: Rect, dpi: u32) -> Rect {
    let s = dpi as f64 / 96.;
    Rect {
        left: (r.left as f64 * s).floor() as i32,
        top: (r.top as f64 * s).floor() as i32,
        right: (r.right as f64 * s).ceil() as i32,
        bottom: (r.bottom as f64 * s).ceil() as i32,
    }
}
