//! Integer move kernels. Original pixels/coverage remain immutable, including overlaps.
use super::{
    geometry::{disjoint, layout},
    raster::over,
};
use micopi_plugin_api as abi;
fn valid(o: &abi::Move) -> bool {
    o.kind <= 2 && o.dx.unsigned_abs() <= 4096 && o.dy.unsigned_abs() <= 4096
}
fn selected(p: u32, c: u32) -> u32 {
    if c == 0 {
        0
    } else {
        (p & 0xffffff) | ((((p >> 24) * c + 127) / 255) << 24)
    }
}
unsafe extern "C" fn pixels(
    o: *const abi::Move,
    s: *const abi::Pixels,
    m: *const abi::Coverage,
    d: *mut abi::Pixels,
) -> i32 {
    if o.is_null() || s.is_null() || m.is_null() || d.is_null() {
        return abi::ERROR;
    }
    let (o, s, m, d) = (&*o, &*s, &*m, &mut *d);
    if !valid(o)
        || o.kind == 2
        || !layout(s.width, s.height, s.stride, s.len)
        || !layout(d.width, d.height, d.stride, d.len)
        || (s.width, s.height) != (d.width, d.height)
        || (s.origin_x, s.origin_y, d.origin_x, d.origin_y) != (0, 0, 0, 0)
        || !disjoint(s.data, s.len, d.data, d.len)
    {
        return abi::ERROR;
    }
    if o.kind == 0 {
        if !m.data.is_null() || m.len != 0 || m.width != 0 || m.height != 0 || m.stride != 0 {
            return abi::ERROR;
        }
    } else if !layout(m.width, m.height, m.stride, m.len)
        || (m.width, m.height) != (s.width, s.height)
        || !disjoint(m.data, m.len, d.data.cast(), d.len * 4)
    {
        return abi::ERROR;
    }
    let src = std::slice::from_raw_parts(s.data, s.len);
    let out = std::slice::from_raw_parts_mut(d.data, d.len);
    let mask = if o.kind == 1 {
        Some(std::slice::from_raw_parts(m.data, m.len))
    } else {
        None
    };
    let (w, h) = (s.width as usize, s.height as usize);
    if (o.dx, o.dy) == (0, 0) {
        for y in 0..h {
            out[y * d.stride as usize..y * d.stride as usize + w]
                .copy_from_slice(&src[y * s.stride as usize..y * s.stride as usize + w]);
        }
        return abi::OK;
    }
    if o.kind == 0 {
        for y in 0..h {
            out[y * d.stride as usize..y * d.stride as usize + w].fill(0);
        }
        let (sx, sy) = (
            (-o.dx).max(0).min(w as i32) as usize,
            (-o.dy).max(0).min(h as i32) as usize,
        );
        let (dx, dy) = (
            o.dx.max(0).min(w as i32) as usize,
            o.dy.max(0).min(h as i32) as usize,
        );
        let (cols, rows) = ((w - sx).min(w - dx), (h - sy).min(h - dy));
        for row in 0..rows {
            out[(dy + row) * d.stride as usize + dx..(dy + row) * d.stride as usize + dx + cols]
                .copy_from_slice(
                    &src[(sy + row) * s.stride as usize + sx
                        ..(sy + row) * s.stride as usize + sx + cols],
                );
        }
        return abi::OK;
    }
    for y in 0..h {
        for x in 0..w {
            let base = src[y * s.stride as usize + x];
            let mut p = if let Some(mask) = mask {
                let c = mask[y * m.stride as usize + x] as u32;
                if c == 0 {
                    base
                } else {
                    let alpha = ((base >> 24) * (255 - c) + 127) / 255;
                    if alpha == 0 {
                        0
                    } else {
                        (base & 0xffffff) | (alpha << 24)
                    }
                }
            } else {
                0
            };
            let (sx, sy) = (x as i32 - o.dx, y as i32 - o.dy);
            if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                let source = src[sy as usize * s.stride as usize + sx as usize];
                p = if let Some(mask) = mask {
                    let moving = selected(
                        source,
                        mask[sy as usize * m.stride as usize + sx as usize] as u32,
                    );
                    if moving >> 24 == 0 {
                        p
                    } else {
                        over(p, moving)
                    }
                } else {
                    source
                };
            }
            out[y * d.stride as usize + x] = p;
        }
    }
    abi::OK
}
unsafe extern "C" fn mask(
    o: *const abi::Move,
    s: *const abi::Coverage,
    out: *mut u8,
    len: usize,
) -> i32 {
    if o.is_null() || s.is_null() {
        return abi::ERROR;
    }
    let (o, s) = (&*o, &*s);
    if !valid(o)
        || o.kind == 0
        || !layout(s.width, s.height, s.stride, s.len)
        || len != s.width as usize * s.height as usize
        || !disjoint(s.data, s.len, out, len)
    {
        return abi::ERROR;
    }
    let source = std::slice::from_raw_parts(s.data, s.len);
    let dest = std::slice::from_raw_parts_mut(out, len);
    let (w, h) = (s.width as usize, s.height as usize);
    dest.fill(0);
    let (sx, sy) = (
        (-o.dx).max(0).min(w as i32) as usize,
        (-o.dy).max(0).min(h as i32) as usize,
    );
    let (dx, dy) = (
        o.dx.max(0).min(w as i32) as usize,
        o.dy.max(0).min(h as i32) as usize,
    );
    let (cols, rows) = ((w - sx).min(w - dx), (h - sy).min(h - dy));
    for row in 0..rows {
        dest[(dy + row) * w + dx..(dy + row) * w + dx + cols].copy_from_slice(
            &source
                [(sy + row) * s.stride as usize + sx..(sy + row) * s.stride as usize + sx + cols],
        );
    }
    abi::OK
}
static API: abi::MoveApi = abi::MoveApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::MoveApi>() as u32,
    pixels: Some(pixels),
    mask: Some(mask),
};
#[no_mangle]
pub extern "C" fn micopi_move_v1() -> *const abi::MoveApi {
    &API
}
#[cfg(test)]
mod tests {
    use super::*;
    fn view(data: &mut [u32], w: u32, h: u32, stride: u32) -> abi::Pixels {
        abi::Pixels {
            data: data.as_mut_ptr(),
            len: data.len(),
            width: w,
            height: h,
            stride,
            origin_x: 0,
            origin_y: 0,
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
    fn whole_layer_and_masks_match_forward_scatter_with_padding_and_clipping() {
        let (w, h, ss) = (7usize, 5usize, 9usize);
        let mut source: Vec<u32> = (0..ss * h).map(|i| i as u32 * 7919).collect();
        let original = source.clone();
        let bytes: Vec<u8> = (0..ss * h).map(|i| (i * 5) as u8).collect();
        for (dx, dy) in [(0, 0), (1, 2), (-3, -1), (7, 0), (-4096, 4096)] {
            let o = abi::Move { kind: 0, dx, dy };
            let mut dest = vec![0xabcdef01; ss * h];
            assert_eq!(
                unsafe {
                    pixels(
                        &o,
                        &view(&mut source, w as u32, h as u32, ss as u32),
                        &absent(),
                        &mut view(&mut dest, w as u32, h as u32, ss as u32),
                    )
                },
                abi::OK
            );
            let mut expected = vec![0u32; w * h];
            let mut expected_mask = vec![0u8; w * h];
            for y in 0..h {
                for x in 0..w {
                    let (tx, ty) = (x as i32 + dx, y as i32 + dy);
                    if tx >= 0 && ty >= 0 && (tx as usize) < w && (ty as usize) < h {
                        expected[ty as usize * w + tx as usize] = source[y * ss + x];
                        expected_mask[ty as usize * w + tx as usize] = bytes[y * ss + x];
                    }
                }
            }
            for y in 0..h {
                assert_eq!(&dest[y * ss..y * ss + w], &expected[y * w..y * w + w]);
                assert_eq!(&dest[y * ss + w..(y + 1) * ss], &[0xabcdef01; 2]);
            }
            let c = abi::Coverage {
                data: bytes.as_ptr(),
                len: bytes.len(),
                width: w as u32,
                height: h as u32,
                stride: ss as u32,
            };
            let mut out = vec![0; w * h];
            assert_eq!(
                unsafe {
                    mask(
                        &abi::Move { kind: 2, dx, dy },
                        &c,
                        out.as_mut_ptr(),
                        out.len(),
                    )
                },
                abi::OK
            );
            assert_eq!(out, expected_mask);
            assert_eq!(source, original);
        }
    }
    #[test]
    fn selected_pixels_clear_then_composite_from_original_with_fractional_edges() {
        let mut src = [0xff0000ff, 0xffff0000, 0x00112233, 0xff00ff00];
        let old = src;
        let coverage = [128u8, 255, 0, 0];
        let c = abi::Coverage {
            data: coverage.as_ptr(),
            len: 4,
            width: 4,
            height: 1,
            stride: 4,
        };
        let mut out = [0; 4];
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Move {
                        kind: 1,
                        dx: 1,
                        dy: 0,
                    },
                    &view(&mut src, 4, 1, 4),
                    &c,
                    &mut view(&mut out, 4, 1, 4),
                )
            },
            abi::OK
        );
        assert_eq!(out, [0x7f0000ff, 0x800000ff, 0xffff0000, 0xff00ff00]);
        assert_eq!(src, old);
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Move {
                        kind: 1,
                        dx: 0,
                        dy: 0,
                    },
                    &view(&mut src, 4, 1, 4),
                    &c,
                    &mut view(&mut out, 4, 1, 4),
                )
            },
            abi::OK
        );
        assert_eq!(out, old);
    }
    #[test]
    fn malformed_and_aliased_buffers_reject_before_output_writes() {
        let mut src = [0xff112233; 4];
        let s = view(&mut src, 2, 2, 2);
        let mut out = [77; 4];
        for o in [
            abi::Move {
                kind: 3,
                dx: 0,
                dy: 0,
            },
            abi::Move {
                kind: 0,
                dx: 4097,
                dy: 0,
            },
            abi::Move {
                kind: 1,
                dx: 1,
                dy: 0,
            },
        ] {
            assert_eq!(
                unsafe { pixels(&o, &s, &absent(), &mut view(&mut out, 2, 2, 2)) },
                abi::ERROR
            );
            assert_eq!(out, [77; 4]);
        }
        let mut alias = view(&mut src, 2, 2, 2);
        assert_eq!(
            unsafe {
                pixels(
                    &abi::Move {
                        kind: 0,
                        dx: 1,
                        dy: 0,
                    },
                    &s,
                    &absent(),
                    &mut alias,
                )
            },
            abi::ERROR
        );
        assert_eq!(src, [0xff112233; 4]);
    }
    #[test]
    #[ignore = "Explicit integer movement cost, excludes host/history/recording"]
    fn movement_cost() {
        let mut src = vec![0x80402010; 4096 * 4096];
        let mut out = vec![0; src.len()];
        let coverage = vec![128u8; src.len()];
        let c = abi::Coverage {
            data: coverage.as_ptr(),
            len: coverage.len(),
            width: 4096,
            height: 4096,
            stride: 4096,
        };
        let no_mask = absent();
        for kind in 0..=1 {
            let start = std::time::Instant::now();
            assert_eq!(
                unsafe {
                    pixels(
                        &abi::Move {
                            kind,
                            dx: 7,
                            dy: -3,
                        },
                        &view(&mut src, 4096, 4096, 4096),
                        if kind == 0 { &no_mask } else { &c },
                        &mut view(&mut out, 4096, 4096, 4096),
                    )
                },
                abi::OK
            );
            eprintln!(
                "4096² move kind {kind}: {:?}; no native allocation",
                start.elapsed()
            );
        }
    }
}
