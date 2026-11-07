//! Shared encoded-byte HSL math, compiled independently into native DLLs.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Parameters {
    hue_sextants: f64,
    saturation: f64,
    lightness: f64,
}
impl Parameters {
    pub fn new(hue: f64, saturation: f64, lightness: f64) -> Option<Self> {
        if !hue.is_finite()
            || !saturation.is_finite()
            || !lightness.is_finite()
            || !(-180. ..=180.).contains(&hue)
            || !(-100. ..=100.).contains(&saturation)
            || !(-100. ..=100.).contains(&lightness)
        {
            return None;
        }
        Some(Self {
            hue_sextants: hue / 60.,
            saturation: saturation / 100.,
            lightness: lightness / 100.,
        })
    }
    pub fn apply(self, pixel: u32) -> u32 {
        if pixel >> 24 == 0
            || self.hue_sextants == 0. && self.saturation == 0. && self.lightness == 0.
        {
            return pixel;
        }
        let [r, g, b] = [16, 8, 0].map(|shift| f64::from((pixel >> shift) & 255) / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        let mut lightness = (max + min) / 2.;
        let saturation = if chroma == 0. {
            0.
        } else {
            chroma / (1. - (2. * lightness - 1.).abs())
        };
        let saturation = if self.saturation <= 0. {
            saturation * (1. + self.saturation)
        } else if chroma == 0. {
            0.
        } else {
            saturation + (1. - saturation) * self.saturation
        };
        let hue = if chroma == 0. {
            0.
        } else if max == r {
            (g - b) / chroma
        } else if max == g {
            (b - r) / chroma + 2.
        } else {
            (r - g) / chroma + 4.
        };
        let hue = (hue + self.hue_sextants).rem_euclid(6.);
        lightness = if self.lightness <= 0. {
            lightness * (1. + self.lightness)
        } else {
            lightness + (1. - lightness) * self.lightness
        };
        let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
        let x = chroma * (1. - (hue.rem_euclid(2.) - 1.).abs());
        let rgb = match hue as u32 {
            0 => [chroma, x, 0.],
            1 => [x, chroma, 0.],
            2 => [0., chroma, x],
            3 => [0., x, chroma],
            4 => [x, 0., chroma],
            _ => [chroma, 0., x],
        };
        let offset = lightness - chroma / 2.;
        let mut result = pixel & 0xff000000;
        for (value, shift) in rgb.into_iter().zip([16, 8, 0]) {
            result |= (((value + offset) * 255.).round().clamp(0., 255.) as u32) << shift;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn apply(pixel: u32, hue: f64, saturation: f64, lightness: f64) -> u32 {
        Parameters::new(hue, saturation, lightness)
            .unwrap()
            .apply(pixel)
    }
    #[test]
    fn primary_hue_rotations_and_half_turn_preserve_alpha() {
        for (before, after) in [
            (0xffff0000, 0xff00ff00),
            (0xff00ff00, 0xff0000ff),
            (0xff0000ff, 0xffff0000),
            (0x7fff0000, 0x7f00ff00),
        ] {
            assert_eq!(apply(before, 120., 0., 0.), after);
        }
        assert_eq!(apply(0xffff0000, 180., 0., 0.), 0xff00ffff);
        assert_eq!(apply(0xffff0000, -180., 0., 0.), 0xff00ffff);
        assert_eq!(apply(0xffff0000, -120., 0., 0.), 0xff0000ff);
    }
    #[test]
    fn achromatic_identity_transparent_bytes_and_lightness_endpoints_are_exact() {
        for value in 0..256u32 {
            let rgb = value * 0x010101;
            for alpha in [0, 37, 128, 255] {
                let pixel = (alpha << 24) | rgb;
                assert_eq!(apply(pixel, 0., 0., 0.), pixel);
                assert_eq!(apply(pixel, 137., 100., 0.), pixel);
                if alpha > 0 {
                    assert_eq!(apply(pixel, 0., 0., -100.), alpha << 24);
                    assert_eq!(apply(pixel, 0., 0., 100.), (alpha << 24) | 0xffffff);
                }
            }
        }
        assert_eq!(apply(0x00123456, 120., 100., 100.), 0x00123456);
        assert_eq!(apply(0xffff0000, 0., -100., 0.), 0xff808080);
        assert_eq!(apply(0xffff0000, 0., 0., 50.), 0xffff8080);
        assert_eq!(apply(0xffff0000, 0., 0., -50.), 0xff800000);
    }
    #[test]
    fn half_turn_matches_integer_complement_over_a_bounded_rgb_lattice() {
        for r in [0u32, 1, 63, 127, 128, 192, 254, 255] {
            for g in [0u32, 1, 63, 127, 128, 192, 254, 255] {
                for b in [0u32, 1, 63, 127, 128, 192, 254, 255] {
                    let sum = r.max(g).max(b) + r.min(g).min(b);
                    let before = 0x25000000 | (r << 16) | (g << 8) | b;
                    let after = 0x25000000 | ((sum - r) << 16) | ((sum - g) << 8) | (sum - b);
                    assert_eq!(apply(before, 180., 0., 0.), after, "{before:08x}");
                }
            }
        }
    }
}
