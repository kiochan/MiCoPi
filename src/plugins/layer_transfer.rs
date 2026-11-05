//! Selection-to-layer orchestration; native kernels own pixel math, the host owns history.
use crate::application::{ActionError, ActionResult, Operation, Session};
use micopi_plugin_api as abi;
fn transfer(s: &mut Session, cut: bool) -> Result<(), String> {
    if s.doc.selection().current.is_none() {
        if cut {
            return Err("Layer via Cut requires an active selection".into());
        }
        s.finish_stroke();
        return s.doc.duplicate_layer(s.doc.active);
    }
    let b = super::clipboard::bounds(s)?;
    if cut && s.doc.alpha_locked(s.doc.active) {
        return Err("Cannot cut an alpha-locked layer".into());
    }
    if s.doc.layers.len() >= crate::engine::MAX_LAYERS
        || s.doc.width * s.doc.height * (s.doc.layers.len() + 1) > 64 * 1024 * 1024
    {
        return Err("Layer count or document pixel limit reached".into());
    }
    if s.doc.graph.enabled && !s.doc.graph.linked {
        return Err("Link the layer stack before extracting selected pixels".into());
    }
    s.finish_stroke();
    let size = [b[2] - b[0], b[3] - b[1]];
    let mut region = vec![0; size[0] * size[1]];
    super::native::transfer(
        s,
        &abi::SelectionTransfer {
            kind: 0,
            x: b[0] as i32,
            y: b[1] as i32,
        },
        &s.doc.layers[s.doc.active].pixels,
        [s.doc.width, s.doc.height],
        s.doc.selection().current.as_ref(),
        &mut region,
        size,
    )?;
    let mut pixels = vec![0; s.doc.width * s.doc.height];
    super::native::transfer(
        s,
        &abi::SelectionTransfer {
            kind: 1,
            x: b[0] as i32,
            y: b[1] as i32,
        },
        &region,
        size,
        None,
        &mut pixels,
        [s.doc.width, s.doc.height],
    )?;
    // Reuse the cropped staging allocation; the clipboard is never involved.
    if cut {
        region.fill(0);
        super::native::clip_region(s, b[0], b[1], size[0], size[1], &mut region)?;
    }
    s.finish_stroke();
    s.doc.insert_transferred_pixels(
        pixels,
        cut.then_some(([b[0], b[1], size[0], size[1]], region.as_slice())),
    )
}
pub(crate) fn execute(s: &mut Session, op: Operation) -> ActionResult {
    transfer(s, op == Operation::LayerViaCut).map_err(|e| ActionError::new("layer_transfer", e))?;
    Ok(s.state())
}
