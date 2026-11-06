//! Independent integer references and prewrite guards for typed A8 composition.
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
fn constant(value: u32) -> abi::CompositeMask {
    abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: value,
    }
}
fn plane(data: &[u8], stride: u32) -> abi::CompositeMask {
    abi::CompositeMask {
        data: data.as_ptr(),
        len: data.len(),
        stride,
        constant: 0,
    }
}
fn layers(sources: &[Vec<u32>]) -> Vec<abi::CompositeLayer> {
    sources
        .iter()
        .map(|source| abi::CompositeLayer {
            data: source.as_ptr(),
            len: source.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        })
        .collect()
}
fn output(data: &mut [u32], width: u32, height: u32) -> abi::Pixels {
    abi::Pixels {
        data: data.as_mut_ptr(),
        len: data.len(),
        width,
        height,
        stride: width,
        origin_x: 0,
        origin_y: 0,
    }
}
fn nearest(n: u64, d: u64) -> u32 {
    let (whole, rest) = (n / d, n % d);
    (whole + u64::from(rest * 2 >= d)) as u32
}
fn expected_apply(pixel: u32, opacity: u32, coverage: u32) -> u32 {
    let factor = nearest(u64::from(opacity) * u64::from(coverage), 255);
    let alpha = u64::from(pixel >> 24) * u64::from(factor) / 255;
    if alpha == 0 {
        0
    } else {
        (pixel & 0xffffff) | ((alpha as u32) << 24)
    }
}
fn source(pixel: u32) -> u32 {
    if pixel >> 24 == 0 {
        0
    } else {
        pixel
    }
}
fn expected_mix(before: u32, after: u32, opacity: u32, coverage: u32) -> u32 {
    let factor = nearest(u64::from(opacity) * u64::from(coverage), 255);
    if factor == 0 {
        return before;
    }
    if factor == 255 {
        return after;
    }
    let weights = [
        u64::from((before >> 24) * (255 - factor)),
        u64::from((after >> 24) * factor),
    ];
    let denominator = weights[0] + weights[1];
    let alpha = nearest(denominator, 255);
    if alpha == 0 {
        return 0;
    }
    [16, 8, 0].into_iter().fold(alpha << 24, |result, shift| {
        let numerator = u64::from((before >> shift) & 255) * weights[0]
            + u64::from((after >> shift) & 255) * weights[1];
        result | (nearest(numerator, denominator) << shift)
    })
}
unsafe fn masked(
    instructions: &[abi::CompositeNode],
    descriptors: &[abi::CompositeLayer],
    masks: &[abi::CompositeMask],
    width: u32,
    height: u32,
    out: &mut abi::Pixels,
) -> i32 {
    masks::render_masked(
        instructions.as_ptr(),
        instructions.len(),
        descriptors.as_ptr(),
        descriptors.len(),
        masks.as_ptr(),
        masks.len(),
        width,
        height,
        out,
    )
}
fn apply_plan(opacity: u32) -> [abi::CompositeNode; 4] {
    [
        node(0, 0, 0, 0, 1.),
        node(6, 0, 0, 0, 0.),
        node(7, 0, 1, 0, opacity as f32 / 255.),
        node(3, 2, 0, 0, 1.),
    ]
}
fn mix_plan(opacity: u32) -> [abi::CompositeNode; 5] {
    [
        node(0, 0, 0, 0, 1.),
        node(0, 1, 0, 0, 1.),
        node(6, 0, 0, 0, 0.),
        node(8, 0, 1, 2, opacity as f32 / 255.),
        node(3, 3, 0, 0, 1.),
    ]
}
fn random(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    *seed
}

