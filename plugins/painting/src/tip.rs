//! Analytic fixed-angle tips. No assets, allocations or retained host pointers.
use micopi_plugin_api as abi;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tip {
    shape: u32,
    roundness: f32,
    cosine: f32,
    sine: f32,
}
impl Default for Tip {
    fn default() -> Self {
        Self {
            shape: abi::TIP_ROUND,
            roundness: 1.,
            cosine: 1.,
            sine: 0.,
        }
    }
}
impl Tip {
    pub fn new(shape: u32, roundness: f32, angle: f32) -> Option<Self> {
        if !matches!(shape, abi::TIP_ROUND | abi::TIP_SQUARE)
            || !roundness.is_finite()
            || !(0.25..=1.).contains(&roundness)
            || !angle.is_finite()
            || !(-180. ..=180.).contains(&angle)
        {
            return None;
        }
        // Exact cardinal axes avoid tiny unwanted coverage from trigonometric
        // roundoff. All other angles are evaluated once before the contact.
        let (sine, cosine) = match angle {
            0. => (0., 1.),
            90. => (1., 0.),
            -90. => (-1., 0.),
            -180. | 180. => (0., -1.),
            _ => angle.to_radians().sin_cos(),
        };
        Some(Self {
            shape,
            roundness,
            cosine,
            sine,
        })
    }
    pub fn reflected(self, odd_axes: bool) -> Self {
        // Both tip shapes are centrally symmetric. Cardinal orientations and
        // legacy circles (and diagonal full squares) retain the same footprint.
        if odd_axes
            && !self.legacy()
            && self.sine != 0.
            && self.cosine != 0.
            && !(self.shape == abi::TIP_SQUARE
                && self.roundness == 1.
                && self.cosine.abs() == self.sine.abs())
        {
            Self {
                sine: -self.sine,
                ..self
            }
        } else {
            self
        }
    }
    pub fn legacy(self) -> bool {
        self.shape == abi::TIP_ROUND && self.roundness == 1.
    }
    pub fn spacing_scale(self) -> f32 {
        if self.legacy() {
            1.
        } else {
            self.roundness
        }
    }
    pub fn geometry(self, radius: f32) -> Geometry {
        let a = radius.max(0.5);
        let b = a * self.roundness;
        let (extent_x, extent_y) = if self.legacy() {
            (a, a)
        } else if self.shape == abi::TIP_SQUARE {
            (
                a * self.cosine.abs() + b * self.sine.abs(),
                a * self.sine.abs() + b * self.cosine.abs(),
            )
        } else {
            (
                (a * self.cosine).hypot(b * self.sine),
                (a * self.sine).hypot(b * self.cosine),
            )
        };
        Geometry {
            tip: self,
            a,
            b,
            inverse_a: 1. / a,
            inverse_b: 1. / b,
            extent_x,
            extent_y,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    tip: Tip,
    a: f32,
    b: f32,
    inverse_a: f32,
    inverse_b: f32,
    extent_x: f32,
    extent_y: f32,
}
impl Default for Geometry {
    fn default() -> Self {
        Tip::default().geometry(0.5)
    }
}
impl Geometry {
    pub fn legacy(self) -> bool {
        self.tip.legacy()
    }
    pub fn region(self, x: f32, y: f32, width: u32, height: u32) -> abi::Region {
        // A normalized radial fringe can reach at most 0.5/q beyond the nominal
        // ellipse support. This conservative bound also covers subpixel axes.
        let margin = if self.legacy() {
            1.
        } else if self.tip.shape == abi::TIP_SQUARE {
            1.
        } else {
            0.5 / self.tip.roundness + 0.5
        };
        let x0 = (x - self.extent_x - margin)
            .floor()
            .max(0.)
            .min(width as f32) as u32;
        let y0 = (y - self.extent_y - margin)
            .floor()
            .max(0.)
            .min(height as f32) as u32;
        let x1 = (x + self.extent_x + margin)
            .ceil()
            .max(0.)
            .min(width as f32) as u32;
        let y1 = (y + self.extent_y + margin)
            .ceil()
            .max(0.)
            .min(height as f32) as u32;
        abi::Region {
            x: x0,
            y: y0,
            width: x1.saturating_sub(x0),
            height: y1.saturating_sub(y0),
        }
    }
    /// Physical normal-distance antialiasing and a common normalized hardness
    /// falloff. The ellipse distance is a first-order normal estimate, not an
    /// iterative closest-point solve. The rectangle distance is exact.
    pub fn coverage(self, dx: f32, dy: f32, hardness: f32) -> f32 {
        let u = self.tip.cosine * dx + self.tip.sine * dy;
        let v = -self.tip.sine * dx + self.tip.cosine * dy;
        let nx = u * self.inverse_a;
        let ny = v * self.inverse_b;
        let (normalized, distance) = if self.tip.shape == abi::TIP_SQUARE {
            let du = u.abs() - self.a;
            let dv = v.abs() - self.b;
            (
                nx.abs().max(ny.abs()),
                du.max(0.).hypot(dv.max(0.)) + du.max(dv).min(0.),
            )
        } else {
            let normalized = nx.hypot(ny);
            let gradient = (nx * self.inverse_a).hypot(ny * self.inverse_b);
            (
                normalized,
                if gradient > 0. {
                    (normalized - 1.) * normalized / gradient
                } else {
                    -self.b
                },
            )
        };
        let edge = (0.5 - distance).clamp(0., 1.);
        let soft = if hardness >= 0.999 {
            1.
        } else {
            ((1. - normalized) / (1. - hardness)).clamp(0., 1.)
        };
        edge * soft
    }
    /// A deliberately expanded enclosure, plus a pixel of integer rounding
    /// slack. It skips only candidates that cannot have positive coverage.
    pub fn row_span(self, x: f32, dy: f32, start: usize, end: usize) -> Option<(usize, usize)> {
        let (left, right) = if self.tip.shape == abi::TIP_SQUARE {
            let mut range = (f32::NEG_INFINITY, f32::INFINITY);
            for (coefficient, offset, half) in [
                (self.tip.cosine, self.tip.sine * dy, self.a + 0.75),
                (-self.tip.sine, self.tip.cosine * dy, self.b + 0.75),
            ] {
                if coefficient == 0. {
                    if offset.abs() > half {
                        return None;
                    }
                } else {
                    let p = (-half - offset) / coefficient;
                    let q = (half - offset) / coefficient;
                    range.0 = range.0.max(p.min(q));
                    range.1 = range.1.min(p.max(q));
                }
            }
            range
        } else {
            // g<=1/b, so the normal fringe is enclosed by this scaled ellipse.
            let a = self.a + 0.5 / self.tip.roundness + 0.25;
            let b = self.b + 0.75;
            let ia = 1. / (a * a);
            let ib = 1. / (b * b);
            let c = self.tip.cosine;
            let s = self.tip.sine;
            let quadratic = c * c * ia + s * s * ib;
            let linear = 2. * dy * c * s * (ia - ib);
            let constant = dy * dy * (s * s * ia + c * c * ib) - 1.;
            let discriminant = linear * linear - 4. * quadratic * constant;
            if discriminant < 0. {
                return None;
            }
            let middle = -linear / (2. * quadratic);
            let half = discriminant.sqrt() / (2. * quadratic);
            (middle - half, middle + half)
        };
        if left > right {
            return None;
        }
        let lo = ((x + left - 0.5).floor() - 1.)
            .max(start as f32)
            .min(end as f32) as usize;
        let hi = ((x + right - 0.5).ceil() + 2.)
            .max(start as f32)
            .min(end as f32) as usize;
        (lo < hi).then_some((lo, hi))
    }
}

#[cfg(test)]
pub(crate) mod probe {
    use std::cell::Cell;
    #[derive(Clone, Copy, Default, Debug)]
    pub struct Counts {
        pub dabs: u64,
        pub candidates: u64,
        pub positive_visits: u64,
        pub loan_regions: u64,
        pub loan_pixels: u64,
    }
    thread_local! {static COUNTS:Cell<Counts>=Cell::new(Counts::default());}
    fn update(f: impl FnOnce(&mut Counts)) {
        COUNTS.with(|counter| {
            let mut value = counter.get();
            f(&mut value);
            counter.set(value);
        });
    }
    pub fn reset() {
        COUNTS.with(|counter| counter.set(Counts::default()));
    }
    pub fn get() -> Counts {
        COUNTS.with(Cell::get)
    }
    pub fn dab() {
        update(|c| c.dabs += 1);
    }
    pub fn candidates(count: usize) {
        update(|c| c.candidates += count as u64);
    }
    pub fn positive() {
        update(|c| c.positive_visits += 1);
    }
    pub fn loan(region: micopi_plugin_api::Region) {
        update(|c| {
            c.loan_regions += 1;
            c.loan_pixels += u64::from(region.width) * u64::from(region.height);
        });
    }
}
