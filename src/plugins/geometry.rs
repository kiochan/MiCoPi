//! Structural geometry orchestration; pixel and mask permutations live in the DLL.
use crate::{
    application::{ActionError, ActionResult, Operation, Session},
    engine::selection::SelectionState,
};
use micopi_plugin_api as abi;
fn run(s: &mut Session, op: Operation) -> Result<(), String> {
    use Operation::*;
    let layer = matches!(op, RotateLayerClockwise | RotateLayerCounterclockwise);
    let kind = match op {
        RotateCanvasClockwise | RotateLayerClockwise => 0,
        RotateCanvasCounterclockwise | RotateLayerCounterclockwise => 1,
        RotateCanvasHalf => 2,
        FlipCanvasHorizontal => 3,
        _ => 4,
    };
    let (sw, sh) = (s.doc.width, s.doc.height);
    if layer && !s.doc.layer_visible(s.doc.active) {
        return Err("Make the active layer visible before rotating it".into());
    }
    let plan = if layer {
        None
    } else {
        Some(
            crate::application::canvas_geometry::for_operation(&s.doc, op, &serde_json::json!({}))
                .map_err(|e| e.message)?,
        )
    };
    let (w, h, options) = if let Some(plan) = &plan {
        let options = plan.geometry.as_ref().unwrap();
        (
            plan.width,
            plan.height,
            abi::Geometry {
                kind: options.kind,
                x: options.x,
                y: options.y,
            },
        )
    } else {
        (
            sw,
            sh,
            abi::Geometry {
                kind,
                x: (sw as i32 - sh as i32).div_euclid(2),
                y: (sh as i32 - sw as i32).div_euclid(2),
            },
        )
    };
    if layer {
        s.finish_stroke();
        let mut pixels = super::native::geometry_pixels(
            s,
            &options,
            &s.doc.layers[s.doc.active].pixels,
            sw,
            sh,
            w,
            h,
        )?;
        super::native::clip_region(s, 0, 0, w, h, &mut pixels)?;
        if s.doc.alpha_locked(s.doc.active)
            && s.doc.layers[s.doc.active]
                .pixels
                .iter()
                .zip(&pixels)
                .any(|(a, b)| a >> 24 != b >> 24 || a >> 24 == 0 && a != b)
        {
            return Err("Rotation would change locked layer transparency".into());
        }
        s.doc.replace_region(0, 0, w, h, &pixels);
        return Ok(());
    }
    let masks = super::mask_geometry::stage(
        s,
        w,
        h,
        &super::mask_geometry::Sampling::Geometry(abi::A8Geometry {
            geometry: abi::Geometry {
                kind: options.kind,
                x: options.x,
                y: options.y,
            },
            fill: 255,
        }),
    )?;
    let state = s.doc.selection();
    let transform = |mask: &Option<crate::engine::selection::Mask>| {
        mask.as_ref()
            .map(|m| super::native::geometry_mask(s, &options, m, w, h))
            .transpose()
    };
    let selection = SelectionState {
        current: transform(&state.current)?,
        previous: transform(&state.previous)?,
    };
    let pixels = s
        .doc
        .layers
        .iter()
        .map(|l| super::native::geometry_pixels(s, &options, &l.pixels, sw, sh, w, h))
        .collect::<Result<Vec<_>, _>>()?;
    let info = plan.unwrap().info;
    s.doc
        .replace_geometry_with_masks(w, h, pixels, masks, selection, info)
}
pub(crate) fn execute(s: &mut Session, op: Operation) -> ActionResult {
    if !matches!(
        op,
        Operation::RotateLayerClockwise | Operation::RotateLayerCounterclockwise
    ) {
        super::mask_geometry::require_idle(s)?;
    }
    if matches!(
        op,
        Operation::RotateLayerClockwise | Operation::RotateLayerCounterclockwise
    ) {
        super::PixelAccess::Geometry.require(&s.doc, s.doc.active)?;
    }
    run(s, op).map_err(|e| ActionError::new("geometry", e))?;
    Ok(s.state())
}
