//! Host validation/history for selection commands; the DLL owns geometry kernels.
use crate::application::{ActionError, ActionResult, Operation, Session};
use serde_json::Value;
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    if op == ReadSelection {
        return Ok(serde_json::to_value(s.doc.selection())?);
    }
    let old = s.doc.selection().clone();
    let current = match op {
        Deselect => None,
        Reselect => Some(old.previous.clone().ok_or_else(|| {
            ActionError::new("selection_empty", "No previous selection to restore")
        })?),
        _ => {
            let mut shape = micopi_plugin_api::SelectionShape {
                kind: 3,
                mode: 0,
                bounds: [0.; 4],
                points: std::ptr::null(),
                count: 0,
            };
            let mut points = Vec::new();
            if op == InvertSelection {
                shape.kind = 4;
            }
            if op == SelectionShape {
                shape.mode = match args["mode"].as_str().unwrap_or("replace") {
                    "add" => 1,
                    "subtract" => 2,
                    "intersect" => 3,
                    _ => 0,
                };
                shape.kind = match args["kind"].as_str().unwrap() {
                    "rectangle" => 0,
                    "ellipse" => 1,
                    _ => 2,
                };
                if shape.kind == 2 {
                    points = serde_json::from_value(args["points"].clone()).map_err(|_| {
                        ActionError::new(
                            "invalid_arguments",
                            "Polygon selection requires coordinate pairs",
                        )
                    })?;
                    shape.points = points.as_ptr();
                    shape.count = points.len();
                } else {
                    let width = args["width"].as_f64().filter(|v| *v > 0.).ok_or_else(|| {
                        ActionError::new("invalid_arguments", "Selection width must be positive")
                    })?;
                    let height = args["height"].as_f64().filter(|v| *v > 0.).ok_or_else(|| {
                        ActionError::new("invalid_arguments", "Selection height must be positive")
                    })?;
                    shape.bounds = [
                        args["x"].as_f64().unwrap_or(0.) as f32,
                        args["y"].as_f64().unwrap_or(0.) as f32,
                        width as f32,
                        height as f32,
                    ];
                }
            }
            // Validate/rasterize before publishing selection metadata. The local
            // point buffer remains alive throughout the synchronous DLL call.
            let result = super::native::selection_shape(s, &shape)
                .map_err(|e| ActionError::new("selection_geometry", e));
            drop(points);
            Some(result?)
        }
    };
    s.finish_stroke();
    let previous = old.current.clone().or(old.previous);
    s.doc
        .set_selection(crate::engine::selection::SelectionState { current, previous })
        .map_err(|e| ActionError::new("selection", e))?;
    Ok(s.state())
}
