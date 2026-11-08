//! Exact identity, independent sampled-pixel oracles and ABI rejection checks.
use super::*;
fn config() -> abi::LiquifyConfig {
    abi::LiquifyConfig {
        abi_version: 1,
        struct_size: 32,
        width: 8,
        height: 6,
        spacing: 1,
        reserved: 0,
        budget_bytes: 1024 * 1024,
    }
}
fn source(data: &[u32]) -> abi::Pixels {
    abi::Pixels {
        data: data.as_ptr() as *mut u32,
        len: data.len(),
        width: 8,
        height: 6,
        stride: 8,
        origin_x: 0,
        origin_y: 0,
    }
}
fn output(data: &mut [u32], x: u32, y: u32, w: u32, h: u32, stride: u32) -> abi::Pixels {
    abi::Pixels {
        data: data.as_mut_ptr(),
        len: data.len(),
        width: w,
        height: h,
        stride,
        origin_x: x,
        origin_y: y,
    }
}
struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { destroy(self.0) }
    }
}
fn handle() -> Handle {
    let h = Handle(unsafe { create(&config()) });
    assert!(!h.0.is_null());
    h
}
fn warp(h: &Handle) {
    let p = abi::LiquifyPush {
        x: 4.,
        y: 3.,
        dx: 1.,
        dy: 0.5,
        radius: 4.,
        strength: 0.75,
    };
    let mut result = abi::LiquifyUpdate::default();
    assert_eq!(unsafe { push(h.0, &p, 1, &mut result) }, abi::OK);
    assert!(result.count > 0);
}
fn reference(src: &[u32], x: f64, y: f64) -> u32 {
    let x = x - 0.5;
    let y = y - 0.5;
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let u = ((x - x.floor()) * 65536. + 0.5).floor() as u64;
    let v = ((y - y.floor()) * 65536. + 0.5).floor() as u64;
    let weights = [
        (65536 - u) * (65536 - v),
        u * (65536 - v),
        (65536 - u) * v,
        u * v,
    ];
    let coords = [(ix, iy), (ix + 1, iy), (ix, iy + 1), (ix + 1, iy + 1)];
    let colors: Vec<_> = coords
        .into_iter()
        .map(|(x, y)| {
            if x >= 0 && x < 8 && y >= 0 && y < 6 {
                src[y as usize * 8 + x as usize]
            } else {
                0
            }
        })
        .collect();
    let coverage: Vec<_> = colors
        .iter()
        .zip(weights)
        .map(|(p, w)| u64::from(p >> 24) * w)
        .collect();
    let sum: u64 = coverage.iter().sum();
    let alpha = (sum + (1u64 << 31)) >> 32;
    if alpha == 0 {
        return 0;
    }
    let mut result = (alpha as u32) << 24;
    for shift in [0, 8, 16] {
        let channel: u64 = colors
            .iter()
            .zip(&coverage)
            .map(|(p, a)| u64::from((p >> shift) & 255) * a)
            .sum();
        result |= (((channel + sum / 2) / sum) as u32) << shift;
    }
    result
}
#[test]
fn identity_and_warped_padded_regions_match_full_frame_and_independent_pixels() {
    let h = handle();
    let data: Vec<_> = (0..48)
        .map(|i| {
            if i % 7 == 0 {
                0x00123456
            } else {
                0x80402010 + i * 137
            }
        })
        .collect();
    let src = source(&data);
    let mut out = vec![0xdeadbeef; 4 * 3];
    let mut dst = output(&mut out, 2, 1, 3, 3, 4);
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut dst) }, abi::OK);
    for y in 0..3 {
        assert_eq!(
            &out[y * 4..y * 4 + 3],
            &data[(y + 1) * 8 + 2..(y + 1) * 8 + 5]
        );
        assert_eq!(out[y * 4 + 3], 0xdeadbeef);
    }
    warp(&h);
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut dst) }, abi::OK);
    let f = unsafe { field(h.0).unwrap() };
    for y in 0..3 {
        for x in 0..3 {
            let (qx, qy) = (x as f64 + 2.5, y as f64 + 1.5);
            let d = f.displacement(qx, qy);
            let expected = if d == [0.; 2] {
                data[(y + 1) * 8 + x + 2]
            } else {
                reference(&data, qx + d[0], qy + d[1])
            };
            assert_eq!(out[y * 4 + x], expected);
        }
        assert_eq!(out[y * 4 + 3], 0xdeadbeef);
    }
    let mut full = vec![0; 48];
    let mut full_out = output(&mut full, 0, 0, 8, 6, 8);
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut full_out) }, abi::OK);
    for y in 0..3 {
        assert_eq!(
            &out[y * 4..y * 4 + 3],
            &full[(y + 1) * 8 + 2..(y + 1) * 8 + 5]
        );
    }
    assert_eq!(unsafe { clear(h.0) }, abi::OK);
    assert_eq!(unsafe { pixels(h.0, 0, &src, &mut full_out) }, abi::OK);
    assert_eq!(full, data);
}
#[test]
fn raw_a8_dense_uniform_and_reveal_exterior_share_region_coordinates() {
    let h = handle();
    warp(&h);
    let bytes: Vec<_> = (0..48).map(|i| (i * 37 % 256) as u8).collect();
    for uniform in [false, true] {
        let src = abi::CompositeMask {
            data: if uniform {
                std::ptr::null()
            } else {
                bytes.as_ptr()
            },
            len: if uniform { 0 } else { 48 },
            stride: if uniform { 0 } else { 8 },
            constant: if uniform { 91 } else { 0 },
        };
        let mut out = vec![17u8; 12];
        let mut dst = abi::A8Pixels {
            data: out.as_mut_ptr(),
            len: 12,
            width: 3,
            height: 3,
            stride: 4,
            origin_x: 2,
            origin_y: 1,
        };
        assert_eq!(unsafe { mask(h.0, 1, &src, &mut dst) }, abi::OK);
        let f = unsafe { field(h.0).unwrap() };
        for y in 0..3 {
            for x in 0..3 {
                let (qx, qy) = (x as f64 + 2.5, y as f64 + 1.5);
                let d = f.displacement(qx, qy);
                let (sx, sy) = (qx + d[0] - 0.5, qy + d[1] - 0.5);
                let (ix, iy) = (sx.floor() as i64, sy.floor() as i64);
                let u = ((sx - sx.floor()) * 65536. + 0.5).floor() as u64;
                let v = ((sy - sy.floor()) * 65536. + 0.5).floor() as u64;
                let weights = [
                    (65536 - u) * (65536 - v),
                    u * (65536 - v),
                    (65536 - u) * v,
                    u * v,
                ];
                let total: u64 = [(ix, iy), (ix + 1, iy), (ix, iy + 1), (ix + 1, iy + 1)]
                    .into_iter()
                    .zip(weights)
                    .map(|((a, b), w)| {
                        let sample = if a < 0 || b < 0 || a >= 8 || b >= 6 {
                            255
                        } else if uniform {
                            91
                        } else {
                            u64::from(bytes[b as usize * 8 + a as usize])
                        };
                        sample * w
                    })
                    .sum();
                assert_eq!(out[y * 4 + x], ((total + (1u64 << 31)) >> 32) as u8);
            }
            assert_eq!(out[y * 4 + 3], 17);
        }
    }
}
#[test]
fn invalid_abi_descriptors_aliases_and_pushes_reject_before_writes_or_field_changes() {
    let mut bad = config();
    bad.struct_size = 31;
    assert!(unsafe { create(&bad) }.is_null());
    bad = config();
    bad.reserved = 1;
    assert!(unsafe { create(&bad) }.is_null());
    let h = handle();
    let input = vec![0x80402010; 48];
    let src = source(&input);
    let mut out = vec![0xdeadbeef; 48];
    let mut dst = output(&mut out, 0, 0, 8, 6, 8);
    assert_eq!(unsafe { pixels(h.0, 2, &src, &mut dst) }, abi::ERROR);
    assert!(out.iter().all(|p| *p == 0xdeadbeef));
    let mut alias = source(&input);
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut alias) }, abi::ERROR);
    assert!(input.iter().all(|p| *p == 0x80402010));
    dst.origin_x = 1;
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut dst) }, abi::ERROR);
    dst.origin_x = 0;
    dst.stride = 7;
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut dst) }, abi::ERROR);
    assert!(out.iter().all(|p| *p == 0xdeadbeef));
    let p = abi::LiquifyPush {
        x: 4.,
        y: 3.,
        dx: 1.,
        dy: 0.,
        radius: 4.,
        strength: f32::NAN,
    };
    let mut result = abi::LiquifyUpdate::default();
    result.count = 91;
    assert_eq!(unsafe { push(h.0, &p, 1, &mut result) }, abi::ERROR);
    assert_eq!(result.count, 91);
    assert_eq!(unsafe { field(h.0).unwrap() }.displacement(4., 3.), [0.; 2]);
}

