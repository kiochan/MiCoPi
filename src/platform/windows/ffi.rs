#![allow(non_snake_case, dead_code)]
use std::ffi::c_void;
pub type Handle = *mut c_void;
pub type Hwnd = Handle;
pub type Dc = Handle;
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[repr(C)]
pub struct MonitorInfo {
    pub size: u32,
    pub monitor: Rect,
    pub work: Rect,
    pub flags: u32,
}
#[repr(C)]
pub struct Margins {
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub bottom: i32,
}
#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmExtendFrameIntoClientArea(hwnd: Hwnd, margins: *const Margins) -> i32;
    pub fn DwmSetWindowAttribute(
        hwnd: Hwnd,
        attribute: u32,
        value: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
    pub fn DwmGetWindowAttribute(
        hwnd: Hwnd,
        attribute: u32,
        value: *mut std::ffi::c_void,
        size: u32,
    ) -> i32;
    pub fn DwmIsCompositionEnabled(enabled: *mut i32) -> i32;
}
#[repr(C)]
pub struct MenuInfo {
    pub size: u32,
    pub mask: u32,
    pub style: u32,
    pub max_height: u32,
    pub background: Handle,
    pub help: u32,
    pub data: usize,
}
#[repr(C)]
pub struct MenuItemInfo {
    pub size: u32,
    pub mask: u32,
    pub kind: u32,
    pub state: u32,
    pub id: u32,
    pub submenu: Handle,
    pub checked: Handle,
    pub unchecked: Handle,
    pub data: usize,
    pub text: *mut u16,
    pub text_len: u32,
    pub bitmap: Handle,
}
#[repr(C)]
pub struct MeasureItem {
    pub kind: u32,
    pub control: u32,
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub data: usize,
}
#[repr(C)]
pub struct DrawItem {
    pub kind: u32,
    pub control: u32,
    pub id: u32,
    pub action: u32,
    pub state: u32,
    pub window: Hwnd,
    pub dc: Dc,
    pub rect: Rect,
    pub data: usize,
}
#[repr(C)]
pub struct Msg {
    pub hwnd: Hwnd,
    pub message: u32,
    pub wparam: usize,
    pub lparam: isize,
    pub time: u32,
    pub pt: Point,
    pub private: u32,
}
#[repr(C)]
pub struct WndClass {
    pub style: u32,
    pub proc: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    pub cls_extra: i32,
    pub wnd_extra: i32,
    pub instance: Handle,
    pub icon: Handle,
    pub cursor: Handle,
    pub background: Handle,
    pub menu: *const u16,
    pub name: *const u16,
}
#[repr(C)]
pub struct Paint {
    pub dc: Dc,
    pub erase: i32,
    pub rect: Rect,
    pub restore: i32,
    pub update: i32,
    pub reserved: [u8; 32],
}
#[repr(C)]
pub struct BitmapInfo {
    pub size: u32,
    pub width: i32,
    pub height: i32,
    pub planes: u16,
    pub bits: u16,
    pub compression: u32,
    pub image_size: u32,
    pub xppm: i32,
    pub yppm: i32,
    pub used: u32,
    pub important: u32,
    pub colors: [u32; 1],
}
impl BitmapInfo {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            size: 40,
            width: w as i32,
            height: -(h as i32),
            planes: 1,
            bits: 32,
            compression: 0,
            image_size: 0,
            xppm: 0,
            yppm: 0,
            used: 0,
            important: 0,
            colors: [0],
        }
    }
}
#[repr(C)]
pub struct OpenFile {
    pub size: u32,
    pub owner: Hwnd,
    pub instance: Handle,
    pub filter: *const u16,
    pub custom: *mut u16,
    pub max_custom: u32,
    pub filter_index: u32,
    pub file: *mut u16,
    pub max_file: u32,
    pub title_file: *mut u16,
    pub max_title: u32,
    pub initial: *const u16,
    pub title: *const u16,
    pub flags: u32,
    pub offset: u16,
    pub extension: u16,
    pub default_ext: *const u16,
    pub data: isize,
    pub hook: Handle,
    pub template: *const u16,
    pub reserved: Handle,
    pub reserved2: u32,
    pub flags_ex: u32,
}
#[repr(C)]
pub struct ChooseColor {
    pub size: u32,
    pub owner: Hwnd,
    pub instance: Handle,
    pub result: u32,
    pub custom: *mut u32,
    pub flags: u32,
    pub data: isize,
    pub hook: Handle,
    pub template: *const u16,
}
#[repr(C)]
pub struct GdiStartup {
    pub version: u32,
    pub callback: Handle,
    pub suppress_thread: i32,
    pub suppress_codecs: i32,
}
#[repr(C)]
pub struct Guid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}
#[repr(C)]
pub struct BitmapData {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub format: i32,
    pub scan: Handle,
    pub reserved: usize,
}
#[repr(C)]
pub struct GpRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PointerInfo {
    pub kind: u32,
    pub id: u32,
    pub frame: u32,
    pub flags: u32,
    pub device: Handle,
    pub target: Hwnd,
    pub pixel: Point,
    pub himetric: Point,
    pub pixel_raw: Point,
    pub himetric_raw: Point,
    pub time: u32,
    pub history: u32,
    pub input_data: i32,
    pub key_states: u32,
    pub performance: u64,
    pub button_change: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PenInfo {
    pub pointer: PointerInfo,
    pub flags: u32,
    pub mask: u32,
    pub pressure: u32,
    pub rotation: u32,
    pub tilt_x: i32,
    pub tilt_y: i32,
}
#[repr(C)]
pub struct MouseTrack {
    pub size: u32,
    pub flags: u32,
    pub hwnd: Hwnd,
    pub hover_time: u32,
}
#[link(name = "user32")]
extern "system" {
    pub fn SetMenuInfo(menu: Handle, info: *const MenuInfo) -> i32;
    pub fn GetMenuInfo(menu: Handle, info: *mut MenuInfo) -> i32;
    pub fn InsertMenuItemW(
        menu: Handle,
        item: u32,
        by_position: i32,
        info: *const MenuItemInfo,
    ) -> i32;
    pub fn GetMenuItemInfoW(
        menu: Handle,
        item: u32,
        by_position: i32,
        info: *mut MenuItemInfo,
    ) -> i32;
    pub fn IsZoomed(hwnd: Hwnd) -> i32;
    pub fn MonitorFromWindow(hwnd: Hwnd, flags: u32) -> Handle;
    pub fn MonitorFromRect(rect: *const Rect, flags: u32) -> Handle;
    pub fn GetMonitorInfoW(monitor: Handle, info: *mut MonitorInfo) -> i32;
    pub fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    pub fn TrackMouseEvent(track: *mut MouseTrack) -> i32;
    pub fn GetCursorPos(point: *mut Point) -> i32;
    pub fn GetPointerPenInfo(id: u32, info: *mut PenInfo) -> i32;
    pub fn GetPointerPenInfoHistory(id: u32, count: *mut u32, info: *mut PenInfo) -> i32;
    pub fn GetMessageExtraInfo() -> isize;
    pub fn GetMessageTime() -> i32;
    pub fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> i32;
    pub fn CreatePopupMenu() -> Handle;
    pub fn AppendMenuW(menu: Handle, flags: u32, id: usize, text: *const u16) -> i32;
    pub fn TrackPopupMenu(
        menu: Handle,
        flags: u32,
        x: i32,
        y: i32,
        reserved: i32,
        hwnd: Hwnd,
        rect: *const Rect,
    ) -> i32;
    pub fn DestroyMenu(menu: Handle) -> i32;
    pub fn SetProcessDPIAware() -> i32;
    pub fn SetWindowPos(hwnd: Hwnd, after: Hwnd, x: i32, y: i32, w: i32, h: i32, flags: u32)
        -> i32;
    pub fn RegisterClassW(class: *const WndClass) -> u16;
    pub fn UnregisterClassW(class: *const u16, instance: Handle) -> i32;
    pub fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Hwnd,
        menu: Handle,
        instance: Handle,
        param: Handle,
    ) -> Hwnd;
    pub fn DefWindowProcW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize;
    pub fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
    pub fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    pub fn TranslateMessage(msg: *const Msg) -> i32;
    pub fn DispatchMessageW(msg: *const Msg) -> isize;
    pub fn PostQuitMessage(code: i32);
    pub fn DestroyWindow(hwnd: Hwnd) -> i32;
    pub fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
    pub fn InvalidateRect(hwnd: Hwnd, rect: *const Rect, erase: i32) -> i32;
    pub fn BeginPaint(hwnd: Hwnd, paint: *mut Paint) -> Dc;
    pub fn EndPaint(hwnd: Hwnd, paint: *const Paint) -> i32;
    pub fn FillRect(dc: Dc, rect: *const Rect, brush: Handle) -> i32;
    pub fn DrawTextW(dc: Dc, text: *const u16, len: i32, rect: *mut Rect, format: u32) -> i32;
    pub fn LoadCursorW(instance: Handle, name: *const u16) -> Handle;
    pub fn SetCursor(cursor: Handle) -> Handle;
    pub fn SetCapture(hwnd: Hwnd) -> Hwnd;
    pub fn ReleaseCapture() -> i32;
    pub fn GetKeyState(key: i32) -> i16;
    pub fn ScreenToClient(hwnd: Hwnd, point: *mut Point) -> i32;
    pub fn MessageBoxW(hwnd: Hwnd, text: *const u16, title: *const u16, flags: u32) -> i32;
    pub fn SetWindowTextW(hwnd: Hwnd, text: *const u16) -> i32;
    pub fn GetDC(hwnd: Hwnd) -> Dc;
    pub fn ReleaseDC(hwnd: Hwnd, dc: Dc) -> i32;
    pub fn PostMessageW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> i32;
}
#[link(name = "gdi32")]
extern "system" {
    pub fn SaveDC(dc: Dc) -> i32;
    pub fn AddFontMemResourceEx(
        data: *const std::ffi::c_void,
        size: u32,
        reserved: Handle,
        count: *mut u32,
    ) -> Handle;
    pub fn RemoveFontMemResourceEx(resource: Handle) -> i32;
    pub fn GetGlyphIndicesW(
        dc: Dc,
        text: *const u16,
        count: i32,
        glyphs: *mut u16,
        flags: u32,
    ) -> u32;
    pub fn SetMapMode(dc: Dc, mode: i32) -> i32;
    pub fn SetWindowExtEx(dc: Dc, x: i32, y: i32, old: *mut Point) -> i32;
    pub fn SetViewportExtEx(dc: Dc, x: i32, y: i32, old: *mut Point) -> i32;
    pub fn CreatePen(style: i32, width: i32, color: u32) -> Handle;
    pub fn GetStockObject(index: i32) -> Handle;
    pub fn Ellipse(dc: Dc, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn RoundRect(
        dc: Dc,
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        ellipse_w: i32,
        ellipse_h: i32,
    ) -> i32;
    pub fn MoveToEx(dc: Dc, x: i32, y: i32, old: *mut Point) -> i32;
    pub fn LineTo(dc: Dc, x: i32, y: i32) -> i32;
    pub fn GetDIBits(
        dc: Dc,
        bitmap: Handle,
        start: u32,
        lines: u32,
        bits: *mut c_void,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
    pub fn RestoreDC(dc: Dc, saved: i32) -> i32;
    pub fn SetViewportOrgEx(dc: Dc, x: i32, y: i32, old: *mut Point) -> i32;
    pub fn IntersectClipRect(dc: Dc, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    pub fn CreateSolidBrush(color: u32) -> Handle;
    pub fn DeleteObject(obj: Handle) -> i32;
    pub fn CreateCompatibleDC(dc: Dc) -> Dc;
    pub fn DeleteDC(dc: Dc) -> i32;
    pub fn CreateCompatibleBitmap(dc: Dc, w: i32, h: i32) -> Handle;
    pub fn SelectObject(dc: Dc, obj: Handle) -> Handle;
    pub fn SetBkMode(dc: Dc, mode: i32) -> i32;
    pub fn SetTextColor(dc: Dc, color: u32) -> u32;
    pub fn CreateFontW(
        height: i32,
        width: i32,
        escape: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Handle;
    pub fn StretchDIBits(
        dc: Dc,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        bits: *const c_void,
        info: *const BitmapInfo,
        usage: u32,
        rop: u32,
    ) -> i32;
    pub fn SetStretchBltMode(dc: Dc, mode: i32) -> i32;
    pub fn BitBlt(
        dst: Dc,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        src: Dc,
        sx: i32,
        sy: i32,
        rop: u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(name: *const u16) -> Handle;
    pub fn GetProcAddress(module: Handle, name: *const u8) -> Handle;
}
#[link(name = "comdlg32")]
extern "system" {
    pub fn GetOpenFileNameW(info: *mut OpenFile) -> i32;
    pub fn GetSaveFileNameW(info: *mut OpenFile) -> i32;
    pub fn ChooseColorW(info: *mut ChooseColor) -> i32;
}
#[link(name = "gdiplus")]
extern "system" {
    pub fn GdiplusStartup(token: *mut usize, input: *const GdiStartup, output: Handle) -> i32;
    pub fn GdiplusShutdown(token: usize);
    pub fn GdipCreateBitmapFromScan0(
        w: i32,
        h: i32,
        stride: i32,
        format: i32,
        scan: *mut u8,
        bitmap: *mut Handle,
    ) -> i32;
    pub fn GdipSaveImageToFile(
        image: Handle,
        path: *const u16,
        clsid: *const Guid,
        params: Handle,
    ) -> i32;
    pub fn GdipDisposeImage(image: Handle) -> i32;
    pub fn GdipCreateBitmapFromFile(path: *const u16, image: *mut Handle) -> i32;
    pub fn GdipGetImageWidth(image: Handle, w: *mut u32) -> i32;
    pub fn GdipGetImageHeight(image: Handle, h: *mut u32) -> i32;
    pub fn GdipBitmapLockBits(
        image: Handle,
        rect: *const GpRect,
        flags: u32,
        format: i32,
        data: *mut BitmapData,
    ) -> i32;
    pub fn GdipBitmapUnlockBits(image: Handle, data: *mut BitmapData) -> i32;
}

#[link(name = "user32")]
extern "system" {
    pub fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
    pub fn GetWindowTextW(hwnd: Hwnd, text: *mut u16, count: i32) -> i32;
    pub fn SetFocus(hwnd: Hwnd) -> Hwnd;
    pub fn GetParent(hwnd: Hwnd) -> Hwnd;
    pub fn SendMessageW(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize;
    pub fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, value: isize) -> isize;
    pub fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
    pub fn CallWindowProcW(proc: isize, hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize;
}
