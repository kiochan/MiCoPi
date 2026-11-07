//! Independent sampler negotiation and borrowed host-dispatch contracts.
use super::*;

unsafe extern "C" fn reframe(
    request: *const abi::A8Reframe,
    source: *const abi::CompositeMask,
    width: u32,
    height: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    if (*request).x != -3
        || (*request).y != 2
        || width != 3
        || height != 2
        || !(*source).data.is_null()
        || (*source).constant != 73
    {
        return abi::ERROR;
    }
    (*output).data.write(37);
    abi::OK
}
unsafe extern "C" fn geometry(
    request: *const abi::A8Geometry,
    _: *const abi::CompositeMask,
    _: u32,
    _: u32,
    _: *mut abi::A8Pixels,
) -> i32 {
    if (*request).geometry.kind == 4 && (*request).fill == 91 {
        -72
    } else {
        abi::ERROR
    }
}
unsafe extern "C" fn resample(
    request: *const abi::Resample,
    _: *const abi::CompositeMask,
    _: u32,
    _: u32,
    _: *mut abi::A8Pixels,
) -> i32 {
    if (*request).method == 2 {
        -73
    } else {
        abi::ERROR
    }
}
fn table() -> abi::A8SamplerApi {
    abi::A8SamplerApi {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::A8SamplerApi>() as u32,
        reframe: Some(reframe),
        geometry: Some(geometry),
        resample: Some(resample),
    }
}
fn stub(api: Option<abi::A8SamplerApi>) -> NativePlugin {
    NativePlugin {
        path: PathBuf::new(),
        api: abi::PluginApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
            describe: None,
            create: None,
            destroy: None,
            invoke: None,
        },
        tools: None,
        tool_options: None,
        selection_mix: None,
        selection_color: None,
        selection_edges: None,
        selection_transfer: None,
        geometry: None,
        trim: None,
        movement: None,
        affine: None,
        projective: None,
        resample: None,
        resample_features: 0,
        a8_sampler: api,
        a8_affine: None,
        a8_affine_bounds: None,
        compositor: None,
        compositor_capabilities: 0,
        compositor_blend_modes: 0,
        compositor_masked: None,
        compositor_effects: None,
        levels_prepare: None,
        levels_render: None,
        curves_prepare: None,
        curves_render: None,
        mask_effect_instance: Default::default(),
        _module: Module { handle: 0 },
    }
}
fn uniform(value: u32) -> abi::CompositeMask {
    abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: value,
    }
}
fn output(bytes: &mut [u8], width: u32, height: u32, stride: u32) -> abi::A8Pixels {
    abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        width,
        height,
        stride,
        origin_x: 0,
        origin_y: 0,
    }
}
fn request() -> abi::A8Reframe {
    abi::A8Reframe {
        x: -3,
        y: 2,
        fill: 91,
        reserved: 0,
    }
}

#[test]
fn a8_sampler_header_is_checked_before_the_mandatory_callback_table() {
    assert!(unsafe { decode_a8_sampler_table(std::ptr::null()) }.is_err());
    for size in 0..std::mem::size_of::<abi::A8SamplerApi>() as u32 {
        // Only eight header bytes exist: a truncated table must not read beyond them.
        let header = abi::Header {
            abi_version: 1,
            struct_size: size,
        };
        assert!(
            unsafe { decode_a8_sampler_table((&header as *const abi::Header).cast()) }.is_err()
        );
    }
    let mut api = table();
    assert!(unsafe { decode_a8_sampler_table(&api) }.is_ok());
    api.struct_size += 8;
    assert!(
        unsafe { decode_a8_sampler_table(&api) }.is_ok(),
        "Ignore future optional tail bytes"
    );
    for version in [0, 2, u32::MAX] {
        let header = abi::Header {
            abi_version: version,
            struct_size: 32,
        };
        assert!(
            unsafe { decode_a8_sampler_table((&header as *const abi::Header).cast()) }.is_err()
        );
    }
    for missing in 0..3 {
        let mut api = table();
        match missing {
            0 => api.reframe = None,
            1 => api.geometry = None,
            _ => api.resample = None,
        }
        assert!(unsafe { decode_a8_sampler_table(&api) }
            .err()
            .unwrap()
            .contains("callbacks"));
    }
}

