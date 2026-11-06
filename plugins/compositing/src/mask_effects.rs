//! Deterministic effective A8 regions. Persistent scratch is separate from raw masks.
use super::{overlaps, span};
use micopi_plugin_api as abi;
use std::ffi::c_void;

#[derive(Default)]
pub(super) struct State {
    first: Vec<u8>,
    second: Vec<u8>,
    columns: Vec<u32>,
}

pub(super) unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(State::default())).cast()
}
pub(super) unsafe extern "C" fn destroy(instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance.cast::<State>()));
    }
}

pub(super) unsafe extern "C" fn reserve(
    instance: *mut c_void,
    width: u32,
    height: u32,
    radius: u32,
) -> i32 {
    let instance = instance.cast::<State>();
    if instance.is_null()
        || !aligned(instance)
        || !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
        || radius > abi::MASK_EFFECT_MAX_RADIUS
    {
        return abi::ERROR;
    }
    // Reserve without resize: preparation must not zero or feather a full frame.
    let area = width as usize * height as usize;
    let state = &mut *instance;
    if state
        .first
        .try_reserve_exact(area.saturating_sub(state.first.len()))
        .is_err()
        || state
            .second
            .try_reserve_exact(area.saturating_sub(state.second.len()))
            .is_err()
        || state
            .columns
            .try_reserve_exact((width as usize).saturating_sub(state.columns.len()))
            .is_err()
    {
        return abi::ERROR;
    }
    abi::OK
}

fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize) % std::mem::align_of::<T>() == 0
}
fn density(value: u8, amount: u32) -> u8 {
    (255 - ((255 - value as u32) * amount + 127) / 255) as u8
}
fn horizontal(source: &[u8], output: &mut [u8], width: usize, height: usize, radius: usize) {
    let divisor = (radius * 2 + 1) as u32;
    for y in 0..height {
        let row = y * width;
        let mut sum = u32::from(source[row]) * (radius as u32 + 1);
        for x in 1..=radius {
            sum += u32::from(source[row + x.min(width - 1)]);
        }
        for x in 0..width {
            output[row + x] = ((sum + radius as u32) / divisor) as u8;
            sum = sum - u32::from(source[row + x.saturating_sub(radius)])
                + u32::from(source[row + (x + radius + 1).min(width - 1)]);
        }
    }
}
fn vertical(
    source: &[u8],
    output: &mut [u8],
    width: usize,
    height: usize,
    radius: usize,
    columns: &mut [u32],
) {
    let divisor = (radius * 2 + 1) as u32;
    for x in 0..width {
        columns[x] = u32::from(source[x]) * (radius as u32 + 1);
    }
    for y in 1..=radius {
        let row = y.min(height - 1) * width;
        for x in 0..width {
            columns[x] += u32::from(source[row + x]);
        }
    }
    for y in 0..height {
        let row = y * width;
        let leaving = y.saturating_sub(radius) * width;
        let entering = (y + radius + 1).min(height - 1) * width;
        for x in 0..width {
            output[row + x] = ((columns[x] + radius as u32) / divisor) as u8;
            columns[x] =
                columns[x] - u32::from(source[leaving + x]) + u32::from(source[entering + x]);
        }
    }
}

