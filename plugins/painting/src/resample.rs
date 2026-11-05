//! Pixel-center image sampling; bounded axis plans, no full-image scratch.
use super::geometry::{disjoint, layout};
use micopi_plugin_api as abi;
#[derive(Clone, Copy)]
struct Axis {
    a: usize,
    b: usize,
    weight: u64,
}
fn axis(source: usize, target: usize) -> Vec<Axis> {
    let den = (2 * target) as i64;
    (0..target)
        .map(|i| {
            let n = ((2 * i + 1) * source) as i64 - target as i64;
            let a = n.div_euclid(den);
            Axis {
                a: a.clamp(0, source as i64 - 1) as usize,
                b: (a + 1).clamp(0, source as i64 - 1) as usize,
                weight: n.rem_euclid(den) as u64,
            }
        })
        .collect()
}
trait Sample: Copy {
    fn mix(p: [Self; 4], weights: [u64; 4], den: u64) -> Self;
}
impl Sample for u8 {
    fn mix(p: [Self; 4], w: [u64; 4], den: u64) -> Self {
        ((p.into_iter()
            .zip(w)
            .map(|(p, w)| u64::from(p) * w)
            .sum::<u64>()
            + den / 2)
            / den) as u8
    }
}
impl Sample for u32 {
    fn mix(p: [Self; 4], w: [u64; 4], den: u64) -> Self {
        let alpha: [u64; 4] = std::array::from_fn(|i| u64::from(p[i] >> 24) * w[i]);
        let sum: u64 = alpha.iter().sum();
        let a = (sum + den / 2) / den;
        if a == 0 {
            return 0;
        }
        let mut out = (a as u32) << 24;
        for shift in [0, 8, 16] {
            let c: u64 = (0..4)
                .map(|i| u64::from((p[i] >> shift) & 255) * alpha[i])
                .sum();
            out |= (((c + sum / 2) / sum) as u32) << shift;
        }
        out
    }
}
fn run<T: Sample>(
    method: u32,
    src: &[T],
    sw: usize,
    sh: usize,
    ss: usize,
    out: &mut [T],
    w: usize,
    h: usize,
    stride: usize,
) {
    if (sw, sh) == (w, h) {
        for y in 0..h {
            out[y * stride..y * stride + w].copy_from_slice(&src[y * ss..y * ss + w]);
        }
    } else if method == 0 {
        let xs: Vec<_> = (0..w)
            .map(|x| ((2 * x + 1) * sw / (2 * w)).min(sw - 1))
            .collect();
        for y in 0..h {
            let row = ((2 * y + 1) * sh / (2 * h)).min(sh - 1) * ss;
            for x in 0..w {
                out[y * stride + x] = src[row + xs[x]];
            }
        }
    } else {
        let xs = axis(sw, w);
        let ys = axis(sh, h);
        let (dx, dy) = ((2 * w) as u64, (2 * h) as u64);
        for (y, q) in ys.iter().enumerate() {
            for (x, p) in xs.iter().enumerate() {
                let weights = [
                    (dx - p.weight) * (dy - q.weight),
                    p.weight * (dy - q.weight),
                    (dx - p.weight) * q.weight,
                    p.weight * q.weight,
                ];
                let samples = [
                    src[q.a * ss + p.a],
                    src[q.a * ss + p.b],
                    src[q.b * ss + p.a],
                    src[q.b * ss + p.b],
                ];
                out[y * stride + x] = T::mix(samples, weights, dx * dy);
            }
        }
    }
}
unsafe extern "C" fn pixels(
    o: *const abi::Resample,
    s: *const abi::Pixels,
    d: *mut abi::Pixels,
) -> i32 {
    if o.is_null() || s.is_null() || d.is_null() {
        return abi::ERROR;
    }
    let (o, s, d) = (&*o, &*s, &mut *d);
    if o.method > 1
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(d.width, d.height, d.stride, d.len)
        || (s.origin_x, s.origin_y, d.origin_x, d.origin_y) != (0, 0, 0, 0)
        || !disjoint(s.data, s.len, d.data, d.len)
    {
        return abi::ERROR;
    }
    run(
        o.method,
        std::slice::from_raw_parts(s.data, s.len),
        s.width as usize,
        s.height as usize,
        s.stride as usize,
        std::slice::from_raw_parts_mut(d.data, d.len),
        d.width as usize,
        d.height as usize,
        d.stride as usize,
    );
    abi::OK
}
unsafe extern "C" fn mask(
    o: *const abi::Resample,
    s: *const abi::Coverage,
    d: *mut u8,
    len: usize,
    w: u32,
    h: u32,
) -> i32 {
    if o.is_null() || s.is_null() {
        return abi::ERROR;
    }
    let (o, s) = (&*o, &*s);
    if o.method > 1
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(w, h, w, len)
        || len != w as usize * h as usize
        || !disjoint(s.data, s.len, d, len)
    {
        return abi::ERROR;
    }
    run(
        o.method,
        std::slice::from_raw_parts(s.data, s.len),
        s.width as usize,
        s.height as usize,
        s.stride as usize,
        std::slice::from_raw_parts_mut(d, len),
        w as usize,
        h as usize,
        w as usize,
    );
    abi::OK
}
static API: abi::ResampleApi = abi::ResampleApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::ResampleApi>() as u32,
    pixels: Some(pixels),
    mask: Some(mask),
};
#[no_mangle]
pub extern "C" fn micopi_resample_v1() -> *const abi::ResampleApi {
    &API
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view(data: &mut [u32], w: usize, h: usize, stride: usize) -> abi::Pixels {
        abi::Pixels {
            data: data.as_mut_ptr(),
            len: data.len(),
            width: w as u32,
            height: h as u32,
            stride: stride as u32,
            origin_x: 0,
            origin_y: 0,
        }
    }
    // Floating inverse lookup is separate from the production rational axis plan.
    fn oracle(src: &[u32], sw: usize, sh: usize, w: usize, h: usize, method: u32) -> Vec<u32> {
        (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                if method == 0 {
                    return src[((y as f64 + 0.5) * sh as f64 / h as f64).floor() as usize * sw
                        + ((x as f64 + 0.5) * sw as f64 / w as f64).floor() as usize];
                }
                let (fx, fy) = (
                    (x as f64 + 0.5) * sw as f64 / w as f64 - 0.5,
                    (y as f64 + 0.5) * sh as f64 / h as f64 - 0.5,
                );
                let mut a = 0.;
                let mut rgb = [0.; 3];
                for j in 0..2 {
                    for k in 0..2 {
                        let sx = (fx.floor() as i64 + k).clamp(0, sw as i64 - 1) as usize;
                        let sy = (fy.floor() as i64 + j).clamp(0, sh as i64 - 1) as usize;
                        let weight = if k == 0 {
                            1. - fx.fract().rem_euclid(1.)
                        } else {
                            fx.fract().rem_euclid(1.)
                        } * if j == 0 {
                            1. - fy.fract().rem_euclid(1.)
                        } else {
                            fy.fract().rem_euclid(1.)
                        };
                        let p = src[sy * sw + sx];
                        let aw = (p >> 24) as f64 * weight;
                        a += aw;
                        for c in 0..3 {
                            rgb[c] += ((p >> (8 * c)) & 255) as f64 * aw;
                        }
                    }
                }
                if a.round() == 0. {
                    return 0;
                }
                (a.round() as u32) << 24
                    | (rgb[0] / a).round() as u32
                    | ((rgb[1] / a).round() as u32) << 8
                    | ((rgb[2] / a).round() as u32) << 16
            })
            .collect()
    }
    #[test]
    fn sampling_matches_independent_reference_and_preserves_padding_and_source() {
        for (sw, sh, w, h) in [
            (3, 2, 7, 5),
            (7, 5, 3, 2),
            (1, 5, 4, 1),
            (5, 1, 1, 7),
            (33, 35, 65, 17),
        ] {
            let source: Vec<_> = (0..sw * sh)
                .map(|i| (i as u32).wrapping_mul(1957793911))
                .collect();
            for method in 0..=1 {
                let mut strided = vec![0xabcdef01; (sw + 3) * sh];
                for y in 0..sh {
                    strided[y * (sw + 3)..y * (sw + 3) + sw]
                        .copy_from_slice(&source[y * sw..y * sw + sw]);
                }
                let before = strided.clone();
                let mut out = vec![0x11223344; (w + 2) * h];
                assert_eq!(
                    unsafe {
                        pixels(
                            &abi::Resample { method },
                            &view(&mut strided, sw, sh, sw + 3),
                            &mut view(&mut out, w, h, w + 2),
                        )
                    },
                    abi::OK
                );
                let expected = oracle(&source, sw, sh, w, h, method);
                for y in 0..h {
                    for x in 0..w {
                        let (p, q) = (out[y * (w + 2) + x], expected[y * w + x]);
                        if method == 0 {
                            assert_eq!(p, q);
                        } else {
                            for shift in [0, 8, 16, 24] {
                                assert!(
                                    (((p >> shift) & 255) as i32 - ((q >> shift) & 255) as i32)
                                        .abs()
                                        <= 1,
                                    "{sw} {sh} {w} {h} {p:x} {q:x}"
                                );
                            }
                        }
                    }
                    assert_eq!(&out[y * (w + 2) + w..(y + 1) * (w + 2)], &[0x11223344; 2]);
                }
                assert_eq!(strided, before);
            }
        }
    }
    #[test]
    fn alpha_edges_mask_weights_and_identity_are_explicit() {
        let src = [0x00ff0000u32, 0xff0000ff];
        let mut out = [0; 3];
        run(1, &src, 2, 1, 2, &mut out, 3, 1, 3);
        assert_eq!(out, [0, 0x800000ff, 0xff0000ff]);
        let mut mask = [0u8; 3];
        run(1, &[0u8, 255], 2, 1, 2, &mut mask, 3, 1, 3);
        assert_eq!(mask, [0, 128, 255]);
        for method in 0..=1 {
            let mut out = [0; 2];
            run(method, &src, 2, 1, 2, &mut out, 2, 1, 2);
            assert_eq!(out, src);
        }
        let mut out = [0; 1];
        run(1, &[0xffffffffu32, 0], 2, 1, 2, &mut out, 1, 1, 1);
        assert_eq!(out, [0x80ffffff]);
    }
    #[test]
    fn invalid_requests_fail_before_writes() {
        let mut source = [0xff112233; 4];
        let s = view(&mut source, 2, 2, 2);
        let mut output = [77; 4];
        for method in [2, u32::MAX] {
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Resample { method },
                        &s,
                        &mut view(&mut output, 2, 2, 2),
                    )
                },
                abi::ERROR
            );
            assert_eq!(output, [77; 4]);
        }
        let mut alias = view(&mut source, 2, 2, 2);
        assert_eq!(
            unsafe { pixels(&abi::Resample { method: 0 }, &s, &mut alias) },
            abi::ERROR
        );
        let mut d = view(&mut output, 2, 2, 2);
        d.len = 3;
        assert_eq!(
            unsafe { pixels(&abi::Resample { method: 0 }, &s, &mut d) },
            abi::ERROR
        );
        let m = abi::Coverage {
            data: source.as_ptr().cast(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
        };
        let mut out = [99u8; 4];
        assert_eq!(
            unsafe { mask(&abi::Resample { method: 0 }, &m, out.as_mut_ptr(), 3, 2, 2) },
            abi::ERROR
        );
        assert_eq!(out, [99; 4]);
    }
    #[test]
    #[ignore = "Explicit image sampling cost, excludes host/history/compression"]
    fn resample_cost() {
        let source = vec![0x80402010u32; 4096 * 4096];
        for (method, w, h) in [(0, 2048, 2048), (1, 2048, 2048), (1, 4095, 4095)] {
            let mut out = vec![0; w * h];
            let start = std::time::Instant::now();
            run(method, &source, 4096, 4096, 4096, &mut out, w, h, w);
            assert!(out.iter().all(|p| *p == 0x80402010));
            eprintln!(
                "4096² -> {w}x{h} method {method}: {:?}; axis plans only",
                start.elapsed()
            );
        }
    }
}