#[test]
fn a8_sampler_wrappers_preserve_uniform_borrows_and_propagate_native_errors() {
    let plugin = stub(Some(table()));
    assert!(plugin.supports_a8_sampler());
    let source = uniform(73);
    let mut bytes = [211; 4];
    let pointer = bytes.as_mut_ptr();
    let mut target = output(&mut bytes, 2, 1, 4);
    plugin
        .a8_reframe(&request(), &source, 3, 2, &mut target)
        .unwrap();
    assert_eq!(bytes, [37, 211, 211, 211]);
    assert_eq!(target.data, pointer);
    assert!(
        source.data.is_null(),
        "Uniform input must not be materialized by the host"
    );
    let before = bytes;
    let error = plugin
        .a8_geometry(
            &abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 4,
                    x: 0,
                    y: 0,
                },
                fill: 91,
            },
            &source,
            3,
            2,
            &mut target,
        )
        .unwrap_err();
    assert!(error.contains("geometry") && error.contains("-72"));
    let error = plugin
        .a8_resample(&abi::Resample { method: 2 }, &source, 3, 2, &mut target)
        .unwrap_err();
    assert!(error.contains("resample") && error.contains("-73"));
    assert_eq!(bytes, before);
    let error = plugin
        .a8_reframe(
            &abi::A8Reframe { x: 0, ..request() },
            &source,
            3,
            2,
            &mut target,
        )
        .unwrap_err();
    assert!(error.contains("reframe") && error.ends_with(": 1"));
    assert_eq!(bytes, before);
    let absent = stub(None);
    assert!(!absent.supports_a8_sampler());
    assert!(absent
        .a8_reframe(&request(), &source, 3, 2, &mut target)
        .is_err());
    assert!(absent
        .a8_geometry(
            &abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 0,
                    x: 0,
                    y: 0
                },
                fill: 0
            },
            &source,
            3,
            2,
            &mut target,
        )
        .is_err());
    assert!(absent
        .a8_resample(&abi::Resample { method: 0 }, &source, 3, 2, &mut target)
        .is_err());
    assert_eq!(bytes, before);
}

#[test]
fn loaded_a8_sampler_uses_borrowed_strided_planes_and_explicit_fill() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let registry = crate::plugins::Registry::load(&root).unwrap();
    let plugin = registry.native_plugins.get("painting").unwrap();
    assert!(
        plugin.supports_a8_sampler(),
        "Stage the matching painting DLL"
    );
    assert!(plugin.geometry.is_some() && plugin.resample.is_some());
    assert_ne!(plugin.resample_features & abi::RESAMPLE_AREA, 0);
    let source_bytes = [0, 64, 255, 201, 202, 128, 192, 254, 203, 204];
    let source = abi::CompositeMask {
        data: source_bytes.as_ptr(),
        len: source_bytes.len(),
        stride: 5,
        constant: 0,
    };
    let mut bytes = [205; 18];
    let pointer = bytes.as_mut_ptr();
    let mut target = output(&mut bytes, 4, 3, 6);
    let identity = abi::A8Reframe {
        x: 0,
        y: 0,
        fill: 99,
        reserved: 0,
    };
    plugin
        .a8_reframe(&identity, &source, 3, 2, &mut target)
        .unwrap();
    assert_eq!(
        bytes,
        [0, 64, 255, 99, 205, 205, 128, 192, 254, 99, 205, 205, 99, 99, 99, 99, 205, 205]
    );
    assert_eq!(source.data, source_bytes.as_ptr());
    assert_eq!(target.data, pointer);

    bytes.fill(205);
    plugin
        .a8_reframe(
            &abi::A8Reframe {
                x: -1,
                y: 1,
                fill: 99,
                reserved: 0,
            },
            &source,
            3,
            2,
            &mut target,
        )
        .unwrap();
    assert_eq!(
        bytes,
        [99, 99, 99, 99, 205, 205, 64, 255, 99, 99, 205, 205, 192, 254, 99, 99, 205, 205]
    );
    bytes.fill(205);
    plugin
        .a8_reframe(&identity, &uniform(42), 3, 2, &mut target)
        .unwrap();
    assert_eq!(
        bytes,
        [42, 42, 42, 99, 205, 205, 42, 42, 42, 99, 205, 205, 99, 99, 99, 99, 205, 205]
    );

    let mut rotated = [205; 12];
    let mut target = output(&mut rotated, 2, 3, 4);
    plugin
        .a8_geometry(
            &abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 0,
                    x: 0,
                    y: 0,
                },
                fill: 99,
            },
            &source,
            3,
            2,
            &mut target,
        )
        .unwrap();
    assert_eq!(
        rotated,
        [128, 0, 205, 205, 192, 64, 205, 205, 254, 255, 205, 205]
    );
    let mut nearest = [205; 8];
    let mut target = output(&mut nearest, 2, 2, 4);
    plugin
        .a8_resample(&abi::Resample { method: 0 }, &source, 3, 2, &mut target)
        .unwrap();
    assert_eq!(nearest, [0, 255, 205, 205, 128, 254, 205, 205]);
    let mut average = [205; 3];
    let mut target = output(&mut average, 1, 1, 3);
    plugin
        .a8_resample(&abi::Resample { method: 2 }, &source, 3, 2, &mut target)
        .unwrap();
    assert_eq!(average, [149, 205, 205]);
    let mut constant = [205; 18];
    let mut target = output(&mut constant, 4, 3, 6);
    plugin
        .a8_resample(
            &abi::Resample { method: 1 },
            &uniform(73),
            3,
            2,
            &mut target,
        )
        .unwrap();
    assert_eq!(
        constant,
        [73, 73, 73, 73, 205, 205, 73, 73, 73, 73, 205, 205, 73, 73, 73, 73, 205, 205]
    );
    assert_eq!(
        source_bytes,
        [0, 64, 255, 201, 202, 128, 192, 254, 203, 204]
    );
}

