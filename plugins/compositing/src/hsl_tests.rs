use super::*;
fn node(kind: u32, a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
    abi::CompositeNode {
        kind,
        a,
        b,
        mode,
        amount,
    }
}
fn layer(data: &[u32]) -> abi::CompositeLayer {
    abi::CompositeLayer {
        data: data.as_ptr(),
        len: data.len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    }
}
fn parameters() -> abi::HslParameters {
    abi::HslParameters {
        hue: 120.,
        saturation: 0.,
        lightness: 0.,
    }
}
fn nodes() -> [abi::CompositeNode; 3] {
    [
        node(0, 0, 0, 0, 1.),
        node(11, 0, 0, 0, 1.),
        node(3, 1, 0, 0, 1.),
    ]
}
unsafe fn draw(
    nodes: &[abi::CompositeNode],
    layers: &[abi::CompositeLayer],
    masks: &[abi::CompositeMask],
    luts: &[abi::CompositeLut],
    parameters: &[abi::HslParameters],
    out: &mut [u32],
) -> i32 {
    let mut target = abi::Pixels {
        data: out.as_mut_ptr(),
        len: out.len(),
        width: 4,
        height: 1,
        stride: 4,
        origin_x: 0,
        origin_y: 0,
    };
    render::<false>(
        nodes.as_ptr(),
        nodes.len(),
        layers.as_ptr(),
        layers.len(),
        masks.as_ptr(),
        masks.len(),
        luts.as_ptr(),
        luts.len(),
        parameters.as_ptr(),
        parameters.len(),
        4,
        1,
        &mut target,
    )
}
unsafe fn sample(
    nodes: &[abi::CompositeNode],
    layers: &[abi::CompositeLayer],
    masks: &[abi::CompositeMask],
    luts: &[abi::CompositeLut],
    parameters: &[abi::HslParameters],
    out: &mut abi::HistogramResult,
) -> i32 {
    let request = abi::HistogramRequest {
        abi_version: 1,
        struct_size: 24,
        width: 4,
        height: 1,
        stride: 4,
        max_samples: 4,
    };
    histogram::<false>(
        &request,
        nodes.as_ptr(),
        nodes.len(),
        layers.as_ptr(),
        layers.len(),
        masks.as_ptr(),
        masks.len(),
        luts.as_ptr(),
        luts.len(),
        parameters.as_ptr(),
        parameters.len(),
        out,
    )
}
#[test]
fn hsl_export_rotates_primary_images_and_histogram_without_source_writes() {
    let api = unsafe { &*micopi_hsl_v1() };
    assert_eq!((api.abi_version, api.struct_size), (1, 24));
    assert!(api.render.is_some() && api.histogram.is_some());
    let data = [0xffff0000, 0x8000ff00, 0xff0000ff, 0x00123456];
    let original = data;
    let layers = [layer(&data)];
    let mut output = [77; 4];
    assert_eq!(
        unsafe { draw(&nodes(), &layers, &[], &[], &[parameters()], &mut output) },
        abi::OK
    );
    assert_eq!(output, [0xff00ff00, 0x800000ff, 0xffff0000, 0]);
    let mut bins = abi::HistogramResult::default();
    assert_eq!(
        unsafe { sample(&nodes(), &layers, &[], &[], &[parameters()], &mut bins) },
        abi::OK
    );
    assert_eq!((bins.sampled_pixels, bins.nontransparent_pixels), (4, 3));
    for channel in 0..3 {
        for byte in 0..256 {
            assert_eq!(
                bins.bins[channel][byte],
                match byte {
                    0 => 2,
                    255 => 1,
                    _ => 0,
                }
            );
        }
    }
    assert_eq!(data, original);
}
#[test]
fn mixed_hsl_masks_master_and_channel_tables_share_render_and_sampling_arithmetic() {
    let data = [0x80ff0000; 4];
    let masks = [abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 128,
    }];
    let luts = [
        abi::CompositeLut {
            values: std::array::from_fn(|i| 255 - i as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| (i / 2) as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        },
    ];
    let nodes = [
        node(0, 0, 0, 0, 1.),
        node(6, 0, 0, 0, 0.),
        node(7, 0, 1, 0, 1.),
        node(9, 2, 0, 0, 1.),
        node(11, 3, 0, 0, 1.),
        node(10, 4, 0, 1, 1.),
        node(3, 5, 0, 0, 1.),
    ];
    let layers = [layer(&data)];
    let mut output = [77; 4];
    assert_eq!(
        unsafe { draw(&nodes, &layers, &masks, &luts, &[parameters()], &mut output) },
        abi::OK
    );
    assert_eq!(output, [0x407f00ff; 4]);
    let mut bins = abi::HistogramResult::default();
    assert_eq!(
        unsafe { sample(&nodes, &layers, &masks, &luts, &[parameters()], &mut bins) },
        abi::OK
    );
    assert_eq!(bins.nontransparent_pixels, 4);
    for (channel, bin) in [(0, 127), (1, 0), (2, 255)] {
        for byte in 0..256 {
            assert_eq!(bins.bins[channel][byte], if byte == bin { 4 } else { 0 });
        }
    }
}
#[test]
fn all_parameter_and_instruction_errors_reject_before_any_output_publication() {
    let data = [0xffff0000; 4];
    let layers = [layer(&data)];
    let mut declared = [parameters(); 64];
    for invalid in [f64::NAN, f64::INFINITY, 181., -181.] {
        declared[63].hue = invalid;
        let mut output = [77; 4];
        assert_eq!(
            unsafe { draw(&nodes(), &layers, &[], &[], &declared, &mut output) },
            abi::ERROR
        );
        assert_eq!(output, [77; 4]);
        let mut bins = abi::HistogramResult::default();
        bins.bins[0][7] = 91;
        assert_eq!(
            unsafe { sample(&nodes(), &layers, &[], &[], &declared, &mut bins) },
            abi::ERROR
        );
        assert_eq!(bins.bins[0][7], 91);
        assert_eq!(bins.sampled_pixels, 0);
    }
    for (saturation, lightness) in [(101., 0.), (-101., 0.), (0., 101.), (0., f64::NEG_INFINITY)] {
        let parameters = [abi::HslParameters {
            hue: 0.,
            saturation,
            lightness,
        }];
        let mut output = [77; 4];
        assert_eq!(
            unsafe { draw(&nodes(), &layers, &[], &[], &parameters, &mut output) },
            abi::ERROR
        );
        assert_eq!(output, [77; 4]);
    }
    for case in 0..4 {
        let mut nodes = nodes();
        match case {
            0 => nodes[1].mode = 1,
            1 => nodes[1].b = 1,
            2 => nodes[1].amount = 0.5,
            _ => nodes[1].a = 2,
        }
        let mut output = [77; 4];
        assert_eq!(
            unsafe { draw(&nodes, &layers, &[], &[], &[parameters()], &mut output) },
            abi::ERROR
        );
        assert_eq!(output, [77; 4]);
    }
}
#[test]
fn parameter_output_alias_and_legacy_renderers_reject_hsl_without_writes() {
    let data = [0xffff0000; 4];
    let layers = [layer(&data)];
    let nodes = nodes();
    let mut parameter = parameters();
    let before = [parameter.hue, parameter.saturation, parameter.lightness];
    let mut target = abi::Pixels {
        data: (&mut parameter as *mut abi::HslParameters).cast(),
        len: 6,
        width: 4,
        height: 1,
        stride: 4,
        origin_x: 0,
        origin_y: 0,
    };
    assert_eq!(
        unsafe {
            render::<false>(
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &parameter,
                1,
                4,
                1,
                &mut target,
            )
        },
        abi::ERROR
    );
    assert_eq!(
        [parameter.hue, parameter.saturation, parameter.lightness],
        before
    );
    let mut output = [77; 4];
    target.data = output.as_mut_ptr();
    target.len = 4;
    assert_eq!(
        unsafe {
            crate::masks::render_plan(
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                4,
                1,
                &mut target,
                &[],
                true,
            )
        },
        abi::ERROR
    );
    assert_eq!(output, [77; 4]);
    let mut result = abi::HistogramResult::default();
    let request = abi::HistogramRequest {
        abi_version: 1,
        struct_size: 24,
        width: 4,
        height: 1,
        stride: 4,
        max_samples: 4,
    };
    let histogram = unsafe { &*crate::histogram::micopi_histogram_v1() };
    assert_eq!(histogram.abi_version, 1);
    assert!(histogram.struct_size >= std::mem::size_of::<abi::HistogramApiPlan>() as u32);
    let histogram =
        unsafe { &*(histogram as *const abi::HistogramApi).cast::<abi::HistogramApiPlan>() };
    assert_eq!(
        unsafe {
            histogram.sample_plan.unwrap()(
                &request,
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut result,
            )
        },
        abi::ERROR
    );
    assert_eq!(result.sampled_pixels, 0);
}

#[test]
fn null_excess_parameters_and_cropped_padded_output_keep_loan_boundaries() {
    let data = [0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff];
    let layers = [layer(&data)];
    let nodes = nodes();
    let parameter = parameters();
    let mut pixels = [77; 4];
    let mut target = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: 4,
        width: 2,
        height: 1,
        stride: 4,
        origin_x: 1,
        origin_y: 0,
    };
    for (pointer, count) in [(std::ptr::null(), 1), (&parameter as *const _, 65)] {
        assert_eq!(
            unsafe {
                render::<false>(
                    nodes.as_ptr(),
                    3,
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    pointer,
                    count,
                    4,
                    1,
                    &mut target,
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, [77; 4]);
    }
    assert_eq!(
        unsafe {
            render::<false>(
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &parameter,
                1,
                4,
                1,
                &mut target,
            )
        },
        abi::OK
    );
    assert_eq!(pixels, [0xff0000ff, 0xffff0000, 77, 77]);
}
