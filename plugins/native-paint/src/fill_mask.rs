//! Optional fill-edge processing. The zero-option path remains in area.rs.
use super::area::{matches, paint};
fn bit(mask: &[u8], i: usize) -> bool {
    mask[i / 8] & (1 << (i % 8)) != 0
}
fn mark(mask: &mut [u8], i: usize) {
    mask[i / 8] |= 1 << (i % 8);
}

fn selection(
    p: &[u32],
    w: usize,
    h: usize,
    seed: usize,
    tolerance: u32,
    contiguous: bool,
) -> Vec<u8> {
    let mut mask = vec![0u8; p.len().div_ceil(8)];
    let target = p[seed];
    if !contiguous {
        for (i, pixel) in p.iter().enumerate() {
            if matches(*pixel, target, tolerance) {
                mark(&mut mask, i);
            }
        }
        return mask;
    }
    fn run(
        p: &[u32],
        mask: &mut [u8],
        w: usize,
        x: usize,
        y: usize,
        target: u32,
        t: u32,
    ) -> (usize, usize) {
        let eligible = |i| !bit(mask, i) && matches(p[i], target, t);
        let mut left = x;
        let mut right = x;
        while left > 0 && eligible(y * w + left - 1) {
            left -= 1;
        }
        while right + 1 < w && eligible(y * w + right + 1) {
            right += 1;
        }
        for x in left..=right {
            mark(mask, y * w + x);
        }
        (left, right)
    }
    let (left, right) = run(p, &mut mask, w, seed % w, seed / w, target, tolerance);
    let mut stack = vec![((seed / w) as u16, left as u16, right as u16)];
    while let Some((y, left, right)) = stack.pop() {
        for ny in [y as isize - 1, y as isize + 1] {
            if ny < 0 || ny >= h as isize {
                continue;
            }
            let y = ny as usize;
            let mut x = left as usize;
            while x <= right as usize {
                let i = y * w + x;
                if !bit(&mask, i) && matches(p[i], target, tolerance) {
                    let (a, b) = run(p, &mut mask, w, x, y, target, tolerance);
                    stack.push((y as u16, a as u16, b as u16));
                    x = b + 1;
                } else {
                    x += 1;
                }
            }
        }
    }
    mask
}

/// Capped Chebyshev distance from either side to its boundary. Both sides start
/// at distance 1; signed pixel-center distance is then +/- (distance - 0.5).
fn distances(mask: &[u8], w: usize, h: usize, limit: u8, d: &mut [u8]) {
    d.fill(limit);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let inside = bit(mask, i);
            let edge = inside && (x == 0 || y == 0 || x + 1 == w || y + 1 == h);
            let boundary = edge
                || (y.saturating_sub(1)..=(y + 1).min(h - 1)).any(|ny| {
                    (x.saturating_sub(1)..=(x + 1).min(w - 1))
                        .any(|nx| bit(mask, ny * w + nx) != inside)
                });
            if boundary {
                d[i] = 1;
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut value = d[i];
            if x > 0 {
                value = value.min(d[i - 1].saturating_add(1));
            }
            if y > 0 {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    value = value.min(d[(y - 1) * w + nx].saturating_add(1));
                }
            }
            d[i] = value;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut value = d[i];
            if x + 1 < w {
                value = value.min(d[i + 1].saturating_add(1));
            }
            if y + 1 < h {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    value = value.min(d[(y + 1) * w + nx].saturating_add(1));
                }
            }
            d[i] = value;
        }
    }
}

