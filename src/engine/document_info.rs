//! Bounded physical-resolution metadata; it never changes raster sampling.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored")]
pub struct Info {
    pub resolution_ppi: u32,
    #[serde(with = "background")]
    pub initial_background: u32,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Stored {
    resolution_ppi: u32,
    #[serde(with = "background")]
    initial_background: u32,
}
impl Default for Stored {
    fn default() -> Self {
        Self {
            resolution_ppi: 72,
            initial_background: 0,
        }
    }
}
impl Default for Info {
    fn default() -> Self {
        Self {
            resolution_ppi: 72,
            initial_background: 0,
        }
    }
}
impl Info {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=2400).contains(&self.resolution_ppi) {
            return Err("Resolution must be between 1 and 2400 pixels per inch".into());
        }
        if self.initial_background != 0 && self.initial_background >> 24 != 255 {
            return Err("Initial background must be transparent or an opaque RGB color".into());
        }
        Ok(())
    }
    pub fn parse_background(text: &str) -> Result<u32, String> {
        if text == "transparent" {
            return Ok(0);
        }
        if text.len() == 7
            && text.starts_with('#')
            && text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Ok(0xff000000 | u32::from_str_radix(&text[1..], 16).unwrap());
        }
        Err("Background must be transparent or #RRGGBB".into())
    }
    pub fn background_text(color: u32) -> String {
        if color == 0 {
            "transparent".into()
        } else {
            format!("#{:06X}", color & 0xffffff)
        }
    }
    pub fn state(&self, width: usize, height: usize) -> serde_json::Value {
        let mut value = serde_json::to_value(self).expect("bounded document info");
        value["width"] = serde_json::json!(width);
        value["height"] = serde_json::json!(height);
        value["width_mm"] = serde_json::json!(width as f64 * 25.4 / self.resolution_ppi as f64);
        value["height_mm"] = serde_json::json!(height as f64 * 25.4 / self.resolution_ppi as f64);
        value
    }
}
impl TryFrom<Stored> for Info {
    type Error = String;
    fn try_from(s: Stored) -> Result<Self, String> {
        let info = Self {
            resolution_ppi: s.resolution_ppi,
            initial_background: s.initial_background,
        };
        info.validate()?;
        Ok(info)
    }
}
mod background {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u32, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::Info::background_text(*value))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
        super::Info::parse_background(&String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
