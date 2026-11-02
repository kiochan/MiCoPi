//! Small retained workspace state, with immediate native widgets in the shell.
use crate::engine::input::Dynamics;
use std::{
    io,
    path::{Path, PathBuf},
};
pub const CHROME_TOP: i32 = 36;
pub const CHROME_BOTTOM: i32 = 24;
pub const DOCUMENT_TABS: i32 = 34;
pub const ACTIVITY_WIDTH: i32 = 48;
pub const CANVAS_TOOLBAR: i32 = 34;
pub const PANEL_HEADER: i32 = 32;
pub fn layer_rows(height: i32) -> usize {
    ((height - PANEL_HEADER - 100 - 304) / 56).max(1) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Box2 {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Box2 {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}
pub struct Layout {
    pub activity: Box2,
    pub tabs: Box2,
    pub toolbar: Box2,
    pub tools: Box2,
    pub layers: Box2,
    pub canvas: Box2,
}
#[derive(Clone, Copy)]
pub struct Theme {
    pub background: u32,
    pub panel: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub button: u32,
    pub selected: u32,
}
impl Theme {
    pub fn preset(id: u8) -> Self {
        match id {
            1 => Self {
                background: 0xe5e1d8,
                panel: 0xf6f3ec,
                text: 0x252b35,
                muted: 0x626b78,
                accent: 0x28735b,
                button: 0xfffdf8,
                selected: 0xe0ebdf,
            },
            2 => Self {
                background: 0x232a30,
                panel: 0x1c2227,
                text: 0xe3e9f0,
                muted: 0xa0acbb,
                accent: 0x58b7de,
                button: 0x293139,
                selected: 0x303e49,
            },
            _ => Self {
                background: 0xe5e9ef,
                panel: 0xf5f7f9,
                text: 0x2b3948,
                muted: 0x758394,
                accent: 0x13877c,
                button: 0xffffff,
                selected: 0xe0f1ed,
            },
        }
    }
}
pub struct Workspace {
    pub docking: super::docking::Docking,
    pub tools_page: u8,
    pub tools_width: i32,
    pub layers_width: i32,
    pub swapped: bool,
    pub tools_visible: bool,
    pub layers_visible: bool,
    pub theme: u8,
    pub accent: Option<u32>,
    pub stabilizer: f32,
    pub dynamics: Dynamics,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            docking: Default::default(),
            tools_page: 0,
            tools_width: 224,
            layers_width: 208,
            swapped: false,
            tools_visible: true,
            layers_visible: true,
            theme: 2,
            accent: None,
            stabilizer: 0.35,
            dynamics: Dynamics::default(),
        }
    }
}
impl Workspace {
    /// Match the same splitter band for hover feedback and drag initiation.
    pub fn splitter_at(&self, w: i32, h: i32, focus: bool, x: i32, y: i32) -> Option<u8> {
        let l = self.layout(w, h, focus);
        for (i, p) in [l.tools, l.layers].iter().enumerate() {
            let edge = if p.x == l.activity.w { p.x + p.w } else { p.x };
            if p.w > 0 && y >= p.y && y < p.y + p.h && (x - edge).abs() <= 5 {
                return Some(i as u8);
            }
        }
        None
    }
    pub fn theme(&self) -> Theme {
        let mut t = Theme::preset(self.theme);
        if let Some(c) = self.accent {
            t.accent = c;
        }
        t
    }
    pub fn enable_docking(&mut self) {
        if !self.docking.enabled {
            let pristine = self.docking == super::docking::Docking::default();
            self.docking.enabled = true;
            if pristine {
                self.docking.left_width = self.tools_width.max(224);
                self.docking.right_width = self.layers_width.max(224);
                if self.swapped {
                    self.docking.swap();
                }
            }
        }
    }
    pub fn layout(&self, w: i32, h: i32, focus: bool) -> Layout {
        if self.docking.enabled {
            let (left, right, c) =
                self.docking
                    .columns(w, h, focus, self.tools_visible, self.layers_visible);
            return Layout {
                activity: Box2 {
                    x: 0,
                    y: CHROME_TOP,
                    w: if focus { 0 } else { ACTIVITY_WIDTH },
                    h: (h - CHROME_TOP - CHROME_BOTTOM).max(1),
                },
                tools: left,
                layers: right,
                tabs: Box2 {
                    x: c.x,
                    y: c.y,
                    w: c.w,
                    h: DOCUMENT_TABS,
                },
                toolbar: Box2 {
                    x: c.x,
                    y: c.y + DOCUMENT_TABS,
                    w: c.w,
                    h: CANVAS_TOOLBAR,
                },
                canvas: Box2 {
                    x: c.x,
                    y: c.y + DOCUMENT_TABS + CANVAS_TOOLBAR,
                    w: c.w,
                    h: (c.h - DOCUMENT_TABS - CANVAS_TOOLBAR).max(1),
                },
            };
        }

        let tw = if self.tools_visible && !focus {
            self.tools_width
        } else {
            0
        };
        let lw = if self.layers_visible && !focus {
            self.layers_width
        } else {
            0
        };
        let activity = if focus { 0 } else { ACTIVITY_WIDTH };
        let (left, right) = if self.swapped { (lw, tw) } else { (tw, lw) };
        let width = (w - activity - left - right).max(1);
        let content_h = (h - CHROME_TOP - CHROME_BOTTOM).max(1);
        Layout {
            activity: Box2 {
                x: 0,
                y: CHROME_TOP,
                w: activity,
                h: content_h,
            },
            tools: Box2 {
                x: if self.swapped { w - tw } else { activity },
                y: CHROME_TOP,
                w: tw,
                h: content_h,
            },
            layers: Box2 {
                x: if self.swapped { activity } else { w - lw },
                y: CHROME_TOP,
                w: lw,
                h: content_h,
            },
            tabs: Box2 {
                x: activity + left,
                y: CHROME_TOP,
                w: width,
                h: DOCUMENT_TABS,
            },
            toolbar: Box2 {
                x: activity + left,
                y: CHROME_TOP + DOCUMENT_TABS,
                w: width,
                h: CANVAS_TOOLBAR,
            },
            canvas: Box2 {
                x: activity + left,
                y: CHROME_TOP + DOCUMENT_TABS + CANVAS_TOOLBAR,
                w: width,
                h: (content_h - DOCUMENT_TABS - CANVAS_TOOLBAR).max(1),
            },
        }
    }
    pub fn encode(&self) -> String {
        let mut text=format!("# MiCoPi workspace v1\ntools_page={}\ntools_width={}\nlayers_width={}\nswapped={}\ntools_visible={}\nlayers_visible={}\ntheme={}\naccent={}\nstabilizer={}\npressure_size={}\npressure_opacity={}\npressure_gamma={}\npressure_minimum={}\n",
        self.tools_page,self.tools_width,self.layers_width,self.swapped,self.tools_visible,self.layers_visible,self.theme,
        self.accent.map(|v|format!("{:06x}",v)).unwrap_or_else(||"auto".into()),self.stabilizer,self.dynamics.size,self.dynamics.opacity,self.dynamics.gamma,self.dynamics.minimum);
        text.push_str(&self.docking.encode());
        text
    }
    pub fn decode(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let (k, v) = match line.split_once('=') {
                Some((k, v)) => (k.trim(), v.trim()),
                None => continue,
            };
            match k {
                "tools_page" => {
                    if let Ok(n) = v.parse::<u8>() {
                        s.tools_page = n.min(1);
                    }
                }
                "tools_width" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.tools_width = n.max(224).min(360);
                    }
                }
                "layers_width" => {
                    if let Ok(n) = v.parse::<i32>() {
                        s.layers_width = n.max(208).min(360);
                    }
                }
                "swapped" => {
                    if let Ok(b) = v.parse() {
                        s.swapped = b;
                    }
                }
                "tools_visible" => {
                    if let Ok(b) = v.parse() {
                        s.tools_visible = b;
                    }
                }
                "layers_visible" => {
                    if let Ok(b) = v.parse() {
                        s.layers_visible = b;
                    }
                }
                "theme" => {
                    if let Ok(n) = v.parse::<u8>() {
                        s.theme = n.min(2);
                    }
                }
                "accent" => {
                    if v.len() == 6 {
                        s.accent = u32::from_str_radix(v, 16).ok();
                    }
                }
                "stabilizer" => {
                    if let Ok(n) = v.parse::<f32>() {
                        if n.is_finite() {
                            s.stabilizer = n.max(0.).min(1.);
                        }
                    }
                }
                "pressure_size" => {
                    if let Ok(b) = v.parse() {
                        s.dynamics.size = b;
                    }
                }
                "pressure_opacity" => {
                    if let Ok(b) = v.parse() {
                        s.dynamics.opacity = b;
                    }
                }
                "pressure_gamma" => {
                    if let Ok(n) = v.parse::<f32>() {
                        if n.is_finite() {
                            s.dynamics.gamma = n.max(0.25).min(4.);
                        }
                    }
                }
                "pressure_minimum" => {
                    if let Ok(n) = v.parse::<f32>() {
                        if n.is_finite() {
                            s.dynamics.minimum = n.max(0.01).min(1.);
                        }
                    }
                }
                _ => s.docking.decode_line(k, v),
            }
        }
        s.docking.normalize_groups();
        s
    }
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut workspace = Self::decode(&text);
        if !text.lines().any(|line| {
            line.split_once('=')
                .map_or(false, |(key, _)| key.trim() == "dock_enabled")
        }) {
            workspace.enable_docking();
        }
        workspace
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension("ini.tmp");
        std::fs::write(&tmp, self.encode())?;
        crate::platform::replace_file(&tmp, path)
    }
    pub fn path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
            .join("MiCoPi.workspace.ini")
    }
}

