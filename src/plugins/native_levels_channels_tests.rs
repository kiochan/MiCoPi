use super::*;

fn parameters() -> abi::LevelsChannelsParameters {
    let range = abi::LevelsParameters {
        abi_version: 0,
        struct_size: 0,
        input_black: 0,
        input_white: 255,
        output_black: 0,
        output_white: 255,
        gamma: 1.,
    };
    abi::LevelsChannelsParameters {
        abi_version: 0,
        struct_size: 0,
        master: range,
        red: range,
        green: range,
        blue: range,
    }
}
unsafe extern "C" fn scalar(_: *const abi::LevelsParameters, _: *mut u8, _: usize) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn render(
    _: *const abi::CompositeNode,
    _: usize,
    _: *const abi::CompositeLayer,
    _: usize,
    _: *const abi::CompositeMask,
    _: usize,
    _: *const abi::CompositeLut,
    _: usize,
    _: u32,
    _: u32,
    _: *mut abi::Pixels,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn identity(
    p: *const abi::LevelsChannelsParameters,
    out: *mut abi::CompositeLut,
    len: usize,
) -> i32 {
    assert_eq!(((*p).abi_version, (*p).struct_size, len), (1, 136, 3));
    for range in [(*p).master, (*p).red, (*p).green, (*p).blue] {
        assert_eq!((range.abi_version, range.struct_size), (1, 32));
    }
    for channel in 0..3 {
        out.add(channel).write(abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        });
    }
    abi::OK
}
unsafe extern "C" fn failure(
    _: *const abi::LevelsChannelsParameters,
    out: *mut abi::CompositeLut,
    _: usize,
) -> i32 {
    (*out).values.fill(77);
    abi::ERROR
}
fn api() -> abi::LevelsApiChannels {
    abi::LevelsApiChannels {
        base: abi::LevelsApiRender {
            base: abi::LevelsApi {
                abi_version: 1,
                struct_size: 40,
                prepare: Some(scalar),
            },
            render: Some(render),
        },
        prepare_channels: Some(identity),
        render_channels: Some(render),
    }
}

#[test]
fn channel_decoder_keeps_old_prefixes_and_rejects_partial_or_missing_tails() {
    unsafe {
        assert!(decode_channels(std::ptr::null()).is_err());
        let short = abi::Header {
            abi_version: 1,
            struct_size: 8,
        };
        assert!(decode_channels((&short as *const abi::Header).cast()).is_err());
        for size in [16, 24, 25, 31, 32, 39, 40, 48] {
            let mut table = api();
            table.base.base.struct_size = size;
            let result = decode_channels(&table.base.base);
            match size {
                16 | 24 => assert!(result.unwrap().is_none()),
                40 | 48 => assert!(result.unwrap().is_some()),
                _ => assert!(result.is_err()),
            }
        }
        for case in 0..4 {
            let mut table = api();
            match case {
                0 => table.base.base.abi_version = 2,
                1 => table.base.render = None,
                2 => table.prepare_channels = None,
                _ => table.render_channels = None,
            }
            assert!(decode_channels(&table.base.base).is_err());
        }
    }
}

#[test]
fn channel_adapter_preflights_all_ranges_and_withholds_partial_failure() {
    assert_eq!(
        prepare_channels(None, parameters()).err().unwrap().code,
        "levels_channels_unavailable"
    );
    for channel in 0..4 {
        for case in 0..5 {
            let mut p = parameters();
            let range = match channel {
                0 => &mut p.master,
                1 => &mut p.red,
                2 => &mut p.green,
                _ => &mut p.blue,
            };
            match case {
                0 => range.input_black = range.input_white,
                1 => range.output_white = 256,
                2 => range.gamma = f64::NAN,
                3 => range.gamma = f64::INFINITY,
                _ => range.gamma = 10.001,
            }
            unsafe extern "C" fn forbidden(
                _: *const abi::LevelsChannelsParameters,
                _: *mut abi::CompositeLut,
                _: usize,
            ) -> i32 {
                panic!("Invalid channel Levels must be rejected before the callback");
            }
            assert_eq!(
                prepare_channels(Some(forbidden), p).err().unwrap().code,
                "levels_parameters"
            );
        }
    }
    assert_eq!(
        prepare_channels(Some(failure), parameters())
            .err()
            .unwrap()
            .code,
        "levels_channels_prepare"
    );
    let tables = prepare_channels(Some(identity), parameters()).unwrap();
    assert!(tables
        .iter()
        .all(|t| t.values == std::array::from_fn(|i| i as u8)));
}

#[test]
fn bundled_channel_levels_are_negotiated_once_and_discoverable() {
    let registry =
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    let compositor = &registry.native_plugins["compositing"];
    assert!(compositor.supports_levels_channels());
    assert!(!registry.native_plugins["painting"].supports_levels_channels());
    let callback = compositor.levels_channels.unwrap().0 as usize;
    let mut p = parameters();
    p.master.gamma = 2.;
    p.red.output_white = 128;
    p.blue.output_black = 255;
    p.blue.output_white = 0;
    for _ in 0..3 {
        let tables = compositor.prepare_levels_channels(p).unwrap();
        assert_eq!(tables[0].values[128], 91);
        assert_eq!(tables[1].values[128], 181);
        assert_eq!(tables[2].values[128], 74);
        assert_eq!(compositor.levels_channels.unwrap().0 as usize, callback);
    }
    let description = registry.describe();
    assert_eq!(
        description["native"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "compositing")
            .unwrap()["filters"]["levels_channels"],
        true
    );
}
