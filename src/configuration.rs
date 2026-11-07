//! One configuration document; UI edits preserve shortcuts and unknown root fields.
use crate::{platform::replace_file, ui::keymap::Keymap};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
mod canvas;
pub use canvas::{CanvasPreset, NewCanvasPreferences};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub language: crate::i18n::Language,
    pub theme: String,
    pub font_size: f32,
    pub small_font_size: f32,
    pub item_spacing: f32,
    pub section_spacing: f32,
    pub panel_padding: f32,
    pub outer_padding: f32,
    pub control_height: f32,
    pub title_height: f32,
    pub toolbar_height: f32,
    pub tab_height: f32,
    pub corner_radius: f32,
    pub icon_size: f32,
    pub left_panel_width: f32,
    pub toolbox_width: f32,
    pub toolbox_height: f32,
    pub right_panel_width: f32,
    pub canvas_margin: f32,
    pub accent: String,
    /// Tools-strip entries shown first, in this order; unlisted entries follow
    /// in their default order. IDs are tool IDs plus `fill` and `gradient`.
    pub tool_order: Vec<String>,
    /// Tools-strip entries that are not shown.
    pub hidden_tools: Vec<String>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            language: crate::i18n::Language::English,
            theme: "graphite".into(),
            font_size: 13.,
            small_font_size: 12.,
            item_spacing: 4.,
            section_spacing: 12.,
            panel_padding: 8.,
            outer_padding: 6.,
            control_height: 26.,
            title_height: 36.,
            toolbar_height: 34.,
            tab_height: 30.,
            corner_radius: 8.,
            icon_size: 18.,
            left_panel_width: 244.,
            toolbox_width: 720.,
            toolbox_height: 80.,
            right_panel_width: 272.,
            canvas_margin: 24.,
            accent: "#B8A2E4".into(),
            tool_order: vec![],
            hidden_tools: vec![],
        }
    }
}
impl Appearance {
    pub fn compact(&mut self) {
        self.item_spacing = 4.;
        self.section_spacing = 8.;
        self.panel_padding = 6.;
        self.outer_padding = 4.;
        self.control_height = 24.;
        self.toolbar_height = 32.;
        self.tab_height = 28.;
    }
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, low, high) in [
            ("font_size", self.font_size, 11., 22.),
            ("small_font_size", self.small_font_size, 10., 18.),
            ("item_spacing", self.item_spacing, 4., 20.),
            ("section_spacing", self.section_spacing, 8., 40.),
            ("panel_padding", self.panel_padding, 6., 32.),
            ("outer_padding", self.outer_padding, 4., 24.),
            ("control_height", self.control_height, 24., 48.),
            ("title_height", self.title_height, 36., 60.),
            ("toolbar_height", self.toolbar_height, 32., 60.),
            ("tab_height", self.tab_height, 28., 52.),
            ("corner_radius", self.corner_radius, 0., 12.),
            ("icon_size", self.icon_size, 16., 28.),
            ("left_panel_width", self.left_panel_width, 200., 440.),
            ("toolbox_width", self.toolbox_width, 200., 1200.),
            ("toolbox_height", self.toolbox_height, 80., 200.),
            ("right_panel_width", self.right_panel_width, 200., 440.),
            ("canvas_margin", self.canvas_margin, 8., 96.),
        ] {
            if !value.is_finite() || value < low || value > high {
                return Err(format!("ui.{name} must be between {low} and {high}"));
            }
        }
        if !["graphite", "light", "warm"].contains(&self.theme.as_str()) {
            return Err("ui.theme must be graphite, light or warm".into());
        }
        if self.control_height < self.font_size + 8.
            || self.tab_height < self.font_size + 10.
            || self.toolbar_height < self.control_height + 6.
            || self.title_height < self.control_height + 6.
        {
            return Err(
                "Controls, tabs and toolbars must fit the font and vertical padding".into(),
            );
        }
        for (name, list) in [
            ("tool_order", &self.tool_order),
            ("hidden_tools", &self.hidden_tools),
        ] {
            if list.len() > 64
                || list
                    .iter()
                    .any(|id| !crate::plugins::valid_id(id) || id.len() > 64)
            {
                return Err(format!("ui.{name} must list at most 64 tool IDs"));
            }
        }
        if self.section_spacing < self.item_spacing {
            return Err("Section spacing cannot be smaller than item spacing".into());
        }
        if self.accent.len() != 7
            || !self.accent.starts_with('#')
            || u32::from_str_radix(&self.accent[1..], 16).is_err()
        {
            return Err("ui.accent requires a #RRGGBB color".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PaintingPreferences {
    pub flow: f32,
    pub opacity_mode: crate::application::OpacityMode,
    pub spacing: f32,
    pub minimum_opacity: f32,
    pub stabilizer: f32,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub pressure_gamma: f32,
    pub minimum_radius: f32,
}
impl Default for PaintingPreferences {
    fn default() -> Self {
        Self {
            flow: 1.,
            opacity_mode: crate::application::OpacityMode::Dab,
            spacing: 0.11,
            minimum_opacity: 0.,
            stabilizer: 0.35,
            pressure_size: true,
            pressure_opacity: false,
            pressure_gamma: 1.,
            minimum_radius: 0.08,
        }
    }
}
impl From<&crate::application::Parameters> for PaintingPreferences {
    fn from(p: &crate::application::Parameters) -> Self {
        Self {
            flow: p.flow,
            opacity_mode: p.opacity_mode,
            spacing: p.spacing,
            minimum_opacity: p.minimum_opacity,
            stabilizer: p.stabilizer,
            pressure_size: p.pressure_size,
            pressure_opacity: p.pressure_opacity,
            pressure_gamma: p.pressure_gamma,
            minimum_radius: p.minimum_radius,
        }
    }
}
impl PaintingPreferences {
    pub fn validate(&self) -> Result<(), String> {
        crate::application::Parameters::default()
            .patched(&serde_json::to_value(self).map_err(|e| e.to_string())?)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    pub fn apply(&self, parameters: &mut crate::application::Parameters) {
        parameters.flow = self.flow;
        parameters.opacity_mode = self.opacity_mode;
        parameters.spacing = self.spacing;
        parameters.minimum_opacity = self.minimum_opacity;
        parameters.stabilizer = self.stabilizer;
        parameters.pressure_size = self.pressure_size;
        parameters.pressure_opacity = self.pressure_opacity;
        parameters.pressure_gamma = self.pressure_gamma;
        parameters.minimum_radius = self.minimum_radius;
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginPreferences {
    pub disabled: Vec<String>,
}
impl PluginPreferences {
    fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for id in &self.disabled {
            if id.is_empty()
                || id.len() > 96
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
                || !seen.insert(id)
            {
                return Err("plugins.disabled contains invalid or duplicate plugin IDs".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone)]
pub struct Settings {
    pub ui: Appearance,
    pub keys: Keymap,
    pub painting: PaintingPreferences,
    pub selection: crate::application::SelectionSampling,
    pub plugins: PluginPreferences,
    pub movement: crate::application::MovementPreferences,
    pub new_canvas: NewCanvasPreferences,
    pub path: PathBuf,
    original: Value,
}
impl Settings {
    pub fn defaults(path: PathBuf) -> Self {
        Self {
            ui: Appearance::default(),
            keys: Keymap::default(),
            painting: PaintingPreferences::default(),
            selection: Default::default(),
            plugins: PluginPreferences::default(),
            movement: Default::default(),
            new_canvas: Default::default(),
            path,
            original: json!({"version": 1}),
        }
    }
    pub fn load(path: PathBuf) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::defaults(path));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Self::decode(&text, path)
    }
    pub fn decode(text: &str, path: PathBuf) -> Result<Self, String> {
        if text.len() > 1024 * 1024 {
            return Err("settings.json exceeds 1 MB".into());
        }
        let original: Value =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        if !original.is_object() {
            return Err("settings.json must contain a root object".into());
        }
        if original.get("version").is_some_and(|v| *v != json!(1)) {
            return Err("Unsupported settings.json version".into());
        }
        let ui: Appearance =
            serde_json::from_value(original.get("ui").cloned().unwrap_or(json!({})))
                .map_err(|e| e.to_string())?;
        ui.validate()?;
        let painting: PaintingPreferences =
            serde_json::from_value(original.get("painting").cloned().unwrap_or(json!({})))
                .map_err(|e| e.to_string())?;
        painting.validate()?;
        let plugins: PluginPreferences =
            serde_json::from_value(original.get("plugins").cloned().unwrap_or(json!({})))
                .map_err(|e| e.to_string())?;
        plugins.validate()?;
        let selection =
            serde_json::from_value(original.get("selection").cloned().unwrap_or(json!({})))
                .map_err(|e| format!("selection: {e}"))?;
        let movement: crate::application::MovementPreferences =
            serde_json::from_value(original.get("movement").cloned().unwrap_or(json!({})))
                .map_err(|e| format!("movement: {e}"))?;
        movement.validate()?;
        let new_canvas: NewCanvasPreferences =
            serde_json::from_value(original.get("new_canvas").cloned().unwrap_or(json!({})))
                .map_err(|e| format!("new_canvas: {e}"))?;
        new_canvas.validate()?;
        let bindings = json!({"version": 1, "keybindings": original.get("keybindings").cloned().unwrap_or(json!({}))});
        let keys = Keymap::decode(&bindings.to_string())?;
        Ok(Self {
            ui,
            keys,
            painting,
            selection,
            plugins,
            movement,
            new_canvas,
            path,
            original,
        })
    }
    pub fn has_painting_preferences(&self) -> bool {
        self.original.get("painting").is_some()
    }
    pub fn encode(&self) -> Result<String, String> {
        self.ui.validate()?;
        self.keys.validate()?;
        self.painting.validate()?;
        self.plugins.validate()?;
        self.movement.validate()?;
        self.new_canvas.validate()?;
        let mut value = self.original.clone();
        value["version"] = json!(1);
        value["movement"] = serde_json::to_value(self.movement).map_err(|e| e.to_string())?;
        value["new_canvas"] = serde_json::to_value(&self.new_canvas).map_err(|e| e.to_string())?;
        value["selection"] = serde_json::to_value(self.selection).map_err(|e| e.to_string())?;
        value["painting"] = serde_json::to_value(&self.painting).map_err(|e| e.to_string())?;
        value["plugins"] = serde_json::to_value(&self.plugins).map_err(|e| e.to_string())?;
        value["ui"] = serde_json::to_value(&self.ui).map_err(|e| e.to_string())?;
        value["keybindings"] = serde_json::from_str::<Value>(&self.keys.encode())
            .map_err(|e| e.to_string())?["keybindings"]
            .clone();
        Ok(serde_json::to_string_pretty(&value).map_err(|e| e.to_string())? + "\n")
    }
    pub fn save(&self) -> Result<(), String> {
        atomic_write(&self.path, &self.encode()?)
    }
}
pub fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    replace_file(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_preferences_are_typed_and_preserve_existing_profile_content() {
        let old = Settings::decode(
            r#"{"custom":{"keep":42},"keybindings":{"file.save":"Ctrl+F12"}}"#,
            PathBuf::new(),
        )
        .unwrap();
        assert_eq!(old.selection, Default::default());
        let mut value: Value = serde_json::from_str(&old.encode().unwrap()).unwrap();
        value["selection"] = json!({"tolerance":7,"contiguous":false,"sample_merged":true});
        let changed = Settings::decode(&value.to_string(), PathBuf::new()).unwrap();
        let roundtrip: Value = serde_json::from_str(&changed.encode().unwrap()).unwrap();
        assert_eq!(roundtrip["selection"], value["selection"]);
        assert_eq!(roundtrip["custom"], json!({"keep":42}));
        assert_eq!(roundtrip["keybindings"], value["keybindings"]);
        for bad in [
            json!({"tolerance":256}),
            json!({"tolerance":1.5}),
            json!({"tolerance":-1}),
            json!({"contiguous":1}),
            json!({"unknown":true}),
        ] {
            value["selection"] = bad;
            assert!(Settings::decode(&value.to_string(), PathBuf::new()).is_err());
        }
    }
    #[test]
    fn language_defaults_validation_and_roundtrip_preserve_other_settings() {
        assert_eq!(
            Settings::defaults(PathBuf::new()).ui.language,
            crate::i18n::Language::English
        );
        for locale in crate::i18n::Language::ALL {
            let text = json!({"version":1,"ui":{"language":locale.code()},"custom":{"value":42},"keybindings":{"file.save":["Ctrl+F12"]}}).to_string();
            let s = Settings::decode(&text, PathBuf::new()).unwrap();
            assert_eq!(s.ui.language, locale);
            let encoded = s.encode().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(&encoded).unwrap()["custom"]["value"],
                42
            );
            assert_eq!(
                Settings::decode(&encoded, PathBuf::new())
                    .unwrap()
                    .ui
                    .language,
                locale
            );
            assert!(encoded.contains("Ctrl+F12"));
        }
        for language in [json!("xx"), json!("zh"), json!(42), Value::Null] {
            assert!(Settings::decode(
                &json!({"ui":{"language":language}}).to_string(),
                PathBuf::new()
            )
            .is_err());
        }
    }
    #[test]
    fn appearance_edits_preserve_existing_shortcuts_and_extra_root_data() {
        let mut s = Settings::decode(r##"{"version":1,"keybindings":{"file.save":["Ctrl+F12"]},"extension":{"a":5},"ui":{"panel_padding":14,"accent":"#88BBCC"}}"##, PathBuf::new()).unwrap();
        s.ui.item_spacing = 10.;
        let copy = Settings::decode(&s.encode().unwrap(), PathBuf::new()).unwrap();
        assert_eq!(copy.ui, s.ui);
        assert_eq!(copy.original["extension"]["a"], 5);
        assert!(copy.keys.encode().contains("Ctrl+F12"));
    }
    #[test]
    fn invalid_metrics_are_rejected_before_affecting_layout() {
        for input in [
            r#"{"ui":{"outer_padding":-1}}"#,
            r#"{"ui":{"font_size":22,"control_height":24}}"#,
            r#"{"ui":{"section_spacing":8,"item_spacing":12}}"#,
            r#"{"ui":{"typo":4}}"#,
            r#"{"ui":{"accent":"red"}}"#,
        ] {
            assert!(Settings::decode(input, PathBuf::new()).is_err(), "{input}");
        }
        Appearance::default().validate().unwrap();
        let s = Settings::decode(
            r#"{"ui":{"tool_order":["zoom","brush"],"hidden_tools":["gradient"]}}"#,
            PathBuf::new(),
        )
        .unwrap();
        assert_eq!(s.ui.tool_order, ["zoom", "brush"]);
        assert_eq!(s.ui.hidden_tools, ["gradient"]);
        assert!(
            Settings::decode(r#"{"ui":{"tool_order":["Not An ID"]}}"#, PathBuf::new()).is_err()
        );
        let mut a = Appearance::default();
        a.compact();
        a.validate().unwrap();
        a.panel_padding = f32::NAN;
        assert!(a.validate().is_err());
    }
}
