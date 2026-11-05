//! Allocation-free merged-image trim bounds; structural cropping stays in the host.
use super::geometry::{disjoint, layout};
use micopi_plugin_api as abi;
fn matches(pixel: u32, reference: u32, o: &abi::Trim) -> bool {
    if o.mode == 0 {
        pixel >> 24 <= o.tolerance
    } else if pixel >> 24 == 0 && reference >> 24 == 0 {
        true
    } else {
        [0, 8, 16, 24]
            .into_iter()
            .all(|s| ((pixel >> s) & 255).abs_diff((reference >> s) & 255) <= o.tolerance)
    }
}
pub(super) unsafe extern "C" fn bounds(
    o: *const abi::Trim,
    s: *const abi::Pixels,
    out: *mut abi::TrimBounds,
) -> i32 {
    if o.is_null()
        || s.is_null()
        || out.is_null()
        || out as usize % std::mem::align_of::<abi::TrimBounds>() != 0
    {
        return abi::ERROR;
    }
    let (o, s) = (&*o, &*s);
    if o.mode > 2
        || o.tolerance > 255
        || o.sides == 0
        || o.sides & !15 != 0
        || !layout(s.width, s.height, s.stride, s.len)
        || (s.origin_x, s.origin_y) != (0, 0)
        || s.data as usize % std::mem::align_of::<u32>() != 0
        || !disjoint(
            s.data.cast::<u8>(),
            s.len * 4,
            out.cast::<u8>(),
            std::mem::size_of::<abi::TrimBounds>(),
        )
    {
        return abi::ERROR;
    }
    let pixels = std::slice::from_raw_parts(s.data, s.len);
    let (w, h, stride) = (s.width as usize, s.height as usize, s.stride as usize);
    let reference = if o.mode == 2 {
        pixels[(h - 1) * stride + w - 1]
    } else {
        pixels[0]
    };
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for (x, &pixel) in pixels[y * stride..y * stride + w].iter().enumerate() {
            if !matches(pixel, reference, o) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if (x0, y0, x1, y1) == (0, 0, w, h) {
            break;
        }
    }
    let b = if x1 == 0 {
        abi::TrimBounds::default()
    } else {
        if o.sides & 8 == 0 {
            x0 = 0;
        }
        if o.sides & 1 == 0 {
            y0 = 0;
        }
        if o.sides & 2 == 0 {
            x1 = w;
        }
        if o.sides & 4 == 0 {
            y1 = h;
        }
        abi::TrimBounds {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        }
    };
    *out = b;
    abi::OK
}
static API: abi::TrimApi = abi::TrimApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::TrimApi>() as u32,
    bounds: Some(bounds),
};
#[no_mangle]
pub extern "C" fn micopi_trim_v1() -> *const abi::TrimApi {
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
    #[test]
    fn native_bounds_match_independent_edge_stripping_for_every_side_and_mode() {
        for (w, h) in [(1, 1), (1, 7), (9, 1), (9, 7), (65, 33)] {
            let mut source = vec![0xff334455; (w + 2) * h];
            for y in h / 3..(h - h / 3).max(h / 3 + 1) {
                for x in w / 3..(w - w / 3).max(w / 3 + 1) {
                    source[y * (w + 2) + x] = 0x80112233;
                }
            }
            for mode in 0..=2 {
                for tolerance in [0, 20, 127, 255] {
                    for sides in 1..=15 {
                        let mut data = source.clone();
                        if mode == 0 {
                            for p in &mut data {
                                if *p == 0xff334455 {
                                    *p = 0x00112233;
                                }
                            }
                        }
                        let before = data.clone();
                        let o = abi::Trim {
                            mode,
                            tolerance,
                            sides,
                        };
                        let mut got = abi::TrimBounds::default();
                        assert_eq!(
                            unsafe {
                                bounds(
                                    &o,
                                    &view(&mut data, w as u32, h as u32, (w + 2) as u32),
                                    &mut got,
                                )
                            },
                            abi::OK
                        );
                        let reference = if mode == 2 {
                            data[(h - 1) * (w + 2) + w - 1]
                        } else {
                            data[0]
                        };
                        let keep = |x: usize, y: usize| {
                            let p = data[y * (w + 2) + x];
                            if mode == 0 {
                                (p >> 24) > tolerance
                            } else {
                                !(p >> 24 == 0 && reference >> 24 == 0)
                                    && [0, 8, 16, 24].iter().any(|c| {
                                        ((p >> *c) & 255u32).abs_diff((reference >> *c) & 255u32)
                                            > tolerance
                                    })
                            }
                        };
                        let any = (0..h).any(|y| (0..w).any(|x| keep(x, y)));
                        let mut expected = abi::TrimBounds::default();
                        if any {
                            let (mut top, mut left, mut bottom, mut right) = (0, 0, h, w);
                            if sides & 1 != 0 {
                                while (0..w).all(|x| !keep(x, top)) {
                                    top += 1;
                                }
                            }
                            if sides & 4 != 0 {
                                while (0..w).all(|x| !keep(x, bottom - 1)) {
                                    bottom -= 1;
                                }
                            }
                            if sides & 8 != 0 {
                                while (0..h).all(|y| !keep(left, y)) {
                                    left += 1;
                                }
                            }
                            if sides & 2 != 0 {
                                while (0..h).all(|y| !keep(right - 1, y)) {
                                    right -= 1;
                                }
                            }
                            expected = abi::TrimBounds {
                                x: left as u32,
                                y: top as u32,
                                width: (right - left) as u32,
                                height: (bottom - top) as u32,
                            };
                        }
                        assert_eq!(got, expected, "{w} {h} {mode} {tolerance} {sides}");
                        assert_eq!(data, before);
                    }
                }
            }
        }
    }
    #[test]
    fn threshold_corners_and_hidden_rgb_have_explicit_semantics() {
        let mut data = [0x00ff0000, 0x01000000, 0x02000000];
        let mut out = abi::TrimBounds::default();
        assert_eq!(
            unsafe {
                bounds(
                    &abi::Trim {
                        mode: 0,
                        tolerance: 1,
                        sides: 15,
                    },
                    &view(&mut data, 3, 1, 3),
                    &mut out,
                )
            },
            abi::OK
        );
        assert_eq!(
            out,
            abi::TrimBounds {
                x: 2,
                y: 0,
                width: 1,
                height: 1
            }
        );
        data = [0x00ff0000, 0x0000ff00, 0x00abcdef];
        assert_eq!(
            unsafe {
                bounds(
                    &abi::Trim {
                        mode: 1,
                        tolerance: 0,
                        sides: 15,
                    },
                    &view(&mut data, 3, 1, 3),
                    &mut out,
                )
            },
            abi::OK
        );
        assert_eq!(out, abi::TrimBounds::default());
        data = [0xff102030, 0xff112031, 0xff909090];
        assert_eq!(
            unsafe {
                bounds(
                    &abi::Trim {
                        mode: 1,
                        tolerance: 1,
                        sides: 15,
                    },
                    &view(&mut data, 3, 1, 3),
                    &mut out,
                )
            },
            abi::OK
        );
        assert_eq!(out.x, 2);
        assert_eq!(out.width, 1);
        assert_eq!(
            unsafe {
                bounds(
                    &abi::Trim {
                        mode: 2,
                        tolerance: 0,
                        sides: 15,
                    },
                    &view(&mut data, 3, 1, 3),
                    &mut out,
                )
            },
            abi::OK
        );
        assert_eq!(out.width, 2);
    }
    #[test]
    fn invalid_buffers_and_options_do_not_write_or_alias_source() {
        let mut data = [0xff112233; 4];
        let source = view(&mut data, 2, 2, 2);
        let sentinel = abi::TrimBounds {
            x: 99,
            y: 99,
            width: 99,
            height: 99,
        };
        for o in [
            abi::Trim {
                mode: 3,
                tolerance: 0,
                sides: 15,
            },
            abi::Trim {
                mode: 0,
                tolerance: 256,
                sides: 15,
            },
            abi::Trim {
                mode: 0,
                tolerance: 0,
                sides: 0,
            },
            abi::Trim {
                mode: 0,
                tolerance: 0,
                sides: 16,
            },
        ] {
            let mut out = sentinel;
            assert_eq!(unsafe { bounds(&o, &source, &mut out) }, abi::ERROR);
            assert_eq!(out, sentinel);
        }
        let o = abi::Trim {
            mode: 0,
            tolerance: 0,
            sides: 15,
        };
        assert_eq!(
            unsafe { bounds(&o, &source, data.as_mut_ptr().cast()) },
            abi::ERROR
        );
        assert_eq!(data, [0xff112233; 4]);
        let mut bad = view(&mut data, 2, 2, 2);
        bad.len = 3;
        let mut out = sentinel;
        assert_eq!(unsafe { bounds(&o, &bad, &mut out) }, abi::ERROR);
        assert_eq!(out, sentinel);
        assert_eq!(
            unsafe { bounds(&o, std::ptr::null(), &mut out) },
            abi::ERROR
        );
    }
    #[test]
    #[ignore = "Explicit native bounds cost, excludes composition/history/GUI"]
    fn trim_cost() {
        let mut data = vec![0u32; 4096 * 4096];
        for y in 512..3584 {
            data[y * 4096 + 512..y * 4096 + 3584].fill(0xff112233);
        }
        let source = view(&mut data, 4096, 4096, 4096);
        for mode in 0..=2 {
            let start = std::time::Instant::now();
            let mut out = abi::TrimBounds::default();
            assert_eq!(
                unsafe {
                    bounds(
                        &abi::Trim {
                            mode,
                            tolerance: 0,
                            sides: 15,
                        },
                        &source,
                        &mut out,
                    )
                },
                abi::OK
            );
            assert_eq!(
                out,
                abi::TrimBounds {
                    x: 512,
                    y: 512,
                    width: 3072,
                    height: 3072
                }
            );
            eprintln!(
                "4096² trim mode {mode}: {:?}, no native allocation",
                start.elapsed()
            );
        }
    }
}
