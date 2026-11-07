//! Capped live stroke influence. Original pixels stay in the host's undo tiles.
use crate::Brush;
use micopi_plugin_api as abi;
use std::ffi::c_void;

const SIDE: usize = 64;
const AREA: usize = SIDE * SIDE;
const MAX_TILES: usize = 4096;
const RETAINED_TILES: usize = 64;
const EMPTY: u16 = u16::MAX;
const FULL: u32 = u16::MAX as u32;

#[derive(Default)]
pub(crate) struct Coverage {
    width: usize,
    height: usize,
    columns: usize,
    indices: Vec<u16>,
    tiles: Vec<Vec<u16>>,
    used: usize,
    #[cfg(test)]
    allocations: usize,
}

impl Coverage {
    pub fn begin(&mut self, width: u32, height: u32) -> Result<(), ()> {
        let (width, height) = (width as usize, height as usize);
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err(());
        }
        let columns = width.div_ceil(SIDE);
        let count = columns * height.div_ceil(SIDE);
        if count > MAX_TILES {
            return Err(());
        }
        self.indices.clear();
        self.indices.try_reserve_exact(count).map_err(|_| ())?;
        self.indices.resize(count, EMPTY);
        self.width = width;
        self.height = height;
        self.columns = columns;
        self.used = 0;
        Ok(())
    }

    /// Keep a small cross-contact pool. Active whole-canvas scratch is at most
    /// 32 MiB; retained coverage is at most 512 KiB per cached tool instance.
    pub fn release(&mut self) {
        self.tiles.truncate(RETAINED_TILES);
        self.used = 0;
    }

    fn reserve_region(&mut self, region: abi::Region) -> Result<(), ()> {
        let (x, y, w, h) = (
            region.x as usize,
            region.y as usize,
            region.width as usize,
            region.height as usize,
        );
        if w == 0
            || h == 0
            || x.checked_add(w).is_none_or(|end| end > self.width)
            || y.checked_add(h).is_none_or(|end| end > self.height)
        {
            return Err(());
        }
        for ty in y / SIDE..=(y + h - 1) / SIDE {
            for tx in x / SIDE..=(x + w - 1) / SIDE {
                let index = ty * self.columns + tx;
                if self.indices[index] != EMPTY {
                    continue;
                }
                if self.used >= MAX_TILES {
                    return Err(());
                }
                if self.used == self.tiles.len() {
                    self.tiles.try_reserve(1).map_err(|_| ())?;
                    let mut tile = Vec::new();
                    tile.try_reserve_exact(AREA).map_err(|_| ())?;
                    tile.resize(AREA, 0);
                    self.tiles.push(tile);
                    #[cfg(test)]
                    {
                        self.allocations += 1;
                    }
                } else {
                    self.tiles[self.used].fill(0);
                }
                self.indices[index] = self.used as u16;
                self.used += 1;
            }
        }
        Ok(())
    }

    pub fn dab(
        &mut self,
        host: &abi::ToolHost,
        region: abi::Region,
        brush: Brush,
        x: f32,
        y: f32,
        flow: f32,
        selection: abi::Coverage,
        alpha_locked: bool,
        a8_value: Option<u8>,
    ) -> i32 {
        if !flow.is_finite() || !(0. ..=1.).contains(&flow) {
            return abi::ERROR;
        }
        if flow == 0. || brush.opacity == 0. {
            return abi::OK;
        }
        let Some(loans) = loans(host, a8_value.is_some()) else {
            return abi::ERROR;
        };
        // A fallible allocation happens before any host loan or pixel write.
        if self.reserve_region(region).is_err() {
            return abi::ERROR;
        }
        let mut dab = Dab {
            coverage: self,
            region,
            brush,
            x,
            y,
            flow,
            selection,
            alpha_locked,
            a8_value: if brush.erase {
                255
            } else {
                a8_value.unwrap_or_default()
            },
        };
        unsafe {
            if let Some(loan) = loans.a8 {
                loan(
                    host.context,
                    region,
                    Some(render_a8),
                    (&mut dab as *mut Dab).cast(),
                )
            } else {
                loans.rgba.unwrap()(
                    host.context,
                    region,
                    Some(render_rgba),
                    (&mut dab as *mut Dab).cast(),
                )
            }
        }
    }
}

