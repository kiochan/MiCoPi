//! Exact disk morphology and fixed-point, linear-time selection smoothing.
use micopi_plugin_api as abi;

pub unsafe extern "C" fn process(
    options: *const abi::SelectionEdges,
    source: *const abi::Coverage,
    output: *mut u8,
    len: usize,
) -> i32 {
    std::panic::catch_unwind(|| run(options, source, output, len)).unwrap_or(abi::ERROR)
}
unsafe fn run(
    options: *const abi::SelectionEdges,
    source: *const abi::Coverage,
    output: *mut u8,
    len: usize,
) -> i32 {
    if options.is_null() || source.is_null() || output.is_null() {
        return abi::ERROR;
    }
    let (o, s) = (&*options, &*source);
    if !super::valid(s)
        || s.data.is_null()
        || len != s.width as usize * s.height as usize
        || o.kind > 3
        || !(1..=256).contains(&o.radius)
        || o.edge > 1
    {
        return abi::ERROR;
    }
    let (w, h, r) = (s.width as usize, s.height as usize, o.radius as usize);
    let source = std::slice::from_raw_parts(s.data, s.len);
    let pixel = |x: usize, y: usize| source[y * s.stride as usize + x];
    let out = std::slice::from_raw_parts_mut(output, len);
    let first = pixel(0, 0);
    let (mut uniform, mut binary) = (true, true);
    for y in 0..h {
        for x in 0..w {
            let v = pixel(x, y);
            uniform &= v == first;
            binary &= matches!(v, 0 | 255);
        }
    }
    if uniform && (first == 0 || o.edge == 0 || o.kind == 0) {
        // Select All, empty masks and constant fractional masks need no scratch.
        out.fill(if o.kind == 3 {
            if first >= 128 {
                255
            } else {
                0
            }
        } else {
            first
        });
        return abi::OK;
    }
    if o.kind < 2 {
        if binary {
            morphology_binary(out, w, h, r, o.kind == 1, o.edge == 1, pixel);
        } else {
            morphology_gray(out, w, h, r, o.kind == 1, o.edge == 1, pixel);
        }
    } else {
        let mut a: Vec<u16> = (0..len)
            .map(|i| (pixel(i % w, i / w) as u16) << 8)
            .collect();
        let mut b = vec![0u16; len];
        let mut sums = vec![0u32; w];
        // Three boxes approximate a Gaussian with variance r*r-r/3. Fixed
        // point retains fractional coverage until the final quantization.
        for radius in [r - 1, r - 1, r] {
            if radius == 0 {
                continue;
            }
            blur_horizontal(&a, &mut b, w, h, radius, o.edge == 1);
            blur_vertical(&b, &mut a, w, h, radius, o.edge == 1, &mut sums);
        }
        for (v, a) in out.iter_mut().zip(a) {
            *v = if o.kind == 3 {
                if a >= 128 * 256 {
                    255
                } else {
                    0
                }
            } else {
                ((a as u32 + 128) >> 8) as u8
            };
        }
    }
    abi::OK
}
fn coordinate(i: isize, n: usize, zero: bool) -> Option<usize> {
    if i < 0 || i >= n as isize {
        if zero {
            None
        } else {
            Some(i.clamp(0, n as isize - 1) as usize)
        }
    } else {
        Some(i as usize)
    }
}
fn blur_horizontal(a: &[u16], b: &mut [u16], w: usize, h: usize, r: usize, zero: bool) {
    let divisor = (2 * r + 1) as u32;
    for y in 0..h {
        let get = |x| coordinate(x, w, zero).map_or(0, |x| a[y * w + x] as u32);
        let mut sum: u32 = (-(r as isize)..=r as isize).map(get).sum();
        for x in 0..w {
            b[y * w + x] = ((sum + divisor / 2) / divisor) as u16;
            sum -= get(x as isize - r as isize);
            sum += get(x as isize + r as isize + 1);
        }
    }
}
fn blur_vertical(
    a: &[u16],
    b: &mut [u16],
    w: usize,
    h: usize,
    r: usize,
    zero: bool,
    sums: &mut [u32],
) {
    let divisor = (2 * r + 1) as u32;
    sums.fill(0);
    for y in -(r as isize)..=r as isize {
        if let Some(y) = coordinate(y, h, zero) {
            for x in 0..w {
                sums[x] += a[y * w + x] as u32;
            }
        }
    }
    // Contiguous row access avoids a full-image strided column pass.
    for y in 0..h {
        let remove = coordinate(y as isize - r as isize, h, zero);
        let add = coordinate(y as isize + r as isize + 1, h, zero);
        for x in 0..w {
            b[y * w + x] = ((sums[x] + divisor / 2) / divisor) as u16;
            sums[x] -= remove.map_or(0, |y| a[y * w + x] as u32);
            sums[x] += add.map_or(0, |y| a[y * w + x] as u32);
        }
    }
}
// Lower envelopes of squared-distance parabolas, with exact integer outputs.
// f64 breakpoints are exact enough for the bounded (<=4096) integer coordinates.
fn distance_line(
    input: &[u32],
    output: &mut [u32],
    vertices: &mut [usize],
    cuts: &mut [f64],
    cap: u32,
) {
    let mut k = 0;
    vertices[0] = 0;
    cuts[0] = f64::NEG_INFINITY;
    cuts[1] = f64::INFINITY;
    for q in 1..input.len() {
        let intersection = |p: usize| {
            ((input[q] as f64 + (q * q) as f64) - (input[p] as f64 + (p * p) as f64))
                / (2. * (q - p) as f64)
        };
        let mut cut = intersection(vertices[k]);
        while cut <= cuts[k] {
            k -= 1;
            cut = intersection(vertices[k]);
        }
        k += 1;
        vertices[k] = q;
        cuts[k] = cut;
        cuts[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, out) in output.iter_mut().enumerate() {
        while cuts[k + 1] < q as f64 {
            k += 1;
        }
        let d = q.abs_diff(vertices[k]);
        *out = (input[vertices[k]] + (d * d) as u32).min(cap);
    }
}
fn morphology_binary(
    out: &mut [u8],
    w: usize,
    h: usize,
    r: usize,
    contract: bool,
    zero: bool,
    pixel: impl Fn(usize, usize) -> u8,
) {
    let cap = (r * r + 1) as u32;
    let mut grid = vec![cap; w * h];
    let n = w.max(h);
    let mut input = vec![0; n];
    let mut result = vec![0; n];
    let mut vertices = vec![0; n];
    let mut cuts = vec![0.; n + 1];
    for y in 0..h {
        for x in 0..w {
            input[x] = if (pixel(x, y) == 0) == contract {
                0
            } else {
                cap
            };
        }
        distance_line(&input[..w], &mut result[..w], &mut vertices, &mut cuts, cap);
        grid[y * w..(y + 1) * w].copy_from_slice(&result[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            input[y] = grid[y * w + x];
        }
        distance_line(&input[..h], &mut result[..h], &mut vertices, &mut cuts, cap);
        for y in 0..h {
            let mut d = result[y];
            if contract && zero {
                let border = (x + 1).min(w - x).min(y + 1).min(h - y);
                d = d.min((border * border) as u32);
            }
            out[y * w + x] = if (d <= (r * r) as u32) != contract {
                255
            } else {
                0
            };
        }
    }
}
fn morphology_gray(
    out: &mut [u8],
    w: usize,
    h: usize,
    r: usize,
    contract: bool,
    zero: bool,
    pixel: impl Fn(usize, usize) -> u8,
) {
    // An exact disk is the union of horizontal intervals. Sliding row maxima
    // preserve all fractional levels; no binary threshold or square substitute.
    let mut queue = std::collections::VecDeque::<(isize, u8)>::with_capacity(w + 2 * r);
    out.fill(0);
    for dy in -(r as isize)..=r as isize {
        let mut extent = 0;
        while (extent + 1) * (extent + 1) + dy.unsigned_abs().pow(2) <= r * r {
            extent += 1;
        }
        for y in 0..h {
            let source_y = coordinate(y as isize + dy, h, zero);
            let get = |x| {
                let v = source_y
                    .zip(coordinate(x, w, zero))
                    .map_or(0, |(y, x)| pixel(x, y));
                if contract {
                    255 - v
                } else {
                    v
                }
            };
            queue.clear();
            for i in -(extent as isize)..w as isize + extent as isize {
                let v = get(i);
                while queue.back().is_some_and(|q| q.1 <= v) {
                    queue.pop_back();
                }
                queue.push_back((i, v));
                let x = i - extent as isize;
                if x < 0 {
                    continue;
                }
                while queue.front().is_some_and(|q| q.0 < x - extent as isize) {
                    queue.pop_front();
                }
                out[y * w + x as usize] = out[y * w + x as usize].max(queue.front().unwrap().1);
            }
        }
    }
    if contract {
        for v in out {
            *v = 255 - *v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn execute(
        source: &[u8],
        w: usize,
        h: usize,
        stride: usize,
        kind: u32,
        radius: u32,
        edge: u32,
    ) -> Vec<u8> {
        let options = abi::SelectionEdges { kind, radius, edge };
        let mask = abi::Coverage {
            data: source.as_ptr(),
            len: source.len(),
            width: w as u32,
            height: h as u32,
            stride: stride as u32,
        };
        let mut out = vec![19; w * h];
        assert_eq!(
            unsafe { process(&options, &mask, out.as_mut_ptr(), out.len()) },
            abi::OK
        );
        out
    }
    fn at(input: &[u16], w: usize, h: usize, x: isize, y: isize, zero: bool) -> u16 {
        if zero && (x < 0 || y < 0 || x >= w as isize || y >= h as isize) {
            0
        } else {
            input[y.clamp(0, h as isize - 1) as usize * w + x.clamp(0, w as isize - 1) as usize]
        }
    }
    #[test]
    fn binary_and_fractional_disks_match_exhaustive_strided_reference() {
        let (w, h, stride) = (17, 13, 20);
        let mut seed = 183u32;
        for binary in [false, true] {
            let source: Vec<u8> = (0..stride * h)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    if binary {
                        if seed >> 31 == 0 {
                            0
                        } else {
                            255
                        }
                    } else {
                        (seed >> 24) as u8
                    }
                })
                .collect();
            let before = source.clone();
            let dense: Vec<u16> = (0..w * h)
                .map(|i| source[i / w * stride + i % w] as u16)
                .collect();
            for r in 1..=5 {
                for zero in [false, true] {
                    for kind in [0, 1] {
                        let mut expected = vec![0; w * h];
                        for y in 0..h {
                            for x in 0..w {
                                let mut v = if kind == 0 { 0 } else { 255 };
                                for dy in -r..=r {
                                    for dx in -r..=r {
                                        if dx * dx + dy * dy > r * r {
                                            continue;
                                        }
                                        let q = at(
                                            &dense,
                                            w,
                                            h,
                                            x as isize + dx,
                                            y as isize + dy,
                                            zero,
                                        ) as u8;
                                        v = if kind == 0 { v.max(q) } else { v.min(q) };
                                    }
                                }
                                expected[y * w + x] = v;
                            }
                        }
                        assert_eq!(
                            execute(&source, w, h, stride, kind, r as u32, zero as u32),
                            expected,
                            "binary={binary}, r={r}, zero={zero}, kind={kind}"
                        );
                        assert_eq!(source, before);
                    }
                }
            }
        }
    }
    #[test]
    fn radius_256_disk_is_exact_at_the_boundary_and_full_masks_respect_edge_policy() {
        let mut source = vec![0; 514];
        source[0] = 255;
        let out = execute(&source, 514, 1, 514, 0, 256, 0);
        for (i, &v) in out.iter().enumerate() {
            assert_eq!(v, if i <= 256 { 255 } else { 0 });
        }
        let source: Vec<_> = source.iter().map(|v| 255 - v).collect();
        let out = execute(&source, 514, 1, 514, 1, 256, 0);
        for (i, &v) in out.iter().enumerate() {
            assert_eq!(v, if i <= 256 { 0 } else { 255 });
        }
        for kind in 0..4 {
            assert!(execute(&[255; 35], 7, 5, 7, kind, 2, 0)
                .iter()
                .all(|v| *v == 255));
            assert!(execute(&[0; 35], 7, 5, 7, kind, 2, 0)
                .iter()
                .all(|v| *v == 0));
            for value in [32, 127, 128] {
                let expected = if kind == 3 {
                    if value >= 128 {
                        255
                    } else {
                        0
                    }
                } else {
                    value
                };
                assert!(execute(&[value; 35], 7, 5, 7, kind, 256, 0)
                    .iter()
                    .all(|v| *v == expected));
            }
        }
        let out = execute(&[255; 35], 7, 5, 7, 1, 1, 1);
        assert_eq!(out.iter().filter(|v| **v == 255).count(), 15);
    }
    #[test]
    fn feather_and_smooth_match_naive_fixed_point_box_convolutions() {
        let (w, h) = (11, 7);
        let source: Vec<u8> = (0..w * h).map(|i| ((i * 79 + 11) % 256) as u8).collect();
        for radius in [1, 2, 5, 16] {
            for edge in [0, 1] {
                let mut a: Vec<u16> = source.iter().map(|v| (*v as u16) << 8).collect();
                for r in [radius - 1, radius - 1, radius] {
                    for vertical in [false, true] {
                        let old = a.clone();
                        for y in 0..h {
                            for x in 0..w {
                                let sum: u32 = (-r..=r)
                                    .map(|d| {
                                        at(
                                            &old,
                                            w,
                                            h,
                                            x as isize + if vertical { 0 } else { d },
                                            y as isize + if vertical { d } else { 0 },
                                            edge == 1,
                                        ) as u32
                                    })
                                    .sum();
                                a[y * w + x] =
                                    ((sum + (2 * r + 1) as u32 / 2) / (2 * r + 1) as u32) as u16;
                            }
                        }
                    }
                }
                let expected: Vec<_> = a.iter().map(|v| ((*v as u32 + 128) >> 8) as u8).collect();
                assert_eq!(execute(&source, w, h, w, 2, radius as u32, edge), expected);
                let expected: Vec<_> = a
                    .iter()
                    .map(|v| if *v >= 32768 { 255 } else { 0 })
                    .collect();
                assert_eq!(execute(&source, w, h, w, 3, radius as u32, edge), expected);
            }
        }
    }
    #[test]
    fn invalid_buffers_and_options_are_rejected_before_writing() {
        let source = [32u8; 4];
        let mut out = [19; 4];
        let mut mask = abi::Coverage {
            data: source.as_ptr(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
        };
        for case in 0..7 {
            let mut options = abi::SelectionEdges {
                kind: 0,
                radius: 1,
                edge: 0,
            };
            mask.len = 4;
            mask.stride = 2;
            match case {
                0 => options.radius = 0,
                1 => options.radius = 257,
                2 => options.kind = 4,
                3 => options.edge = 2,
                4 => mask.len = 3,
                5 => mask.stride = 1,
                _ => mask.stride = 3,
            }
            assert_eq!(
                unsafe { process(&options, &mask, out.as_mut_ptr(), 4) },
                abi::ERROR
            );
            assert_eq!(out, [19; 4]);
        }
    }
    #[test]
    #[ignore = "explicit release-mode cost measurement"]
    fn selection_edges_cost() {
        for (w, h, kind, fractional) in [
            (4096, 4096, 0, false),
            (4096, 4096, 1, false),
            (4096, 4096, 2, false),
            (4096, 4096, 3, false),
            (1024, 1024, 0, true),
        ] {
            let source: Vec<_> = (0..w * h)
                .map(|i| {
                    if (w / 4..3 * w / 4).contains(&(i % w))
                        && (h / 4..3 * h / 4).contains(&(i / w))
                    {
                        if fractional {
                            128
                        } else {
                            255
                        }
                    } else {
                        0
                    }
                })
                .collect();
            let start = std::time::Instant::now();
            let out = execute(&source, w, h, w, kind, 32, 0);
            eprintln!(
                "{w}x{h} kind {kind} fractional={fractional}: {:?}; center={}",
                start.elapsed(),
                out[h / 2 * w + w / 2]
            );
        }
    }
}
