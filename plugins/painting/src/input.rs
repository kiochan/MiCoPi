//! Allocation-free, timestamp-based pen/mouse filtering, independent of Windows.
use crate::Brush;

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub time: u32,
}
#[derive(Clone, Copy)]
pub struct Dynamics {
    pub size: bool,
    pub opacity: bool,
    pub gamma: f32,
    pub minimum: f32,
}
impl Default for Dynamics {
    fn default() -> Self {
        Self {
            size: true,
            opacity: false,
            gamma: 1.,
            minimum: 0.08,
        }
    }
}
impl Dynamics {
    pub fn apply(self, mut brush: Brush, pressure: f32) -> Brush {
        let p = pressure.max(0.).min(1.).powf(self.gamma.max(0.25).min(4.));
        if self.size {
            brush.radius *= self.minimum + (1. - self.minimum) * p;
        }
        if self.opacity {
            brush.opacity *= p;
        }
        // A valid zero-pressure sample must never deposit paint.
        if pressure <= 0. {
            brush.opacity = 0.;
        }
        brush
    }
}

pub struct Stabilizer {
    filtered: Option<Sample>,
    raw: Option<Sample>,
    speed: f32,
}
impl Default for Stabilizer {
    fn default() -> Self {
        Self {
            filtered: None,
            raw: None,
            speed: 0.,
        }
    }
}
fn alpha(cutoff: f32, dt: f32) -> f32 {
    let t = std::f32::consts::TAU * cutoff * dt;
    t / (1. + t)
}
impl Stabilizer {
    pub fn reset(&mut self, s: Sample) -> Sample {
        self.filtered = Some(s);
        self.raw = Some(s);
        self.speed = 0.;
        s
    }
    pub fn push(&mut self, s: Sample, strength: f32) -> Sample {
        let old = match self.filtered {
            Some(s) => s,
            None => return self.reset(s),
        };
        let raw = self.raw.unwrap();
        let dt = (s.time.wrapping_sub(raw.time) as f32 / 1000.)
            .max(0.001)
            .min(0.05);
        let velocity = (s.x - raw.x).hypot(s.y - raw.y) / dt;
        self.speed += alpha(10., dt) * (velocity - self.speed);
        let strength = strength.max(0.).min(1.);
        let a = if strength == 0. {
            1.
        } else {
            alpha(
                1.5 + (1. - strength) * 18. + self.speed * 0.035 * (1. - strength * 0.75),
                dt,
            )
        };
        let p = alpha(20., dt);
        let filtered = Sample {
            x: old.x + (s.x - old.x) * a,
            y: old.y + (s.y - old.y) * a,
            pressure: if strength == 0. {
                s.pressure
            } else {
                old.pressure + (s.pressure - old.pressure) * p
            },
            time: s.time,
        };
        self.filtered = Some(filtered);
        self.raw = Some(s);
        filtered
    }
    /// Reach the last actual contact on release, without a delayed drawing loop.
    pub fn finish(&mut self) -> Option<Sample> {
        let raw = self.raw.take();
        self.filtered = None;
        self.speed = 0.;
        raw
    }
    pub fn cancel(&mut self) {
        self.raw = None;
        self.filtered = None;
        self.speed = 0.;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(x: f32, y: f32, time: u32) -> Sample {
        Sample {
            x,
            y,
            pressure: 0.6,
            time,
        }
    }
    #[test]
    fn off_is_exact_and_endpoint_is_preserved() {
        let mut f = Stabilizer::default();
        f.reset(s(0., 0., 0));
        let q = f.push(s(10., 20., 8), 0.);
        assert_eq!((q.x, q.y), (10., 20.));
        let q = f.finish().unwrap();
        assert_eq!((q.x, q.y), (10., 20.));
        assert!(f.finish().is_none());
    }
    #[test]
    fn reduces_slow_jitter() {
        let mut f = Stabilizer::default();
        f.reset(s(0., 0., 0));
        let mut energy = 0.;
        for i in 1..200 {
            let q = f.push(
                s(i as f32 * 0.05, if i % 2 == 0 { 1. } else { -1. }, i * 4),
                0.8,
            );
            energy += q.y * q.y;
        }
        assert!(energy < 20., "{}", energy);
    }
    #[test]
    fn timestamps_wrap_and_cancel() {
        let mut f = Stabilizer::default();
        f.reset(s(0., 0., u32::MAX - 2));
        let q = f.push(s(1., 1., 2), 1.);
        assert!(q.x.is_finite());
        f.cancel();
        assert!(f.finish().is_none());
    }
    #[test]
    fn dynamics_endpoints() {
        let b = Brush {
            radius: 20.,
            opacity: 0.8,
            hardness: 1.,
            color: 0,
            erase: false,
        };
        let d = Dynamics::default();
        assert_eq!(d.apply(b, 1.).radius, 20.);
        assert!(d.apply(b, 0.1).radius < 5.);
        assert_eq!(d.apply(b, 0.).opacity, 0.);
        assert_eq!(
            Dynamics {
                size: false,
                opacity: true,
                ..d
            }
            .apply(b, 0.5)
            .opacity,
            0.4
        );
    }
}
