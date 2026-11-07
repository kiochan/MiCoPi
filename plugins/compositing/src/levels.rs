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
unsafe extern "C" fn render(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
) -> i32 {
    if output.is_null() || lut_count > 64 || (lut_count != 0 && luts.is_null()) {
        return abi::ERROR;
    }
    let out = std::ptr::read_unaligned(output);
    if lut_count != 0 {
        let Some(source) = super::span(luts, lut_count) else {
            return abi::ERROR;
        };
        let Some(destination) = super::span(out.data, out.len) else {
            return abi::ERROR;
        };
        if super::overlaps(source, destination) {
            return abi::ERROR;
        }
    }
    let luts = if lut_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(luts, lut_count)
    };
    super::masks::render_plan(
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        width,
        height,
        output,
        luts,
        false,
    )
}
static API: abi::LevelsApiRender = abi::LevelsApiRender {
    base: abi::LevelsApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::LevelsApiRender>() as u32,
        prepare: Some(prepare),
    },
    render: Some(render),
};
#[no_mangle]
pub extern "C" fn micopi_levels_v1() -> *const abi::LevelsApi {
    &API.base
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
        apply_lut(&self.0, pixel)
    }
}
pub(super) fn apply_lut(table: &[u8; 256], pixel: u32) -> u32 {
    if pixel >> 24 == 0 {
        return pixel;
    }
    (pixel & 0xff00_0000)
        | (u32::from(table[((pixel >> 16) & 255) as usize]) << 16)
        | (u32::from(table[((pixel >> 8) & 255) as usize]) << 8)
        | u32::from(table[(pixel & 255) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn instruction(kind: u32, a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
        abi::CompositeNode {
            kind,
            a,
            b,
            mode,
            amount,
        }
    }
    #[test]
    fn levels_render_preserves_mask_alpha_and_padding_and_rejects_legacy_entry() {
        let source = [0x804080ff];
        let layers = [abi::CompositeLayer {
            data: source.as_ptr(),
            len: 1,
            opacity: 255,
            visible: 1,
            mode: 0,
        }];
        let masks = [abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: 128,
        }];
        let luts = [abi::CompositeLut {
            values: std::array::from_fn(|i| 255 - i as u8),
        }];
        let nodes = [
            instruction(0, 0, 0, 0, 1.),
            instruction(6, 0, 0, 0, 0.),
            instruction(7, 0, 1, 0, 1.),
            instruction(9, 2, 0, 0, 1.),
            instruction(3, 3, 0, 0, 1.),
        ];
        let mut result = [0x12345678; 2];
        let mut out = abi::Pixels {
            data: result.as_mut_ptr(),
            len: 2,
            width: 1,
            height: 1,
            stride: 2,
            origin_x: 0,
            origin_y: 0,
        };
        unsafe {
            assert_eq!(
                super::super::masks::render_masked(
                    nodes.as_ptr(),
                    5,
                    layers.as_ptr(),
                    1,
                    masks.as_ptr(),
                    1,
                    1,
                    1,
                    &mut out
                ),
                abi::ERROR
            );
            assert_eq!(result, [0x12345678; 2]);
            assert_eq!(
                render(
                    nodes.as_ptr(),
                    5,
                    layers.as_ptr(),
                    1,
                    masks.as_ptr(),
                    1,
                    luts.as_ptr(),
                    1,
                    1,
                    1,
                    &mut out
                ),
                abi::OK
            );
        }
        assert_eq!(result, [0x40bf7f00, 0x12345678]);
        assert_eq!(source, [0x804080ff]);
    }
    #[test]
    fn levels_render_preflights_unused_invalid_nodes_and_aliases_before_writes() {
        let source = [0xff123456];
        let layers = [abi::CompositeLayer {
            data: source.as_ptr(),
            len: 1,
            opacity: 255,
            visible: 1,
            mode: 0,
        }];
        let mut luts = [abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        }];
        let mut nodes = [
            instruction(0, 0, 0, 0, 1.),
            instruction(9, 0, 0, 1, 1.),
            instruction(3, 0, 0, 0, 1.),
        ];
        let mut result = [0x12345678];
        let mut out = abi::Pixels {
            data: result.as_mut_ptr(),
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        unsafe {
            assert_eq!(
                render(
                    nodes.as_ptr(),
                    3,
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    luts.as_ptr(),
                    1,
                    1,
                    1,
                    &mut out
                ),
                abi::ERROR
            );
            assert_eq!(result, [0x12345678]);
            nodes[1].mode = 0;
            assert_eq!(
                render(
                    nodes.as_ptr(),
                    3,
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    1,
                    1,
                    &mut out
                ),
                abi::ERROR
            );
            assert_eq!(result, [0x12345678]);
            out.data = luts.as_mut_ptr().cast();
            assert_eq!(
                render(
                    nodes.as_ptr(),
                    3,
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    luts.as_ptr(),
                    1,
                    1,
                    1,
                    &mut out
                ),
                abi::ERROR
            );
        }
        assert_eq!(luts[0].values, std::array::from_fn(|i| i as u8));
    }
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