struct Loans {
    rgba: Option<abi::OriginPixelsLoan>,
    a8: Option<abi::OriginA8Loan>,
}
fn loans(host: &abi::ToolHost, a8: bool) -> Option<Loans> {
    if host.abi_version != abi::TOOL_ABI_VERSION
        || host.struct_size < std::mem::size_of::<abi::ToolHostStroke>() as u32
    {
        return None;
    }
    unsafe {
        let tail = (host as *const abi::ToolHost).cast::<abi::ToolHostStroke>();
        if std::ptr::read_unaligned(std::ptr::addr_of!((*tail).tile_side)) != SIDE as u32 {
            return None;
        }
        let features = std::ptr::read_unaligned(std::ptr::addr_of!((*tail).features));
        if a8 {
            let loan = std::ptr::read_unaligned(std::ptr::addr_of!((*tail).a8_origin));
            (features & abi::TOOL_HOST_ORIGIN_A8 != 0 && loan.is_some()).then_some(Loans {
                rgba: None,
                a8: loan,
            })
        } else {
            let loan = std::ptr::read_unaligned(std::ptr::addr_of!((*tail).pixels_origin));
            (features & abi::TOOL_HOST_ORIGIN_RGBA != 0 && loan.is_some()).then_some(Loans {
                rgba: loan,
                a8: None,
            })
        }
    }
}
pub(crate) fn supported(host: &abi::ToolHost, a8: bool) -> bool {
    loans(host, a8).is_some()
}

struct Dab {
    coverage: *mut Coverage,
    region: abi::Region,
    brush: Brush,
    x: f32,
    y: f32,
    flow: f32,
    selection: abi::Coverage,
    alpha_locked: bool,
    a8_value: u8,
}

fn span(pointer: usize, len: usize, size: usize) -> Option<(usize, usize)> {
    if pointer == 0 || len == 0 {
        return None;
    }
    Some((pointer, pointer.checked_add(len.checked_mul(size)?)?))
}
fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
fn extent(len: usize, width: usize, height: usize, stride: usize) -> bool {
    width > 0
        && height > 0
        && stride >= width
        && (height - 1)
            .checked_mul(stride)
            .and_then(|v| v.checked_add(width))
            .is_some_and(|v| v <= len)
}

/// Both loans are one intersection of a prepared 64-pixel undo tile. Validate
/// all spans before touching coverage or current pixels, including stride gaps.
unsafe fn admitted(
    dab: &Dab,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    origin_stride: u32,
    origin_len: usize,
    output_stride: u32,
    output_len: usize,
    origin_span: (usize, usize),
    output_span: (usize, usize),
    descriptor_spans: [(usize, usize); 2],
    user_span: (usize, usize),
) -> Option<usize> {
    if dab.coverage.is_null() || (dab.coverage as usize) % std::mem::align_of::<Coverage>() != 0 {
        return None;
    }
    let coverage = &*dab.coverage;
    let object = span(dab.coverage as usize, 1, std::mem::size_of::<Coverage>())?;
    let indices = span(
        coverage.indices.as_ptr() as usize,
        coverage.indices.capacity(),
        std::mem::size_of::<u16>(),
    );
    let descriptors = span(
        coverage.tiles.as_ptr() as usize,
        coverage.tiles.capacity(),
        std::mem::size_of::<Vec<u16>>(),
    );
    if [Some(object), indices, descriptors]
        .into_iter()
        .flatten()
        .any(|owned| overlaps(output_span, owned) || overlaps(origin_span, owned))
    {
        return None;
    }
    let (x, y, w, h) = (x as usize, y as usize, w as usize, h as usize);
    let (end_x, end_y) = (x.checked_add(w)?, y.checked_add(h)?);
    let (rx, ry, rw, rh) = (
        dab.region.x as usize,
        dab.region.y as usize,
        dab.region.width as usize,
        dab.region.height as usize,
    );
    if w > SIDE
        || h > SIDE
        || origin_stride as usize > SIDE
        || origin_len > AREA
        || !extent(origin_len, w, h, origin_stride as usize)
        || !extent(output_len, w, h, output_stride as usize)
        || output_len > 4096 * 4096
        || end_x > coverage.width
        || end_y > coverage.height
        || x < rx
        || y < ry
        || end_x > rx.checked_add(rw)?
        || end_y > ry.checked_add(rh)?
        || x / SIDE != (end_x - 1) / SIDE
        || y / SIDE != (end_y - 1) / SIDE
        || !dab.x.is_finite()
        || !dab.y.is_finite()
        || !dab.flow.is_finite()
        || !(0. ..=1.).contains(&dab.flow)
        || !dab.brush.opacity.is_finite()
        || !(0. ..=1.).contains(&dab.brush.opacity)
        || !dab.brush.radius.is_finite()
        || dab.brush.radius <= 0.
        || !dab.brush.hardness.is_finite()
        || !(0. ..=1.).contains(&dab.brush.hardness)
        || !crate::selection::valid(&dab.selection)
        || overlaps(origin_span, output_span)
        || overlaps(output_span, user_span)
        || overlaps(origin_span, user_span)
        || descriptor_spans
            .into_iter()
            .any(|v| overlaps(output_span, v) || overlaps(origin_span, v))
    {
        return None;
    }
    let index = *coverage
        .indices
        .get(y / SIDE * coverage.columns + x / SIDE)?;
    if index == EMPTY {
        return None;
    }
    let tile = coverage.tiles.get(index as usize)?;
    let scratch = span(tile.as_ptr() as usize, tile.len(), 2)?;
    if overlaps(scratch, output_span)
        || overlaps(scratch, origin_span)
        || overlaps(scratch, user_span)
        || descriptor_spans.into_iter().any(|v| overlaps(scratch, v))
    {
        return None;
    }
    if !dab.selection.data.is_null() {
        let selection = &dab.selection;
        if selection.width as usize != coverage.width
            || selection.height as usize != coverage.height
        {
            return None;
        }
        let selected = span(selection.data as usize, selection.len, 1)?;
        if overlaps(selected, output_span) || overlaps(selected, scratch) {
            return None;
        }
    }
    Some(index as usize)
}

