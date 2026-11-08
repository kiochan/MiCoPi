//! Bounded observations of prepared plans, without intermediate image buffers.
use super::{abi, grid};
use crate::{overlaps, span};

pub(super) unsafe extern "C" fn sample_plan(
    request: *const abi::HistogramRequest,
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    output: *mut abi::HistogramResult,
) -> i32 {
    sample_color_plan(
        request,
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        luts,
        lut_count,
        output,
        &[],
    )
}
pub(crate) unsafe fn sample_color_plan(
    request: *const abi::HistogramRequest,
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    output: *mut abi::HistogramResult,
    hsl: &[crate::hsl_math::Parameters],
) -> i32 {
    sample_quantized_plan(
        request,
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        luts,
        lut_count,
        output,
        hsl,
        &[],
    )
}

pub(crate) unsafe fn sample_quantized_plan(
    request: *const abi::HistogramRequest,
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    output: *mut abi::HistogramResult,
    hsl: &[crate::hsl_math::Parameters],
    quantization: &[crate::quantize_math::Quantize],
) -> i32 {
    if request.is_null()
        || output.is_null()
        || nodes.is_null()
        || layers.is_null()
        || !(1..=64).contains(&count)
        || !(1..=16).contains(&layer_count)
        || mask_count > 32
        || mask_count != 0 && masks.is_null()
        || lut_count > 64
        || lut_count != 0 && luts.is_null()
    {
        return abi::ERROR;
    }
    let header = std::ptr::read_unaligned(request.cast::<abi::Header>());
    let destination_header = std::ptr::read_unaligned(output.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::HistogramRequest>() as u32
        || destination_header.abi_version != 1
        || destination_header.struct_size < std::mem::size_of::<abi::HistogramResult>() as u32
    {
        return abi::ERROR;
    }
    let p = std::ptr::read_unaligned(request);
    if !(1..=4096).contains(&p.width)
        || !(1..=4096).contains(&p.height)
        || p.stride != p.width
        || !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&p.max_samples)
    {
        return abi::ERROR;
    }
    let Some(destination) = span(output, 1) else {
        return abi::ERROR;
    };
    if span(request, 1).is_none_or(|s| overlaps(s, destination))
        || span(nodes, count).is_none_or(|s| overlaps(s, destination))
        || span(layers, layer_count).is_none_or(|s| overlaps(s, destination))
        || mask_count != 0 && span(masks, mask_count).is_none_or(|s| overlaps(s, destination))
        || lut_count != 0 && span(luts, lut_count).is_none_or(|s| overlaps(s, destination))
    {
        return abi::ERROR;
    }
    let luts = if lut_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(luts, lut_count)
    };
    // Reuse complete renderer validation, including disconnected instructions,
    // typed coverage and every borrowed plane. Only a private single pixel is
    // evaluated here; no partial histogram can escape on failure.
    let mut pixel = 0;
    let mut target = abi::Pixels {
        data: &mut pixel,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    if crate::masks::render_quantized_plan(
        nodes,
        count,
        layers,
        layer_count,
        masks,
        mask_count,
        p.width,
        p.height,
        &mut target,
        luts,
        true,
        hsl,
        quantization,
    ) != abi::OK
    {
        return abi::ERROR;
    }
    let nodes = std::slice::from_raw_parts(nodes, count);
    let layers = std::slice::from_raw_parts(layers, layer_count);
    let masks = if mask_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(masks, mask_count)
    };
    if layers
        .iter()
        .any(|l| span(l.data, l.len).is_none_or(|s| overlaps(s, destination)))
        || masks.iter().any(|m| {
            !m.data.is_null() && span(m.data, m.len).is_none_or(|s| overlaps(s, destination))
        })
    {
        return abi::ERROR;
    }
    let (gw, gh) = grid(&p);
    let mut result = abi::HistogramResult {
        grid_width: gw,
        grid_height: gh,
        sampled_pixels: u64::from(gw) * u64::from(gh),
        ..Default::default()
    };
    let mut values = [0; 64];
    for row in 0..gh {
        let y = (2 * row as usize + 1) * p.height as usize / (2 * gh as usize);
        for col in 0..gw {
            let x = (2 * col as usize + 1) * p.width as usize / (2 * gw as usize);
            let pixel = crate::masks::evaluate_quantized(
                nodes,
                layers,
                masks,
                luts,
                hsl,
                quantization,
                p.width,
                x,
                y,
                &mut values,
            );
            if pixel >> 24 == 0 {
                continue;
            }
            result.nontransparent_pixels += 1;
            for (channel, shift) in [16, 8, 0].into_iter().enumerate() {
                let byte = ((pixel >> shift) & 255) as usize;
                result.bins[channel][byte] += 1;
                result.bins[3][byte] += 1;
            }
        }
    }
    std::ptr::write_unaligned(output, result);
    abi::OK
}

#[cfg(test)]
#[path = "histogram_plan_tests.rs"]
mod tests;
