//! Independent full-frame A8 sampling over immutable dense or uniform sources.
use super::{geometry, resample};
use micopi_plugin_api as abi;

const SIDE: u32 = 4096;
const AREA: usize = SIDE as usize * SIDE as usize;

fn span<T>(pointer: *const T, count: usize) -> Option<(usize, usize)> {
    let bytes = count.checked_mul(std::mem::size_of::<T>())?;
    if pointer.is_null()
        || pointer as usize % std::mem::align_of::<T>() != 0
        || bytes > isize::MAX as usize
    {
        return None;
    }
    Some((pointer as usize, (pointer as usize).checked_add(bytes)?))
}
fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
fn dimensions(width: u32, height: u32) -> bool {
    (1..=SIDE).contains(&width) && (1..=SIDE).contains(&height)
}
fn layout(width: u32, height: u32, stride: u32, len: usize) -> bool {
    dimensions(width, height)
        && stride >= width
        && stride <= SIDE
        && len <= AREA
        && (height as usize - 1)
            .checked_mul(stride as usize)
            .and_then(|offset| offset.checked_add(width as usize))
            .is_some_and(|needed| needed <= len)
}

struct Validated<R> {
    request: R,
    source: abi::CompositeMask,
    output: abi::A8Pixels,
}
// Copy validated metadata before forming pixel references: an output pointer
// may alias caller-owned request/descriptor storage, which must reject first.
unsafe fn validate<R>(
    request: *const R,
    source: *const abi::CompositeMask,
    sw: u32,
    sh: u32,
    output: *mut abi::A8Pixels,
) -> Option<Validated<R>> {
    if !dimensions(sw, sh) {
        return None;
    }
    let metadata = [span(request, 1)?, span(source, 1)?, span(output, 1)?];
    let request = std::ptr::read(request);
    let source = std::ptr::read(source);
    let output = std::ptr::read(output);
    if !layout(output.width, output.height, output.stride, output.len)
        || output.origin_x != 0
        || output.origin_y != 0
    {
        return None;
    }
    let target = span(output.data, output.len)?;
    if metadata
        .into_iter()
        .any(|metadata| overlaps(target, metadata))
    {
        return None;
    }
    if source.data.is_null() {
        if source.len != 0 || source.stride != 0 || source.constant > 255 {
            return None;
        }
    } else {
        if source.constant != 0 || !layout(sw, sh, source.stride, source.len) {
            return None;
        }
        if overlaps(target, span(source.data, source.len)?) {
            return None;
        }
    }
    Some(Validated {
        request,
        source,
        output,
    })
}
fn fill(output: &mut [u8], target: &abi::A8Pixels, value: u8) {
    for y in 0..target.height as usize {
        output[y * target.stride as usize..y * target.stride as usize + target.width as usize]
            .fill(value);
    }
}
fn offsets(x: i32, y: i32) -> bool {
    x.unsigned_abs() <= SIDE && y.unsigned_abs() <= SIDE
}

unsafe fn reframe_bytes(
    source: &abi::CompositeMask,
    sw: usize,
    sh: usize,
    target: &abi::A8Pixels,
    x: i32,
    y: i32,
    uncovered: u8,
) {
    let output = std::slice::from_raw_parts_mut(target.data, target.len);
    fill(output, target, uncovered);
    let (sx, sy) = (
        (-x).max(0).min(sw as i32) as usize,
        (-y).max(0).min(sh as i32) as usize,
    );
    let (dx, dy) = (
        x.max(0).min(target.width as i32) as usize,
        y.max(0).min(target.height as i32) as usize,
    );
    let (cols, rows) = (
        (sw - sx).min(target.width as usize - dx),
        (sh - sy).min(target.height as usize - dy),
    );
    if cols == 0 || rows == 0 {
        return;
    }
    for row in 0..rows {
        let destination = &mut output[(dy + row) * target.stride as usize + dx
            ..(dy + row) * target.stride as usize + dx + cols];
        if source.data.is_null() {
            destination.fill(source.constant as u8);
        } else {
            let input = std::slice::from_raw_parts(
                source.data.add((sy + row) * source.stride as usize + sx),
                cols,
            );
            destination.copy_from_slice(input);
        }
    }
}

unsafe extern "C" fn reframe(
    request: *const abi::A8Reframe,
    source: *const abi::CompositeMask,
    sw: u32,
    sh: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    let Some(value) = validate(request, source, sw, sh, output) else {
        return abi::ERROR;
    };
    let request = &value.request;
    if request.reserved != 0 || request.fill > 255 || !offsets(request.x, request.y) {
        return abi::ERROR;
    }
    reframe_bytes(
        &value.source,
        sw as usize,
        sh as usize,
        &value.output,
        request.x,
        request.y,
        request.fill as u8,
    );
    abi::OK
}
unsafe extern "C" fn exact_geometry(
    request: *const abi::A8Geometry,
    source: *const abi::CompositeMask,
    sw: u32,
    sh: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    let Some(value) = validate(request, source, sw, sh, output) else {
        return abi::ERROR;
    };
    let request = &value.request;
    let options = &request.geometry;
    if options.kind > 4 || request.fill > 255 || !offsets(options.x, options.y) {
        return abi::ERROR;
    }
    if value.source.data.is_null() {
        let (width, height) = if options.kind <= 1 {
            (sh, sw)
        } else {
            (sw, sh)
        };
        reframe_bytes(
            &value.source,
            width as usize,
            height as usize,
            &value.output,
            options.x,
            options.y,
            request.fill as u8,
        );
    } else {
        geometry::run(
            options,
            std::slice::from_raw_parts(value.source.data, value.source.len),
            sw as usize,
            sh as usize,
            value.source.stride as usize,
            std::slice::from_raw_parts_mut(value.output.data, value.output.len),
            value.output.width as usize,
            value.output.height as usize,
            value.output.stride as usize,
            request.fill as u8,
        );
    }
    abi::OK
}
unsafe extern "C" fn resize(
    request: *const abi::Resample,
    source: *const abi::CompositeMask,
    sw: u32,
    sh: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    let Some(value) = validate(request, source, sw, sh, output) else {
        return abi::ERROR;
    };
    if value.request.method > 2 {
        return abi::ERROR;
    }
    let output = std::slice::from_raw_parts_mut(value.output.data, value.output.len);
    if value.source.data.is_null() {
        fill(output, &value.output, value.source.constant as u8);
    } else {
        resample::a8(
            value.request.method,
            std::slice::from_raw_parts(value.source.data, value.source.len),
            sw as usize,
            sh as usize,
            value.source.stride as usize,
            output,
            value.output.width as usize,
            value.output.height as usize,
            value.output.stride as usize,
        );
    }
    abi::OK
}
static API: abi::A8SamplerApi = abi::A8SamplerApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::A8SamplerApi>() as u32,
    reframe: Some(reframe),
    geometry: Some(exact_geometry),
    resample: Some(resize),
};
#[no_mangle]
pub extern "C" fn micopi_a8_sampler_v1() -> *const abi::A8SamplerApi {
    &API
}

#[cfg(test)]
#[path = "a8_sampler_tests.rs"]
mod tests;
