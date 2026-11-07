//! Typed A8 coverage composition, separate from the unchanged legacy renderer.
use super::{filter, mix, opacity, over, overlaps, span};
use micopi_plugin_api as abi;

fn aligned<T>(pointer: *const T) -> bool {
    (pointer as usize) % std::mem::align_of::<T>() == 0
}

/// Resolve opacity and spatial coverage once, using positive half-up rounding.
fn weight(opacity: f32, coverage: u32) -> u32 {
    ((opacity * 255.).round() as u32 * coverage + 127) / 255
}

fn apply(pixel: u32, amount: u32) -> u32 {
    let alpha = (pixel >> 24) * amount / 255;
    if alpha == 0 {
        0
    } else {
        (pixel & 0x00ffffff) | (alpha << 24)
    }
}

pub(super) unsafe extern "C" fn render_masked(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
) -> i32 {
    render_plan(
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        width,
        height,
        output,
        &[],
        false,
    )
}

pub(super) unsafe fn render_plan(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
    luts: &[abi::CompositeLut],
    curves: bool,
) -> i32 {
    render_color_plan(
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        width,
        height,
        output,
        luts,
        curves,
        &[],
    )
}

pub(super) unsafe fn render_color_plan(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
    luts: &[abi::CompositeLut],
    curves: bool,
    hsl: &[crate::hsl_math::Parameters],
) -> i32 {
    if hsl.len() > 64
        || output.is_null()
        || !aligned(output)
        || layers.is_null()
        || !aligned(layers)
        || !(1..=16).contains(&layer_count)
        || count > 64
        || (count != 0 && (nodes.is_null() || !aligned(nodes)))
        || mask_count > 32
        || (mask_count != 0 && (masks.is_null() || !aligned(masks)))
    {
        return abi::ERROR;
    }
    let out = &*output;
    let (w, h, stride) = (out.width as usize, out.height as usize, out.stride as usize);
    if w == 0
        || h == 0
        || !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
        || out.data.is_null()
        || !aligned(out.data)
        || stride < w
        || out
            .origin_x
            .checked_add(out.width)
            .is_none_or(|v| v > width)
        || out
            .origin_y
            .checked_add(out.height)
            .is_none_or(|v| v > height)
    {
        return abi::ERROR;
    }
    let Some(used) = (h - 1).checked_mul(stride).and_then(|n| n.checked_add(w)) else {
        return abi::ERROR;
    };
    if used > out.len {
        return abi::ERROR;
    }
    let Some(destination) = span(out.data, used) else {
        return abi::ERROR;
    };
    // Validate the entire call before constructing writable slices. Row padding
    // belongs to the borrowed output span, even though it is never written.
    if span(output, 1).is_none_or(|p| overlaps(destination, p))
        || span(layers, layer_count).is_none_or(|p| overlaps(destination, p))
        || (count != 0 && span(nodes, count).is_none_or(|p| overlaps(destination, p)))
        || (mask_count != 0 && span(masks, mask_count).is_none_or(|p| overlaps(destination, p)))
    {
        return abi::ERROR;
    }
    let layers = std::slice::from_raw_parts(layers, layer_count);
    let area = width as usize * height as usize;
    if layers.iter().any(|l| {
        l.data.is_null()
            || !aligned(l.data)
            || l.len < area
            || l.opacity > 255
            || l.visible > 1
            || l.mode > abi::BLEND_MODE_MAX
            || span(l.data, area).is_none_or(|p| overlaps(destination, p))
    }) {
        return abi::ERROR;
    }
    let masks = if mask_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(masks, mask_count)
    };
    if masks.iter().any(|m| {
        if m.data.is_null() {
            m.len != 0 || m.stride != 0 || m.constant > 255
        } else {
            let used = (height as usize - 1)
                .checked_mul(m.stride as usize)
                .and_then(|n| n.checked_add(width as usize));
            m.constant != 0
                || m.stride < width
                || used.is_none_or(|n| n > m.len)
                || used.is_none_or(|n| span(m.data, n).is_none_or(|p| overlaps(destination, p)))
        }
    }) {
        return abi::ERROR;
    }
    let nodes = if count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(nodes, count)
    };
    // Coverage and color occupy separate typed slots in one fixed-size array.
    // Check every instruction, including values not used by the final output.
    let mut coverage = [false; 64];
    for (i, n) in nodes.iter().enumerate() {
        let image = |index: u32| (index as usize) < i && !coverage[index as usize];
        let mask = |index: u32| (index as usize) < i && coverage[index as usize];
        let valid_opacity = || (0. ..=1.).contains(&n.amount);
        let valid = n.amount.is_finite()
            && match n.kind {
                0 => (n.a as usize) < layer_count,
                1 => {
                    image(n.a)
                        && n.mode <= 4
                        && n.amount >= if n.mode <= 1 { 0. } else { -1. }
                        && n.amount <= 1.
                }
                2 => image(n.a) && image(n.b) && n.mode <= abi::BLEND_MODE_MAX && valid_opacity(),
                3 => image(n.a),
                4 => n.a == 0 && n.b == 0 && n.mode == 0 && n.amount == 0.,
                5 => image(n.a) && image(n.b) && n.mode == 0 && valid_opacity(),
                6 => (n.a as usize) < mask_count && n.b == 0 && n.mode == 0 && n.amount == 0.,
                7 => image(n.a) && mask(n.b) && n.mode == 0 && valid_opacity(),
                8 => image(n.a) && image(n.b) && mask(n.mode) && valid_opacity(),
                9 => image(n.a) && n.b == 0 && (n.mode as usize) < luts.len() && n.amount == 1.,
                10 => {
                    curves
                        && image(n.a)
                        && n.b == 0
                        && n.amount == 1.
                        && (n.mode as usize)
                            .checked_add(3)
                            .is_some_and(|end| end <= luts.len())
                }
                11 => image(n.a) && n.b == 0 && n.amount == 1. && (n.mode as usize) < hsl.len(),
                _ => false,
            };
        if !valid {
            return abi::ERROR;
        }
        coverage[i] = n.kind == 6;
    }
    if count != 0 && coverage[count - 1] {
        return abi::ERROR;
    }
    if !nodes.iter().any(|n| n.kind >= 6) {
        // The legacy body and generated hot loop remain unchanged. Unused A8
        // descriptors were still validated before this exact delegation.
        return super::render(
            nodes.as_ptr(),
            nodes.len(),
            layers.as_ptr(),
            layers.len(),
            width,
            height,
            output,
        );
    }
    let pixels = std::slice::from_raw_parts_mut(out.data, used);
    let mut values = [0u32; 64];
    for y in 0..h {
        let global_y = out.origin_y as usize + y;
        for x in 0..w {
            let global_x = out.origin_x as usize + x;
            pixels[y * stride + x] = evaluate_color(
                nodes,
                layers,
                masks,
                luts,
                hsl,
                width,
                global_x,
                global_y,
                &mut values,
            );
        }
    }
    abi::OK
}

