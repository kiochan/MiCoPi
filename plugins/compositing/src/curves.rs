//! Cold bounded channel-curve compilation. Document integration is separate.
use micopi_plugin_api as abi;

unsafe extern "C" fn prepare(
    parameters: *const abi::CurvesParameters,
    output: *mut u8,
    len: usize,
) -> i32 {
    if parameters.is_null() || output.is_null() || len < 256 {
        return abi::ERROR;
    }
    // Read the common header before accessing the complete fixed-size record.
    let header = std::ptr::read_unaligned(parameters.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::CurvesParameters>() as u32
    {
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
    let parameters = std::ptr::read_unaligned(parameters);
    let Some(table) = compile(&parameters) else {
        return abi::ERROR;
    };
    std::ptr::copy_nonoverlapping(table.as_ptr(), output, table.len());
    abi::OK
}

pub(super) unsafe extern "C" fn render(
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
        true,
    )
}
pub(super) fn apply_channels(tables: &[abi::CompositeLut], pixel: u32) -> u32 {
    if pixel >> 24 == 0 {
        return pixel;
    }
    (pixel & 0xff00_0000)
        | (u32::from(tables[0].values[((pixel >> 16) & 255) as usize]) << 16)
        | (u32::from(tables[1].values[((pixel >> 8) & 255) as usize]) << 8)
        | u32::from(tables[2].values[(pixel & 255) as usize])
}
static API: abi::CurvesApiRender = abi::CurvesApiRender {
    base: abi::CurvesApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::CurvesApiRender>() as u32,
        prepare: Some(prepare),
    },
    render: Some(render),
};
#[no_mangle]
pub extern "C" fn micopi_curves_v1() -> *const abi::CurvesApi {
    &API.base
}