#[test]
fn edge_push_uses_transparent_rgba_and_reveal_a8_exterior() {
    let h = handle();
    let p = abi::LiquifyPush {
        x: 0.,
        y: 0.,
        dx: 3.,
        dy: 0.,
        radius: 4.,
        strength: 1.,
    };
    let mut update = abi::LiquifyUpdate::default();
    assert_eq!(unsafe { push(h.0, &p, 1, &mut update) }, abi::OK);
    let input = vec![0xffffffff; 48];
    let src = source(&input);
    let mut rgba = vec![17; 1];
    let mut dst = output(&mut rgba, 0, 0, 1, 1, 1);
    let src_mask = abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 0,
    };
    let mut byte = 17;
    let mut a8 = abi::A8Pixels {
        data: &mut byte,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    for method in [0, 1] {
        assert_eq!(unsafe { pixels(h.0, method, &src, &mut dst) }, abi::OK);
        assert_eq!(rgba, [0]);
        assert_eq!(unsafe { mask(h.0, method, &src_mask, &mut a8) }, abi::OK);
        assert_eq!(byte, 255);
    }
    let mut invalid = src_mask;
    invalid.constant = 256;
    byte = 17;
    assert_eq!(unsafe { mask(h.0, 1, &invalid, &mut a8) }, abi::ERROR);
    assert_eq!(byte, 17);
    let protected_output = unsafe { field(h.0).unwrap() }.storage()[0]
        .0
        .cast_mut()
        .cast::<u32>();
    let mut alias = abi::Pixels {
        data: protected_output,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    assert_eq!(unsafe { pixels(h.0, 1, &src, &mut alias) }, abi::ERROR);
}
