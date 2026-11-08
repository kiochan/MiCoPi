//! Optional cached field ABI and allocation-free immutable-source ROI sampling.
use super::{
    a8_sampler::{overlaps, span},
    affine,
    liquify_field::{Field, Push},
};
use micopi_plugin_api as abi;
use std::ffi::c_void;
const SIDE: u32 = 16384;
fn layout(w: u32, h: u32, stride: u32, len: usize) -> bool {
    (1..=SIDE).contains(&w)
        && (1..=SIDE).contains(&h)
        && stride >= w
        && stride <= SIDE
        && len <= SIDE as usize * SIDE as usize
        && (h as usize - 1) * stride as usize + w as usize <= len
}
fn roi(f: &Field, w: u32, h: u32, stride: u32, len: usize, x: u32, y: u32) -> bool {
    let (fw, fh) = f.dimensions();
    layout(w, h, stride, len)
        && x.checked_add(w).is_some_and(|v| v <= fw)
        && y.checked_add(h).is_some_and(|v| v <= fh)
}
unsafe fn field<'a>(state: *const c_void) -> Option<&'a Field> {
    span(state.cast::<Field>(), 1)?;
    Some(&*state.cast::<Field>())
}
unsafe fn protected(f: &Field, state: *const c_void, target: (usize, usize)) -> bool {
    if overlaps(span(state.cast::<Field>(), 1).unwrap(), target) {
        return true;
    }
    f.storage()
        .into_iter()
        .any(|(p, n)| span(p, n).is_none_or(|s| overlaps(s, target)))
}
unsafe extern "C" fn create(config: *const abi::LiquifyConfig) -> *mut c_void {
    if span(config.cast::<u32>(), 2).is_none()
        || std::ptr::read(config.cast::<u32>()) != 1
        || std::ptr::read(config.cast::<u32>().add(1))
            < std::mem::size_of::<abi::LiquifyConfig>() as u32
        || span(config, 1).is_none()
    {
        return std::ptr::null_mut();
    }
    let c = std::ptr::read(config);
    if c.abi_version != 1
        || c.struct_size < std::mem::size_of::<abi::LiquifyConfig>() as u32
        || c.reserved != 0
    {
        return std::ptr::null_mut();
    }
    let Ok(budget) = usize::try_from(c.budget_bytes) else {
        return std::ptr::null_mut();
    };
    Field::new(c.width, c.height, c.spacing, budget)
        .map_or(std::ptr::null_mut(), |f| Box::into_raw(Box::new(f)).cast())
}
unsafe extern "C" fn destroy(state: *mut c_void) {
    if span(state.cast::<Field>(), 1).is_some() {
        drop(Box::from_raw(state.cast::<Field>()));
    }
}
unsafe extern "C" fn clear(state: *mut c_void) -> i32 {
    if span(state.cast::<Field>(), 1).is_none() {
        return abi::ERROR;
    }
    (&mut *state.cast::<Field>()).clear();
    abi::OK
}
unsafe extern "C" fn push(
    state: *mut c_void,
    input: *const abi::LiquifyPush,
    count: usize,
    output: *mut abi::LiquifyUpdate,
) -> i32 {
    if !(1..=4).contains(&count) {
        return abi::ERROR;
    }
    let Some(f) = field(state) else {
        return abi::ERROR;
    };
    let (Some(source), Some(target)) = (span(input, count), span(output, 1)) else {
        return abi::ERROR;
    };
    if overlaps(source, target) || protected(f, state, source) || protected(f, state, target) {
        return abi::ERROR;
    }
    let mut pushes = [Push {
        x: 0.,
        y: 0.,
        dx: 0.,
        dy: 0.,
        radius: 0.,
        strength: 0.,
    }; 4];
    for (i, p) in pushes.iter_mut().enumerate().take(count) {
        let v = std::ptr::read(input.add(i));
        *p = Push {
            x: v.x,
            y: v.y,
            dx: v.dx,
            dy: v.dy,
            radius: v.radius,
            strength: v.strength,
        };
    }
    let Ok(regions) = (&mut *state.cast::<Field>()).push(&pushes[..count]) else {
        return abi::ERROR;
    };
    let mut result = abi::LiquifyUpdate::default();
    result.count = regions.len as u32;
    for (out, b) in result
        .regions
        .iter_mut()
        .zip(regions.items)
        .take(regions.len)
    {
        *out = abi::Region {
            x: b[0],
            y: b[1],
            width: b[2] - b[0],
            height: b[3] - b[1],
        };
    }
    std::ptr::write(output, result);
    abi::OK
}
unsafe extern "C" fn pixels(
    state: *const c_void,
    method: u32,
    source: *const abi::Pixels,
    output: *mut abi::Pixels,
) -> i32 {
    let Some(f) = field(state) else {
        return abi::ERROR;
    };
    let (Some(sm), Some(dm)) = (span(source, 1), span(output, 1)) else {
        return abi::ERROR;
    };
    let s = std::ptr::read(source);
    let d = std::ptr::read(output);
    let (fw, fh) = f.dimensions();
    if method > 1
        || !layout(s.width, s.height, s.stride, s.len)
        || (s.width, s.height) != (fw, fh)
        || (s.origin_x, s.origin_y) != (0, 0)
        || !roi(
            f, d.width, d.height, d.stride, d.len, d.origin_x, d.origin_y,
        )
    {
        return abi::ERROR;
    }
    let (Some(src), Some(dst)) = (span(s.data, s.len), span(d.data, d.len)) else {
        return abi::ERROR;
    };
    if overlaps(src, dst) || overlaps(sm, dst) || overlaps(dm, dst) || protected(f, state, dst) {
        return abi::ERROR;
    }
    let src = affine::Source {
        pixels: std::slice::from_raw_parts(s.data, s.len),
        mask: None,
        width: fw as usize,
        height: fh as usize,
        stride: s.stride as usize,
    };
    let dest = std::slice::from_raw_parts_mut(d.data, d.len);
    for y in 0..d.height as usize {
        for x in 0..d.width as usize {
            let gx = x + d.origin_x as usize;
            let gy = y + d.origin_y as usize;
            let (qx, qy) = (gx as f64 + 0.5, gy as f64 + 0.5);
            let delta = f.displacement(qx, qy);
            dest[y * d.stride as usize + x] = if delta == [0.; 2] {
                src.pixels[gy * src.stride + gx]
            } else {
                src.sample(method, qx + delta[0], qy + delta[1])
            };
        }
    }
    abi::OK
}
unsafe extern "C" fn mask(
    state: *const c_void,
    method: u32,
    source: *const abi::CompositeMask,
    output: *mut abi::A8Pixels,
) -> i32 {
    let Some(f) = field(state) else {
        return abi::ERROR;
    };
    let (Some(sm), Some(dm)) = (span(source, 1), span(output, 1)) else {
        return abi::ERROR;
    };
    let s = std::ptr::read(source);
    let d = std::ptr::read(output);
    let (fw, fh) = f.dimensions();
    if method > 1
        || !roi(
            f, d.width, d.height, d.stride, d.len, d.origin_x, d.origin_y,
        )
    {
        return abi::ERROR;
    }
    let Some(dst) = span(d.data, d.len) else {
        return abi::ERROR;
    };
    if overlaps(sm, dst) || overlaps(dm, dst) || protected(f, state, dst) {
        return abi::ERROR;
    }
    if s.data.is_null() {
        if s.len != 0 || s.stride != 0 || s.constant > 255 {
            return abi::ERROR;
        }
    } else {
        if s.constant != 0 || !layout(fw, fh, s.stride, s.len) {
            return abi::ERROR;
        }
        let Some(src) = span(s.data, s.len) else {
            return abi::ERROR;
        };
        if overlaps(src, dst) {
            return abi::ERROR;
        }
    }
    let at = |x: i64, y: i64| -> u64 {
        if x < 0 || y < 0 || x >= i64::from(fw) || y >= i64::from(fh) {
            255
        } else if s.data.is_null() {
            u64::from(s.constant)
        } else {
            u64::from(*s.data.add(y as usize * s.stride as usize + x as usize))
        }
    };
    let bytes = std::slice::from_raw_parts_mut(d.data, d.len);
    for y in 0..d.height as usize {
        for x in 0..d.width as usize {
            let (gx, gy) = (x + d.origin_x as usize, y + d.origin_y as usize);
            let (qx, qy) = (gx as f64 + 0.5, gy as f64 + 0.5);
            let delta = f.displacement(qx, qy);
            let (sx, sy) = (qx + delta[0], qy + delta[1]);
            let value = if delta == [0.; 2] || method == 0 {
                at(sx.floor() as i64, sy.floor() as i64)
            } else {
                let (ix, iy, w) = affine::weights(sx, sy);
                let values = [
                    at(ix, iy),
                    at(ix + 1, iy),
                    at(ix, iy + 1),
                    at(ix + 1, iy + 1),
                ];
                (values.into_iter().zip(w).map(|(a, b)| a * b).sum::<u64>() + affine::DEN / 2)
                    / affine::DEN
            };
            bytes[y * d.stride as usize + x] = value as u8;
        }
    }
    abi::OK
}
static API: abi::LiquifyApi = abi::LiquifyApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::LiquifyApi>() as u32,
    create: Some(create),
    destroy: Some(destroy),
    clear: Some(clear),
    push: Some(push),
    pixels: Some(pixels),
    mask: Some(mask),
};
#[no_mangle]
pub extern "C" fn micopi_liquify_v1() -> *const abi::LiquifyApi {
    &API
}

#[cfg(test)]
#[path = "liquify_tests.rs"]
mod tests;