fn compile(parameters: &abi::CurvesParameters) -> Option<[u8; 256]> {
    let count = parameters.count as usize;
    if !(2..=abi::CURVES_MAX_POINTS).contains(&count)
        || !matches!(
            parameters.interpolation,
            abi::CURVES_LINEAR | abi::CURVES_SMOOTH
        )
    {
        return None;
    }
    let points = &parameters.points[..count];
    if points.iter().any(|p| p.input > 255 || p.output > 255)
        || points.windows(2).any(|p| p[0].input >= p[1].input)
    {
        return None;
    }
    let mut widths = [0.; abi::CURVES_MAX_POINTS - 1];
    let mut slopes = [0.; abi::CURVES_MAX_POINTS - 1];
    let mut tangents = [0.; abi::CURVES_MAX_POINTS];
    for (index, pair) in points.windows(2).enumerate() {
        widths[index] = f64::from(pair[1].input - pair[0].input);
        slopes[index] = (f64::from(pair[1].output) - f64::from(pair[0].output)) / widths[index];
    }
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        let left = slopes[index - 1];
        let right = slopes[index];
        // Zero slopes at plateaus and turning points; otherwise the weighted
        // harmonic mean preserves the shape of unevenly spaced intervals.
        if left * right > 0. {
            let w1 = 2. * widths[index] + widths[index - 1];
            let w2 = widths[index] + 2. * widths[index - 1];
            tangents[index] = (w1 + w2) / (w1 / left + w2 / right);
        }
    }
    let mut result = [0; 256];
    let mut segment = 0;
    for (input, output) in result.iter_mut().enumerate() {
        let input = input as u32;
        let value = if input <= points[0].input {
            f64::from(points[0].output)
        } else if input >= points[count - 1].input {
            f64::from(points[count - 1].output)
        } else {
            while input > points[segment + 1].input {
                segment += 1;
            }
            let a = f64::from(points[segment].output);
            let b = f64::from(points[segment + 1].output);
            let width = widths[segment];
            let t = f64::from(input - points[segment].input) / width;
            if parameters.interpolation == abi::CURVES_LINEAR {
                a + (b - a) * t
            } else {
                // Cubic Hermite basis, evaluated only during cold preparation.
                let t2 = t * t;
                let t3 = t2 * t;
                ((2. * t3 - 3. * t2 + 1.) * a
                    + (t3 - 2. * t2 + t) * width * tangents[segment]
                    + (-2. * t3 + 3. * t2) * b
                    + (t3 - t2) * width * tangents[segment + 1])
                    .clamp(a.min(b), a.max(b))
            }
        };
        *output = value.round().clamp(0., 255.) as u8;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters(points: &[(u32, u32)], interpolation: u32) -> abi::CurvesParameters {
        let mut value = abi::CurvesParameters {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::CurvesParameters>() as u32,
            count: points.len() as u32,
            interpolation,
            points: [abi::CurvePoint::default(); abi::CURVES_MAX_POINTS],
        };
        for (slot, (input, output)) in value.points.iter_mut().zip(points) {
            *slot = abi::CurvePoint {
                input: *input,
                output: *output,
            };
        }
        value
    }
    fn invoke(p: &abi::CurvesParameters) -> [u8; 256] {
        let mut output = [42; 264];
        let api = unsafe { &*micopi_curves_v1() };
        assert_eq!(api.abi_version, 1);
        assert_eq!(
            api.struct_size as usize,
            std::mem::size_of::<abi::CurvesApiRender>()
        );
        assert_eq!(
            unsafe { api.prepare.unwrap()(p, output.as_mut_ptr(), output.len()) },
            abi::OK
        );
        assert_eq!(&output[256..], &[42; 8]);
        output[..256].try_into().unwrap()
    }
    #[test]
    fn channel_tables_preserve_alpha_and_transparent_words() {
        let identity = abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        };
        let inverse = abi::CompositeLut {
            values: std::array::from_fn(|i| 255 - i as u8),
        };
        let constant = abi::CompositeLut { values: [42; 256] };
        assert_eq!(
            apply_channels(&[inverse, identity, constant], 0x80406080),
            0x80bf602a
        );
        assert_eq!(
            apply_channels(&[inverse, identity, constant], 0x00406080),
            0x00406080
        );
    }
    #[test]
    fn channel_renderer_admits_mixed_luts_masks_and_rejects_invalid_unused_nodes() {
        let luts = [
            abi::CompositeLut {
                values: std::array::from_fn(|i| i as u8),
            },
            abi::CompositeLut {
                values: std::array::from_fn(|i| 255 - i as u8),
            },
            abi::CompositeLut {
                values: std::array::from_fn(|i| i as u8),
            },
            abi::CompositeLut { values: [42; 256] },
        ];
        let n = |kind, a, b, mode, amount| abi::CompositeNode {
            kind,
            a,
            b,
            mode,
            amount,
        };
        let mut nodes = vec![
            n(0, 0, 0, 0, 1.),
            n(9, 0, 0, 0, 1.),
            n(10, 1, 0, 1, 1.),
            n(6, 0, 0, 0, 0.),
            n(7, 2, 3, 0, 1.),
        ];
        let data = [0x80406080];
        let layers = [abi::CompositeLayer {
            data: data.as_ptr(),
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
        let mut output = [17, 19];
        let mut target = abi::Pixels {
            data: output.as_mut_ptr(),
            len: 2,
            width: 1,
            height: 1,
            stride: 2,
            origin_x: 0,
            origin_y: 0,
        };
        let call = |nodes: &[abi::CompositeNode], target: &mut abi::Pixels| unsafe {
            render(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                1,
                masks.as_ptr(),
                1,
                luts.as_ptr(),
                luts.len(),
                1,
                1,
                target,
            )
        };
        assert_eq!(call(&nodes, &mut target), abi::OK);
        assert_eq!(output, [0x40bf602a, 19]);
        for bad in [
            n(10, 0, 0, 2, 1.),
            n(10, 0, 0, u32::MAX, 1.),
            n(10, 3, 0, 1, 1.),
            n(10, 0, 1, 1, 1.),
            n(10, 0, 0, 1, 0.5),
        ] {
            output = [17, 19];
            nodes.push(bad); // validate even if a later output ignores it
            nodes.push(n(3, 4, 0, 0, 1.));
            assert_eq!(call(&nodes, &mut target), abi::ERROR);
            assert_eq!(output, [17, 19]);
            nodes.truncate(5);
        }
        output = [17, 19];
        unsafe {
            let legacy = &*super::super::levels::micopi_levels_v1().cast::<abi::LevelsApiRender>();
            assert_eq!(
                legacy.render.unwrap()(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    masks.as_ptr(),
                    1,
                    luts.as_ptr(),
                    luts.len(),
                    1,
                    1,
                    &mut target
                ),
                abi::ERROR
            );
        }
        assert_eq!(output, [17, 19]);
        // A LUT borrowed as output is rejected before writing its first word.
        let mut aliased = luts;
        target.data = aliased.as_mut_ptr().cast();
        target.len = 1;
        unsafe {
            assert_eq!(
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    masks.as_ptr(),
                    1,
                    aliased.as_ptr(),
                    4,
                    1,
                    1,
                    &mut target
                ),
                abi::ERROR
            );
        }
        assert_eq!(aliased[1].values[0], 255);
    }
    #[test]
    fn identity_inverse_and_constant_are_exact_for_both_modes() {
        for mode in [abi::CURVES_LINEAR, abi::CURVES_SMOOTH] {
            for (points, inverse, constant) in [
                (vec![(0, 0), (255, 255)], false, None),
                (vec![(0, 255), (255, 0)], true, None),
                (vec![(0, 73), (127, 73), (255, 73)], false, Some(73)),
            ] {
                let actual = invoke(&parameters(&points, mode));
                for (x, y) in actual.iter().enumerate() {
                    assert_eq!(
                        *y,
                        constant.unwrap_or(if inverse { 255 - x as u8 } else { x as u8 })
                    );
                }
            }
        }
    }
    #[test]
    fn linear_rounding_clamped_endpoints_and_unused_slots() {
        let mut p = parameters(&[(32, 20), (96, 85), (224, 213)], abi::CURVES_LINEAR);
        p.points[31] = abi::CurvePoint {
            input: u32::MAX,
            output: u32::MAX,
        };
        let table = invoke(&p);
        for x in 0..256 {
            let expected = if x <= 32 {
                20.
            } else if x < 96 {
                20. + f64::from(x - 32) * 65. / 64.
            } else if x < 224 {
                f64::from(x - 11)
            } else {
                213.
            };
            assert_eq!(table[x as usize], expected.round() as u8, "input {x}");
        }
        assert_eq!(table[64], 53); // positive half-up 52.5
    }
    #[test]
    fn smooth_turning_point_has_independent_hermite_witnesses() {
        let table = invoke(&parameters(
            &[(0, 0), (128, 128), (255, 1)],
            abi::CURVES_SMOOTH,
        ));
        // Endpoint secant=1, turning tangent=0; at t=1/2: 64+16.
        assert_eq!(table[64], 80);
        assert_eq!(table[128], 128);
        assert_eq!(table[255], 1);
        assert!(table[..129].windows(2).all(|v| v[0] <= v[1]));
        assert!(table[128..].windows(2).all(|v| v[0] >= v[1]));
        let linear = invoke(&parameters(
            &[(0, 0), (128, 128), (255, 1)],
            abi::CURVES_LINEAR,
        ));
        assert_eq!(linear[64], 64);
    }
    #[test]
    fn smooth_unequal_spacing_uses_weighted_tangents() {
        // h=(64,128), slopes=(1,1/4), weights=(320,256), tangent=3/7.
        // t=1/2 in the first interval: 32+8-24/7=36+4/7 -> 37.
        let table = invoke(&parameters(
            &[(0, 0), (64, 64), (192, 96)],
            abi::CURVES_SMOOTH,
        ));
        assert_eq!(table[32], 37);
        assert_eq!(table[64], 64);
        assert_eq!(table[192], 96);
    }
    #[test]
    fn dense_curves_preserve_knots_plateaus_and_each_segment_range() {
        for mode in [abi::CURVES_LINEAR, abi::CURVES_SMOOTH] {
            for seed in 0..48u32 {
                let points = (0..32)
                    .map(|i| (i * 8, ((i / 2) * 37 + seed * 11) % 256))
                    .collect::<Vec<_>>();
                let table = invoke(&parameters(&points, mode));
                for pair in points.windows(2) {
                    assert_eq!(table[pair[0].0 as usize], pair[0].1 as u8);
                    for x in pair[0].0..=pair[1].0 {
                        assert!((pair[0].1.min(pair[1].1)..=pair[0].1.max(pair[1].1))
                            .contains(&u32::from(table[x as usize])));
                    }
                }
                assert_eq!(table[255], points[31].1 as u8);
            }
        }
    }
    #[test]
    fn invalid_parameters_reject_before_any_output_write() {
        let good = parameters(&[(0, 0), (255, 255)], abi::CURVES_SMOOTH);
        for case in 0..10 {
            let mut p = good;
            match case {
                0 => p.abi_version = 2,
                1 => p.struct_size -= 1,
                2 => p.count = 0,
                3 => p.count = 1,
                4 => p.count = 33,
                5 => p.count = u32::MAX,
                6 => p.interpolation = 2,
                7 => p.points[0].input = 255,
                8 => p.points[1].input = 256,
                _ => p.points[1].output = 256,
            }
            let mut out = [91; 264];
            assert_eq!(
                unsafe { prepare(&p, out.as_mut_ptr(), out.len()) },
                abi::ERROR,
                "case {case}"
            );
            assert_eq!(out, [91; 264]);
        }
        let reversed = parameters(&[(90, 10), (80, 20)], abi::CURVES_LINEAR);
        assert!(compile(&reversed).is_none());
    }
    #[test]
    fn null_short_header_alias_and_capacity_reject_without_writes() {
        let mut p = parameters(&[(0, 0), (255, 255)], abi::CURVES_LINEAR);
        let mut out = [51; 256];
        unsafe {
            assert_eq!(prepare(std::ptr::null(), out.as_mut_ptr(), 256), abi::ERROR);
            assert_eq!(prepare(&p, std::ptr::null_mut(), 256), abi::ERROR);
            assert_eq!(prepare(&p, out.as_mut_ptr(), 255), abi::ERROR);
            let short = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert_eq!(
                prepare((&short as *const abi::Header).cast(), out.as_mut_ptr(), 256),
                abi::ERROR
            );
            let pointer = &mut p as *mut abi::CurvesParameters;
            assert_eq!(prepare(pointer, pointer.cast(), 256), abi::ERROR);
            assert_eq!(p.points[1].input, 255);
            assert_eq!(p.count, 2);
        }
        assert_eq!(out, [51; 256]);
    }
    #[test]
    fn compatible_future_records_and_unaligned_parameters_keep_prefix_behavior() {
        #[repr(C)]
        struct Future {
            base: abi::CurvesParameters,
            tail: [u8; 32],
        }
        let mut p = parameters(&[(0, 0), (255, 255)], abi::CURVES_SMOOTH);
        p.struct_size = std::mem::size_of::<Future>() as u32;
        let extended = Future {
            base: p,
            tail: [97; 32],
        };
        assert_eq!(invoke(&extended.base), std::array::from_fn(|i| i as u8));
        assert_eq!(extended.tail, [97; 32]);
        let p = parameters(&[(0, 255), (255, 0)], abi::CURVES_LINEAR);
        let mut storage = [0u8; std::mem::size_of::<abi::CurvesParameters>() + 1];
        let mut output = [0; 256];
        unsafe {
            let pointer = storage.as_mut_ptr().add(1).cast::<abi::CurvesParameters>();
            std::ptr::write_unaligned(pointer, p);
            assert_eq!(prepare(pointer, output.as_mut_ptr(), output.len()), abi::OK);
        }
        assert_eq!(output, std::array::from_fn(|i| 255 - i as u8));
    }
}
