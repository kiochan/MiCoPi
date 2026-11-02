use super::*;
#[test]
fn four_edge_panels_render_and_resize_horizontal_rows_at_multiple_dpi() {
    unsafe {
        use crate::ui::docking::{Panel, Place, Target};
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.workspace.enable_docking();
        app.keymap = Default::default();
        let preview = Box2 {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        };
        for (p, place) in
            Panel::ALL
                .iter()
                .zip([Place::Left, Place::Right, Place::Top, Place::Bottom])
        {
            app.workspace.docking.move_panel(
                *p,
                Target::Dock {
                    place,
                    index: 0,
                    preview,
                },
            );
        }
        let before = app.dock_layout().top;
        let x = before.x + before.w / 2;
        let y = before.y + before.h;
        assert_eq!(app.cursor_at(x, y, false, false, false), 32645);
        app.down(null_mut(), x, y, false);
        app.motion(null_mut(), x, y + 30);
        assert!(app.dock_layout().top.h > before.h);
        app.cancel_input(null_mut());
        assert_eq!(app.dock_layout().top.h, before.h);
        app.workspace.docking.move_panel(
            Panel::Brush,
            Target::Dock {
                place: Place::Top,
                index: 0,
                preview,
            },
        );
        let l = app.dock_layout();
        let first = l
            .regions
            .iter()
            .filter(|r| app.workspace.docking.panels[r.panel.index()].place == Place::Top)
            .min_by_key(|r| r.bounds.x)
            .unwrap();
        let b = first.bounds;
        let panel = first.panel;
        let point = (b.x + b.w + 3, b.y + 80);
        assert_eq!(app.cursor_at(point.0, point.1, false, false, false), 32644);
        app.down(null_mut(), point.0, point.1, false);
        app.motion(null_mut(), point.0 + 25, point.1);
        assert!(app.panel_box(panel).w > b.w);
        app.cancel_input(null_mut());
        app.workspace.docking.move_panel(
            Panel::Brush,
            Target::Dock {
                place: Place::Right,
                index: 0,
                preview,
            },
        );
        for (dpi, width, height) in [(96, 1280, 900), (144, 1350, 900)] {
            app.resize(width, height, dpi);
            let dc = GetDC(null_mut());
            let surface = Backbuffer::new(dc, width, height);
            ReleaseDC(null_mut(), dc);
            app.render_scene(surface.dc);
            for hit in &app.hits {
                assert!(
                    hit.r.left >= 0
                        && hit.r.right <= app.size.0
                        && hit.r.top >= 0
                        && hit.r.bottom <= app.size.1,
                    "{}",
                    hit.id
                );
            }
            assert!(app.hits.iter().any(|h| h.id == 4002));
            assert!(app.hits.iter().any(|h| h.id == 4003));
            assert!(app.view().top > TOP + DOCUMENT_TABS);
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
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            save_png(
                &root.join(format!("panels-four-sides-dpi{}.png", dpi)),
                &mut pixels,
                width as usize,
                height as usize,
            )
            .unwrap();
        }
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn panel_tabs_switch_persist_detach_and_move_groups_without_painting_canvas() {
    unsafe {
        use crate::ui::docking::{Panel, Target};
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
        let path = root.join(format!("panel-tabs-ui-{}.ini", std::process::id()));
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.workspace.enable_docking();
        app.workspace_path = path.clone();
        app.keymap = Default::default();
        app.doc.begin(
            300.,
            250.,
            Brush {
                radius: 12.,
                color: 0x58b7de,
                ..app.brush()
            },
        );
        app.doc.line(600., 600., app.brush());
        app.doc.finish();
        let original = app.doc.layers[0].pixels.clone();
        app.command(null_mut(), 10122);
        app.command(null_mut(), 10123);
        assert_eq!(
            app.workspace.docking.root(Panel::Color),
            app.workspace.docking.root(Panel::Brush)
        );
        assert_eq!(
            app.workspace.docking.root(Panel::Navigator),
            app.workspace.docking.root(Panel::Layers)
        );
        let dc = GetDC(null_mut());
        let surface = Backbuffer::new(dc, 1280, 900);
        ReleaseDC(null_mut(), dc);
        app.fit();
        app.render_scene(surface.dc);
        assert!(!app.hits.iter().any(|h| h.id == 800));
        assert!(app.hits.iter().any(|h| h.id == 20));
        assert!(!app.hits.iter().any(|h| h.id == 810));
        assert!(app.hits.iter().any(|h| h.id == 805));
        app.command(null_mut(), 10110);
        app.render_scene(surface.dc);
        assert!(app.hits.iter().any(|h| h.id == 800));
        app.command(null_mut(), 10111);
        app.render_scene(surface.dc);
        assert!(app.hits.iter().any(|h| h.id == 20));
        let click_tab = |app: &mut App, panel: Panel| {
            let region = *app
                .dock_layout()
                .regions
                .iter()
                .find(|r| r.contains_panel(panel))
                .unwrap();
            let b = region.tab_bounds(panel).unwrap();
            app.down(null_mut(), b.x + 20, b.y + 12, false);
            app.up(null_mut());
        };
        click_tab(&mut app, Panel::Color);
        app.render_scene(surface.dc);
        assert!(app.hits.iter().any(|h| h.id == 800));
        assert!(!app.hits.iter().any(|h| h.id == 20));
        let saved = Workspace::load(&path);
        assert_eq!(
            saved.docking.active[saved.docking.root(Panel::Color)],
            Panel::Color
        );
        click_tab(&mut app, Panel::Brush);
        app.render_scene(surface.dc);
        for name in ["docked", "floating"] {
            if name == "floating" {
                app.workspace.docking.move_group(
                    Panel::Layers,
                    Target::Float(Box2 {
                        x: 540,
                        y: 155,
                        w: 330,
                        h: 390,
                    }),
                );
                app.fit();
                app.render_scene(surface.dc);
                let region = *app
                    .dock_layout()
                    .regions
                    .iter()
                    .find(|r| r.contains_panel(Panel::Layers))
                    .unwrap();
                let b = region.bounds;
                assert!(region.content().y > b.y);
                assert_eq!(app.cursor_at(b.x, b.y + 80, false, false, false), 32644);
                assert_eq!(app.cursor_at(b.x + 80, b.y, false, false, false), 32645);
                app.down(null_mut(), b.x + 1, b.y + 1, false);
                app.motion(null_mut(), b.x - 20, b.y - 20);
                assert!(app.panel_box(Panel::Layers).w > region.content().w);
                assert_eq!(
                    app.workspace.docking.panels[Panel::Navigator.index()]
                        .floating
                        .w,
                    app.workspace.docking.panels[Panel::Layers.index()]
                        .floating
                        .w
                );
                app.motion(null_mut(), b.x - 4000, b.y - 4000);
                let resized = app.dock_layout().panel(Panel::Layers).unwrap();
                assert_eq!(resized.x + resized.w, b.x + b.w);
                assert_eq!(resized.y + resized.h, b.y + b.h);
                app.cancel_input(null_mut());
                app.down(null_mut(), b.x + 170, b.y + 12, false);
                app.motion(null_mut(), 850, 640);
                app.cancel_input(null_mut());
                assert_eq!(
                    app.workspace.docking.root(Panel::Layers),
                    app.workspace.docking.root(Panel::Navigator)
                );
            }
            app.render_scene(surface.dc);
            let mut pixels = vec![0u32; 1280 * 900];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    900,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(1280, 900),
                    0
                ),
                900
            );
            SelectObject(surface.dc, surface.bitmap);
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            save_png(
                &root.join(format!("panel-tabs-{}.png", name)),
                &mut pixels,
                1280,
                900,
            )
            .unwrap();
        }
        let region = *app
            .dock_layout()
            .regions
            .iter()
            .find(|r| r.contains_panel(Panel::Navigator))
            .unwrap();
        let b = region.tab_bounds(Panel::Navigator).unwrap();
        app.down(null_mut(), b.x + 20, b.y + 12, false);
        app.motion(null_mut(), 960, 570);
        app.up(null_mut());
        assert_ne!(
            app.workspace.docking.root(Panel::Layers),
            app.workspace.docking.root(Panel::Navigator)
        );
        assert_eq!(
            app.workspace.docking.panels[Panel::Navigator.index()].place,
            crate::ui::docking::Place::Floating
        );
        assert_eq!(app.doc.layers[0].pixels, original);
        assert!(!app.drawing);
        // Group tab positions continue to match logical pointer coordinates at 150% DPI.
        app.resize(1920, 1350, 144);
        let dc = GetDC(null_mut());
        let high = Backbuffer::new(dc, 1920, 1350);
        ReleaseDC(null_mut(), dc);
        app.render_scene(high.dc);
        let r = *app
            .dock_layout()
            .regions
            .iter()
            .find(|r| r.contains_panel(Panel::Brush))
            .unwrap();
        let b = r.tab_bounds(Panel::Brush).unwrap();
        let physical = Point {
            x: dpi::physical(b.x + 20, 144),
            y: dpi::physical(b.y + 12, 144),
        };
        let logical = app.logical_point(physical);
        assert_eq!(r.tab_at(logical.x, logical.y), Some(Panel::Brush));
        std::fs::remove_file(path).unwrap();
        drop(high);
        drop(surface);
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn docking_float_render_controls_resize_and_cancel_keep_ink_untouched() {
    unsafe {
        use crate::ui::docking::{Panel, Place, Target};
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.workspace.enable_docking();
        app.keymap = Default::default();
        app.fit();
        let dc = GetDC(null_mut());
        let surface = Backbuffer::new(dc, 1280, 900);
        ReleaseDC(null_mut(), dc);
        let color = app.panel_box(Panel::Color);
        let edge = color.x + color.w - 2;
        assert_eq!(
            app.cursor_at(edge, color.y + 80, false, false, false),
            32644
        );
        app.down(null_mut(), edge, color.y + 80, false);
        app.motion(null_mut(), edge + 35, color.y + 80);
        assert!(app.workspace.docking.left_width > 224);
        app.cancel_input(null_mut());
        let color = app.panel_box(Panel::Color);
        let split = (color.x + 80, color.y + color.h + 3);
        assert_eq!(app.cursor_at(split.0, split.1, false, false, false), 32645);
        app.down(null_mut(), split.0, split.1, false);
        app.motion(null_mut(), split.0, split.1 + 30);
        assert!(app.panel_box(Panel::Color).h > color.h);
        app.cancel_input(null_mut());
        assert_eq!(app.panel_box(Panel::Color).h, color.h);
        for name in ["docked", "floating", "reordered"] {
            if name == "floating" {
                app.workspace.docking.move_panel(
                    Panel::Navigator,
                    Target::Float(Box2 {
                        x: 500,
                        y: 150,
                        w: 300,
                        h: 270,
                    }),
                );
            }
            if name == "reordered" {
                app.workspace.docking.move_panel(
                    Panel::Layers,
                    Target::Dock {
                        place: Place::Left,
                        index: 0,
                        preview: Box2 {
                            x: 0,
                            y: 0,
                            w: 1,
                            h: 1,
                        },
                    },
                );
            }
            app.fit();
            app.render_scene(surface.dc);
            for p in Panel::ALL {
                let b = app.panel_box(p);
                let header = app
                    .hits
                    .iter()
                    .find(|h| h.id == 4000 + p.index() as u32)
                    .unwrap();
                assert_eq!((header.r.left, header.r.top), (b.x, b.y));
            }
            let brush = app.panel_box(Panel::Brush);
            let slider = app.hits.iter().find(|h| h.id == 20).map(|h| h.r);
            if let Some(slider) = slider {
                app.adjust(20, (slider.left + slider.right) / 2);
                assert!(app.radius > 40. && app.radius < 60.);
                assert!(brush.contains(slider.left, slider.top));
            }
            let layer = app.panel_box(Panel::Layers);
            app.adjust_control(805, layer.x + 65, 0);
            assert_eq!(app.doc.layers[0].opacity, 0);
            app.doc.layers[0].opacity = 255;
            if name == "floating" {
                let n = app.panel_box(Panel::Navigator);
                app.last_mouse = (n.x + 80, n.y + n.h - 20);
                app.cursor_visible = true;
                assert!(app.brush_cursor_rect().is_none());
                app.down(null_mut(), n.x + 80, n.y + n.h - 20, false);
                assert!(!app.drawing && app.dock_drag.is_none());
                app.down(null_mut(), n.x + n.w - 4, n.y + n.h - 4, false);
                assert!(app.dock_drag.is_some());
                app.motion(null_mut(), n.x + n.w + 40, n.y + n.h + 40);
                assert!(app.panel_box(Panel::Navigator).w > n.w);
                app.cancel_input(null_mut());
                assert_eq!(app.panel_box(Panel::Navigator).w, n.w);
                app.start_panel_drag(null_mut(), Panel::Brush, brush.x + 30, brush.y + 16);
                app.motion(null_mut(), 640, 320);
                assert!(!app.drawing);
                app.cancel_input(null_mut());
                assert_eq!(
                    app.workspace.docking.panels[Panel::Brush.index()].place,
                    Place::Left
                );
            }
            app.render_scene(surface.dc);
            let mut pixels = vec![0u32; 1280 * 900];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    900,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(1280, 900),
                    0
                ),
                900
            );
            SelectObject(surface.dc, surface.bitmap);
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            save_png(
                &root.join(format!("panels-{}.png", name)),
                &mut pixels,
                1280,
                900,
            )
            .unwrap();
        }
        app.resize(1350, 900, 144);
        let dc = GetDC(null_mut());
        let high = Backbuffer::new(dc, 1350, 900);
        ReleaseDC(null_mut(), dc);
        app.render_scene(high.dc);
        for h in &app.hits {
            assert!(
                h.r.left >= 0 && h.r.right <= 900 && h.r.top >= 0 && h.r.bottom <= 600,
                "{}",
                h.id
            );
        }
        let header = app.hits.iter().find(|h| h.id == 4002).unwrap();
        let point = Point {
            x: dpi::physical(header.r.left + 20, 144),
            y: dpi::physical(header.r.top + 12, 144),
        };
        let logical = app.logical_point(point);
        assert_eq!(
            app.cursor_at(logical.x, logical.y, false, false, false),
            32646
        );
        app.focus = true;
        app.render_scene(high.dc);
        assert!(!app.hits.iter().any(|h| (4000..=4003).contains(&h.id)));
        assert_eq!(app.view().left, 0);
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
        drop(surface);
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn caption_repaint_is_suppressed_while_native_metadata_and_visibility_survive() {
    unsafe {
        // A WS_VISIBLE child of a hidden parent exercises redraw suppression
        // without creating a visible desktop window or changing user focus.
        let instance = GetModuleHandleW(null());
        let name = wide(&format!("MiCoPi.CaptionTest.{}", std::process::id()));
        let class = WndClass {
            style: 3 | 8,
            proc: window_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: null_mut(),
            background: null_mut(),
            menu: null(),
            name: name.as_ptr(),
        };
        assert_ne!(RegisterClassW(&class), 0);
        let parent = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("parent").as_ptr(),
            0x02cf0000,
            0,
            0,
            1000,
            700,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        assert!(!parent.is_null());
        let child = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("child").as_ptr(),
            0x50000000,
            0,
            0,
            500,
            300,
            parent,
            null_mut(),
            instance,
            null_mut(),
        );
        assert!(!child.is_null());
        for window in [parent, child] {
            let style = GetWindowLongPtrW(window, -16);
            let mut before = Rect::default();
            GetClientRect(window, &mut before);
            chrome::appearance(window, crate::ui::Theme::preset(2), true);
            assert!(chrome::set_title(window, "* 测试绘画 — MiCoPi"));
            assert!(
                !chrome::set_title(window, "* 测试绘画 — MiCoPi"),
                "unchanged titles must not send another WM_SETTEXT"
            );
            for _ in 0..3 {
                assert_eq!(SendMessageW(window, 0x86, 0, 0), 1);
                assert_eq!(SendMessageW(window, 0x86, 1, 0), 1);
                for message in [0x85, 0xae, 0xaf] {
                    assert_eq!(SendMessageW(window, message, 1, 0), 0);
                }
                SendMessageW(window, 0x80, 0, 0);
            }
            let mut title = vec![0u16; 64];
            let count = GetWindowTextW(window, title.as_mut_ptr(), 64);
            assert_eq!(
                String::from_utf16_lossy(&title[..count as usize]),
                "* 测试绘画 — MiCoPi"
            );
            assert_eq!(
                GetWindowLongPtrW(window, -16),
                style,
                "caption suppression must restore all window style bits"
            );
            let mut after = Rect::default();
            GetClientRect(window, &mut after);
            assert_eq!((before.right, before.bottom), (after.right, after.bottom));
            let mut composition = 0;
            if window == parent
                && DwmIsCompositionEnabled(&mut composition) >= 0
                && composition != 0
            {
                let mut enabled = 0u32;
                assert!(DwmGetWindowAttribute(window, 1, &mut enabled as *mut _ as _, 4) >= 0);
                assert_ne!(
                    enabled, 0,
                    "blocking GDI title drawing must retain DWM composition"
                );
            }
        }
        DestroyWindow(parent);
        UnregisterClassW(name.as_ptr(), instance);
    }
}
#[test]
fn editor_shell_small_windows_and_sidebar_pages_share_draw_and_hit_geometry() {
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.keymap = Default::default();
        for (page, swapped, dpi) in [(0, false, 96), (1, false, 96), (1, true, 144)] {
            app.workspace.tools_page = page;
            app.workspace.swapped = swapped;
            app.tools_scroll = 0;
            app.resize(dpi::physical(900, dpi), dpi::physical(600, dpi), dpi);
            let dc = GetDC(null_mut());
            let surface = Backbuffer::new(dc, app.physical_size.0, app.physical_size.1);
            ReleaseDC(null_mut(), dc);
            app.render_scene(surface.dc);
            assert!(app.hits.iter().any(|h| h.id == 10110));
            assert!(app.hits.iter().any(|h| h.id == 10111));
            assert_eq!(
                app.hits.iter().any(|h| h.id == 800),
                page == 0,
                "color controls must be clipped out of the brush page"
            );
            let search = app
                .hits
                .iter()
                .find(|h| h.id == 95 && h.r.top < TOP)
                .unwrap();
            assert_eq!(
                chrome::hit(
                    900,
                    600,
                    (search.r.left + search.r.right) / 2,
                    (search.r.top + search.r.bottom) / 2,
                    5,
                    false
                ),
                1
            );
            for hit in &app.hits {
                assert!(
                    hit.r.left >= 0 && hit.r.right <= 900 && hit.r.top >= 0 && hit.r.bottom <= 600,
                    "{}",
                    hit.id
                );
            }
            if page == 1 {
                let slider = app.hits.iter().find(|h| h.id == 20).unwrap().r;
                let x = (slider.left + slider.right) / 2;
                app.adjust(20, x);
                assert!(app.radius > 40. && app.radius < 60.);
                let p = app.workspace.layout(900, 600, false).tools;
                assert!(p.contains(slider.left, slider.top));
            }
            let (width, height) = app.physical_size;
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
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            save_png(
                &root.join(format!("shell-small-page{}-dpi{}.png", page, dpi)),
                &mut pixels,
                width as usize,
                height as usize,
            )
            .unwrap();
        }
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn custom_frame_distinguishes_resize_caption_menus_and_buttons() {
    for (x, y, expected) in [
        (0, 0, 13),
        (1279, 0, 14),
        (0, 899, 16),
        (1279, 899, 17),
        (0, 400, 10),
        (1279, 400, 11),
        (600, 0, 12),
        (600, 899, 15),
        (50, 15, 2),
        (650, 15, 1),
        (150, 15, 1),
        (400, 15, 1),
        (1160, 15, 1),
        (1255, 15, 1),
        (600, 400, 1),
    ] {
        assert_eq!(
            chrome::hit(1280, 900, x, y, 5, false),
            expected,
            "{},{}",
            x,
            y
        );
    }
    assert_eq!(chrome::hit(1280, 900, 650, 0, 5, true), 2);
    assert_eq!(chrome::hit(1280, 900, 0, 400, 5, true), 1);
    assert_eq!(chrome::hit(1280, 900, -1, 30, 5, false), 0);
    for width in [900, 1280, 1920] {
        for id in chrome::MINIMIZE..=chrome::CLOSE {
            let r = chrome::button(width, id);
            assert_eq!(r.right - r.left, 46);
            assert_eq!(chrome::hit(width, 900, r.left + 23, 15, 5, false), 1);
        }
        assert_eq!(chrome::button(width, chrome::CLOSE).right, width);
    }
}
#[test]
fn command_overlay_scales_blocks_canvas_and_renders_both_modes() {
    unsafe {
        use crate::ui::palette::{Mode, Palette};
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.keymap = Default::default();
        for (name, mode, theme, dpi, w, h) in [
            ("commands", Mode::Commands, 0, 96, 1280, 900),
            ("bindings", Mode::Bindings, 2, 144, 1350, 900),
        ] {
            app.workspace.theme = theme;
            app.resize(w, h, dpi);
            let mut palette = Palette::new(mode, &app.keymap);
            palette.query(
                if mode == Mode::Commands {
                    "Brush"
                } else {
                    "tool"
                }
                .into(),
                &app.keymap,
            );
            app.palette = Some(palette);
            let dc = GetDC(null_mut());
            let surface = Backbuffer::new(dc, w, h);
            ReleaseDC(null_mut(), dc);
            app.render_scene(surface.dc);
            assert!(app.hits.iter().all(|hit| hit.id >= 12000));
            assert!(app.hits.iter().any(|hit| hit.id >= 20000));
            for hit in &app.hits {
                assert!(
                    hit.r.left >= 0
                        && hit.r.right <= app.size.0
                        && hit.r.top >= 0
                        && hit.r.bottom < app.size.1 - BOTTOM
                );
            }
            app.cursor_visible = true;
            app.last_mouse = (600, 400);
            assert!(app.brush_cursor_rect().is_none());
            assert_eq!(app.cursor_at(600, 400, false, false, false), 32512);
            app.down(null_mut(), 5, app.size.1 - 40, false);
            assert!(!app.drawing);
            assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
            let mut pixels = vec![0u32; w as usize * h as usize];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    h as u32,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(w as usize, h as usize),
                    0
                ),
                h
            );
            SelectObject(surface.dc, surface.bitmap);
            for pixel in &mut pixels {
                *pixel |= 0xff000000;
            }
            save_png(
                &root.join(format!("palette-{}.png", name)),
                &mut pixels,
                w as usize,
                h as usize,
            )
            .unwrap();
        }
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn ps_hand_zoom_and_wheel_actions_keep_the_expected_view() {
    unsafe {
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.fit();
        app.cursor_visible = true;
        app.last_mouse = (600, 400);
        app.select_tool(Tool::Hand);
        assert_eq!(app.cursor_at(600, 400, false, false, false), 32649);
        assert!(app.brush_cursor_rect().is_none());
        assert_eq!(app.cursor_at(15, 45, false, false, false), 32512);
        app.select_tool(Tool::Zoom);
        assert_eq!(app.cursor_at(600, 400, false, true, false), 32515);
        assert!(app.brush_cursor_rect().is_none());
        let before = app.canvas_pos(650, 420);
        app.zoom_at(650, 420, 1.25);
        let after = app.canvas_pos(650, 420);
        assert!((before.0 - after.0).abs() < 0.001 && (before.1 - after.1).abs() < 0.001);
        let zoom = app.zoom;
        let pan = app.pan;
        app.canvas_wheel(650, 420, 120., false, false);
        assert_eq!(app.zoom, zoom);
        assert_eq!(app.pan.1, pan.1 + 48.);
        app.canvas_wheel(650, 420, 120., false, true);
        assert_eq!(app.pan.0, pan.0 + 48.);
        let before = app.canvas_pos(650, 420);
        app.canvas_wheel(650, 420, 120., true, false);
        let after = app.canvas_pos(650, 420);
        assert!(app.zoom > zoom);
        assert!((before.0 - after.0).abs() < 0.001 && (before.1 - after.1).abs() < 0.001);
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
}
#[test]
fn resizing_refits_only_in_fit_mode_and_keeps_physical_manual_zoom() {
    unsafe {
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.resize(1100, 760, 96);
        let small = app.zoom;
        app.resize(1600, 1100, 96);
        assert!(app.zoom > small);
        assert_eq!(app.pan, (0., 0.));
        app.fit_to_window = false;
        app.zoom = 2.;
        app.pan = (12., -9.);
        app.resize(1200, 800, 96);
        assert_eq!(app.zoom, 2.);
        assert_eq!(app.pan, (12., -9.));
        app.resize(1920, 1350, 144);
        assert_eq!(app.size, (1280, 900));
        assert!((app.zoom_percent() - 200.).abs() < 0.001);
        assert_eq!(app.logical_point(Point { x: 150, y: 300 }).x, 100);
        assert!((app.pan.0 / app.zoom - 6.).abs() < 0.001);
        app.fit();
        assert!(app.fit_to_window);
        assert_eq!(app.pan, (0., 0.));
    }
}
#[test]
fn cursor_uses_splitter_and_header_shapes_through_drag() {
    unsafe {
        let mut app = App::new();
        app.workspace = Workspace::default();
        assert_eq!(app.cursor_at(272, 200, false, false, false), 32644);
        assert_eq!(app.cursor_at(1072, 200, false, false, false), 32644);
        assert_eq!(app.cursor_at(60, TOP + 10, false, false, false), 32646);
        assert_eq!(app.cursor_at(600, 400, false, false, false), 0);
        app.ui_drag = Some(0);
        assert_eq!(app.cursor_at(600, 400, false, false, false), 32644);
        app.ui_drag = None;
        assert_eq!(app.cursor_at(600, 400, true, false, false), 32649);
        assert_eq!(app.cursor_at(600, 400, false, true, false), 32515);
        app.workspace.swapped = true;
        assert_eq!(app.cursor_at(1056, 200, false, false, false), 32644);
    }
}
#[test]
fn native_hidden_window_resize_messages_update_layout() {
    unsafe {
        // Only a private, hidden test window is created. No desktop pointer input is sent.
        let instance = GetModuleHandleW(null());
        let name = wide(&format!("MiCoPi.ResizeTest.{}", std::process::id()));
        let class = WndClass {
            style: 3 | 8,
            proc: window_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: null_mut(),
            background: null_mut(),
            menu: null(),
            name: name.as_ptr(),
        };
        assert_ne!(RegisterClassW(&class), 0);
        APP.with(|s| *s.borrow_mut() = Some(App::new()));
        let window = CreateWindowExW(
            0,
            name.as_ptr(),
            name.as_ptr(),
            0xcf0000,
            0,
            0,
            1200,
            850,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        assert!(!window.is_null());
        assert_eq!(mem::size_of::<Margins>(), 16);
        chrome::appearance(window, crate::ui::Theme::preset(2), true);
        let mut composition = 0;
        if DwmIsCompositionEnabled(&mut composition) >= 0 && composition != 0 {
            let mut enabled = 0u32;
            assert!(DwmGetWindowAttribute(window, 1, &mut enabled as *mut _ as _, 4) >= 0);
            assert_ne!(
                enabled, 0,
                "custom frame must retain system non-client composition"
            );
        }
        // Newer attributes are optional on Windows 10; query native state where supported.
        for (maximized, expected) in [(false, 3u32), (true, 1), (false, 3)] {
            chrome::shape(window, maximized);
            let mut preference = 0u32;
            if DwmGetWindowAttribute(window, 33, &mut preference as *mut _ as _, 4) >= 0 {
                assert_eq!(
                    preference, expected,
                    "restoring a window must re-enable the small corner preference"
                );
            }
        }
        for theme in [0, 2] {
            let t = crate::ui::Theme::preset(theme);
            chrome::appearance(window, t, theme == 2);
            let mut border = 0u32;
            if DwmGetWindowAttribute(window, 34, &mut border as *mut _ as _, 4) >= 0 {
                assert_eq!(border, color(t.selected));
            }
        }
        let mut outer = Rect::default();
        let mut client = Rect::default();
        GetWindowRect(window, &mut outer);
        GetClientRect(window, &mut client);
        assert_eq!(
            client.right,
            outer.right - outer.left,
            "custom frame must remove the OS caption and border insets"
        );
        assert_eq!(client.bottom, outer.bottom - outer.top);
        let mut limits = [Point::default(); 5];
        SendMessageW(window, 0x24, 0, limits.as_mut_ptr() as isize);
        let mut monitor = MonitorInfo {
            size: mem::size_of::<MonitorInfo>() as u32,
            monitor: Rect::default(),
            work: Rect::default(),
            flags: 0,
        };
        assert_ne!(
            GetMonitorInfoW(MonitorFromWindow(window, 2), &mut monitor),
            0
        );
        assert_eq!(limits[1].x, monitor.work.right - monitor.work.left);
        assert_eq!(limits[1].y, monitor.work.bottom - monitor.work.top);
        let style = GetWindowLongPtrW(window, -16);
        SetWindowLongPtrW(window, -16, style | 0x01000000);
        for wparam in [0, 1] {
            let mut areas = [rect(
                monitor.work.left - 8,
                monitor.work.top - 8,
                monitor.work.right - monitor.work.left + 16,
                monitor.work.bottom - monitor.work.top + 16,
            ); 3];
            assert_eq!(
                SendMessageW(window, 0x83, wparam, areas.as_mut_ptr() as isize),
                0
            );
            assert_eq!(
                (areas[0].left, areas[0].top, areas[0].right, areas[0].bottom),
                (
                    monitor.work.left,
                    monitor.work.top,
                    monitor.work.right,
                    monitor.work.bottom
                )
            );
        }
        SetWindowLongPtrW(window, -16, style);
        SetWindowPos(window, null_mut(), 0, 0, 1100, 760, 0x16);
        let small = APP.with(|s| s.borrow().as_ref().unwrap().zoom);
        SetWindowPos(window, null_mut(), 0, 0, 1600, 1100, 0x16);
        APP.with(|s| {
            let state = s.borrow();
            let app = state.as_ref().unwrap();
            assert!(app.zoom > small);
            let mut r = Rect::default();
            GetClientRect(window, &mut r);
            assert_eq!(app.physical_size, (r.right, r.bottom));
        });
        // Native EDIT owns text input (including IME), never the global tool handler.
        let edit = APP.with(|slot| {
            let mut state = slot.borrow_mut();
            let app = state.as_mut().unwrap();
            app.keymap = Default::default();
            app.open_palette(window, crate::ui::palette::Mode::Commands);
            app.palette_edit
        });
        assert!(!edit.is_null());
        APP.with(|slot| {
            let mut state = slot.borrow_mut();
            let app = state.as_mut().unwrap();
            let mut client = Rect::default();
            GetClientRect(window, &mut client);
            app.maximized = true;
            app.resize(client.right, client.bottom, dpi::for_window(window));
            let (palette, _) = app.palette_geometry();
            let expected = app.client_point(Point {
                x: palette.left + 20,
                y: palette.top + 44,
            });
            let mut actual = Rect::default();
            GetWindowRect(edit, &mut actual);
            let mut origin = Point {
                x: actual.left,
                y: actual.top,
            };
            ScreenToClient(window, &mut origin);
            assert_eq!((origin.x, origin.y), (expected.x, expected.y));
            app.maximized = false;
            app.resize(client.right, client.bottom, dpi::for_window(window));
        });
        SendMessageW(edit, 0x102, 'b' as usize, 0);
        APP.with(|slot| {
            let mut state = slot.borrow_mut();
            let app = state.as_mut().unwrap();
            app.palette_query(window);
            assert_eq!(app.palette.as_ref().unwrap().query, "b");
            assert!(app.tool == Tool::Brush);
        });
        SetWindowTextW(edit, wide("Undo").as_ptr());
        APP.with(|slot| {
            let mut state = slot.borrow_mut();
            let app = state.as_mut().unwrap();
            app.palette_query(window);
            let p = app.palette.as_ref().unwrap();
            assert_eq!(
                crate::ui::keymap::COMMANDS[p.current().unwrap()].id,
                "edit.undo"
            );
            app.open_palette(window, crate::ui::palette::Mode::Bindings);
            SetWindowTextW(app.palette_edit, wide("tool.brush").as_ptr());
            app.palette_query(window);
            app.palette_key(window, 113);
            SetWindowTextW(app.palette_edit, wide("E").as_ptr());
            let original = app.keymap.encode();
            app.palette_key(window, 13);
            assert!(app.palette.as_ref().unwrap().message.contains("Conflicts"));
            assert_eq!(app.keymap.encode(), original);
            app.palette_key(window, 27);
            assert!(app.palette.as_ref().unwrap().recording.is_none());
            app.close_palette(window);
            assert!(app.palette_edit.is_null());
            assert!(app.palette.is_none());
            app.execute_action(window, crate::ui::shortcuts::Action::Eraser);
            assert!(app.tool == Tool::Eraser);
        });
        DestroyWindow(window);
        APP.with(|s| *s.borrow_mut() = None);
        UnregisterClassW(name.as_ptr(), instance);
    }
}
#[test]
fn high_dpi_render_and_hit_coordinates_use_the_same_scale() {
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.resize(1920, 1350, 144);
        app.doc.begin(
            600.,
            450.,
            Brush {
                radius: 20.,
                opacity: 1.,
                hardness: 1.,
                color: 0xff0000,
                erase: false,
            },
        );
        app.doc.finish();
        let dc = GetDC(null_mut());
        let surface = Backbuffer::new(dc, 1920, 1350);
        ReleaseDC(null_mut(), dc);
        for swapped in [false, true] {
            app.workspace.swapped = swapped;
            app.render_scene(surface.dc);
            let mut pixels = vec![0u32; 1920 * 1350];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    1350,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(1920, 1350),
                    0
                ),
                1350
            );
            SelectObject(surface.dc, surface.bitmap);
            let button = app.hits.iter().find(|h| h.id == 10).unwrap();
            let logical = Point {
                x: button.r.left + 3,
                y: button.r.top + 12,
            };
            let physical = Point {
                x: dpi::physical(logical.x, 144),
                y: dpi::physical(logical.y, 144),
            };
            let hit = app.logical_point(physical);
            assert!(inside(button.r, hit.x, hit.y));
            assert_eq!(
                pixels[physical.y as usize * 1920 + physical.x as usize] & 0xffffff,
                app.workspace.theme().selected
            );
            let view = app.view();
            let cx = dpi::physical((view.left + view.right) / 2, 144);
            let cy = dpi::physical((view.top + view.bottom) / 2, 144);
            assert_eq!(
                pixels[cy as usize * 1920 + cx as usize] & 0xffffff,
                0xff0000
            );
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            let root = std::env::current_dir().unwrap().join("target/test-output");
            std::fs::create_dir_all(&root).unwrap();
            save_png(
                &root.join(if swapped {
                    "workspace-dpi150-swapped.png"
                } else {
                    "workspace-dpi150.png"
                }),
                &mut pixels,
                1920,
                1350,
            )
            .unwrap();
        }
        drop(surface);
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn workbench_controls_follow_swapped_scrolled_layout() {
    unsafe {
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.size = (1280, 900);
        app.apply_preset(3);
        assert_eq!(app.radius, 22.);
        assert_eq!(app.hardness, 0.);
        app.apply_preset(5);
        assert!(app.tool == Tool::Eraser);
        app.size_preset(3);
        assert_eq!(app.radius, 2.5);
        for swapped in [false, true] {
            app.workspace.swapped = swapped;
            app.tools_scroll = 37;
            let l = app.workspace.layout(1280, 900, false);
            let g = crate::ui::ToolsGeometry::new(l.tools.w);
            let (o, side) = crate::engine::color::square(g.wheel);
            app.color_hue = 0.;
            app.adjust_control(
                801,
                l.tools.x + (l.tools.w - g.wheel) / 2 + o + side - 1,
                l.tools.y + PANEL_HEADER - app.tools_scroll + 6 + o,
            );
            assert_eq!(app.ink, 0xff0000);
            app.adjust_control(804, l.tools.x + 34 + l.tools.w - 80, 0);
            assert_eq!(app.ink, 0xff00ff);
            app.zoom = 4.;
            let n = app.navigator_rect();
            app.adjust_control(810, n.left, n.top);
            assert_eq!(
                app.pan,
                (app.doc.width as f32 * 2., app.doc.height as f32 * 2.)
            );
            app.adjust_control(805, l.layers.x + 65, 0);
            assert_eq!(app.doc.layers[app.doc.active].opacity, 0);
            assert!(app.doc.dirty);
        }
    }
}
#[test]
fn brush_cursor_matches_zoom_pressure_and_tool() {
    unsafe {
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.cursor_visible = true;
        app.last_mouse = (600, 400);
        app.radius = 20.;
        app.zoom = 2.;
        let r = app.brush_cursor_rect().unwrap();
        assert_eq!((r.left, r.right), (560, 641));
        app.tool = Tool::Eraser;
        assert_eq!(app.brush_cursor_rect().unwrap().left, 560);
        app.pen_id = Some(1);
        app.drawing = true;
        app.pressure = 0.25;
        assert!(app.brush_cursor_rect().unwrap().right - r.left < 81);
        app.tool = Tool::Picker;
        assert!(app.brush_cursor_rect().is_none());
        app.tool = Tool::Brush;
        app.last_mouse = (10, 400);
        assert!(app.brush_cursor_rect().is_none());
        app.last_mouse = (600, 400);
        app.cursor_override = true;
        assert!(app.brush_cursor_rect().is_none());
    }
}
#[test]
fn cursor_overlay_clips_and_leaves_document_and_scene_clean() {
    unsafe {
        unsafe fn capture(s: &Backbuffer) -> Vec<u32> {
            let mut p = vec![0; 1280 * 900];
            SelectObject(s.dc, s.old);
            assert_eq!(
                GetDIBits(
                    s.dc,
                    s.bitmap,
                    0,
                    900,
                    p.as_mut_ptr() as _,
                    &mut BitmapInfo::new(1280, 900),
                    0
                ),
                900
            );
            SelectObject(s.dc, s.bitmap);
            p
        }
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.size = (1280, 900);
        app.fit();
        app.radius = 50.;
        let dc = GetDC(null_mut());
        let s = Backbuffer::new(dc, 1280, 900);
        ReleaseDC(null_mut(), dc);
        app.draw_frame(s.dc);
        let clean = capture(&s);
        app.cursor_visible = true;
        app.last_mouse = (600, 400);
        app.draw_brush_cursor(s.dc);
        let preview = capture(&s);
        assert!(preview.iter().zip(&clean).filter(|(a, b)| a != b).count() > 80);
        assert!(app.doc.layers[0].pixels.iter().all(|p| *p == 0));
        app.draw_frame(s.dc);
        assert!(
            capture(&s) == clean,
            "redrawing the clean scene must remove the cursor overlay"
        );
        let v = app.view();
        app.last_mouse = (v.left + 1, 400);
        app.draw_brush_cursor(s.dc);
        let edge = capture(&s);
        for y in 0..900 {
            for x in 0..v.left as usize {
                let i = y * 1280 + x;
                assert_eq!(edge[i], clean[i]);
            }
        }
        drop(s);
        drop(app);
    }
}
#[test]
fn pen_abi_matches_windows_x64() {
    assert_eq!(mem::size_of::<PointerInfo>(), 96);
    assert_eq!(mem::size_of::<PenInfo>(), 120);
}
#[test]
fn render_custom_workspaces_offscreen() {
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.size = (1280, 900);
        app.fit();
        // A real varying-width raster stroke provides useful visual regression output.
        app.doc.begin(
            150.,
            400.,
            Brush {
                radius: 1.,
                ..app.brush()
            },
        );
        for i in 1..180 {
            app.doc.line(
                150. + i as f32 * 4.,
                400. + (i as f32 * 0.025).sin() * 180.,
                Brush {
                    radius: 1. + i as f32 * 0.15,
                    ..app.brush()
                },
            );
        }
        app.doc.finish();
        let dc = GetDC(null_mut());
        let surface = Backbuffer::new(dc, 1280, 900);
        ReleaseDC(null_mut(), dc);
        let root = std::env::current_dir().unwrap().join("target/test-output");
        std::fs::create_dir_all(&root).unwrap();
        for (name, swapped, focus, theme, width) in [
            ("default", false, false, 0, 224),
            ("custom", true, false, 1, 300),
            ("dark", false, false, 2, 224),
            ("hover", false, false, 0, 224),
            ("focus", false, true, 2, 224),
        ] {
            app.workspace.swapped = swapped;
            app.workspace.theme = theme;
            app.workspace.tools_width = width;
            app.focus = focus;
            app.hovered = None;
            app.draw_frame(surface.dc);
            if name == "hover" {
                let h = app.hits.iter().find(|h| h.id == 40).unwrap();
                app.hovered = Some((h.id, h.r.left, h.r.top));
                app.draw_frame(surface.dc);
            }
            let layout = app.workspace.layout(1280, 900, focus);
            if focus {
                assert!(!app
                    .hits
                    .iter()
                    .any(|h| h.id == 500 || h.id == 501 || h.id == 10110));
                assert!(app.hits.iter().any(|h| h.id == 10));
            } else {
                let h = app
                    .hits
                    .iter()
                    .find(|h| h.id == 10 && layout.tools.contains(h.r.left, h.r.top))
                    .unwrap();
                assert!(layout.tools.contains(h.r.left, h.r.top));
                let h = app.hits.iter().find(|h| h.id == 200).unwrap();
                assert!(layout.layers.contains(h.r.left, h.r.top));
                let h = app.hits.iter().find(|h| h.id == 849).expect(
                    "all brush diameter presets must be visible at the default window height",
                );
                assert_eq!(
                    h.r.bottom - h.r.top,
                    23,
                    "last diameter preset must not be clipped"
                );
            }
            let mut pixels = vec![0u32; 1280 * 900];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    900,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(1280, 900),
                    0
                ),
                900
            );
            SelectObject(surface.dc, surface.bitmap);
            for p in &mut pixels {
                *p |= 0xff000000;
            }
            if name == "hover" {
                let h = app.hits.iter().find(|h| h.id == 40).unwrap();
                assert_eq!(
                    pixels[(h.r.top as usize + 6) * 1280 + h.r.left as usize + 6] & 0xffffff,
                    app.workspace.theme().selected
                );
            }
            assert_eq!(
                pixels[(app.view().top as usize + 1) * 1280 + 640] & 0xffffff,
                app.workspace.theme().background
            );
            save_png(
                &root.join(format!("workspace-{}.png", name)),
                &mut pixels,
                1280,
                900,
            )
            .unwrap();
        }
        drop(surface);
        drop(app);
        GdiplusShutdown(token);
    }
}
#[test]
fn png_roundtrip_with_alpha_and_unicode_path() {
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
        let root = std::env::current_dir().unwrap().join("target/test-output");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("透明画布-test.png");
        let mut pixels = vec![0xff123456, 0x8078abef, 0, 0xffff0000];
        save_png(&path, &mut pixels, 2, 2).unwrap();
        let d = load_image(&path).unwrap();
        assert_eq!(d.width, 2);
        assert_eq!(d.height, 2);
        assert_eq!(d.layers[0].pixels, pixels);
        std::fs::remove_file(path).unwrap();
        GdiplusShutdown(token);
    }
}