#[test]
fn mask_apply_matches_all_alpha_coverage_and_opacity_bytes() {
    let sources = vec![(0..65536).map(|i| ((i % 256) << 24) | 0x3f91d7).collect()];
    let descriptors = layers(&sources);
    let coverage: Vec<_> = (0..65536).map(|i| (i / 256) as u8).collect();
    let masks = [plane(&coverage, 256)];
    let mut actual = vec![0; 65536];
    for opacity in 0..=255 {
        assert_eq!((opacity as f32 / 255. * 255.).round() as u32, opacity);
        let plan = apply_plan(opacity);
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &masks,
                    256,
                    256,
                    &mut output(&mut actual, 256, 256),
                )
            },
            abi::OK
        );
        for (i, &pixel) in actual.iter().enumerate() {
            assert_eq!(
                pixel,
                expected_apply(sources[0][i], opacity, u32::from(coverage[i])),
                "alpha{}, coverage{}, opacity{opacity}",
                i % 256,
                i / 256
            );
        }
    }
}

#[test]
fn masked_mix_matches_alpha_pairs_and_every_opacity_coverage_factor() {
    let sources = vec![
        (0..65536).map(|i| ((i % 256) << 24) | 0x125ca7).collect(),
        (0..65536).map(|i| ((i / 256) << 24) | 0xd3300e).collect(),
    ];
    let descriptors = layers(&sources);
    let mut actual = vec![0; 65536];
    for (opacity, coverage) in [
        (0, 255),
        (255, 0),
        (255, 255),
        (128, 255),
        (255, 128),
        (1, 127),
        (254, 254),
    ] {
        let plan = mix_plan(opacity);
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &[constant(coverage)],
                    256,
                    256,
                    &mut output(&mut actual, 256, 256),
                )
            },
            abi::OK
        );
        for (i, &pixel) in actual.iter().enumerate() {
            assert_eq!(
                pixel,
                expected_mix(
                    source(sources[0][i]),
                    source(sources[1][i]),
                    opacity,
                    coverage
                )
            );
        }
    }
    let mut seed = 0x6d61736b;
    let random_sources = vec![
        (0..256).map(|_| random(&mut seed)).collect(),
        (0..256).map(|_| random(&mut seed)).collect(),
    ];
    let random_layers = layers(&random_sources);
    let coverage: Vec<_> = (0..=255).collect();
    let masks = [plane(&coverage, 256)];
    for opacity in 0..=255 {
        let plan = mix_plan(opacity);
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &random_layers,
                    &masks,
                    256,
                    1,
                    &mut output(&mut actual[..256], 256, 1),
                )
            },
            abi::OK
        );
        for (i, &pixel) in actual[..256].iter().enumerate() {
            assert_eq!(
                pixel,
                expected_mix(
                    source(random_sources[0][i]),
                    source(random_sources[1][i]),
                    opacity,
                    i as u32
                )
            );
        }
    }
}

