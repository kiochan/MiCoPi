//! Independent raw byte affine sampler over existing immutable A8 descriptors.
use super::{a8_sampler, affine};
use micopi_plugin_api as abi;

unsafe extern "C" fn sample(
    request: *const abi::A8Affine,
    source: *const abi::CompositeMask,
    sw: u32,
    sh: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    let Some(value) = a8_sampler::validate(request, source, sw, sh, output) else {
        return abi::ERROR;
    };
    if value.request.fill > 255 {
        return abi::ERROR;
    }
    // Share only pure admission/coordinate helpers. The existing RGBA and
    // selected-mask tables and their arithmetic remain unchanged.
    let Some(inverse) = affine::inverse(&abi::Affine {
        kind: 0,
        method: value.request.method,
        matrix: value.request.matrix,
    }) else {
        return abi::ERROR;
    };
    let at = |x: i64, y: i64| -> u64 {
        if x < 0 || y < 0 || x >= i64::from(sw) || y >= i64::from(sh) {
            return u64::from(value.request.fill);
        }
        if value.source.data.is_null() {
            u64::from(value.source.constant)
        } else {
            u64::from(
                *value
                    .source
                    .data
                    .add(y as usize * value.source.stride as usize + x as usize),
            )
        }
    };
    let target = &value.output;
    let bytes = std::slice::from_raw_parts_mut(target.data, target.len);
    for y in 0..target.height as usize {
        for x in 0..target.width as usize {
            let (sx, sy) = affine::coordinate(&inverse, x, y);
            let sample = if value.request.method == 0 {
                at(sx.floor() as i64, sy.floor() as i64)
            } else {
                let (ix, iy, weights) = affine::weights(sx, sy);
                let pixels = [
                    at(ix, iy),
                    at(ix + 1, iy),
                    at(ix, iy + 1),
                    at(ix + 1, iy + 1),
                ];
                (pixels
                    .into_iter()
                    .zip(weights)
                    .map(|(sample, weight)| sample * weight)
                    .sum::<u64>()
                    + affine::DEN / 2)
                    / affine::DEN
            };
            bytes[y * target.stride as usize + x] = sample as u8;
        }
    }
    abi::OK
}

static API: abi::A8AffineApi = abi::A8AffineApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::A8AffineApi>() as u32,
    sample: Some(sample),
};
#[no_mangle]
pub extern "C" fn micopi_a8_affine_v1() -> *const abi::A8AffineApi {
    &API
}

#[cfg(test)]
#[path = "a8_affine_tests.rs"]
mod tests;
