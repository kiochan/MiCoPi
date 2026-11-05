//! Typed selection clipping for cold pixel actions. The host retains history.
use micopi_plugin_api as abi;
mod color;
mod edges;
mod transfer;
pub fn valid(mask: &abi::Coverage) -> bool {
    if mask.data.is_null() {
        return mask.len == 0 && mask.width == 0 && mask.height == 0 && mask.stride == 0;
    }
    mask.width > 0
        && mask.height > 0
        && mask.width <= 4096
        && mask.height <= 4096
        && mask.stride >= mask.width
        && mask.stride <= 4096
        && (mask.height as usize - 1)
            .checked_mul(mask.stride as usize)
            .and_then(|v| v.checked_add(mask.width as usize))
            .is_some_and(|v| v <= mask.len)
        && mask.len <= 4096 * 4096
}
fn valid_pixels(p: &abi::Pixels, mask: &abi::Coverage) -> bool {
    !p.data.is_null()
        && p.width > 0
        && p.height > 0
        && p.stride >= p.width
        && p.origin_x
            .checked_add(p.width)
            .is_some_and(|v| v <= mask.width)
        && p.origin_y
            .checked_add(p.height)
            .is_some_and(|v| v <= mask.height)
        && (p.height as usize - 1)
            .checked_mul(p.stride as usize)
            .and_then(|v| v.checked_add(p.width as usize))
            .is_some_and(|v| v <= p.len)
}
unsafe extern "C" fn mix(
    mask: *const abi::Coverage,
    before: *const abi::Pixels,
    after: *mut abi::Pixels,
) -> i32 {
    if mask.is_null() || before.is_null() || after.is_null() {
        return abi::ERROR;
    }
    let (mask, before, after) = (&*mask, &*before, &mut *after);
    if !valid(mask)
        || mask.data.is_null()
        || !valid_pixels(before, mask)
        || !valid_pixels(after, mask)
        || (
            before.width,
            before.height,
            before.origin_x,
            before.origin_y,
        ) != (after.width, after.height, after.origin_x, after.origin_y)
    {
        return abi::ERROR;
    }
    for y in 0..after.height as usize {
        for x in 0..after.width as usize {
            let m = *mask.data.add(
                (after.origin_y as usize + y) * mask.stride as usize + after.origin_x as usize + x,
            ) as u32;
            let dst = &mut *after.data.add(y * after.stride as usize + x);
            let src = *before.data.add(y * before.stride as usize + x);
            *dst = interpolate(src, *dst, m);
        }
    }
    abi::OK
}
fn interpolate(before: u32, after: u32, m: u32) -> u32 {
    if m == 0 {
        return before;
    }
    if m == 255 {
        return after;
    }
    let a = (before >> 24) * (255 - m) + (after >> 24) * m;
    if a == 0 {
        return before;
    }
    let mut output = ((a + 127) / 255) << 24;
    for s in [0, 8, 16] {
        let c = ((before >> s) & 255) * (before >> 24) * (255 - m)
            + ((after >> s) & 255) * (after >> 24) * m;
        output |= ((c + a / 2) / a).min(255) << s;
    }
    output
}
static API: abi::SelectionApiTransfer = abi::SelectionApiTransfer {
    base: abi::SelectionApiEdges {
        base: abi::SelectionApiColor {
            base: abi::SelectionApi {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::SelectionApiTransfer>() as u32,
                mix: Some(mix),
                rasterize: Some(rasterize),
            },
            color: Some(color::select),
        },
        edges: Some(edges::process),
    },
    transfer: Some(transfer::process),
};
unsafe extern "C" fn rasterize(
    shape: *const abi::SelectionShape,
    previous: *const abi::Coverage,
    output: *mut u8,
    len: usize,
    width: u32,
    height: u32,
) -> i32 {
    if shape.is_null()
        || previous.is_null()
        || output.is_null()
        || width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || len != width as usize * height as usize
    {
        return abi::ERROR;
    }
    let (shape, previous) = (&*shape, &*previous);
    if shape.kind > 4
        || shape.mode > 3
        || !valid(previous)
        || (!previous.data.is_null() && (previous.width, previous.height) != (width, height))
        || shape
            .bounds
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 65536.)
        || (shape.kind <= 1 && (shape.bounds[2] <= 0. || shape.bounds[3] <= 0.))
        || (shape.kind == 2 && (shape.count < 3 || shape.count > 4096 || shape.points.is_null()))
    {
        return abi::ERROR;
    }
    let points = if shape.kind == 2 {
        std::slice::from_raw_parts(shape.points, shape.count)
    } else {
        &[]
    };
    if points
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 65536.)
    {
        return abi::ERROR;
    }
    let out = std::slice::from_raw_parts_mut(output, len);
    let [left, top, w, h] = shape.bounds;
    let mut crossings = Vec::with_capacity(points.len());
    for y in 0..height as usize {
        let py = y as f32 + 0.5;
        crossings.clear();
        if !points.is_empty() {
            let mut a = points[points.len() - 1];
            for &b in points {
                if (a[1] > py) != (b[1] > py) {
                    crossings.push((b[0] - a[0]) * (py - a[1]) / (b[1] - a[1]) + a[0]);
                }
                a = b;
            }
            crossings.sort_by(f32::total_cmp);
        }
        let mut crossing = 0;
        for x in 0..width as usize {
            let old = if previous.data.is_null() {
                0
            } else {
                *previous.data.add(y * previous.stride as usize + x)
            };
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let selected = match shape.kind {
                0 => px >= left && py >= top && px < left + w && py < top + h,
                1 => {
                    let dx = (px - left - w * 0.5) / (w * 0.5);
                    let dy = (py - top - h * 0.5) / (h * 0.5);
                    dx * dx + dy * dy <= 1.
                }
                2 => {
                    while crossing < crossings.len() && px >= crossings[crossing] {
                        crossing += 1;
                    }
                    crossing % 2 != 0
                }
                3 => true,
                _ => false,
            };
            let value = if shape.kind == 4 {
                255 - old
            } else if selected {
                255
            } else {
                0
            };
            out[y * width as usize + x] = match shape.mode {
                0 => value,
                1 => old.max(value),
                2 => old.saturating_sub(value),
                _ => old.min(value),
            };
        }
    }
    abi::OK
}
#[no_mangle]
pub extern "C" fn micopi_selection_v1() -> *const abi::SelectionApi {
    &API.base.base.base
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn premultiplied_mixing_has_exact_endpoints_and_retains_hidden_storage() {
        assert_eq!(interpolate(0x00123456, 0xffabcdef, 0), 0x00123456);
        assert_eq!(interpolate(0x80112233, 0, 255), 0);
        assert_eq!(interpolate(0x80112233, 0, 128), 0x40112233);
        assert_eq!(interpolate(0x00123456, 0x00abcdef, 128), 0x00123456);
        assert_eq!(interpolate(0xffff0000, 0xff0000ff, 128), 0xff7f0080);
    }
    #[test]
    fn polygon_scanlines_match_independent_ray_tests_for_concave_boundaries() {
        let points = [[1., 1.], [9., 1.], [9., 4.], [4., 4.], [4., 9.], [1., 9.]];
        let shape = abi::SelectionShape {
            kind: 2,
            mode: 0,
            bounds: [0.; 4],
            points: points.as_ptr(),
            count: points.len(),
        };
        let mut out = vec![0; 100];
        assert_eq!(
            unsafe {
                rasterize(
                    &shape,
                    &abi::Coverage::default(),
                    out.as_mut_ptr(),
                    out.len(),
                    10,
                    10,
                )
            },
            abi::OK
        );
        for y in 0..10 {
            for x in 0..10 {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let count = points
                    .iter()
                    .zip(points.iter().cycle().skip(1))
                    .filter(|(a, b)| {
                        let (ax, ay, bx, by) = (a[0] as f64, a[1] as f64, b[0] as f64, b[1] as f64);
                        (ay > py) != (by > py) && px < (bx - ax) * (py - ay) / (by - ay) + ax
                    })
                    .count();
                assert_eq!(out[y * 10 + x], if count % 2 != 0 { 255 } else { 0 });
            }
        }
        let before = out.clone();
        let mut invalid = shape;
        invalid.mode = 4;
        assert_eq!(
            unsafe {
                rasterize(
                    &invalid,
                    &abi::Coverage::default(),
                    out.as_mut_ptr(),
                    out.len(),
                    10,
                    10,
                )
            },
            abi::ERROR
        );
        assert_eq!(out, before);
    }
}
