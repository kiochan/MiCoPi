//! Scanline fill and premultiplied-alpha gradients on host-scoped pixels.
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    let da = dst >> 24;
    let oa = a + (da * (255 - a) + 127) / 255;
    if oa == 0 {
        return dst;
    }
    let mut out = oa << 24;
    for shift in [0, 8, 16] {
        let c =
            (((src >> shift) & 255) * a * 255 + ((dst >> shift) & 255) * da * (255 - a) + oa * 127)
                / (oa * 255);
        out |= c.min(255) << shift;
    }
    out
}
fn matches(a: u32, b: u32, t: u32) -> bool {
    let aa = a >> 24;
    let ba = b >> 24;
    aa.abs_diff(ba) <= t
        && [0, 8, 16]
            .into_iter()
            .all(|s| (((a >> s) & 255) * aa / 255).abs_diff(((b >> s) & 255) * ba / 255) <= t)
}
/// Match the seed across the whole active layer without a visited bitmap.
pub fn fill_all(p: &mut [u32], seed: usize, color: u32, tolerance: u32) {
    if color >> 24 == 0 {
        return;
    }
    let target = p[seed];
    if tolerance == 0 && target == color {
        return;
    }
    for pixel in p {
        if matches(*pixel, target, tolerance) {
            *pixel = over(*pixel, color);
        }
    }
}

