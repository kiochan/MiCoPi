use crate::Brush;
use micopi_plugin_api as abi;
use std::ffi::c_void;
#[derive(Default)]
pub struct Raster {
    pub last: Option<(f32, f32)>,
    pub last_brush: Option<Brush>,
    pub distance: f32,
    pub width: u32,
    pub height: u32,
    pub failed: bool,
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
            let spacing = (b.radius * 0.22).max(0.5);
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
        let mut dab = Dab { x, y, brush: b };
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
            *pixel = if b.erase {
                (old & 0xffffff) | ((((old >> 24) * (255 - a) + 127) / 255) << 24)
            } else {
                over(old, (b.color & 0xffffff) | (a << 24))
            };
        }
    }
    abi::OK
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
