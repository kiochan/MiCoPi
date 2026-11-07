//! Cold Levels preparation; the renderer will consume the prepared byte table.
//! This module is a prerequisite, not yet a registered document effect.

#[derive(Clone, Copy, Debug)]
pub(crate) struct Levels {
    pub input_black: u8,
    pub input_white: u8,
    pub gamma: f64,
    pub output_black: u8,
    pub output_white: u8,
}
impl Default for Levels {
    fn default() -> Self {
        Self {
            input_black: 0,
            input_white: 255,
            gamma: 1.,
            output_black: 0,
            output_white: 255,
        }
    }
}
pub(crate) struct Prepared(pub [u8; 256]);
impl Levels {
    /// Validate before constructing a table. No images or retained pointers are
    /// involved, and every input byte has a defined output, including clipping.
    pub fn prepare(self) -> Option<Prepared> {
        if self.input_black >= self.input_white
            || !self.gamma.is_finite()
            || !(0.1..=10.).contains(&self.gamma)
        {
            return None;
        }
        let mut table = [0; 256];
        let low = f64::from(self.input_black);
        let width = f64::from(self.input_white) - low;
        let start = f64::from(self.output_black);
        let range = f64::from(self.output_white) - start;
        for (input, output) in table.iter_mut().enumerate() {
            let normalized = ((input as f64 - low) / width).clamp(0., 1.);
            let curved = if self.gamma == 1. {
                normalized
            } else {
                normalized.powf(1. / self.gamma)
            };
            *output = (start + range * curved).round().clamp(0., 255.) as u8;
        }
        Some(Prepared(table))
    }
}
impl Prepared {
    /// Apply independently to encoded RGB; alpha and transparent words survive.
    pub fn pixel(&self, pixel: u32) -> u32 {
        if pixel >> 24 == 0 {
            return pixel;
        }
        (pixel & 0xff00_0000)
            | (u32::from(self.0[((pixel >> 16) & 255) as usize]) << 16)
            | (u32::from(self.0[((pixel >> 8) & 255) as usize]) << 8)
            | u32::from(self.0[(pixel & 255) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_retains_every_byte_and_alpha_word() {
        let table = Levels::default().prepare().unwrap();
        for value in 0..=255u32 {
            assert_eq!(table.0[value as usize], value as u8);
            for alpha in [0, 1, 127, 255] {
                let word = (alpha << 24) | (value << 16) | ((255 - value) << 8) | value;
                assert_eq!(table.pixel(word), word);
            }
        }
    }
    #[test]
    fn clipped_input_and_reversed_output_follow_independent_integer_reference() {
        for (black, white) in [(0, 255), (32, 224), (128, 129)] {
            for (start, end) in [(0, 255), (16, 240), (240, 16)] {
                let table = Levels {
                    input_black: black,
                    input_white: white,
                    output_black: start,
                    output_white: end,
                    ..Default::default()
                }
                .prepare()
                .unwrap();
                let denominator = i32::from(white - black);
                for input in 0..=255u16 {
                    let clamped = i32::from(input).clamp(i32::from(black), i32::from(white))
                        - i32::from(black);
                    let numerator = i32::from(start) * denominator
                        + (i32::from(end) - i32::from(start)) * clamped;
                    let expected = (numerator * 2 + denominator) / (denominator * 2);
                    assert_eq!(table.0[input as usize], expected as u8);
                }
            }
        }
    }
    #[test]
    fn gamma_has_expected_midtones_and_preserves_transparency() {
        let bright = Levels {
            gamma: 2.,
            ..Default::default()
        }
        .prepare()
        .unwrap();
        let dark = Levels {
            gamma: 0.5,
            ..Default::default()
        }
        .prepare()
        .unwrap();
        assert_eq!(bright.0[64], 128);
        assert_eq!(dark.0[128], 64);
        for table in [bright, dark] {
            assert_eq!(table.0[0], 0);
            assert_eq!(table.0[255], 255);
            assert!(table.0.windows(2).all(|pair| pair[0] <= pair[1]));
            assert_eq!(table.pixel(0x00123456), 0x00123456);
            assert_eq!(table.pixel(0x7f4080ff) >> 24, 127);
        }
    }
    #[test]
    fn invalid_domains_never_produce_a_prepared_table() {
        for gamma in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.,
            0.099999,
            10.00001,
        ] {
            assert!(Levels {
                gamma,
                ..Default::default()
            }
            .prepare()
            .is_none());
        }
        for (input_black, input_white) in [(0, 0), (255, 0), (255, 255)] {
            assert!(Levels {
                input_black,
                input_white,
                ..Default::default()
            }
            .prepare()
            .is_none());
        }
    }
}
