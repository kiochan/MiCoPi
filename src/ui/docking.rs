//! Four lightweight dockable painting panels; all geometry is in logical pixels.
use super::workspace::{Box2, ACTIVITY_WIDTH, CHROME_BOTTOM, CHROME_TOP};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Color,
    Brush,
    Navigator,
    Layers,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::Workspace;
    #[test]
    fn four_edge_docks_tile_the_work_area_and_preserve_panel_minimum_widths() {
        let mut w = Workspace::default();
        w.enable_docking();
        let origin = w
            .docking
            .layout(1280, 900, false, true, true)
            .panel(Panel::Brush)
            .unwrap();
        for (x, y, place) in [
            (60, 120, Place::Left),
            (1270, 400, Place::Right),
            (650, 40, Place::Top),
            (650, 870, Place::Bottom),
        ] {
            assert!(
                matches!(w.docking.target(Panel::Brush,x,y,origin,(20,16),1280,900,true,true),Target::Dock{place:p,..}if p==place)
            );
        }
        for (panel, place) in
            Panel::ALL
                .iter()
                .zip([Place::Left, Place::Right, Place::Top, Place::Bottom])
        {
            w.docking.move_panel(
                *panel,
                Target::Dock {
                    place,
                    index: 0,
                    preview: origin,
                },
            );
        }
        let overlaps = |a: Box2, b: Box2| {
            a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
        };
        for width in [900, 1280, 1920] {
            for height in [600, 900] {
                let l = w.docking.layout(width, height, false, true, true);
                assert_eq!(l.regions.len(), 4);
                assert!(l.top.h > 0 && l.bottom.h > 0);
                assert!(l.center.h >= 240);
                for (i, a) in l.regions.iter().enumerate() {
                    assert!(!overlaps(a.bounds, l.center));
                    assert!(
                        a.bounds.x >= ACTIVITY_WIDTH
                            && a.bounds.x + a.bounds.w <= width
                            && a.bounds.y >= CHROME_TOP
                            && a.bounds.y + a.bounds.h <= height - CHROME_BOTTOM
                    );
                    for b in &l.regions[i + 1..] {
                        assert!(!overlaps(a.bounds, b.bounds));
                    }
                }
                let viewport = w.layout(width, height, false);
                assert_eq!(viewport.tabs.y, l.center.y);
                assert_eq!(viewport.canvas.y + viewport.canvas.h, l.bottom.y);
                assert_eq!(viewport.activity.h, height - CHROME_TOP - CHROME_BOTTOM);
            }
        }
        let saved = Workspace::decode(&w.encode());
        assert_eq!(saved.docking.panels[2].place, Place::Top);
        assert_eq!(saved.docking.panels[3].place, Place::Bottom);
        w.docking.left_width = 420;
        w.docking.move_panel(
            Panel::Brush,
            Target::Dock {
                place: Place::Top,
                index: 1,
                preview: origin,
            },
        );
        w.docking.move_panel(
            Panel::Layers,
            Target::Dock {
                place: Place::Top,
                index: 2,
                preview: origin,
            },
        );
        let l = w.docking.layout(900, 600, false, true, true);
        for r in l
            .regions
            .iter()
            .filter(|r| w.docking.panels[r.panel.index()].place == Place::Top)
        {
            assert!(r.bounds.w >= 160);
        }
        w.docking.move_panel(Panel::Brush, tab(Panel::Navigator, 1));
        assert_eq!(
            w.docking.root(Panel::Brush),
            w.docking.root(Panel::Navigator)
        );
        assert_eq!(w.docking.panels[1].place, Place::Top);
    }
    fn tab(with: Panel, index: usize) -> Target {
        Target::Tab {
            with,
            index,
            preview: Box2 {
                x: 0,
                y: 0,
                w: 100,
                h: 32,
            },
        }
    }
    #[test]
    fn tab_groups_support_selection_reorder_tear_off_and_whole_group_moves() {
        let mut d = Docking::default();
        d.enabled = true;
        d.move_panel(Panel::Brush, tab(Panel::Color, 1));
        assert_eq!(d.root(Panel::Brush), d.root(Panel::Color));
        assert_eq!(d.members(Panel::Color), vec![Panel::Color, Panel::Brush]);
        let l = d.layout(1280, 900, false, true, true);
        assert_eq!(l.regions.len(), 3);
        assert_eq!(
            l.regions
                .iter()
                .find(|r| r.contains_panel(Panel::Color))
                .unwrap()
                .panel,
            Panel::Brush
        );
        d.move_panel(Panel::Brush, tab(Panel::Color, 0));
        assert_eq!(d.members(Panel::Color), vec![Panel::Brush, Panel::Color]);
        d.select(Panel::Color);
        d.move_group(
            Panel::Color,
            Target::Float(Box2 {
                x: 400,
                y: 140,
                w: 320,
                h: 400,
            }),
        );
        assert_eq!(
            d.members(Panel::Color),
            vec![Panel::Brush, Panel::Color],
            "moving a group must retain tab order"
        );
        assert_eq!(d.panels[0].place, Place::Floating);
        assert_eq!(d.panels[1].floating.x, 400);
        d.move_panel(Panel::Color, tab(Panel::Navigator, 0));
        assert_ne!(d.root(Panel::Color), d.root(Panel::Brush));
        assert_eq!(
            d.members(Panel::Navigator),
            vec![Panel::Color, Panel::Navigator]
        );
        assert_eq!(d.active[d.root(Panel::Brush)], Panel::Brush);
        d.move_group(Panel::Navigator, tab(Panel::Layers, 0));
        assert_eq!(
            d.members(Panel::Layers),
            vec![Panel::Color, Panel::Navigator, Panel::Layers]
        );
        assert_eq!(d.active[d.root(Panel::Layers)], Panel::Navigator);
    }
    #[test]
    fn group_drop_regions_visibility_and_corner_resize_stay_consistent() {
        let mut d = Docking::default();
        d.enabled = true;
        d.move_panel(Panel::Brush, tab(Panel::Color, 1));
        let l = d.layout(1280, 900, false, true, true);
        let r = *l
            .regions
            .iter()
            .find(|r| r.contains_panel(Panel::Brush))
            .unwrap();
        let b = r.tab_bounds(Panel::Color).unwrap();
        assert!(matches!(
            d.target(
                Panel::Layers,
                b.x + 20,
                b.y + 12,
                r.bounds,
                (20, 12),
                1280,
                900,
                true,
                true
            ),
            Target::Tab { .. }
        ));
        assert_eq!(r.tab_at(b.x + 20, b.y + 12), Some(Panel::Color));
        d.move_panel(Panel::Layers, tab(Panel::Color, 2));
        d.select(Panel::Brush);
        let hidden = d.layout(1280, 900, false, false, true);
        let mixed = hidden
            .regions
            .iter()
            .find(|r| r.contains_panel(Panel::Layers))
            .unwrap();
        assert_eq!(mixed.panel, Panel::Layers);
        assert_eq!(mixed.tabs.iter().flatten().count(), 1);
        d.move_group(
            Panel::Layers,
            Target::Float(Box2 {
                x: 450,
                y: 180,
                w: 320,
                h: 400,
            }),
        );
        let l = d.layout(1280, 900, false, true, true);
        let r = *l
            .regions
            .iter()
            .find(|r| r.contains_panel(Panel::Layers))
            .unwrap();
        let b = r.bounds;
        assert_eq!(r.resize_edges(b.x, b.y), 5);
        assert_eq!(r.resize_edges(b.x + b.w - 1, b.y), 6);
        assert_eq!(r.resize_edges(b.x, b.y + b.h - 1), 9);
        assert_eq!(r.resize_edges(b.x + b.w - 1, b.y + b.h - 1), 10);
        assert_eq!(r.resize_edges(b.x + 80, b.y + 80), 0);
        assert!(r.content().y > b.y);
        assert!(r.content().x > b.x);
    }
    #[test]
    fn saved_groups_retain_members_active_tab_order_and_old_layout_compatibility() {
        let mut w = Workspace::default();
        w.enable_docking();
        w.docking.move_panel(Panel::Color, tab(Panel::Navigator, 0));
        w.docking.select(Panel::Navigator);
        let restored = Workspace::decode(&w.encode());
        assert_eq!(
            restored.docking.root(Panel::Color),
            restored.docking.root(Panel::Navigator)
        );
        assert_eq!(
            restored.docking.members(Panel::Color),
            vec![Panel::Color, Panel::Navigator]
        );
        assert_eq!(
            restored.docking.active[restored.docking.root(Panel::Color)],
            Panel::Navigator
        );
        let mut resumed = Workspace::decode(&w.encode());
        resumed.docking.enabled = false;
        let mut expected = resumed.docking.clone();
        expected.enabled = true;
        resumed.enable_docking();
        assert_eq!(resumed.docking, expected);
        let old =
            Workspace::decode("dock_enabled=true\ndock_navigator=float,0,300,500,160,300,300\n");
        assert_eq!(
            old.docking.members(Panel::Navigator),
            vec![Panel::Navigator]
        );
        assert_eq!(old.docking.panels[2].place, Place::Floating);
        let invalid =
            Workspace::decode("dock_groups=99,1,2,3\ndock_active=-1,1,2,3\ndock_tabs=7,0,0,0\n");
        assert_eq!(invalid.docking.groups, [0, 1, 2, 3]);
    }
    #[test]
    fn reorder_redock_float_and_configuration_round_trip() {
        let mut w = Workspace::default();
        w.enable_docking();
        let placeholder = Box2 {
            x: 0,
            y: 0,
            w: 240,
            h: 36,
        };
        w.docking.move_panel(
            Panel::Layers,
            Target::Dock {
                place: Place::Left,
                index: 0,
                preview: placeholder,
            },
        );
        let l = w.docking.layout(1280, 900, false, true, true);
        assert!(l.panel(Panel::Layers).unwrap().y < l.panel(Panel::Color).unwrap().y);
        w.docking.move_panel(
            Panel::Navigator,
            Target::Float(Box2 {
                x: 520,
                y: 180,
                w: 300,
                h: 260,
            }),
        );
        let restored = Workspace::decode(&w.encode());
        assert!(restored.docking.enabled);
        assert_eq!(
            restored.docking.panels[Panel::Layers.index()].place,
            Place::Left
        );
        let n = restored.docking.panels[Panel::Navigator.index()];
        assert_eq!(n.place, Place::Floating);
        assert_eq!((n.floating.x, n.floating.w), (520, 300));
        let l = restored.docking.layout(1280, 900, false, true, true);
        assert_eq!(l.regions.len(), 4);
        assert_eq!(l.right.w, 0);
        assert_eq!(l.top_at(550, 200).unwrap().panel, Panel::Navigator);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("dock-layout-{}.ini", std::process::id()));
        std::fs::write(&path, "theme=0\ntools_width=280\n").unwrap();
        let migrated = Workspace::load(&path);
        assert!(migrated.docking.enabled);
        assert_eq!(migrated.theme, 0);
        assert_eq!(migrated.docking.left_width, 280);
        std::fs::write(&path, "dock_enabled=false\ntheme=1\n").unwrap();
        assert!(!Workspace::load(&path).docking.enabled);
        w.save(&path).unwrap();
        assert!(Workspace::load(&path).docking.enabled);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn drop_targets_distinguish_column_edges_insertion_and_floating() {
        let mut d = Docking::default();
        d.enabled = true;
        let l = d.layout(1280, 900, false, true, true);
        let origin = l.panel(Panel::Brush).unwrap();
        assert!(matches!(
            d.target(
                Panel::Brush,
                60,
                90,
                origin,
                (20, 16),
                1280,
                900,
                true,
                true
            ),
            Target::Dock {
                place: Place::Left,
                index: 0,
                ..
            }
        ));
        assert!(matches!(
            d.target(
                Panel::Brush,
                1270,
                880,
                origin,
                (20, 16),
                1280,
                900,
                true,
                true
            ),
            Target::Dock {
                place: Place::Right,
                index: 2,
                ..
            }
        ));
        let target = d.target(
            Panel::Brush,
            650,
            300,
            origin,
            (20, 16),
            1280,
            900,
            true,
            true,
        );
        assert!(matches!(target, Target::Float(_)));
        d.move_panel(Panel::Brush, target);
        assert_eq!(d.panels[1].place, Place::Floating);
        let l = d.layout(900, 600, false, true, true);
        let r = l.panel(Panel::Brush).unwrap();
        assert!(
            r.x >= ACTIVITY_WIDTH
                && r.y >= CHROME_TOP
                && r.x + r.w <= 900
                && r.y + r.h <= 600 - CHROME_BOTTOM
        );
    }
    #[test]
    fn stacked_panels_never_overlap_and_hidden_groups_release_canvas_space() {
        for w in [900, 1280, 1920] {
            for h in [600, 900] {
                let mut d = Docking::default();
                d.enabled = true;
                for (i, p) in Panel::ALL.iter().enumerate() {
                    d.panels[p.index()].place = Place::Left;
                    d.panels[p.index()].order = i as u8;
                }
                let l = d.layout(w, h, false, true, true);
                assert_eq!(l.regions.len(), 4);
                for pair in l.regions.windows(2) {
                    assert_eq!(pair[0].bounds.y + pair[0].bounds.h + 6, pair[1].bounds.y);
                }
                for r in &l.regions {
                    assert!(r.bounds.h >= 96);
                    assert!(r.bounds.y + r.bounds.h <= h - CHROME_BOTTOM);
                }
                assert_eq!(d.layout(w, h, true, true, true).center.w, w);
                assert_eq!(
                    d.layout(w, h, false, false, false).center.w,
                    w - ACTIVITY_WIDTH
                );
            }
        }
        let mut d = Docking::default();
        d.decode_line("dock_brush", "float,0,350,-99999,99999,99999,99999");
        let r = d
            .layout(900, 600, false, true, true)
            .panel(Panel::Brush)
            .unwrap();
        assert!(r.x + r.w <= 900 && r.y + r.h <= 600 - CHROME_BOTTOM);
        d.decode_line("dock_layers", "unknown,1,2,3,4,5,6");
        assert_eq!(d.panels[3].place, Place::Right);
    }
}
impl Panel {
    pub const ALL: [Self; 4] = [Self::Color, Self::Brush, Self::Navigator, Self::Layers];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Color => "Color",
            Self::Brush => "Brush",
            Self::Navigator => "Navigator",
            Self::Layers => "Layers",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Brush => "brush",
            Self::Navigator => "navigator",
            Self::Layers => "layers",
        }
    }
    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Left,
    Right,
    Floating,
    Top,
    Bottom,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub place: Place,
    pub order: u8,
    pub weight: i32,
    pub floating: Box2,
}
pub const FLOAT_CAPTION: i32 = 24;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Docking {
    pub enabled: bool,
    pub panels: [State; 4],
    pub groups: [u8; 4],
    pub active: [Panel; 4],
    pub tab_order: [u8; 4],
    pub left_width: i32,
    pub right_width: i32,
    pub top_height: i32,
    pub bottom_height: i32,
}
impl Default for Docking {
    fn default() -> Self {
        let state = |place, order, weight, x, y, w, h| State {
            place,
            order,
            weight,
            floating: Box2 { x, y, w, h },
        };
        Self {
            enabled: false,
            groups: [0, 1, 2, 3],
            active: Panel::ALL,
            tab_order: [0, 1, 2, 3],
            left_width: 240,
            right_width: 240,
            top_height: 200,
            bottom_height: 200,
            panels: [
                state(Place::Left, 0, 350, 80, 80, 240, 350),
                state(Place::Left, 1, 450, 120, 120, 260, 450),
                state(Place::Right, 0, 230, 700, 80, 240, 230),
                state(Place::Right, 1, 570, 740, 160, 260, 440),
            ],
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub panel: Panel,
    pub bounds: Box2,
    pub floating: bool,
    pub tabs: [Option<Panel>; 4],
}
impl Region {
    pub fn contains_panel(self, p: Panel) -> bool {
        self.tabs.iter().any(|t| *t == Some(p))
    }
    pub fn content(self) -> Box2 {
        let b = self.bounds;
        if self.floating {
            Box2 {
                x: b.x + 2,
                y: b.y + FLOAT_CAPTION,
                w: (b.w - 4).max(1),
                h: (b.h - FLOAT_CAPTION - 2).max(1),
            }
        } else {
            b
        }
    }
    pub fn tab_bounds(self, p: Panel) -> Option<Box2> {
        let b = self.content();
        let count = self.tabs.iter().flatten().count().max(1) as i32;
        let width = ((b.w - 40) / count).max(36).min(88);
        self.tabs.iter().position(|v| *v == Some(p)).map(|i| Box2 {
            x: b.x + 5 + i as i32 * width,
            y: b.y + 1,
            w: width,
            h: super::workspace::PANEL_HEADER - 2,
        })
    }
    pub fn tab_at(self, x: i32, y: i32) -> Option<Panel> {
        self.tabs
            .iter()
            .flatten()
            .copied()
            .find(|p| self.tab_bounds(*p).unwrap().contains(x, y))
    }
    pub fn resize_edges(self, x: i32, y: i32) -> u8 {
        if !self.floating || !self.bounds.contains(x, y) {
            return 0;
        }
        let b = self.bounds;
        let corner = 14;
        let band = 4;
        let mut edges = 0;
        if x < b.x + band {
            edges |= 1;
        }
        if x >= b.x + b.w - band {
            edges |= 2;
        }
        if y < b.y + band {
            edges |= 4;
        }
        if y >= b.y + b.h - band {
            edges |= 8;
        }
        if x >= b.x + b.w - corner && y >= b.y + b.h - corner {
            edges = 2 | 8;
        }
        edges
    }
}
pub struct DockLayout {
    pub regions: Vec<Region>,
    pub left: Box2,
    pub right: Box2,
    pub center: Box2,
    pub top: Box2,
    pub bottom: Box2,
}
impl DockLayout {
    pub fn panel(&self, panel: Panel) -> Option<Box2> {
        self.regions
            .iter()
            .find(|r| r.contains_panel(panel))
            .map(|r| r.bounds)
    }
    pub fn top_at(&self, x: i32, y: i32) -> Option<Region> {
        self.regions
            .iter()
            .rev()
            .find(|r| r.bounds.contains(x, y))
            .copied()
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Target {
    Dock {
        place: Place,
        index: usize,
        preview: Box2,
    },
    Float(Box2),
    Tab {
        with: Panel,
        index: usize,
        preview: Box2,
    },
}
impl Target {
    pub fn preview(self) -> Box2 {
        match self {
            Self::Dock { preview, .. } | Self::Tab { preview, .. } | Self::Float(preview) => {
                preview
            }
        }
    }
}
impl Docking {
    pub fn root(&self, p: Panel) -> usize {
        self.groups[p.index()] as usize
    }
    pub fn members(&self, p: Panel) -> Vec<Panel> {
        let root = self.root(p);
        let mut m: Vec<_> = Panel::ALL
            .iter()
            .copied()
            .filter(|p| self.root(*p) == root)
            .collect();
        m.sort_by_key(|p| self.tab_order[p.index()]);
        m
    }
    pub fn select(&mut self, p: Panel) {
        let root = self.root(p);
        self.active[root] = p;
    }
    pub fn sync(&mut self, p: Panel) {
        let state = self.panels[p.index()];
        let root = self.root(p);
        for member in Panel::ALL {
            if self.root(member) == root {
                self.panels[member.index()] = state;
            }
        }
    }
    pub fn normalize_groups(&mut self) {
        let old = self.groups;
        let active = self.active;
        for p in Panel::ALL {
            let label = old[p.index()].min(3);
            let root = Panel::ALL
                .iter()
                .find(|m| old[m.index()].min(3) == label)
                .unwrap()
                .index();
            self.groups[p.index()] = root as u8;
            self.active[root] = if old[active[label as usize].index()].min(3) == label {
                active[label as usize]
            } else {
                Panel::ALL[root]
            };
        }
        for p in Panel::ALL {
            if self.root(p) == p.index() {
                self.sync(p);
            }
        }
    }
    fn detach(&mut self, p: Panel) {
        let old_root = self.root(p);
        let others: Vec<_> = self.members(p).into_iter().filter(|m| *m != p).collect();
        if let Some(first) = others.first() {
            let root = first.index();
            let selected = if self.active[old_root] == p {
                *first
            } else {
                self.active[old_root]
            };
            for m in others {
                self.groups[m.index()] = root as u8;
            }
            self.active[root] = selected;
        }
        self.groups[p.index()] = p.index() as u8;
        self.active[p.index()] = p;
        self.tab_order[p.index()] = 0;
    }
    fn join(&mut self, p: Panel, with: Panel, index: usize) {
        let same = self.root(p) == self.root(with);
        if !same {
            self.detach(p);
        }
        let root = self.root(with);
        let mut members = self.members(with);
        members.retain(|m| *m != p);
        members.insert(index.min(members.len()), p);
        let state = self.panels[with.index()];
        for (i, m) in members.iter().enumerate() {
            self.groups[m.index()] = root as u8;
            self.panels[m.index()] = state;
            self.tab_order[m.index()] = i as u8;
        }
        self.active[root] = p;
    }
    pub fn move_group(&mut self, p: Panel, target: Target) {
        let members = self.members(p);
        if let Target::Tab { with, index, .. } = target {
            if self.root(with) == self.root(p) {
                return;
            }
            for (i, m) in members.iter().enumerate() {
                self.join(*m, with, index + i);
            }
        } else {
            let first = members[0];
            self.move_panel(first, target);
            for m in members.iter().skip(1) {
                let end = self.members(first).len();
                self.join(*m, first, end);
            }
        }
        self.select(p);
    }
    pub fn raise(&mut self, p: Panel) {
        if self.panels[p.index()].place != Place::Floating {
            return;
        }
        let mut roots: Vec<_> = Panel::ALL
            .iter()
            .copied()
            .filter(|m| {
                self.root(*m) == m.index() && self.panels[m.index()].place == Place::Floating
            })
            .collect();
        roots.sort_by_key(|m| self.panels[m.index()].order);
        roots.retain(|m| self.root(*m) != self.root(p));
        roots.push(p);
        for (i, m) in roots.iter().enumerate() {
            self.panels[m.index()].order = i as u8;
            self.sync(*m);
        }
    }
    pub fn swap(&mut self) {
        for p in &mut self.panels {
            p.place = match p.place {
                Place::Left => Place::Right,
                Place::Right => Place::Left,
                Place::Floating => Place::Floating,
                Place::Top => Place::Top,
                Place::Bottom => Place::Bottom,
            };
        }
        std::mem::swap(&mut self.left_width, &mut self.right_width);
    }
    pub fn visible(panel: Panel, tools: bool, layers: bool) -> bool {
        match panel {
            Panel::Color | Panel::Brush => tools,
            _ => layers,
        }
    }
    pub fn columns(
        &self,
        w: i32,
        h: i32,
        focus: bool,
        tools: bool,
        layers: bool,
    ) -> (Box2, Box2, Box2) {
        let activity = if focus { 0 } else { ACTIVITY_WIDTH };
        let top = CHROME_TOP;
        let height = (h - top - CHROME_BOTTOM).max(1);
        let present = |place| {
            !focus
                && Panel::ALL.iter().any(|p| {
                    Self::visible(*p, tools, layers) && self.panels[p.index()].place == place
                })
        };
        let has_left = present(Place::Left);
        let has_right = present(Place::Right);
        let mut top_groups = 0u8;
        let mut bottom_groups = 0u8;
        if !focus {
            for p in Panel::ALL {
                if Self::visible(p, tools, layers) {
                    match self.panels[p.index()].place {
                        Place::Top => top_groups |= 1 << self.root(p),
                        Place::Bottom => bottom_groups |= 1 << self.root(p),
                        _ => {}
                    }
                }
            }
        }
        let count = top_groups.count_ones().max(bottom_groups.count_ones()) as i32;
        let center_min = (count * 160 + (count - 1).max(0) * 6).max(240);
        let available = (w - activity - center_min).max(0);
        let cap = if has_left && has_right {
            available / 2
        } else {
            available
        };
        let lw = if has_left {
            self.left_width.min(cap).max(1)
        } else {
            0
        };
        let rw = if has_right {
            self.right_width.min(cap).max(1)
        } else {
            0
        };
        let has_top = present(Place::Top);
        let has_bottom = present(Place::Bottom);
        let space = (height - 240).max(0);
        let row_cap = if has_top && has_bottom {
            space / 2
        } else {
            space
        };
        let th = if has_top {
            self.top_height.min(row_cap).max(1)
        } else {
            0
        };
        let bh = if has_bottom {
            self.bottom_height.min(row_cap).max(1)
        } else {
            0
        };
        (
            Box2 {
                x: activity,
                y: top,
                w: lw,
                h: height,
            },
            Box2 {
                x: w - rw,
                y: top,
                w: rw,
                h: height,
            },
            Box2 {
                x: activity + lw,
                y: top + th,
                w: (w - activity - lw - rw).max(1),
                h: (height - th - bh).max(1),
            },
        )
    }
    pub fn floating_at(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        focus: bool,
        tools: bool,
        layers: bool,
    ) -> bool {
        !focus
            && Panel::ALL.iter().any(|p| {
                Self::visible(*p, tools, layers)
                    && self.panels[p.index()].place == Place::Floating
                    && self
                        .clamp_float(self.panels[p.index()].floating, w, h)
                        .contains(x, y)
            })
    }
    pub fn layout(&self, w: i32, h: i32, focus: bool, tools: bool, layers: bool) -> DockLayout {
        let mut groups: [Vec<Panel>; 4] = [vec![], vec![], vec![], vec![]];
        let mut floating = vec![];
        let mut tab_sets = [[None; 4]; 4];
        if !focus {
            for root in 0..4 {
                let mut members: Vec<_> = Panel::ALL
                    .iter()
                    .copied()
                    .filter(|p| self.root(*p) == root && Self::visible(*p, tools, layers))
                    .collect();
                if members.is_empty() {
                    continue;
                }
                members.sort_by_key(|p| self.tab_order[p.index()]);
                let p = if members.contains(&self.active[root]) {
                    self.active[root]
                } else {
                    members[0]
                };
                for (i, m) in members.iter().enumerate() {
                    tab_sets[p.index()][i] = Some(*m);
                }
                match self.panels[p.index()].place {
                    Place::Left => groups[0].push(p),
                    Place::Right => groups[1].push(p),
                    Place::Floating => floating.push(p),
                    Place::Top => groups[2].push(p),
                    Place::Bottom => groups[3].push(p),
                }
            }
        }
        for g in &mut groups {
            g.sort_by_key(|p| self.panels[p.index()].order);
        }
        let (left, right, center) = self.columns(w, h, focus, tools, layers);
        let top_row = Box2 {
            x: center.x,
            y: CHROME_TOP,
            w: center.w,
            h: center.y - CHROME_TOP,
        };
        let bottom_row = Box2 {
            x: center.x,
            y: center.y + center.h,
            w: center.w,
            h: h - CHROME_BOTTOM - center.y - center.h,
        };
        let mut regions = vec![];
        for (side, (g, area)) in groups
            .iter()
            .zip([left, right, top_row, bottom_row])
            .enumerate()
        {
            let horizontal = side >= 2;
            let length = if horizontal { area.w } else { area.h };
            let mut remaining = length - (g.len().saturating_sub(1) as i32 * 6);
            let mut weight = g
                .iter()
                .map(|p| self.panels[p.index()].weight)
                .sum::<i32>()
                .max(1);
            let mut position = if horizontal { area.x } else { area.y };
            for (i, p) in g.iter().enumerate() {
                let rest = (g.len() - i - 1) as i32;
                let minimum = if horizontal {
                    160.min(((length - (g.len() - 1) as i32 * 6) / g.len() as i32).max(1))
                } else {
                    96.min((length / g.len() as i32 - 6).max(1))
                };
                let wanted = (remaining * self.panels[p.index()].weight / weight)
                    .max(minimum)
                    .min((remaining - rest * minimum).max(1));
                let span = if rest == 0 { remaining.max(1) } else { wanted };
                let bounds = if horizontal {
                    Box2 {
                        x: position,
                        y: area.y,
                        w: span,
                        h: area.h,
                    }
                } else {
                    Box2 {
                        x: area.x,
                        y: position,
                        w: area.w,
                        h: span,
                    }
                };
                regions.push(Region {
                    panel: *p,
                    tabs: tab_sets[p.index()],
                    bounds,
                    floating: false,
                });
                remaining -= span;
                weight -= self.panels[p.index()].weight;
                position += span + 6;
            }
        }
        floating.sort_by_key(|p| self.panels[p.index()].order);
        for p in floating {
            let r = self.clamp_float(self.panels[p.index()].floating, w, h);
            regions.push(Region {
                panel: p,
                tabs: tab_sets[p.index()],
                bounds: r,
                floating: true,
            });
        }
        DockLayout {
            regions,
            left,
            right,
            center,
            top: top_row,
            bottom: bottom_row,
        }
    }
    pub fn clamp_float(&self, mut r: Box2, w: i32, h: i32) -> Box2 {
        r.w = r.w.max(224).min(800).min((w - ACTIVITY_WIDTH - 8).max(1));
        r.h =
            r.h.max(150)
                .min((h - CHROME_TOP - CHROME_BOTTOM - 8).max(1));
        r.x =
            r.x.max(ACTIVITY_WIDTH + 4)
                .min((w - r.w - 4).max(ACTIVITY_WIDTH));
        r.y =
            r.y.max(CHROME_TOP + 4)
                .min((h - CHROME_BOTTOM - r.h - 4).max(CHROME_TOP));
        r
    }
    pub fn target(
        &self,
        panel: Panel,
        x: i32,
        y: i32,
        origin: Box2,
        grab: (i32, i32),
        w: i32,
        h: i32,
        tools: bool,
        layers: bool,
    ) -> Target {
        let layout = self.layout(w, h, false, tools, layers);
        if let Some(r) = layout.top_at(x, y) {
            let b = r.content();
            if y >= b.y
                && y < b.y + super::workspace::PANEL_HEADER
                && (self.root(r.panel) != self.root(panel) || r.tabs.iter().flatten().count() > 1)
            {
                let others: Vec<_> = r
                    .tabs
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|p| *p != panel)
                    .collect();
                let index = others
                    .iter()
                    .take_while(|p| {
                        let t = r.tab_bounds(**p).unwrap();
                        x > t.x + t.w / 2
                    })
                    .count();
                return Target::Tab {
                    with: r.panel,
                    index,
                    preview: Box2 {
                        x: b.x,
                        y: b.y,
                        w: b.w,
                        h: super::workspace::PANEL_HEADER,
                    },
                };
            }
        }
        let hovered = layout.regions.iter().find(|r| {
            !r.floating && self.root(r.panel) != self.root(panel) && r.bounds.contains(x, y)
        });
        let side = if x < ACTIVITY_WIDTH + 64 {
            Some(Place::Left)
        } else if x > w - 64 {
            Some(Place::Right)
        } else if y < CHROME_TOP + 48 {
            Some(Place::Top)
        } else if y > h - CHROME_BOTTOM - 48 {
            Some(Place::Bottom)
        } else {
            hovered.map(|r| self.panels[r.panel.index()].place)
        };
        if let Some(place) = side {
            let mut others: Vec<_> = layout
                .regions
                .iter()
                .filter(|r| {
                    !r.floating
                        && self.root(r.panel) != self.root(panel)
                        && self.panels[r.panel.index()].place == place
                })
                .collect();
            let horizontal = matches!(place, Place::Top | Place::Bottom);
            others.sort_by_key(|r| if horizontal { r.bounds.x } else { r.bounds.y });
            let index = others
                .iter()
                .take_while(|r| {
                    if horizontal {
                        x > r.bounds.x + r.bounds.w / 2
                    } else {
                        y > r.bounds.y + r.bounds.h / 2
                    }
                })
                .count();
            let col = match place {
                Place::Left => layout.left,
                Place::Right => layout.right,
                Place::Top => layout.top,
                Place::Bottom => layout.bottom,
                _ => unreachable!(),
            };
            let preview = if others.is_empty() {
                if horizontal {
                    let height = if place == Place::Top {
                        self.top_height
                    } else {
                        self.bottom_height
                    }
                    .min((h - CHROME_TOP - CHROME_BOTTOM - 240).max(1));
                    Box2 {
                        x: layout.center.x,
                        y: if place == Place::Top {
                            CHROME_TOP
                        } else {
                            h - CHROME_BOTTOM - height
                        },
                        w: layout.center.w,
                        h: height,
                    }
                } else {
                    let width = if col.w == 0 {
                        if place == Place::Left {
                            self.left_width
                        } else {
                            self.right_width
                        }
                    } else {
                        col.w
                    };
                    Box2 {
                        x: if place == Place::Left {
                            ACTIVITY_WIDTH
                        } else {
                            w - width
                        },
                        y: CHROME_TOP,
                        w: width,
                        h: h - CHROME_TOP - CHROME_BOTTOM,
                    }
                }
            } else {
                let r = others[index.min(others.len() - 1)].bounds;
                if horizontal {
                    Box2 {
                        x: if index < others.len() {
                            r.x
                        } else {
                            r.x + r.w - 36
                        },
                        y: r.y,
                        w: 36,
                        h: r.h,
                    }
                } else {
                    Box2 {
                        x: r.x,
                        y: if index < others.len() {
                            r.y
                        } else {
                            r.y + r.h - 36
                        },
                        w: r.w,
                        h: 36,
                    }
                }
            };
            Target::Dock {
                place,
                index,
                preview,
            }
        } else {
            Target::Float(self.clamp_float(
                Box2 {
                    x: x - grab.0,
                    y: y - grab.1,
                    w: origin.w.max(224),
                    h: origin.h.max(180).min(500),
                },
                w,
                h,
            ))
        }
    }
    pub fn move_panel(&mut self, panel: Panel, target: Target) {
        self.enabled = true;
        if let Target::Tab { with, index, .. } = target {
            self.join(panel, with, index);
            return;
        }
        self.detach(panel);
        match target {
            Target::Float(r) => {
                let top = self
                    .panels
                    .iter()
                    .map(|p| p.order)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1);
                let p = &mut self.panels[panel.index()];
                p.place = Place::Floating;
                p.floating = r;
                p.order = top;
            }
            Target::Dock { place, index, .. } => {
                let mut ordered: Vec<_> = Panel::ALL
                    .iter()
                    .copied()
                    .filter(|p| {
                        *p != panel
                            && self.root(*p) == p.index()
                            && self.panels[p.index()].place == place
                    })
                    .collect();
                ordered.sort_by_key(|p| self.panels[p.index()].order);
                ordered.insert(index.min(ordered.len()), panel);
                for (i, p) in ordered.iter().enumerate() {
                    self.panels[p.index()].place = place;
                    self.panels[p.index()].order = i as u8;
                    self.sync(*p);
                }
            }
            Target::Tab { .. } => unreachable!(),
        }
    }
    pub fn encode(&self) -> String {
        let mut s = format!(
            "dock_enabled={}\ndock_left_width={}\ndock_right_width={}\n",
            self.enabled, self.left_width, self.right_width
        );
        s.push_str(&format!(
            "dock_groups={},{},{},{}\ndock_active={},{},{},{}\ndock_tabs={},{},{},{}\n",
            self.groups[0],
            self.groups[1],
            self.groups[2],
            self.groups[3],
            self.active[0].index(),
            self.active[1].index(),
            self.active[2].index(),
            self.active[3].index(),
            self.tab_order[0],
            self.tab_order[1],
            self.tab_order[2],
            self.tab_order[3]
        ));
        s.push_str(&format!(
            "dock_top_height={}\ndock_bottom_height={}\n",
            self.top_height, self.bottom_height
        ));
        for p in Panel::ALL {
            let a = self.panels[p.index()];
            let r = a.floating;
            s.push_str(&format!(
                "dock_{}={},{},{},{},{},{},{}\n",
                p.key(),
                match a.place {
                    Place::Left => "left",
                    Place::Right => "right",
                    Place::Floating => "float",
                    Place::Top => "top",
                    Place::Bottom => "bottom",
                },
                a.order,
                a.weight,
                r.x,
                r.y,
                r.w,
                r.h
            ));
        }
        s
    }
    pub fn decode_line(&mut self, k: &str, v: &str) {
        if matches!(k, "dock_groups" | "dock_active" | "dock_tabs") {
            let values: Result<Vec<usize>, _> = v.split(',').map(|v| v.parse()).collect();
            if let Ok(a) = values {
                if a.len() == 4 && a.iter().all(|v| *v < 4) {
                    for i in 0..4 {
                        match k {
                            "dock_groups" => self.groups[i] = a[i] as u8,
                            "dock_active" => self.active[i] = Panel::ALL[a[i]],
                            _ => self.tab_order[i] = a[i] as u8,
                        }
                    }
                }
            }
            return;
        }
        if matches!(k, "dock_top_height" | "dock_bottom_height") {
            if let Ok(n) = v.parse::<i32>() {
                let n = n.max(96).min(420);
                if k == "dock_top_height" {
                    self.top_height = n
                } else {
                    self.bottom_height = n
                }
            }
            return;
        }
        match k {
            "dock_enabled" => {
                if let Ok(b) = v.parse() {
                    self.enabled = b;
                }
                return;
            }
            "dock_left_width" | "dock_right_width" => {
                if let Ok(n) = v.parse::<i32>() {
                    let n = n.max(224).min(420);
                    if k == "dock_left_width" {
                        self.left_width = n
                    } else {
                        self.right_width = n
                    }
                }
                return;
            }
            _ => {}
        }
        if let Some(panel) = Panel::ALL.iter().find(|p| k == format!("dock_{}", p.key())) {
            let values: Vec<_> = v.split(',').collect();
            if values.len() != 7 {
                return;
            }
            let place = match values[0] {
                "left" => Place::Left,
                "right" => Place::Right,
                "float" => Place::Floating,
                "top" => Place::Top,
                "bottom" => Place::Bottom,
                _ => return,
            };
            let nums: Result<Vec<i32>, _> = values[1..].iter().map(|s| s.parse()).collect();
            if let Ok(n) = nums {
                self.panels[panel.index()] = State {
                    place,
                    order: n[0].max(0).min(250) as u8,
                    weight: n[1].max(1).min(2000),
                    floating: Box2 {
                        x: n[2].max(-10000).min(10000),
                        y: n[3].max(-10000).min(10000),
                        w: n[4].max(224).min(800),
                        h: n[5].max(150).min(1200),
                    },
                };
            }
        }
    }
}
