use super::*;

enum Request {
    Reframe(abi::A8Reframe),
    Geometry(abi::A8Geometry),
    Resize(abi::Resample),
}
impl Request {
    fn valid(kind: usize) -> Self {
        match kind {
            0 => Self::Reframe(abi::A8Reframe {
                x: 0,
                y: 0,
                fill: 0,
                reserved: 0,
            }),
            1 => Self::Geometry(abi::A8Geometry {
                geometry: abi::Geometry {
                    kind: 3,
                    x: 0,
                    y: 0,
                },
                fill: 0,
            }),
            _ => Self::Resize(abi::Resample { method: 1 }),
        }
    }
    fn pointer(&self) -> *const u8 {
        match self {
            Self::Reframe(value) => (value as *const abi::A8Reframe).cast(),
            Self::Geometry(value) => (value as *const abi::A8Geometry).cast(),
            Self::Resize(value) => (value as *const abi::Resample).cast(),
        }
    }
    unsafe fn invoke(
        &self,
        request: *const u8,
        source: *const abi::CompositeMask,
        sw: u32,
        sh: u32,
        output: *mut abi::A8Pixels,
    ) -> i32 {
        let api = &*micopi_a8_sampler_v1();
        match self {
            Self::Reframe(_) => (api.reframe.unwrap())(request.cast(), source, sw, sh, output),
            Self::Geometry(_) => (api.geometry.unwrap())(request.cast(), source, sw, sh, output),
            Self::Resize(_) => (api.resample.unwrap())(request.cast(), source, sw, sh, output),
        }
    }
    fn call(
        &self,
        source: &abi::CompositeMask,
        sw: u32,
        sh: u32,
        output: &mut abi::A8Pixels,
    ) -> i32 {
        unsafe { self.invoke(self.pointer(), source, sw, sh, output) }
    }
}
fn source(bytes: &[u8], stride: usize) -> abi::CompositeMask {
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
fn output(bytes: &mut [u8], width: usize, height: usize, stride: usize) -> abi::A8Pixels {
    abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        width: width as u32,
        height: height as u32,
        stride: stride as u32,
        origin_x: 0,
        origin_y: 0,
    }
}
fn packed(bytes: &[u8], width: usize, height: usize, stride: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(width * height);
    for y in 0..height {
        result.extend_from_slice(&bytes[y * stride..y * stride + width]);
        assert!(bytes[y * stride + width..(y + 1) * stride]
            .iter()
            .all(|value| *value == 0xa5));
    }
    result
}
fn reframe_reference(
    bytes: &[u8],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
    x: i32,
    y: i32,
    fill: u8,
) -> Vec<u8> {
    (0..w * h)
        .map(|index| {
            let (sx, sy) = (
                index as i64 % w as i64 - i64::from(x),
                index as i64 / w as i64 - i64::from(y),
            );
            if sx < 0 || sy < 0 || sx >= sw as i64 || sy >= sh as i64 {
                fill
            } else {
                bytes[sy as usize * sw + sx as usize]
            }
        })
        .collect()
}
fn geometry_reference(
    bytes: &[u8],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
    kind: u32,
    x: i32,
    y: i32,
    fill: u8,
) -> Vec<u8> {
    // Destination inverse lookup, independent of the native tiled scatter.
    (0..w * h)
        .map(|index| {
            let (dx, dy) = (
                index as i64 % w as i64 - i64::from(x),
                index as i64 / w as i64 - i64::from(y),
            );
            let (sx, sy) = match kind {
                0 => (dy, sh as i64 - 1 - dx),
                1 => (sw as i64 - 1 - dy, dx),
                2 => (sw as i64 - 1 - dx, sh as i64 - 1 - dy),
                3 => (sw as i64 - 1 - dx, dy),
                _ => (dx, sh as i64 - 1 - dy),
            };
            if sx < 0 || sy < 0 || sx >= sw as i64 || sy >= sh as i64 {
                fill
            } else {
                bytes[sy as usize * sw + sx as usize]
            }
        })
        .collect()
}
fn axis_weights(source: usize, target: usize, dest: usize, method: u32) -> (Vec<u64>, u64) {
    let mut result = vec![0; source];
    if method == 0 {
        result[((2 * dest + 1) * source / (2 * target)).min(source - 1)] = 1;
        return (result, 1);
    }
    if method == 2 && source > target {
        let (left, right) = (dest * source, (dest + 1) * source);
        for (index, value) in result.iter_mut().enumerate() {
            *value = right
                .min((index + 1) * target)
                .saturating_sub(left.max(index * target)) as u64;
        }
        return (result, source as u64);
    }
    let divisor = (2 * target) as i64;
    let numerator = ((2 * dest + 1) * source) as i64 - target as i64;
    let first = numerator.div_euclid(divisor);
    let last = numerator.rem_euclid(divisor) as u64;
    result[first.clamp(0, source as i64 - 1) as usize] += divisor as u64 - last;
    result[(first + 1).clamp(0, source as i64 - 1) as usize] += last;
    (result, divisor as u64)
}
fn resize_reference(
    bytes: &[u8],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
    method: u32,
) -> Vec<u8> {
    // Full 2-D weighted sum with a single quantization, no separable cache/plan.
    (0..w * h)
        .map(|index| {
            let (wx, dx) = axis_weights(sw, w, index % w, method);
            let (wy, dy) = axis_weights(sh, h, index / w, method);
            let mut numerator = 0;
            for y in 0..sh {
                for x in 0..sw {
                    numerator += u64::from(bytes[y * sw + x]) * wx[x] * wy[y];
                }
            }
            ((numerator + dx * dy / 2) / (dx * dy)) as u8
        })
        .collect()
}

