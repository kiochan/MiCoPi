//! Bounded cold-path transfer kernels. No allocation, history or host calls.
use micopi_plugin_api as abi;
fn valid(p: &abi::Pixels) -> bool {
    !p.data.is_null()
        && p.width > 0
        && p.height > 0
        && p.width <= 4096
        && p.height <= 4096
        && p.origin_x == 0
        && p.origin_y == 0
        && p.stride >= p.width
        && p.stride <= 4096
        && p.len <= 4096 * 4096
        && (p.height as usize - 1) * p.stride as usize + p.width as usize <= p.len
}
pub(super) unsafe extern "C" fn process(
    options: *const abi::SelectionTransfer,
    source: *const abi::Pixels,
    mask: *const abi::Coverage,
    output: *mut abi::Pixels,
) -> i32 {
    if options.is_null() || source.is_null() || mask.is_null() || output.is_null() {
        return abi::ERROR;
    }
    let (o, s, m, d) = (&*options, &*source, &*mask, &mut *output);
    if !valid(s) || !valid(d) || !super::valid(m) || o.kind > 1 {
        return abi::ERROR;
    }
    // Reject overlapping storage before making any writes, including malformed callers.
    let Some(se) = (s.data as usize).checked_add(s.len * 4) else {
        return abi::ERROR;
    };
    let Some(de) = (d.data as usize).checked_add(d.len * 4) else {
        return abi::ERROR;
    };
    if (s.data as usize) < de && (d.data as usize) < se {
        return abi::ERROR;
    }
    if !m.data.is_null() {
        let Some(me) = (m.data as usize).checked_add(m.len) else {
            return abi::ERROR;
        };
        if (m.data as usize) < de && (d.data as usize) < me {
            return abi::ERROR;
        }
    }
    if o.kind == 0 {
        if o.x < 0
            || o.y < 0
            || o.x as u64 + d.width as u64 > s.width as u64
            || o.y as u64 + d.height as u64 > s.height as u64
            || (!m.data.is_null() && (m.width, m.height) != (s.width, s.height))
        {
            return abi::ERROR;
        }
        if m.data.is_null() {
            for y in 0..d.height as usize {
                std::ptr::copy_nonoverlapping(
                    s.data
                        .add((o.y as usize + y) * s.stride as usize + o.x as usize),
                    d.data.add(y * d.stride as usize),
                    d.width as usize,
                );
            }
            return abi::OK;
        }
        for y in 0..d.height as usize {
            for x in 0..d.width as usize {
                let sx = o.x as usize + x;
                let sy = o.y as usize + y;
                let p = *s.data.add(sy * s.stride as usize + sx);
                let c = *m.data.add(sy * m.stride as usize + sx) as u32;
                *d.data.add(y * d.stride as usize + x) = if c == 0 {
                    0
                } else {
                    (p & 0xffffff) | ((((p >> 24) * c + 127) / 255) << 24)
                };
            }
        }
    } else {
        if !m.data.is_null() {
            return abi::ERROR;
        }
        let sx = (-i64::from(o.x)).max(0).min(s.width as i64) as usize;
        let sy = (-i64::from(o.y)).max(0).min(s.height as i64) as usize;
        let dx = i64::from(o.x).max(0).min(d.width as i64) as usize;
        let dy = i64::from(o.y).max(0).min(d.height as i64) as usize;
        let w = (s.width as usize - sx).min(d.width as usize - dx);
        let h = (s.height as usize - sy).min(d.height as usize - dy);
        for y in 0..h {
            std::ptr::copy_nonoverlapping(
                s.data.add((sy + y) * s.stride as usize + sx),
                d.data.add((dy + y) * d.stride as usize + dx),
                w,
            );
        }
    }
    abi::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixels(data: &mut [u32], w: u32, h: u32, stride: u32) -> abi::Pixels {
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
    fn extraction_preserves_storage_and_scales_alpha_with_strided_coverage() {
        let mut source = vec![
            0x00112233, 0x80402010, 0xffffffff, 0, 0xffabcdee, 0x80aaaaaa, 0x44556677, 0,
        ];
        let before = source.clone();
        let mask = [255, 128, 0, 99, 255, 255, 255, 99];
        let mask = abi::Coverage {
            data: mask.as_ptr(),
            len: mask.len(),
            width: 3,
            height: 2,
            stride: 4,
        };
        let mut output = vec![9; 8];
        let status = unsafe {
            process(
                &abi::SelectionTransfer {
                    kind: 0,
                    x: 0,
                    y: 0,
                },
                &pixels(&mut source, 3, 2, 4),
                &mask,
                &mut pixels(&mut output, 3, 2, 4),
            )
        };
        assert_eq!(status, abi::OK);
        assert_eq!(
            output,
            vec![0x00112233, 0x40402010, 0, 9, 0xffabcdee, 0x80aaaaaa, 0x44556677, 9]
        );
        assert_eq!(source, before);
    }
    #[test]
    fn placement_matches_reference_for_all_edges_and_extreme_coordinates() {
        let mut source = vec![1, 2, 99, 3, 4, 99];
        for x in [-4096, -2, -1, 0, 1, 2, 3, 4096, i32::MIN, i32::MAX] {
            for y in [-2, -1, 0, 1, 2, i32::MIN, i32::MAX] {
                let mut output = vec![77; 8];
                assert_eq!(
                    unsafe {
                        process(
                            &abi::SelectionTransfer { kind: 1, x, y },
                            &pixels(&mut source, 2, 2, 3),
                            &absent(),
                            &mut pixels(&mut output, 3, 2, 4),
                        )
                    },
                    abi::OK
                );
                for dy in 0..2 {
                    for dx in 0..3 {
                        let (sx, sy) = (dx as i64 - x as i64, dy as i64 - y as i64);
                        assert_eq!(
                            output[dy * 4 + dx],
                            if (0..2).contains(&sx) && (0..2).contains(&sy) {
                                source[sy as usize * 3 + sx as usize]
                            } else {
                                77
                            }
                        );
                    }
                }
                assert_eq!(output[3], 77);
                assert_eq!(output[7], 77);
            }
        }
    }
    #[test]
    fn unrestricted_copy_preserves_hidden_rgb_and_crop_padding() {
        let mut source = vec![1, 0x00112233, 0x80402010, 99, 2, 0xffffffff, 0, 99];
        let mut output = vec![77; 6];
        assert_eq!(
            unsafe {
                process(
                    &abi::SelectionTransfer {
                        kind: 0,
                        x: 1,
                        y: 0,
                    },
                    &pixels(&mut source, 3, 2, 4),
                    &absent(),
                    &mut pixels(&mut output, 2, 2, 3),
                )
            },
            abi::OK
        );
        assert_eq!(output, vec![0x00112233, 0x80402010, 77, 0xffffffff, 0, 77]);
    }
    #[test]
    fn invalid_requests_and_aliases_reject_without_writing() {
        let mut source = vec![1; 4];
        let mut output = vec![99; 4];
        for (kind, x, y) in [(2, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, i32::MAX)] {
            assert_eq!(
                unsafe {
                    process(
                        &abi::SelectionTransfer { kind, x, y },
                        &pixels(&mut source, 2, 2, 2),
                        &absent(),
                        &mut pixels(&mut output, 2, 2, 2),
                    )
                },
                abi::ERROR
            );
            assert_eq!(output, vec![99; 4]);
        }
        let src = pixels(&mut source, 2, 2, 2);
        let mut dst = pixels(&mut source, 2, 2, 2);
        assert_eq!(
            unsafe {
                process(
                    &abi::SelectionTransfer {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &src,
                    &absent(),
                    &mut dst,
                )
            },
            abi::ERROR
        );
        dst = pixels(&mut output, 2, 2, 2);
        dst.len = 3;
        assert_eq!(
            unsafe {
                process(
                    &abi::SelectionTransfer {
                        kind: 0,
                        x: 0,
                        y: 0,
                    },
                    &src,
                    &absent(),
                    &mut dst,
                )
            },
            abi::ERROR
        );
        assert_eq!(output, vec![99; 4]);
    }
}

#[cfg(test)]
mod cost {
    use super::*;
    #[test]
    #[ignore = "Explicit cold transfer timing; excludes host hashing/history/render"]
    fn selection_transfer_cost() {
        for side in [1024, 4096] {
            let mut source = vec![0x80402010; side * side];
            let mut output = vec![0; source.len()];
            let source = abi::Pixels {
                data: source.as_mut_ptr(),
                len: source.len(),
                width: side as u32,
                height: side as u32,
                stride: side as u32,
                origin_x: 0,
                origin_y: 0,
            };
            let mut output = abi::Pixels {
                data: output.as_mut_ptr(),
                len: output.len(),
                width: side as u32,
                height: side as u32,
                stride: side as u32,
                origin_x: 0,
                origin_y: 0,
            };
            let mask = abi::Coverage {
                data: std::ptr::null(),
                len: 0,
                width: 0,
                height: 0,
                stride: 0,
            };
            let start = std::time::Instant::now();
            assert_eq!(
                unsafe {
                    process(
                        &abi::SelectionTransfer {
                            kind: 0,
                            x: 0,
                            y: 0,
                        },
                        &source,
                        &mask,
                        &mut output,
                    )
                },
                abi::OK
            );
            eprintln!(
                "extract {side}x{side}: {:?}, no native scratch",
                start.elapsed()
            );
        }
    }
}
