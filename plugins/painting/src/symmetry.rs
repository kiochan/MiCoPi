//! Fixed-size document-space dab reflection; no allocation or host state.
use crate::tip::Tip;
use micopi_plugin_api as abi;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Symmetry {
    pub axes: u32,
    pub x: f32,
    pub y: f32,
}
impl Default for Symmetry {
    fn default() -> Self {
        Self {
            axes: 0,
            x: 0.5,
            y: 0.5,
        }
    }
}
impl Symmetry {
    pub fn new(axes: u32, x: f32, y: f32) -> Option<Self> {
        (axes & !(abi::SYMMETRY_X | abi::SYMMETRY_Y) == 0
            && x.is_finite()
            && y.is_finite()
            && (0. ..=1.).contains(&x)
            && (0. ..=1.).contains(&y))
        .then_some(Self { axes, x, y })
    }
    pub fn resolve(self, width: u32, height: u32) -> Self {
        Self {
            x: self.x * width as f32,
            y: self.y * height as f32,
            ..self
        }
    }
    pub fn stamps(self, x: f32, y: f32, tip: Tip) -> ([Stamp; 4], usize) {
        let first = Stamp { x, y, tip };
        let mut stamps = [first; 4];
        let mut count = 1;
        for axes in [
            self.axes & abi::SYMMETRY_X,
            self.axes & abi::SYMMETRY_Y,
            self.axes,
        ] {
            if axes == 0 {
                continue;
            }
            let stamp = Stamp {
                x: if axes & abi::SYMMETRY_X != 0 {
                    2. * self.x - x
                } else {
                    x
                },
                y: if axes & abi::SYMMETRY_Y != 0 {
                    2. * self.y - y
                } else {
                    y
                },
                tip: tip.reflected(axes == abi::SYMMETRY_X || axes == abi::SYMMETRY_Y),
            };
            if !stamps[..count].contains(&stamp) {
                stamps[count] = stamp;
                count += 1;
            }
        }
        (stamps, count)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stamp {
    pub x: f32,
    pub y: f32,
    pub tip: Tip,
}
