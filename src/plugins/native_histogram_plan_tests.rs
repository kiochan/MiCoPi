use super::*;
fn request() -> abi::HistogramRequest {
    abi::HistogramRequest {
        abi_version: 0,
        struct_size: 0,
        width: 1,
        height: 1,
        stride: 1,
        max_samples: 1,
    }
}
fn nodes() -> [abi::CompositeNode; 3] {
    [
        abi::CompositeNode {
            kind: 0,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 1,
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
    ]
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
unsafe extern "C" fn raw(
    _: *const abi::HistogramRequest,
    _: *const u32,
    _: usize,
    _: *mut abi::HistogramResult,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn identity(
    p: *const abi::HistogramRequest,
    _: *const abi::CompositeNode,
    _: usize,
    _: *const abi::CompositeLayer,
    _: usize,
    _: *const abi::CompositeMask,
    _: usize,
    _: *const abi::CompositeLut,
    _: usize,
    out: *mut abi::HistogramResult,
) -> i32 {
    assert_eq!(((*p).abi_version, (*p).struct_size), (1, 24));
    (*out).grid_width = 1;
    (*out).grid_height = 1;
    (*out).sampled_pixels = 1;
    (*out).nontransparent_pixels = 1;
    for channel in 0..3 {
        (*out).bins[channel][0] = 1;
    }
    (*out).bins[3][0] = 3;
    abi::OK
}
unsafe extern "C" fn failed(
    _: *const abi::HistogramRequest,
    _: *const abi::CompositeNode,
    _: usize,
    _: *const abi::CompositeLayer,
    _: usize,
    _: *const abi::CompositeMask,
    _: usize,
    _: *const abi::CompositeLut,
    _: usize,
    out: *mut abi::HistogramResult,
) -> i32 {
    (*out).bins[0][0] = u64::MAX;
    abi::ERROR
}
unsafe extern "C" fn corrupt(
    p: *const abi::HistogramRequest,
    n: *const abi::CompositeNode,
    nc: usize,
    l: *const abi::CompositeLayer,
    lc: usize,
    m: *const abi::CompositeMask,
    mc: usize,
    t: *const abi::CompositeLut,
    tc: usize,
    out: *mut abi::HistogramResult,
) -> i32 {
    identity(p, n, nc, l, lc, m, mc, t, tc, out);
    (*out).bins[0][0] = u64::MAX;
    abi::OK
}
#[test]
fn decoder_retains_raw_prefix_and_rejects_partial_or_missing_plan_tails() {
    unsafe {
        assert!(decode_plan(std::ptr::null()).is_err());
        for size in [8, 16, 20, 23, 24, 32] {
            let api = abi::HistogramApiPlan {
                base: abi::HistogramApi {
                    abi_version: 1,
                    struct_size: size,
                    sample: Some(raw),
                },
                sample_plan: Some(identity),
            };
            let result = decode_plan(&api.base);
            match size {
                16 => assert!(result.unwrap().is_none()),
                24 | 32 => assert!(result.unwrap().is_some()),
                _ => assert!(result.is_err()),
            }
        }
        let api = abi::HistogramApiPlan {
            base: abi::HistogramApi {
                abi_version: 1,
                struct_size: 24,
                sample: Some(raw),
            },
            sample_plan: None,
        };
        assert!(decode_plan(&api.base).is_err());
    }
}
#[test]
fn adapter_checks_bounds_and_never_publishes_failed_or_corrupt_plan_output() {
    let data = [0xff123456];
    let layers = [layer(&data)];
    let n = nodes();
    let p = request();
    assert_eq!(
        sample_plan(None, &n, &layers, &[], &[], p)
            .err()
            .unwrap()
            .code,
        "histogram_plan_unavailable"
    );
    for (callback, expected) in [
        (failed as abi::HistogramPlanFn, "histogram_plan_sample"),
        (corrupt as abi::HistogramPlanFn, "histogram_result"),
    ] {
        let code = sample_plan(Some(callback), &n, &layers, &[], &[], p)
            .err()
            .unwrap()
            .code;
        assert_eq!(code, expected);
    }
    let mut bad = p;
    bad.stride = 2;
    assert_eq!(
        sample_plan(Some(identity), &n, &layers, &[], &[], bad)
            .err()
            .unwrap()
            .code,
        "histogram_parameters"
    );
    assert_eq!(
        sample_plan(Some(identity), &[], &layers, &[], &[], p)
            .err()
            .unwrap()
            .code,
        "histogram_parameters"
    );
    assert_eq!(
        sample_plan(Some(identity), &n, &[], &[], &[], p)
            .err()
            .unwrap()
            .code,
        "histogram_parameters"
    );
    let result = sample_plan(Some(identity), &n, &layers, &[], &[], p).unwrap();
    assert_eq!(result.bins[3][0], 3);
}
#[test]
fn result_validation_rejects_a_smaller_but_in_bounds_approximate_grid() {
    let p = abi::HistogramRequest {
        width: 4,
        height: 4,
        stride: 4,
        max_samples: 4,
        ..request()
    };
    let result = abi::HistogramResult {
        grid_width: 1,
        grid_height: 1,
        sampled_pixels: 1,
        ..Default::default()
    };
    assert_eq!(
        validate_result(result, &p).err().unwrap().code,
        "histogram_result"
    );
}
#[test]
fn bundled_plan_sampler_is_cached_and_agrees_with_known_invert_bins() {
    let registry =
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    let native = &registry.native_plugins["compositing"];
    assert!(native.supports_histogram());
    assert!(native.supports_histogram_plan());
    assert!(!registry.native_plugins["painting"].supports_histogram_plan());
    let callback = native.histogram_plan.unwrap() as usize;
    let data = [0xff123456];
    let layers = [layer(&data)];
    for _ in 0..3 {
        let out = native
            .histogram_plan(&nodes(), &layers, &[], &[], request())
            .unwrap();
        assert_eq!(out.bins[0][237], 1);
        assert_eq!(out.bins[1][203], 1);
        assert_eq!(out.bins[2][169], 1);
        assert_eq!(native.histogram_plan.unwrap() as usize, callback);
    }
    assert_eq!(data, [0xff123456]);
    let description = registry.describe();
    assert_eq!(
        description["native"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "compositing")
            .unwrap()["analysis"]["histogram_plan"],
        true
    );
}
