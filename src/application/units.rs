//! Cold physical-unit conversion. Pixel bounds apply after positive half-up rounding.
use crate::engine::document_info::Info;
use serde::{Deserialize, Serialize};

/// Exact metric axes, or the document's scalar default. Unknown PNG units stay unknown.
pub(crate) fn pixels_per_inch(info: Info) -> Option<[f64; 2]> {
    match info.png_density {
        Some(d) if d.metric() => Some([d.xppu as f64 * 0.0254, d.yppu as f64 * 0.0254]),
        None => Some([info.resolution_ppi as f64; 2]),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Unit {
    #[default]
    #[serde(rename = "px")]
    Pixels,
    #[serde(rename = "mm")]
    Millimeters,
    #[serde(rename = "in")]
    Inches,
}
impl Unit {
    pub(crate) fn to_pixels(self, value: f64, ppi: Option<f64>) -> Result<usize, &'static str> {
        if !value.is_finite() || value <= 0. {
            return Err("Dimensions must be finite and positive");
        }
        let pixels = match self {
            Self::Pixels => {
                if value.fract() != 0. {
                    return Err("Pixel dimensions must be integers");
                }
                value
            }
            Self::Millimeters | Self::Inches => {
                let ppi = ppi
                    .filter(|v| v.is_finite() && *v > 0.)
                    .ok_or("Physical dimensions require known density or an explicit resolution")?;
                match self {
                    Self::Millimeters => value / 25.4 * ppi,
                    _ => value * ppi,
                }
            }
        };
        rounded_pixels(pixels)
    }

    /// Presentation conversion for already validated pixels and positive density.
    #[cfg(feature = "desktop")]
    pub(crate) fn from_pixels(self, pixels: usize, ppi: f64) -> f64 {
        match self {
            Self::Pixels => pixels as f64,
            Self::Millimeters => pixels as f64 / ppi * 25.4,
            Self::Inches => pixels as f64 / ppi,
        }
    }
}

pub(crate) fn rounded_pixels(value: f64) -> Result<usize, &'static str> {
    if !value.is_finite() || value <= 0. {
        return Err("Dimensions must be finite and positive");
    }
    let rounded = value.round();
    if !(1.0..=4096.0).contains(&rounded) {
        return Err("Derived dimensions must be between 1 and 4096 pixels");
    }
    Ok(rounded as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_rounding_rejects_zero_overflow_unknown_density_and_fractional_pixels() {
        assert_eq!(Unit::Inches.to_pixels(0.5, Some(1.)), Ok(1));
        assert_eq!(Unit::Inches.to_pixels(4096.49, Some(1.)), Ok(4096));
        assert!(Unit::Inches.to_pixels(4096.5, Some(1.)).is_err());
        assert!(Unit::Inches.to_pixels(0.49, Some(1.)).is_err());
        assert_eq!(Unit::Millimeters.to_pixels(25.4, Some(300.)), Ok(300));
        assert!(Unit::Millimeters.to_pixels(25.4, None).is_err());
        for value in [0., -1., f64::NAN, f64::INFINITY, 1.5] {
            assert!(Unit::Pixels.to_pixels(value, None).is_err());
        }
        assert_eq!(Unit::Pixels.to_pixels(1., None), Ok(1));
    }
}
