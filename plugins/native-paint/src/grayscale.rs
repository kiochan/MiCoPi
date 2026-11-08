//! Encoded RGB luma; full strength uses the legacy filter's exact integer form.
use serde_json::Value;
#[derive(Clone, Copy)]
pub(super) struct Parameters {
    amount: f32,
}
pub(super) fn parse(value: &Value) -> Result<Parameters, String> {
    let object = value.as_object().ok_or("Grayscale requires an object")?;
    if object.keys().any(|key| key != "amount") {
        return Err("Unknown grayscale parameter".into());
    }
    let amount = match value.get("amount") {
        None => 100.,
        Some(value) => value
            .as_f64()
            .filter(|value| value.is_finite() && (0. ..=100.).contains(value))
            .ok_or("Invalid grayscale amount")?,
    };
    Ok(Parameters {
        amount: (amount / 100.) as f32,
    })
}
impl Parameters {
    #[inline]
    pub(super) fn apply(self, pixel: u32) -> u32 {
        if pixel >> 24 == 0 || self.amount == 0. {
            return pixel;
        }
        if self.amount == 1. {
            let gray = (54 * ((pixel >> 16) & 255)
                + 183 * ((pixel >> 8) & 255)
                + 19 * (pixel & 255)
                + 128)
                >> 8;
            return (pixel & 0xff000000) | gray * 0x010101;
        }
        crate::filter_math::filter(pixel, 1, self.amount)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn bounded_rgb_lattice_matches_independent_integer_luma_and_legacy_pixels() {
        let full = parse(&json!({})).unwrap();
        let neutral = parse(&json!({"amount":0})).unwrap();
        for r in [0u32, 1, 63, 127, 128, 192, 254, 255] {
            for g in [0u32, 1, 63, 127, 128, 192, 254, 255] {
                for b in [0u32, 1, 63, 127, 128, 192, 254, 255] {
                    let weighted = r * 54 + g * 183 + b * 19;
                    let gray = weighted / 256 + u32::from(weighted % 256 >= 128);
                    for alpha in [0, 37, 128, 255] {
                        let pixel = alpha << 24 | r << 16 | g << 8 | b;
                        let expected = if alpha == 0 {
                            pixel
                        } else {
                            alpha << 24 | gray * 0x010101
                        };
                        assert_eq!(full.apply(pixel), expected);
                        assert_eq!(full.apply(pixel), crate::filter_math::filter(pixel, 1, 1.));
                        assert_eq!(neutral.apply(pixel), pixel);
                    }
                }
            }
        }
        assert_eq!(full.apply(0x80ff0000), 0x80363636);
        assert_eq!(full.apply(0xff00ff00), 0xffb6b6b6);
        assert_eq!(
            parse(&json!({"amount":50})).unwrap().apply(0x80ff0000),
            0x809a1b1b
        );
    }
    #[test]
    fn invalid_parameters_reject_before_pixel_access() {
        for value in [
            json!(null),
            json!([]),
            json!({"amount":-1}),
            json!({"amount":101}),
            json!({"amount":null}),
            json!({"amount":"20"}),
            json!({"brightness":10}),
        ] {
            assert!(parse(&value).is_err());
        }
    }
}
