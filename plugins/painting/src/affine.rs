//! Frame-bounded affine sampling. No allocation or mutable-source overlap feedback.
use super::{
    geometry::{disjoint, layout},
    raster::over,
};
use micopi_plugin_api as abi;
const UNIT: u64 = 65536;
const DEN: u64 = UNIT * UNIT;
fn inverse(o: &abi::Affine) -> Option<[f64; 6]> {
    let m = o.matrix;
    if o.kind > 2
        || o.method > 1
        || m.iter().any(|v| !v.is_finite())
        || [m[0], m[1], m[3], m[4]].iter().any(|v| v.abs() > 256.)
        || m[2].abs() > 4_194_304.
        || m[5].abs() > 4_194_304.
    {
        return None;
    }
    let det = m[0] * m[4] - m[1] * m[3];
    if det.abs() < 1e-8 {
        return None;
    }
    Some([
        m[4] / det,
        -m[1] / det,
        (m[1] * m[5] - m[4] * m[2]) / det,
        -m[3] / det,
        m[0] / det,
        (m[3] * m[2] - m[0] * m[5]) / det,
    ])
}
fn coordinate(m: &[f64; 6], x: usize, y: usize) -> (f64, f64) {
    let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
    (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5])
}
fn weights(x: f64, y: f64) -> (i64, i64, [u64; 4]) {
    let (x, y) = (x - 0.5, y - 0.5);
    let (ix, iy) = (x.floor(), y.floor());
    let (a, b) = (
        ((x - ix) * UNIT as f64 + 0.5).floor() as u64,
        ((y - iy) * UNIT as f64 + 0.5).floor() as u64,
    );
    (
        ix as i64,
        iy as i64,
        [
            (UNIT - a) * (UNIT - b),
            a * (UNIT - b),
            (UNIT - a) * b,
            a * b,
        ],
    )
}
fn index(x: i64, y: i64, w: usize, h: usize, stride: usize) -> Option<usize> {
    (x >= 0 && y >= 0 && x < w as i64 && y < h as i64).then(|| y as usize * stride + x as usize)
}
fn extracted(p: u32, c: u8) -> u32 {
    let a = ((p >> 24) * u32::from(c) + 127) / 255;
    if a == 0 {
        0
    } else {
        (p & 0xffffff) | (a << 24)
    }
}
fn cleared(p: u32, c: u8) -> u32 {
    if c == 0 {
        return p;
    }
    let a = ((p >> 24) * (255 - u32::from(c)) + 127) / 255;
    if a == 0 {
        0
    } else {
        (p & 0xffffff) | (a << 24)
    }
}
struct Source<'a> {
    pixels: &'a [u32],
    mask: Option<(&'a [u8], usize)>,
    width: usize,
    height: usize,
    stride: usize,
}
impl Source<'_> {
    fn at(&self, x: i64, y: i64) -> u32 {
        let Some(i) = index(x, y, self.width, self.height, self.stride) else {
            return 0;
        };
        let p = self.pixels[i];
        if let Some((m, stride)) = self.mask {
            extracted(p, m[y as usize * stride + x as usize])
        } else {
            p
        }
    }
    fn sample(&self, method: u32, x: f64, y: f64) -> u32 {
        if method == 0 {
            return self.at(x.floor() as i64, y.floor() as i64);
        }
        let (ix, iy, w) = weights(x, y);
        if w[0] == DEN {
            return self.at(ix, iy);
        }
        let p = [
            self.at(ix, iy),
            self.at(ix + 1, iy),
            self.at(ix, iy + 1),
            self.at(ix + 1, iy + 1),
        ];
        let alpha: [u64; 4] = std::array::from_fn(|i| u64::from(p[i] >> 24) * w[i]);
        let total: u64 = alpha.iter().sum();
        let a = (total + DEN / 2) / DEN;
        if a == 0 {
            return 0;
        }
        let mut out = (a as u32) << 24;
        for shift in [0, 8, 16] {
            let sum: u64 = (0..4)
                .map(|i| u64::from((p[i] >> shift) & 255) * alpha[i])
                .sum();
            out |= (((sum + total / 2) / total) as u32) << shift;
        }
        out
    }
}
unsafe extern "C" fn pixels(
    o: *const abi::Affine,
    s: *const abi::Pixels,
    c: *const abi::Coverage,
    d: *mut abi::Pixels,
) -> i32 {
    if o.is_null() || s.is_null() || c.is_null() || d.is_null() {
        return abi::ERROR;
    }
    let (o, s, c, d) = (&*o, &*s, &*c, &mut *d);
    let Some(inv) = inverse(o) else {
        return abi::ERROR;
    };
    if o.kind == 2
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(d.width, d.height, d.stride, d.len)
        || (s.width, s.height) != (d.width, d.height)
        || (s.origin_x, s.origin_y, d.origin_x, d.origin_y) != (0, 0, 0, 0)
        || !disjoint(s.data, s.len, d.data, d.len)
    {
        return abi::ERROR;
    }
    if o.kind == 0 {
        if !c.data.is_null() || c.len != 0 || c.width != 0 || c.height != 0 || c.stride != 0 {
            return abi::ERROR;
        }
    } else if !layout(c.width, c.height, c.stride, c.len)
        || (c.width, c.height) != (s.width, s.height)
        || !disjoint(c.data, c.len, d.data.cast(), d.len * 4)
    {
        return abi::ERROR;
    }
    let src = Source {
        pixels: std::slice::from_raw_parts(s.data, s.len),
        mask: if o.kind == 1 {
            Some((std::slice::from_raw_parts(c.data, c.len), c.stride as usize))
        } else {
            None
        },
        width: s.width as usize,
        height: s.height as usize,
        stride: s.stride as usize,
    };
    let out = std::slice::from_raw_parts_mut(d.data, d.len);
    let identity = o.matrix == [1., 0., 0., 0., 1., 0.];
    for y in 0..src.height {
        for x in 0..src.width {
            let base = src.pixels[y * src.stride + x];
            let value = if identity {
                base
            } else {
                let (sx, sy) = coordinate(&inv, x, y);
                let moving = src.sample(o.method, sx, sy);
                if let Some((mask, stride)) = src.mask {
                    let rest = cleared(base, mask[y * stride + x]);
                    if moving >> 24 == 0 {
                        rest
                    } else {
                        over(rest, moving)
                    }
                } else {
                    moving
                }
            };
            out[y * d.stride as usize + x] = value;
        }
    }
    abi::OK
}
unsafe extern "C" fn mask(
    o: *const abi::Affine,
    s: *const abi::Coverage,
    out: *mut u8,
    len: usize,
) -> i32 {
    if o.is_null() || s.is_null() {
        return abi::ERROR;
    }
    let (o, s) = (&*o, &*s);
    let Some(inv) = inverse(o) else {
        return abi::ERROR;
    };
    if o.kind == 0
        || !layout(s.width, s.height, s.stride, s.len)
        || len != s.width as usize * s.height as usize
        || !disjoint(s.data, s.len, out, len)
    {
        return abi::ERROR;
    }
    let src = std::slice::from_raw_parts(s.data, s.len);
    let dest = std::slice::from_raw_parts_mut(out, len);
    let (w, h) = (s.width as usize, s.height as usize);
    let at = |x, y| index(x, y, w, h, s.stride as usize).map_or(0, |i| u64::from(src[i]));
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = coordinate(&inv, x, y);
            dest[y * w + x] = if o.method == 0 {
                at(sx.floor() as i64, sy.floor() as i64) as u8
            } else {
                let (ix, iy, weights) = weights(sx, sy);
                let v = [
                    at(ix, iy),
                    at(ix + 1, iy),
                    at(ix, iy + 1),
                    at(ix + 1, iy + 1),
                ];
                ((v.into_iter().zip(weights).map(|(p, w)| p * w).sum::<u64>() + DEN / 2) / DEN)
                    as u8
            };
        }
    }
    abi::OK
}
static API: abi::AffineApi = abi::AffineApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::AffineApi>() as u32,
    pixels: Some(pixels),
    mask: Some(mask),
    bounds: Some(bounds),
};
unsafe extern "C" fn bounds(s: *const abi::Pixels, out: *mut abi::TrimBounds) -> i32 {
    super::trim::bounds(
        &abi::Trim {
            mode: 0,
            tolerance: 0,
            sides: 15,
        },
        s,
        out,
    )
}
#[no_mangle]
pub extern "C" fn micopi_affine_v1() -> *const abi::AffineApi {
    &API
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(p: &mut [u32], w: u32, h: u32, stride: u32) -> abi::Pixels {
        abi::Pixels {
            data: p.as_mut_ptr(),
            len: p.len(),
            width: w,
            height: h,
            stride,
            origin_x: 0,
            origin_y: 0,
        }
    }
    fn coverage(p: &[u8], w: u32, h: u32, stride: u32) -> abi::Coverage {
        abi::Coverage {
            data: p.as_ptr(),
            len: p.len(),
            width: w,
            height: h,
            stride,
        }
    }
    fn absent() -> abi::Coverage {
        abi::Coverage {
            data: std::ptr::null(),
            len: 0,
            width: 0,
            height: 0,
            stride: 0,
        }
    }
    #[test]
    fn integer_affines_match_forward_scatter_and_preserve_words_and_padding() {
        let (w, h, stride) = (5, 5, 7);
        let mut source: Vec<u32> = (0..stride * h).map(|i| (i * 7919) as u32).collect();
        let original = source.clone();
        let bytes: Vec<u8> = (0..stride * h).map(|i| (i * 7) as u8).collect();
        for matrix in [
            [1., 0., 0., 0., 1., 0.],
            [1., 0., 2., 0., 1., -1.],
            [0., -1., 5., 1., 0., 0.],
            [-1., 0., 5., 0., 1., 0.],
            [0., 1., 0., -1., 0., 5.],
            [1., 1., -0.5, 0., 1., 0.],
            [1., 0., 0., 1., 1., -0.5],
            [1., 1., -0.5, 1., 2., -1.],
        ] {
            let mut expected = vec![0; stride * h];
            let mut expected_mask = vec![0; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
                    let tx = (matrix[0] * cx + matrix[1] * cy + matrix[2]).floor() as i64;
                    let ty = (matrix[3] * cx + matrix[4] * cy + matrix[5]).floor() as i64;
                    if let Some(i) = index(tx, ty, w, h, stride) {
                        expected[i] = source[y * stride + x];
                        expected_mask[ty as usize * w + tx as usize] = bytes[y * stride + x];
                    }
                }
            }
            for method in 0..=1 {
                let o = abi::Affine {
                    kind: 0,
                    method,
                    matrix,
                };
                let mut out = vec![77; stride * h];
                assert_eq!(
                    unsafe {
                        pixels(
                            &o,
                            &view(&mut source, w as u32, h as u32, stride as u32),
                            &absent(),
                            &mut view(&mut out, w as u32, h as u32, stride as u32),
                        )
                    },
                    abi::OK
                );
                for y in 0..h {
                    assert_eq!(
                        &out[y * stride..y * stride + w],
                        &expected[y * stride..y * stride + w]
                    );
                    assert_eq!(&out[y * stride + w..(y + 1) * stride], &[77; 2]);
                }
                let mut dest = vec![0; w * h];
                let m = coverage(&bytes, w as u32, h as u32, stride as u32);
                assert_eq!(
                    unsafe {
                        mask(
                            &abi::Affine { kind: 2, ..o },
                            &m,
                            dest.as_mut_ptr(),
                            dest.len(),
                        )
                    },
                    abi::OK
                );
                assert_eq!(dest, expected_mask);
            }
            assert_eq!(source, original);
        }
    }
    #[test]
    fn half_pixel_sampling_uses_transparent_edges_and_premultiplied_color() {
        let mut source = [0xffff0000, 0x000000ff];
        let mut out = [0; 2];
        let o = abi::Affine {
            kind: 0,
            method: 1,
            matrix: [1., 0., 0.5, 0., 1., 0.],
        };
        assert_eq!(
            unsafe {
                pixels(
                    &o,
                    &view(&mut source, 2, 1, 2),
                    &absent(),
                    &mut view(&mut out, 2, 1, 2),
                )
            },
            abi::OK
        );
        assert_eq!(out, [0x80ff0000, 0x80ff0000]);
        let c = coverage(&[255, 0], 2, 1, 2);
        let mut result = [0; 2];
        assert_eq!(
            unsafe { mask(&abi::Affine { kind: 2, ..o }, &c, result.as_mut_ptr(), 2) },
            abi::OK
        );
        assert_eq!(result, [128, 128]);
    }
    #[test]
    fn fractional_shear_matches_hand_calculated_premultiplied_rows_and_mask() {
        let mut source = [
            0xff445566, 0xff778899, 0xffaabbcc, 0xffff0000, 0x8000ff00, 0x000000ff, 0xff112233,
            0x7f445566, 0xffaabbcc,
        ];
        let original = source;
        let mut out = [0; 9];
        // Row zero has no shift, row one shifts half a pixel, row two one pixel.
        let o = abi::Affine {
            kind: 0,
            method: 1,
            matrix: [1., 0.5, -0.25, 0., 1., 0.],
        };
        assert_eq!(
            unsafe {
                pixels(
                    &o,
                    &view(&mut source, 3, 3, 3),
                    &absent(),
                    &mut view(&mut out, 3, 3, 3),
                )
            },
            abi::OK
        );
        assert_eq!(
            out,
            [
                0xff445566, 0xff778899, 0xffaabbcc, 0x80ff0000, 0xc0aa5500, 0x4000ff00, 0,
                0xff112233, 0x7f445566
            ]
        );
        let bytes = [7, 255, 23, 255, 128, 0, 32, 64, 128];
        let c = coverage(&bytes, 3, 3, 3);
        let mut mask_out = [0; 9];
        assert_eq!(
            unsafe { mask(&abi::Affine { kind: 2, ..o }, &c, mask_out.as_mut_ptr(), 9) },
            abi::OK
        );
        assert_eq!(mask_out, [7, 255, 23, 128, 192, 64, 0, 32, 64]);
        assert_eq!(source, original);
    }
    #[test]
    fn selected_affine_translation_matches_original_clear_and_paste_without_feedback() {
        let mut source = [0xff0000ff, 0xffff0000, 0x00112233, 0xff00ff00];
        let original = source;
        let bytes = [128, 255, 0, 0];
        let c = coverage(&bytes, 4, 1, 4);
        let mut out = [0; 4];
        for method in 0..=1 {
            let o = abi::Affine {
                kind: 1,
                method,
                matrix: [1., 0., 1., 0., 1., 0.],
            };
            assert_eq!(
                unsafe {
                    pixels(
                        &o,
                        &view(&mut source, 4, 1, 4),
                        &c,
                        &mut view(&mut out, 4, 1, 4),
                    )
                },
                abi::OK
            );
            assert_eq!(out, [0x7f0000ff, 0x800000ff, 0xffff0000, 0xff00ff00]);
            assert_eq!(source, original);
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Affine {
                            matrix: [1., 0., 0., 0., 1., 0.],
                            ..o
                        },
                        &view(&mut source, 4, 1, 4),
                        &c,
                        &mut view(&mut out, 4, 1, 4),
                    )
                },
                abi::OK
            );
            assert_eq!(out, original);
        }
    }
    #[test]
    fn singular_nonfinite_excessive_options_and_aliases_reject_before_writes() {
        let mut source = [0xffabcdef; 4];
        let mut out = [77; 4];
        for matrix in [
            [0.; 6],
            [f64::NAN, 0., 0., 0., 1., 0.],
            [1., 0., f64::INFINITY, 0., 1., 0.],
            [257., 0., 0., 0., 1., 0.],
            [1., 0., 4_194_305., 0., 1., 0.],
        ] {
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Affine {
                            kind: 0,
                            method: 1,
                            matrix,
                        },
                        &view(&mut source, 2, 2, 2),
                        &absent(),
                        &mut view(&mut out, 2, 2, 2),
                    )
                },
                abi::ERROR
            );
            assert_eq!(out, [77; 4]);
        }
        let o = abi::Affine {
            kind: 0,
            method: 0,
            matrix: [1., 0., 0., 0., 1., 0.],
        };
        let s = view(&mut source, 2, 2, 2);
        let mut d = view(&mut source, 2, 2, 2);
        assert_eq!(unsafe { pixels(&o, &s, &absent(), &mut d) }, abi::ERROR);
        for bad in [
            abi::Affine { method: 2, ..o },
            abi::Affine { kind: 3, ..o },
            abi::Affine { kind: 1, ..o },
        ] {
            assert_eq!(
                unsafe { pixels(&bad, &s, &absent(), &mut view(&mut out, 2, 2, 2)) },
                abi::ERROR
            );
            assert_eq!(out, [77; 4]);
        }
        assert_eq!(source, [0xffabcdef; 4]);
    }
    #[test]
    fn rotated_bilinear_pixels_and_masks_match_an_independent_float_reference() {
        let (w, h, stride) = (8usize, 6usize, 10usize);
        let mut src: Vec<u32> = (0..stride * h)
            .map(|i| ((i * 73 % 256) as u32) << 24 | ((i * 311_123) as u32 & 0xffffff))
            .collect();
        let bytes: Vec<u8> = (0..stride * h).map(|i| (i * 97 % 256) as u8).collect();
        let matrix = [0.8, -0.6, 2., 0.6, 0.8, -1.];
        let o = abi::Affine {
            kind: 0,
            method: 1,
            matrix,
        };
        let mut out = vec![0; stride * h];
        assert_eq!(
            unsafe {
                pixels(
                    &o,
                    &view(&mut src, w as u32, h as u32, stride as u32),
                    &absent(),
                    &mut view(&mut out, w as u32, h as u32, stride as u32),
                )
            },
            abi::OK
        );
        let mut mask_out = vec![0; w * h];
        let c = coverage(&bytes, w as u32, h as u32, stride as u32);
        assert_eq!(
            unsafe {
                mask(
                    &abi::Affine { kind: 2, ..o },
                    &c,
                    mask_out.as_mut_ptr(),
                    mask_out.len(),
                )
            },
            abi::OK
        );
        for y in 0..h {
            for x in 0..w {
                // Analytic inverse of the fixed 3/5,4/5 rotation, independent of inverse()/weights().
                let (dx, dy) = (x as f64 + 0.5 - 2., y as f64 + 0.5 + 1.);
                let (sx, sy) = (0.8 * dx + 0.6 * dy - 0.5, -0.6 * dx + 0.8 * dy - 0.5);
                let (ix, iy) = (sx.floor() as i64, sy.floor() as i64);
                let (fx, fy) = (sx - sx.floor(), sy - sy.floor());
                let mut alpha = 0.;
                let mut channels = [0.; 3];
                let mut cov = 0.;
                for yy in 0..=1 {
                    for xx in 0..=1 {
                        let (tx, ty) = (ix + xx, iy + yy);
                        if tx < 0 || ty < 0 || tx >= w as i64 || ty >= h as i64 {
                            continue;
                        }
                        let weight = (if xx == 0 { 1. - fx } else { fx })
                            * (if yy == 0 { 1. - fy } else { fy });
                        let i = ty as usize * stride + tx as usize;
                        let p = src[i];
                        let a = (p >> 24) as f64 * weight;
                        alpha += a;
                        cov += bytes[i] as f64 * weight;
                        for (n, shift) in [0, 8, 16].into_iter().enumerate() {
                            channels[n] += ((p >> shift) & 255) as f64 * a;
                        }
                    }
                }
                let got = out[y * stride + x];
                assert!(((got >> 24) as i32 - alpha.round() as i32).abs() <= 1);
                if alpha.round() > 0. {
                    for (n, shift) in [0, 8, 16].into_iter().enumerate() {
                        assert!(
                            (((got >> shift) & 255) as i32 - (channels[n] / alpha).round() as i32)
                                .abs()
                                <= 1
                        );
                    }
                }
                assert!((mask_out[y * w + x] as i32 - cov.round() as i32).abs() <= 1);
            }
        }
    }
    #[test]
    #[ignore = "Explicit affine kernel costs; excludes host/preview/history/recording"]
    fn affine_cost() {
        let mut source = vec![0x80402010; 4096 * 4096];
        let mut out = vec![0; source.len()];
        let bytes = vec![128; source.len()];
        let c = coverage(&bytes, 4096, 4096, 4096);
        let no_mask = absent();
        for kind in 0..=1 {
            for method in 0..=1 {
                let o = abi::Affine {
                    kind,
                    method,
                    matrix: [0.93, -0.2, 500., 0.2, 0.93, -200.],
                };
                let now = std::time::Instant::now();
                assert_eq!(
                    unsafe {
                        pixels(
                            &o,
                            &view(&mut source, 4096, 4096, 4096),
                            if kind == 0 { &no_mask } else { &c },
                            &mut view(&mut out, 4096, 4096, 4096),
                        )
                    },
                    abi::OK
                );
                eprintln!(
                    "4096² affine kind {kind}, method {method}: {:?}; no kernel allocation",
                    now.elapsed()
                );
            }
        }
    }
}
