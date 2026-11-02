//! Small HSV helpers and a cached, CPU-generated color selector. No image assets.
pub fn hsv(h: f32, s: f32, v: f32) -> u32 {
    let h = h.rem_euclid(1.) * 6.;
    let c = v * s;
    let x = c * (1. - (h % 2. - 1.).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.),
        1 => (x, c, 0.),
        2 => (0., c, x),
        3 => (0., x, c),
        4 => (x, 0., c),
        _ => (c, 0., x),
    };
    let channel = |n: f32| ((n + m).max(0.).min(1.) * 255.).round() as u32;
    (channel(r) << 16) | (channel(g) << 8) | channel(b)
}
pub fn to_hsv(rgb: u32) -> (f32, f32, f32) {
    let r = ((rgb >> 16) & 255) as f32 / 255.;
    let g = ((rgb >> 8) & 255) as f32 / 255.;
    let b = (rgb & 255) as f32 / 255.;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d < 0.00001 {
        0.
    } else if max == r {
        ((g - b) / d).rem_euclid(6.) / 6.
    } else if max == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    (h, if max == 0. { 0. } else { d / max }, max)
}
pub fn square(size: i32) -> (i32, i32) {
    let side = ((size as f32 / 2. - 20.) * std::f32::consts::SQRT_2).floor() as i32;
    ((size - side) / 2, side)
}
pub fn wheel(pixels: &mut Vec<u32>, size: usize, hue: f32, background: u32) {
    pixels.resize(size * size, 0);
    let center = size as f32 / 2.;
    let r = center - 2.;
    let (offset, side) = square(size as i32);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let dist = dx.hypot(dy);
            let coverage = (r + 0.75 - dist)
                .max(0.)
                .min(1.)
                .min((dist - r + 14.75).max(0.).min(1.));
            let c = if coverage > 0. {
                let hue = hsv(dy.atan2(dx) / std::f32::consts::TAU, 1., 1.);
                let mut c = 0;
                for shift in [0, 8, 16] {
                    let a = ((hue >> shift) & 255) as f32;
                    let b = ((background >> shift) & 255) as f32;
                    c |= ((a * coverage + b * (1. - coverage)).round() as u32) << shift;
                }
                c
            } else if x as i32 >= offset
                && y as i32 >= offset
                && (x as i32) < offset + side
                && (y as i32) < offset + side
            {
                hsv(
                    hue,
                    (x as i32 - offset) as f32 / (side - 1) as f32,
                    1. - (y as i32 - offset) as f32 / (side - 1) as f32,
                )
            } else {
                background
            };
            pixels[y * size + x] = 0xff000000 | c;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hsv_roundtrip() {
        for c in [
            0, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0x856aab, 0x333743,
        ] {
            let (h, s, v) = to_hsv(c);
            assert_eq!(hsv(h, s, v), c);
        }
    }
    #[test]
    fn selector_matches_color_coordinates() {
        let mut p = vec![];
        wheel(&mut p, 190, 0., 0xffffff);
        let (o, s) = square(190);
        assert_eq!(
            p[o as usize * 190 + o as usize + s as usize - 1],
            0xffff0000
        );
        assert_eq!(p[(o + s - 1) as usize * 190 + o as usize], 0xff000000);
    }
}
