use super::*;

fn settings(radius: u32, amount: u32) -> abi::MaskEffectRequest {
    abi::MaskEffectRequest {
        flags: 0,
        density: amount,
        feather_radius: radius,
        reserved: 0,
    }
}
fn plane(bytes: &[u8], stride: usize) -> abi::CompositeMask {
    abi::CompositeMask {
        data: bytes.as_ptr(),
        len: bytes.len(),
        stride: stride as u32,
        constant: 0,
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
fn output(bytes: &mut [u8], roi: [usize; 4], stride: usize) -> abi::A8Pixels {
    abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        width: roi[2] as u32,
        height: roi[3] as u32,
        stride: stride as u32,
        origin_x: roi[0] as u32,
        origin_y: roi[1] as u32,
    }
}
fn run(
    state: &mut State,
    options: &abi::MaskEffectRequest,
    source: &abi::CompositeMask,
    size: [usize; 2],
    roi: [usize; 4],
    stride: usize,
) -> Vec<u8> {
    let mut bytes = vec![43; stride * roi[3]];
    let mut out = output(&mut bytes, roi, stride);
    assert_eq!(
        unsafe {
            region(
                (state as *mut State).cast(),
                options,
                source,
                size[0] as u32,
                size[1] as u32,
                &mut out,
            )
        },
        abi::OK
    );
    for y in 0..roi[3] {
        assert!(bytes[y * stride + roi[2]..(y + 1) * stride]
            .iter()
            .all(|v| *v == 43));
    }
    bytes
}
// Independent direct convolution: no rolling sums or cropped-halo implementation.
fn reference(bytes: &[u8], width: usize, height: usize, radius: usize, amount: u32) -> Vec<u8> {
    let mut value = bytes.to_vec();
    for r in [(radius + 2) / 3, (radius + 1) / 3, radius / 3] {
        if r == 0 {
            continue;
        }
        for horizontal in [true, false] {
            let old = value.clone();
            for y in 0..height {
                for x in 0..width {
                    let mut samples = 0u64;
                    for offset in -(r as isize)..=r as isize {
                        let xx = if horizontal {
                            (x as isize + offset).clamp(0, width as isize - 1) as usize
                        } else {
                            x
                        };
                        let yy = if horizontal {
                            y
                        } else {
                            (y as isize + offset).clamp(0, height as isize - 1) as usize
                        };
                        samples += old[yy * width + xx] as u64;
                    }
                    value[y * width + x] =
                        ((samples * 2 + (2 * r + 1) as u64) / (2 * (2 * r + 1)) as u64) as u8;
                }
            }
        }
    }
    value
        .iter()
        .map(|value| (255 - ((255 - *value as u32) * amount * 2 + 255) / (255 * 2)) as u8)
        .collect()
}

#[test]
fn every_byte_density_pair_and_raw_inversion_are_exact_and_scratch_free() {
    let source: Vec<_> = (0..=255).map(|v| v as u8).collect();
    let mut state = State::default();
    for amount in 0..=255 {
        let actual = run(
            &mut state,
            &settings(0, amount),
            &plane(&source, 256),
            [256, 1],
            [0, 0, 256, 1],
            259,
        );
        assert_eq!(&actual[..256], reference(&source, 256, 1, 0, amount));
    }
    let mut invert = settings(0, 255);
    invert.flags = abi::MASK_EFFECT_INVERT_RAW;
    let actual = run(
        &mut state,
        &invert,
        &plane(&source, 256),
        [256, 1],
        [0, 0, 256, 1],
        257,
    );
    assert_eq!(
        &actual[..256],
        source.iter().map(|v| 255 - v).collect::<Vec<_>>()
    );
    assert_eq!((state.first.capacity(), state.second.capacity()), (0, 0));
}

#[test]
fn all_small_binary_masks_match_independent_passes_at_every_roi_and_border() {
    for (width, height) in [(1, 1), (1, 3), (3, 1), (2, 2), (3, 2), (3, 3)] {
        for bits in 0..(1usize << (width * height)) {
            let source: Vec<_> = (0..width * height)
                .map(|i| if bits & (1 << i) == 0 { 0 } else { 255 })
                .collect();
            let stride = width + 2;
            let mut padded = vec![19; stride * height];
            for y in 0..height {
                padded[y * stride..y * stride + width]
                    .copy_from_slice(&source[y * width..(y + 1) * width]);
            }
            let mut state = State::default();
            for radius in [0, 1, 2, 3, 5] {
                let expected = reference(&source, width, height, radius, 173);
                for y in 0..height {
                    for x in 0..width {
                        let roi = [x, y, width - x, height - y];
                        let actual = run(
                            &mut state,
                            &settings(radius as u32, 173),
                            &plane(&padded, stride),
                            [width, height],
                            roi,
                            roi[2] + 1,
                        );
                        for yy in 0..roi[3] {
                            assert_eq!(
                                &actual[yy * (roi[2] + 1)..yy * (roi[2] + 1) + roi[2]],
                                &expected[(y + yy) * width + x..(y + yy + 1) * width],
                                "{width}x{height},bits={bits},radius={radius},roi={roi:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn uniform_planes_clamp_edges_and_maximum_radius_preserve_white_without_scratch() {
    let mut state = State::default();
    for value in [0, 1, 127, 128, 254, 255] {
        for amount in [0, 1, 128, 254, 255] {
            let actual = run(
                &mut state,
                &settings(256, amount),
                &uniform(value),
                [4096, 4096],
                [4090, 4091, 6, 5],
                8,
            );
            let expected = reference(&[value as u8], 1, 1, 256, amount)[0];
            for y in 0..5 {
                assert!(actual[y * 8..y * 8 + 6].iter().all(|v| *v == expected));
            }
        }
    }
    assert_eq!((state.first.capacity(), state.second.capacity()), (0, 0));
    let white = vec![255; 9];
    assert_eq!(
        run(
            &mut state,
            &settings(256, 255),
            &plane(&white, 3),
            [3, 3],
            [0, 0, 3, 3],
            3
        ),
        white
    );
}

#[test]
fn maximum_radius_dirty_halos_equal_full_frame_and_warmed_scratch_never_grows() {
    let small: Vec<_> = (0..35).map(|i| ((i * 79 + 17) % 256) as u8).collect();
    let expected = reference(&small, 7, 5, 256, 137);
    let mut small_state = State::default();
    assert_eq!(
        run(
            &mut small_state,
            &settings(256, 137),
            &plane(&small, 7),
            [7, 5],
            [0, 0, 7, 5],
            7
        ),
        expected
    );
    let (width, height) = (613, 519);
    let source: Vec<_> = (0..width * height)
        .map(|i| ((i * 137 + i / width * 23) % 256) as u8)
        .collect();
    let mut state = State::default();
    let full = run(
        &mut state,
        &settings(256, 137),
        &plane(&source, width),
        [width, height],
        [0, 0, width, height],
        width,
    );
    let capacity = (state.first.capacity(), state.second.capacity());
    let pointers = (state.first.as_ptr(), state.second.as_ptr());
    for roi in [
        [0, 0, 1, 1],
        [308, 259, 1, 1],
        [612, 518, 1, 1],
        [299, 11, 17, 23],
    ] {
        let actual = run(
            &mut state,
            &settings(256, 137),
            &plane(&source, width),
            [width, height],
            roi,
            roi[2] + 3,
        );
        for y in 0..roi[3] {
            assert_eq!(
                &actual[y * (roi[2] + 3)..y * (roi[2] + 3) + roi[2]],
                &full[(roi[1] + y) * width + roi[0]..(roi[1] + y) * width + roi[0] + roi[2]]
            );
        }
        assert_eq!((state.first.capacity(), state.second.capacity()), capacity);
        assert_eq!((state.first.as_ptr(), state.second.as_ptr()), pointers);
    }
    let mut local = State::default();
    run(
        &mut local,
        &settings(7, 211),
        &plane(&source, width),
        [width, height],
        [300, 250, 2, 3],
        2,
    );
    assert_eq!(local.first.len(), (2 + 14) * (3 + 14));
    assert_eq!(local.second.len(), local.first.len());
}

#[test]
fn malformed_parameters_descriptors_and_aliases_reject_before_output_changes() {
    let source = vec![31; 16];
    let mut state = State::default();
    let valid = settings(2, 255);
    let mut cases = vec![settings(257, 255), settings(2, 256)];
    for (flags, radius, amount, reserved) in [
        (2, 0, 255, 0),
        (1, 1, 255, 0),
        (1, 0, 254, 0),
        (0, 2, 255, 1),
    ] {
        cases.push(abi::MaskEffectRequest {
            flags,
            feather_radius: radius,
            density: amount,
            reserved,
        });
    }
    for options in cases {
        let mut bytes = vec![77; 4];
        let mut out = output(&mut bytes, [1, 1, 2, 2], 2);
        assert_eq!(
            unsafe {
                region(
                    (&mut state as *mut State).cast(),
                    &options,
                    &plane(&source, 4),
                    4,
                    4,
                    &mut out,
                )
            },
            abi::ERROR
        );
        assert_eq!(bytes, vec![77; 4]);
    }
    for bad in [
        uniform(256),
        abi::CompositeMask {
            len: 1,
            ..uniform(2)
        },
        abi::CompositeMask {
            constant: 1,
            ..plane(&source, 4)
        },
        abi::CompositeMask {
            len: 12,
            ..plane(&source, 4)
        },
        plane(&source, 3),
    ] {
        let mut bytes = vec![77; 4];
        let mut out = output(&mut bytes, [1, 1, 2, 2], 2);
        assert_eq!(
            unsafe {
                region(
                    (&mut state as *mut State).cast(),
                    &valid,
                    &bad,
                    4,
                    4,
                    &mut out,
                )
            },
            abi::ERROR
        );
        assert_eq!(bytes, vec![77; 4]);
    }
    for kind in 0..7 {
        let mut bytes = vec![77; 16];
        let mut out = output(&mut bytes, [1, 1, 2, 2], 4);
        match kind {
            0 => out.origin_x = 4,
            1 => out.origin_y = u32::MAX,
            2 => out.width = 0,
            3 => out.stride = 1,
            4 => out.len = 5,
            5 => out.data = source.as_ptr() as *mut u8,
            _ => out.data = std::ptr::null_mut(),
        }
        assert_eq!(
            unsafe {
                region(
                    (&mut state as *mut State).cast(),
                    &valid,
                    &plane(&source, 4),
                    4,
                    4,
                    &mut out,
                )
            },
            abi::ERROR
        );
        assert_eq!(bytes, vec![77; 16]);
        assert_eq!(source, vec![31; 16]);
    }
    run(
        &mut state,
        &valid,
        &plane(&source, 4),
        [4, 4],
        [0, 0, 4, 4],
        4,
    );
    let mut out = abi::A8Pixels {
        data: state.first.as_mut_ptr(),
        len: 16,
        width: 4,
        height: 4,
        stride: 4,
        origin_x: 0,
        origin_y: 0,
    };
    let saved = state.first.clone();
    assert_eq!(
        unsafe {
            region(
                (&mut state as *mut State).cast(),
                &valid,
                &plane(&source, 4),
                4,
                4,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(state.first, saved);
    let aliased = plane(&state.first, 4);
    let mut bytes = vec![77; 16];
    let mut out = output(&mut bytes, [0, 0, 4, 4], 4);
    assert_eq!(
        unsafe {
            region(
                (&mut state as *mut State).cast(),
                &valid,
                &aliased,
                4,
                4,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(bytes, vec![77; 16]);
}

#[test]
fn cache_local_columns_match_independent_wide_narrow_nonsquare_strided_and_roi_references() {
    for (width, height) in [
        (4096, 1),
        (1, 4096),
        (777, 7),
        (7, 777),
        (513, 65),
        (65, 513),
    ] {
        let source: Vec<_> = (0..width * height)
            .map(|index| ((index * 17 + index / width * 73) & 255) as u8)
            .collect();
        let mut padded = vec![53; (width + 3) * height];
        for y in 0..height {
            padded[y * (width + 3)..y * (width + 3) + width]
                .copy_from_slice(&source[y * width..(y + 1) * width]);
        }
        for radius in [1, 8, 256] {
            let expected = reference(&source, width, height, radius, 203);
            let mut state = State::default();
            assert_eq!(
                unsafe {
                    reserve(
                        (&mut state as *mut State).cast(),
                        width as u32,
                        height as u32,
                        radius as u32,
                    )
                },
                abi::OK
            );
            let allocations = [
                (state.first.as_ptr() as usize, state.first.capacity()),
                (state.second.as_ptr() as usize, state.second.capacity()),
                (state.columns.as_ptr() as usize, state.columns.capacity()),
            ];
            for roi in [
                [0, 0, width, height],
                [0, 0, width.min(17), height.min(13)],
                [
                    width / 2,
                    height / 2,
                    (width - width / 2).min(19),
                    (height - height / 2).min(23),
                ],
                [width - 1, height - 1, 1, 1],
            ] {
                let actual = run(
                    &mut state,
                    &settings(radius as u32, 203),
                    &plane(&padded, width + 3),
                    [width, height],
                    roi,
                    roi[2] + 5,
                );
                for y in 0..roi[3] {
                    assert_eq!(
                        &actual[y * (roi[2] + 5)..y * (roi[2] + 5) + roi[2]],
                        &expected
                            [(roi[1] + y) * width + roi[0]..(roi[1] + y) * width + roi[0] + roi[2]],
                        "{width}x{height},R={radius},roi={roi:?}"
                    );
                }
                assert_eq!(
                    [
                        (state.first.as_ptr() as usize, state.first.capacity()),
                        (state.second.as_ptr() as usize, state.second.capacity()),
                        (state.columns.as_ptr() as usize, state.columns.capacity())
                    ],
                    allocations
                );
            }
            assert_eq!(state.columns.capacity(), width);
        }
    }
}

#[test]
fn column_scratch_aliases_reject_before_writing_source_output_or_scratch() {
    let source = vec![73; 16];
    let mut state = State::default();
    run(
        &mut state,
        &settings(2, 203),
        &plane(&source, 4),
        [4, 4],
        [0, 0, 4, 4],
        4,
    );
    let saved = state.columns.clone();
    let mut out = abi::A8Pixels {
        data: state.columns.as_mut_ptr().cast(),
        len: 16,
        stride: 4,
        origin_x: 0,
        origin_y: 0,
        width: 4,
        height: 4,
    };
    assert_eq!(
        unsafe {
            region(
                (&mut state as *mut State).cast(),
                &settings(2, 203),
                &plane(&source, 4),
                4,
                4,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(state.columns, saved);
    let alias = abi::CompositeMask {
        data: state.columns.as_ptr().cast(),
        len: 16,
        stride: 4,
        constant: 0,
    };
    let mut bytes = vec![19; 16];
    let mut out = output(&mut bytes, [0, 0, 4, 4], 4);
    assert_eq!(
        unsafe {
            region(
                (&mut state as *mut State).cast(),
                &settings(2, 203),
                &alias,
                4,
                4,
                &mut out,
            )
        },
        abi::ERROR
    );
    assert_eq!(bytes, vec![19; 16]);
    assert_eq!(state.columns, saved);
    assert_eq!(source, vec![73; 16]);
}
