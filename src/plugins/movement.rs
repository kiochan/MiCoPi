//! Atomic integer movement; native kernels own pixel/coverage math.
pub(crate) use super::owner_movement::{admit_masked, validate_literal};
#[cfg(feature = "desktop")]
pub(crate) use super::owner_movement::{destination, preview, MoveDestination};
use crate::application::{ActionError, ActionResult, MoveTarget, Operation, Session};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
pub(crate) fn resolved_target(s: &Session, target: MoveTarget) -> MoveTarget {
    if target == MoveTarget::Auto {
        if s.doc.selection().current.is_some() {
            MoveTarget::Selection
        } else {
            MoveTarget::Layer
        }
    } else {
        target
    }
}
pub(crate) fn preview_bounds(s: &Session, target: MoveTarget) -> Result<[usize; 4], String> {
    let target = resolved_target(s, target);
    if target != MoveTarget::SelectionMask && !s.doc.layer_visible(s.doc.active) {
        return Err("Make the active layer visible before moving pixels".into());
    }
    if target == MoveTarget::Layer {
        Ok([0, 0, s.doc.width, s.doc.height])
    } else {
        if s.doc.selection().current.is_none() {
            return Err("Selection movement requires a nonempty active selection".into());
        }
        super::clipboard::bounds(s)
    }
}
fn options(s: &Session, op: Operation, args: &Value) -> abi::Move {
    let p = s.movement_options();
    let target: MoveTarget = args
        .get("target")
        .map(|v| serde_json::from_value(v.clone()).unwrap())
        .unwrap_or(p.target);
    let target = if target == MoveTarget::Auto {
        if s.doc.selection().current.is_some() {
            MoveTarget::Selection
        } else {
            MoveTarget::Layer
        }
    } else {
        target
    };
    let (dx, dy) = if op == Operation::Nudge {
        let step = if args["large"] == true {
            p.large_step
        } else {
            p.step
        } as i32;
        match args["direction"].as_str().unwrap() {
            "left" => (-step, 0),
            "right" => (step, 0),
            "up" => (0, -step),
            _ => (0, step),
        }
    } else {
        (
            args["dx"].as_f64().unwrap() as i32,
            args["dy"].as_f64().unwrap() as i32,
        )
    };
    abi::Move {
        kind: match target {
            MoveTarget::Layer => 0,
            MoveTarget::Selection => 1,
            _ => 2,
        },
        dx,
        dy,
    }
}
pub(crate) fn context(s: &Session, op: Operation, args: &Value) -> Value {
    if s.doc.has_raster_masks() || args.get("mask_id").is_some() {
        return super::owner_movement::context(s, op, args);
    }
    let o = options(s, op, args);
    json!({"target":(["layer","selection","selection_mask"][o.kind as usize]),"dx":o.dx,"dy":o.dy})
}
fn run(s: &mut Session, op: Operation, args: &Value) -> Result<(), String> {
    let o = options(s, op, args);
    if o.kind != 2 && !s.doc.layer_visible(s.doc.active) {
        return Err("Make the active layer visible before moving pixels".into());
    }
    if o.kind != 0
        && s.doc
            .selection()
            .current
            .as_ref()
            .is_none_or(|m| m.coverage().iter().all(|c| *c == 0))
    {
        return Err("Selection movement requires a nonempty active selection".into());
    }
    s.finish_stroke();
    if (o.dx, o.dy) == (0, 0) {
        return Ok(());
    }
    let mut selection = s.doc.selection().clone();
    if o.kind != 0 {
        selection.current = Some(super::native::move_mask(s, &o)?);
    }
    let pixels = if o.kind == 2 {
        None
    } else {
        Some(super::native::move_pixels(s, &o)?)
    };
    s.doc.apply_movement(pixels.as_deref(), selection)
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    validate_literal(&args)?;
    if s.doc.has_raster_masks() || args.get("mask_id").is_some() {
        return super::owner_movement::execute(s, op, &args);
    }
    if options(s, op, &args).kind != 2 {
        super::PixelAccess::Geometry.require(&s.doc, s.doc.active)?;
    }
    run(s, op, &args).map_err(|e| ActionError::new("movement", e))?;
    Ok(s.state())
}