#[test]
fn independent_entry_exposes_three_required_callbacks_and_keeps_legacy_table_sizes() {
    let api = unsafe { &*micopi_a8_sampler_v1() };
    assert_eq!((api.abi_version, api.struct_size), (1, 32));
    assert!(api.reframe.is_some() && api.geometry.is_some() && api.resample.is_some());
    let geometry = unsafe { &*geometry::micopi_geometry_v1() };
    assert_eq!(geometry.struct_size, 24);
    let resize = unsafe { &*resample::micopi_resample_v1() };
    assert_eq!(resize.struct_size, 32);
    assert!(
        geometry.mask.is_some()
            && geometry.pixels.is_some()
            && resize.mask.is_some()
            && resize.pixels.is_some()
    );
}

#[test]
fn reframing_dense_and_uniform_planes_matches_inverse_lookup_and_preserves_padding() {
    for (sw, sh) in [(1, 1), (1, 7), (8, 1), (3, 2), (7, 5), (33, 35)] {
        let pixels: Vec<_> = (0..sw * sh)
            .map(|index| ((index * 71 + 19) % 256) as u8)
            .collect();
        let mut padded = vec![0x6b; (sw + 3) * sh];
        for y in 0..sh {
            padded[y * (sw + 3)..y * (sw + 3) + sw].copy_from_slice(&pixels[y * sw..(y + 1) * sw]);
        }
        let before = padded.clone();
        for (w, h) in [(sw, sh), (sw + 2, sh + 3), (2, 3)] {
            for (x, y) in [
                (-4096, 4096),
                (-3, -2),
                (-1, 1),
                (0, 0),
                (2, 3),
                (4096, -4096),
            ] {
                for fill in [0, 73, 255] {
                    for constant in [None, Some(0), Some(91), Some(255)] {
                        let descriptor = constant.map_or_else(|| source(&padded, sw + 3), uniform);
                        let raw = constant
                            .map_or_else(|| pixels.clone(), |value| vec![value as u8; sw * sh]);
                        let mut target = vec![0xa5; (w + 4) * h];
                        let request = Request::Reframe(abi::A8Reframe {
                            x,
                            y,
                            fill: fill as u32,
                            reserved: 0,
                        });
                        assert_eq!(
                            request.call(
                                &descriptor,
                                sw as u32,
                                sh as u32,
                                &mut output(&mut target, w, h, w + 4)
                            ),
                            abi::OK
                        );
                        assert_eq!(
                            packed(&target, w, h, w + 4),
                            reframe_reference(&raw, sw, sh, w, h, x, y, fill)
                        );
                        assert_eq!(padded, before);
                    }
                }
            }
        }
    }
}

