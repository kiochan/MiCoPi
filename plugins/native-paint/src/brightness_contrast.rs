//! Cold 256-byte lookup preparation; each edited pixel needs only byte loads.
use serde_json::Value;
#[derive(Clone, Copy)]
pub(super) struct Table([u8; 256]);
pub(super) fn parse(value: &Value) -> Result<Table, String> {
    let object = value
        .as_object()
        .ok_or("Brightness/Contrast requires an object")?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "brightness" | "contrast"))
    {
        return Err("Unknown Brightness/Contrast parameter".into());
    }
    let number = |key: &str| -> Result<f32, String> {
        match value.get(key) {
            None => Ok(0.),
            Some(value) => value
                .as_f64()
                .filter(|value| value.is_finite() && (-100. ..=100.).contains(value))
                .map(|value| (value / 100.) as f32)
                .ok_or_else(|| format!("Invalid {key}")),
        }
    };
    let brightness = number("brightness")?;
    let contrast = number("contrast")?;
    Ok(Table(std::array::from_fn(|byte| {
        let gray = 0xff000000 | byte as u32 * 0x010101;
        let bright = crate::filter_math::filter(gray, 2, brightness);
        crate::filter_math::filter(bright, 3, contrast) as u8
    })))
}
impl Table {
    #[inline]
    pub(super) fn apply(&self, pixel: u32) -> u32 {
        if pixel >> 24 == 0 {
            return pixel;
        }
        (pixel & 0xff000000)
            | ((self.0[((pixel >> 16) & 255) as usize] as u32) << 16)
            | ((self.0[((pixel >> 8) & 255) as usize] as u32) << 8)
            | self.0[(pixel & 255) as usize] as u32
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn all_bytes_match_independent_integer_endpoints_and_combined_order() {
        for (brightness, contrast, reference) in [
            (0, 0, 0),
            (50, 0, 1),
            (-50, 0, 2),
            (0, -100, 3),
            (0, 100, 4),
            (25, 100, 5),
        ] {
            let table = parse(&json!({"brightness":brightness,"contrast":contrast})).unwrap();
            for byte in 0..256i32 {
                let expected = match reference {
                    0 => byte,
                    1 => byte + 128,
                    2 => byte - 127,
                    3 => 128,
                    4 => 4 * byte - 382,
                    _ => 4 * (byte + 64).min(255) - 382,
                }
                .clamp(0, 255) as u32;
                for alpha in [0, 37, 128, 255] {
                    let pixel = (alpha << 24) | byte as u32 * 0x010101;
                    assert_eq!(
                        table.apply(pixel),
                        if alpha == 0 {
                            pixel
                        } else {
                            (alpha << 24) | expected * 0x010101
                        },
                        "{brightness}/{contrast}/{byte}/{alpha}"
                    );
                }
            }
        }
        let table = parse(&json!({"brightness":25,"contrast":100})).unwrap();
        assert_eq!(table.apply(0x80204060), 0x800282ff);
        assert_eq!(parse(&json!({})).unwrap().apply(0x7f123456), 0x7f123456);
    }
    #[test]
    fn invalid_inputs_reject_before_table_or_pixel_processing() {
        for value in [
            json!(null),
            json!([]),
            json!({"brightness":101}),
            json!({"contrast":-101}),
            json!({"brightness":null}),
            json!({"contrast":"30"}),
            json!({"amount":10}),
        ] {
            assert!(parse(&value).is_err());
        }
    }
}