pub fn apply(
    p: &mut [u32],
    w: usize,
    h: usize,
    seed: usize,
    color: u32,
    tolerance: u32,
    contiguous: bool,
    expand: i32,
    feather: u32,
) {
    apply_mode::<false>(p, w, h, seed, color, tolerance, contiguous, expand, feather)
}
pub fn apply_locked(
    p: &mut [u32],
    w: usize,
    h: usize,
    seed: usize,
    color: u32,
    tolerance: u32,
    contiguous: bool,
    expand: i32,
    feather: u32,
) {
    apply_mode::<true>(p, w, h, seed, color, tolerance, contiguous, expand, feather)
}
fn apply_mode<const LOCKED: bool>(
    p: &mut [u32],
    w: usize,
    h: usize,
    seed: usize,
    color: u32,
    tolerance: u32,
    contiguous: bool,
    expand: i32,
    feather: u32,
) {
    if color >> 24 == 0 {
        return;
    }
    let mut mask = selection(p, w, h, seed, tolerance, contiguous);
    let mut distance = vec![0u8; p.len()];
    if expand != 0 {
        let radius = expand.unsigned_abs() as u8;
        distances(&mask, w, h, radius + 1, &mut distance);
        for (i, d) in distance.iter().enumerate() {
            let inside = bit(&mask, i);
            let included = if expand > 0 {
                inside || *d <= radius
            } else {
                inside && *d > radius
            };
            if included {
                mark(&mut mask, i);
            } else {
                mask[i / 8] &= !(1 << (i % 8));
            }
        }
    }
    // Feather the final geometry: merged expansions must have no internal seam,
    // and fully contracted regions must remain empty.
    if feather > 0 {
        distances(&mask, w, h, feather as u8 + 1, &mut distance);
    }
    for (i, pixel) in p.iter_mut().enumerate() {
        let coverage = if feather == 0 {
            if bit(&mask, i) {
                1.
            } else {
                0.
            }
        } else {
            let signed = if bit(&mask, i) {
                distance[i] as f32 - 0.5
            } else {
                0.5 - distance[i] as f32
            };
            ((signed + feather as f32) / (2. * feather as f32)).clamp(0., 1.)
        };
        let alpha = ((color >> 24) as f32 * coverage).round() as u32;
        if alpha > 0 {
            *pixel = paint::<LOCKED>(*pixel, (color & 0xffffff) | (alpha << 24));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::area::over;
    use super::*;
    #[test]
    fn expansion_contraction_and_feather_have_defined_pixel_edges() {
        let mut p = vec![0xff000000; 25];
        p[12] = 0;
        apply(&mut p, 5, 5, 12, 0xffff0000, 0, true, 1, 0);
        for y in 0..5 {
            for x in 0..5 {
                assert_eq!(
                    p[y * 5 + x],
                    if (1..=3).contains(&x) && (1..=3).contains(&y) {
                        0xffff0000
                    } else {
                        0xff000000
                    }
                );
            }
        }
        let mut p = vec![0; 25];
        apply(&mut p, 5, 5, 12, 0xffff0000, 0, true, -1, 0);
        for y in 0..5 {
            for x in 0..5 {
                assert_eq!(
                    p[y * 5 + x],
                    if (1..=3).contains(&x) && (1..=3).contains(&y) {
                        0xffff0000
                    } else {
                        0
                    }
                );
            }
        }
        let mut p = vec![0; 9];
        apply(&mut p, 3, 3, 4, 0xffff0000, 0, true, 0, 1);
        assert_eq!(p[0], 0xbfff0000);
        assert_eq!(p[4], 0xffff0000);
        let mut p = vec![0; 1];
        apply(&mut p, 1, 1, 0, 0xffff0000, 0, true, -1, 0);
        assert_eq!(p, [0]);
        apply(&mut p, 1, 1, 0, 0xffff0000, 0, true, -1, 1);
        assert_eq!(p, [0]);
        let mut joined = vec![0xff000000; 35];
        joined[16] = 0;
        joined[18] = 0;
        apply(&mut joined, 7, 5, 16, 0xffff0000, 0, false, 1, 1);
        assert_eq!(joined[17], 0xffff0000);
    }
    #[test]
    fn selection_and_distance_match_independent_flood_and_nearest_pixel_oracles() {
        for seed in 0..24u32 {
            let (w, h) = (7usize, 6usize);
            let mut rng = seed + 1;
            let original: Vec<u32> = (0..w * h)
                .map(|_| {
                    rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
                    if rng >> 30 == 0 {
                        0xff000000
                    } else {
                        0
                    }
                })
                .collect();
            let seed = seed as usize % (w * h);
            for contiguous in [false, true] {
                let mut selected = vec![false; w * h];
                if contiguous {
                    let mut queue = std::collections::VecDeque::from([seed]);
                    while let Some(i) = queue.pop_front() {
                        if selected[i] || original[i] != original[seed] {
                            continue;
                        }
                        selected[i] = true;
                        let (x, y) = (i % w, i / w);
                        for (nx, ny) in [
                            (x as isize - 1, y as isize),
                            (x as isize + 1, y as isize),
                            (x as isize, y as isize - 1),
                            (x as isize, y as isize + 1),
                        ] {
                            if nx >= 0 && ny >= 0 && nx < w as isize && ny < h as isize {
                                queue.push_back(ny as usize * w + nx as usize);
                            }
                        }
                    }
                } else {
                    for i in 0..w * h {
                        selected[i] = original[i] == original[seed];
                    }
                }
                for expand in [-32, -3, -1, 0, 1, 3, 32] {
                    for feather in [0, 1, 2, 4, 32] {
                        let nearest = |mask: &[bool], i: usize| {
                            let (x, y) = (i % w, i / w);
                            let mut nearest = if mask[i] {
                                (x + 1).min(y + 1).min(w - x).min(h - y)
                            } else {
                                999
                            };
                            for j in 0..w * h {
                                if mask[i] != mask[j] {
                                    nearest = nearest.min(x.abs_diff(j % w).max(y.abs_diff(j / w)));
                                }
                            }
                            nearest
                        };
                        let mut grown = selected.clone();
                        for i in 0..w * h {
                            let d = nearest(&selected, i) as i32;
                            grown[i] = if expand >= 0 {
                                selected[i] || d <= expand
                            } else {
                                selected[i] && d > -expand
                            };
                        }
                        let mut expected = original.clone();
                        for i in 0..w * h {
                            let d = nearest(&grown, i);
                            let signed = if grown[i] {
                                d as f32 - 0.5
                            } else {
                                0.5 - d as f32
                            };
                            let coverage = if feather == 0 {
                                if grown[i] {
                                    1.
                                } else {
                                    0.
                                }
                            } else {
                                ((signed + feather as f32) / (2. * feather as f32)).clamp(0., 1.)
                            };
                            let a = (192. * coverage).round() as u32;
                            if a > 0 {
                                expected[i] = over(original[i], 0x0033aaff | (a << 24));
                            }
                        }
                        let mut actual = original.clone();
                        apply(
                            &mut actual,
                            w,
                            h,
                            seed,
                            0xc033aaff,
                            0,
                            contiguous,
                            expand,
                            feather,
                        );
                        assert_eq!(actual,expected,"seed {seed}, contiguous {contiguous}, expand {expand}, feather {feather}");
                    }
                }
            }
        }
    }
}
