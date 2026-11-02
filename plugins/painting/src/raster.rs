use crate::Brush;
use micopi_plugin_api as abi;
use std::ffi::c_void;
#[derive(Default)]
pub struct Raster {
    pub alpha_locked: bool,
    pub last: Option<(f32, f32)>,
    pub last_brush: Option<Brush>,
    pub distance: f32,
    pub width: u32,
    pub height: u32,
    pub failed: bool,
    pub spacing: f32,
}
impl Raster {
    pub fn begin(&mut self, host: &abi::ToolHost, x: f32, y: f32, brush: Brush) {
        self.last = Some((x, y));
        self.last_brush = Some(brush);
        self.distance = 0.;
        self.dab(host, x, y, brush);
    }
    pub fn line(&mut self, host: &abi::ToolHost, x: f32, y: f32, b: Brush) {
        if let Some((px, py)) = self.last {
            let dx = x - px;
            let dy = y - py;
            let len = dx.hypot(dy);
            let spacing = (b.radius * (self.spacing * 2.)).max(0.5);
            let previous = self.last_brush.unwrap_or(b);
            if len < 0.0001 && previous.opacity <= 0. && b.opacity > 0. {
                self.dab(host, x, y, b);
            }
            // A rapid pressure decrease can shrink spacing below the old residual.
            // Never extrapolate a dab backwards along the incoming segment.
            let mut d = (spacing - self.distance).max(0.);
            if len < 0.0001 {
                self.last = Some((x, y));
                self.last_brush = Some(b);
                return;
            }
            while d <= len {
                let t = d / len;
                self.dab(
                    host,
                    px + dx * t,
                    py + dy * t,
                    Brush {
                        radius: previous.radius + (b.radius - previous.radius) * t,
                        opacity: previous.opacity + (b.opacity - previous.opacity) * t,
                        ..b
                    },
                );
                d += spacing;
            }
            self.distance = (self.distance + len) % spacing;
        }
        self.last = Some((x, y));
        self.last_brush = Some(b);
    }

    fn dab(&mut self, host: &abi::ToolHost, x: f32, y: f32, b: Brush) {
        if self.failed || b.opacity <= 0. || b.radius <= 0. {
            return;
        }
        let r = b.radius.max(0.5);
        let x0 = (x - r - 1.).max(0.).min(self.width as f32) as u32;
        let y0 = (y - r - 1.).max(0.).min(self.height as f32) as u32;
        let x1 = (x + r + 1.).ceil().max(0.).min(self.width as f32) as u32;
        let y1 = (y + r + 1.).ceil().max(0.).min(self.height as f32) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let mut dab = Dab {
            x,
            y,
            brush: b,
            alpha_locked: self.alpha_locked,
        };
        self.failed = unsafe {
            (host.pixels)(
                host.context,
                abi::Region {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                },
                Some(render),
                (&mut dab as *mut Dab).cast(),
            )
        } != abi::OK;
    }
}
struct Dab {
    alpha_locked: bool,
    x: f32,
    y: f32,
    brush: Brush,
}
unsafe extern "C" fn render(user: *mut c_void, buffer: *mut abi::Pixels) -> i32 {
    if user.is_null() || buffer.is_null() {
        return abi::ERROR;
    }
    let dab = &*user.cast::<Dab>();
    let buffer = &mut *buffer;
    let w = buffer.width as usize;
    let h = buffer.height as usize;
    let stride = buffer.stride as usize;
    if w == 0
        || h == 0
        || stride < w
        || (h - 1)
            .checked_mul(stride)
            .and_then(|v| v.checked_add(w))
            .is_none_or(|v| v > buffer.len)
        || buffer.data.is_null()
    {
        return abi::ERROR;
    }
    let pixels = std::slice::from_raw_parts_mut(buffer.data, buffer.len);
    let b = dab.brush;
    let r = b.radius.max(0.5);
    for row in 0..h {
        let py = buffer.origin_y as usize + row;
        for column in 0..w {
            let px = buffer.origin_x as usize + column;
            let dx = px as f32 + 0.5 - dab.x;
            let dy = py as f32 + 0.5 - dab.y;
            let squared = dx * dx + dy * dy;
            // Outside pixels have zero coverage. Solid interior pixels need no
            // distance evaluation; retain hypot on the antialiased boundary.
            if squared > (r + 0.5) * (r + 0.5) {
                continue;
            }
            let d = if b.hardness >= 0.999 && r >= 0.5 && squared <= (r - 0.5) * (r - 0.5) {
                0.
            } else {
                dx.hypot(dy)
            };
            let edge = (r - d + 0.5).max(0.).min(1.);
            let soft = if b.hardness >= 0.999 {
                1.
            } else {
                ((r - d) / (r * (1. - b.hardness))).max(0.).min(1.)
            };
            let a = (255. * b.opacity * edge * soft).round() as u32;
            if a == 0 {
                continue;
            }
            let pixel = &mut pixels[row * stride + column];
            let old = *pixel;
            *pixel = if dab.alpha_locked {
                if b.erase {
                    old
                } else {
                    atop(old, (b.color & 0xffffff) | (a << 24))
                }
            } else if b.erase {
                (old & 0xffffff) | ((((old >> 24) * (255 - a) + 127) / 255) << 24)
            } else {
                over(old, (b.color & 0xffffff) | (a << 24))
            };
        }
    }
    abi::OK
}
fn atop(dst: u32, src: u32) -> u32 {
    if dst >> 24 == 0 {
        return dst;
    }
    let a = src >> 24;
    let mut out = dst & 0xff000000;
    for s in [0, 8, 16] {
        out |= ((((src >> s) & 255) * a + ((dst >> s) & 255) * (255 - a) + 127) / 255) << s;
    }
    out
}
pub fn over(dst: u32, src: u32) -> u32 {
    let a = src >> 24;
    if a == 0 {
        return dst;
    }
    if a == 255 {
        return src;
    }
    let da = dst >> 24;
    let out_a = a + (da * (255 - a) + 127) / 255;
    let mut out = out_a << 24;
    for shift in [0, 8, 16] {
        let s = (src >> shift) & 255;
        let d = (dst >> shift) & 255;
        let c = (s * a * 255 + d * da * (255 - a) + out_a * 127) / (out_a * 255);
        out |= c.min(255) << shift;
    }
    out
}

