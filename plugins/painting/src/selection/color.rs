//! Read-only color sampling; output coverage is separate from source pixels.
use micopi_plugin_api as abi;
fn normalized(pixel: u32) -> u32 {
    if pixel >> 24 == 0 {
        0
    } else {
        pixel
    }
}
fn channels(p: u32) -> [u8; 4] {
    [p as u8, (p >> 8) as u8, (p >> 16) as u8, (p >> 24) as u8]
}
fn matches(a: u32, b: u32, t: u8) -> bool {
    channels(a)
        .into_iter()
        .zip(channels(b))
        .all(|(a, b)| a.abs_diff(b) <= t)
}
fn component(
    out: &mut [u8],
    w: usize,
    h: usize,
    seeds: impl Iterator<Item = usize>,
    eligible: impl Fn(usize) -> bool,
) {
    fn run(
        out: &mut [u8],
        w: usize,
        x: usize,
        y: usize,
        p: &impl Fn(usize) -> bool,
    ) -> (usize, usize) {
        let mut a = x;
        let mut b = x;
        while a > 0 && out[y * w + a - 1] == 0 && p(y * w + a - 1) {
            a -= 1;
        }
        while b + 1 < w && out[y * w + b + 1] == 0 && p(y * w + b + 1) {
            b += 1;
        }
        out[y * w + a..=y * w + b].fill(255);
        (a, b)
    }
    let mut stack = Vec::new();
    for seed in seeds {
        if out[seed] != 0 || !eligible(seed) {
            continue;
        }
        let (a, b) = run(out, w, seed % w, seed / w, &eligible);
        stack.push((seed / w, a, b));
        while let Some((y, a, b)) = stack.pop() {
            for ny in [y as isize - 1, y as isize + 1] {
                if ny < 0 || ny >= h as isize {
                    continue;
                }
                let y = ny as usize;
                let mut x = a;
                while x <= b {
                    let i = y * w + x;
                    if out[i] == 0 && eligible(i) {
                        let (a, b) = run(out, w, x, y, &eligible);
                        stack.push((y, a, b));
                        x = b + 1;
                    } else {
                        x += 1;
                    }
                }
            }
        }
    }
}
struct Palette {
    colors: Vec<u32>,
    axes: Vec<usize>,
    lo: [u8; 4],
    hi: [u8; 4],
    t: u8,
}
impl Palette {
    fn new(mut colors: Vec<u32>, t: u8) -> Self {
        colors.sort_unstable();
        colors.dedup();
        let mut lo = [255; 4];
        let mut hi = [0; 4];
        for &c in &colors {
            for (i, c) in channels(c).into_iter().enumerate() {
                lo[i] = lo[i].min(c);
                hi[i] = hi[i].max(c);
            }
        }
        let axes: Vec<_> = (0..4).filter(|&i| lo[i] != hi[i]).collect();
        fn build(colors: &mut [u32], depth: usize, axes: &[usize]) {
            if colors.len() < 2 {
                return;
            }
            let axis = axes[depth % axes.len()];
            let m = colors.len() / 2;
            colors.select_nth_unstable_by_key(m, |c| channels(*c)[axis]);
            let (a, b) = colors.split_at_mut(m);
            build(a, depth + 1, axes);
            build(&mut b[1..], depth + 1, axes);
        }
        if t > 0 && colors.len() > 1 {
            build(&mut colors, 0, &axes);
        }
        Self {
            colors,
            axes,
            lo,
            hi,
            t,
        }
    }
    fn contains(&self, p: u32) -> bool {
        if self.t == 0 {
            return self.colors.binary_search(&p).is_ok();
        }
        fn search(
            colors: &[u32],
            depth: usize,
            axes: &[usize],
            q: [u8; 4],
            lo: [u8; 4],
            hi: [u8; 4],
            t: u8,
        ) -> bool {
            if colors.is_empty()
                || (0..4).any(|i| q[i].saturating_add(t) < lo[i] || q[i].saturating_sub(t) > hi[i])
            {
                return false;
            }
            if (0..4).all(|i| q[i].abs_diff(lo[i]) <= t && q[i].abs_diff(hi[i]) <= t) {
                return true;
            }
            let m = colors.len() / 2;
            let c = channels(colors[m]);
            if q.into_iter().zip(c).all(|(a, b)| a.abs_diff(b) <= t) {
                return true;
            }
            if colors.len() == 1 {
                return false;
            }
            let axis = axes[depth % axes.len()];
            let mut lower = hi;
            lower[axis] = c[axis];
            let mut upper = lo;
            upper[axis] = c[axis];
            if q[axis] < c[axis] {
                search(&colors[..m], depth + 1, axes, q, lo, lower, t)
                    || search(&colors[m + 1..], depth + 1, axes, q, upper, hi, t)
            } else {
                search(&colors[m + 1..], depth + 1, axes, q, upper, hi, t)
                    || search(&colors[..m], depth + 1, axes, q, lo, lower, t)
            }
        }
        search(
            &self.colors,
            0,
            &self.axes,
            channels(p),
            self.lo,
            self.hi,
            self.t,
        )
    }
}
pub unsafe extern "C" fn select(
    options: *const abi::SelectionColor,
    source: *const abi::Pixels,
    previous: *const abi::Coverage,
    output: *mut u8,
    len: usize,
) -> i32 {
    std::panic::catch_unwind(|| generate(options, source, previous, output, len))
        .unwrap_or(abi::ERROR)
}
unsafe fn generate(
    options: *const abi::SelectionColor,
    source: *const abi::Pixels,
    previous: *const abi::Coverage,
    output: *mut u8,
    len: usize,
) -> i32 {
    if options.is_null() || source.is_null() || previous.is_null() || output.is_null() {
        return abi::ERROR;
    }
    let (o, s, p) = (&*options, &*source, &*previous);
    let (w, h) = (s.width as usize, s.height as usize);
    if w == 0
        || h == 0
        || w > 4096
        || h > 4096
        || s.data.is_null()
        || s.origin_x != 0
        || s.origin_y != 0
        || s.stride < s.width
        || s.stride > 4096
        || s.len > 4096 * 4096
        || (h - 1) * s.stride as usize + w > s.len
        || len != w * h
        || !super::valid(p)
        || (!p.data.is_null() && (p.width, p.height) != (s.width, s.height))
        || o.kind > 2
        || o.mode > 3
        || o.contiguous > 1
        || o.tolerance > 255
        || (o.kind == 0 && (o.x >= s.width || o.y >= s.height))
        || (o.kind > 0 && (p.data.is_null() || o.mode != 1))
    {
        return abi::ERROR;
    }
    let source = std::slice::from_raw_parts(s.data, s.len);
    let previous = if p.data.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(p.data, p.len)
    };
    let pixel = |i: usize| normalized(source[(i / w) * s.stride as usize + i % w]);
    let old = |i: usize| {
        if previous.is_empty() {
            0
        } else {
            previous[(i / w) * p.stride as usize + i % w]
        }
    };
    let out = std::slice::from_raw_parts_mut(output, len);
    if o.kind == 0 {
        out.fill(0);
        let seed = o.y as usize * w + o.x as usize;
        let target = pixel(seed);
        let eligible = |i| matches(pixel(i), target, o.tolerance as u8);
        if o.contiguous != 0 {
            component(out, w, h, std::iter::once(seed), eligible);
        } else {
            for (i, v) in out.iter_mut().enumerate() {
                *v = if eligible(i) { 255 } else { 0 };
            }
        }
        for (i, v) in out.iter_mut().enumerate() {
            *v = match o.mode {
                0 => *v,
                1 => old(i).max(*v),
                2 => old(i).saturating_sub(*v),
                _ => old(i).min(*v),
            };
        }
    } else {
        if !(0..len).any(|i| old(i) > 0) {
            return abi::ERROR;
        }
        if (0..len).all(|i| old(i) > 0) {
            for (i, v) in out.iter_mut().enumerate() {
                *v = old(i);
            }
            return abi::OK;
        }
        let palette = Palette::new(
            (0..len).filter(|&i| old(i) > 0).map(pixel).collect(),
            o.tolerance as u8,
        );
        // Small direct-mapped memoization for repeated source colors; no image copy.
        let cache = std::cell::RefCell::new(vec![0u64; 4096]);
        let eligible = |i| {
            let color = pixel(i);
            let index = (color.wrapping_mul(0x9e3779b9) >> 20) as usize;
            let entry = cache.borrow()[index];
            if entry & 3 != 0 && entry >> 2 == color as u64 {
                return entry & 3 == 2;
            }
            let yes = palette.contains(color);
            cache.borrow_mut()[index] = (color as u64) << 2 | if yes { 2 } else { 1 };
            yes
        };
        out.fill(0);
        if o.kind == 1 {
            component(out, w, h, (0..len).filter(|&i| old(i) > 0), eligible);
        } else {
            for (i, v) in out.iter_mut().enumerate() {
                *v = if eligible(i) { 255 } else { 0 };
            }
        }
        for (i, v) in out.iter_mut().enumerate() {
            if old(i) > 0 {
                *v = old(i);
            }
        }
    }
    abi::OK
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(kind: u32) -> abi::SelectionColor {
        abi::SelectionColor {
            kind,
            mode: if kind == 0 { 0 } else { 1 },
            x: 0,
            y: 0,
            tolerance: 0,
            contiguous: 1,
        }
    }
    fn execute(
        pixels: &mut [u32],
        w: u32,
        h: u32,
        stride: u32,
        mask: &[u8],
        o: &abi::SelectionColor,
    ) -> Vec<u8> {
        let before = pixels.to_vec();
        let source = abi::Pixels {
            data: pixels.as_mut_ptr(),
            len: pixels.len(),
            width: w,
            height: h,
            stride,
            origin_x: 0,
            origin_y: 0,
        };
        let previous = if mask.is_empty() {
            abi::Coverage::default()
        } else {
            abi::Coverage {
                data: mask.as_ptr(),
                len: mask.len(),
                width: w,
                height: h,
                stride: w,
            }
        };
        let mut out = vec![17; (w * h) as usize];
        assert_eq!(
            unsafe { select(o, &source, &previous, out.as_mut_ptr(), out.len()) },
            abi::OK
        );
        assert_eq!(pixels, before);
        out
    }
    fn random(seed: &mut u32) -> u32 {
        *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *seed
    }
    #[test]
    fn palette_tree_is_exact_against_exhaustive_rgba_search() {
        let mut seed = 781;
        for n in [1, 2, 17, 257, 2049] {
            for t in [0, 1, 31, 127, 255] {
                for narrow in [false, true] {
                    let colors: Vec<_> = (0..n)
                        .map(|_| {
                            normalized(if narrow {
                                0xff123400 | (random(&mut seed) & 255)
                            } else {
                                random(&mut seed)
                            })
                        })
                        .collect();
                    let palette = Palette::new(colors.clone(), t);
                    for i in 0..1024 {
                        let p = if i < n {
                            colors[i]
                        } else {
                            normalized(random(&mut seed))
                        };
                        assert_eq!(
                            palette.contains(p),
                            colors.iter().any(|c| matches(*c, p, t)),
                            "n={n} t={t} p={p:x}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn strided_wand_matches_breadth_first_reference_and_combines_fractional_masks() {
        let (w, h, stride) = (31usize, 23usize, 34usize);
        let mut seed = 912;
        let mut source: Vec<_> = (0..h * stride)
            .map(|_| {
                [0xff001122, 0xff051628, 0xff8899aa, 0x00123456]
                    [(random(&mut seed) >> 28) as usize % 4]
            })
            .collect();
        let mask: Vec<_> = (0..w * h)
            .map(|_| (random(&mut seed) >> 24) as u8)
            .collect();
        for contiguous in [0, 1] {
            for tolerance in [0, 32, 255] {
                let mut o = options(0);
                o.x = 7;
                o.y = 8;
                o.contiguous = contiguous;
                o.tolerance = tolerance;
                let eligible = |i: usize| {
                    matches(
                        normalized(source[(i / w) * stride + i % w]),
                        normalized(source[8 * stride + 7]),
                        tolerance as u8,
                    )
                };
                let mut reference = vec![0u8; w * h];
                if contiguous == 0 {
                    for (i, v) in reference.iter_mut().enumerate() {
                        *v = if eligible(i) { 255 } else { 0 };
                    }
                } else {
                    let mut queue = std::collections::VecDeque::from([8 * w + 7]);
                    reference[8 * w + 7] = 255;
                    while let Some(i) = queue.pop_front() {
                        for (x, y) in [
                            (i as isize % w as isize - 1, i / w),
                            (i as isize % w as isize + 1, i / w),
                            (i as isize % w as isize, (i / w).wrapping_sub(1)),
                            (i as isize % w as isize, i / w + 1),
                        ] {
                            if x < 0 || x as usize >= w || y >= h {
                                continue;
                            }
                            let j = y * w + x as usize;
                            if reference[j] == 0 && eligible(j) {
                                reference[j] = 255;
                                queue.push_back(j);
                            }
                        }
                    }
                }
                for mode in 0..4 {
                    o.mode = mode;
                    let expected: Vec<_> = reference
                        .iter()
                        .zip(&mask)
                        .map(|(&v, &old)| match mode {
                            0 => v,
                            1 => v.max(old),
                            2 => old.saturating_sub(v),
                            _ => old.min(v),
                        })
                        .collect();
                    assert_eq!(
                        execute(&mut source, w as u32, h as u32, stride as u32, &mask, &o),
                        expected
                    );
                }
            }
        }
    }
    #[test]
    fn grow_is_connected_similar_is_global_and_neither_chains_tolerance() {
        let mut source = [0xff000000, 0xff101010, 0xff202020, 0xffffffff, 0xff000000];
        let mask = [128, 0, 0, 0, 0];
        let mut o = options(1);
        o.tolerance = 16;
        assert_eq!(
            execute(&mut source, 5, 1, 5, &mask, &o),
            [128, 255, 0, 0, 0]
        );
        o.kind = 2;
        assert_eq!(
            execute(&mut source, 5, 1, 5, &mask, &o),
            [128, 255, 0, 0, 255]
        );
        let mask = [128, 0, 0, 0, 32];
        o.kind = 1;
        assert_eq!(
            execute(&mut source, 5, 1, 5, &mask, &o),
            [128, 255, 0, 0, 32]
        );
        let full = [1, 32, 128, 255, 3];
        assert_eq!(execute(&mut source, 5, 1, 5, &full, &o), full);
    }
    #[test]
    fn zero_alpha_ignores_hidden_rgb_but_alpha_is_part_of_tolerance() {
        let mut source = [0x00112233, 0x00abcdef, 0x01000000, 0xff000000];
        let mut o = options(0);
        assert_eq!(execute(&mut source, 4, 1, 4, &[], &o), [255, 255, 0, 0]);
        o.tolerance = 1;
        assert_eq!(execute(&mut source, 4, 1, 4, &[], &o), [255, 255, 255, 0]);
    }
    #[test]
    fn malformed_buffers_and_options_fail_without_writes() {
        let mut source = [0xff112233; 4];
        let mut pixels = abi::Pixels {
            data: source.as_mut_ptr(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
            origin_x: 0,
            origin_y: 0,
        };
        let mut o;
        let coverage = abi::Coverage::default();
        let mut out = [17; 4];
        for case in 0..8 {
            o = options(0);
            pixels.len = 4;
            pixels.stride = 2;
            pixels.origin_x = 0;
            match case {
                0 => o.x = 2,
                1 => o.tolerance = 256,
                2 => o.mode = 4,
                3 => o.kind = 3,
                4 => pixels.len = 3,
                5 => pixels.stride = 1,
                6 => pixels.origin_x = 1,
                _ => o.kind = 1,
            }
            assert_eq!(
                unsafe { select(&o, &pixels, &coverage, out.as_mut_ptr(), 4) },
                abi::ERROR
            );
            assert_eq!(out, [17; 4]);
            assert_eq!(source, [0xff112233; 4]);
        }
        o = options(1);
        let empty = [0; 4];
        pixels.origin_x = 0;
        let coverage = abi::Coverage {
            data: empty.as_ptr(),
            len: 4,
            width: 2,
            height: 2,
            stride: 2,
        };
        assert_eq!(
            unsafe { select(&o, &pixels, &coverage, out.as_mut_ptr(), 4) },
            abi::ERROR
        );
        assert_eq!(out, [17; 4]);
    }
    #[test]
    #[ignore = "explicit release-mode cost measurement"]
    fn color_selection_cost() {
        let (w, h) = (2048, 2048);
        let mut seed = 991;
        let mut source: Vec<_> = (0..w * h)
            .map(|_| 0xff000000 | (random(&mut seed) & 0xffffff))
            .collect();
        let mask: Vec<_> = (0..w * h)
            .map(|i| if i % w < w / 2 { 255 } else { 0 })
            .collect();
        for kind in [0, 1, 2] {
            let mut o = options(kind);
            o.tolerance = 4;
            let start = std::time::Instant::now();
            let out = execute(
                &mut source,
                w,
                h,
                w,
                if kind == 0 { &[] } else { &mask },
                &o,
            );
            eprintln!(
                "2048x2048 kind {kind}: {:?}, selected {}",
                start.elapsed(),
                out.iter().filter(|v| **v > 0).count()
            );
        }
    }
}
