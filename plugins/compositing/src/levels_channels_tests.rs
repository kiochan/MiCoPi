use super::*;

fn range() -> abi::LevelsParameters {
    abi::LevelsParameters {
        abi_version: 1,
        struct_size: 32,
        input_black: 0,
        input_white: 255,
        output_black: 0,
        output_white: 255,
        gamma: 1.,
    }
}
fn parameters() -> abi::LevelsChannelsParameters {
    abi::LevelsChannelsParameters {
        abi_version: 1,
        struct_size: 136,
        master: range(),
        red: range(),
        green: range(),
        blue: range(),
    }
}

#[test]
fn channels_match_independent_byte_reference_and_write_only_three_tables() {
    let mut p = parameters();
    p.master.gamma = 2.;
    p.red.output_white = 128;
    p.green.input_black = 64;
    p.green.input_white = 192;
    p.blue.output_black = 255;
    p.blue.output_white = 0;
    let mut tables = [abi::CompositeLut { values: [77; 256] }; 4];
    assert_eq!(
        unsafe { prepare_channels(&p, tables.as_mut_ptr(), 4) },
        abi::OK
    );
    for x in 0..256 {
        let master = ((x as f64 / 255.).sqrt() * 255.).round();
        assert_eq!(tables[0].values[x], (master * 128. / 255.).round() as u8);
        assert_eq!(
            tables[1].values[x],
            (((master - 64.) / 128.).clamp(0., 1.) * 255.).round() as u8
        );
        assert_eq!(tables[2].values[x], (255. - master) as u8);
    }
    assert_eq!(tables[3].values, [77; 256]);
    // Applying red before master would give 128, not 90: order is observable.
    assert_eq!(tables[0].values[64], 64);
    assert_eq!(tables[0].values[128], 90);
    let identity = parameters();
    assert_eq!(
        unsafe { prepare_channels(&identity, tables.as_mut_ptr(), 3) },
        abi::OK
    );
    for table in &tables[..3] {
        assert_eq!(table.values, std::array::from_fn(|i| i as u8));
    }
}

#[test]
fn channel_rejections_preserve_all_output_including_late_invalid_channels() {
    let original = [abi::CompositeLut { values: [91; 256] }; 4];
    for case in 0..12 {
        let mut p = parameters();
        match case {
            0 => p.abi_version = 2,
            1 => p.struct_size = 135,
            2 => p.master.abi_version = 2,
            3 => p.red.struct_size = 31,
            4 => p.green.struct_size = 40,
            5 => p.blue.input_black = 255,
            6 => p.blue.gamma = f64::NAN,
            7 => p.blue.gamma = f64::INFINITY,
            8 => p.blue.gamma = 0.099,
            9 => p.blue.gamma = 10.001,
            10 => p.blue.output_white = 256,
            _ => p.green.input_black = 256,
        }
        let mut output = original;
        assert_eq!(
            unsafe { prepare_channels(&p, output.as_mut_ptr(), 4) },
            abi::ERROR
        );
        for (a, b) in output.iter().zip(original) {
            assert_eq!(a.values, b.values);
        }
    }
    let p = parameters();
    let mut output = original;
    for length in [0, 1, 2, usize::MAX] {
        assert_eq!(
            unsafe { prepare_channels(&p, output.as_mut_ptr(), length) },
            abi::ERROR
        );
        assert!(output.iter().all(|t| t.values == [91; 256]));
    }
    assert_eq!(
        unsafe { prepare_channels(std::ptr::null(), output.as_mut_ptr(), 3) },
        abi::ERROR
    );
    assert_eq!(
        unsafe { prepare_channels(&p, std::ptr::null_mut(), 3) },
        abi::ERROR
    );
    let short = abi::Header {
        abi_version: 1,
        struct_size: 8,
    };
    assert_eq!(
        unsafe {
            prepare_channels(
                (&short as *const abi::Header).cast(),
                output.as_mut_ptr(),
                3,
            )
        },
        abi::ERROR
    );
    assert!(output.iter().all(|t| t.values == [91; 256]));
}

#[test]
fn channels_reject_aliases_and_accept_only_the_known_outer_prefix() {
    let mut storage = [0u64; 128];
    let pointer = storage.as_mut_ptr().cast::<abi::LevelsChannelsParameters>();
    unsafe {
        pointer.write(parameters());
    }
    let before = storage;
    assert_eq!(
        unsafe { prepare_channels(pointer, pointer.cast(), 3) },
        abi::ERROR
    );
    assert_eq!(storage, before);
    #[repr(C)]
    struct Future {
        base: abi::LevelsChannelsParameters,
        tail: u64,
    }
    let mut future = Future {
        base: parameters(),
        tail: 0x12345678,
    };
    future.base.struct_size = std::mem::size_of::<Future>() as u32;
    let mut out = [abi::CompositeLut { values: [0; 256] }; 3];
    assert_eq!(
        unsafe { prepare_channels(&future.base, out.as_mut_ptr(), 3) },
        abi::OK
    );
    assert_eq!(future.tail, 0x12345678);
    assert!(out
        .iter()
        .all(|t| t.values == std::array::from_fn(|i| i as u8)));
}

#[test]
fn negotiated_channel_render_preserves_alpha_and_legacy_rejection() {
    let mut p = parameters();
    p.red.output_white = 128;
    p.green.output_black = 255;
    p.green.output_white = 0;
    let api = unsafe { &*micopi_levels_v1().cast::<abi::LevelsApiChannels>() };
    assert_eq!(api.base.base.struct_size, 40);
    let mut tables = [abi::CompositeLut { values: [0; 256] }; 3];
    assert_eq!(
        unsafe { (api.prepare_channels.unwrap())(&p, tables.as_mut_ptr(), 3) },
        abi::OK
    );
    let nodes = [
        abi::CompositeNode {
            kind: 0,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 10,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 3,
            a: 1,
            b: 0,
            mode: 0,
            amount: 1.,
        },
    ];
    for alpha in [0u32, 1, 127, 255] {
        let pixels = [(alpha << 24) | 0x4080c0];
        let layers = [abi::CompositeLayer {
            data: pixels.as_ptr(),
            len: 1,
            opacity: 255,
            visible: 1,
            mode: 0,
        }];
        let mut output = [0x12345678];
        let mut target = abi::Pixels {
            data: output.as_mut_ptr(),
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        let call = |f: abi::LevelsRenderFn, target: &mut abi::Pixels| unsafe {
            f(
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                tables.as_ptr(),
                3,
                1,
                1,
                target,
            )
        };
        assert_eq!(call(api.base.render.unwrap(), &mut target), abi::ERROR);
        assert_eq!(output, [0x12345678]);
        assert_eq!(call(api.render_channels.unwrap(), &mut target), abi::OK);
        // Transparent composition publishes zero; nonzero alpha remains exact.
        assert_eq!(
            output,
            [if alpha == 0 {
                0
            } else {
                (alpha << 24) | 0x207fc0
            }]
        );
        assert_eq!(pixels, [(alpha << 24) | 0x4080c0]);
    }
}
