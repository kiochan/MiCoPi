//! Cold instruction-indexed quantization preparation; no image allocations.
use crate::quantize_math::{self, Quantize};
use micopi_plugin_api as abi;
pub(super) unsafe fn prepare(
    nodes: *const abi::CompositeNode,
    count: usize,
) -> Option<[Quantize; 64]> {
    if count > 64
        || (count != 0
            && (nodes.is_null()
                || nodes as usize % std::mem::align_of::<abi::CompositeNode>() != 0))
    {
        return None;
    }
    let mut result = [Quantize::Threshold(0); 64];
    if count == 0 {
        return Some(result);
    }
    crate::span(nodes, count)?;
    for (slot, node) in result
        .iter_mut()
        .zip(std::slice::from_raw_parts(nodes, count))
    {
        *slot = match node.kind {
            12 => quantize_math::threshold(node.mode)?,
            13 => quantize_math::posterize(node.mode)?,
            _ => Quantize::Threshold(0),
        };
    }
    Some(result)
}

#[cfg(test)]
#[path = "quantization_tests.rs"]
mod tests;
