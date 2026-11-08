//! Integer threshold and cold posterization tables for direct raster edits.
use serde_json::Value;

pub(super) enum Quantize {
    Threshold(u32),
    Posterize([u8; 256]),
}
pub(super) fn parse(handler: &str, args: &Value) -> Result<Quantize, String> {
    let (key, min, max, default) = match handler {
        "threshold" => ("level", 0, 255, 128),
        "posterize" => ("levels", 2, 256, 4),
        _ => return Err("Unknown quantization operation".into()),
    };
    let object = args.as_object().ok_or("Quantization requires an object")?;
    if object.keys().any(|name| name != key) {
        return Err("Unknown quantization parameter".into());
    }
    let number = match object.get(key) {
        None => default,
        Some(value) => value
            .as_f64()
            .filter(|v| v.is_finite() && v.fract() == 0. && (*v >= min as f64 && *v <= max as f64))
            .map(|v| v as u32)
            .ok_or_else(|| format!("Invalid {key}"))?,
    };
    if handler == "threshold" {
        Ok(Quantize::Threshold(number * 256))
    } else {
        let intervals = number - 1;
        Ok(Quantize::Posterize(std::array::from_fn(|byte| {
            let bucket = (byte as u32 * intervals + 127) / 255;
            ((bucket * 255 + intervals / 2) / intervals) as u8
        })))
    }
}
impl Quantize {
    #[inline]
    pub(super) fn apply(&self, pixel: u32) -> u32 {
        if pixel >> 24 == 0 {
            return pixel;
        }
        let r = (pixel >> 16) & 255;
        let g = (pixel >> 8) & 255;
        let b = pixel & 255;
        let rgb = match self {
            Self::Threshold(cutoff) => {
                if 54 * r + 183 * g + 19 * b >= *cutoff {
                    0xffffff
                } else {
                    0
                }
            }
            Self::Posterize(table) => {
                (table[r as usize] as u32) << 16
                    | (table[g as usize] as u32) << 8
                    | table[b as usize] as u32
            }
        };
        (pixel & 0xff000000) | rgb
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn all_posterize_levels_and_bytes_match_nearest_palette_reference() {
        for levels in 2..=256 {
            let kernel = parse("posterize", &json!({"levels":levels})).unwrap();
            let palette: Vec<u32> = (0..levels)
                .map(|i| (i as f64 * 255. / (levels - 1) as f64).round() as u32)
                .collect();
            for byte in 0..256u32 {
                // Bucket boundaries are in the unrounded evenly spaced domain.
                let bucket = (byte as f64 * (levels - 1) as f64 / 255.).round() as usize;
                let expected = palette[bucket];
                for alpha in [0, 37, 128, 255] {
                    let pixel = alpha << 24 | byte << 16 | (255 - byte) << 8 | byte;
                    let inverse_bucket =
                        ((255 - byte) as f64 * (levels - 1) as f64 / 255.).round() as usize;
                    assert_eq!(
                        kernel.apply(pixel),
                        if alpha == 0 {
                            pixel
                        } else {
                            alpha << 24 | expected << 16 | palette[inverse_bucket] << 8 | expected
                        }
                    );
                }
            }
        }
    }
    #[test]
    fn threshold_preserves_alpha_hidden_bytes_and_exact_unrounded_boundaries() {
        for level in [0, 1, 54, 127, 128, 254, 255] {
            let kernel = parse("threshold", &json!({"level":level})).unwrap();
            for r in [0, 1, 63, 127, 128, 192, 254, 255] {
                for g in [0, 1, 63, 127, 128, 192, 254, 255] {
                    for b in [0, 1, 63, 127, 128, 192, 254, 255] {
                        let white = (r as f64 * 54. + g as f64 * 183. + b as f64 * 19.) / 256.
                            >= level as f64;
                        for alpha in [0, 37, 128, 255] {
                            let pixel = alpha << 24 | r << 16 | g << 8 | b;
                            assert_eq!(
                                kernel.apply(pixel),
                                if alpha == 0 {
                                    pixel
                                } else {
                                    alpha << 24 | if white { 0xffffff } else { 0 }
                                }
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            parse("threshold", &json!({"level":54}))
                .unwrap()
                .apply(0x80ff0000),
            0x80000000
        );
        assert_eq!(
            parse("threshold", &json!({"level":53}))
                .unwrap()
                .apply(0x80ff0000),
            0x80ffffff
        );
        assert_eq!(
            parse("posterize", &json!({})).unwrap().apply(0x80204060),
            0x80005555
        );
    }
    #[test]
    fn reject_invalid_parameters_before_pixel_loans() {
        for handler in ["threshold", "posterize"] {
            let key = if handler == "threshold" {
                "level"
            } else {
                "levels"
            };
            for value in [
                json!(null),
                json!([]),
                json!({"unknown":1}),
                json!({(key):null}),
                json!({(key):1.5}),
                json!({(key):-1}),
                json!({(key):257}),
                json!({(key):"4"}),
            ] {
                assert!(parse(handler, &value).is_err(), "{handler}: {value}");
            }
        }
        assert!(parse("posterize", &json!({"levels":1})).is_err());
        assert!(parse("threshold", &json!({"level":256})).is_err());
    }
}
