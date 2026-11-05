//! Profile-owned document creation presets; loading these does not modify artwork.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanvasPreset {
    pub id: String,
    pub name: String,
    pub width: usize,
    pub height: usize,
    pub resolution_ppi: u32,
    pub background: String,
}
impl CanvasPreset {
    pub fn validate(&self) -> Result<(), String> {
        if !crate::plugins::valid_id(&self.id) {
            return Err("new_canvas preset IDs must be valid identifiers".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.name.chars().any(char::is_control)
        {
            return Err(
                "new_canvas preset names must contain 1 to 128 bytes without controls".into(),
            );
        }
        if self.width == 0 || self.height == 0 || self.width > 4096 || self.height > 4096 {
            return Err("new_canvas preset dimensions must be between 1 and 4096 pixels".into());
        }
        if !(1..=2400).contains(&self.resolution_ppi) {
            return Err("new_canvas preset resolution must be between 1 and 2400 PPI".into());
        }
        crate::engine::document_info::Info::parse_background(&self.background)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NewCanvasPreferences {
    pub default_preset: String,
    pub presets: Vec<CanvasPreset>,
}
impl Default for NewCanvasPreferences {
    fn default() -> Self {
        let preset =
            |id: &str, name: &str, width, height, resolution_ppi, background: &str| CanvasPreset {
                id: id.into(),
                name: name.into(),
                width,
                height,
                resolution_ppi,
                background: background.into(),
            };
        Self {
            default_preset: "illustration".into(),
            presets: vec![
                preset("illustration", "Illustration", 1200, 900, 72, "transparent"),
                preset("square", "Square", 2048, 2048, 300, "transparent"),
                preset("a4", "A4", 2480, 3508, 300, "#FFFFFF"),
                preset("b5", "B5", 2150, 3035, 300, "#FFFFFF"),
            ],
        }
    }
}
impl NewCanvasPreferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.presets.is_empty() || self.presets.len() > 32 {
            return Err("new_canvas.presets must contain between 1 and 32 presets".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for preset in &self.presets {
            preset.validate()?;
            if !seen.insert(&preset.id) {
                return Err("new_canvas.presets contains duplicate IDs".into());
            }
        }
        if !seen.contains(&self.default_preset) {
            return Err("new_canvas.default_preset must identify an existing preset".into());
        }
        Ok(())
    }
    pub fn selected(&self) -> Option<&CanvasPreset> {
        self.presets.iter().find(|p| p.id == self.default_preset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::Settings;
    use serde_json::{json, Value};
    use std::path::PathBuf;

    #[test]
    fn new_canvas_preferences_roundtrip_preserves_custom_names_and_other_settings() {
        let mut config = Settings::decode(
            r#"{"custom":{"keep":42},"keybindings":{"file.save":"Ctrl+F12"}}"#,
            PathBuf::new(),
        )
        .unwrap();
        assert_eq!(config.new_canvas, NewCanvasPreferences::default());
        config.new_canvas.presets[0].name = "私のページ · 나의 그림".into();
        config.new_canvas.default_preset = "a4".into();
        let encoded = config.encode().unwrap();
        let copy = Settings::decode(&encoded, PathBuf::new()).unwrap();
        assert_eq!(copy.new_canvas, config.new_canvas);
        assert_eq!(copy.new_canvas.selected().unwrap().width, 2480);
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["custom"], json!({"keep":42}));
        assert_eq!(value["keybindings"]["file.save"], json!(["Ctrl+F12"]));
    }

    #[test]
    fn new_canvas_presets_reject_invalid_dimensions_resolution_colors_and_names() {
        let valid = serde_json::to_value(NewCanvasPreferences::default()).unwrap();
        for (key, value) in [
            ("id", json!("")),
            ("id", json!("UpperCase")),
            ("name", json!("")),
            ("name", json!(" \t ")),
            ("name", json!("x".repeat(129))),
            ("name", json!("bad\nname")),
            ("width", json!(0)),
            ("height", json!(4097)),
            ("width", json!(1.5)),
            ("width", json!(-1)),
            ("resolution_ppi", json!(0)),
            ("resolution_ppi", json!(2401)),
            ("resolution_ppi", json!(72.5)),
            ("background", json!("white")),
            ("background", json!("#fff")),
            ("background", json!("#FFFFFFFF")),
            ("background", json!("#GGGGGG")),
            ("typo", json!(true)),
        ] {
            let mut edited = valid.clone();
            edited["presets"][0][key] = value;
            assert!(
                Settings::decode(&json!({"new_canvas":edited}).to_string(), PathBuf::new())
                    .is_err(),
                "{key}: {edited}"
            );
        }
        for color in ["transparent", "#FFFFFF", "#aabbcc", "#000000"] {
            let mut prefs = NewCanvasPreferences::default();
            prefs.presets[0].background = color.into();
            prefs.validate().unwrap();
        }
    }

    #[test]
    fn new_canvas_preferences_bound_the_list_and_default_reference_on_decode_and_encode() {
        let mut prefs = NewCanvasPreferences::default();
        for bad in [
            json!({"default_preset":"missing"}),
            json!({"presets":[]}),
            json!({"unknown":true}),
        ] {
            assert!(
                Settings::decode(&json!({"new_canvas":bad}).to_string(), PathBuf::new()).is_err()
            );
        }
        prefs.presets.push(prefs.presets[0].clone());
        assert!(prefs.validate().is_err());
        prefs.presets = (0..33)
            .map(|i| {
                let mut p = prefs.presets[0].clone();
                p.id = format!("preset{i}");
                p
            })
            .collect();
        prefs.default_preset = "preset0".into();
        assert!(prefs.validate().is_err());
        prefs.presets.pop();
        prefs.validate().unwrap();
        let mut config = Settings::defaults(PathBuf::new());
        config.new_canvas = prefs;
        config.encode().unwrap();
        config.new_canvas.default_preset = "absent".into();
        assert!(config.encode().is_err());
    }
}
