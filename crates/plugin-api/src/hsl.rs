//! Independent optional HSL plan rendering and bounded observation.
use crate::{
    CompositeLayer, CompositeLut, CompositeMask, CompositeNode, HistogramRequest, HistogramResult,
    Pixels,
};
pub const HSL_SYMBOL: &[u8] = b"micopi_hsl_v1\0";
pub const HSL_MAX_PARAMETERS: usize = 64;
/// Fixed-stride version-one records: finite degrees [-180,180] and percentages
/// [-100,100]. No pointers, ownership or per-record version tails.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct HslParameters {
    pub hue: f64,
    pub saturation: f64,
    pub lightness: f64,
}
/// Kind 11: a=image input, b=0, mode=parameter index, amount=1.
/// Complete declarations, including unused parameters, validate before writes.
pub type HslRenderFn = unsafe extern "C" fn(
    *const CompositeNode,
    usize,
    *const CompositeLayer,
    usize,
    *const CompositeMask,
    usize,
    *const CompositeLut,
    usize,
    *const HslParameters,
    usize,
    u32,
    u32,
    *mut Pixels,
) -> i32;
pub type HslHistogramFn = unsafe extern "C" fn(
    *const HistogramRequest,
    *const CompositeNode,
    usize,
    *const CompositeLayer,
    usize,
    *const CompositeMask,
    usize,
    *const CompositeLut,
    usize,
    *const HslParameters,
    usize,
    *mut HistogramResult,
) -> i32;
/// Both callbacks are mandatory. Other compositor/Levels/Curves/histogram tables
/// remain unchanged and do not accept HSL instructions.
#[repr(C)]
pub struct HslApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub render: Option<HslRenderFn>,
    pub histogram: Option<HslHistogramFn>,
}
pub type HslEntry = unsafe extern "C" fn() -> *const HslApi;

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};
    #[test]
    fn hsl_has_independent_stable_x64_layout() {
        assert_eq!(size_of::<HslParameters>(), 24);
        assert_eq!(offset_of!(HslParameters, saturation), 8);
        assert_eq!(offset_of!(HslParameters, lightness), 16);
        assert_eq!(size_of::<HslApi>(), 24);
        assert_eq!(offset_of!(HslApi, render), 8);
        assert_eq!(offset_of!(HslApi, histogram), 16);
        assert_eq!(HSL_SYMBOL, b"micopi_hsl_v1\0");
    }
}
