//! Independent finite homography sampling; shares the established alpha/coverage kernels.
use micopi_plugin_api as abi;
fn inverse(o: &abi::Projective) -> Option<[f64; 9]> {
    let [a, b, c, d, e, f, g, h, i] = o.matrix;
    if o.kind > 2
        || o.method > 1
        || o.matrix.iter().any(|v| !v.is_finite())
        || [a, b, d, e].iter().any(|v| v.abs() > 256.)
        || [c, f].iter().any(|v| v.abs() > 4_194_304.)
        || [g, h, i].iter().any(|v| v.abs() > 16.)
    {
        return None;
    }
    let co = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let det = a * co[0] + b * co[3] + c * co[6];
    if det.abs() < 1e-8 {
        return None;
    }
    Some(co.map(|v| v / det))
}
fn coordinate(inv: &[f64; 9], x: usize, y: usize) -> Option<(f64, f64)> {
    let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
    let den = inv[6] * x + inv[7] * y + inv[8];
    // H(H^-1 q) has forward denominator 1/den: reject the back face and horizon.
    if !(1e-12..=1e6).contains(&den) {
        return None;
    }
    let reciprocal = 1. / den;
    let (sx, sy) = (
        (inv[0] * x + inv[1] * y + inv[2]) * reciprocal,
        (inv[3] * x + inv[4] * y + inv[5]) * reciprocal,
    );
    (sx.is_finite() && sy.is_finite()).then_some((sx, sy))
}
unsafe extern "C" fn pixels(
    o: *const abi::Projective,
    s: *const abi::Pixels,
    c: *const abi::Coverage,
    d: *mut abi::Pixels,
) -> i32 {
    if o.is_null() || s.is_null() || c.is_null() || d.is_null() {
        return abi::ERROR;
    }
    let o = &*o;
    let Some(inv) = inverse(o) else {
        return abi::ERROR;
    };
    super::affine::render_pixels(
        &*s,
        &*c,
        &*d,
        o.kind,
        o.method,
        o.matrix[8] > 0.
            && o.matrix
                == [
                    o.matrix[8],
                    0.,
                    0.,
                    0.,
                    o.matrix[8],
                    0.,
                    0.,
                    0.,
                    o.matrix[8],
                ],
        |x, y| coordinate(&inv, x, y),
    )
}
unsafe extern "C" fn mask(
    o: *const abi::Projective,
    s: *const abi::Coverage,
    d: *mut u8,
    len: usize,
) -> i32 {
    if o.is_null() || s.is_null() {
        return abi::ERROR;
    }
    let o = &*o;
    let Some(inv) = inverse(o) else {
        return abi::ERROR;
    };
    super::affine::render_mask(&*s, d, len, o.kind, o.method, |x, y| coordinate(&inv, x, y))
}
static API: abi::ProjectiveApi = abi::ProjectiveApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::ProjectiveApi>() as u32,
    pixels: Some(pixels),
    mask: Some(mask),
};
#[no_mangle]
pub extern "C" fn micopi_projective_v1() -> *const abi::ProjectiveApi {
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
        abi::Coverage::default()
    }
    #[test]
    fn normalized_trapezoid_nearest_matches_analytic_integer_lookup_and_masks() {
        let mut src: Vec<_> = (0..16).map(|i| 0xff000000 | i).collect();
        let original = src.clone();
        let o = abi::Projective {
            kind: 0,
            method: 0,
            matrix: [1., 0., 0., 0., 1., 0., 0.25, 0., 1.],
        };
        let mut out = vec![77; 16];
        assert_eq!(
            unsafe {
                pixels(
                    &o,
                    &view(&mut src, 4, 4, 4),
                    &absent(),
                    &mut view(&mut out, 4, 4, 4),
                )
            },
            abi::OK
        );
        assert_eq!(
            out,
            [src[0], src[2], 0, 0, src[4], src[10], 0, 0, src[8], 0, 0, 0, 0, 0, 0, 0]
        );
        let bytes: Vec<_> = (0..16).map(|i| i * 15).collect();
        let c = coverage(&bytes, 4, 4, 4);
        let mut dest = [77; 16];
        assert_eq!(
            unsafe { mask(&abi::Projective { kind: 2, ..o }, &c, dest.as_mut_ptr(), 16) },
            abi::OK
        );
        assert_eq!(dest, [0, 30, 0, 0, 60, 150, 0, 0, 120, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(src, original);
    }
    #[test]
    fn general_homogeneous_scale_and_zero_origin_denominator_are_supported() {
        let o = abi::Projective {
            kind: 0,
            method: 0,
            matrix: [1., 0., 1., 0., 1., 0., 0.1, 0., 0.],
        };
        let inv = inverse(&o).unwrap();
        let (x, y) = coordinate(&inv, 12, 5).unwrap();
        assert!((x - 4.).abs() < 1e-10 && (y - 2.2).abs() < 1e-10);
        let scaled = abi::Projective {
            matrix: o.matrix.map(|v| v * 2.),
            ..o
        };
        let p = coordinate(&inverse(&scaled).unwrap(), 12, 5).unwrap();
        assert!((p.0 - x).abs() < 1e-10 && (p.1 - y).abs() < 1e-10);
        let horizon = abi::Projective {
            matrix: [1., 0., 0.5, 0., 1., 0., 0.125, 0., 1.],
            ..o
        };
        assert!(coordinate(&inverse(&horizon).unwrap(), 8, 0).is_none());
        assert!(coordinate(&inverse(&horizon).unwrap(), 9, 0).is_none());
    }
    #[test]
    fn affine_special_case_matches_published_sampler_for_words_selection_and_padding() {
        let (w, h, stride) = (5, 5, 7);
        let mut src: Vec<u32> = (0..stride * h)
            .map(|i| ((i % 4) as u32 * 85) << 24 | (i as u32 * 7919 & 0xffffff))
            .collect();
        let bytes: Vec<u8> = (0..stride * h).map(|i| (i * 7) as u8).collect();
        let c = coverage(&bytes, w as u32, h as u32, stride as u32);
        let no = absent();
        for matrix in [
            [1., 0., 0., 0., 1., 0.],
            [1., 0.3, -0.7, -0.2, 0.94, 0.5],
            [0., -1., 5., 1., 0., 0.],
        ] {
            for kind in 0..=1 {
                for method in 0..=1 {
                    let a = abi::Affine {
                        kind,
                        method,
                        matrix,
                    };
                    let [a0, b, c0, d, e, f] = matrix;
                    for scale in [1., 2.] {
                        let p = abi::Projective {
                            kind,
                            method,
                            matrix: [a0, b, c0, d, e, f, 0., 0., 1.].map(|v| v * scale),
                        };
                        let mut old = vec![77; stride * h];
                        let mut new = old.clone();
                        let mask = if kind == 0 { &no } else { &c };
                        assert_eq!(
                            unsafe {
                                ((*super::super::affine::micopi_affine_v1()).pixels.unwrap())(
                                    &a,
                                    &view(&mut src, w as u32, h as u32, stride as u32),
                                    mask,
                                    &mut view(&mut old, w as u32, h as u32, stride as u32),
                                )
                            },
                            abi::OK
                        );
                        assert_eq!(
                            unsafe {
                                pixels(
                                    &p,
                                    &view(&mut src, w as u32, h as u32, stride as u32),
                                    mask,
                                    &mut view(&mut new, w as u32, h as u32, stride as u32),
                                )
                            },
                            abi::OK
                        );
                        assert_eq!(old, new);
                    }
                }
            }
        }
    }
    #[test]
    fn bilinear_projective_pixels_and_masks_match_independent_float_reference() {
        let (w, h, stride) = (5, 5, 7);
        let mut src: Vec<u32> = (0..stride * h)
            .map(|i| ((i % 4) as u32 * 85) << 24 | (i as u32 * 7919 & 0xffffff))
            .collect();
        let original = src.clone();
        let bytes: Vec<u8> = (0..stride * h).map(|i| (i * 7) as u8).collect();
        let o = abi::Projective {
            kind: 0,
            method: 1,
            matrix: [1., 0., 0., 0., 1., 0., 0.125, 0., 1.],
        };
        let mut out = vec![77; stride * h];
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
        let c = coverage(&bytes, w as u32, h as u32, stride as u32);
        let mut cov = vec![0; w * h];
        assert_eq!(
            unsafe {
                mask(
                    &abi::Projective { kind: 2, ..o },
                    &c,
                    cov.as_mut_ptr(),
                    cov.len(),
                )
            },
            abi::OK
        );
        for y in 0..h {
            for x in 0..w {
                // Analytic inverse x=u/(1-u/8), y=v/(1-u/8); no production inverse/weights.
                let den = 1. - (x as f64 + 0.5) / 8.;
                let (sx, sy) = ((x as f64 + 0.5) / den - 0.5, (y as f64 + 0.5) / den - 0.5);
                let (ix, iy) = (sx.floor() as i64, sy.floor() as i64);
                let (fx, fy) = (sx - sx.floor(), sy - sy.floor());
                let mut a = 0.;
                let mut rgb = [0.; 3];
                let mut coverage = 0.;
                for yy in 0..=1 {
                    for xx in 0..=1 {
                        let (tx, ty) = (ix + xx, iy + yy);
                        if tx < 0 || ty < 0 || tx >= w as i64 || ty >= h as i64 {
                            continue;
                        }
                        let weight = (if xx == 0 { 1. - fx } else { fx })
                            * (if yy == 0 { 1. - fy } else { fy });
                        let i = ty as usize * stride + tx as usize;
                        let alpha = (src[i] >> 24) as f64 * weight;
                        a += alpha;
                        coverage += bytes[i] as f64 * weight;
                        for (n, shift) in [0, 8, 16].into_iter().enumerate() {
                            rgb[n] += ((src[i] >> shift) & 255) as f64 * alpha;
                        }
                    }
                }
                let got = out[y * stride + x];
                assert!(((got >> 24) as i32 - a.round() as i32).abs() <= 1);
                if a.round() > 0. {
                    for (n, shift) in [0, 8, 16].into_iter().enumerate() {
                        assert!(
                            (((got >> shift) & 255) as i32 - (rgb[n] / a).round() as i32).abs()
                                <= 1
                        );
                    }
                }
                assert!((cov[y * w + x] as i32 - coverage.round() as i32).abs() <= 1);
            }
            assert_eq!(&out[y * stride + w..(y + 1) * stride], &[77, 77]);
        }
        assert_eq!(src, original);
    }
    #[test]
    fn invalid_matrix_descriptors_and_aliases_reject_before_writes() {
        let mut src = [0xffabcdef; 4];
        let mut out = [77; 4];
        let valid = [1., 0., 0., 0., 1., 0., 0.1, 0., 1.];
        for matrix in [
            [0.; 9],
            [1., 0., 0., 0., 1., 0., 0., 0., 0.],
            [257., 0., 0., 0., 1., 0., 0., 0., 1.],
            [1., 0., 0., 0., 1., 0., 17., 0., 1.],
            [1., 0., f64::NAN, 0., 1., 0., 0., 0., 1.],
        ] {
            let o = abi::Projective {
                kind: 0,
                method: 1,
                matrix,
            };
            assert_eq!(
                unsafe {
                    pixels(
                        &o,
                        &view(&mut src, 2, 2, 2),
                        &absent(),
                        &mut view(&mut out, 2, 2, 2),
                    )
                },
                abi::ERROR
            );
            assert_eq!(out, [77; 4]);
        }
        let o = abi::Projective {
            kind: 0,
            method: 1,
            matrix: valid,
        };
        assert_eq!(
            unsafe {
                pixels(
                    &o,
                    &view(&mut src, 2, 2, 2),
                    &absent(),
                    &mut view(&mut src, 2, 2, 2),
                )
            },
            abi::ERROR
        );
        assert_eq!(src, [0xffabcdef; 4]);
        let bad = abi::Pixels {
            width: 3,
            ..view(&mut src, 2, 2, 2)
        };
        assert_eq!(
            unsafe { pixels(&o, &bad, &absent(), &mut view(&mut out, 2, 2, 2)) },
            abi::ERROR
        );
        assert_eq!(out, [77; 4]);
        let mut bytes = [255; 4];
        let c = coverage(&bytes, 2, 2, 2);
        assert_eq!(
            unsafe { mask(&abi::Projective { kind: 2, ..o }, &c, bytes.as_mut_ptr(), 4) },
            abi::ERROR
        );
    }
    #[test]
    #[ignore = "Explicit kernel-only projective costs; excludes host/history/recording"]
    fn projective_cost() {
        let mut src = vec![0x80402010; 4096 * 4096];
        let mut out = vec![0; src.len()];
        let bytes = vec![128; src.len()];
        let c = coverage(&bytes, 4096, 4096, 4096);
        let no = absent();
        for kind in 0..=1 {
            for method in 0..=1 {
                let o = abi::Projective {
                    kind,
                    method,
                    matrix: [0.98, -0.1, 18., 0.1, 0.98, -12., 0.00005, -0.00002, 1.],
                };
                let start = std::time::Instant::now();
                assert_eq!(
                    unsafe {
                        pixels(
                            &o,
                            &view(&mut src, 4096, 4096, 4096),
                            if kind == 0 { &no } else { &c },
                            &mut view(&mut out, 4096, 4096, 4096),
                        )
                    },
                    abi::OK
                );
                println!(
                    "projective kind={kind} method={method}: {:?}",
                    start.elapsed()
                );
                std::hint::black_box(&out);
            }
        }
    }
}