#[test]
fn maximized_client_clamps_invisible_frame_on_offset_monitors_and_taskbars() {
    let work = rect(-1920, 40, 1920, 1040);
    let proposed = rect(-1928, -8, 1936, 1096);
    let fitted = chrome::fit_maximized_client(proposed, work);
    assert_eq!(
        (fitted.left, fitted.top, fitted.right, fitted.bottom),
        (-1920, 40, 0, 1080)
    );
    let work = rect(80, -1440, 2480, 1400);
    let fitted = chrome::fit_maximized_client(rect(-8, -1448, 2576, 1456), work);
    assert_eq!(
        (fitted.left, fitted.top, fitted.right, fitted.bottom),
        (80, -1440, 2560, -40)
    );
    let fitted = chrome::fit_maximized_client(work, work);
    assert_eq!(
        (fitted.left, fitted.top, fitted.right, fitted.bottom),
        (work.left, work.top, work.right, work.bottom)
    );
}
#[test]
fn maximized_padding_keeps_render_input_panels_and_cursor_in_alignment() {
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
        let mut app = App::new();
        app.workspace = Workspace::default();
        app.workspace.theme = 2;
        app.workspace.enable_docking();
        app.keymap = Default::default();
        for dpi in [96, 144, 192] {
            app.maximized = true;
            let (w, h) = (dpi::physical(1280, dpi), dpi::physical(900, dpi));
            app.resize(w, h, dpi);
            assert_eq!(app.size, (1264, 884));
            let dc = GetDC(null_mut());
            let surface = Backbuffer::new(dc, w, h);
            ReleaseDC(null_mut(), dc);
            app.render_scene(surface.dc);
            for hit in &app.hits {
                let local = Point {
                    x: (hit.r.left + hit.r.right) / 2,
                    y: (hit.r.top + hit.r.bottom) / 2,
                };
                let device = app.client_point(local);
                let roundtrip = app.logical_point(device);
                assert_eq!((roundtrip.x, roundtrip.y), (local.x, local.y));
                assert!(device.x >= dpi::physical(8, dpi) && device.x < w - dpi::physical(8, dpi));
                assert!(device.y >= dpi::physical(8, dpi) && device.y < h - dpi::physical(8, dpi));
            }
            let eraser = app.hits.iter().find(|hit| hit.id == 11).unwrap().r;
            let click = app.client_point(Point {
                x: eraser.left + 10,
                y: eraser.top + 10,
            });
            let click = app.logical_point(click);
            assert_eq!(
                app.hits
                    .iter()
                    .rev()
                    .find(|h| inside(h.r, click.x, click.y))
                    .map(|h| h.id),
                Some(11)
            );
            app.down(null_mut(), click.x, click.y, false);
            assert!(!app.drawing);
            let damage = app.client_rect(rect(100, 120, 40, 50));
            assert_eq!(damage.left, dpi::physical(108, dpi));
            assert_eq!(damage.top, dpi::physical(128, dpi));
            let mut pixels = vec![0u32; (w * h) as usize];
            SelectObject(surface.dc, surface.old);
            assert_eq!(
                GetDIBits(
                    surface.dc,
                    surface.bitmap,
                    0,
                    h as u32,
                    pixels.as_mut_ptr() as _,
                    &mut BitmapInfo::new(w as usize, h as usize),
                    0
                ),
                h
            );
            SelectObject(surface.dc, surface.bitmap);
            let at = |x: i32, y: i32| pixels[(y * w + x) as usize] & 0xffffff;
            for (x, y) in [
                (0, 0),
                (w / 2, 0),
                (w - 1, h / 2),
                (w / 2, h - 1),
                (0, h / 2),
            ] {
                assert_eq!(at(x, y), app.workspace.theme().panel);
            }
            for pixel in &mut pixels {
                *pixel |= 0xff000000;
            }
            save_png(
                &root.join(format!("workspace-maximized-dpi{}.png", dpi)),
                &mut pixels,
                w as usize,
                h as usize,
            )
            .unwrap();
            app.maximized = false;
            app.resize(w, h, dpi);
            assert_eq!(app.size, (1280, 900));
            assert_eq!(app.content_padding(), 0);
        }
        drop(app);
        GdiplusShutdown(token);
    }
}
#[cfg(feature = "desktop")]
#[test]
fn desktop_bridge_forwards_activation_to_the_framework_without_native_repaint() {
    thread_local! { static ACTIVATION: std::cell::Cell<(usize,usize,isize)> = const { std::cell::Cell::new((0,0,0)) }; }
    unsafe extern "system" fn framework_proc(hwnd: Hwnd, msg: u32, w: usize, l: isize) -> isize {
        if msg == 0x86 {
            ACTIVATION.with(|state| state.set((state.get().0 + 1, w, l)));
            return 1;
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
    unsafe {
        let instance = GetModuleHandleW(null());
        let name = wide(&format!("MiCoPi.BridgeFocusTest.{}", std::process::id()));
        let class = WndClass {
            style: 0,
            proc: framework_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: null_mut(),
            cursor: null_mut(),
            background: null_mut(),
            menu: null(),
            name: name.as_ptr(),
        };
        assert_ne!(RegisterClassW(&class), 0);
        let window = CreateWindowExW(
            0,
            name.as_ptr(),
            wide("focus regression").as_ptr(),
            0x00cf0000,
            0,
            0,
            640,
            480,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        assert!(!window.is_null());
        let native = desktop_bridge::Native::new(window as isize, eframe::egui::Context::default());
        ACTIVATION.with(|state| state.set((0, 0, 0)));
        for active in [1, 0, 1] {
            assert_eq!(SendMessageW(window, 0x86, active, 0), 1);
            let (count, received, repaint) = ACTIVATION.with(|state| state.get());
            assert!(count > 0, "native chrome swallowed framework activation");
            assert_eq!(received, active);
            assert_eq!(
                repaint, -1,
                "activation must preserve custom title-bar painting"
            );
        }
        assert_eq!(ACTIVATION.with(|state| state.get().0), 3);
        drop(native);
        DestroyWindow(window);
        UnregisterClassW(name.as_ptr(), instance);
    }
}
