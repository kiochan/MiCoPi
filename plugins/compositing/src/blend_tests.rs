//! Independent encoded-RGB references for the public blend opcodes.
use super::*;

fn color(pixel: u32) -> [f64; 3] {
    [16, 8, 0].map(|shift| ((pixel >> shift) & 255) as f64 / 255.)
}
fn luminance(c: [f64; 3]) -> f64 {
    c[0] * 0.30 + c[1] * 0.59 + c[2] * 0.11
}
fn saturation(c: [f64; 3]) -> f64 {
    c.into_iter().fold(f64::NEG_INFINITY, f64::max) - c.into_iter().fold(f64::INFINITY, f64::min)
}
fn with_saturation(c: [f64; 3], range: f64) -> [f64; 3] {
    let low = c.into_iter().fold(f64::INFINITY, f64::min);
    let span = saturation(c);
    c.map(|v| {
        if span == 0. {
            0.
        } else {
            (v - low) * range / span
        }
    })
}
fn with_luminance(c: [f64; 3], target: f64) -> [f64; 3] {
    // Clip along the ray from the requested neutral gray, preserving luminance.
    let shifted = c.map(|v| v + target - luminance(c));
    let low = shifted.into_iter().fold(f64::INFINITY, f64::min);
    let high = shifted.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let lower = if low < 0. {
        target / (target - low)
    } else {
        1.
    };
    let upper = if high > 1. {
        (1. - target) / (high - target)
    } else {
        1.
    };
    shifted.map(|v| target + (v - target) * lower.min(upper))
}
fn burn(b: f64, s: f64) -> f64 {
    if b == 1. {
        1.
    } else if s == 0. {
        0.
    } else {
        1. - ((1. - b) / s).min(1.)
    }
}
fn dodge(b: f64, s: f64) -> f64 {
    if b == 0. {
        0.
    } else if s == 1. {
        1.
    } else {
        (b / (1. - s)).min(1.)
    }
}
fn vivid(b: f64, s: f64) -> f64 {
    if s < 0.5 {
        burn(b, 2. * s)
    } else {
        dodge(b, 2. * s - 1.)
    }
}
fn hard_mix_reference(backdrop: u32, source: u32) -> f64 {
    // Compare the exact W3C Burn/Dodge fraction with 1/2 before quantizing.
    // This avoids a floating-point Vivid Light tie falling below its boundary.
    let (numerator, denominator) = if source < 128 {
        if backdrop == 255 {
            (1, 1)
        } else if source == 0 {
            (0, 1)
        } else {
            ((backdrop + 2 * source).saturating_sub(255), 2 * source)
        }
    } else if backdrop == 0 {
        (0, 1)
    } else if source == 255 {
        (1, 1)
    } else {
        (backdrop.min(2 * (255 - source)), 2 * (255 - source))
    };
    if numerator * 2 >= denominator {
        1.
    } else {
        0.
    }
}
fn blend_reference(backdrop: u32, source: u32, mode: u32) -> [f64; 3] {
    let b = color(backdrop);
    let s = color(source);
    match mode {
        10 | 12 => {
            let sum = |p: u32| [0, 8, 16].map(|n| (p >> n) & 255).into_iter().sum::<u32>();
            if (mode == 10 && sum(source) < sum(backdrop))
                || (mode == 12 && sum(source) > sum(backdrop))
            {
                s
            } else {
                b
            }
        }
        18 => [16, 8, 0]
            .map(|shift| hard_mix_reference((backdrop >> shift) & 255, (source >> shift) & 255)),
        22 => with_luminance(with_saturation(s, saturation(b)), luminance(b)),
        23 => with_luminance(with_saturation(b, saturation(s)), luminance(b)),
        24 => with_luminance(s, luminance(b)),
        25 => with_luminance(b, luminance(s)),
        _ => std::array::from_fn(|i| {
            let (b, s) = (b[i], s[i]);
            match mode {
                0 => s,
                1 => b * s,
                2 => 1. - (1. - b) * (1. - s),
                3 => {
                    if b <= 0.5 {
                        2. * b * s
                    } else {
                        1. - 2. * (1. - b) * (1. - s)
                    }
                }
                4 => b.min(s),
                5 => b.max(s),
                6 => (b + s).min(1.),
                7 => (b - s).abs(),
                8 => burn(b, s),
                9 => (b + s - 1.).max(0.),
                11 => dodge(b, s),
                13 => {
                    if s <= 0.5 {
                        b - (1. - 2. * s) * b * (1. - b)
                    } else {
                        let d = if b <= 0.25 {
                            ((16. * b - 12.) * b + 4.) * b
                        } else {
                            b.sqrt()
                        };
                        b + (2. * s - 1.) * (d - b)
                    }
                }
                14 => {
                    if s <= 0.5 {
                        2. * b * s
                    } else {
                        1. - 2. * (1. - b) * (1. - s)
                    }
                }
                15 => vivid(b, s),
                16 => (b + 2. * s - 1.).clamp(0., 1.),
                17 => {
                    if s <= 0.5 {
                        b.min(2. * s)
                    } else {
                        b.max(2. * s - 1.)
                    }
                }
                19 => b + s - 2. * b * s,
                20 => (b - s).max(0.),
                21 => {
                    if s == 0. {
                        1.
                    } else {
                        (b / s).min(1.)
                    }
                }
                _ => panic!("unknown mode {mode}"),
            }
        }),
    }
}