#[derive(Clone, Copy)]
pub enum Spread {
    Pad,
    Repeat,
    Reflect,
}
impl Spread {
    fn amount(self, t: f32) -> f32 {
        match self {
            Self::Pad => t.clamp(0., 1.),
            Self::Repeat => t.rem_euclid(1.),
            Self::Reflect => {
                let phase = t.rem_euclid(2.);
                if phase > 1. {
                    2. - phase
                } else {
                    phase
                }
            }
        }
    }
}
pub fn fill(p: &mut [u32], w: usize, h: usize, x: usize, y: usize, color: u32, tolerance: u32) {
    if color >> 24 == 0 {
        return;
    }
    let target = p[y * w + x];
    if tolerance == 0 && target == color {
        return;
    }
    let mut seen = vec![0u8; (w * h).div_ceil(8)];
    let mut stack = Vec::<(u16, u16, u16)>::new();
    fn eligible(p: &[u32], seen: &[u8], i: usize, target: u32, t: u32) -> bool {
        seen[i / 8] & (1 << (i % 8)) == 0 && matches(p[i], target, t)
    }
    fn run(
        p: &mut [u32],
        seen: &mut [u8],
        w: usize,
        x: usize,
        y: usize,
        target: u32,
        t: u32,
        c: u32,
    ) -> (usize, usize) {
        let mut l = x;
        let mut r = x;
        while l > 0 && eligible(p, seen, y * w + l - 1, target, t) {
            l -= 1;
        }
        while r + 1 < w && eligible(p, seen, y * w + r + 1, target, t) {
            r += 1;
        }
        for x in l..=r {
            let i = y * w + x;
            seen[i / 8] |= 1 << (i % 8);
            p[i] = over(p[i], c);
        }
        (l, r)
    }
    let (l, r) = run(p, &mut seen, w, x, y, target, tolerance, color);
    stack.push((y as u16, l as u16, r as u16));
    while let Some((y, l, r)) = stack.pop() {
        for ny in [y as isize - 1, y as isize + 1] {
            if ny < 0 || ny >= h as isize {
                continue;
            }
            let ny = ny as usize;
            let mut x = l as usize;
            while x <= r as usize {
                if eligible(p, &seen, ny * w + x, target, tolerance) {
                    let (a, b) = run(p, &mut seen, w, x, ny, target, tolerance, color);
                    stack.push((ny as u16, a as u16, b as u16));
                    x = b + 1;
                } else {
                    x += 1;
                }
            }
        }
    }
}
pub fn gradient(
    p: &mut [u32],
    w: usize,
    from: [f32; 2],
    to: [f32; 2],
    a: u32,
    b: u32,
    radial: bool,
    spread: Spread,
) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let d = dx * dx + dy * dy;
    let aa = (a >> 24) as f32;
    let ba = (b >> 24) as f32;
    for (i, pixel) in p.iter_mut().enumerate() {
        let x = (i % w) as f32 + 0.5 - from[0];
        let y = (i / w) as f32 + 0.5 - from[1];
        let t = if radial {
            ((x * x + y * y) / d).sqrt()
        } else {
            (x * dx + y * dy) / d
        };
        let t = spread.amount(t);
        let alpha = aa + (ba - aa) * t;
        let mut c = (alpha.round() as u32) << 24;
        if alpha > 0. {
            for s in [0, 8, 16] {
                let ap = ((a >> s) & 255) as f32 * aa;
                let bp = ((b >> s) & 255) as f32 * ba;
                c |= (((ap + (bp - ap) * t) / alpha).round() as u32).min(255) << s;
            }
        }
        *pixel = over(*pixel, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fill_obeys_connectivity_tolerance_alpha_and_transparent_rgb() {
        let mut p = vec![0, 0x00ffffff, 0xff000000, 0, 0xff000000, 0];
        fill(&mut p, 3, 2, 0, 0, 0x80ff0000, 0);
        assert_eq!(
            p,
            vec![0x80ff0000, 0x80ff0000, 0xff000000, 0x80ff0000, 0xff000000, 0]
        );
        let mut p = vec![0xff202020, 0xff252525, 0xff404040];
        fill(&mut p, 3, 1, 0, 0, 0xffabcdef, 5);
        assert_eq!(p, vec![0xffabcdef, 0xffabcdef, 0xff404040]);
    }
    #[test]
    fn scanline_matches_independent_breadth_first_reference() {
        for seed in 0..40u32 {
            let mut rng = seed + 1;
            let original: Vec<u32> = (0..117)
                .map(|_| {
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if rng >> 30 == 0 {
                        0xff000000
                    } else {
                        0
                    }
                })
                .collect();
            let (x, y) = (seed as usize % 13, seed as usize % 9);
            let mut expected = original.clone();
            let mut seen = vec![false; 117];
            let mut queue = std::collections::VecDeque::from([(x, y)]);
            while let Some((x, y)) = queue.pop_front() {
                let i = y * 13 + x;
                if seen[i] || original[i] != original[(seed as usize % 9) * 13 + seed as usize % 13]
                {
                    continue;
                }
                seen[i] = true;
                expected[i] = over(expected[i], 0x804488ff);
                for (nx, ny) in [
                    (x as isize - 1, y as isize),
                    (x as isize + 1, y as isize),
                    (x as isize, y as isize - 1),
                    (x as isize, y as isize + 1),
                ] {
                    if (0..13).contains(&nx) && (0..9).contains(&ny) {
                        queue.push_back((nx as usize, ny as usize));
                    }
                }
            }
            let mut actual = original;
            fill(&mut actual, 13, 9, x, y, 0x804488ff, 0);
            assert_eq!(actual, expected);
        }
    }
    #[test]
    fn gradients_have_exact_endpoints_and_no_transparent_color_fringe() {
        let mut p = vec![0; 3];
        gradient(
            &mut p,
            3,
            [0.5, 0.5],
            [2.5, 0.5],
            0xffff0000,
            0xff0000ff,
            false,
            Spread::Pad,
        );
        assert_eq!(p, vec![0xffff0000, 0xff800080, 0xff0000ff]);
        let mut p = vec![0; 3];
        gradient(
            &mut p,
            3,
            [0.5, 0.5],
            [2.5, 0.5],
            0xffff0000,
            0x000000ff,
            false,
            Spread::Pad,
        );
        assert_eq!(p, vec![0xffff0000, 0x80ff0000, 0]);
        let mut p = vec![0; 9];
        gradient(
            &mut p,
            3,
            [1.5, 1.5],
            [2.5, 1.5],
            0xffffffff,
            0xff000000,
            true,
            Spread::Pad,
        );
        assert_eq!(p[4], 0xffffffff);
        assert_eq!(p[1], 0xff000000);
    }
    #[test]
    fn global_fill_crosses_barriers_and_uses_seed_tolerance() {
        let original = vec![0xff202020, 0xff000000, 0xff252525, 0xff000000, 0xff282828];
        let mut connected = original.clone();
        fill(&mut connected, 5, 1, 0, 0, 0xffff0000, 5);
        assert_eq!(
            connected,
            vec![0xffff0000, 0xff000000, 0xff252525, 0xff000000, 0xff282828]
        );
        let mut all = original;
        fill_all(&mut all, 0, 0xffff0000, 5);
        assert_eq!(
            all,
            vec![0xffff0000, 0xff000000, 0xffff0000, 0xff000000, 0xff282828]
        );
        let mut alpha = vec![0x00ffffff, 0xff000000, 0x0000ff00];
        fill_all(&mut alpha, 0, 0x80ff0000, 0);
        assert_eq!(alpha, vec![0x80ff0000, 0xff000000, 0x80ff0000]);
    }
    #[test]
    fn gradient_spreads_handle_negative_phase_and_period_boundaries() {
        for (spread, expected) in [
            (
                Spread::Pad,
                vec![0xffff0000, 0xffff0000, 0xff800080, 0xff0000ff, 0xff0000ff],
            ),
            (
                Spread::Repeat,
                vec![0xff800080, 0xffff0000, 0xff800080, 0xffff0000, 0xff800080],
            ),
            (
                Spread::Reflect,
                vec![0xff800080, 0xffff0000, 0xff800080, 0xff0000ff, 0xff800080],
            ),
        ] {
            let mut p = vec![0; 5];
            gradient(
                &mut p,
                5,
                [1.5, 0.5],
                [3.5, 0.5],
                0xffff0000,
                0xff0000ff,
                false,
                spread,
            );
            assert_eq!(p, expected);
        }
        let mut p = vec![0; 5];
        gradient(
            &mut p,
            5,
            [0.5, 0.5],
            [2.5, 0.5],
            0xffffffff,
            0xff000000,
            true,
            Spread::Repeat,
        );
        assert_eq!(
            p,
            vec![0xffffffff, 0xff808080, 0xffffffff, 0xff808080, 0xffffffff]
        );
    }
}
