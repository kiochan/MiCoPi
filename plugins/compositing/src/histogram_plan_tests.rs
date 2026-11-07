use super::*;
fn request() -> abi::HistogramRequest {
    abi::HistogramRequest {
        abi_version: 1,
        struct_size: 24,
        width: 4,
        height: 4,
        stride: 4,
        max_samples: 4,
    }
}
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
fn input() -> [u32; 16] {
    std::array::from_fn(|v| {
        ((if v % 5 == 0 { 0 } else { 128 }) << 24) | ((255 - v as u32) << 16) | 0x4000 | v as u32
    })
}
unsafe fn call(
    p: &abi::HistogramRequest,
    n: &[abi::CompositeNode],
    l: &[abi::CompositeLayer],
    m: &[abi::CompositeMask],
    t: &[abi::CompositeLut],
    out: &mut abi::HistogramResult,
) -> i32 {
    sample_plan(
        p,
        n.as_ptr(),
        n.len(),
        l.as_ptr(),
        l.len(),
        m.as_ptr(),
        m.len(),
        t.as_ptr(),
        t.len(),
        out,
    )
}
#[test]
fn centered_plan_samples_match_independent_invert_bins_without_source_writes() {
    let data = input();
    let original = data;
    let layers = [layer(&data)];
    let nodes = [
        node(0, 0, 0, 0, 1.),
        node(1, 0, 0, 0, 1.),
        node(3, 1, 0, 0, 1.),
    ];
    let mut out = abi::HistogramResult::default();
    assert_eq!(
        unsafe { call(&request(), &nodes, &layers, &[], &[], &mut out) },
        abi::OK
    );
    assert_eq!(
        (
            out.grid_width,
            out.grid_height,
            out.sampled_pixels,
            out.nontransparent_pixels
        ),
        (2, 2, 4, 2)
    );
    for i in 0..256 {
        assert_eq!(out.bins[0][i], u64::from([7, 13].contains(&i)));
        assert_eq!(out.bins[1][i], if i == 191 { 2 } else { 0 });
        assert_eq!(out.bins[2][i], u64::from([248, 242].contains(&i)));
        assert_eq!(
            out.bins[3][i],
            out.bins[0][i] + out.bins[1][i] + out.bins[2][i]
        );
    }
    assert_eq!(data, original);
}
#[test]
fn thin_images_sample_the_center_without_a_full_width_output_buffer() {
    let mut data = vec![0xff000000; 1024];
    data[512] = 0xff204060;
    let nodes = [node(0, 0, 0, 0, 1.), node(3, 0, 0, 0, 1.)];
    let p = abi::HistogramRequest {
        width: 1024,
        height: 1,
        stride: 1024,
        max_samples: 1,
        ..request()
    };
    let mut out = abi::HistogramResult::default();
    assert_eq!(
        unsafe { call(&p, &nodes, &[layer(&data)], &[], &[], &mut out) },
        abi::OK
    );
    assert_eq!(
        (out.grid_width, out.grid_height, out.sampled_pixels),
        (1, 1, 1)
    );
    assert_eq!(out.bins[0][32], 1);
    assert_eq!(out.bins[1][64], 1);
    assert_eq!(out.bins[2][96], 1);
}
#[test]
fn masked_blend_and_mixed_lut_sampling_match_render_then_raw_histogram() {
    let a = input();
    let b = [0x7f2080f0; 16];
    let layers = [layer(&a), layer(&b)];
    let coverage: [u8; 16] = std::array::from_fn(|i| (i * 17) as u8);
    let masks = [abi::CompositeMask {
        data: coverage.as_ptr(),
        len: 16,
        stride: 4,
        constant: 0,
    }];
    let tables = [
        abi::CompositeLut {
            values: std::array::from_fn(|i| 255 - i as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| (i * 2).min(255) as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| i as u8),
        },
        abi::CompositeLut {
            values: std::array::from_fn(|i| 255 - i as u8),
        },
    ];
    let nodes = [
        node(0, 0, 0, 0, 1.),
        node(0, 1, 0, 0, 1.),
        node(2, 0, 1, 2, 0.75),
        node(6, 0, 0, 0, 0.),
        node(7, 2, 3, 0, 0.8),
        node(9, 4, 0, 0, 1.),
        node(10, 5, 0, 1, 1.),
        node(3, 6, 0, 0, 1.),
    ];
    let mut pixels = [0; 16];
    let mut target = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: 16,
        width: 4,
        height: 4,
        stride: 4,
        origin_x: 0,
        origin_y: 0,
    };
    assert_eq!(
        unsafe {
            crate::curves::render(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                2,
                masks.as_ptr(),
                1,
                tables.as_ptr(),
                4,
                4,
                4,
                &mut target,
            )
        },
        abi::OK
    );
    let mut expected = abi::HistogramResult::default();
    let mut sampled = expected;
    assert_eq!(
        unsafe { super::super::sample(&request(), pixels.as_ptr(), 16, &mut expected) },
        abi::OK
    );
    assert_eq!(
        unsafe { call(&request(), &nodes, &layers, &masks, &tables, &mut sampled) },
        abi::OK
    );
    assert_eq!(sampled.bins, expected.bins);
    assert_eq!(
        sampled.nontransparent_pixels,
        expected.nontransparent_pixels
    );
    assert_eq!(a, input());
    assert_eq!(coverage, std::array::from_fn(|i| (i * 17) as u8));
}
#[test]
fn invalid_plan_requests_and_unused_declarations_never_publish_partial_counts() {
    let data = input();
    let layers = [layer(&data)];
    let nodes = [
        node(0, 0, 0, 0, 1.),
        node(1, 0, 0, 0, 1.),
        node(3, 1, 0, 0, 1.),
    ];
    for case in 0..12 {
        let mut p = request();
        let mut n = nodes;
        let mut l = layers;
        let mut m = vec![];
        match case {
            0 => p.width = 0,
            1 => p.height = 4097,
            2 => p.stride = 5,
            3 => p.max_samples = 0,
            4 => p.max_samples = 65537,
            5 => p.abi_version = 2,
            6 => p.struct_size = 8,
            7 => n[1].mode = 99,
            8 => l[0].len = 15,
            9 => m.push(abi::CompositeMask {
                data: std::ptr::null(),
                len: 0,
                stride: 0,
                constant: 256,
            }),
            10 => n[1] = node(10, 0, 0, 0, 1.),
            _ => n[2] = node(6, 0, 0, 0, 0.),
        }
        let mut out = abi::HistogramResult {
            sampled_pixels: 77,
            nontransparent_pixels: 33,
            bins: [[91; 256]; 4],
            ..Default::default()
        };
        assert_eq!(
            unsafe { call(&p, &n, &l, &m, &[], &mut out) },
            abi::ERROR,
            "case {case}"
        );
        assert_eq!((out.sampled_pixels, out.nontransparent_pixels), (77, 33));
        assert_eq!(out.bins, [[91; 256]; 4]);
    }
    let mut out = abi::HistogramResult::default();
    assert_eq!(
        unsafe { call(&request(), &[], &layers, &[], &[], &mut out) },
        abi::ERROR
    );
    let short = abi::Header {
        abi_version: 1,
        struct_size: 8,
    };
    assert_eq!(
        unsafe {
            sample_plan(
                (&short as *const abi::Header).cast(),
                nodes.as_ptr(),
                3,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                &mut out,
            )
        },
        abi::ERROR
    );
}
#[test]
fn output_cannot_alias_pixel_loans_or_request_and_future_tails_stay_untouched() {
    let nodes = [node(0, 0, 0, 0, 1.), node(3, 0, 0, 0, 1.)];
    let mut storage = [0u64; 1100];
    let output = storage.as_mut_ptr().cast::<abi::HistogramResult>();
    unsafe {
        output.write(abi::HistogramResult::default());
    }
    let layers = [abi::CompositeLayer {
        data: output.cast(),
        len: 16,
        opacity: 255,
        visible: 1,
        mode: 0,
    }];
    let original = storage;
    assert_eq!(
        unsafe {
            sample_plan(
                &request(),
                nodes.as_ptr(),
                2,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                output,
            )
        },
        abi::ERROR
    );
    assert_eq!(storage, original);
    let data = input();
    let layers = [layer(&data)];
    let mut p = request();
    p.struct_size = 8224;
    unsafe {
        output.cast::<abi::HistogramRequest>().write(p);
    }
    let original = storage;
    assert_eq!(
        unsafe {
            sample_plan(
                output.cast(),
                nodes.as_ptr(),
                2,
                layers.as_ptr(),
                1,
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                output,
            )
        },
        abi::ERROR
    );
    assert_eq!(storage, original);
    #[repr(C)]
    struct Future {
        base: abi::HistogramResult,
        tail: u64,
    }
    let mut out = Future {
        base: abi::HistogramResult::default(),
        tail: 0x12345678,
    };
    out.base.struct_size = std::mem::size_of::<Future>() as u32;
    assert_eq!(
        unsafe { call(&request(), &nodes, &layers, &[], &[], &mut out.base) },
        abi::OK
    );
    assert_eq!(out.tail, 0x12345678);
}
