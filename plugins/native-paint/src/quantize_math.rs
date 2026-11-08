//! Shared integer quantization kernels, independent of JSON and host types.
#[derive(Clone, Copy)]
pub(super) enum Quantize {
    Threshold(u32),
    Posterize([u8; 256]),
}
pub(super) fn threshold(level: u32) -> Option<Quantize> {
    (level <= 255).then(|| Quantize::Threshold(level * 256))
}
pub(super) fn posterize(levels: u32) -> Option<Quantize> {
    if !(2..=256).contains(&levels) {
        return None;
    }
    let intervals = levels - 1;
    Some(Quantize::Posterize(std::array::from_fn(|byte| {
        let bucket = (byte as u32 * intervals + 127) / 255;
        ((bucket * 255 + intervals / 2) / intervals) as u8
    })))
}
impl Quantize {
    #[inline]
    pub(super) fn apply(&self, pixel: u32) -> u32 {
        if pixel >> 24 == 0 {
            return pixel;
        }
        let r = (pixel >> 16) & 255;
        let g = (pixel >> 8) & 255;
        let b = pixel & 255;
        let rgb = match self {
            Self::Threshold(cutoff) => {
                if 54 * r + 183 * g + 19 * b >= *cutoff {
                    0xffffff
                } else {
                    0
                }
            }
            Self::Posterize(table) => {
                (table[r as usize] as u32) << 16
                    | (table[g as usize] as u32) << 8
                    | table[b as usize] as u32
            }
        };
        (pixel & 0xff000000) | rgb
    }
}
