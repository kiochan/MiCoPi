//! Bounded physical-resolution metadata; it never changes raster sampling.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DensityUnit {
    Unspecified,
    Meter,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "StoredDensity")]
pub struct Density {
    pub xppu: u32,
    pub yppu: u32,
    pub unit: DensityUnit,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredDensity {
    xppu: u32,
    yppu: u32,
    unit: DensityUnit,
}
impl Density {
    pub fn validate(&self) -> Result<(), String> {
        if self.xppu > i32::MAX as u32 || self.yppu > i32::MAX as u32 {
            return Err("PNG density axes must be between 0 and 2147483647".into());
        }
        Ok(())
    }
    pub fn from_ppi(ppi: u32) -> Self {
        let axis = ((u64::from(ppi) * 10000 + 127) / 254) as u32;
        Self {
            xppu: axis,
            yppu: axis,
            unit: DensityUnit::Meter,
        }
    }
    fn nominal_ppi(&self) -> u32 {
        if self.unit == DensityUnit::Meter && self.xppu > 0 {
            (self.xppu as f64 * 0.0254).round().clamp(1., 2400.) as u32
        } else {
            72
        }
    }
    pub fn metric(&self) -> bool {
        self.unit == DensityUnit::Meter && self.xppu > 0 && self.yppu > 0
    }
}
impl TryFrom<StoredDensity> for Density {
    type Error = String;
    fn try_from(s: StoredDensity) -> Result<Self, String> {
        let d = Self {
            xppu: s.xppu,
            yppu: s.yppu,
            unit: s.unit,
        };
        d.validate()?;
        Ok(d)
    }
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored")]
pub struct Info {
    pub resolution_ppi: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub resolution_explicit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png_density: Option<Density>,
    #[serde(with = "background")]
    pub initial_background: u32,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Stored {
    resolution_ppi: u32,
    resolution_explicit: bool,
    png_density: Option<Density>,
    #[serde(with = "background")]
    initial_background: u32,
}
impl Default for Stored {
    fn default() -> Self {
        Self {
            resolution_ppi: 72,
            resolution_explicit: false,
            png_density: None,
            initial_background: 0,
        }
    }
}
impl Default for Info {
    fn default() -> Self {
        Self {
            resolution_ppi: 72,
            resolution_explicit: false,
            png_density: None,
            initial_background: 0,
        }
    }
}
impl Info {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(density) = self.png_density {
            density.validate()?;
        }
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
    pub fn with_png_density(density: Density) -> Self {
        Self {
            resolution_ppi: density.nominal_ppi(),
            png_density: Some(density),
            ..Self::default()
        }
    }
    pub fn export_density(&self) -> Option<Density> {
        self.png_density.or_else(|| {
            (self.resolution_explicit || self.resolution_ppi != 72)
                .then(|| Density::from_ppi(self.resolution_ppi))
        })
    }
    pub fn rotated_quarter(mut self) -> Self {
        if let Some(d) = self.png_density.as_mut() {
            std::mem::swap(&mut d.xppu, &mut d.yppu);
            self.resolution_ppi = d.nominal_ppi();
        }
        self
    }
    pub fn state(&self, width: usize, height: usize) -> serde_json::Value {
        let mut value = serde_json::to_value(self).expect("bounded document info");
        value["width"] = serde_json::json!(width);
        value["height"] = serde_json::json!(height);
        value["resolution_source"] = serde_json::json!(if self.png_density.is_some() {
            "png"
        } else {
            "document"
        });
        let metric = self.png_density.is_none_or(|d| d.metric());
        value["physical_size_known"] = serde_json::json!(metric);
        if metric {
            let (x, y) = self
                .png_density
                .map(|d| (d.xppu as f64 * 0.0254, d.yppu as f64 * 0.0254))
                .unwrap_or((self.resolution_ppi as f64, self.resolution_ppi as f64));
            value["resolution_ppi_x"] = serde_json::json!(x);
            value["resolution_ppi_y"] = serde_json::json!(y);
            value["width_mm"] = serde_json::json!(width as f64 * 25.4 / x);
            value["height_mm"] = serde_json::json!(height as f64 * 25.4 / y);
        }
        value
    }
}
impl TryFrom<Stored> for Info {
    type Error = String;
    fn try_from(s: Stored) -> Result<Self, String> {
        let info = Self {
            resolution_ppi: s.resolution_ppi,
            resolution_explicit: s.resolution_explicit,
            png_density: s.png_density,
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