fn advance(previous: u16, cap: u16, amount: u32) -> u16 {
    previous + ((u32::from(cap.saturating_sub(previous)) * amount + FULL / 2) / FULL) as u16
}
fn amount(dab: &Dab, px: usize, py: usize) -> u32 {
    let r = dab.brush.radius.max(0.5);
    let dx = px as f32 + 0.5 - dab.x;
    let dy = py as f32 + 0.5 - dab.y;
    let squared = dx * dx + dy * dy;
    if squared > (r + 0.5) * (r + 0.5) {
        return 0;
    }
    let d = if dab.brush.hardness >= 0.999 && squared <= (r - 0.5) * (r - 0.5) {
        0.
    } else {
        dx.hypot(dy)
    };
    let edge = (r - d + 0.5).clamp(0., 1.);
    let soft = if dab.brush.hardness >= 0.999 {
        1.
    } else {
        ((r - d) / (r * (1. - dab.brush.hardness))).clamp(0., 1.)
    };
    (FULL as f32 * dab.flow * edge * soft).round() as u32
}
unsafe fn selected(dab: &Dab, weight: u16, x: usize, y: usize) -> u32 {
    let selection = if dab.selection.data.is_null() {
        255
    } else {
        *dab.selection
            .data
            .add(y * dab.selection.stride as usize + x) as u32
    };
    (u32::from(weight) * selection + FULL / 2) / FULL
}