#[test]
fn mask_dirty_regions_use_global_mask_strides_and_preserve_output_guards() {
    let sources = vec![(0..28).map(|i| ((i * 9 + 3) << 24) | 0x7193c2).collect()];
    let descriptors = layers(&sources);
    let coverage: Vec<_> = (0..36).map(|i| (i * 7) as u8).collect();
    let masks = [plane(&coverage, 9)];
    for opacity in 0..=255 {
        let mut actual = [0xabcdef01; 12];
        let mut out = abi::Pixels {
            data: unsafe { actual.as_mut_ptr().add(1) },
            len: 8,
            width: 3,
            height: 2,
            stride: 5,
            origin_x: 2,
            origin_y: 1,
        };
        assert_eq!(
            unsafe { masked(&apply_plan(opacity), &descriptors, &masks, 7, 4, &mut out) },
            abi::OK
        );
        let mut expected = [0xabcdef01; 12];
        for y in 0..2 {
            for x in 0..3 {
                expected[1 + y * 5 + x] = expected_apply(
                    sources[0][(y + 1) * 7 + x + 2],
                    opacity,
                    u32::from(coverage[(y + 1) * 9 + x + 2]),
                );
            }
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn uniform_masks_and_white_group_gates_retain_legacy_complete_pixels() {
    let mut seed = 0x77686974;
    let sources: Vec<Vec<u32>> = (0..3)
        .map(|_| (0..257).map(|_| random(&mut seed)).collect())
        .collect();
    let descriptors = layers(&sources);
    let mut legacy = vec![0; 257];
    let mut actual = vec![0; 257];
    for mode in 0..=abi::BLEND_MODE_MAX {
        for opacity in [0, 1, 127, 128, 254, 255] {
            let o = opacity as f32 / 255.;
            let old = [
                node(0, 0, 0, 0, 1.),
                node(0, 1, 0, 0, 1.),
                node(0, 2, 0, 0, 1.),
                node(2, 1, 2, 0, 1.),
                node(2, 0, 3, mode, o),
                node(3, 4, 0, 0, 1.),
            ];
            let plan = [
                node(0, 0, 0, 0, 1.),
                node(0, 1, 0, 0, 1.),
                node(0, 2, 0, 0, 1.),
                node(2, 1, 2, 0, 1.),
                node(6, 0, 0, 0, 0.),
                node(7, 3, 4, 0, o),
                node(2, 0, 5, mode, 1.),
                node(3, 6, 0, 0, 1.),
            ];
            assert_eq!(
                unsafe {
                    render(
                        old.as_ptr(),
                        old.len(),
                        descriptors.as_ptr(),
                        descriptors.len(),
                        257,
                        1,
                        &mut output(&mut legacy, 257, 1),
                    )
                },
                abi::OK
            );
            assert_eq!(
                unsafe {
                    masked(
                        &plan,
                        &descriptors,
                        &[constant(255)],
                        257,
                        1,
                        &mut output(&mut actual, 257, 1),
                    )
                },
                abi::OK
            );
            assert_eq!(actual, legacy, "isolated mode{mode}, opacity{opacity}");
        }
    }
    for value in 0..=255 {
        let data = vec![value as u8; 257];
        assert_eq!(
            unsafe {
                masked(
                    &apply_plan(179),
                    &descriptors,
                    &[constant(value)],
                    257,
                    1,
                    &mut output(&mut legacy, 257, 1),
                )
            },
            abi::OK
        );
        assert_eq!(
            unsafe {
                masked(
                    &apply_plan(179),
                    &descriptors,
                    &[plane(&data, 257)],
                    257,
                    1,
                    &mut output(&mut actual, 257, 1),
                )
            },
            abi::OK
        );
        assert_eq!(actual, legacy, "uniform{value}");
    }
    for opacity in 0..=255 {
        let old = [
            node(0, 0, 0, 0, 1.),
            node(0, 1, 0, 0, 1.),
            node(5, 0, 1, 0, opacity as f32 / 255.),
            node(3, 2, 0, 0, 1.),
        ];
        assert_eq!(
            unsafe {
                render(
                    old.as_ptr(),
                    old.len(),
                    descriptors.as_ptr(),
                    descriptors.len(),
                    257,
                    1,
                    &mut output(&mut legacy, 257, 1),
                )
            },
            abi::OK
        );
        assert_eq!(
            unsafe {
                masked(
                    &mix_plan(opacity),
                    &descriptors,
                    &[constant(255)],
                    257,
                    1,
                    &mut output(&mut actual, 257, 1),
                )
            },
            abi::OK
        );
        assert_eq!(actual, legacy, "white PassThrough gate opacity{opacity}");
    }
}

#[test]
fn white_layer_mask_after_filters_preserves_legacy_layer_opacity_pixels() {
    let mut seed = 0x66696c74;
    let sources = vec![(0..1024).map(|_| random(&mut seed)).collect()];
    let mut descriptors = layers(&sources);
    let mut legacy = vec![0; 1024];
    let mut actual = vec![0; 1024];
    for opacity in [0, 1, 127, 128, 254, 255] {
        for filter in 0..=4 {
            for amount in [0., 0.27, 1.] {
                let old = [
                    node(0, 0, 0, 0, 1.),
                    node(1, 0, 0, filter, amount),
                    node(3, 1, 0, 0, 1.),
                ];
                descriptors[0].opacity = opacity;
                assert_eq!(
                    unsafe {
                        render(
                            old.as_ptr(),
                            old.len(),
                            descriptors.as_ptr(),
                            1,
                            32,
                            32,
                            &mut output(&mut legacy, 32, 32),
                        )
                    },
                    abi::OK
                );
                let plan = [
                    node(0, 0, 0, 0, 1.),
                    node(1, 0, 0, filter, amount),
                    node(6, 0, 0, 0, 0.),
                    node(7, 1, 2, 0, opacity as f32 / 255.),
                    node(3, 3, 0, 0, 1.),
                ];
                descriptors[0].opacity = 255;
                assert_eq!(
                    unsafe {
                        masked(
                            &plan,
                            &descriptors,
                            &[constant(255)],
                            32,
                            32,
                            &mut output(&mut actual, 32, 32),
                        )
                    },
                    abi::OK
                );
                assert_eq!(
                    actual, legacy,
                    "filter{filter},amount{amount},opacity{opacity}"
                );
            }
        }
    }
}

#[test]
fn masked_entry_delegates_legacy_plans_without_altering_complete_outputs() {
    let mut seed = 0x6c656761;
    let sources = vec![
        (0..1024).map(|_| random(&mut seed)).collect(),
        (0..1024).map(|_| random(&mut seed)).collect(),
    ];
    let descriptors = layers(&sources);
    let mut legacy = vec![0; 1024];
    let mut actual = vec![0; 1024];
    let mut plans = vec![vec![]];
    for mode in 0..=abi::BLEND_MODE_MAX {
        plans.push(vec![
            node(4, 0, 0, 0, 0.),
            node(0, 0, 0, 0, 1.),
            node(1, 1, 0, 2, 0.17),
            node(0, 1, 0, 0, 1.),
            node(2, 2, 3, mode, 0.71),
            node(5, 0, 4, 0, 0.83),
            node(3, 5, 0, 0, 1.),
        ]);
    }
    for plan in plans {
        assert_eq!(
            unsafe {
                render(
                    plan.as_ptr(),
                    plan.len(),
                    descriptors.as_ptr(),
                    descriptors.len(),
                    32,
                    32,
                    &mut output(&mut legacy, 32, 32),
                )
            },
            abi::OK
        );
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &[],
                    32,
                    32,
                    &mut output(&mut actual, 32, 32),
                )
            },
            abi::OK
        );
        assert_eq!(actual, legacy);
        // Even unused, valid constant descriptors do not select another loop.
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &[constant(83)],
                    32,
                    32,
                    &mut output(&mut actual, 32, 32),
                )
            },
            abi::OK
        );
        assert_eq!(actual, legacy);
    }
}

