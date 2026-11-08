use crate::Brush;
use micopi_plugin_api as abi;
use std::ffi::c_void;
pub struct Raster {
    pub selection: abi::Coverage,
    pub alpha_locked: bool,
    pub a8_value: Option<u8>,
    pub last: Option<(f32, f32)>,
    pub last_brush: Option<Brush>,
    pub distance: f32,
    pub width: u32,
    pub height: u32,
    pub failed: bool,
    pub spacing: f32,
    pub flow: f32,
    pub stroke: Option<crate::stroke::Coverage>,
    pub(crate) tip: crate::tip::Tip,
    pub(crate) symmetry: crate::symmetry::Symmetry,
}
impl Default for Raster {
    fn default() -> Self {
        Self {
            selection: Default::default(),
            alpha_locked: false,
            a8_value: None,
            last: None,
            last_brush: None,
            distance: 0.,
            width: 0,
            height: 0,
            failed: false,
            spacing: 0.,
            flow: 1.,
            stroke: None,
            tip: Default::default(),
            symmetry: Default::default(),
        }
    }
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
            let spacing = if self.tip.legacy() {
                (b.radius * (self.spacing * 2.)).max(0.5)
            } else {
                (b.radius * (self.spacing * 2.) * self.tip.spacing_scale()).max(0.5)
            };
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
        if self.symmetry.axes == 0 {
            self.dab_one(host, x, y, b);
            return;
        }
        let original = self.tip;
        let (stamps, count) = self.symmetry.stamps(x, y, original);
        for stamp in &stamps[..count] {
            self.tip = stamp.tip;
            self.dab_one(host, stamp.x, stamp.y, b);
            if self.failed {
                break;
            }
        }
        self.tip = original;
    }
    fn dab_one(&mut self, host: &abi::ToolHost, x: f32, y: f32, mut b: Brush) {
        if self.failed || b.opacity <= 0. || b.radius <= 0. {
            return;
        }
        let r = b.radius.max(0.5);
        let geometry = self.tip.geometry(r);
        let (x0, y0, x1, y1) = if self.tip.legacy() {
            (
                (x - r - 1.).max(0.).min(self.width as f32) as u32,
                (y - r - 1.).max(0.).min(self.height as f32) as u32,
                (x + r + 1.).ceil().max(0.).min(self.width as f32) as u32,
                (y + r + 1.).ceil().max(0.).min(self.height as f32) as u32,
            )
        } else {
            let region = geometry.region(x, y, self.width, self.height);
            (
                region.x,
                region.y,
                region.x + region.width,
                region.y + region.height,
            )
        };
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        #[cfg(test)]
        crate::tip::probe::dab();
        if let Some(stroke) = &mut self.stroke {
            self.failed = stroke.dab_tip(
                host,
                abi::Region {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                },
                b,
                x,
                y,
                self.flow,
                self.selection,
                self.alpha_locked,
                self.a8_value,
                geometry,
            ) != abi::OK;
            return;
        }
        if self.flow != 1. {
            b.opacity *= self.flow;
        }
        if b.opacity <= 0. {
            return;
        }
        if let Some(value) = self.a8_value {
            let Some(loan) = a8_loan(host) else {
                self.failed = true;
                return;
            };
            let mut dab = A8Dab {
                selection: self.selection,
                x,
                y,
                brush: b,
                width: self.width,
                height: self.height,
                value: if b.erase { 255 } else { value },
                geometry,
            };
            #[cfg(test)]
            crate::tip::probe::loan(abi::Region {
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
            });
            self.failed = unsafe {
                loan(
                    host.context,
                    abi::Region {
                        x: x0,
                        y: y0,
                        width: x1 - x0,
                        height: y1 - y0,
                    },
                    Some(render_a8),
                    (&mut dab as *mut A8Dab).cast(),
                )
            } != abi::OK;
            return;
        }
        let mut dab = Dab {
            selection: self.selection,
            x,
            y,
            brush: b,
            alpha_locked: self.alpha_locked,
            geometry,
        };
        #[cfg(test)]
        crate::tip::probe::loan(abi::Region {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        });
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
type A8Loan =
    unsafe extern "C" fn(*mut c_void, abi::Region, Option<abi::A8Transform>, *mut c_void) -> i32;
pub(crate) fn a8_loan(host: &abi::ToolHost) -> Option<A8Loan> {
    if host.abi_version != abi::TOOL_ABI_VERSION
        || host.struct_size < std::mem::size_of::<abi::ToolHostA8>() as u32
    {
        return None;
    }
    unsafe {
        std::ptr::read_unaligned(std::ptr::addr_of!(
            (*(host as *const abi::ToolHost).cast::<abi::ToolHostA8>()).a8
        ))
    }
}
#[derive(Clone, Copy)]
struct A8Dab {
    selection: abi::Coverage,
    x: f32,
    y: f32,
    brush: Brush,
    width: u32,
    height: u32,
    value: u8,
    geometry: crate::tip::Geometry,
}
fn a8_span(start: usize, len: usize) -> Option<(usize, usize)> {
    (len <= isize::MAX as usize)
        .then(|| start.checked_add(len).map(|end| (start, end)))
        .flatten()
}
fn a8_overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
unsafe extern "C" fn render_a8(user: *mut c_void, buffer: *mut abi::A8Pixels) -> i32 {
    if user.is_null()
        || buffer.is_null()
        || user as usize % std::mem::align_of::<A8Dab>() != 0
        || buffer as usize % std::mem::align_of::<abi::A8Pixels>() != 0
    {
        return abi::ERROR;
    }
    let Some(user_span) = a8_span(user as usize, std::mem::size_of::<A8Dab>()) else {
        return abi::ERROR;
    };
    let Some(descriptor_span) = a8_span(buffer as usize, std::mem::size_of::<abi::A8Pixels>())
    else {
        return abi::ERROR;
    };
    if a8_overlaps(user_span, descriptor_span) {
        return abi::ERROR;
    }
    // Snapshot plain metadata without creating references to host storage. The
    // writable span must be disjoint from both originals, including the inline
    // selection descriptor in A8Dab, before a mutable byte slice can be formed.
    let dab = std::ptr::read(user.cast::<A8Dab>());
    let buffer = std::ptr::read(buffer);
    let (w, h, stride) = (
        buffer.width as usize,
        buffer.height as usize,
        buffer.stride as usize,
    );
    let b = dab.brush;
    if buffer.data.is_null()
        || w == 0
        || h == 0
        || stride < w
        || stride > 4096
        || dab.width == 0
        || dab.height == 0
        || dab.width > 4096
        || dab.height > 4096
        || buffer
            .origin_x
            .checked_add(buffer.width)
            .is_none_or(|v| v > dab.width)
        || buffer
            .origin_y
            .checked_add(buffer.height)
            .is_none_or(|v| v > dab.height)
        || (h - 1)
            .checked_mul(stride)
            .and_then(|v| v.checked_add(w))
            .is_none_or(|v| v > buffer.len)
        || buffer.len > 4096 * 4096
        || !dab.x.is_finite()
        || !dab.y.is_finite()
        || !b.radius.is_finite()
        || b.radius <= 0.
        || !b.opacity.is_finite()
        || !(0. ..=1.).contains(&b.opacity)
        || !b.hardness.is_finite()
        || !(0. ..=1.).contains(&b.hardness)
        || !crate::selection::valid(&dab.selection)
    {
        return abi::ERROR;
    }
    let Some(output_span) = a8_span(buffer.data as usize, buffer.len) else {
        return abi::ERROR;
    };
    if a8_overlaps(output_span, descriptor_span) || a8_overlaps(output_span, user_span) {
        return abi::ERROR;
    }
    if !dab.selection.data.is_null() {
        let mask = &dab.selection;
        let Some(mask_span) = a8_span(mask.data as usize, mask.len) else {
            return abi::ERROR;
        };
        if (mask.width, mask.height) != (dab.width, dab.height)
            || a8_overlaps(output_span, mask_span)
        {
            return abi::ERROR;
        }
    }
    let pixels = std::slice::from_raw_parts_mut(buffer.data, buffer.len);
    if !dab.geometry.legacy() {
        return render_tip_a8(&dab, &buffer, pixels);
    }
    let r = b.radius.max(0.5);
    for row in 0..h {
        #[cfg(test)]
        crate::tip::probe::candidates(w);
        let py = buffer.origin_y as usize + row;
        for column in 0..w {
            let px = buffer.origin_x as usize + column;
            let dx = px as f32 + 0.5 - dab.x;
            let dy = py as f32 + 0.5 - dab.y;
            let squared = dx * dx + dy * dy;
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
            let mut a = (255. * b.opacity * edge * soft).round() as u32;
            if !dab.selection.data.is_null() {
                let coverage =
                    *dab.selection
                        .data
                        .add(py * dab.selection.stride as usize + px) as u32;
                a = (a * coverage + 127) / 255;
            }
            if a != 0 {
                #[cfg(test)]
                crate::tip::probe::positive();
                let pixel = &mut pixels[row * stride + column];
                *pixel = ((*pixel as u32 * (255 - a) + dab.value as u32 * a + 127) / 255) as u8;
            }
        }
    }
    abi::OK
}
fn render_tip_a8(dab: &A8Dab, buffer: &abi::A8Pixels, pixels: &mut [u8]) -> i32 {
    let b = dab.brush;
    let start = buffer.origin_x as usize;
    let end = start + buffer.width as usize;
    for row in 0..buffer.height as usize {
        let py = buffer.origin_y as usize + row;
        let Some((lo, hi)) = dab
            .geometry
            .row_span(dab.x, py as f32 + 0.5 - dab.y, start, end)
        else {
            continue;
        };
        #[cfg(test)]
        crate::tip::probe::candidates(hi - lo);
        for px in lo..hi {
            let weight =
                dab.geometry
                    .coverage(px as f32 + 0.5 - dab.x, py as f32 + 0.5 - dab.y, b.hardness);
            let mut a = (255. * b.opacity * weight).round() as u32;
            if !dab.selection.data.is_null() {
                let coverage = unsafe {
                    *dab.selection
                        .data
                        .add(py * dab.selection.stride as usize + px)
                } as u32;
                a = (a * coverage + 127) / 255;
            }
            if a != 0 {
                #[cfg(test)]
                crate::tip::probe::positive();
                let pixel = &mut pixels[row * buffer.stride as usize + px - start];
                *pixel =
                    ((u32::from(*pixel) * (255 - a) + u32::from(dab.value) * a + 127) / 255) as u8;
            }
        }
    }
    abi::OK
}
fn render_tip_rgba(dab: &Dab, buffer: &abi::Pixels, pixels: &mut [u32]) -> i32 {
    let b = dab.brush;
    let start = buffer.origin_x as usize;
    let end = start + buffer.width as usize;
    for row in 0..buffer.height as usize {
        let py = buffer.origin_y as usize + row;
        let Some((lo, hi)) = dab
            .geometry
            .row_span(dab.x, py as f32 + 0.5 - dab.y, start, end)
        else {
            continue;
        };
        #[cfg(test)]
        crate::tip::probe::candidates(hi - lo);
        for px in lo..hi {
            let weight =
                dab.geometry
                    .coverage(px as f32 + 0.5 - dab.x, py as f32 + 0.5 - dab.y, b.hardness);
            let mut a = (255. * b.opacity * weight).round() as u32;
            if !dab.selection.data.is_null() {
                if px >= dab.selection.width as usize || py >= dab.selection.height as usize {
                    return abi::ERROR;
                }
                let coverage = unsafe {
                    *dab.selection
                        .data
                        .add(py * dab.selection.stride as usize + px)
                } as u32;
                a = (a * coverage + 127) / 255;
            }
            if a == 0 {
                continue;
            }
            #[cfg(test)]
            crate::tip::probe::positive();
            let pixel = &mut pixels[row * buffer.stride as usize + px - start];
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
struct Dab {
    selection: abi::Coverage,
    alpha_locked: bool,
    x: f32,
    y: f32,
    brush: Brush,
    geometry: crate::tip::Geometry,
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
    if !dab.geometry.legacy() {
        return render_tip_rgba(dab, buffer, pixels);
    }
    let b = dab.brush;
    let r = b.radius.max(0.5);
    for row in 0..h {
        #[cfg(test)]
        crate::tip::probe::candidates(w);
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
            let mut a = (255. * b.opacity * edge * soft).round() as u32;
            if !dab.selection.data.is_null() {
                if px >= dab.selection.width as usize || py >= dab.selection.height as usize {
                    return abi::ERROR;
                }
                let coverage =
                    *dab.selection
                        .data
                        .add(py * dab.selection.stride as usize + px) as u32;
                a = (a * coverage + 127) / 255;
            }
            if a == 0 {
                continue;
            }
            #[cfg(test)]
            crate::tip::probe::positive();
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
pub(super) fn atop(dst: u32, src: u32) -> u32 {
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
#[cfg(test)]
mod a8_sink_tests {
    use super::*;
    fn dab(value: u8) -> A8Dab {
        A8Dab {
            selection: Default::default(),
            x: 4.,
            y: 2.,
            brush: Brush {
                radius: 20.,
                opacity: 0.5,
                hardness: 1.,
                color: 0,
                erase: false,
            },
            width: 8,
            height: 4,
            value,
            geometry: Default::default(),
        }
    }
    fn region(data: &mut [u8]) -> abi::A8Pixels {
        abi::A8Pixels {
            data: data.as_mut_ptr(),
            len: data.len(),
            width: 3,
            height: 2,
            stride: 6,
            origin_x: 2,
            origin_y: 1,
        }
    }
    #[test]
    fn strided_a8_dabs_clip_once_and_preserve_row_gaps() {
        let selection: Vec<u8> = (0..40).map(|i| [0, 64, 128, 255][i % 4]).collect();
        let mut d = dab(219);
        d.selection = abi::Coverage {
            data: selection.as_ptr(),
            len: selection.len(),
            width: 8,
            height: 4,
            stride: 10,
        };
        let mut data = vec![37u8; 12];
        let before = data.clone();
        let mut output = region(&mut data);
        assert_eq!(
            unsafe { render_a8((&mut d as *mut A8Dab).cast(), &mut output) },
            abi::OK
        );
        for row in 0..2 {
            for column in 0..6 {
                let at = row * 6 + column;
                let expected = if column < 3 {
                    let m = selection[(row + 1) * 10 + column + 2] as u32;
                    let alpha = (128 * m + 127) / 255;
                    (37. + (219. - 37.) * (alpha as f64 / 255.)).round() as u8
                } else {
                    before[at]
                };
                assert_eq!(data[at], expected, "row{row} column{column}");
            }
        }
    }
    #[test]
    fn a8_rejects_invalid_storage_geometry_and_selection_aliases_before_writes() {
        for case in 0..16 {
            let mut data = vec![37u8; 40];
            let before = data.clone();
            let mut d = dab(219);
            let mut output = region(&mut data);
            match case {
                0 => output.data = std::ptr::null_mut(),
                1 => output.width = 0,
                2 => output.height = 0,
                3 => output.stride = 2,
                4 => output.len = 8,
                5 => output.origin_x = u32::MAX,
                6 => output.origin_y = 3,
                7 => output.len = 4096 * 4096 + 1,
                8 => output.data = (usize::MAX - 3) as *mut u8,
                9 => {
                    d.selection = abi::Coverage {
                        data: data.as_ptr(),
                        len: 32,
                        width: 8,
                        height: 4,
                        stride: 8,
                    }
                }
                10 => {
                    d.selection = abi::Coverage {
                        data: std::ptr::null(),
                        len: 1,
                        width: 0,
                        height: 0,
                        stride: 0,
                    }
                }
                11 => d.x = f32::NAN,
                12 => d.brush.opacity = f32::INFINITY,
                13 => d.brush.radius = f32::NAN,
                14 => d.brush.hardness = -0.1,
                _ => d.width = 4097,
            }
            assert_eq!(
                unsafe { render_a8((&mut d as *mut A8Dab).cast(), &mut output) },
                abi::ERROR,
                "case{case}"
            );
            assert_eq!(data, before, "case{case} wrote before rejection");
        }
        assert_eq!(
            unsafe { render_a8(std::ptr::null_mut(), std::ptr::null_mut()) },
            abi::ERROR
        );
    }
    #[repr(C)]
    struct Guarded<T> {
        before: [u8; 16],
        value: T,
        after: [u8; 16],
    }
    impl<T> Guarded<T> {
        fn new(value: T) -> Self {
            Self {
                before: [0xa5; 16],
                value,
                after: [0x5a; 16],
            }
        }
        fn assert_guards(&self) {
            assert_eq!(self.before, [0xa5; 16]);
            assert_eq!(self.after, [0x5a; 16]);
        }
    }
    #[test]
    fn a8_rejects_misaligned_or_overflowing_metadata_before_dereferencing() {
        #[repr(align(8))]
        struct Storage([u8; 256]);
        let mut storage = Storage([0x67; 256]);
        let mut data = vec![37u8; 40];
        let mut d = Guarded::new(dab(219));
        let mut output = Guarded::new(region(&mut data));
        let user = (&mut d.value as *mut A8Dab).cast();
        let descriptor = &mut output.value as *mut abi::A8Pixels;
        let bad_user = unsafe { storage.0.as_mut_ptr().add(1) }.cast();
        let bad_descriptor = unsafe { storage.0.as_mut_ptr().add(1) }.cast();
        assert_eq!(unsafe { render_a8(bad_user, descriptor) }, abi::ERROR);
        assert_eq!(unsafe { render_a8(user, bad_descriptor) }, abi::ERROR);
        let overflowing_user = (usize::MAX & !(std::mem::align_of::<A8Dab>() - 1)) as *mut c_void;
        let overflowing_descriptor =
            (usize::MAX & !(std::mem::align_of::<abi::A8Pixels>() - 1)) as *mut abi::A8Pixels;
        assert_eq!(
            unsafe { render_a8(overflowing_user, descriptor) },
            abi::ERROR
        );
        assert_eq!(
            unsafe { render_a8(user, overflowing_descriptor) },
            abi::ERROR
        );
        assert_eq!(storage.0, [0x67; 256]);
        assert_eq!(data, vec![37u8; 40]);
        assert_eq!(d.value.x, 4.);
        assert_eq!(output.value.width, 3);
        d.assert_guards();
        output.assert_guards();
    }
    #[test]
    fn a8_rejects_output_aliases_with_descriptor_user_and_inline_selection_storage() {
        for alias_selection in [false, true] {
            let mut data = vec![37u8; 40];
            let mut d = Guarded::new(dab(219));
            let mut output = Guarded::new(region(&mut data));
            output.value.data = if alias_selection {
                std::ptr::addr_of_mut!(d.value.selection.width).cast()
            } else {
                std::ptr::addr_of_mut!(d.value.x).cast()
            };
            assert_eq!(
                unsafe { render_a8((&mut d.value as *mut A8Dab).cast(), &mut output.value) },
                abi::ERROR
            );
            assert_eq!(d.value.x, 4.);
            assert_eq!(d.value.y, 2.);
            assert_eq!(
                (
                    d.value.selection.width,
                    d.value.selection.height,
                    d.value.selection.stride
                ),
                (0, 0, 0)
            );
            assert_eq!(output.value.width, 3);
            assert_eq!(data, vec![37u8; 40]);
            d.assert_guards();
            output.assert_guards();
        }
        let mut data = vec![37u8; 40];
        let mut d = Guarded::new(dab(219));
        let mut output = Guarded::new(region(&mut data));
        output.value.data = std::ptr::addr_of_mut!(output.value.width).cast();
        assert_eq!(
            unsafe { render_a8((&mut d.value as *mut A8Dab).cast(), &mut output.value) },
            abi::ERROR
        );
        assert_eq!(
            (
                output.value.width,
                output.value.height,
                output.value.stride,
                output.value.origin_x,
                output.value.origin_y
            ),
            (3, 2, 6, 2, 1)
        );
        let user = (&mut d.value as *mut A8Dab).cast::<c_void>();
        assert_eq!(unsafe { render_a8(user, user.cast()) }, abi::ERROR);
        assert_eq!(d.value.x, 4.);
        assert_eq!(data, vec![37u8; 40]);
        d.assert_guards();
        output.assert_guards();
    }
    #[test]
    fn a8_all_byte_pairs_match_float_reference_at_opacity_boundaries() {
        let mut d = dab(0);
        d.width = 1;
        d.height = 1;
        d.x = 0.5;
        d.y = 0.5;
        let mut byte = 0u8;
        let mut output = abi::A8Pixels {
            data: &mut byte,
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        for alpha in [0u32, 1, 64, 127, 128, 254, 255] {
            d.brush.opacity = alpha as f32 / 255.;
            for before in 0u32..=255 {
                for after in 0u32..=255 {
                    byte = before as u8;
                    d.value = after as u8;
                    let expected = (before as f64
                        + (after as f64 - before as f64) * (alpha as f64 / 255.))
                        .round() as u8;
                    assert_eq!(
                        unsafe { render_a8((&mut d as *mut A8Dab).cast(), &mut output) },
                        abi::OK
                    );
                    assert_eq!(byte, expected, "{before}/{after}/{alpha}");
                }
            }
        }
    }
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
    fn fractional_mask_scales_dab_opacity_without_touching_unselected_pixels() {
        let coverage = [0u8, 64, 128, 255];
        let mut pixels = [0x00123456; 4];
        let mut buffer = abi::Pixels {
            data: pixels.as_mut_ptr(),
            len: 4,
            width: 4,
            height: 1,
            stride: 4,
            origin_x: 0,
            origin_y: 0,
        };
        let mut dab = Dab {
            selection: abi::Coverage {
                data: coverage.as_ptr(),
                len: 4,
                width: 4,
                height: 1,
                stride: 4,
            },
            alpha_locked: false,
            geometry: Default::default(),
            x: 2.,
            y: 0.5,
            brush: Brush {
                radius: 20.,
                opacity: 1.,
                hardness: 1.,
                color: 0xffaabbcc,
                erase: false,
            },
        };
        assert_eq!(
            unsafe { render((&mut dab as *mut Dab).cast(), &mut buffer) },
            abi::OK
        );
        assert_eq!(pixels, [0x00123456, 0x40aabbcc, 0x80aabbcc, 0xffaabbcc]);
    }
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
            geometry: Default::default(),
            selection: Default::default(),
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