unsafe extern "C" fn render_rgba(
    user: *mut c_void,
    origin: *const abi::OriginPixels,
    current: *mut abi::Pixels,
) -> i32 {
    if user.is_null()
        || origin.is_null()
        || current.is_null()
        || (user as usize) % std::mem::align_of::<Dab>() != 0
    {
        return abi::ERROR;
    }
    let Some(user_span) = span(user as usize, 1, std::mem::size_of::<Dab>()) else {
        return abi::ERROR;
    };
    let Some(origin_descriptor) =
        span(origin as usize, 1, std::mem::size_of::<abi::OriginPixels>())
    else {
        return abi::ERROR;
    };
    let Some(current_descriptor) = span(current as usize, 1, std::mem::size_of::<abi::Pixels>())
    else {
        return abi::ERROR;
    };
    let dab = &*user.cast::<Dab>();
    let source = std::ptr::read_unaligned(origin);
    let target = std::ptr::read_unaligned(current);
    if (source.data as usize) % std::mem::align_of::<u32>() != 0
        || (target.data as usize) % std::mem::align_of::<u32>() != 0
    {
        return abi::ERROR;
    }
    if (
        source.width,
        source.height,
        source.origin_x,
        source.origin_y,
    ) != (
        target.width,
        target.height,
        target.origin_x,
        target.origin_y,
    ) {
        return abi::ERROR;
    }
    let Some(source_span) = span(source.data as usize, source.len, 4) else {
        return abi::ERROR;
    };
    let Some(output_span) = span(target.data as usize, target.len, 4) else {
        return abi::ERROR;
    };
    let Some(index) = admitted(
        dab,
        target.origin_x,
        target.origin_y,
        target.width,
        target.height,
        source.stride,
        source.len,
        target.stride,
        target.len,
        source_span,
        output_span,
        [origin_descriptor, current_descriptor],
        user_span,
    ) else {
        return abi::ERROR;
    };
    let cap = (FULL as f32 * dab.brush.opacity).round() as u16;
    let tile = &mut (&mut *dab.coverage).tiles[index];
    for row in 0..target.height as usize {
        for column in 0..target.width as usize {
            let (x, y) = (
                target.origin_x as usize + column,
                target.origin_y as usize + row,
            );
            let original = *source.data.add(row * source.stride as usize + column);
            if dab.alpha_locked && (dab.brush.erase || original >> 24 == 0) {
                continue;
            }
            let slot = (y % SIDE) * SIDE + x % SIDE;
            let old = tile[slot];
            let weight = advance(old, cap, amount(dab, x, y));
            if weight == old {
                continue;
            }
            tile[slot] = weight;
            let a = selected(dab, weight, x, y);
            let pixel = if dab.alpha_locked {
                super::raster::atop(original, (dab.brush.color & 0xffffff) | (a << 24))
            } else if dab.brush.erase {
                (original & 0xffffff) | ((((original >> 24) * (255 - a) + 127) / 255) << 24)
            } else {
                super::raster::over(original, (dab.brush.color & 0xffffff) | (a << 24))
            };
            *target.data.add(row * target.stride as usize + column) = pixel;
        }
    }
    abi::OK
}

unsafe extern "C" fn render_a8(
    user: *mut c_void,
    origin: *const abi::OriginA8Pixels,
    current: *mut abi::A8Pixels,
) -> i32 {
    if user.is_null()
        || origin.is_null()
        || current.is_null()
        || (user as usize) % std::mem::align_of::<Dab>() != 0
    {
        return abi::ERROR;
    }
    let Some(user_span) = span(user as usize, 1, std::mem::size_of::<Dab>()) else {
        return abi::ERROR;
    };
    let Some(origin_descriptor) = span(
        origin as usize,
        1,
        std::mem::size_of::<abi::OriginA8Pixels>(),
    ) else {
        return abi::ERROR;
    };
    let Some(current_descriptor) = span(current as usize, 1, std::mem::size_of::<abi::A8Pixels>())
    else {
        return abi::ERROR;
    };
    let dab = &*user.cast::<Dab>();
    let source = std::ptr::read_unaligned(origin);
    let target = std::ptr::read_unaligned(current);
    if (
        source.width,
        source.height,
        source.origin_x,
        source.origin_y,
    ) != (
        target.width,
        target.height,
        target.origin_x,
        target.origin_y,
    ) {
        return abi::ERROR;
    }
    let Some(source_span) = span(source.data as usize, source.len, 1) else {
        return abi::ERROR;
    };
    let Some(output_span) = span(target.data as usize, target.len, 1) else {
        return abi::ERROR;
    };
    let Some(index) = admitted(
        dab,
        target.origin_x,
        target.origin_y,
        target.width,
        target.height,
        source.stride,
        source.len,
        target.stride,
        target.len,
        source_span,
        output_span,
        [origin_descriptor, current_descriptor],
        user_span,
    ) else {
        return abi::ERROR;
    };
    let cap = (FULL as f32 * dab.brush.opacity).round() as u16;
    let tile = &mut (&mut *dab.coverage).tiles[index];
    for row in 0..target.height as usize {
        for column in 0..target.width as usize {
            let (x, y) = (
                target.origin_x as usize + column,
                target.origin_y as usize + row,
            );
            let slot = (y % SIDE) * SIDE + x % SIDE;
            let old = tile[slot];
            let weight = advance(old, cap, amount(dab, x, y));
            if old == weight {
                continue;
            }
            tile[slot] = weight;
            let a = selected(dab, weight, x, y);
            let original = *source.data.add(row * source.stride as usize + column) as u32;
            *target.data.add(row * target.stride as usize + column) =
                ((original * (255 - a) + u32::from(dab.a8_value) * a + 127) / 255) as u8;
        }
    }
    abi::OK
}

#[cfg(test)]
#[path = "stroke_tests.rs"]
mod tests;
