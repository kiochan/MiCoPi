//! Independent exact references for the capability-gated premultiplied Mix node.
use super::*;

fn rounded(numerator: u64, denominator: u64) -> u32 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    (quotient + u64::from(remainder * 2 >= denominator)) as u32
}

fn reference(before: u32, after: u32, amount: u32) -> u32 {
    match amount {
        0 => return before,
        255 => return after,
        _ => {}
    }
    let weights = [255 - amount, amount].map(u64::from);
    let colors = [before, after];
    let coverage: [u64; 2] = std::array::from_fn(|i| u64::from(colors[i] >> 24) * weights[i]);
    let denominator = coverage.iter().sum::<u64>();
    let alpha = rounded(denominator, 255);
    if alpha == 0 {
        return 0;
    }
    [16, 8, 0].into_iter().fold(alpha << 24, |pixel, shift| {
        let numerator = (0..2)
            .map(|i| u64::from((colors[i] >> shift) & 255) * coverage[i])
            .sum();
        pixel | (rounded(numerator, denominator) << shift)
    })
}

fn next(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
    *state
}

fn source_color(pixel: u32) -> u32 {
    if pixel >> 24 == 0 {
        0
    } else {
        pixel
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

fn plan(amount: f32) -> [abi::CompositeNode; 4] {
    [
        node(0, 0, 0, 0, 1.),
        node(0, 1, 0, 0, 1.),
        node(5, 0, 1, 0, amount),
        node(3, 2, 0, 0, 1.),
    ]
}

fn layers(sources: &[Vec<u32>; 2]) -> [abi::CompositeLayer; 2] {
    std::array::from_fn(|i| abi::CompositeLayer {
        data: sources[i].as_ptr(),
        len: sources[i].len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    })
}

#[test]
fn mix_matches_exact_reference_for_every_alpha_pair_and_boundary_weights() {
    for before_alpha in 0..=255 {
        for after_alpha in 0..=255 {
            let before = (before_alpha << 24) | 0x125ca7;
            let after = (after_alpha << 24) | 0xd3300e;
            for amount in [0, 1, 127, 128, 254, 255] {
                assert_eq!(
                    mix(before, after, amount),
                    reference(before, after, amount),
                    "alpha {before_alpha}/{after_alpha}, weight {amount}"
                );
            }
        }
    }
    // Preserve exact endpoint words, including hidden RGB. Interior zero-alpha
    // results are canonical, and RGB uses the unrounded weighted alpha.
    assert_eq!(mix(0x00112233, 0x00abcdef, 0), 0x00112233);
    assert_eq!(mix(0x00112233, 0x00abcdef, 255), 0x00abcdef);
    assert_eq!(mix(0x00112233, 0x00abcdef, 128), 0);
    assert_eq!(mix(0x00112233, 0x01abcdef, 127), 0);
    assert_eq!(mix(0x00112233, 0x01abcdef, 128), 0x01abcdef);
    assert_eq!(mix(0x01000000, 0x02ffffff, 128), 0x02aaaaaa);
    assert_eq!(mix(0x01000000, 0x02010101, 85), 0x01010101);
}

#[test]
fn mix_matches_exact_random_colors_and_all_weights_without_endpoint_drift() {
    let mut state = 0x6d697821;
    for amount in 0..=255 {
        assert_eq!((amount as f32 / 255. * 255.).round() as u32, amount);
        for _ in 0..128 {
            let before = next(&mut state);
            let after = next(&mut state);
            assert_eq!(mix(before, after, amount), reference(before, after, amount));
            assert_eq!(mix(before, after, amount), mix(after, before, 255 - amount));
        }
    }
}

#[test]
fn mix_dirty_regions_use_global_coordinates_and_keep_padding_and_guards() {
    let mut state = 0x73747264;
    let sources = std::array::from_fn(|_| (0..28).map(|_| next(&mut state)).collect());
    let descriptors = layers(&sources);
    for amount in 0..=255 {
        let instructions = plan(amount as f32 / 255.);
        let mut pixels = [0x12345678; 12];
        let mut output = abi::Pixels {
            data: unsafe { pixels.as_mut_ptr().add(1) },
            len: 8,
            width: 3,
            height: 2,
            stride: 5,
            origin_x: 2,
            origin_y: 1,
        };
        assert_eq!(
            unsafe {
                render(
                    instructions.as_ptr(),
                    4,
                    descriptors.as_ptr(),
                    2,
                    7,
                    4,
                    &mut output,
                )
            },
            abi::OK
        );
        let mut expected = [0x12345678; 12];
        for y in 0..2 {
            for x in 0..3 {
                let source = (y + 1) * 7 + x + 2;
                expected[1 + y * 5 + x] = reference(
                    source_color(sources[0][source]),
                    source_color(sources[1][source]),
                    amount,
                );
            }
        }
        assert_eq!(pixels, expected, "weight {amount}");
    }
}

#[test]
fn mix_rejects_invalid_even_unreachable_instructions_and_aliases_before_writing() {
    let sources = [vec![0x01234567; 4], vec![0xfedcba98; 4]];
    let descriptors = layers(&sources);
    let mut pixels = [0x12345678; 4];
    let mut output = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: 4,
        width: 2,
        height: 2,
        stride: 2,
        origin_x: 0,
        origin_y: 0,
    };
    for invalid in [
        node(5, 2, 1, 0, 0.5),
        node(5, 2, 1, 0, 0.),
        node(5, 0, 3, 0, 0.5),
        node(5, 0, 3, 0, 1.),
        node(5, u32::MAX, 1, 0, 0.5),
        node(5, 0, 1, 1, 0.5),
        node(5, 0, 1, 0, -f32::EPSILON),
        node(5, 0, 1, 0, 1. + f32::EPSILON),
        node(5, 0, 1, 0, f32::NAN),
        node(5, 0, 1, 0, f32::INFINITY),
        node(5, 0, 1, 0, f32::NEG_INFINITY),
    ] {
        let mut instructions = plan(0.5);
        instructions[2] = invalid;
        assert_eq!(
            unsafe {
                render(
                    instructions.as_ptr(),
                    4,
                    descriptors.as_ptr(),
                    2,
                    2,
                    2,
                    &mut output,
                )
            },
            abi::ERROR
        );
        assert_eq!(pixels, [0x12345678; 4]);
    }
    let unused = [
        node(0, 0, 0, 0, 1.),
        node(3, 0, 0, 0, 1.),
        node(5, 0, 1, 1, 0.5),
    ];
    assert_eq!(
        unsafe {
            render(
                unused.as_ptr(),
                3,
                descriptors.as_ptr(),
                2,
                2,
                2,
                &mut output,
            )
        },
        abi::ERROR
    );
    assert_eq!(pixels, [0x12345678; 4]);
    let instructions = plan(0.5);
    output.data = sources[0].as_ptr().cast_mut();
    assert_eq!(
        unsafe {
            render(
                instructions.as_ptr(),
                4,
                descriptors.as_ptr(),
                2,
                2,
                2,
                &mut output,
            )
        },
        abi::ERROR
    );
    assert_eq!(sources[0], [0x01234567; 4]);
    output.data = instructions.as_ptr().cast_mut().cast();
    assert_eq!(
        unsafe {
            render(
                instructions.as_ptr(),
                4,
                descriptors.as_ptr(),
                2,
                2,
                2,
                &mut output,
            )
        },
        abi::ERROR
    );
    assert_eq!(instructions[2].kind, 5);
    assert_eq!(instructions[2].amount, 0.5);
}

#[test]
fn mix_uses_the_existing_sixty_four_slot_plan_budget() {
    let sources = [vec![0x7f125ca7], vec![0xedd3300e]];
    let descriptors = layers(&sources);
    let mut instructions = plan(0.5)[..2].to_vec();
    let mut expected = sources[1][0];
    for slot in 2..63 {
        instructions.push(node(5, slot - 1, 0, 0, 128. / 255.));
        expected = reference(expected, sources[0][0], 128);
    }
    instructions.push(node(3, 62, 0, 0, 1.));
    let mut pixel = 0x12345678;
    let mut output = abi::Pixels {
        data: &mut pixel,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    assert_eq!(instructions.len(), 64);
    assert_eq!(
        unsafe {
            render(
                instructions.as_ptr(),
                64,
                descriptors.as_ptr(),
                2,
                1,
                1,
                &mut output,
            )
        },
        abi::OK
    );
    assert_eq!(pixel, expected);
    instructions.push(node(3, 63, 0, 0, 1.));
    pixel = 0x12345678;
    assert_eq!(
        unsafe {
            render(
                instructions.as_ptr(),
                65,
                descriptors.as_ptr(),
                2,
                1,
                1,
                &mut output,
            )
        },
        abi::ERROR
    );
    assert_eq!(pixel, 0x12345678);
}

#[test]
#[ignore = "Release-only diagnostic; no CI timing threshold"]
fn mix_region_cost() {
    use std::time::Instant;
    const SIDE: u32 = 1024;
    let mut state = 0x6d697863;
    let sources = std::array::from_fn(|_| (0..SIDE * SIDE).map(|_| next(&mut state)).collect());
    let descriptors = layers(&sources);
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
    let invoke = std::hint::black_box(
        render
            as unsafe extern "C" fn(
                *const abi::CompositeNode,
                usize,
                *const abi::CompositeLayer,
                usize,
                u32,
                u32,
                *mut abi::Pixels,
            ) -> i32,
    );
    for amount in [0, 128, 255] {
        let instructions = plan(amount as f32 / 255.);
        let start = Instant::now();
        let status = unsafe {
            invoke(
                instructions.as_ptr(),
                4,
                descriptors.as_ptr(),
                2,
                SIDE,
                SIDE,
                &mut output,
            )
        };
        let elapsed = start.elapsed();
        assert_eq!(status, abi::OK);
        for index in [0, 1, 127, 1023, 1024, 524287, 1048575] {
            assert_eq!(
                pixels[index],
                reference(
                    source_color(sources[0][index]),
                    source_color(sources[1][index]),
                    amount
                )
            );
        }
        eprintln!("mix_region_cost weight={amount} region={SIDE}x{SIDE} cold_single_render_ms={:.3}; two prepared full-range ARGB sources, four instructions; validation+native composition included; allocations, fixture generation and reference checks excluded; no Session/history/journal, encoding, I/O or pen-latency measurement",elapsed.as_secs_f64()*1000.);
    }
}