#[test]
fn exact_half_byte_burn_and_hard_mix_thresholds_match_high_precision_golden_outputs() {
    assert_eq!(over(0xfffa0000, 0xffaa0000, 8), 0xfff80000);
    assert_eq!(over(0xfffa0000, 0xff060000, 8), 0xff2b0000);
    assert_eq!(over(0x6dfa44d8, 0x42aa8257, 8), 0x93e547a8);
    assert_eq!(over(0xff040404, 0xfffbfbfb, 18), 0xffffffff);
    assert_eq!(over(0xff000000, 0xffffffff, 18), 0xff000000);
    assert_eq!(over(0xffffffff, 0xff000000, 18), 0xffffffff);

    // FNV-1a over all 65,536 opaque gray byte pairs, backdrop outer/source
    // inner, each output encoded as little-endian ARGB u32. The independent
    // golden generator used Python Decimal precision 60 + ROUND_HALF_UP for
    // W3C Soft Light, and exact Fraction Vivid Light >= 1/2 for Hard Mix.
    // No production MiCoPi module, native DLL or f64 kernel was consulted.
    for (mode, expected) in [(13, 0xcc58ce000879f8eau64), (18, 0xe0a7af27ed2cd984)] {
        let mut digest = 0xcbf29ce484222325u64;
        for b in 0..=255 {
            for s in 0..=255 {
                let pixel = over(0xff000000 | b * 0x010101, 0xff000000 | s * 0x010101, mode);
                if mode == 18 {
                    let channel = if hard_mix_reference(b, s) == 1. {
                        255
                    } else {
                        0
                    };
                    assert_eq!(pixel, 0xff000000 | channel * 0x010101, "Hard Mix {b}/{s}");
                }
                for byte in pixel.to_le_bytes() {
                    digest = (digest ^ byte as u64).wrapping_mul(0x100000001b3);
                }
            }
        }
        assert_eq!(
            digest, expected,
            "exhaustive opcode {mode} high-precision fixture"
        );
    }
}
fn reference(backdrop: u32, source: u32, mode: u32) -> u32 {
    let sa = (source >> 24) as f64 / 255.;
    if sa == 0. {
        return backdrop;
    }
    let ba = (backdrop >> 24) as f64 / 255.;
    let alpha = ((sa + ba * (1. - sa)) * 255. + 1e-9).round() as u32;
    let mixed = blend_reference(backdrop, source, mode);
    let b = color(backdrop);
    let s = color(source);
    let mut result = alpha << 24;
    for i in 0..3 {
        // The blend channel and output alpha are quantized before coverage,
        // retaining the established host/DLL 8-bit contract and truncation bias.
        let blended = (mixed[i].clamp(0., 1.) * 255. + 1e-9).round() / 255.;
        let premultiplied = ba * (1. - sa) * b[i] + sa * (1. - ba) * s[i] + ba * sa * blended;
        let channel = (premultiplied * 255. / (alpha as f64 / 255.) + 127. / 255. + 1e-9)
            .floor()
            .clamp(0., 255.) as u32;
        result |= channel << [16, 8, 0][i];
    }
    result
}
fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    *state
}
fn descriptors(sources: &[Vec<u32>]) -> Vec<abi::CompositeLayer> {
    sources
        .iter()
        .map(|p| abi::CompositeLayer {
            data: p.as_ptr(),
            len: p.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        })
        .collect()
}
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
fn expanded_modes_match_normalized_reference_at_endpoints_neutrals_ties_and_random_colors() {
    // W3C Compositing Level 1 blending equations; the extended arithmetic
    // conventions and quantization are intentionally explicit in this oracle.
    let palette = [
        0, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0x808080, 0x7f8081, 0x817f80, 0x7f7f7f,
        0x404040, 0xc0c0c0, 0x0100fe,
    ];
    for mode in 8..26 {
        for b in palette {
            for s in palette {
                for (ba, sa) in [(0, 0), (0, 255), (64, 128), (128, 64), (255, 255)] {
                    let (b, s) = ((ba << 24) | b, (sa << 24) | s);
                    assert_eq!(
                        over(b, s, mode),
                        reference(b, s, mode),
                        "mode {mode}, {b:08x}/{s:08x}"
                    );
                }
            }
        }
        let mut random = 0x7ae39b21;
        for _ in 0..2048 {
            let (b, s) = (next(&mut random), next(&mut random));
            assert_eq!(
                over(b, s, mode),
                reference(b, s, mode),
                "mode {mode}, {b:08x}/{s:08x}"
            );
        }
    }
    assert_eq!(over(0xff000000, 0xff000000, 21), 0xffffffff);
    assert_eq!(over(0xffffffff, 0xff000000, 8), 0xffffffff);
    assert_eq!(over(0xff000000, 0xffffffff, 11), 0xff000000);
    for mode in [10, 12] {
        assert_eq!(
            over(0xff7f8081, 0xff817f80, mode),
            0xff7f8081,
            "backdrop wins sum tie"
        );
    }
}

