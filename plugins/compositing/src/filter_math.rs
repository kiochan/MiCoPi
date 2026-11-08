//! Legacy encoded-byte filter arithmetic shared by independent native DLLs.
pub fn filter(p: u32, mode: u32, amount: f32) -> u32 {
    if p >> 24 == 0 {
        return p;
    }
    let r = ((p >> 16) & 255) as f32;
    let g = ((p >> 8) & 255) as f32;
    let b = (p & 255) as f32;
    let gray = (r * 54. + g * 183. + b * 19.) / 256.;
    let mut out = p & 0xff000000;
    for (v, shift) in [(r, 16), (g, 8), (b, 0)] {
        let v = match mode {
            0 => v + (255. - 2. * v) * amount,
            1 => v + (gray - v) * amount,
            2 => v + amount * 255.,
            3 => (v - 127.5) * (1. + amount).powi(2) + 127.5,
            4 => gray + (v - gray) * (1. + amount),
            _ => v,
        };
        out |= (v.round().clamp(0., 255.) as u32) << shift;
    }
    out
}
