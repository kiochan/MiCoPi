//! Extended color kernels. RGB is normalized in its stored encoded color space.
//! Standard modes follow https://www.w3.org/TR/compositing-1/#blending.
//! Quantize the blend color once, then retain the legacy integer alpha rounding.
use micopi_plugin_api as abi;

type Rgb = [f64; 3];

// Keep extended floating-point register spills out of every legacy pixel call.
// Extended modes remain ordinary workloads; only this inlining boundary differs.
#[inline(never)]
pub(super) fn over(dst: u32, src: u32, mode: u32) -> u32 {
    let a = src >> 24;
    if a == 0 {
        return dst;
    }
    let da = dst >> 24;
    let out_a = a + (da * (255 - a) + 127) / 255;
    let blended = color(dst, src, mode);
    let mut out = out_a << 24;
    for (i, shift) in [16, 8, 0].into_iter().enumerate() {
        let s = (src >> shift) & 255;
        let d = (dst >> shift) & 255;
        let b = blended[i];
        let c =
            ((255 - a) * da * d + (255 - da) * a * s + a * da * b + out_a * 127) / (out_a * 255);
        out |= c.min(255) << shift;
    }
    out
}

fn rgb(pixel: u32) -> Rgb {
    [
        ((pixel >> 16) & 255) as f64 / 255.,
        ((pixel >> 8) & 255) as f64 / 255.,
        (pixel & 255) as f64 / 255.,
    ]
}

fn color(dst: u32, src: u32, mode: u32) -> [u32; 3] {
    match mode {
        // Integer sums make equal-brightness colors unambiguous; ties retain
        // the backdrop, independently of the source and backdrop alpha.
        abi::BLEND_DARKER_COLOR | abi::BLEND_LIGHTER_COLOR => {
            let sum = |p: u32| ((p >> 16) & 255) + ((p >> 8) & 255) + (p & 255);
            let take_source = if mode == abi::BLEND_DARKER_COLOR {
                sum(src) < sum(dst)
            } else {
                sum(src) > sum(dst)
            };
            let pixel = if take_source { src } else { dst };
            [16, 8, 0].map(|shift| (pixel >> shift) & 255)
        }
        abi::BLEND_HUE..=abi::BLEND_LUMINOSITY => {
            let d = rgb(dst);
            let s = rgb(src);
            let mixed = match mode {
                abi::BLEND_HUE => set_lum(set_sat(s, sat(d)), lum(d)),
                abi::BLEND_SATURATION => set_lum(set_sat(d, sat(s)), lum(d)),
                abi::BLEND_COLOR => set_lum(s, lum(d)),
                _ => set_lum(d, lum(s)),
            };
            mixed.map(quantize)
        }
        _ => [16, 8, 0].map(|shift| channel((dst >> shift) & 255, (src >> shift) & 255, mode)),
    }
}

fn quantize(value: f64) -> u32 {
    let scaled = value.clamp(0., 1.) * 255.;
    let half = scaled.floor() + 0.5;
    // SetLum/SetSat and Soft Light use a bounded number of f64 operations.
    // Snap only their roundoff neighborhood of an exact half-byte boundary;
    // do not let cancellation accidentally turn half-up into half-down.
    const HALF_TIE_BOUND: f64 = 32. * f64::EPSILON * 255.;
    if (scaled - half).abs() <= HALF_TIE_BOUND {
        half.round() as u32
    } else {
        scaled.round() as u32
    }
}

fn ratio(numerator: u32, denominator: u32) -> u32 {
    // Every rational kernel has a positive bounded denominator. Integer
    // arithmetic makes half-up exact, including cancellation-heavy endpoints.
    ((numerator * 2 + denominator) / (denominator * 2)).min(255)
}

fn burn(d: u32, s: u32) -> u32 {
    // W3C precedence: a white backdrop wins even over a black source.
    if d == 255 {
        255
    } else if s == 0 {
        0
    } else {
        ratio(255 * (d + s).saturating_sub(255), s)
    }
}

fn dodge(d: u32, s: u32) -> u32 {
    // W3C precedence: a black backdrop wins even over a white source.
    if d == 0 {
        0
    } else if s == 255 {
        255
    } else {
        ratio(255 * d, 255 - s)
    }
}

fn vivid(d: u32, s: u32) -> u32 {
    if s < 128 {
        burn(d, 2 * s)
    } else if d == 0 {
        0
    } else if s == 255 {
        255
    } else {
        ratio(255 * d, 2 * (255 - s))
    }
}

fn channel(d: u32, s: u32, mode: u32) -> u32 {
    match mode {
        abi::BLEND_COLOR_BURN => burn(d, s),
        abi::BLEND_LINEAR_BURN => (d + s).saturating_sub(255),
        abi::BLEND_COLOR_DODGE => dodge(d, s),
        abi::BLEND_SOFT_LIGHT => {
            let d = d as f64 / 255.;
            let s = s as f64 / 255.;
            let value = if s <= 0.5 {
                d - (1. - 2. * s) * d * (1. - d)
            } else {
                let curve = if d <= 0.25 {
                    ((16. * d - 12.) * d + 4.) * d
                } else {
                    d.sqrt()
                };
                d + (2. * s - 1.) * (curve - d)
            };
            quantize(value)
        }
        abi::BLEND_HARD_LIGHT => {
            if s < 128 {
                ratio(2 * d * s, 255)
            } else {
                ratio(255 * 255 - 2 * (255 - d) * (255 - s), 255)
            }
        }
        abi::BLEND_VIVID_LIGHT => vivid(d, s),
        abi::BLEND_LINEAR_LIGHT => (d + 2 * s).saturating_sub(255).min(255),
        abi::BLEND_PIN_LIGHT => {
            if s < 128 {
                d.min(2 * s)
            } else {
                d.max(2 * s - 255)
            }
        }
        abi::BLEND_HARD_MIX => {
            // Exact unquantized Vivid Light >= 0.5, retaining Dodge's black
            // backdrop precedence at d=0/s=255. Never threshold rounded bytes.
            if d != 0 && d + s >= 255 {
                255
            } else {
                0
            }
        }
        abi::BLEND_EXCLUSION => ratio(255 * (d + s) - 2 * d * s, 255),
        abi::BLEND_SUBTRACT => d.saturating_sub(s),
        abi::BLEND_DIVIDE => {
            // MiCoPi defines division by a black source as white, including 0/0.
            if s == 0 {
                255
            } else {
                ratio(255 * d, s)
            }
        }
        _ => s,
    }
}

fn lum(c: Rgb) -> f64 {
    0.30 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn sat(c: Rgb) -> f64 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_lum(mut c: Rgb, value: f64) -> Rgb {
    let offset = value - lum(c);
    for v in &mut c {
        *v += offset;
    }
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    if n < 0. {
        for v in &mut c {
            *v = l + (*v - l) * l / (l - n);
        }
    }
    if x > 1. {
        for v in &mut c {
            *v = l + (*v - l) * (1. - l) / (x - l);
        }
    }
    c
}

fn set_sat(c: Rgb, value: f64) -> Rgb {
    // A three-element sorting network keeps RGB ties stable without allocation.
    let mut order = [0, 1, 2];
    for (a, b) in [(0, 1), (1, 2), (0, 1)] {
        if c[order[a]] > c[order[b]] {
            order.swap(a, b);
        }
    }
    let [min, mid, max] = order;
    let mut result = [0.; 3];
    if c[max] > c[min] {
        result[mid] = (c[mid] - c[min]) * value / (c[max] - c[min]);
        result[max] = value;
    }
    result
}