#[test]
fn original_eight_modes_keep_previous_dll_fixture_hashes() {
    // Captured from the prior 24-byte compositor table, SHA-256
    // 370a84c85e8d292322d029c95f8696de49ee9cf3b624115abc758728de11b73e.
    let expected = [
        0x53d40785cb0806f9,
        0xb9d031450ac4a532,
        0x28679c181d95c92f,
        0x922a6875a7be030f,
        0xccba2ff2efb3aafc,
        0x6d55172366ed032d,
        0xa476eaec4c986b6f,
        0xff5081bebf02a140,
    ];
    let mut state = 0x5eed1234;
    let sources: Vec<Vec<u32>> = (0..2)
        .map(|_| (0..256).map(|_| next(&mut state)).collect())
        .collect();
    let mut layers = descriptors(&sources);
    for (mode, expected) in expected.into_iter().enumerate() {
        layers[1].mode = mode as u32;
        let mut pixels = vec![0; 256];
        let mut output = abi::Pixels {
            data: pixels.as_mut_ptr(),
            len: pixels.len(),
            width: 16,
            height: 16,
            stride: 16,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe { render(std::ptr::null(), 0, layers.as_ptr(), 2, 16, 16, &mut output) },
            abi::OK
        );
        let digest = pixels
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ byte as u64).wrapping_mul(0x100000001b3)
            });
        assert_eq!(digest, expected, "legacy opcode {mode}");
    }
}