/// One source for widget drawing and pointer geometry in the scrollable toolbox.
pub struct ToolsGeometry {
    pub wheel: i32,
    pub rgb: i32,
    pub palette: i32,
    pub tools: i32,
    pub presets: i32,
    pub parameters: i32,
    pub pressure: i32,
    pub curve: i32,
    pub sizes: i32,
    pub height: i32,
}
impl ToolsGeometry {
    pub fn new(width: i32) -> Self {
        let wheel = (width - 36).min(160);
        let rgb = wheel + 20;
        let palette = rgb + 62;
        let tools = palette + 72;
        let presets = tools + 42;
        let parameters = presets + 132;
        let pressure = parameters + 118;
        let curve = pressure + 32;
        let sizes = curve + 38;
        Self {
            wheel,
            rgb,
            palette,
            tools,
            presets,
            parameters,
            pressure,
            curve,
            sizes,
            height: sizes + 80,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_shell_regions_tile_the_window_and_follow_visibility() {
        for width in [900, 1280, 1920] {
            for height in [600, 900] {
                for swapped in [false, true] {
                    for focus in [false, true] {
                        for tools in [false, true] {
                            for layers in [false, true] {
                                let mut w = Workspace::default();
                                w.swapped = swapped;
                                w.tools_visible = tools;
                                w.layers_visible = layers;
                                let l = w.layout(width, height, focus);
                                assert_eq!(l.canvas.y, l.toolbar.y + l.toolbar.h);
                                assert_eq!(l.toolbar.y, l.tabs.y + l.tabs.h);
                                assert_eq!(l.canvas.y + l.canvas.h, height - CHROME_BOTTOM);
                                assert_eq!(
                                    l.activity.w + l.tools.w + l.canvas.w + l.layers.w,
                                    width
                                );
                                assert_eq!(l.tabs.x, l.canvas.x);
                                assert_eq!(l.toolbar.w, l.canvas.w);
                                if focus {
                                    assert_eq!(l.activity.w, 0);
                                    assert_eq!(l.canvas.x, 0);
                                } else {
                                    assert_eq!(l.activity.w, ACTIVITY_WIDTH);
                                    assert!(l.canvas.x >= ACTIVITY_WIDTH);
                                }
                                for p in [l.tools, l.layers] {
                                    if p.w > 0 {
                                        assert!(p.x >= l.activity.w);
                                        let edge =
                                            if p.x == l.activity.w { p.x + p.w } else { p.x };
                                        assert!(w
                                            .splitter_at(width, height, focus, edge, 200)
                                            .is_some());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        let saved = Workspace::decode("tools_page=1\ntheme=0");
        let restored = Workspace::decode(&saved.encode());
        assert_eq!(restored.tools_page, 1);
        assert_eq!(restored.theme, 0);
    }
    #[test]
    fn splitter_hit_regions_follow_side_visibility_and_height() {
        let mut s = Workspace::default();
        assert_eq!(s.splitter_at(1280, 900, false, 272, 200), Some(0));
        assert_eq!(s.splitter_at(1280, 900, false, 1072, 200), Some(1));
        assert_eq!(s.splitter_at(1280, 900, false, 272, 20), None);
        assert_eq!(s.splitter_at(1280, 900, true, 272, 200), None);
        s.swapped = true;
        assert_eq!(s.splitter_at(1280, 900, false, 1056, 200), Some(0));
        assert_eq!(s.splitter_at(1280, 900, false, 256, 200), Some(1));
        s.tools_visible = false;
        assert_eq!(s.splitter_at(1280, 900, false, 1056, 200), None);
    }
    #[test]
    fn config_roundtrip_and_validation() {
        let s=Workspace::decode("tools_width=999\nlayers_width=-5\nstabilizer=NaN\npressure_gamma=0\ntheme=2\naccent=abcdef\nswapped=true");
        assert_eq!(s.tools_width, 360);
        assert_eq!(s.layers_width, 208);
        assert_eq!(s.stabilizer, 0.35);
        let r = Workspace::decode(&s.encode());
        assert!(r.swapped);
        assert_eq!(r.accent, Some(0xabcdef));
        assert_eq!(r.dynamics.gamma, 0.25);
    }
    #[test]
    fn panels_swap_and_focus_reclaims_space() {
        let mut s = Workspace::default();
        let a = s.layout(1280, 900, false);
        assert_eq!(a.canvas.w, 800);
        assert_eq!(a.tools.x, ACTIVITY_WIDTH);
        s.swapped = true;
        let b = s.layout(1280, 900, false);
        assert_eq!(b.tools.x, 1056);
        assert_eq!(b.layers.x, ACTIVITY_WIDTH);
        assert_eq!(s.layout(1280, 900, true).canvas.w, 1280);
    }
}