#[test]
fn mask_types_and_all_declared_instructions_are_validated_before_writes() {
    let sources = vec![vec![0xff446688]];
    let descriptors = layers(&sources);
    let good = apply_plan(255).to_vec();
    let mut bad = vec![];
    for index in 0..good.len() {
        let mut plan = good.clone();
        plan[index].amount = f32::NAN;
        bad.push(plan);
    }
    for change in [
        node(6, 1, 0, 0, 0.),
        node(6, 0, 1, 0, 0.),
        node(6, 0, 0, 1, 0.),
        node(6, 0, 0, 0, 1.),
    ] {
        let mut plan = good.clone();
        plan[1] = change;
        bad.push(plan);
    }
    for change in [
        node(7, 1, 0, 0, 1.),
        node(7, 0, 0, 0, 1.),
        node(7, 0, 1, 1, 1.),
        node(7, 0, 1, 0, -0.01),
        node(7, 0, 1, 0, 1.01),
        node(7, 0, 3, 0, 1.),
        node(8, 0, 0, 0, 1.),
        node(8, 0, 1, 1, 1.),
        node(8, 0, 0, 3, 1.),
    ] {
        let mut plan = good.clone();
        plan[2] = change;
        bad.push(plan);
    }
    for kind in [1, 2, 3, 5] {
        let mut plan = good.clone();
        plan[2] = node(kind, 1, 0, 0, 0.5);
        bad.push(plan);
    }
    bad.push(vec![node(6, 0, 0, 0, 0.)]);
    let mut disconnected = good.clone();
    disconnected.push(node(1, 1, 0, 0, 1.));
    disconnected.push(node(3, 2, 0, 0, 1.));
    bad.push(disconnected);
    for plan in bad {
        let mut pixels = [0x12345678];
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &[constant(255)],
                    1,
                    1,
                    &mut output(&mut pixels, 1, 1),
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, [0x12345678]);
    }
    let mut full = good;
    while full.len() < 64 {
        full.push(node(3, full.len() as u32 - 1, 0, 0, 1.));
    }
    let mut pixels = [0];
    assert_eq!(
        unsafe {
            masked(
                &full,
                &descriptors,
                &[constant(255)],
                1,
                1,
                &mut output(&mut pixels, 1, 1),
            )
        },
        abi::OK
    );
    assert_eq!(pixels, [0xff446688]);
    full.push(node(3, 63, 0, 0, 1.));
    pixels = [0x12345678];
    assert_eq!(
        unsafe {
            masked(
                &full,
                &descriptors,
                &[constant(255)],
                1,
                1,
                &mut output(&mut pixels, 1, 1),
            )
        },
        abi::ERROR
    );
    assert_eq!(pixels, [0x12345678]);
}