#[test]
fn loaded_a8_sampler_propagates_prewrite_rejection_without_changing_sources_or_padding() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let registry = crate::plugins::Registry::load(&root).unwrap();
    let plugin = registry.native_plugins.get("painting").unwrap();
    assert!(
        plugin.supports_a8_sampler(),
        "Stage the matching painting DLL"
    );
    let source_bytes = [0, 64, 128, 255];
    let source = abi::CompositeMask {
        data: source_bytes.as_ptr(),
        len: 4,
        stride: 2,
        constant: 0,
    };
    let mut bytes = [211; 6];
    let mut target = output(&mut bytes, 2, 2, 3);
    assert!(plugin
        .a8_reframe(
            &abi::A8Reframe {
                x: 0,
                y: 0,
                fill: 256,
                reserved: 0
            },
            &source,
            2,
            2,
            &mut target,
        )
        .is_err());
    assert!(plugin
        .a8_reframe(
            &abi::A8Reframe {
                x: 0,
                y: 0,
                fill: 0,
                reserved: 1
            },
            &source,
            2,
            2,
            &mut target,
        )
        .is_err());
    assert!(plugin
        .a8_geometry(
            &abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 5,
                    x: 0,
                    y: 0
                },
                fill: 0
            },
            &source,
            2,
            2,
            &mut target,
        )
        .is_err());
    assert!(plugin
        .a8_resample(&abi::Resample { method: 3 }, &source, 2, 2, &mut target)
        .is_err());
    assert!(plugin
        .a8_resample(
            &abi::Resample { method: 0 },
            &uniform(256),
            2,
            2,
            &mut target
        )
        .is_err());
    target.origin_x = 1;
    assert!(plugin
        .a8_reframe(
            &abi::A8Reframe {
                x: 0,
                y: 0,
                fill: 0,
                reserved: 0
            },
            &source,
            2,
            2,
            &mut target,
        )
        .is_err());
    assert_eq!(bytes, [211; 6]);
    assert_eq!(source_bytes, [0, 64, 128, 255]);
}

#[test]
fn preserved_painting_dll_without_sampler_keeps_its_legacy_features_and_rejects_new_calls() {
    let baseline = std::env::var_os("MICOPI_A8_BASELINE_DLL")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target/a8-sampler-baseline/micopi_painting_tools.dll")
        });
    if !baseline.is_file() {
        eprintln!(
            "Previous painting DLL compatibility check skipped: {}",
            baseline.display()
        );
        return;
    }
    let (plugin, _) = NativePlugin::load(
        baseline.parent().unwrap(),
        baseline.file_name().unwrap().to_str().unwrap(),
        "painting",
    )
    .unwrap();
    assert!(plugin.has_tools());
    assert!(
        plugin.geometry.is_some() && plugin.resample.is_some() && plugin.selection_mix.is_some()
    );
    assert_ne!(plugin.resample_features & abi::RESAMPLE_AREA, 0);
    assert!(!plugin.supports_a8_sampler());
    let mut bytes = [211; 4];
    let mut target = output(&mut bytes, 2, 2, 2);
    let source = uniform(255);
    assert!(plugin
        .a8_reframe(&request(), &source, 2, 2, &mut target)
        .is_err());
    assert!(plugin
        .a8_geometry(
            &abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 0,
                    x: 0,
                    y: 0
                },
                fill: 0
            },
            &source,
            2,
            2,
            &mut target,
        )
        .is_err());
    assert!(plugin
        .a8_resample(&abi::Resample { method: 0 }, &source, 2, 2, &mut target)
        .is_err());
    assert_eq!(bytes, [211; 4]);
}
