//! Command registry and validated, atomic shortcut configuration, without dependencies.
use super::{shortcuts::Action, workspace::Workspace};
use std::{
    io,
    path::{Path, PathBuf},
};
pub const CTRL: u8 = 1;
pub const SHIFT: u8 = 2;
pub const ALT: u8 = 4;
pub const WHEEL: u16 = 0xff01;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub key: u16,
    pub mods: u8,
}

impl Chord {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim().replace("++", "+Plus");
        let mut c = Self { key: 0, mods: 0 };
        for part in text.split('+') {
            let p = part.trim().to_ascii_lowercase();
            let m = match p.as_str() {
                "ctrl" | "control" => CTRL,
                "shift" => SHIFT,
                "alt" => ALT,
                _ => 0,
            };
            if m != 0 {
                if c.mods & m != 0 {
                    return Err("Duplicate Modifier".into());
                }
                c.mods |= m;
                continue;
            }
            if c.key != 0 {
                return Err("Only One Regular Key per Shortcut".into());
            }
            c.key = match p.as_str() {
                "space" => 32,
                "tab" => 9,
                "enter" | "return" => 13,
                "escape" | "esc" => 27,
                "backspace" => 8,
                "delete" | "del" => 46,
                "up" => 38,
                "down" => 40,
                "left" => 37,
                "right" => 39,
                "home" => 36,
                "end" => 35,
                "pageup" => 33,
                "pagedown" => 34,
                "plus" | "=" => 187,
                "minus" | "-" => 189,
                "[" => 219,
                "]" => 221,
                "comma" | "," => 188,
                "period" | "." => 190,
                "slash" | "/" => 191,
                "semicolon" => 186,
                "quote" => 222,
                "backslash" => 220,
                "backtick" => 192,
                "numplus" => 107,
                "numminus" => 109,
                "wheel" => WHEEL,
                _ if p.len() == 1 && p.as_bytes()[0].is_ascii_alphanumeric() => {
                    p.as_bytes()[0].to_ascii_uppercase() as u16
                }
                _ if p.starts_with('f') => {
                    let n = p[1..]
                        .parse::<u16>()
                        .map_err(|_| format!("Unknown Key: {}", part))?;
                    if !(1..=24).contains(&n) {
                        return Err("Function Keys Range from F1 to F24".into());
                    }
                    111 + n
                }
                _ if p.starts_with("num") => {
                    let n = p[3..]
                        .parse::<u16>()
                        .map_err(|_| format!("Unknown Key: {}", part))?;
                    if n > 9 {
                        return Err("Numpad Keys Range from Num0 to Num9".into());
                    }
                    96 + n
                }
                _ => return Err(format!("Unknown Key: {}", part)),
            };
        }
        if c.key == 0 && c.mods == 0 {
            return Err("Empty Shortcut".into());
        }
        if c.key == 115 && c.mods == ALT {
            return Err("Alt+F4 Is Reserved by Windows".into());
        }
        Ok(c)
    }
    pub fn text(self) -> String {
        let mut s = String::new();
        for (m, t) in [(CTRL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift")] {
            if self.mods & m != 0 {
                if !s.is_empty() {
                    s.push('+');
                }
                s.push_str(t);
            }
        }
        let key = match self.key {
            0 => String::new(),
            32 => "Space".into(),
            9 => "Tab".into(),
            13 => "Enter".into(),
            27 => "Escape".into(),
            8 => "Backspace".into(),
            46 => "Delete".into(),
            38 => "Up".into(),
            40 => "Down".into(),
            37 => "Left".into(),
            39 => "Right".into(),
            36 => "Home".into(),
            35 => "End".into(),
            33 => "PageUp".into(),
            34 => "PageDown".into(),
            187 => "Plus".into(),
            189 => "Minus".into(),
            219 => "[".into(),
            221 => "]".into(),
            188 => "Comma".into(),
            190 => "Period".into(),
            191 => "Slash".into(),
            186 => "Semicolon".into(),
            222 => "Quote".into(),
            220 => "Backslash".into(),
            192 => "Backtick".into(),
            107 => "NumPlus".into(),
            109 => "NumMinus".into(),
            WHEEL => "Wheel".into(),
            112..=135 => format!("F{}", self.key - 111),
            96..=105 => format!("Num{}", self.key - 96),
            _ => char::from_u32(self.key as u32).unwrap_or('?').to_string(),
        };
        if !key.is_empty() {
            if !s.is_empty() {
                s.push('+');
            }
            s.push_str(&key);
        }
        s
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    Pan,
    Pick,
    Zoom,
    ZoomOut,
    ToolZoomOut,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wheel {
    Zoom,
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Press(Action),
    Hold(Hold),
    Wheel(Wheel),
}
#[derive(Clone, Copy)]
pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub keywords: &'static str,
    pub kind: Kind,
    pub defaults: &'static str,
    pub ui: u32,
}
macro_rules! c {
    ($id:literal,$title:literal,$cat:literal,$words:literal,$a:expr,$keys:literal,$ui:expr) => {
        Command {
            id: $id,
            title: $title,
            category: $cat,
            keywords: $words,
            kind: Kind::Press($a),
            defaults: $keys,
            ui: $ui,
        }
    };
}
use Action::*;
pub static COMMANDS: &[Command] = &[
    c!(
        "replay.open",
        "Time-lapse Replay",
        "View",
        "replay timelapse history event verify huifang",
        Action::UiCommand(10131),
        "",
        10131
    ),
    c!(
        "graph.open",
        "Open Node Editor",
        "Workspace",
        "node graph filter blend jiedian lvjing",
        Action::UiCommand(10130),
        "",
        10130
    ),
    c!(
        "workspace.tabs_tools",
        "Group Color and Brush Tabs",
        "Workspace",
        "tabs group merge color brush bq hz",
        UiCommand(10122),
        "",
        10122
    ),
    c!(
        "workspace.tabs_layers",
        "Group Navigator and Layer Tabs",
        "Workspace",
        "tabs group merge navigator layer bq tc",
        UiCommand(10123),
        "",
        10123
    ),
    c!(
        "workspace.docking",
        "Free Panel Layout",
        "Workspace",
        "dock panel layout buju tingkao mb",
        UiCommand(10120),
        "",
        10120
    ),
    c!(
        "workspace.docking_reset",
        "Reset Panel Docking",
        "Workspace",
        "reset docking panel buju cz",
        UiCommand(10121),
        "",
        10121
    ),
    c!(
        "workspace.colors",
        "Show Color Sidebar",
        "Workspace",
        "color panel yanse ys",
        UiCommand(10110),
        "",
        10110
    ),
    c!(
        "workspace.brushes",
        "Show Brush Sidebar",
        "Workspace",
        "brush presets bishua bs",
        UiCommand(10111),
        "",
        10111
    ),
    c!(
        "file.new",
        "New Canvas",
        "File",
        "new canvas xinjian xj",
        New,
        "Ctrl+N",
        1
    ),
    c!(
        "file.open",
        "Open Artwork",
        "File",
        "open dakai dk",
        Open,
        "Ctrl+O",
        2
    ),
    c!(
        "canvas.size_dialog",
        "Canvas Size…",
        "Canvas",
        "resize canvas size kuobu huabu",
        UiCommand(10144),
        "Ctrl+Alt+C",
        10144
    ),
    c!(
        "canvas.crop_dialog",
        "Crop Canvas…",
        "Canvas",
        "crop canvas caijian huabu",
        UiCommand(10145),
        "",
        10145
    ),
    c!(
        "file.save",
        "Save Project",
        "File",
        "save baocun bc",
        Save,
        "Ctrl+S",
        3
    ),
    c!(
        "file.save_as",
        "Save Project As",
        "File",
        "save as lingcun lc",
        SaveAs,
        "Ctrl+Shift+S",
        7
    ),
    c!(
        "file.export",
        "Export PNG",
        "File",
        "export daochu dc",
        Export,
        "Ctrl+Alt+Shift+W",
        4
    ),
    c!(
        "edit.undo",
        "Undo",
        "Edit",
        "undo chexiao cx",
        Undo,
        "Ctrl+Z",
        5
    ),
    c!(
        "edit.redo",
        "Redo",
        "Edit",
        "redo chongzuo cz",
        Redo,
        "Ctrl+Shift+Z",
        6
    ),
    c!(
        "tool.brush",
        "Brush Tool",
        "Tools",
        "brush paint huabi hb",
        Brush,
        "B, Shift+B",
        10
    ),
    c!(
        "tool.eraser",
        "Eraser",
        "Tools",
        "eraser xiangpi xp",
        Eraser,
        "E, Shift+E",
        11
    ),
    c!(
        "tool.picker",
        "Eyedropper",
        "Tools",
        "picker eyedropper xiguan xg",
        Picker,
        "I, Shift+I",
        12
    ),
    c!(
        "tool.hand",
        "Hand",
        "Tools",
        "hand pan zhuashou zs",
        Hand,
        "H, Shift+H",
        13
    ),
    c!(
        "tool.zoom",
        "Zoom",
        "Tools",
        "zoom suofang sf",
        Zoom,
        "Z, Shift+Z",
        14
    ),
    c!(
        "view.fit",
        "Fit to Window",
        "View",
        "fit shiying shck",
        Fit,
        "Ctrl+0, Ctrl+Num0",
        40
    ),
    c!(
        "view.actual",
        "Actual Pixels",
        "View",
        "actual pixels 100 shiji",
        ActualPixels,
        "Ctrl+1, Ctrl+Num1",
        41
    ),
    c!(
        "view.zoom_in",
        "Zoom In",
        "View",
        "zoom in fangda fd",
        ZoomIn,
        "Ctrl+Plus, Ctrl+Shift+Plus, Ctrl+NumPlus",
        85
    ),
    c!(
        "view.zoom_out",
        "Zoom Out",
        "View",
        "zoom out suoxiao sx",
        ZoomOut,
        "Ctrl+Minus, Ctrl+NumMinus",
        86
    ),
    c!(
        "layer.new",
        "New Layer",
        "Layers",
        "layer new tuceng tc xjtc",
        NewLayer,
        "Ctrl+Shift+N, Ctrl+Alt+Shift+N",
        42
    ),
    c!(
        "group.create_selected",
        "Group Selection",
        "Layers",
        "group folder selected layers organize",
        UiCommand(10200),
        "Ctrl+G",
        10200
    ),
    c!(
        "group.ungroup_parent",
        "Ungroup Active Layer's Parent",
        "Layers",
        "ungroup folder parent layers",
        UiCommand(10201),
        "Ctrl+Shift+G",
        10201
    ),
    c!(
        "clipboard.system_copy",
        "Copy",
        "Edit",
        "copy clipboard selection",
        UiCommand(10161),
        "Ctrl+C",
        10161
    ),
    c!(
        "clipboard.system_copy_merged",
        "Copy Merged",
        "Edit",
        "copy merged clipboard",
        UiCommand(10162),
        "Ctrl+Shift+C",
        10162
    ),
    c!(
        "clipboard.system_cut",
        "Cut",
        "Edit",
        "cut clipboard selection",
        UiCommand(10163),
        "Ctrl+X",
        10163
    ),
    c!(
        "clipboard.system_paste",
        "Paste",
        "Edit",
        "paste clipboard image",
        UiCommand(10164),
        "Ctrl+V",
        10164
    ),
    c!(
        "clipboard.system_paste_in_place",
        "Paste in Place",
        "Edit",
        "paste clipboard original position",
        UiCommand(10165),
        "Ctrl+Shift+V",
        10165
    ),
    c!(
        "toolbox.mode.move",
        "Move Tool",
        "Tools",
        "move drag position selected pixels",
        UiCommand(10175),
        "V",
        10175
    ),
    c!(
        "movement.left",
        "Nudge Left",
        "Move",
        "move nudge position",
        UiCommand(10176),
        "Left",
        10176
    ),
    c!(
        "movement.left.large",
        "Nudge Left by Large Step",
        "Move",
        "move nudge position",
        UiCommand(10180),
        "Shift+Left",
        10180
    ),
    c!(
        "movement.right",
        "Nudge Right",
        "Move",
        "move nudge position",
        UiCommand(10177),
        "Right",
        10177
    ),
    c!(
        "movement.right.large",
        "Nudge Right by Large Step",
        "Move",
        "move nudge position",
        UiCommand(10181),
        "Shift+Right",
        10181
    ),
    c!(
        "movement.up",
        "Nudge Up",
        "Move",
        "move nudge position",
        UiCommand(10178),
        "Up",
        10178
    ),
    c!(
        "movement.up.large",
        "Nudge Up by Large Step",
        "Move",
        "move nudge position",
        UiCommand(10182),
        "Shift+Up",
        10182
    ),
    c!(
        "movement.down",
        "Nudge Down",
        "Move",
        "move nudge position",
        UiCommand(10179),
        "Down",
        10179
    ),
    c!(
        "movement.down.large",
        "Nudge Down by Large Step",
        "Move",
        "move nudge position",
        UiCommand(10183),
        "Shift+Down",
        10183
    ),
    c!(
        "canvas.trim",
        "Trim Canvas…",
        "Image",
        "trim crop transparent border corner color",
        UiCommand(10174),
        "",
        10174
    ),
    c!(
        "canvas.transform.repeat",
        "Repeat Last Transform",
        "Edit",
        "repeat transform again matrix",
        UiCommand(10185),
        "Ctrl+Shift+T",
        10185
    ),
    c!(
        "canvas.transform.repeat_copy",
        "Repeat Transform with Copy",
        "Edit",
        "repeat transform duplicate pattern copy",
        UiCommand(10186),
        "Ctrl+Alt+Shift+T",
        10186
    ),
    c!(
        "canvas.transform.open",
        "Free Transform…",
        "Edit",
        "transform scale rotate skew shear perspective distort corners flip pivot",
        UiCommand(10184),
        "Ctrl+T",
        10184
    ),
    c!(
        "canvas.image_size",
        "Image Size…",
        "Image",
        "resize scale resample aspect resolution",
        UiCommand(10173),
        "Ctrl+Alt+I",
        10173
    ),
    c!(
        "canvas.rotate_90_cw",
        "Rotate Canvas 90° Clockwise",
        "Image",
        "rotate flip exact geometry",
        UiCommand(10166),
        "",
        10166
    ),
    c!(
        "canvas.rotate_90_ccw",
        "Rotate Canvas 90° Counterclockwise",
        "Image",
        "rotate flip exact geometry",
        UiCommand(10167),
        "",
        10167
    ),
    c!(
        "canvas.rotate_180",
        "Rotate Canvas 180°",
        "Image",
        "rotate flip exact geometry",
        UiCommand(10168),
        "",
        10168
    ),
    c!(
        "canvas.flip_horizontal",
        "Flip Canvas Horizontally",
        "Image",
        "rotate flip exact geometry",
        UiCommand(10169),
        "",
        10169
    ),
    c!(
        "canvas.flip_vertical",
        "Flip Canvas Vertically",
        "Image",
        "rotate flip exact geometry",
        UiCommand(10170),
        "",
        10170
    ),
    c!(
        "layer.rotate_90_cw",
        "Rotate Layer 90° Clockwise",
        "Layers",
        "rotate flip exact geometry",
        UiCommand(10171),
        "",
        10171
    ),
    c!(
        "layer.rotate_90_ccw",
        "Rotate Layer 90° Counterclockwise",
        "Layers",
        "rotate flip exact geometry",
        UiCommand(10172),
        "",
        10172
    ),
    c!(
        "layer.via_copy",
        "Layer via Copy",
        "Layers",
        "selection copy duplicate layer",
        UiCommand(10159),
        "Ctrl+J",
        10159
    ),
    c!(
        "layer.via_cut",
        "Layer via Cut",
        "Layers",
        "selection cut new layer",
        UiCommand(10160),
        "Ctrl+Shift+J",
        10160
    ),
    c!(
        "layer.duplicate",
        "Duplicate Layer",
        "Layers",
        "duplicate copy layer fuzhi tuceng",
        UiCommand(10140),
        "",
        10140
    ),
    c!(
        "layer.clear",
        "Clear Current Layer",
        "Layers",
        "clear qingkong qk tc",
        UiCommand(43),
        "",
        43
    ),
    c!(
        "layer.merge",
        "Merge Layers",
        "Layers",
        "merge selected down combine layers he bing",
        UiCommand(10187),
        "Ctrl+E",
        10187
    ),
    c!(
        "layer.merge_down",
        "Merge Down",
        "Layers",
        "merge down combine layer he bing tuceng",
        UiCommand(10146),
        "",
        10146
    ),
    c!(
        "layer.merge_visible",
        "Merge Visible",
        "Layers",
        "merge visible combine layers he bing kejian",
        UiCommand(10147),
        "Ctrl+Shift+E",
        10147
    ),
    c!(
        "layer.flatten",
        "Flatten Image",
        "Layers",
        "flatten rasterize image pinhe",
        UiCommand(10148),
        "",
        10148
    ),
    c!(
        "layer.stamp_visible",
        "Stamp Visible",
        "Layers",
        "stamp visible merged copy new layer gaiyin",
        UiCommand(10149),
        "Ctrl+Alt+Shift+E",
        10149
    ),
    c!(
        "layer.fill_foreground",
        "Fill with Foreground",
        "Layers",
        "fill foreground qianjingse",
        UiCommand(10142),
        "Alt+Backspace",
        10142
    ),
    c!(
        "selection.all",
        "Select All",
        "Selection",
        "select all xuanqu",
        UiCommand(10150),
        "Ctrl+A",
        10150
    ),
    c!(
        "toolbox.mode.selection_rectangle",
        "Rectangular Marquee",
        "Selection",
        "rectangle marquee selection xuanqu",
        UiCommand(10154),
        "M",
        10154
    ),
    c!(
        "toolbox.mode.selection_ellipse",
        "Elliptical Marquee",
        "Selection",
        "ellipse circle marquee selection",
        UiCommand(10155),
        "Shift+M",
        10155
    ),
    c!(
        "toolbox.mode.selection_lasso",
        "Lasso",
        "Selection",
        "lasso freehand selection",
        UiCommand(10156),
        "L",
        10156
    ),
    c!(
        "toolbox.mode.selection_polygon",
        "Polygonal Lasso",
        "Selection",
        "polygon selection vertices",
        UiCommand(10157),
        "Shift+L",
        10157
    ),
    c!(
        "toolbox.mode.selection_wand",
        "Magic Wand",
        "Selection",
        "wand color tolerance selection",
        UiCommand(10158),
        "W",
        10158
    ),
    c!(
        "selection.deselect",
        "Deselect",
        "Selection",
        "deselect clear selection",
        UiCommand(10151),
        "Ctrl+D",
        10151
    ),
    c!(
        "selection.reselect",
        "Reselect",
        "Selection",
        "restore selection",
        UiCommand(10152),
        "Ctrl+Shift+D",
        10152
    ),
    c!(
        "selection.invert",
        "Invert Selection",
        "Selection",
        "invert selection reverse",
        UiCommand(10153),
        "Ctrl+Shift+I",
        10153
    ),
    c!(
        "layer.fill_background",
        "Fill with Background",
        "Layers",
        "fill background beijingse",
        UiCommand(10143),
        "Ctrl+Backspace",
        10143
    ),
    c!(
        "brush.smaller",
        "Decrease Brush Size",
        "Brush",
        "brush size small hb",
        SmallerBrush,
        "[",
        0
    ),
    c!(
        "brush.larger",
        "Increase Brush Size",
        "Brush",
        "brush size large hb",
        LargerBrush,
        "]",
        0
    ),
    c!(
        "brush.softer",
        "Decrease Hardness",
        "Brush",
        "brush hardness soft yd",
        SofterBrush,
        "Shift+[",
        0
    ),
    c!(
        "brush.harder",
        "Increase Hardness",
        "Brush",
        "brush hardness hard yd",
        HarderBrush,
        "Shift+]",
        0
    ),
    c!(
        "brush.opacity.0",
        "Opacity 100% / Key 0",
        "Brush",
        "opacity btm",
        OpacityDigit(0),
        "0",
        0
    ),
    c!(
        "brush.opacity.1",
        "Opacity 10% / Key 1",
        "Brush",
        "opacity btm",
        OpacityDigit(1),
        "1",
        0
    ),
    c!(
        "brush.opacity.2",
        "Opacity 20% / Key 2",
        "Brush",
        "opacity btm",
        OpacityDigit(2),
        "2",
        0
    ),
    c!(
        "brush.opacity.3",
        "Opacity 30% / Key 3",
        "Brush",
        "opacity btm",
        OpacityDigit(3),
        "3",
        0
    ),
    c!(
        "brush.opacity.4",
        "Opacity 40% / Key 4",
        "Brush",
        "opacity btm",
        OpacityDigit(4),
        "4",
        0
    ),
    c!(
        "brush.opacity.5",
        "Opacity 50% / Key 5",
        "Brush",
        "opacity btm",
        OpacityDigit(5),
        "5",
        0
    ),
    c!(
        "brush.opacity.6",
        "Opacity 60% / Key 6",
        "Brush",
        "opacity btm",
        OpacityDigit(6),
        "6",
        0
    ),
    c!(
        "brush.opacity.7",
        "Opacity 70% / Key 7",
        "Brush",
        "opacity btm",
        OpacityDigit(7),
        "7",
        0
    ),
    c!(
        "brush.opacity.8",
        "Opacity 80% / Key 8",
        "Brush",
        "opacity btm",
        OpacityDigit(8),
        "8",
        0
    ),
    c!(
        "brush.opacity.9",
        "Opacity 90% / Key 9",
        "Brush",
        "opacity btm",
        OpacityDigit(9),
        "9",
        0
    ),
    c!(
        "color.default",
        "Default Black and White",
        "Color",
        "default colors yanse ys",
        DefaultColors,
        "D",
        0
    ),
    c!(
        "color.swap",
        "Swap Foreground and Background",
        "Color",
        "swap colors yanse ys",
        SwapColors,
        "X",
        0
    ),
    c!(
        "color.dialog",
        "System Color Picker",
        "Color",
        "color picker dialog yanse ys",
        UiCommand(30),
        "",
        30
    ),
    c!(
        "workspace.preferences",
        "Settings",
        "Workspace",
        "preferences settings shezhi sz",
        Preferences,
        "Ctrl+Comma, Ctrl+K",
        50
    ),
    c!(
        "workspace.panels",
        "Toggle Panels",
        "Workspace",
        "panels mianban mb",
        TogglePanels,
        "Tab",
        69
    ),
    c!(
        "workspace.layers",
        "Toggle Layers Panel",
        "Workspace",
        "layers panel tcmb",
        ToggleLayers,
        "Shift+Tab",
        61
    ),
    c!(
        "workspace.tools",
        "Toggle Tools Panel",
        "Workspace",
        "tools panel gjmb",
        UiCommand(60),
        "",
        60
    ),
    c!(
        "workspace.swap",
        "Swap Sidebars",
        "Workspace",
        "swap layout buju bj",
        UiCommand(62),
        "",
        62
    ),
    c!(
        "workspace.reset",
        "Reset Workspace",
        "Workspace",
        "reset layout buju bj",
        UiCommand(67),
        "",
        67
    ),
    c!(
        "workspace.reload",
        "Reload Workspace",
        "Workspace",
        "reload settings pz",
        UiCommand(68),
        "",
        68
    ),
    c!(
        "workspace.path",
        "Show Workspace Config Path",
        "Workspace",
        "config path pz lj",
        UiCommand(79),
        "",
        79
    ),
    c!(
        "commands.open",
        "Open Command Palette",
        "Commands",
        "command palette search mingling mlmb",
        OpenPalette,
        "Ctrl+Shift+P",
        95
    ),
    c!(
        "keybindings.edit",
        "Configure Shortcuts",
        "Commands",
        "keybindings shortcuts kuaijiejian kjj",
        ShowShortcuts,
        "Ctrl+Alt+Shift+K",
        93
    ),
    c!(
        "keybindings.path",
        "Show settings.json Location",
        "Commands",
        "settings config file path pz lj",
        UiCommand(98),
        "",
        98
    ),
    c!(
        "keybindings.reload",
        "Reload Shortcuts",
        "Commands",
        "reload keybindings shortcuts kjj pz",
        ReloadBindings,
        "",
        96
    ),
    c!(
        "keybindings.reset",
        "Reset All Shortcuts",
        "Commands",
        "reset keybindings shortcuts kjj",
        ResetBindings,
        "",
        97
    ),
    c!(
        "input.cancel",
        "Cancel Current Input",
        "Input",
        "cancel escape quxiao qx",
        Cancel,
        "Escape",
        0
    ),
    Command {
        id: "hold.pan",
        title: "Temporary Hand (Hold)",
        category: "Gestures",
        keywords: "hand pan space zs",
        kind: Kind::Hold(Hold::Pan),
        defaults: "Space",
        ui: 0,
    },
    Command {
        id: "hold.pick",
        title: "Temporary Eyedropper (Hold)",
        category: "Gestures",
        keywords: "picker alt xg",
        kind: Kind::Hold(Hold::Pick),
        defaults: "Alt",
        ui: 0,
    },
    Command {
        id: "hold.zoom",
        title: "Temporary Zoom In (Hold)",
        category: "Gestures",
        keywords: "zoom temporary fd",
        kind: Kind::Hold(Hold::Zoom),
        defaults: "Ctrl+Space",
        ui: 0,
    },
    Command {
        id: "hold.zoom_out",
        title: "Temporary Zoom Out (Hold)",
        category: "Gestures",
        keywords: "zoom out temporary sx",
        kind: Kind::Hold(Hold::ZoomOut),
        defaults: "Ctrl+Alt+Space",
        ui: 0,
    },
    Command {
        id: "hold.tool_zoom_out",
        title: "Zoom Tool: Temporary Zoom Out",
        category: "Gestures",
        keywords: "zoom alt temporary",
        kind: Kind::Hold(Hold::ToolZoomOut),
        defaults: "Alt",
        ui: 0,
    },
    Command {
        id: "wheel.zoom",
        title: "Scroll to Zoom",
        category: "Gestures",
        keywords: "wheel zoom sf",
        kind: Kind::Wheel(Wheel::Zoom),
        defaults: "Alt+Wheel",
        ui: 0,
    },
    Command {
        id: "wheel.horizontal",
        title: "Scroll to Pan Horizontally",
        category: "Gestures",
        keywords: "wheel horizontal pan",
        kind: Kind::Wheel(Wheel::Horizontal),
        defaults: "Shift+Wheel",
        ui: 0,
    },
    Command {
        id: "wheel.vertical",
        title: "Scroll to Pan Vertically",
        category: "Gestures",
        keywords: "wheel vertical pan",
        kind: Kind::Wheel(Wheel::Vertical),
        defaults: "Wheel",
        ui: 0,
    },
    c!(
        "theme.light",
        "Mist Theme",
        "Appearance Settings",
        "light theme wubai wb",
        UiCommand(63),
        "",
        63
    ),
    c!(
        "theme.warm",
        "Warm Paper Theme",
        "Appearance Settings",
        "warm paper nuanzhi nz",
        UiCommand(64),
        "",
        64
    ),
    c!(
        "theme.dark",
        "Graphite Theme",
        "Appearance Settings",
        "dark theme shimo sm",
        UiCommand(65),
        "",
        65
    ),
    c!(
        "theme.accent",
        "Custom Accent Color",
        "Appearance Settings",
        "accent color qiangdiao",
        UiCommand(66),
        "",
        66
    ),
    c!(
        "pressure.size",
        "Toggle Pressure Size",
        "Brush",
        "pressure size yagan yg",
        UiCommand(51),
        "",
        51
    ),
    c!(
        "pressure.opacity",
        "Toggle Pressure Opacity",
        "Brush",
        "pressure opacity yagan yg",
        UiCommand(52),
        "",
        52
    ),
    c!(
        "pressure.curve",
        "Cycle Pressure Curve",
        "Brush",
        "pressure curve quxian",
        UiCommand(53),
        "",
        53
    ),
    c!(
        "pressure.soft",
        "Soft Pressure Curve",
        "Brush",
        "pressure curve soft",
        UiCommand(72),
        "",
        72
    ),
    c!(
        "pressure.linear",
        "Linear Pressure Curve",
        "Brush",
        "pressure curve linear",
        UiCommand(73),
        "",
        73
    ),
    c!(
        "pressure.firm",
        "Firm Pressure Curve",
        "Brush",
        "pressure curve firm",
        UiCommand(74),
        "",
        74
    ),
    c!(
        "stabilizer.off",
        "Disable Stabilizer",
        "Brush",
        "stabilizer smoothing fangdou fd",
        UiCommand(75),
        "",
        75
    ),
    c!(
        "stabilizer.light",
        "Light Stabilizer 35%",
        "Brush",
        "stabilizer smoothing fangdou fd",
        UiCommand(76),
        "",
        76
    ),
    c!(
        "stabilizer.medium",
        "Medium Stabilizer 70%",
        "Brush",
        "stabilizer smoothing fangdou fd",
        UiCommand(77),
        "",
        77
    ),
    c!(
        "stabilizer.strong",
        "Strong Stabilizer 100%",
        "Brush",
        "stabilizer smoothing fangdou fd",
        UiCommand(78),
        "",
        78
    ),
    c!(
        "preset.pencil",
        "Pencil Preset",
        "Brush",
        "preset pencil qianbi",
        UiCommand(820),
        "",
        820
    ),
    c!(
        "preset.ink",
        "Ink Preset",
        "Brush",
        "preset ink moxian",
        UiCommand(821),
        "",
        821
    ),
    c!(
        "preset.brush",
        "Brush Preset",
        "Brush",
        "preset brush huabi",
        UiCommand(822),
        "",
        822
    ),
    c!(
        "preset.soft",
        "Soft Preset",
        "Brush",
        "preset soft airbrush",
        UiCommand(823),
        "",
        823
    ),
    c!(
        "preset.marker",
        "Marker Preset",
        "Brush",
        "preset marker makebi",
        UiCommand(824),
        "",
        824
    ),
    c!(
        "preset.eraser",
        "Eraser Preset",
        "Brush",
        "preset eraser xiangpi",
        UiCommand(825),
        "",
        825
    ),
    c!(
        "size.1",
        "Brush Diameter 1 px",
        "Brush",
        "size diameter",
        UiCommand(840),
        "",
        840
    ),
    c!(
        "size.2",
        "Brush Diameter 2 px",
        "Brush",
        "size diameter",
        UiCommand(841),
        "",
        841
    ),
    c!(
        "size.3",
        "Brush Diameter 3 px",
        "Brush",
        "size diameter",
        UiCommand(842),
        "",
        842
    ),
    c!(
        "size.5",
        "Brush Diameter 5 px",
        "Brush",
        "size diameter",
        UiCommand(843),
        "",
        843
    ),
    c!(
        "size.8",
        "Brush Diameter 8 px",
        "Brush",
        "size diameter",
        UiCommand(844),
        "",
        844
    ),
    c!(
        "size.12",
        "Brush Diameter 12 px",
        "Brush",
        "size diameter",
        UiCommand(845),
        "",
        845
    ),
    c!(
        "size.20",
        "Brush Diameter 20 px",
        "Brush",
        "size diameter",
        UiCommand(846),
        "",
        846
    ),
    c!(
        "size.35",
        "Brush Diameter 35 px",
        "Brush",
        "size diameter",
        UiCommand(847),
        "",
        847
    ),
    c!(
        "size.60",
        "Brush Diameter 60 px",
        "Brush",
        "size diameter",
        UiCommand(848),
        "",
        848
    ),
    c!(
        "size.100",
        "Brush Diameter 100 px",
        "Brush",
        "size diameter",
        UiCommand(849),
        "",
        849
    ),
    c!(
        "canvas.size_preset",
        "Cycle New Canvas Size",
        "Canvas",
        "canvas size preset",
        UiCommand(46),
        "",
        46
    ),
];
#[derive(Clone)]
pub struct Keymap {
    pub bindings: Vec<Vec<Chord>>,
    extra: Vec<PluginCommand>,
    pending: std::collections::BTreeMap<String, Vec<Chord>>,
}
#[derive(Clone)]
struct PluginCommand {
    owner: Option<String>,
    id: String,
    title: String,
    action: String,
    ui: u32,
}
#[derive(Clone, Copy)]
pub struct CommandView<'a> {
    pub owner: Option<&'a str>,
    pub id: &'a str,
    pub title: &'a str,
    pub category: &'a str,
    pub keywords: &'a str,
    pub kind: Kind,
    pub ui: u32,
}
impl Default for Keymap {
    fn default() -> Self {
        Self {
            extra: vec![],
            pending: Default::default(),
            bindings: COMMANDS
                .iter()
                .map(|c| parse_list(c.defaults).unwrap())
                .collect(),
        }
    }
}
fn parse_list(s: &str) -> Result<Vec<Chord>, String> {
    if s.trim().is_empty() {
        return Ok(vec![]);
    }
    s.split(',').map(Chord::parse).collect()
}
#[derive(Clone, Copy, Default)]
pub struct InputState {
    pub pan: bool,
    pub pick: bool,
    pub zoom: bool,
    pub zoom_out: bool,
    pub tool_zoom_out: bool,
}
impl Keymap {
    pub fn command(&self, index: usize) -> CommandView<'_> {
        if index < COMMANDS.len() {
            let c = &COMMANDS[index];
            CommandView {
                owner: None,
                id: c.id,
                title: c.title,
                category: c.category,
                keywords: c.keywords,
                kind: c.kind,
                ui: c.ui,
            }
        } else {
            let c = &self.extra[index - COMMANDS.len()];
            CommandView {
                owner: c.owner.as_deref(),
                id: &c.id,
                title: &c.title,
                category: "Plugin Commands",
                keywords: &c.action,
                kind: Kind::Press(Action::UiCommand(c.ui)),
                ui: c.ui,
            }
        }
    }
    pub fn commands(&self) -> impl Iterator<Item = CommandView<'_>> {
        (0..COMMANDS.len() + self.extra.len()).map(|i| self.command(i))
    }
    pub fn plugin_action(&self, ui: u32) -> Option<&str> {
        self.extra
            .iter()
            .find(|c| c.ui == ui)
            .map(|c| c.action.as_str())
    }
    pub fn register_action_owners(&mut self, owners: &std::collections::BTreeMap<String, String>) {
        for command in &mut self.extra {
            command.owner = owners.get(&command.action).cloned();
        }
    }
    pub fn register_actions(
        &mut self,
        actions: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        for (action, title) in actions {
            let id = format!("plugin.{action}");
            if next.extra.iter().any(|c| c.id == id) {
                continue;
            }
            let ui = id
                .bytes()
                .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
                | 0x80000000;
            if next.commands().any(|c| c.ui == ui) {
                return Err(format!("Command identity collision: {id}"));
            }
            let keys = next.pending.remove(&id).unwrap_or_default();
            next.extra.push(PluginCommand {
                owner: None,
                id,
                title,
                action,
                ui,
            });
            next.bindings.push(keys);
        }
        if let Err(error) = next.validate() {
            for keys in &mut next.bindings[COMMANDS.len()..] {
                keys.clear();
            }
            *self = next;
            return Err(format!("Plugin Bindings Disabled: {error}"));
        }
        *self = next;
        Ok(())
    }

    pub fn resolve(&self, key: usize, mods: u8) -> Option<Action> {
        self.commands().enumerate().find_map(|(i, c)| {
            if let Kind::Press(a) = c.kind {
                self.bindings[i]
                    .iter()
                    .any(|b| b.key as usize == key && b.mods == mods)
                    .then(|| a)
            } else {
                None
            }
        })
    }
    pub fn wheel(&self, mods: u8) -> Option<Wheel> {
        self.commands().enumerate().find_map(|(i, c)| {
            if let Kind::Wheel(a) = c.kind {
                self.bindings[i].iter().any(|b| b.mods == mods).then(|| a)
            } else {
                None
            }
        })
    }
    pub fn held(&self, mods: u8, mut pressed: impl FnMut(u16) -> bool) -> InputState {
        let mut s = InputState::default();
        for (i, c) in self.commands().enumerate() {
            if let Kind::Hold(h) = c.kind {
                if self.bindings[i]
                    .iter()
                    .any(|b| b.mods == mods && (b.key == 0 || pressed(b.key)))
                {
                    match h {
                        Hold::Pan => s.pan = true,
                        Hold::Pick => s.pick = true,
                        Hold::Zoom => s.zoom = true,
                        Hold::ZoomOut => s.zoom_out = true,
                        Hold::ToolZoomOut => s.tool_zoom_out = true,
                    }
                }
            }
        }
        s
    }
    pub fn hold_event(&self, key: usize, mods: u8) -> bool {
        self.commands().enumerate().any(|(i, c)| {
            matches!(c.kind, Kind::Hold(_))
                && self.bindings[i].iter().any(|b| {
                    b.mods == mods
                        && (b.key as usize == key
                            || (b.key == 0 && matches!(key, 16 | 17 | 18 | 160..=165)))
                })
        })
    }
    pub fn label(&self, index: usize) -> String {
        self.bindings
            .get(index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .map(|b| b.text())
            .collect::<Vec<_>>()
            .join(", ")
    }
    pub fn ui_hint(&self, id: u32) -> Option<String> {
        if id == 0 {
            return None;
        }
        self.commands().position(|c| c.ui == id).map(|i| {
            let k = self.label(i);
            format!(
                "{}{}{}",
                crate::i18n::translated_for(self.command(i).owner, self.command(i).title),
                if k.is_empty() { "" } else { "  " },
                k
            )
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        for (i, keys) in self.bindings.iter().enumerate() {
            for (a, k) in keys.iter().enumerate() {
                match self.command(i).kind {
                    Kind::Press(_) => {
                        if k.key == 0 || k.key == WHEEL {
                            return Err(format!(
                                "{} Requires a Regular Key",
                                self.command(i).title
                            ));
                        }
                    }
                    Kind::Hold(_) => {
                        if k.key == WHEEL {
                            return Err("Hold Gestures Cannot Use the Scroll Wheel".into());
                        }
                    }
                    Kind::Wheel(_) => {
                        if k.key != WHEEL {
                            return Err("Bind Scroll Actions to Wheel or Modifier+Wheel".into());
                        }
                    }
                }
                if keys[..a].contains(k) {
                    return Err(format!("Duplicate Shortcut in {}", self.command(i).title));
                }
                for j in 0..i {
                    if self.bindings[j].contains(k) {
                        let shared = matches!(
                            (self.command(i).kind, self.command(j).kind),
                            (Kind::Hold(Hold::Pick), Kind::Hold(Hold::ToolZoomOut))
                                | (Kind::Hold(Hold::ToolZoomOut), Kind::Hold(Hold::Pick))
                        );
                        if !shared {
                            return Err(format!(
                                "{} Conflicts between {} and {}",
                                k.text(),
                                self.command(j).title,
                                self.command(i).title
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn changed(&self, index: usize, keys: Vec<Chord>) -> Result<Self, String> {
        let mut next = self.clone();
        next.bindings[index] = keys;
        next.validate()?;
        Ok(next)
    }
    pub fn decode(text: &str) -> Result<Self, String> {
        let mut map = Self::default();
        let entries = super::settings::read(text)?;
        let explicit: std::collections::BTreeSet<_> =
            entries.iter().map(|(id, _)| id.clone()).collect();
        for (id, keys) in entries {
            let parsed = keys
                .iter()
                .map(|k| Chord::parse(k))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(i) = COMMANDS.iter().position(|c| c.id == id) {
                map.bindings[i] = parsed;
            } else if id.starts_with("plugin.")
                && id.len() <= 160
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                if parsed.iter().any(|k| k.key == 0 || k.key == WHEEL) {
                    return Err("Plugin commands require a regular key".into());
                }
                map.pending.insert(id, parsed);
            } else {
                return Err(format!("Unknown Command: {id}"));
            }
        }
        // New gesture and layer defaults yield to explicit older bindings,
        // including whole-layer duplication and unloaded plugin commands.
        let reserved: Vec<_> = COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, c)| explicit.contains(c.id))
            .flat_map(|(i, _)| map.bindings[i].iter().copied())
            .chain(map.pending.values().flatten().copied())
            .collect();
        for (i, command) in COMMANDS.iter().enumerate() {
            if (command.id.starts_with("toolbox.mode.selection_")
                || matches!(
                    command.id,
                    "layer.via_copy"
                        | "layer.merge"
                        | "layer.via_cut"
                        | "canvas.image_size"
                        | "canvas.transform.open"
                        | "canvas.transform.repeat"
                        | "canvas.transform.repeat_copy"
                )
                || command.id.starts_with("clipboard.system_")
                || command.id.starts_with("group.")
                || command.id == "toolbox.mode.move"
                || command.id.starts_with("movement."))
                && !explicit.contains(command.id)
            {
                map.bindings[i].retain(|key| !reserved.contains(key));
            }
        }
        map.validate()?;
        Ok(map)
    }
    pub fn encode(&self) -> String {
        super::settings::write(
            self.commands()
                .enumerate()
                .map(|(i, c)| (c.id, self.bindings[i].iter().map(|k| k.text()).collect()))
                .chain(
                    self.pending
                        .iter()
                        .map(|(id, keys)| (id.as_str(), keys.iter().map(|k| k.text()).collect())),
                ),
        )
    }

    pub fn path() -> PathBuf {
        Workspace::path().with_file_name("settings.json")
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                #[cfg(all(windows, feature = "desktop"))]
                {
                    return crate::desktop::settings::Settings::decode(&s, path.to_path_buf())
                        .map(|s| s.keys);
                }
                #[cfg(not(all(windows, feature = "desktop")))]
                Self::decode(&s)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        #[cfg(all(windows, feature = "desktop"))]
        if path.exists() {
            let mut settings = crate::desktop::settings::Settings::load(path.to_path_buf())?;
            settings.keys = self.clone();
            return settings.save();
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.encode()).map_err(|e| e.to_string())?;
        crate::platform::replace_file(&tmp, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn merge_layers_default_keeps_explicit_legacy_merge_down_bindings() {
        assert_eq!(
            super::Keymap::default().resolve('E' as usize, super::CTRL),
            Some(super::Action::UiCommand(10187))
        );
        let index = super::COMMANDS
            .iter()
            .position(|command| command.id == "layer.merge")
            .unwrap();
        let legacy =
            super::Keymap::decode(r#"{"keybindings":{"layer.merge_down":"Ctrl+E"}}"#).unwrap();
        assert!(legacy.bindings[index].is_empty());
        assert_eq!(
            legacy.resolve('E' as usize, super::CTRL),
            Some(super::Action::UiCommand(10146))
        );
        let explicit = super::Keymap::decode(
            r#"{"keybindings":{"layer.merge":"Ctrl+Alt+E","layer.merge_down":"Ctrl+E"}}"#,
        )
        .unwrap();
        assert_eq!(
            explicit.resolve('E' as usize, super::CTRL | super::ALT),
            Some(super::Action::UiCommand(10187))
        );
        assert_eq!(
            explicit.resolve('E' as usize, super::CTRL),
            Some(super::Action::UiCommand(10146))
        );
        let unloaded =
            super::Keymap::decode(r#"{"keybindings":{"plugin.custom.merge":"Ctrl+E"}}"#).unwrap();
        assert!(unloaded.bindings[index].is_empty());
        assert_eq!(
            super::Keymap::decode(&explicit.encode()).unwrap().bindings,
            explicit.bindings
        );
    }
    #[test]
    fn group_shortcuts_keep_ps_defaults_and_yield_to_explicit_older_bindings() {
        let create = super::COMMANDS
            .iter()
            .position(|c| c.id == "group.create_selected")
            .unwrap();
        let ungroup = super::COMMANDS
            .iter()
            .position(|c| c.id == "group.ungroup_parent")
            .unwrap();
        let defaults = super::Keymap::default();
        assert_eq!(
            defaults.bindings[create],
            vec![super::Chord::parse("Ctrl+G").unwrap()]
        );
        assert_eq!(
            defaults.bindings[ungroup],
            vec![super::Chord::parse("Ctrl+Shift+G").unwrap()]
        );
        for json in [
            r#"{"keybindings":{"plugin.custom.group":"Ctrl+G","plugin.custom.ungroup":"Ctrl+Shift+G"}}"#,
            r#"{"keybindings":{"tool.brush":"Ctrl+G","tool.eraser":"Ctrl+Shift+G"}}"#,
        ] {
            let map = super::Keymap::decode(json).unwrap();
            assert!(map.bindings[create].is_empty());
            assert!(map.bindings[ungroup].is_empty());
        }
        let disabled = super::Keymap::decode(
            r#"{"keybindings":{"group.create_selected":[],"group.ungroup_parent":[]}}"#,
        )
        .unwrap();
        assert!(disabled.bindings[create].is_empty() && disabled.bindings[ungroup].is_empty());
    }
    use super::*;
    #[test]
    fn plugin_commands_are_bindable_and_survive_unloaded_plugins_and_restart() {
        let mut map = Keymap::decode(
            r#"{"version":1,"keybindings":{"plugin.window.toolbox.float":["Ctrl+Alt+F12"]}}"#,
        )
        .unwrap();
        let encoded = map.encode();
        assert!(encoded.contains("plugin.window.toolbox.float"));
        map.register_actions(vec![("window.toolbox.float".into(), "Float Tools".into())])
            .unwrap();
        let i = map
            .commands()
            .position(|c| c.id == "plugin.window.toolbox.float")
            .unwrap();
        let action = map.resolve(123, CTRL | ALT).unwrap();
        assert_eq!(action, Action::UiCommand(map.command(i).ui));
        assert_eq!(
            map.plugin_action(map.command(i).ui),
            Some("window.toolbox.float")
        );
        let mut copy = Keymap::decode(&map.encode()).unwrap();
        copy.register_actions(vec![("window.toolbox.float".into(), "Float Tools".into())])
            .unwrap();
        assert_eq!(copy.bindings, map.bindings);
        let mut palette =
            crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, &copy);
        palette.query("Float Tools".into(), &copy);
        assert!(palette.results.contains(&i));
    }
    fn index(id: &str) -> usize {
        COMMANDS.iter().position(|c| c.id == id).unwrap()
    }
    #[test]
    fn movement_defaults_preserve_explicit_older_tool_and_plugin_bindings() {
        for (id, key) in [
            ("toolbox.mode.move", "V"),
            ("movement.left", "Left"),
            ("movement.right.large", "Shift+Right"),
            ("canvas.transform.open", "Ctrl+T"),
            ("canvas.transform.repeat", "Ctrl+Shift+T"),
            ("canvas.transform.repeat_copy", "Ctrl+Alt+Shift+T"),
        ] {
            let i = COMMANDS.iter().position(|c| c.id == id).unwrap();
            let p = serde_json::json!({"keybindings":{"plugin.custom.older":key}}).to_string();
            let map = Keymap::decode(&p).unwrap();
            assert!(map.bindings[i].is_empty());
            assert_eq!(
                Keymap::decode(&map.encode()).unwrap().bindings,
                map.bindings
            );
        }
    }
    #[test]
    fn defaults_and_all_named_keys_round_trip() {
        let map = Keymap::default();
        map.validate().unwrap();
        let decoded = Keymap::decode(&map.encode()).unwrap();
        assert_eq!(map.bindings, decoded.bindings);
        for text in [
            "Ctrl++",
            "Ctrl+Alt+Shift+F24",
            "Num9",
            "Shift+[",
            "Alt",
            "Ctrl+Wheel",
            "Quote",
            "Backslash",
            "NumPlus",
            "Escape",
        ] {
            let key = Chord::parse(text).unwrap();
            assert_eq!(Chord::parse(&key.text()).unwrap(), key);
        }
        for text in [
            "Ctrl+Ctrl+B",
            "Ctrl+B+E",
            "F25",
            "Num10",
            "Alt+F4",
            "unknown",
            "",
        ] {
            assert!(Chord::parse(text).is_err(), "{}", text);
        }
    }
    #[test]
    fn partial_config_overrides_aliases_unbinds_and_preserves_defaults() {
        let map =
            Keymap::decode(r#"{"keybindings":{"tool.brush":["V","Shift+V"],"tool.eraser":[]}}"#)
                .unwrap();
        assert_eq!(map.resolve(86, 0), Some(Action::Brush));
        assert_eq!(map.resolve(66, 0), None);
        assert_eq!(map.resolve(69, 0), None);
        assert_eq!(map.resolve(83, CTRL), Some(Action::Save));
        assert_eq!(map.ui_hint(10).unwrap(), "Brush Tool  V, Shift+V");
        assert_eq!(map.bindings[index("tool.brush")].len(), 2);
    }
    #[test]
    fn conflicting_or_invalid_changes_are_rejected_transactionally() {
        let map = Keymap::default();
        let i = index("tool.brush");
        let before = map.encode();
        assert!(map
            .changed(i, vec![Chord::parse("E").unwrap()])
            .err()
            .unwrap()
            .contains("Eraser"));
        assert_eq!(map.encode(), before);
        for fields in [
            r#""tool.brush":"E""#,
            r#""tool.brush":["B","B"]"#,
            r#""file.save":"Ctrl""#,
            r#""wheel.zoom":"B""#,
            r#""hold.pan":"Wheel""#,
            r#""missing":"B""#,
            r#""tool.brush":"V","tool.brush":"Q""#,
        ] {
            assert!(
                Keymap::decode(&format!("{{\"keybindings\":{{{}}}}}", fields)).is_err(),
                "{}",
                fields
            );
        }
    }
    #[test]
    fn hold_and_wheel_bindings_really_follow_custom_configuration() {
        let map=Keymap::decode(r#"{"keybindings":{"hold.pan":"Q","hold.pick":"Ctrl","hold.tool_zoom_out":"Shift","wheel.zoom":"Ctrl+Wheel","wheel.horizontal":"Alt+Wheel"}}"#).unwrap();
        assert!(map.held(0, |k| k == 81).pan);
        assert!(!map.held(0, |k| k == 32).pan);
        assert!(map.held(CTRL, |_| false).pick);
        assert!(!map.held(ALT, |_| false).pick);
        assert!(map.held(SHIFT, |_| false).tool_zoom_out);
        assert_eq!(map.wheel(CTRL), Some(Wheel::Zoom));
        assert_eq!(map.wheel(ALT), Some(Wheel::Horizontal));
        assert_eq!(map.wheel(SHIFT), None);
        assert!(map.hold_event(81, 0));
        assert!(!map.hold_event(115, ALT));
    }
    #[test]
    fn config_file_is_utf8_and_atomically_replaceable() {
        let path = {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
            std::fs::create_dir_all(&root).unwrap();
            root.join(format!("micopi-keymap-test-{}.json", std::process::id()))
        };
        let map = Keymap::decode(r#"{"keybindings":{"tool.brush":"V"}}"#).unwrap();
        map.save(&path).unwrap();
        assert_eq!(Keymap::load(&path).unwrap().bindings, map.bindings);
        Keymap::default().save(&path).unwrap();
        assert_eq!(
            Keymap::load(&path).unwrap().bindings,
            Keymap::default().bindings
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn new_selection_defaults_preserve_older_explicit_builtin_and_plugin_keys() {
        let mut map =
            Keymap::decode(r#"{"keybindings":{"tool.brush":"M","plugin.custom.tool":"L"}}"#)
                .unwrap();
        let rectangle = COMMANDS
            .iter()
            .position(|c| c.id == "toolbox.mode.selection_rectangle")
            .unwrap();
        let lasso = COMMANDS
            .iter()
            .position(|c| c.id == "toolbox.mode.selection_lasso")
            .unwrap();
        let brush = COMMANDS.iter().position(|c| c.id == "tool.brush").unwrap();
        assert!(map.bindings[rectangle].is_empty());
        assert!(map.bindings[lasso].is_empty());
        assert_eq!(map.bindings[brush], vec![Chord::parse("M").unwrap()]);
        let copy = Keymap::decode(&map.encode()).unwrap();
        assert_eq!(copy.bindings, map.bindings);
        map.register_actions(vec![("custom.tool".into(), "Custom Tool".into())])
            .unwrap();
        let plugin = map
            .commands()
            .position(|c| c.id == "plugin.custom.tool")
            .unwrap();
        assert_eq!(map.bindings[plugin], vec![Chord::parse("L").unwrap()]);
        assert!(Keymap::decode(
            r#"{"keybindings":{"tool.brush":"M","toolbox.mode.selection_rectangle":"M"}}"#
        )
        .is_err());
    }
    #[test]
    fn wand_default_yields_to_explicit_older_w_bindings() {
        let wand = COMMANDS
            .iter()
            .position(|c| c.id == "toolbox.mode.selection_wand")
            .unwrap();
        assert_eq!(
            Keymap::default().bindings[wand],
            vec![Chord::parse("W").unwrap()]
        );
        for text in [
            r#"{"keybindings":{"tool.brush":"W"}}"#,
            r#"{"keybindings":{"plugin.custom.tool":"W"}}"#,
        ] {
            let map = Keymap::decode(text).unwrap();
            assert!(map.bindings[wand].is_empty());
            assert_eq!(
                Keymap::decode(&map.encode()).unwrap().bindings,
                map.bindings
            );
        }
    }
    #[test]
    fn selection_to_layer_defaults_preserve_explicit_duplicate_and_plugin_bindings() {
        let copy = COMMANDS
            .iter()
            .position(|c| c.id == "layer.via_copy")
            .unwrap();
        let cut = COMMANDS
            .iter()
            .position(|c| c.id == "layer.via_cut")
            .unwrap();
        assert_eq!(
            Keymap::default().bindings[copy],
            vec![Chord::parse("Ctrl+J").unwrap()]
        );
        assert_eq!(
            Keymap::default().bindings[cut],
            vec![Chord::parse("Ctrl+Shift+J").unwrap()]
        );
        for config in [
            r#"{"keybindings":{"layer.duplicate":"Ctrl+J","plugin.custom.tool":"Ctrl+Shift+J"}}"#,
            r#"{"keybindings":{"plugin.custom.tool":"Ctrl+J","tool.brush":"Ctrl+Shift+J"}}"#,
        ] {
            let map = Keymap::decode(config).unwrap();
            assert!(map.bindings[copy].is_empty());
            assert!(map.bindings[cut].is_empty());
            assert_eq!(
                Keymap::decode(&map.encode()).unwrap().bindings,
                map.bindings
            );
        }
        let map =
            Keymap::decode(r#"{"keybindings":{"layer.via_copy":"Ctrl+Alt+J","layer.via_cut":[]}}"#)
                .unwrap();
        assert_eq!(
            map.bindings[copy],
            vec![Chord::parse("Ctrl+Alt+J").unwrap()]
        );
        assert!(map.bindings[cut].is_empty());
    }
    #[test]
    fn image_size_and_system_clipboard_defaults_yield_to_explicit_older_bindings() {
        for (id, key) in [
            ("canvas.image_size", "Ctrl+Alt+I"),
            ("clipboard.system_copy", "Ctrl+C"),
            ("clipboard.system_copy_merged", "Ctrl+Shift+C"),
            ("clipboard.system_cut", "Ctrl+X"),
            ("clipboard.system_paste", "Ctrl+V"),
            ("clipboard.system_paste_in_place", "Ctrl+Shift+V"),
        ] {
            let index = COMMANDS.iter().position(|c| c.id == id).unwrap();
            assert_eq!(
                Keymap::default().bindings[index],
                vec![Chord::parse(key).unwrap()]
            );
            let text = serde_json::json!({"keybindings":{"plugin.custom.tool":[key]}}).to_string();
            let map = Keymap::decode(&text).unwrap();
            assert!(map.bindings[index].is_empty());
            assert_eq!(
                Keymap::decode(&map.encode()).unwrap().bindings,
                map.bindings
            );
        }
    }
}
