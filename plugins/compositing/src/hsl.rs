//! Independent HSL plan capability; old rendering tables still reject kind 11.
use crate::{hsl_math::Parameters, overlaps, span};
use micopi_plugin_api as abi;

unsafe fn prepare(
    parameters: *const abi::HslParameters,
    count: usize,
    destination: (usize, usize),
    descriptor: (usize, usize),
) -> Option<[Parameters; abi::HSL_MAX_PARAMETERS]> {
    if count > abi::HSL_MAX_PARAMETERS {
        return None;
    }
    let mut prepared = [Parameters::default(); abi::HSL_MAX_PARAMETERS];
    if count == 0 {
        return Some(prepared);
    }
    if parameters.is_null() || parameters as usize % std::mem::align_of::<abi::HslParameters>() != 0
    {
        return None;
    }
    let source = span(parameters, count)?;
    if overlaps(source, destination) || overlaps(source, descriptor) {
        return None;
    }
    for (out, value) in prepared
        .iter_mut()
        .zip(std::slice::from_raw_parts(parameters, count))
    {
        *out = Parameters::new(value.hue, value.saturation, value.lightness)?;
    }
    Some(prepared)
}
unsafe extern "C" fn render(
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    parameters: *const abi::HslParameters,
    parameter_count: usize,
    width: u32,
    height: u32,
    output: *mut abi::Pixels,
) -> i32 {
    if output.is_null() || lut_count > 64 || lut_count != 0 && luts.is_null() {
        return abi::ERROR;
    }
    let target = std::ptr::read_unaligned(output);
    let Some(destination) = span(target.data, target.len) else {
        return abi::ERROR;
    };
    let Some(descriptor) = span(output, 1) else {
        return abi::ERROR;
    };
    if lut_count != 0 && span(luts, lut_count).is_none_or(|s| overlaps(s, destination)) {
        return abi::ERROR;
    }
    let Some(prepared) = prepare(parameters, parameter_count, destination, descriptor) else {
        return abi::ERROR;
    };
    let luts = if lut_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(luts, lut_count)
    };
    crate::masks::render_color_plan(
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
        true,
        &prepared[..parameter_count],
    )
}
unsafe extern "C" fn histogram(
    request: *const abi::HistogramRequest,
    nodes: *const abi::CompositeNode,
    count: usize,
    layers: *const abi::CompositeLayer,
    layer_count: usize,
    masks: *const abi::CompositeMask,
    mask_count: usize,
    luts: *const abi::CompositeLut,
    lut_count: usize,
    parameters: *const abi::HslParameters,
    parameter_count: usize,
    output: *mut abi::HistogramResult,
) -> i32 {
    let Some(destination) = span(output, 1) else {
        return abi::ERROR;
    };
    let Some(prepared) = prepare(parameters, parameter_count, destination, destination) else {
        return abi::ERROR;
    };
    crate::histogram::plan::sample_color_plan(
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
        &prepared[..parameter_count],
    )
}
static API: abi::HslApi = abi::HslApi {
    abi_version: 1,
    struct_size: std::mem::size_of::<abi::HslApi>() as u32,
    render: Some(render),
    histogram: Some(histogram),
};
#[no_mangle]
pub extern "C" fn micopi_hsl_v1() -> *const abi::HslApi {
    &API
}

#[cfg(test)]
#[path = "hsl_tests.rs"]
mod tests;