#[test]
fn malformed_mask_planes_output_geometry_aliases_and_alignment_reject_before_writes() {
    let sources = vec![vec![0xff123456; 4]];
    let descriptors = layers(&sources);
    let storage = [0x807f0100u32; 4];
    let raw = unsafe { std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), 16) };
    let good_mask = plane(raw, 2);
    let plan = apply_plan(128);
    let mut bad_masks = vec![];
    let mut m = constant(255);
    m.len = 1;
    bad_masks.push(m);
    let mut m = constant(255);
    m.stride = 1;
    bad_masks.push(m);
    bad_masks.push(constant(256));
    let mut m = good_mask;
    m.len = 3;
    bad_masks.push(m);
    let mut m = good_mask;
    m.stride = 1;
    bad_masks.push(m);
    let mut m = good_mask;
    m.constant = 1;
    bad_masks.push(m);
    for mask in bad_masks {
        let mut pixels = [0x76543210; 4];
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &[mask],
                    2,
                    2,
                    &mut output(&mut pixels, 2, 2),
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, [0x76543210; 4]);
        // Invalid unreferenced descriptors still reject a legacy flat stack.
        assert_eq!(
            unsafe {
                masked(
                    &[],
                    &descriptors,
                    &[mask],
                    2,
                    2,
                    &mut output(&mut pixels, 2, 2),
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, [0x76543210; 4]);
    }
    for changed in 0..8 {
        let mut pixels = [0x76543210; 4];
        let mut out = output(&mut pixels, 2, 2);
        match changed {
            0 => out.width = 0,
            1 => out.height = 0,
            2 => out.stride = 1,
            3 => out.len = 3,
            4 => out.origin_x = 1,
            5 => out.origin_y = u32::MAX,
            6 => out.data = std::ptr::null_mut(),
            _ => out.data = 1usize as *mut u32,
        }
        assert_eq!(
            unsafe { masked(&plan, &descriptors, &[good_mask], 2, 2, &mut out) },
            abi::ERROR
        );
        assert_eq!(pixels, [0x76543210; 4]);
    }
    let masks = [good_mask];
    // Raw-pointer aliases are never turned into mutable slices on rejection.
    for data in [
        sources[0].as_ptr(),
        storage.as_ptr(),
        plan.as_ptr().cast(),
        descriptors.as_ptr().cast(),
        masks.as_ptr().cast(),
    ] {
        let mut out = abi::Pixels {
            data: data as *mut u32,
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe { masked(&plan, &descriptors, &masks, 2, 2, &mut out) },
            abi::ERROR
        );
    }
    let mut pixels = [0x76543210; 4];
    let mut out = output(&mut pixels, 2, 2);
    out.data = (&mut out as *mut abi::Pixels).cast();
    assert_eq!(
        unsafe { masked(&plan, &descriptors, &masks, 2, 2, &mut out) },
        abi::ERROR
    );
    assert_eq!(pixels, [0x76543210; 4]);
    let mut out = output(&mut pixels, 2, 2);
    for bad in 0..4 {
        assert_eq!(
            unsafe {
                masks::render_masked(
                    if bad == 0 {
                        1usize as *const _
                    } else {
                        plan.as_ptr()
                    },
                    plan.len(),
                    if bad == 1 {
                        1usize as *const _
                    } else {
                        descriptors.as_ptr()
                    },
                    descriptors.len(),
                    if bad == 2 {
                        1usize as *const _
                    } else {
                        masks.as_ptr()
                    },
                    masks.len(),
                    2,
                    2,
                    if bad == 3 { 1usize as *mut _ } else { &mut out },
                )
            },
            abi::ERROR
        );
    }
    assert_eq!(pixels, [0x76543210; 4]);
    let too_many = [constant(255); 33];
    assert_eq!(
        unsafe { masked(&plan, &descriptors, &too_many, 2, 2, &mut out) },
        abi::ERROR
    );
    assert_eq!(pixels, [0x76543210; 4]);
    for changed in 0..6 {
        let mut bad_layers = descriptors.clone();
        match changed {
            0 => bad_layers[0].len = 3,
            1 => bad_layers[0].opacity = 256,
            2 => bad_layers[0].visible = 2,
            3 => bad_layers[0].mode = abi::BLEND_MODE_MAX + 1,
            4 => bad_layers[0].data = std::ptr::null(),
            _ => bad_layers[0].data = 1usize as *const u32,
        }
        assert_eq!(
            unsafe { masked(&plan, &bad_layers, &masks, 2, 2, &mut out) },
            abi::ERROR
        );
        assert_eq!(pixels, [0x76543210; 4]);
    }
    assert_eq!(
        unsafe {
            masks::render_masked(
                plan.as_ptr(),
                plan.len(),
                descriptors.as_ptr(),
                descriptors.len(),
                std::ptr::null(),
                1,
                2,
                2,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(
        unsafe {
            masks::render_masked(
                plan.as_ptr(),
                plan.len(),
                descriptors.as_ptr(),
                0,
                masks.as_ptr(),
                1,
                2,
                2,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(
        unsafe {
            masks::render_masked(
                plan.as_ptr(),
                plan.len(),
                descriptors.as_ptr(),
                descriptors.len(),
                masks.as_ptr(),
                1,
                4097,
                2,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(pixels, [0x76543210; 4]);
}

#[test]
#[ignore = "Release masked-region diagnostic; no CI timing threshold"]
fn masked_region_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Run this diagnostic with --release"
    );
    const SIDE: usize = 1024;
    let mut seed = 0x4138636f;
    let sources: Vec<Vec<u32>> = (0..2)
        .map(|_| (0..SIDE * SIDE).map(|_| random(&mut seed)).collect())
        .collect();
    let descriptors = layers(&sources);
    let coverage: Vec<u8> = (0..SIDE * SIDE)
        .map(|_| (random(&mut seed) >> 24) as u8)
        .collect();
    let masks = [plane(&coverage, SIDE as u32)];
    let mut pixels = vec![0; SIDE * SIDE];
    for opacity in [0, 128, 255] {
        let plan = mix_plan(opacity);
        let mut out = output(&mut pixels, SIDE as u32, SIDE as u32);
        let start = std::time::Instant::now();
        assert_eq!(
            unsafe {
                masked(
                    &plan,
                    &descriptors,
                    &masks,
                    SIDE as u32,
                    SIDE as u32,
                    &mut out,
                )
            },
            abi::OK
        );
        let elapsed = start.elapsed();
        for (i, pixel) in pixels.iter().enumerate() {
            assert_eq!(
                *pixel,
                expected_mix(
                    source(sources[0][i]),
                    source(sources[1][i]),
                    opacity,
                    u32::from(coverage[i])
                )
            );
        }
        println!("masked_region_cost 1024x1024, two full-range ARGB sources, one full-range A8 plane, five instructions, opacity{opacity}: {:.3}ms. Native validation/composition included; allocation, source generation, full reference check, Session/history/journal, encoding, GUI and physical pen input excluded. No CI threshold or peak-memory claim.",elapsed.as_secs_f64()*1000.);
    }
}
