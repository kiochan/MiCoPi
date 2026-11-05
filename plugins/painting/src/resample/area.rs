//! Exact box footprints for shrinking axes; linear centers otherwise.
//! Separable integer sums are never rounded between passes. Reuse two source rows.
#[derive(Clone, Copy)]
struct Plan {
    a: usize,
    b: usize,
    first: u32,
    last: u32,
}
fn plan(source: usize, target: usize, i: usize) -> Plan {
    if source > target {
        let (left, right) = (i * source, (i + 1) * source);
        let (a, b) = (left / target, (right - 1) / target);
        Plan {
            a,
            b,
            first: (((a + 1) * target).min(right) - left) as u32,
            last: if a == b {
                0
            } else {
                (right - b * target) as u32
            },
        }
    } else {
        let den = (2 * target) as i64;
        let n = ((2 * i + 1) * source) as i64 - target as i64;
        let start = n.div_euclid(den);
        let (a, b) = (
            start.clamp(0, source as i64 - 1) as usize,
            (start + 1).clamp(0, source as i64 - 1) as usize,
        );
        let last = n.rem_euclid(den) as u32;
        Plan {
            a,
            b,
            first: if a == b {
                den as u32
            } else {
                den as u32 - last
            },
            last: if a == b { 0 } else { last },
        }
    }
}
impl Plan {
    fn visit(&self, inner: u32, mut f: impl FnMut(usize, u32)) {
        if self.first != 0 {
            f(self.a, self.first);
        }
        if self.b > self.a {
            for i in self.a + 1..self.b {
                f(i, inner);
            }
            if self.last != 0 {
                f(self.b, self.last);
            }
        }
    }
}
fn denominator(source: usize, target: usize) -> u64 {
    if source > target {
        source as u64
    } else {
        (2 * target) as u64
    }
}
pub(super) trait AreaSample: Copy {
    type Sum: Copy + Default;
    fn value(self) -> Self::Sum;
    fn add(sum: &mut Self::Sum, value: Self::Sum, weight: u32);
    fn finish(sum: Self::Sum, den: u64) -> Self;
}
impl AreaSample for u8 {
    type Sum = u64;
    fn value(self) -> u64 {
        u64::from(self)
    }
    fn add(sum: &mut u64, value: u64, weight: u32) {
        *sum += value * u64::from(weight);
    }
    fn finish(sum: u64, den: u64) -> Self {
        ((sum + den / 2) / den) as u8
    }
}
impl AreaSample for u32 {
    type Sum = [u64; 4];
    fn value(self) -> Self::Sum {
        let a = u64::from(self >> 24);
        [
            a,
            u64::from(self & 255) * a,
            u64::from((self >> 8) & 255) * a,
            u64::from((self >> 16) & 255) * a,
        ]
    }
    fn add(sum: &mut Self::Sum, value: Self::Sum, weight: u32) {
        for i in 0..4 {
            sum[i] += value[i] * u64::from(weight);
        }
    }
    fn finish(sum: Self::Sum, den: u64) -> Self {
        let a = (sum[0] + den / 2) / den;
        if a == 0 {
            return 0;
        }
        (a as u32) << 24
            | ((sum[1] + sum[0] / 2) / sum[0]) as u32
            | (((sum[2] + sum[0] / 2) / sum[0]) as u32) << 8
            | (((sum[3] + sum[0] / 2) / sum[0]) as u32) << 16
    }
}
pub(super) fn run<T: AreaSample>(
    src: &[T],
    sw: usize,
    sh: usize,
    ss: usize,
    out: &mut [T],
    w: usize,
    h: usize,
    stride: usize,
) {
    let xs: Vec<_> = (0..w).map(|x| plan(sw, w, x)).collect();
    let den = denominator(sw, w) * denominator(sh, h);
    let mut cache: [Vec<T::Sum>; 2] = std::array::from_fn(|_| vec![T::Sum::default(); w]);
    let mut cached = [None; 2];
    let mut row = vec![T::Sum::default(); w];
    for y in 0..h {
        row.fill(T::Sum::default());
        plan(sh, h, y).visit(h as u32, |sy, wy| {
            let slot = sy % 2;
            if cached[slot] != Some(sy) {
                for (x, p) in xs.iter().enumerate() {
                    let mut sum = T::Sum::default();
                    p.visit(w as u32, |sx, wx| {
                        T::add(&mut sum, src[sy * ss + sx].value(), wx)
                    });
                    cache[slot][x] = sum;
                }
                cached[slot] = Some(sy);
            }
            for x in 0..w {
                T::add(&mut row[x], cache[slot][x], wy);
            }
        });
        for x in 0..w {
            out[y * stride + x] = T::finish(row[x], den);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Direct 2-D floating integration, independent of separable row sums/plans.
    fn weight(source: usize, target: usize, dest: usize, index: usize) -> f64 {
        if source > target {
            let left = dest as f64 * source as f64 / target as f64;
            let right = (dest + 1) as f64 * source as f64 / target as f64;
            (right.min(index as f64 + 1.) - left.max(index as f64)).max(0.) / (right - left)
        } else {
            let center = (dest as f64 + 0.5) * source as f64 / target as f64 - 0.5;
            let base = center.floor() as i64;
            let fraction = center - center.floor();
            [(base, 1. - fraction), (base + 1, fraction)]
                .into_iter()
                .filter(|(i, _)| i.clamp(&0, &(source as i64 - 1)) == &(index as i64))
                .map(|(_, w)| w)
                .sum()
        }
    }
    #[test]
    fn area_matches_direct_integration_for_noninteger_mixed_and_strided_images() {
        for (sw, sh, w, h) in [
            (7, 5, 3, 2),
            (33, 17, 13, 7),
            (4, 3, 7, 2),
            (7, 5, 3, 9),
            (1, 7, 4, 3),
            (7, 1, 3, 4),
            (5, 3, 5, 3),
        ] {
            let src: Vec<_> = (0..sw * sh)
                .map(|i| (i as u32).wrapping_mul(1957793911))
                .collect();
            let mut source = vec![0x12345678; (sw + 2) * sh];
            for y in 0..sh {
                source[y * (sw + 2)..y * (sw + 2) + sw].copy_from_slice(&src[y * sw..y * sw + sw]);
            }
            let before = source.clone();
            let mut out = vec![0x99887766; (w + 1) * h];
            run(&source, sw, sh, sw + 2, &mut out, w, h, w + 1);
            for y in 0..h {
                for x in 0..w {
                    let mut sums = [0.; 4];
                    for sy in 0..sh {
                        for sx in 0..sw {
                            let weight = weight(sw, w, x, sx) * weight(sh, h, y, sy);
                            let p = src[sy * sw + sx];
                            let a = (p >> 24) as f64;
                            sums[0] += a * weight;
                            for c in 1..4 {
                                sums[c] += ((p >> (8 * (c - 1))) & 255) as f64 * a * weight;
                            }
                        }
                    }
                    let expected = if sums[0].round() == 0. {
                        0
                    } else {
                        (sums[0].round() as u32) << 24
                            | (sums[1] / sums[0]).round() as u32
                            | ((sums[2] / sums[0]).round() as u32) << 8
                            | ((sums[3] / sums[0]).round() as u32) << 16
                    };
                    for shift in [0, 8, 16, 24] {
                        assert!(
                            (((out[y * (w + 1) + x] >> shift) & 255) as i32
                                - ((expected >> shift) & 255) as i32)
                                .abs()
                                <= 1,
                            "{sw} {sh} {w} {h} {x} {y}"
                        );
                    }
                }
                assert_eq!(out[y * (w + 1) + w], 0x99887766);
            }
            assert_eq!(source, before);
        }
    }
    #[test]
    fn exact_footprints_keep_thin_lines_and_transparent_edges() {
        let mut out = [0; 2];
        let source = [
            0xffffffffu32,
            0xff000000,
            0xff000000,
            0xff000000,
            0xff000000,
            0xff000000,
            0xff000000,
            0xff000000,
        ];
        run(&source, 8, 1, 8, &mut out, 2, 1, 2);
        assert_eq!(out, [0xff404040, 0xff000000]);
        let mut pixels = [0; 1];
        run(
            &[0x00ff0000u32, 0xff0000ff, 0, 0],
            4,
            1,
            4,
            &mut pixels,
            1,
            1,
            1,
        );
        assert_eq!(pixels, [0x400000ff]);
        let mut mask = [0; 1];
        run(&[0u8, 255, 0, 0], 4, 1, 4, &mut mask, 1, 1, 1);
        assert_eq!(mask, [64]);
        let mut fractional = [0; 2];
        run(&[0u8, 255, 0], 3, 1, 3, &mut fractional, 2, 1, 2);
        assert_eq!(fractional, [85, 85]);
    }
    #[test]
    fn footprint_plans_cover_source_exactly_with_bounded_sums() {
        for source in [1, 2, 3, 17, 4096] {
            for target in [1, 2, 5, 16, 4095, 4096] {
                for x in 0..target {
                    let mut sum = 0u64;
                    plan(source, target, x).visit(target as u32, |i, w| {
                        assert!(i < source);
                        sum += u64::from(w);
                    });
                    assert_eq!(sum, denominator(source, target));
                    assert!(65025 * sum * 8192 < u64::MAX);
                }
            }
        }
    }
    #[test]
    #[ignore = "Explicit area filter cost, excludes host/history/compression"]
    fn area_cost() {
        let src: Vec<_> = (0..4096 * 4096)
            .map(|i| 0xff000000 | ((i as u32).wrapping_mul(1957793911) & 0xffffff))
            .collect();
        for (w, h) in [(1024, 1024), (2731, 2048), (4096, 2048), (4095, 4095)] {
            let mut out = vec![0; w * h];
            let start = std::time::Instant::now();
            run(&src, 4096, 4096, 4096, &mut out, w, h, w);
            assert!(out.iter().all(|p| p >> 24 == 255));
            eprintln!(
                "4096² -> {w}x{h} area: {:?}; at most 480 KiB native scratch",
                start.elapsed()
            );
        }
    }
}
