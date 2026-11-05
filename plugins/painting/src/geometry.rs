//! Exact orthogonal permutations, tiled for source/destination locality. No allocation.
use micopi_plugin_api as abi;
fn layout(width: u32, height: u32, stride: u32, len: usize) -> bool {
    width > 0
        && height > 0
        && width <= 4096
        && height <= 4096
        && stride >= width
        && stride <= 4096
        && len <= 4096 * 4096
        && (height as usize - 1) * stride as usize + width as usize <= len
}
fn options(o: &abi::Geometry) -> bool {
    o.kind <= 4 && o.x.unsigned_abs() <= 4096 && o.y.unsigned_abs() <= 4096
}
fn disjoint<T>(a: *const T, an: usize, b: *mut T, bn: usize) -> bool {
    if a.is_null()
        || b.is_null()
        || a as usize % std::mem::align_of::<T>() != 0
        || b as usize % std::mem::align_of::<T>() != 0
    {
        return false;
    }
    match (
        (a as usize).checked_add(an * std::mem::size_of::<T>()),
        (b as usize).checked_add(bn * std::mem::size_of::<T>()),
    ) {
        (Some(ae), Some(be)) => ae <= b as usize || be <= a as usize,
        _ => false,
    }
}
fn run<T: Copy + Default>(
    o: &abi::Geometry,
    source: &[T],
    sw: usize,
    sh: usize,
    ss: usize,
    target: &mut [T],
    tw: usize,
    th: usize,
    ts: usize,
) {
    let rotated = if o.kind <= 1 { (sh, sw) } else { (sw, sh) };
    if (o.x, o.y) != (0, 0) || (tw, th) != rotated {
        for y in 0..th {
            target[y * ts..y * ts + tw].fill(T::default());
        }
    }
    for by in (0..sh).step_by(32) {
        for bx in (0..sw).step_by(32) {
            for y in by..(by + 32).min(sh) {
                for x in bx..(bx + 32).min(sw) {
                    let (tx, ty) = match o.kind {
                        0 => (sh - 1 - y, x),
                        1 => (y, sw - 1 - x),
                        2 => (sw - 1 - x, sh - 1 - y),
                        3 => (sw - 1 - x, y),
                        _ => (x, sh - 1 - y),
                    };
                    let (tx, ty) = (tx as i32 + o.x, ty as i32 + o.y);
                    if tx >= 0 && ty >= 0 && (tx as usize) < tw && (ty as usize) < th {
                        target[ty as usize * ts + tx as usize] = source[y * ss + x];
                    }
                }
            }
        }
    }
}
unsafe extern "C" fn pixels(
    o: *const abi::Geometry,
    s: *const abi::Pixels,
    d: *mut abi::Pixels,
) -> i32 {
    if o.is_null() || s.is_null() || d.is_null() {
        return abi::ERROR;
    }
    let (o, s, d) = (&*o, &*s, &mut *d);
    if !options(o)
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(d.width, d.height, d.stride, d.len)
        || s.origin_x != 0
        || s.origin_y != 0
        || d.origin_x != 0
        || d.origin_y != 0
        || !disjoint(s.data, s.len, d.data, d.len)
    {
        return abi::ERROR;
    }
    run(
        o,
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
    o: *const abi::Geometry,
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
    if !options(o)
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(w, h, w, len)
        || len != w as usize * h as usize
        || !disjoint(s.data, s.len, d, len)
    {
        return abi::ERROR;
    }
    run(
        o,
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
static API: abi::GeometryApi = abi::GeometryApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::GeometryApi>() as u32,
    pixels: Some(pixels),
    mask: Some(mask),
};
#[no_mangle]
pub extern "C" fn micopi_geometry_v1() -> *const abi::GeometryApi {
    &API
}

#[cfg(test)]
mod tests {
    use super::*;
    fn oracle(
        source: &[u32],
        sw: usize,
        sh: usize,
        w: usize,
        h: usize,
        kind: u32,
        dx: i32,
        dy: i32,
    ) -> Vec<u32> {
        let mut out = vec![0; w * h];
        // Inverse coordinate lookup is independent of the kernel's forward scatter.
        for y in 0..h {
            for x in 0..w {
                let (x, y) = (x as i32 - dx, y as i32 - dy);
                let (sx, sy) = match kind {
                    0 => (y, sh as i32 - 1 - x),
                    1 => (sw as i32 - 1 - y, x),
                    2 => (sw as i32 - 1 - x, sh as i32 - 1 - y),
                    3 => (sw as i32 - 1 - x, y),
                    _ => (x, sh as i32 - 1 - y),
                };
                if sx >= 0 && sy >= 0 && (sx as usize) < sw && (sy as usize) < sh {
                    out[(y + dy) as usize * w + (x + dx) as usize] =
                        source[sy as usize * sw + sx as usize];
                }
            }
        }
        out
    }
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
    #[test]
    fn exact_pixel_permutations_match_inverse_oracle_across_tiles_parity_and_clipping() {
        for (sw, sh) in [(1, 1), (1, 7), (8, 1), (3, 2), (2, 5), (33, 65), (64, 67)] {
            let mut src = (0..sw * sh)
                .map(|i| ((i as u32 * 37) & 255) << 24 | (i as u32 * 7919 & 0xffffff))
                .collect::<Vec<_>>();
            let before = src.clone();
            for kind in 0..5 {
                for centered in [false, true] {
                    let (w, h) = if kind <= 1 && !centered {
                        (sh, sw)
                    } else {
                        (sw, sh)
                    };
                    let (dx, dy) = if kind <= 1 && centered {
                        (
                            (sw as i32 - sh as i32).div_euclid(2),
                            (sh as i32 - sw as i32).div_euclid(2),
                        )
                    } else {
                        (0, 0)
                    };
                    let mut out = vec![0x12345678; w * h];
                    assert_eq!(
                        unsafe {
                            pixels(
                                &abi::Geometry { kind, x: dx, y: dy },
                                &view(&mut src, sw, sh, sw),
                                &mut view(&mut out, w, h, w),
                            )
                        },
                        abi::OK
                    );
                    assert_eq!(out, oracle(&src, sw, sh, w, h, kind, dx, dy));
                    assert_eq!(src, before);
                }
            }
        }
    }
    #[test]
    fn strided_storage_keeps_padding_and_mask_coverage_exact() {
        let mut src = vec![1, 2, 3, 99, 4, 5, 6, 99];
        let mut out = vec![77; 9];
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Geometry {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &view(&mut src, 3, 2, 4),
                    &mut view(&mut out, 2, 3, 3),
                )
            },
            abi::OK
        );
        assert_eq!(out, vec![4, 1, 77, 5, 2, 77, 6, 3, 77]);
        let coverage = [0, 64, 128, 99, 192, 254, 255, 99];
        let mut out = [77; 6];
        let source = abi::Coverage {
            data: coverage.as_ptr(),
            len: 8,
            width: 3,
            height: 2,
            stride: 4,
        };
        assert_eq!(
            unsafe {
                mask(
                    &abi::Geometry {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &source,
                    out.as_mut_ptr(),
                    6,
                    2,
                    3,
                )
            },
            abi::OK
        );
        assert_eq!(out, [192, 0, 254, 64, 255, 128]);
    }
    #[test]
    fn invalid_and_aliasing_descriptors_fail_before_writes() {
        let mut src = [1, 2, 3, 4];
        let mut out = [77; 4];
        for (kind, x) in [(5, 0), (0, 4097), (0, i32::MIN)] {
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Geometry { kind, x, y: 0 },
                        &view(&mut src, 2, 2, 2),
                        &mut view(&mut out, 2, 2, 2),
                    )
                },
                abi::ERROR
            );
            assert_eq!(out, [77; 4]);
        }
        let s = view(&mut src, 2, 2, 2);
        let mut d = view(&mut src, 2, 2, 2);
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Geometry {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &s,
                    &mut d,
                )
            },
            abi::ERROR
        );
        let mut d = view(&mut out, 2, 2, 2);
        d.origin_x = 1;
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Geometry {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &s,
                    &mut d,
                )
            },
            abi::ERROR
        );
        assert_eq!(out, [77; 4]);
    }
    #[test]
    #[ignore = "Explicit cold geometry timing, excludes host/history/compression"]
    fn exact_geometry_cost() {
        for n in [1024, 4096] {
            let mut src = vec![0x80402010; n * n];
            let mut out = vec![0; n * n];
            let start = std::time::Instant::now();
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Geometry {
                            kind: 0,
                            x: 0,
                            y: 0,
                        },
                        &view(&mut src, n, n, n),
                        &mut view(&mut out, n, n, n),
                    )
                },
                abi::OK
            );
            eprintln!(
                "{n}x{n} clockwise: {:?}, no native scratch",
                start.elapsed()
            );
        }
    }
}
