//! Dock/float gestures stay inside the workbench and never enter the ink path.
use super::*;
use crate::ui::docking::{Docking, Panel, Region, Target, FLOAT_CAPTION};
#[derive(Clone, Copy)]
pub(super) enum Gesture {
    Move(Panel),
    FloatSize(Panel, u8),
    Column(bool),
    Split(Panel, Panel, bool),
    Row(bool),
}
pub(super) struct Drag {
    kind: Gesture,
    pub(super) before: Docking,
    start: (i32, i32),
    origin: Box2,
    grab: (i32, i32),
    moved: bool,
    target: Option<Target>,
    whole_group: bool,
}
fn resize_cursor(edges: u8) -> usize {
    match edges {
        5 | 10 => 32642,
        6 | 9 => 32643,
        1 | 2 => 32644,
        4 | 8 => 32645,
        _ => 32642,
    }
}
impl App {
    pub(super) fn dock_layout(&self) -> crate::ui::docking::DockLayout {
        self.workspace.docking.layout(
            self.size.0,
            self.size.1,
            self.focus,
            self.workspace.tools_visible,
            self.workspace.layers_visible,
        )
    }
    pub(super) fn panel_box(&self, p: Panel) -> Box2 {
        if self.workspace.docking.enabled {
            self.dock_layout()
                .regions
                .iter()
                .find(|r| r.contains_panel(p))
                .map(|r| r.content())
                .unwrap_or(Box2 {
                    x: 0,
                    y: TOP,
                    w: 0,
                    h: 0,
                })
        } else {
            let l = self.workspace.layout(self.size.0, self.size.1, self.focus);
            match p {
                Panel::Color | Panel::Brush => l.tools,
                _ => l.layers,
            }
        }
    }
    fn dock_boundary(
        &self,
        l: &crate::ui::docking::DockLayout,
        x: i32,
        y: i32,
    ) -> Option<(Gesture, Box2)> {
        if l.top_at(x, y).map_or(false, |r| r.floating) {
            return None;
        }
        for (i, c) in [l.left, l.right].iter().enumerate() {
            let edge = if i == 0 { c.x + c.w } else { c.x };
            if c.w > 0 && (x - edge).abs() <= 5 && y >= c.y && y < c.y + c.h {
                return Some((Gesture::Column(i == 0), *c));
            }
        }
        for (i, r) in [l.top, l.bottom].iter().enumerate() {
            let edge = if i == 0 { r.y + r.h } else { r.y };
            if r.h > 0 && (y - edge).abs() <= 5 && x >= r.x && x < r.x + r.w {
                return Some((Gesture::Row(i == 0), *r));
            }
        }
        for a in &l.regions {
            if a.floating {
                continue;
            }
            for b in &l.regions {
                if b.floating {
                    continue;
                }
                let r = a.bounds;
                let n = b.bounds;
                if r.x == n.x
                    && r.y + r.h + 6 == n.y
                    && x >= r.x
                    && x < r.x + r.w
                    && (y - r.y - r.h - 3).abs() <= 3
                {
                    return Some((Gesture::Split(a.panel, b.panel, false), r));
                }
                if r.y == n.y
                    && r.x + r.w + 6 == n.x
                    && y >= r.y
                    && y < r.y + r.h
                    && (x - r.x - r.w - 3).abs() <= 3
                {
                    return Some((Gesture::Split(a.panel, b.panel, true), r));
                }
            }
        }
        None
    }
    pub(super) fn dock_cursor(&self, x: i32, y: i32) -> Option<usize> {
        let cursor = |kind| match kind {
            Gesture::Move(_) => 32646,
            Gesture::Column(_) | Gesture::Split(_, _, true) => 32644,
            Gesture::Row(_) | Gesture::Split(_, _, false) => 32645,
            Gesture::FloatSize(_, edges) => resize_cursor(edges),
        };
        if let Some(d) = &self.dock_drag {
            return Some(cursor(d.kind));
        }
        if !self.workspace.docking.enabled {
            return None;
        }
        let l = self.dock_layout();
        if let Some((kind, _)) = self.dock_boundary(&l, x, y) {
            return Some(cursor(kind));
        }
        l.top_at(x, y).map(|r| {
            let edges = r.resize_edges(x, y);
            if edges != 0 {
                resize_cursor(edges)
            } else if y < r.content().y + PANEL_HEADER {
                32646
            } else {
                32512
            }
        })
    }
    pub(super) unsafe fn start_panel_drag(&mut self, hwnd: Hwnd, panel: Panel, x: i32, y: i32) {
        let before = self.workspace.docking.clone();
        self.workspace.enable_docking();
        let origin = self.dock_layout().panel(panel).unwrap();
        self.dock_drag = Some(Drag {
            kind: Gesture::Move(panel),
            before,
            start: (x, y),
            origin,
            grab: (
                (x - origin.x).max(8).min(origin.w - 8),
                (y - origin.y).max(0).min(PANEL_HEADER + FLOAT_CAPTION),
            ),
            moved: false,
            target: None,
            whole_group: false,
        });
        self.cursor_visible = false;
        if self.pen_id.is_none() {
            SetCapture(hwnd);
        }
        self.invalidate_scene(hwnd);
        self.sync_cursor(hwnd);
    }
    pub(super) unsafe fn dock_down(&mut self, hwnd: Hwnd, x: i32, y: i32) -> bool {
        if !self.workspace.docking.enabled {
            return false;
        }
        let l = self.dock_layout();
        let mut selected = self.dock_boundary(&l, x, y);
        if selected.is_none() {
            if let Some(r) = l.top_at(x, y) {
                let b = r.bounds;
                let edges = r.resize_edges(x, y);
                if r.floating {
                    self.workspace.docking.raise(r.panel);
                    self.invalidate_scene(hwnd);
                }
                if edges != 0 {
                    selected = Some((Gesture::FloatSize(r.panel, edges), b));
                } else if y < r.content().y + PANEL_HEADER {
                    let tab = r.tab_at(x, y);
                    let panel = tab.unwrap_or(r.panel);
                    let changed =
                        self.workspace.docking.active[self.workspace.docking.root(panel)] != panel;
                    if changed {
                        self.workspace.docking.select(panel);
                        self.hits.clear();
                        self.save_workspace();
                    }
                    self.start_panel_drag(hwnd, panel, x, y);
                    if let Some(d) = self.dock_drag.as_mut() {
                        d.whole_group = tab.is_none();
                    }
                    return true;
                } else if r.floating && !self.hits.iter().any(|h| inside(h.r, x, y)) {
                    return true;
                }
            }
        }
        if let Some((kind, origin)) = selected {
            self.dock_drag = Some(Drag {
                kind,
                before: self.workspace.docking.clone(),
                start: (x, y),
                origin,
                grab: (0, 0),
                moved: false,
                target: None,
                whole_group: false,
            });
            self.cursor_visible = false;
            if self.pen_id.is_none() {
                SetCapture(hwnd);
            }
            self.invalidate_scene(hwnd);
            self.sync_cursor(hwnd);
            return true;
        }
        false
    }
    pub(super) unsafe fn dock_motion(&mut self, hwnd: Hwnd, x: i32, y: i32) -> bool {
        let mut d = match self.dock_drag.take() {
            Some(d) => d,
            None => return false,
        };
        let dx = x - d.start.0;
        let dy = y - d.start.1;
        d.moved |= dx.abs() + dy.abs() >= 4;
        if d.moved {
            match d.kind {
                Gesture::Move(panel) => {
                    d.target = Some(self.workspace.docking.target(
                        panel,
                        x,
                        y,
                        d.origin,
                        d.grab,
                        self.size.0,
                        self.size.1,
                        self.workspace.tools_visible,
                        self.workspace.layers_visible,
                    ))
                }
                Gesture::FloatSize(panel, edges) => {
                    let width = (d.origin.w
                        + if edges & 1 != 0 {
                            -dx
                        } else if edges & 2 != 0 {
                            dx
                        } else {
                            0
                        })
                    .max(224);
                    let height = (d.origin.h
                        + if edges & 4 != 0 {
                            -dy
                        } else if edges & 8 != 0 {
                            dy
                        } else {
                            0
                        })
                    .max(150);
                    let max_width = if edges & 1 != 0 {
                        d.origin.x + d.origin.w - crate::ui::ACTIVITY_WIDTH - 4
                    } else {
                        self.size.0 - d.origin.x - 4
                    };
                    let max_height = if edges & 4 != 0 {
                        d.origin.y + d.origin.h - TOP - 4
                    } else {
                        self.size.1 - BOTTOM - d.origin.y - 4
                    };
                    let width = width.min(max_width.min(800).max(224));
                    let height = height.min(max_height.max(150));
                    let r = Box2 {
                        x: if edges & 1 != 0 {
                            d.origin.x + d.origin.w - width
                        } else {
                            d.origin.x
                        },
                        y: if edges & 4 != 0 {
                            d.origin.y + d.origin.h - height
                        } else {
                            d.origin.y
                        },
                        w: width,
                        h: height,
                    };
                    self.workspace.docking.panels[panel.index()].floating = self
                        .workspace
                        .docking
                        .clamp_float(r, self.size.0, self.size.1);
                    self.workspace.docking.sync(panel);
                }
                Gesture::Column(left) => {
                    let width = if left {
                        d.origin.w + dx
                    } else {
                        d.origin.w - dx
                    };
                    if left {
                        self.workspace.docking.left_width = width.max(224).min(420)
                    } else {
                        self.workspace.docking.right_width = width.max(224).min(420)
                    }
                }
                Gesture::Row(top) => {
                    let value = (d.origin.h + if top { dy } else { -dy }).max(96).min(420);
                    if top {
                        self.workspace.docking.top_height = value
                    } else {
                        self.workspace.docking.bottom_height = value
                    }
                }
                Gesture::Split(top, bottom, horizontal) => {
                    let l = d.before.layout(
                        self.size.0,
                        self.size.1,
                        false,
                        self.workspace.tools_visible,
                        self.workspace.layers_visible,
                    );
                    if let (Some(a), Some(b)) = (l.panel(top), l.panel(bottom)) {
                        let total = if horizontal { a.w + b.w } else { a.h + b.h };
                        let height = (if horizontal { a.w + dx } else { a.h + dy })
                            .max((if horizontal { 160 } else { 96 }).min(total / 2))
                            .min(total - (if horizontal { 160 } else { 96 }).min(total / 2));
                        let weight = d.before.panels[top.index()].weight
                            + d.before.panels[bottom.index()].weight;
                        let weight_top = (weight * height / total).max(1);
                        self.workspace.docking.panels[top.index()].weight = weight_top;
                        self.workspace.docking.panels[bottom.index()].weight =
                            (weight - weight_top).max(1);
                        self.workspace.docking.sync(top);
                        self.workspace.docking.sync(bottom);
                    }
                }
            }
        }
        self.last_mouse = (x, y);
        self.dock_drag = Some(d);
        if self.fit_to_window {
            self.fit();
        }
        self.invalidate_scene(hwnd);
        self.sync_cursor(hwnd);
        true
    }
    pub(super) unsafe fn dock_up(&mut self, hwnd: Hwnd) -> bool {
        let d = match self.dock_drag.take() {
            Some(d) => d,
            None => return false,
        };
        if d.moved {
            if let Gesture::Move(p) = d.kind {
                if let Some(target) = d.target {
                    if d.whole_group {
                        self.workspace.docking.move_group(p, target);
                    } else {
                        self.workspace.docking.move_panel(p, target);
                    }
                }
            }
            self.status =
                "Layout saved · Drag headers to dock; release over the canvas to float".into();
            self.save_workspace();
        } else {
            self.workspace.docking = d.before;
        }
        if self.fit_to_window {
            self.fit();
        }
        if self.pen_id.is_none() {
            ReleaseCapture();
        }
        self.hits.clear();
        self.invalidate_scene(hwnd);
        self.sync_cursor(hwnd);
        true
    }
    pub(super) unsafe fn draw_dock_panels(&mut self, dc: Dc) {
        if !self.workspace.docking.enabled {
            return;
        }
        let l = self.dock_layout();
        let t = self.workspace.theme();
        for c in [l.left, l.right, l.top, l.bottom] {
            if c.w > 0 && c.h > 0 {
                Self::fill(dc, rect(c.x, c.y, c.w, c.h), t.background);
            }
        }
        for r in l.regions {
            let outer = r.bounds;
            let p = r.content();
            if r.floating {
                self.hits.retain(|h| {
                    h.r.right <= outer.x
                        || h.r.left >= outer.x + outer.w
                        || h.r.bottom <= outer.y
                        || h.r.top >= outer.y + outer.h
                });
                self.rounded(
                    dc,
                    rect(outer.x + 5, outer.y + 7, outer.w, outer.h),
                    0x111820,
                    7,
                );
                self.rounded(
                    dc,
                    rect(outer.x + 2, outer.y + 3, outer.w, outer.h),
                    0x18212a,
                    6,
                );
                Self::fill(dc, rect(outer.x, outer.y, outer.w, outer.h), t.button);
            }
            self.render_panel = Some(r.panel);
            let old = self.tools_scroll;
            self.tools_scroll = self.panel_scroll[r.panel.index()];
            match r.panel {
                Panel::Color | Panel::Brush => self.draw_tools(dc, p),
                _ => self.draw_layers(dc, p),
            }
            self.panel_scroll[r.panel.index()] = self.tools_scroll;
            self.tools_scroll = old;
            self.render_panel = None;
            self.draw_panel_tabs(dc, r);
            if r.floating {
                Self::fill(
                    dc,
                    rect(outer.x + 2, outer.y + 2, outer.w - 4, FLOAT_CAPTION - 2),
                    t.selected,
                );
                SelectObject(dc, self.small_font);
                Self::text(
                    dc,
                    rect(outer.x + 10, outer.y + 1, outer.w - 24, FLOAT_CAPTION - 1),
                    "Floating Panel · Drag Here to Move the Group",
                    t.muted,
                );
                self.hit(
                    rect(outer.x, outer.y, outer.w, FLOAT_CAPTION),
                    4400 + r.panel.index() as u32,
                );
                self.frame(dc, rect(outer.x, outer.y, outer.w, outer.h), t.accent);
                self.frame(
                    dc,
                    rect(outer.x + 1, outer.y + 1, outer.w - 2, outer.h - 2),
                    t.button,
                );
                let saved = SaveDC(dc);
                let pen = CreatePen(0, 1, color(t.muted));
                SelectObject(dc, pen);
                for n in [0, 4, 8] {
                    MoveToEx(
                        dc,
                        outer.x + outer.w - 5 - n,
                        outer.y + outer.h - 4,
                        null_mut(),
                    );
                    LineTo(dc, outer.x + outer.w - 4, outer.y + outer.h - 5 - n);
                }
                RestoreDC(dc, saved);
                DeleteObject(pen);
            } else {
                self.frame(dc, rect(outer.x, outer.y, outer.w, outer.h), t.button);
            }
        }
        SelectObject(dc, self.font);
    }
    unsafe fn draw_panel_tabs(&mut self, dc: Dc, region: Region) {
        let p = region.content();
        let saved = SaveDC(dc);
        IntersectClipRect(dc, p.x, p.y, p.x + p.w, p.y + PANEL_HEADER);
        let old_clip = self.widget_clip;
        self.widget_clip = Some(rect(p.x, p.y, p.w, PANEL_HEADER));
        let t = self.workspace.theme();
        Self::fill(dc, rect(p.x, p.y, p.w, PANEL_HEADER), t.panel);
        Self::fill(dc, rect(p.x, p.y + PANEL_HEADER - 1, p.w, 1), t.button);
        SelectObject(dc, self.font);
        for panel in region.tabs.iter().flatten().copied() {
            let b = region.tab_bounds(panel).unwrap();
            let selected = panel == region.panel;
            if selected {
                Self::fill(dc, rect(b.x, b.y, b.w, b.h), t.button);
                Self::fill(dc, rect(b.x + 8, b.y + b.h - 2, b.w - 16, 2), t.accent);
            } else if self
                .hovered
                .map_or(false, |h| h.0 == 4300 + panel.index() as u32)
            {
                self.rounded(dc, rect(b.x, b.y, b.w, b.h), t.selected, 3);
            }
            Self::centered(
                dc,
                rect(b.x, b.y, b.w, b.h),
                panel.name(),
                if selected { t.text } else { t.muted },
            );
            self.hit(rect(b.x, b.y, b.w, b.h), 4300 + panel.index() as u32);
        }
        for row in 0..3 {
            for col in 0..2 {
                Self::fill(
                    dc,
                    rect(p.x + p.w - 17 + col * 4, p.y + 10 + row * 4, 1, 1),
                    t.muted,
                );
            }
        }
        self.widget_clip = old_clip;
        RestoreDC(dc, saved);
    }
    pub(super) unsafe fn draw_dock_preview(&self, dc: Dc) {
        if let Some(d) = &self.dock_drag {
            if let Gesture::Move(p) = d.kind {
                if let Some(target) = d.target {
                    let r = target.preview();
                    let t = self.workspace.theme();
                    self.frame(dc, rect(r.x, r.y, r.w, r.h), t.accent);
                    self.frame(dc, rect(r.x + 1, r.y + 1, r.w - 2, r.h - 2), t.accent);
                    Self::fill(
                        dc,
                        rect(
                            (r.x + 4).min(self.size.0 - 220),
                            (r.y + 4).min(self.size.1 - BOTTOM - 32),
                            210,
                            28,
                        ),
                        t.selected,
                    );
                    Self::text(
                        dc,
                        rect(
                            (r.x + 12).min(self.size.0 - 212),
                            (r.y + 4).min(self.size.1 - BOTTOM - 32),
                            194,
                            28,
                        ),
                        &format!(
                            "{} · {}",
                            p.name(),
                            match target {
                                Target::Float(_) => "Release to Float",
                                Target::Tab { .. } => "Group / Reorder Tabs",
                                Target::Dock { place, .. } => match place {
                                    crate::ui::docking::Place::Top => "Dock Top",
                                    crate::ui::docking::Place::Bottom => "Dock Bottom",
                                    crate::ui::docking::Place::Left => "Dock Left",
                                    crate::ui::docking::Place::Right => "Dock Right",
                                    _ => "Release to Dock",
                                },
                            }
                        ),
                        t.text,
                    );
                }
            }
        }
    }
    pub(super) fn panel_scroll_limit(&self, p: Panel, b: Box2) -> i32 {
        let g = crate::ui::ToolsGeometry::new(b.w);
        let content = match p {
            Panel::Color => g.tools - 24,
            Panel::Brush => g.height - g.tools,
            _ => 0,
        };
        (content - (b.h - PANEL_HEADER)).max(0)
    }
}
