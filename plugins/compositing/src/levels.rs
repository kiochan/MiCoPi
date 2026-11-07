//! Cold Levels preparation; the renderer will consume the prepared byte table.
//! This module is a prerequisite, not yet a registered document effect.
use micopi_plugin_api as abi;

unsafe extern "C" fn prepare(
    parameters: *const abi::LevelsParameters,
    output: *mut u8,
    len: usize,
) -> i32 {
    if parameters.is_null() || output.is_null() || len < 256 {
        return abi::ERROR;
    }
    let Some(source) = super::span(parameters, 1) else {
        return abi::ERROR;
    };
    let Some(destination) = super::span(output, len) else {
        return abi::ERROR;
    };
    if super::overlaps(source, destination) {
        return abi::ERROR;
    }
    let header = std::ptr::read_unaligned(parameters.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::LevelsParameters>() as u32
    {
        return abi::ERROR;
    }
    let p = std::ptr::read_unaligned(parameters);
    if [p.input_black, p.input_white, p.output_black, p.output_white]
        .iter()
        .any(|v| *v > 255)
    {
        return abi::ERROR;
    }
    let Some(table) = (Levels {
        input_black: p.input_black as u8,
        input_white: p.input_white as u8,
        output_black: p.output_black as u8,
        output_white: p.output_white as u8,
        gamma: p.gamma,
    })
    .prepare() else {
        return abi::ERROR;
    };
    std::ptr::copy_nonoverlapping(table.0.as_ptr(), output, 256);
    abi::OK
}
static API: abi::LevelsApi = abi::LevelsApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::LevelsApi>() as u32,
    prepare: Some(prepare),
};
#[no_mangle]
pub extern "C" fn micopi_levels_v1() -> *const abi::LevelsApi {
    &API
}

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
    fn preparation_abi_rejects_invalid_spans_without_writing_and_preserves_padding() {
        let mut p = abi::LevelsParameters {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::LevelsParameters>() as u32,
            input_black: 0,
            input_white: 255,
            output_black: 0,
            output_white: 255,
            gamma: 1.,
        };
        let mut out = [91u8; 264];
        let api = unsafe { &*micopi_levels_v1() };
        assert_eq!(api.abi_version, 1);
        let callback = api.prepare.unwrap();
        unsafe {
            let short = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert_eq!(
                callback(
                    (&short as *const abi::Header).cast(),
                    out.as_mut_ptr(),
                    out.len()
                ),
                abi::ERROR
            );
            assert_eq!(callback(&p, out.as_mut_ptr(), 255), abi::ERROR);
            assert_eq!(out, [91; 264]);
            p.input_white = 256;
            assert_eq!(callback(&p, out.as_mut_ptr(), out.len()), abi::ERROR);
            assert_eq!(out, [91; 264]);
            p.input_white = 255;
            assert_eq!(
                callback(
                    &p,
                    (&p as *const abi::LevelsParameters).cast_mut().cast(),
                    256
                ),
                abi::ERROR
            );
            assert_eq!(
                callback(std::ptr::null(), out.as_mut_ptr(), out.len()),
                abi::ERROR
            );
            assert_eq!(callback(&p, std::ptr::null_mut(), 256), abi::ERROR);
            assert_eq!(callback(&p, out.as_mut_ptr(), out.len()), abi::OK);
        }
        assert_eq!(&out[..256], &(0..=255u8).collect::<Vec<_>>());
        assert_eq!(&out[256..], &[91; 8]);
    }
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