#[test]
fn extended_modes_render_dirty_strides_with_identical_layer_and_graph_gates() {
    let mut state = 0x72617374;
    let sources: Vec<Vec<u32>> = (0..2)
        .map(|_| (0..35).map(|_| next(&mut state)).collect())
        .collect();
    for mode in 8..26 {
        let mut layers = descriptors(&sources);
        layers[1].mode = mode;
        layers[1].opacity = 137;
        let nodes = [
            instruction(0, 0, 0, 0, 1.),
            instruction(0, 1, 0, 0, 1.),
            instruction(2, 0, 1, mode, 1.),
            instruction(3, 2, 0, 0, 1.),
        ];
        for graph in [false, true] {
            let mut result = [0xdeadbeef; 17];
            let mut output = abi::Pixels {
                data: result.as_mut_ptr(),
                len: result.len(),
                width: 3,
                height: 3,
                stride: 5,
                origin_x: 2,
                origin_y: 1,
            };
            let (ptr, count) = if graph {
                (nodes.as_ptr(), nodes.len())
            } else {
                (std::ptr::null(), 0)
            };
            assert_eq!(
                unsafe { render(ptr, count, layers.as_ptr(), 2, 7, 5, &mut output) },
                abi::OK
            );
            for y in 0..3 {
                for x in 0..5 {
                    if y * 5 + x >= result.len() {
                        continue;
                    }
                    let expected = if x < 3 {
                        let i = (y + 1) * 7 + x + 2;
                        let b = reference(0, sources[0][i], 0);
                        let s = opacity(sources[1][i], 137);
                        reference(b, if s >> 24 == 0 { 0 } else { s }, mode)
                    } else {
                        0xdeadbeef
                    };
                    assert_eq!(
                        result[y * 5 + x],
                        expected,
                        "mode {mode} graph {graph} ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn extended_mode_validation_rejects_before_writing_even_hidden_or_unreachable_entries() {
    let sources = vec![vec![0xff804020; 4], vec![0xffc02080; 4]];
    let mut layers = descriptors(&sources);
    let sentinel = [0xdeadbeef; 4];
    let reject = |nodes: &[abi::CompositeNode], layers: &[abi::CompositeLayer]| {
        let mut pixels = sentinel;
        let mut output = abi::Pixels {
            data: pixels.as_mut_ptr(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    nodes.len(),
                    layers.as_ptr(),
                    layers.len(),
                    2,
                    2,
                    &mut output,
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, sentinel);
    };
    layers[1].visible = 0;
    layers[1].mode = 26;
    reject(&[], &layers);
    layers[1].mode = 0;
    reject(
        &[
            instruction(0, 0, 0, 0, 1.),
            instruction(2, 0, 0, 26, 1.),
            instruction(3, 0, 0, 0, 1.),
        ],
        &layers,
    );
    reject(
        &[
            instruction(0, 0, 0, 0, 1.),
            instruction(2, 0, 2, 25, 1.),
            instruction(3, 0, 0, 0, 1.),
        ],
        &layers,
    );
    reject(
        &[
            instruction(0, 0, 0, 0, 1.),
            instruction(2, 0, 0, 25, f32::NAN),
            instruction(3, 1, 0, 0, 1.),
        ],
        &layers,
    );
    let mut pixels = sources[0].clone();
    layers[0].data = pixels.as_ptr();
    let mut output = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: 4,
        width: 2,
        height: 2,
        stride: 2,
        origin_x: 0,
        origin_y: 0,
    };
    assert_eq!(
        unsafe { render(std::ptr::null(), 0, layers.as_ptr(), 2, 2, 2, &mut output) },
        abi::ERROR
    );
    assert_eq!(pixels, sources[0]);
}

#[test]
#[ignore = "Release-only diagnostic; no CI timing threshold"]
fn extended_blend_cost() {
    use std::time::Instant;
    const SIDE: u32 = 1024;
    let mut state = 0x6d656173;
    let sources: Vec<Vec<u32>> = (0..2)
        .map(|_| {
            (0..SIDE * SIDE)
                .map(|_| next(&mut state) | 0xff000000)
                .collect()
        })
        .collect();
    let layers = descriptors(&sources);
    let mut pixels = vec![0; (SIDE * SIDE) as usize];
    let mut output = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: pixels.len(),
        width: SIDE,
        height: SIDE,
        stride: SIDE,
        origin_x: 0,
        origin_y: 0,
    };
    for mode in 8..26 {
        let nodes = [
            instruction(0, 0, 0, 0, 1.),
            instruction(0, 1, 0, 0, 1.),
            instruction(2, 0, 1, mode, 1.),
            instruction(3, 2, 0, 0, 1.),
        ];
        let begin = Instant::now();
        let status = unsafe {
            render(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                2,
                SIDE,
                SIDE,
                &mut output,
            )
        };
        let elapsed = begin.elapsed();
        assert_eq!(status, abi::OK);
        for &i in &[0, 1, 127, 1023, 1024, 524287, 1048575] {
            assert_eq!(
                pixels[i],
                reference(sources[0][i], sources[1][i], mode),
                "mode {mode} sample {i}"
            );
        }
        eprintln!("extended_blend_cost opcode={mode} region={SIDE}x{SIDE} cold_single_render_ms={:.3}; two prepared opaque ARGB sources, four instructions; validation+native composition included; allocations, fixture generation and reference checks excluded; no Session/history/journal, encoding, I/O or pen-latency measurement",elapsed.as_secs_f64()*1000.);
    }
}