pub(super) unsafe extern "C" fn region(
    instance: *mut c_void,
    request_pointer: *const abi::MaskEffectRequest,
    source_pointer: *const abi::CompositeMask,
    width: u32,
    height: u32,
    output_pointer: *mut abi::A8Pixels,
) -> i32 {
    let instance = instance.cast::<State>();
    if instance.is_null()
        || !aligned(instance)
        || request_pointer.is_null()
        || !aligned(request_pointer)
        || source_pointer.is_null()
        || !aligned(source_pointer)
        || output_pointer.is_null()
        || !aligned(output_pointer)
        || !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
    {
        return abi::ERROR;
    }
    let request = *request_pointer;
    let source = *source_pointer;
    let output = &*output_pointer;
    let invert = request.flags == abi::MASK_EFFECT_INVERT_RAW;
    if request.reserved != 0
        || request.density > 255
        || request.feather_radius > abi::MASK_EFFECT_MAX_RADIUS
        || request.flags != 0 && !invert
        || invert && (request.density != 255 || request.feather_radius != 0)
        || output.width == 0
        || output.height == 0
        || output.data.is_null()
        || output.stride < output.width
        || output
            .origin_x
            .checked_add(output.width)
            .is_none_or(|right| right > width)
        || output
            .origin_y
            .checked_add(output.height)
            .is_none_or(|bottom| bottom > height)
    {
        return abi::ERROR;
    }
    let Some(used) = (output.height as usize - 1)
        .checked_mul(output.stride as usize)
        .and_then(|v| v.checked_add(output.width as usize))
    else {
        return abi::ERROR;
    };
    let Some(destination) = span(output.data, used) else {
        return abi::ERROR;
    };
    if used > output.len {
        return abi::ERROR;
    }
    let source_span = if source.data.is_null() {
        if source.len != 0 || source.stride != 0 || source.constant > 255 {
            return abi::ERROR;
        }
        None
    } else {
        let Some(needed) = (height as usize - 1)
            .checked_mul(source.stride as usize)
            .and_then(|v| v.checked_add(width as usize))
        else {
            return abi::ERROR;
        };
        if source.constant != 0 || source.stride < width || source.len < needed {
            return abi::ERROR;
        }
        let Some(input) = span(source.data, needed) else {
            return abi::ERROR;
        };
        if overlaps(destination, input) {
            return abi::ERROR;
        }
        Some(input)
    };
    let metadata = [
        span(instance, 1),
        span(request_pointer, 1),
        span(source_pointer, 1),
        span(output_pointer, 1),
    ];
    if metadata
        .into_iter()
        .any(|value| value.is_none_or(|value| overlaps(destination, value)))
    {
        return abi::ERROR;
    }
    let instance_span = metadata[0].unwrap();
    if source_span.is_some_and(|input| overlaps(input, instance_span))
        || metadata[1..]
            .iter()
            .any(|value| overlaps(value.unwrap(), instance_span))
    {
        return abi::ERROR;
    }
    // Reject caller aliases into persistent scratch before any resize or write.
    let state = &*instance;
    for (capacity, scratch) in [
        (
            state.first.capacity(),
            span(state.first.as_ptr(), state.first.capacity()),
        ),
        (
            state.second.capacity(),
            span(state.second.as_ptr(), state.second.capacity()),
        ),
        (
            state.columns.capacity(),
            span(state.columns.as_ptr(), state.columns.capacity()),
        ),
    ] {
        if capacity != 0 {
            let Some(scratch) = scratch else {
                return abi::ERROR;
            };
            if overlaps(destination, scratch)
                || source_span.is_some_and(|input| overlaps(input, scratch))
                || metadata[1..]
                    .iter()
                    .any(|value| overlaps(value.unwrap(), scratch))
            {
                return abi::ERROR;
            }
        }
    }
    let (w, h, stride) = (
        output.width as usize,
        output.height as usize,
        output.stride as usize,
    );
    // Validated uniform and zero-density requests need no halo or scratch.
    if source.data.is_null() || !invert && request.density == 0 {
        let value = if invert {
            255 - source.constant as u8
        } else {
            density(source.constant as u8, request.density)
        };
        let target = std::slice::from_raw_parts_mut(output.data, used);
        for y in 0..h {
            target[y * stride..y * stride + w].fill(value);
        }
        return abi::OK;
    }
    if request.feather_radius == 0 {
        let target = std::slice::from_raw_parts_mut(output.data, used);
        for y in 0..h {
            let input = source.data.add(
                (output.origin_y as usize + y) * source.stride as usize + output.origin_x as usize,
            );
            for x in 0..w {
                let value = *input.add(x);
                target[y * stride + x] = if invert {
                    255 - value
                } else {
                    density(value, request.density)
                };
            }
        }
        return abi::OK;
    }
    let radius = request.feather_radius as usize;
    let left = (output.origin_x as usize).saturating_sub(radius);
    let top = (output.origin_y as usize).saturating_sub(radius);
    let right = (output.origin_x as usize + w + radius).min(width as usize);
    let bottom = (output.origin_y as usize + h + radius).min(height as usize);
    let (halo_width, halo_height) = (right - left, bottom - top);
    let Some(area) = halo_width.checked_mul(halo_height) else {
        return abi::ERROR;
    };
    if area > 4096 * 4096 {
        return abi::ERROR;
    }
    let state = &mut *instance;
    if state
        .first
        .try_reserve_exact(area.saturating_sub(state.first.len()))
        .is_err()
        || state
            .second
            .try_reserve_exact(area.saturating_sub(state.second.len()))
            .is_err()
        || state
            .columns
            .try_reserve_exact(halo_width.saturating_sub(state.columns.len()))
            .is_err()
    {
        return abi::ERROR;
    }
    state.first.resize(area, 0);
    state.second.resize(area, 0);
    state.columns.resize(halo_width, 0);
    for y in 0..halo_height {
        let row = std::slice::from_raw_parts(
            source.data.add((top + y) * source.stride as usize + left),
            halo_width,
        );
        state.first[y * halo_width..(y + 1) * halo_width].copy_from_slice(row);
    }
    for pass_radius in [(radius + 2) / 3, (radius + 1) / 3, radius / 3] {
        if pass_radius == 0 {
            continue;
        }
        horizontal(
            &state.first,
            &mut state.second,
            halo_width,
            halo_height,
            pass_radius,
        );
        vertical(
            &state.second,
            &mut state.first,
            halo_width,
            halo_height,
            pass_radius,
            &mut state.columns,
        );
    }
    let target = std::slice::from_raw_parts_mut(output.data, used);
    let (local_x, local_y) = (
        output.origin_x as usize - left,
        output.origin_y as usize - top,
    );
    for y in 0..h {
        for x in 0..w {
            target[y * stride + x] = density(
                state.first[(local_y + y) * halo_width + local_x + x],
                request.density,
            );
        }
    }
    abi::OK
}

#[cfg(test)]
#[path = "mask_effects_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mask_effects_benchmark.rs"]
mod benchmark;

#[cfg(test)]
#[path = "mask_effects_reserve_tests.rs"]
mod reserve_tests;

#[cfg(all(test, windows))]
#[path = "mask_effects_dll_benchmark.rs"]
mod dll_benchmark;
