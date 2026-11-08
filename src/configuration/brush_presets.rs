//! Bounded profile data, never registry contributions or executable plugins.
use crate::application::{OpacityMode, Parameters, TipShape};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

pub const MAX_PRESETS: usize = 128;
pub const MAX_BUNDLE_BYTES: usize = 1024 * 1024;
pub const BUNDLE_FORMAT: &str = "micopi.brush-presets";
pub(crate) const REQUIRED_PARAMETER_KEYS: [&str; 12] = [
    "radius",
    "opacity",
    "hardness",
    "stabilizer",
    "pressure_size",
    "pressure_opacity",
    "pressure_gamma",
    "minimum_radius",
    "spacing",
    "minimum_opacity",
    "flow",
    "opacity_mode",
];
const OPTIONAL_PARAMETER_KEYS: [&str; 6] = [
    "tip_shape",
    "tip_roundness",
    "tip_angle",
    "symmetry",
    "symmetry_x",
    "symmetry_y",
];

/// A complete scalar/dynamics snapshot. Artist foreground/background swatches
/// are deliberately outside the format and survive every preset application.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PresetParameters {
    pub radius: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub stabilizer: f32,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub pressure_gamma: f32,
    pub minimum_radius: f32,
    #[serde(serialize_with = "crate::application::serialize_spacing")]
    pub spacing: f32,
    pub minimum_opacity: f32,
    pub flow: f32,
    pub opacity_mode: OpacityMode,
    #[serde(skip_serializing_if = "TipShape::is_round")]
    pub tip_shape: TipShape,
    #[serde(skip_serializing_if = "crate::application::full_tip_roundness")]
    pub tip_roundness: f32,
    #[serde(skip_serializing_if = "crate::application::zero_tip_angle")]
    pub tip_angle: f32,
    #[serde(skip_serializing_if = "crate::application::SymmetryMode::is_off")]
    pub symmetry: crate::application::SymmetryMode,
    #[serde(skip_serializing_if = "crate::application::brush_symmetry::is_center")]
    pub symmetry_x: f32,
    #[serde(skip_serializing_if = "crate::application::brush_symmetry::is_center")]
    pub symmetry_y: f32,
}
impl PresetParameters {
    fn from_validated(p: &Parameters) -> Self {
        Self {
            radius: p.radius,
            opacity: p.opacity,
            hardness: p.hardness,
            stabilizer: p.stabilizer,
            pressure_size: p.pressure_size,
            pressure_opacity: p.pressure_opacity,
            pressure_gamma: p.pressure_gamma,
            minimum_radius: p.minimum_radius,
            spacing: p.spacing,
            minimum_opacity: p.minimum_opacity,
            flow: p.flow,
            opacity_mode: p.opacity_mode,
            tip_shape: p.tip_shape,
            tip_roundness: p.tip_roundness,
            tip_angle: p.tip_angle,
            symmetry: p.symmetry,
            symmetry_x: p.symmetry_x,
            symmetry_y: p.symmetry_y,
        }
    }
    pub fn capture(p: &Parameters) -> Result<Self, String> {
        let result = Self::from_validated(p);
        result.validate()?;
        Ok(result)
    }
    pub fn decode_value(raw: &Value) -> Result<Self, String> {
        let object = raw
            .as_object()
            .ok_or("Preset parameters must be a complete object")?;
        if REQUIRED_PARAMETER_KEYS
            .iter()
            .any(|key| !object.contains_key(*key))
            || object.keys().any(|key| {
                !REQUIRED_PARAMETER_KEYS.contains(&key.as_str())
                    && !OPTIONAL_PARAMETER_KEYS.contains(&key.as_str())
            })
        {
            return Err("Preset parameters require every scalar/dynamics field and no color or unknown fields".into());
        }
        // Validate original JSON numbers before any f32 conversion can round an
        // out-of-domain literal onto a valid native boundary.
        let p = Parameters::default()
            .patched(raw)
            .map_err(|e| e.to_string())?;
        Ok(Self::from_validated(&p))
    }
    pub fn value(&self) -> Result<Value, String> {
        let value = canonical_value(self)?;
        Parameters::default()
            .patched(&value)
            .map_err(|e| e.to_string())?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.value().map(|_| ())
    }
    pub fn apply_to(&self, current: &Parameters) -> Result<Parameters, String> {
        let mut next = Parameters::default()
            .patched(&self.value()?)
            .map_err(|e| e.to_string())?;
        next.color = current.color.clone();
        next.background_color = current.background_color.clone();
        Ok(next)
    }
}
impl<'de> Deserialize<'de> for PresetParameters {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::decode_value(&Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserPreset {
    pub id: u64,
    pub name: String,
    pub owner: String,
    pub tool_id: String,
    pub parameters: PresetParameters,
}
impl UserPreset {
    pub fn new(
        id: u64,
        name: String,
        owner: String,
        tool_id: String,
        parameters: PresetParameters,
    ) -> Result<Self, String> {
        let preset = Self {
            id,
            name,
            owner,
            tool_id,
            parameters,
        };
        preset.validate()?;
        Ok(preset)
    }
    pub fn decode_value(raw: &Value) -> Result<Self, String> {
        let value: Self = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id == 0 || self.id > u32::MAX as u64 {
            return Err("Brush preset IDs must be positive 32-bit identities".into());
        }
        if self.name.trim().is_empty()
            || self.name.len() > 512
            || self.name.chars().any(char::is_control)
        {
            return Err("Brush preset names require 1 to 512 UTF-8 bytes without controls".into());
        }
        if !crate::plugins::valid_id(&self.owner) || !crate::plugins::valid_id(&self.tool_id) {
            return Err(
                "Brush preset owner and tool_id must be exact bounded contribution identifiers"
                    .into(),
            );
        }
        self.parameters.validate()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BrushPresetLibrary {
    pub version: u32,
    pub next_id: u64,
    pub items: Vec<UserPreset>,
}
impl Default for BrushPresetLibrary {
    fn default() -> Self {
        Self {
            version: 1,
            next_id: 1,
            items: Vec::new(),
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryWire {
    version: u32,
    next_id: u64,
    items: Vec<UserPreset>,
}
impl BrushPresetLibrary {
    pub fn decode_value(raw: &Value) -> Result<Self, String> {
        preflight_items(raw)?;
        let wire: LibraryWire = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let library = Self {
            version: wire.version,
            next_id: wire.next_id,
            items: wire.items,
        };
        library.validate()?;
        Ok(library)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.next_id == 0
            || self.next_id > u32::MAX as u64 + 1
            || self.items.len() > MAX_PRESETS
        {
            return Err("Unsupported brush preset library version, count or allocator".into());
        }
        let mut previous = 0;
        for item in &self.items {
            item.validate()?;
            if item.id <= previous || item.id >= self.next_id {
                return Err("Brush preset IDs must be sorted, unique and below next_id".into());
            }
            previous = item.id;
        }
        Ok(())
    }
    pub fn is_default(&self) -> bool {
        self.version == 1 && self.next_id == 1 && self.items.is_empty()
    }
    pub fn get(&self, id: u64) -> Option<&UserPreset> {
        self.items.iter().find(|item| item.id == id)
    }
    pub fn insert(
        &mut self,
        name: String,
        owner: String,
        tool_id: String,
        parameters: PresetParameters,
    ) -> Result<u64, String> {
        self.validate()?;
        if self.items.len() == MAX_PRESETS || self.next_id > u32::MAX as u64 {
            return Err("Brush preset count or identity limit reached".into());
        }
        let item = UserPreset::new(self.next_id, name, owner, tool_id, parameters)?;
        let id = item.id;
        self.items.push(item);
        self.next_id = id + 1;
        Ok(id)
    }
    pub fn remove(&mut self, id: u64) -> Result<bool, String> {
        self.validate()?;
        let Some(index) = self.items.iter().position(|item| item.id == id) else {
            return Ok(false);
        };
        self.items.remove(index);
        Ok(true)
    }
}
impl<'de> Deserialize<'de> for BrushPresetLibrary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::decode_value(&Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BrushPresetBundle {
    pub format: String,
    pub version: u32,
    pub items: Vec<UserPreset>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BundleWire {
    format: String,
    version: u32,
    items: Vec<UserPreset>,
}
impl BrushPresetBundle {
    pub fn from_items(items: Vec<UserPreset>) -> Result<Self, String> {
        let bundle = Self {
            format: BUNDLE_FORMAT.into(),
            version: 1,
            items,
        };
        bundle.validate()?;
        if encode_canonical(&bundle)?.len() > MAX_BUNDLE_BYTES {
            return Err("Brush preset bundle exceeds 1 MiB".into());
        }
        Ok(bundle)
    }
    pub fn decode(text: &str) -> Result<Self, String> {
        if text.len() > MAX_BUNDLE_BYTES {
            return Err("Brush preset bundle exceeds 1 MiB".into());
        }
        let raw: Value =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        Self::decode_value(&raw)
    }
    pub fn decode_value(raw: &Value) -> Result<Self, String> {
        preflight_items(raw)?;
        let wire: BundleWire = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let bundle = Self {
            format: wire.format,
            version: wire.version,
            items: wire.items,
        };
        bundle.validate()?;
        if encode_canonical(&bundle)?.len() > MAX_BUNDLE_BYTES {
            return Err("Brush preset bundle exceeds 1 MiB".into());
        }
        Ok(bundle)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format != BUNDLE_FORMAT || self.version != 1 || self.items.len() > MAX_PRESETS {
            return Err("Unsupported brush preset bundle format, version or count".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for item in &self.items {
            item.validate()?;
            if !ids.insert(item.id) {
                return Err("Duplicate brush preset bundle identity".into());
            }
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<String, String> {
        self.validate()?;
        let text = encode_canonical(self)?;
        if text.len() > MAX_BUNDLE_BYTES {
            return Err("Brush preset bundle exceeds 1 MiB".into());
        }
        Ok(text)
    }
}
impl<'de> Deserialize<'de> for BrushPresetBundle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::decode_value(&Value::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
fn preflight_items(raw: &Value) -> Result<(), String> {
    let items = raw
        .get("items")
        .and_then(Value::as_array)
        .ok_or("Brush presets require an items array")?;
    if items.len() > MAX_PRESETS {
        return Err("Brush preset count exceeds 128".into());
    }
    Ok(())
}
/// A serialization round trip keeps native f32 decimals at their shortest wire
/// representation. Direct to_value would widen them before schema validation.
pub(crate) fn canonical_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::from_str(&serde_json::to_string(value).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
pub(crate) fn encode_canonical<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n")
}

#[cfg(test)]
#[path = "brush_presets_tests.rs"]
mod tests;