#[cfg(test)]
mod spacing_tests {
    use super::*;
    #[test]
    fn locked_dabs_recolor_partial_alpha_and_preserve_transparent_storage() {
        let original = [0x00abcdef, 0x40112233, 0x80112233, 0xff112233];
        let mut p = original;
        let mut buffer = abi::Pixels {
            data: p.as_mut_ptr(),
            len: 4,
            width: 4,
            height: 1,
            stride: 4,
            origin_x: 0,
            origin_y: 0,
        };
        let mut dab = Dab {
            x: 2.,
            y: 0.5,
            alpha_locked: true,
            brush: Brush {
                radius: 20.,
                opacity: 1.,
                hardness: 1.,
                color: 0x00ff00,
                erase: false,
            },
        };
        assert_eq!(
            unsafe { render((&mut dab as *mut Dab).cast(), &mut buffer) },
            abi::OK
        );
        assert_eq!(p, [0x00abcdef, 0x4000ff00, 0x8000ff00, 0xff00ff00]);
        p.copy_from_slice(&original);
        dab.brush.erase = true;
        assert_eq!(
            unsafe { render((&mut dab as *mut Dab).cast(), &mut buffer) },
            abi::OK
        );
        assert_eq!(p, original);
        assert_eq!(atop(0x80112233, 0x8000ff00), 0x80089119);
    }
    unsafe extern "C" fn count(
        ctx: *mut c_void,
        _: abi::Region,
        _: Option<abi::Transform>,
        _: *mut c_void,
    ) -> i32 {
        *ctx.cast::<usize>() += 1;
        abi::OK
    }
    unsafe extern "C" fn pick(_: *mut c_void, _: f32, _: f32, _: *mut u32) -> i32 {
        abi::ERROR
    }
    #[test]
    fn larger_diameter_spacing_reduces_dab_callbacks() {
        let count_dabs = |spacing| {
            let mut n = 0usize;
            let host = abi::ToolHost {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::ToolHost>() as u32,
                context: (&mut n as *mut usize).cast(),
                pixels: count,
                pick,
            };
            let b = Brush {
                radius: 5.,
                opacity: 1.,
                hardness: 1.,
                color: 0,
                erase: false,
            };
            let mut raster = Raster {
                width: 256,
                height: 64,
                spacing,
                ..Default::default()
            };
            raster.begin(&host, 10., 20., b);
            raster.line(&host, 110., 20., b);
            n
        };
        assert_eq!(count_dabs(1.), 11);
        assert!(count_dabs(0.11) > 80);
    }
}
