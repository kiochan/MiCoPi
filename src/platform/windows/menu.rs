//! Theme-owned menu rendering; Win32 retains navigation and nested popup handling.
use super::*;
use crate::ui::{
    keymap::{Keymap, COMMANDS},
    Theme,
};
use std::rc::Rc;

pub(super) struct Popup {
    pub root: Handle,
    icons: Rc<icons::Icons>,
    font: Handle,
    brush: Handle,
    theme: Theme,
    dpi: u32,
    // Stable addresses: Windows retains itemData until the popup is destroyed.
    items: Vec<Box<Item>>,
}
struct Item {
    icons: Rc<icons::Icons>,
    id: u32,
    title: String,
    shortcut: String,
    font: Handle,
    theme: Theme,
    dpi: u32,
    width: u32,
    height: u32,
    submenu: bool,
}
impl Popup {
    pub unsafe fn new(theme: Theme, dpi: u32) -> Self {
        let font = CreateFontW(
            -dpi::physical(14, dpi),
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
        let mut popup = Self {
            root: CreatePopupMenu(),
            icons: Rc::new(icons::Icons::new(dpi)),
            font,
            brush: CreateSolidBrush(color(theme.panel)),
            theme,
            dpi,
            items: vec![],
        };
        popup.style(popup.root);
        popup
    }
    unsafe fn style(&mut self, menu: Handle) {
        SetMenuInfo(
            menu,
            &MenuInfo {
                size: mem::size_of::<MenuInfo>() as u32,
                mask: 0x2 | 0x10,
                style: 0x80000000,
                max_height: 0,
                background: self.brush,
                help: 0,
                data: 0,
            },
        );
    }
    pub unsafe fn submenu(&mut self, parent: Handle, title: &str) -> Handle {
        let child = CreatePopupMenu();
        self.style(child);
        self.append(parent, 0, title, String::new(), false, child);
        child
    }
    pub unsafe fn item(
        &mut self,
        parent: Handle,
        id: u32,
        label: &str,
        checked: bool,
        map: &Keymap,
    ) {
        let (title, shortcut) = COMMANDS
            .iter()
            .position(|c| c.ui == id)
            .map(|i| (COMMANDS[i].title, map.label(i)))
            .unwrap_or((label, String::new()));
        self.append(parent, id, title, shortcut, checked, null_mut());
    }
    unsafe fn append(
        &mut self,
        parent: Handle,
        id: u32,
        title: &str,
        shortcut: String,
        checked: bool,
        submenu: Handle,
    ) {
        let dc = GetDC(null_mut());
        let saved = SaveDC(dc);
        SelectObject(dc, self.font);
        let mut label = Rect::default();
        let mut keys = Rect::default();
        DrawTextW(
            dc,
            wide(title).as_ptr(),
            -1,
            &mut label,
            0x400 | 0x20 | 0x800,
        );
        keys.right = self.icons.caps_width(dc, &shortcut, self.font, self.dpi);
        RestoreDC(dc, saved);
        ReleaseDC(null_mut(), dc);
        let width = (label.right
            + keys.right
            + dpi::physical(if shortcut.is_empty() { 64 } else { 96 }, self.dpi))
        .max(dpi::physical(224, self.dpi));
        let item = Box::new(Item {
            icons: Rc::clone(&self.icons),
            id,
            title: title.into(),
            shortcut,
            font: self.font,
            theme: self.theme,
            dpi: self.dpi,
            width: width as u32,
            height: dpi::physical(32, self.dpi) as u32,
            submenu: !submenu.is_null(),
        });
        let text = wide(&format!(
            "{}{}{}",
            item.title,
            if item.shortcut.is_empty() { "" } else { "\t" },
            item.shortcut
        ));
        let info = MenuItemInfo {
            size: mem::size_of::<MenuItemInfo>() as u32,
            mask: 0x100 | 0x40 | 0x20 | 0x4 | 0x2 | 0x1,
            kind: 0x100,
            state: if checked { 8 } else { 0 },
            id,
            submenu,
            checked: null_mut(),
            unchecked: null_mut(),
            data: &*item as *const Item as usize,
            text: text.as_ptr() as *mut u16,
            text_len: 0,
            bitmap: null_mut(),
        };
        InsertMenuItemW(parent, u32::MAX, 1, &info);
        self.items.push(item);
    }
    pub unsafe fn track(&self, hwnd: Hwnd, point: Point) -> u32 {
        TrackPopupMenu(
            self.root,
            0x100 | 0x80 | 2,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        ) as u32
    }
}
impl Drop for Popup {
    fn drop(&mut self) {
        unsafe {
            DestroyMenu(self.root);
            DeleteObject(self.font);
            DeleteObject(self.brush);
        }
    }
}
// These notifications arrive synchronously inside TrackPopupMenu while App is borrowed.
// Item-owned rendering data avoids borrowing App again (including for submenus).
pub(super) unsafe fn message(msg: u32, l: isize) -> Option<isize> {
    if l == 0 {
        return None;
    }
    match msg {
        0x2c => {
            let m = &mut *(l as *mut MeasureItem);
            if m.kind != 1 || m.data == 0 {
                return None;
            }
            let item = &*(m.data as *const Item);
            m.width = item.width;
            m.height = item.height;
            Some(1)
        }
        0x2b => {
            let d = &*(l as *const DrawItem);
            if d.kind != 1 || d.data == 0 {
                return None;
            }
            draw(d, &*(d.data as *const Item));
            Some(1)
        }
        _ => None,
    }
}
unsafe fn draw(d: &DrawItem, item: &Item) {
    let dc = d.dc;
    let saved = SaveDC(dc);
    let t = item.theme;
    let scale = |n| dpi::physical(n, item.dpi);
    let r = d.rect;
    App::fill(dc, r, t.panel);
    let selected = d.state & 1 != 0;
    let disabled = d.state & (2 | 4) != 0;
    if selected && !disabled {
        let inset = scale(3);
        let brush = CreateSolidBrush(color(t.selected));
        SelectObject(dc, brush);
        SelectObject(dc, GetStockObject(8));
        RoundRect(
            dc,
            r.left + inset,
            r.top + scale(2),
            r.right - inset,
            r.bottom - scale(2),
            scale(8),
            scale(8),
        );
        SelectObject(dc, GetStockObject(0));
        DeleteObject(brush);
    }
    let ink = if disabled { t.muted } else { t.text };
    let middle = (r.top + r.bottom) / 2;
    if d.state & 8 != 0 {
        let pen = CreatePen(
            0,
            scale(2).max(1),
            color(if disabled { t.muted } else { t.accent }),
        );
        let old = SelectObject(dc, pen);
        MoveToEx(dc, r.left + scale(11), middle, null_mut());
        LineTo(dc, r.left + scale(14), middle + scale(3));
        LineTo(dc, r.left + scale(20), middle - scale(4));
        SelectObject(dc, old);
        DeleteObject(pen);
    }
    if d.state & 8 == 0 {
        if let Some(icon) = icons::command(item.id) {
            item.icons.draw(
                dc,
                rect(r.left + scale(6), r.top, scale(22), r.bottom - r.top),
                icon,
                16,
                ink,
            );
        }
    }
    SelectObject(dc, item.font);
    SetBkMode(dc, 1);
    SetTextColor(dc, color(ink));
    let mut title = Rect {
        left: r.left + scale(30),
        top: r.top,
        right: r.right - scale(26),
        bottom: r.bottom,
    };
    let extent = item
        .icons
        .caps_width(dc, &item.shortcut, item.font, item.dpi);
    if !item.shortcut.is_empty() {
        title.right = r.right - scale(28) - extent - scale(24);
    }
    DrawTextW(
        dc,
        wide(&item.title).as_ptr(),
        -1,
        &mut title,
        0x24 | 0x800 | 0x8000,
    );
    let keys = Rect {
        left: title.right + scale(12),
        top: r.top,
        right: r.right - scale(28),
        bottom: r.bottom,
    };
    item.icons
        .keycaps(dc, keys, &item.shortcut, item.font, t, item.dpi);
    if item.submenu {
        let pen = CreatePen(0, scale(1).max(1), color(ink));
        let old = SelectObject(dc, pen);
        let x = r.right - scale(16);
        MoveToEx(dc, x - scale(2), middle - scale(3), null_mut());
        LineTo(dc, x + scale(1), middle);
        LineTo(dc, x - scale(2), middle + scale(3));
        SelectObject(dc, old);
        DeleteObject(pen);
    }
    RestoreDC(dc, saved);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_menu_metadata_and_callbacks_survive_the_app_borrow() {
        unsafe {
            assert_eq!(mem::size_of::<MenuInfo>(), 40);
            assert_eq!(mem::size_of::<MenuItemInfo>(), 80);
            assert_eq!(mem::size_of::<DrawItem>(), 64);
            assert_eq!(mem::size_of::<MeasureItem>(), 32);
            let mut popup = Popup::new(Theme::preset(2), 144);
            let root = popup.root;
            let child = popup.submenu(root, "UI Theme");
            let map = Keymap::decode(r#"{"keybindings":{"file.save":["Ctrl+F12"]}}"#).unwrap();
            popup.item(child, 3, "Save", true, &map);
            let mut info: MenuItemInfo = mem::zeroed();
            info.size = mem::size_of::<MenuItemInfo>() as u32;
            info.mask = 0x100 | 0x20 | 0x1 | 0x4;
            assert_ne!(GetMenuItemInfoW(root, 0, 1, &mut info), 0);
            assert_eq!(info.submenu, child);
            assert_ne!(info.kind & 0x100, 0);
            assert_ne!(GetMenuItemInfoW(child, 0, 1, &mut info), 0);
            assert_ne!(info.state & 8, 0);
            let item = &*(info.data as *const Item);
            assert_eq!(item.shortcut, "Ctrl+F12");
            let mut style: MenuInfo = mem::zeroed();
            style.size = mem::size_of::<MenuInfo>() as u32;
            style.mask = 0x2 | 0x10;
            assert_ne!(GetMenuInfo(child, &mut style), 0);
            assert_eq!(style.background, popup.brush);
            assert_ne!(style.style & 0x80000000, 0);
            APP.with(|state| {
                let _borrow = state.borrow_mut();
                let mut measure = MeasureItem {
                    kind: 1,
                    control: 0,
                    id: 3,
                    width: 0,
                    height: 0,
                    data: info.data,
                };
                assert_eq!(
                    window_proc(null_mut(), 0x2c, 0, &mut measure as *mut _ as isize),
                    1
                );
                assert!(measure.width >= 336);
                assert_eq!(measure.height, 48);
                let dc = GetDC(null_mut());
                let surface = Backbuffer::new(dc, measure.width as i32, measure.height as i32);
                ReleaseDC(null_mut(), dc);
                let draw = DrawItem {
                    kind: 1,
                    control: 0,
                    id: 3,
                    action: 1,
                    state: 9,
                    window: child,
                    dc: surface.dc,
                    rect: rect(0, 0, measure.width as i32, measure.height as i32),
                    data: info.data,
                };
                assert_eq!(
                    window_proc(null_mut(), 0x2b, 0, &draw as *const _ as isize),
                    1
                );
            });
        }
    }
    #[test]
    fn menu_rows_follow_all_themes_and_dpi_with_readable_states() {
        unsafe {
            let mut token = 0;
            assert_eq!(
                GdiplusStartup(
                    &mut token,
                    &GdiStartup {
                        version: 1,
                        callback: null_mut(),
                        suppress_thread: 0,
                        suppress_codecs: 0
                    },
                    null_mut()
                ),
                0
            );
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
            std::fs::create_dir_all(&root).unwrap();
            for (theme, dpi) in [(0, 96), (1, 96), (2, 96), (2, 144)] {
                let t = Theme::preset(theme);
                let mut popup = Popup::new(t, dpi);
                let menu = popup.root;
                let map = Keymap::default();
                popup.item(menu, 1, "New", false, &map);
                popup.item(menu, 3, "Save", false, &map);
                popup.item(menu, 65, "Graphite", true, &map);
                popup.submenu(menu, "Panels and Layout");
                popup.item(menu, 4, "Export", false, &map);
                let width = popup.items.iter().map(|i| i.width).max().unwrap() as i32;
                let row = popup.items[0].height as i32;
                let height = row * 5;
                let dc = GetDC(null_mut());
                let surface = Backbuffer::new(dc, width, height);
                ReleaseDC(null_mut(), dc);
                let saved = SaveDC(surface.dc);
                for (i, item) in popup.items.iter().enumerate() {
                    let d = DrawItem {
                        kind: 1,
                        control: 0,
                        id: i as u32,
                        action: 1,
                        state: match i {
                            1 => 1,
                            2 => 8,
                            4 => 4,
                            _ => 0,
                        },
                        window: menu,
                        dc: surface.dc,
                        rect: rect(0, i as i32 * row, width, row),
                        data: &**item as *const Item as usize,
                    };
                    assert_eq!(message(0x2b, &d as *const _ as isize), Some(1));
                }
                assert_eq!(
                    SaveDC(surface.dc),
                    saved + 1,
                    "menu drawing must restore the caller's DC state"
                );
                RestoreDC(surface.dc, saved);
                let mut pixels = vec![0u32; width as usize * height as usize];
                SelectObject(surface.dc, surface.old);
                assert_eq!(
                    GetDIBits(
                        surface.dc,
                        surface.bitmap,
                        0,
                        height as u32,
                        pixels.as_mut_ptr() as _,
                        &mut BitmapInfo::new(width as usize, height as usize),
                        0
                    ),
                    height
                );
                SelectObject(surface.dc, surface.bitmap);
                let at = |x: i32, y: i32| pixels[(y * width + x) as usize] & 0xffffff;
                assert_eq!(at(1, row / 2), t.panel);
                assert_eq!(at(width - 8, row + row / 2), t.selected);
                assert_eq!(at(1, 2 * row + row / 2), t.panel);
                assert!(pixels
                    [2 * row as usize * width as usize..3 * row as usize * width as usize]
                    .iter()
                    .any(|p| p & 0xffffff == t.accent));
                for pixel in &mut pixels {
                    *pixel |= 0xff000000;
                }
                save_png(
                    &root.join(format!("menu-theme-{}-dpi{}.png", theme, dpi)),
                    &mut pixels,
                    width as usize,
                    height as usize,
                )
                .unwrap();
            }
            GdiplusShutdown(token);
        }
    }
}