#[test]
fn geometry_all_kinds_uniforms_offsets_and_strides_match_inverse_reference_and_legacy_bytes() {
    for (sw, sh) in [(1, 1), (1, 7), (8, 1), (3, 2), (7, 5), (33, 35)] {
        let pixels: Vec<_> = (0..sw * sh)
            .map(|index| ((index * 137 + 23) % 256) as u8)
            .collect();
        let mut padded = vec![0x6b; (sw + 2) * sh];
        for y in 0..sh {
            padded[y * (sw + 2)..y * (sw + 2) + sw].copy_from_slice(&pixels[y * sw..(y + 1) * sw]);
        }
        for kind in 0..5 {
            let (w, h) = if kind <= 1 {
                (sh + 2, sw + 1)
            } else {
                (sw + 2, sh + 1)
            };
            for (x, y) in [(-4096, 4096), (-1, 2), (0, 0), (2, -1), (4096, -4096)] {
                for fill in [0, 51, 255] {
                    for constant in [None, Some(0), Some(91), Some(255)] {
                        let descriptor = constant.map_or_else(|| source(&padded, sw + 2), uniform);
                        let raw = constant
                            .map_or_else(|| pixels.clone(), |value| vec![value as u8; sw * sh]);
                        let mut target = vec![0xa5; (w + 3) * h];
                        let request = Request::Geometry(abi::A8Geometry {
                            geometry: abi::Geometry { kind, x, y },
                            fill: fill as u32,
                        });
                        assert_eq!(
                            request.call(
                                &descriptor,
                                sw as u32,
                                sh as u32,
                                &mut output(&mut target, w, h, w + 3)
                            ),
                            abi::OK
                        );
                        let actual = packed(&target, w, h, w + 3);
                        assert_eq!(
                            actual,
                            geometry_reference(&raw, sw, sh, w, h, kind, x, y, fill)
                        );
                        if fill == 0 && constant.is_none() {
                            let mut old = vec![0xa5; w * h];
                            let coverage = abi::Coverage {
                                data: padded.as_ptr(),
                                len: padded.len(),
                                width: sw as u32,
                                height: sh as u32,
                                stride: (sw + 2) as u32,
                            };
                            let api = unsafe { &*geometry::micopi_geometry_v1() };
                            assert_eq!(
                                unsafe {
                                    (api.mask.unwrap())(
                                        &abi::Geometry { kind, x, y },
                                        &coverage,
                                        old.as_mut_ptr(),
                                        old.len(),
                                        w as u32,
                                        h as u32,
                                    )
                                },
                                abi::OK
                            );
                            assert_eq!(actual, old);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn nearest_bilinear_and_area_match_independent_coverage_sums_identity_and_old_callbacks() {
    for (sw, sh) in [(1, 1), (1, 5), (7, 1), (3, 2), (7, 5), (11, 7)] {
        let pixels: Vec<_> = (0..sw * sh)
            .map(|index| ((index * 79 + index / sw * 37) % 256) as u8)
            .collect();
        let mut padded = vec![0x6b; (sw + 3) * sh];
        for y in 0..sh {
            padded[y * (sw + 3)..y * (sw + 3) + sw].copy_from_slice(&pixels[y * sw..(y + 1) * sw]);
        }
        let unchanged = padded.clone();
        for (w, h) in [(1, 1), (sw, sh), (2, 3), (sw + 2, sh + 3), (4, 2)] {
            for method in 0..3 {
                let mut target = vec![0xa5; (w + 4) * h];
                let request = Request::Resize(abi::Resample { method });
                assert_eq!(
                    request.call(
                        &source(&padded, sw + 3),
                        sw as u32,
                        sh as u32,
                        &mut output(&mut target, w, h, w + 4)
                    ),
                    abi::OK
                );
                let actual = packed(&target, w, h, w + 4);
                assert_eq!(
                    actual,
                    resize_reference(&pixels, sw, sh, w, h, method),
                    "{sw}x{sh}->{w}x{h},method={method}"
                );
                let mut old = vec![0xa5; w * h];
                let coverage = abi::Coverage {
                    data: padded.as_ptr(),
                    len: padded.len(),
                    width: sw as u32,
                    height: sh as u32,
                    stride: (sw + 3) as u32,
                };
                let api = unsafe { &*resample::micopi_resample_v1() };
                assert_eq!(
                    unsafe {
                        (api.mask.unwrap())(
                            &abi::Resample { method },
                            &coverage,
                            old.as_mut_ptr(),
                            old.len(),
                            w as u32,
                            h as u32,
                        )
                    },
                    abi::OK
                );
                assert_eq!(actual, old);
                assert_eq!(padded, unchanged);
            }
        }
    }
}

#[test]
fn uniform_maximum_source_has_no_materialized_plane_and_preserves_requested_fill() {
    for value in [0, 73, 255] {
        for kind in 0..3 {
            let mut bytes = vec![0xa5; 7 * 3];
            let request = Request::valid(kind);
            assert_eq!(
                request.call(
                    &uniform(value),
                    4096,
                    4096,
                    &mut output(&mut bytes, 4, 3, 7)
                ),
                abi::OK
            );
            assert_eq!(packed(&bytes, 4, 3, 7), vec![value as u8; 12]);
        }
        let mut bytes = vec![0xa5; 7 * 3];
        let request = Request::Reframe(abi::A8Reframe {
            x: 4096,
            y: 0,
            fill: 91,
            reserved: 0,
        });
        assert_eq!(
            request.call(
                &uniform(value),
                4096,
                4096,
                &mut output(&mut bytes, 4, 3, 7)
            ),
            abi::OK
        );
        assert_eq!(packed(&bytes, 4, 3, 7), vec![91; 12]);
    }
    let mut maximum = vec![0; AREA];
    let request = Request::Resize(abi::Resample { method: 2 });
    assert_eq!(
        request.call(
            &uniform(255),
            4096,
            4096,
            &mut output(&mut maximum, 4096, 4096, 4096)
        ),
        abi::OK
    );
    assert!(maximum.iter().all(|value| *value == 255));
}

#[test]
fn invalid_options_dimensions_and_descriptors_reject_before_any_output_write() {
    let pixels = vec![73; 16];
    for kind in 0..3 {
        for case in 0..8 {
            let mut request = Request::valid(kind);
            match &mut request {
                Request::Reframe(value) => match case % 4 {
                    0 => value.fill = 256,
                    1 => value.reserved = 1,
                    2 => value.x = i32::MIN,
                    _ => value.y = 4097,
                },
                Request::Geometry(value) => match case % 4 {
                    0 => value.fill = 256,
                    1 => value.geometry.kind = 5,
                    2 => value.geometry.x = i32::MIN,
                    _ => value.geometry.y = 4097,
                },
                Request::Resize(value) => value.method = 3 + case,
            }
            let mut bytes = vec![0xa5; 16];
            assert_eq!(
                request.call(&source(&pixels, 4), 4, 4, &mut output(&mut bytes, 4, 4, 4)),
                abi::ERROR
            );
            assert_eq!(bytes, vec![0xa5; 16]);
        }
        let request = Request::valid(kind);
        for (sw, sh) in [(0, 1), (1, 0), (4097, 1), (1, 4097), (u32::MAX, u32::MAX)] {
            let mut bytes = vec![0xa5; 16];
            assert_eq!(
                request.call(&uniform(73), sw, sh, &mut output(&mut bytes, 4, 4, 4)),
                abi::ERROR
            );
            assert_eq!(bytes, vec![0xa5; 16]);
        }
        for case in 0..10 {
            let mut descriptor = if case < 3 {
                uniform(73)
            } else {
                source(&pixels, 4)
            };
            match case {
                0 => descriptor.len = 1,
                1 => descriptor.stride = 1,
                2 => descriptor.constant = 256,
                3 => descriptor.len = 15,
                4 => descriptor.stride = 3,
                5 => descriptor.stride = 4097,
                6 => descriptor.len = AREA + 1,
                7 => descriptor.constant = 1,
                8 => descriptor.data = (usize::MAX - 7) as *const u8,
                _ => descriptor.len = 0,
            }
            let mut bytes = vec![0xa5; 16];
            assert_eq!(
                request.call(&descriptor, 4, 4, &mut output(&mut bytes, 4, 4, 4)),
                abi::ERROR
            );
            assert_eq!(bytes, vec![0xa5; 16]);
        }
        for case in 0..12 {
            let mut bytes = vec![0xa5; 16];
            let mut target = output(&mut bytes, 4, 4, 4);
            match case {
                0 => target.width = 0,
                1 => target.height = 0,
                2 => target.width = 4097,
                3 => target.height = 4097,
                4 => target.stride = 3,
                5 => target.stride = 4097,
                6 => target.len = 15,
                7 => target.len = AREA + 1,
                8 => target.origin_x = 1,
                9 => target.origin_y = 1,
                10 => target.data = std::ptr::null_mut(),
                _ => target.data = (usize::MAX - 7) as *mut u8,
            }
            assert_eq!(
                request.call(&source(&pixels, 4), 4, 4, &mut target),
                abi::ERROR
            );
            assert_eq!(bytes, vec![0xa5; 16]);
        }
        assert_eq!(pixels, vec![73; 16]);
    }
}

#[test]
fn null_misaligned_and_metadata_or_pixel_aliases_reject_without_writes() {
    let mut pixels = vec![73; 24];
    for kind in 0..3 {
        let request = Request::valid(kind);
        let descriptor = source(&pixels, 6);
        let mut bytes = vec![0xa5; 16];
        let mut target = output(&mut bytes, 4, 4, 4);
        let request_pointer = request.pointer();
        for metadata in 0..6 {
            let (r, s, d) = match metadata {
                0 => (
                    std::ptr::null(),
                    &descriptor as *const _,
                    &mut target as *mut _,
                ),
                1 => (request_pointer, std::ptr::null(), &mut target as *mut _),
                2 => (
                    request_pointer,
                    &descriptor as *const _,
                    std::ptr::null_mut(),
                ),
                3 => (
                    unsafe { request_pointer.add(1) },
                    &descriptor as *const _,
                    &mut target as *mut _,
                ),
                4 => (
                    request_pointer,
                    unsafe {
                        (&descriptor as *const abi::CompositeMask)
                            .cast::<u8>()
                            .add(1)
                            .cast()
                    },
                    &mut target as *mut _,
                ),
                _ => (request_pointer, &descriptor as *const _, unsafe {
                    (&mut target as *mut abi::A8Pixels)
                        .cast::<u8>()
                        .add(1)
                        .cast()
                }),
            };
            assert_eq!(unsafe { request.invoke(r, s, 4, 4, d) }, abi::ERROR);
            assert_eq!(bytes, vec![0xa5; 16]);
        }
        for pointer in [
            request_pointer as *mut u8,
            (&descriptor as *const abi::CompositeMask).cast::<u8>() as *mut u8,
            pixels.as_mut_ptr(),
            unsafe { pixels.as_mut_ptr().add(4) },
        ] {
            let mut target = output(&mut bytes, 4, 4, 4);
            target.data = pointer;
            assert_eq!(request.call(&descriptor, 4, 4, &mut target), abi::ERROR);
            assert_eq!(bytes, vec![0xa5; 16]);
            assert_eq!(pixels, vec![73; 24]);
        }
        let mut target = output(&mut bytes, 4, 4, 4);
        target.data = (&mut target as *mut abi::A8Pixels).cast();
        let saved = (
            target.data,
            target.len,
            target.stride,
            target.width,
            target.height,
            target.origin_x,
            target.origin_y,
        );
        assert_eq!(request.call(&descriptor, 4, 4, &mut target), abi::ERROR);
        assert_eq!(
            (
                target.data,
                target.len,
                target.stride,
                target.width,
                target.height,
                target.origin_x,
                target.origin_y
            ),
            saved
        );
        assert_eq!(bytes, vec![0xa5; 16]);
    }
}

#[test]
fn reframe_zero_fill_keeps_legacy_integer_coverage_move_results() {
    let raw: Vec<_> = (0..35)
        .map(|index| ((index * 53 + 17) % 256) as u8)
        .collect();
    let coverage = abi::Coverage {
        data: raw.as_ptr(),
        len: raw.len(),
        width: 7,
        height: 5,
        stride: 7,
    };
    let api = unsafe { &*crate::movement::micopi_move_v1() };
    for (x, y) in [(-4096, 4096), (-2, 1), (0, 0), (3, -2), (4096, -4096)] {
        let mut old = vec![0; 35];
        assert_eq!(
            unsafe {
                (api.mask.unwrap())(
                    &abi::Move {
                        kind: 2,
                        dx: x,
                        dy: y,
                    },
                    &coverage,
                    old.as_mut_ptr(),
                    old.len(),
                )
            },
            abi::OK
        );
        let mut bytes = vec![0xa5; 11 * 5];
        let request = Request::Reframe(abi::A8Reframe {
            x,
            y,
            fill: 0,
            reserved: 0,
        });
        assert_eq!(
            request.call(&source(&raw, 7), 7, 5, &mut output(&mut bytes, 7, 5, 11)),
            abi::OK
        );
        assert_eq!(packed(&bytes, 7, 5, 11), old);
    }
}