/// Internal evaluator for already-validated image plans. Coverage remains a
/// separate typed slot; callers must reject empty plans and coverage outputs.
#[inline(always)]
pub(super) unsafe fn evaluate_color(
    nodes: &[abi::CompositeNode],
    layers: &[abi::CompositeLayer],
    masks: &[abi::CompositeMask],
    luts: &[abi::CompositeLut],
    hsl: &[crate::hsl_math::Parameters],
    width: u32,
    global_x: usize,
    global_y: usize,
    values: &mut [u32; 64],
) -> u32 {
    let index = global_y * width as usize + global_x;
    for (i, n) in nodes.iter().enumerate() {
        values[i] = match n.kind {
            0 => {
                let layer = &layers[n.a as usize];
                if layer.visible == 0 {
                    0
                } else {
                    let pixel = opacity(*layer.data.add(index), layer.opacity);
                    if pixel >> 24 == 0 {
                        0
                    } else {
                        pixel
                    }
                }
            }
            1 => filter(values[n.a as usize], n.mode, n.amount),
            2 => over(
                values[n.a as usize],
                opacity(values[n.b as usize], (n.amount * 255.).round() as u32),
                n.mode,
            ),
            3 => values[n.a as usize],
            4 => 0,
            5 => mix(
                values[n.a as usize],
                values[n.b as usize],
                (n.amount * 255.).round() as u32,
            ),
            6 => {
                let mask = &masks[n.a as usize];
                if mask.data.is_null() {
                    mask.constant
                } else {
                    u32::from(*mask.data.add(global_y * mask.stride as usize + global_x))
                }
            }
            7 => apply(values[n.a as usize], weight(n.amount, values[n.b as usize])),
            8 => mix(
                values[n.a as usize],
                values[n.b as usize],
                weight(n.amount, values[n.mode as usize]),
            ),
            9 => super::levels::apply_lut(&luts[n.mode as usize].values, values[n.a as usize]),
            10 => {
                super::curves::apply_channels(&luts[n.mode as usize..][..3], values[n.a as usize])
            }
            11 => hsl[n.mode as usize].apply(values[n.a as usize]),
            _ => unreachable!(),
        };
    }
    values[nodes.len() - 1]
}
