use super::*;
fn node(kind: u32, a: u32, mode: u32) -> abi::CompositeNode {
    abi::CompositeNode {
        kind,
        a,
        b: 0,
        mode,
        amount: 1.,
    }
}
fn layer(pixels: &[u32]) -> abi::CompositeLayer {
    abi::CompositeLayer {
        data: pixels.as_ptr(),
        len: pixels.len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    }
}
unsafe fn draw(
    api: &abi::HslApi,
    nodes: &[abi::CompositeNode],
    layers: &[abi::CompositeLayer],
    out: &mut [u32],
) -> i32 {
    let mut target = abi::Pixels {
        data: out.as_mut_ptr(),
        len: out.len(),
        width: 2,
        height: 2,
        stride: 3,
        origin_x: 1,
        origin_y: 0,
    };
    api.render.unwrap()(
        nodes.as_ptr(),
        nodes.len(),
        layers.as_ptr(),
        layers.len(),
        std::ptr::null(),
        0,
        std::ptr::null(),
        0,
        std::ptr::null(),
        0,
        4,
        2,
        &mut target,
    )
}
fn reference(pixel: u32, kind: u32, level: u32) -> u32 {
    if pixel >> 24 == 0 {
        return 0;
    }
    let r = (pixel >> 16) & 255;
    let g = (pixel >> 8) & 255;
    let b = pixel & 255;
    let rgb = if kind == 12 {
        let gray = (54. * r as f64 + 183. * g as f64 + 19. * b as f64) / 256.;
        if gray >= level as f64 {
            0xffffff
        } else {
            0
        }
    } else {
        let n = (level - 1) as f64;
        let q = |c: u32| (((c as f64 * n / 255.).round() * 255. / n).round()) as u32;
        q(r) << 16 | q(g) << 8 | q(b)
    };
    (pixel & 0xff000000) | rgb
}
#[test]
fn padded_roi_and_histograms_match_independent_quantization_references() {
    let source = [
        0x80ff0000, 0xff204060, 0x80112233, 0x00123456, 0xff00ff00, 0xff0000ff, 0xff808080,
        0xffffffff,
    ];
    let layers = [layer(&source)];
    let api = unsafe { &*crate::hsl::micopi_quantization_v1() };
    assert_eq!(api.abi_version, 1);
    assert_eq!(api.struct_size, 24);
    for (kind, level) in [
        (12, 0),
        (12, 53),
        (12, 54),
        (12, 128),
        (12, 255),
        (13, 2),
        (13, 4),
        (13, 7),
        (13, 256),
    ] {
        let nodes = [node(0, 0, 0), node(kind, 0, level), node(3, 1, 0)];
        let mut out = [0x12345678; 6];
        assert_eq!(unsafe { draw(api, &nodes, &layers, &mut out) }, abi::OK);
        assert_eq!(
            out,
            [
                reference(source[1], kind, level),
                reference(source[2], kind, level),
                0x12345678,
                reference(source[5], kind, level),
                reference(source[6], kind, level),
                0x12345678
            ]
        );
        let request = abi::HistogramRequest {
            abi_version: 1,
            struct_size: 24,
            width: 4,
            height: 2,
            stride: 4,
            max_samples: 8,
        };
        let mut result = abi::HistogramResult::default();
        assert_eq!(
            unsafe {
                api.histogram.unwrap()(
                    &request,
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    &mut result,
                )
            },
            abi::OK
        );
        assert_eq!(result.sampled_pixels, 8);
        assert_eq!(result.nontransparent_pixels, 7);
        for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
            let mut expected = [0u64; 256];
            for p in source {
                if p >> 24 != 0 {
                    expected[((reference(p, kind, level) >> shift) & 255) as usize] += 1;
                }
            }
            assert_eq!(result.bins[channel], expected);
        }
    }
}
#[test]
fn legacy_rejection_invalid_disconnected_declarations_and_aliases_preserve_output() {
    let source = [0x80ff0000; 8];
    let layers = [layer(&source)];
    let api = unsafe { &*crate::hsl::micopi_quantization_v1() };
    let legacy = unsafe { &*crate::hsl::micopi_hsl_v1() };
    for kind in [12, 13] {
        let nodes = [node(0, 0, 0), node(kind, 0, 128), node(3, 1, 0)];
        let mut out = [93; 6];
        assert_eq!(
            unsafe { draw(legacy, &nodes, &layers, &mut out) },
            abi::ERROR
        );
        assert_eq!(out, [93; 6]);
        for invalid in [
            node(kind, 0, 257),
            node(kind, 0, u32::MAX),
            node(kind, 9, 128),
            abi::CompositeNode {
                b: 1,
                ..node(kind, 0, 128)
            },
            abi::CompositeNode {
                amount: 0.5,
                ..node(kind, 0, 128)
            },
        ] {
            // The invalid effect is disconnected from the published output.
            let nodes = [node(0, 0, 0), invalid, node(3, 0, 0)];
            assert_eq!(unsafe { draw(api, &nodes, &layers, &mut out) }, abi::ERROR);
            assert_eq!(out, [93; 6]);
            let request = abi::HistogramRequest {
                abi_version: 1,
                struct_size: 24,
                width: 4,
                height: 2,
                stride: 4,
                max_samples: 8,
            };
            let mut result = abi::HistogramResult::default();
            result.sampled_pixels = 93;
            assert_eq!(
                unsafe {
                    api.histogram.unwrap()(
                        &request,
                        nodes.as_ptr(),
                        3,
                        layers.as_ptr(),
                        1,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        std::ptr::null(),
                        0,
                        &mut result,
                    )
                },
                abi::ERROR
            );
            assert_eq!(result.sampled_pixels, 93);
            assert_eq!(result.bins, [[0; 256]; 4]);
        }
        let mut alias = [93; 8];
        let descriptor = abi::CompositeLayer {
            data: alias.as_ptr(),
            len: 8,
            opacity: 255,
            visible: 1,
            mode: 0,
        };
        assert_eq!(
            unsafe { draw(api, &nodes, &[descriptor], &mut alias) },
            abi::ERROR
        );
        assert_eq!(alias, [93; 8]);
    }
}
#[test]
fn quantization_composes_with_hsl_and_typed_a8_without_intermediate_images() {
    let source = [0x80ff0000];
    let layers = [layer(&source)];
    let masks = [abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 128,
    }];
    let mut coverage = node(6, 0, 0);
    coverage.amount = 0.;
    let mut apply = node(7, 3, 0);
    apply.b = 4;
    let nodes = [
        node(0, 0, 0),
        node(11, 0, 0),
        node(13, 1, 4),
        node(12, 2, 60),
        coverage,
        apply,
        node(3, 5, 0),
    ];
    let parameters = [abi::HslParameters {
        hue: 120.,
        saturation: 0.,
        lightness: 0.,
    }];
    let mut pixel = 93;
    let mut target = abi::Pixels {
        data: &mut pixel,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    let api = unsafe { &*crate::hsl::micopi_quantization_v1() };
    assert_eq!(
        unsafe {
            api.render.unwrap()(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                1,
                masks.as_ptr(),
                1,
                std::ptr::null(),
                0,
                parameters.as_ptr(),
                1,
                1,
                1,
                &mut target,
            )
        },
        abi::OK
    );
    assert_eq!(pixel, 0x40ffffff);
}
