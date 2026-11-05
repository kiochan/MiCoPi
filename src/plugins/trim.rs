//! Native merged-image bounds with host-owned all-layer framing/history.
use crate::application::{ActionError, ActionResult, Operation, Session};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
fn analyze(s: &mut Session, args: &Value) -> Result<Value, String> {
    let mut sides = 0;
    for (key, bit) in [("top", 1), ("right", 2), ("bottom", 4), ("left", 8)] {
        if args[key].as_bool().unwrap_or(true) {
            sides |= bit;
        }
    }
    if sides == 0 {
        return Err("Choose at least one side to trim".into());
    }
    let options = abi::Trim {
        mode: match args["mode"].as_str() {
            Some("top_left") => 1,
            Some("bottom_right") => 2,
            _ => 0,
        },
        tolerance: args["tolerance"].as_f64().unwrap_or(0.) as u32,
        sides,
    };
    let b = super::native::trim_bounds(s, &options)?;
    let empty = b.width == 0;
    let (x, y, w, h) = if empty {
        (0, 0, s.doc.width, s.doc.height)
    } else {
        (
            b.x as usize,
            b.y as usize,
            b.width as usize,
            b.height as usize,
        )
    };
    Ok(
        json!({"x":x,"y":y,"width":w,"height":h,"source_width":s.doc.width,"source_height":s.doc.height,"empty":empty,"would_change":!empty&&(x,y,w,h)!=(0,0,s.doc.width,s.doc.height),"revision":s.revision,"drawing":s.is_drawing()}),
    )
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::CanvasTrim {
        s.finish_stroke();
    }
    let b = analyze(s, &args).map_err(|e| ActionError::new("trim", e))?;
    if op == Operation::TrimBounds {
        return Ok(b);
    }
    if b["would_change"] == true {
        s.doc
            .reframe(
                b["width"].as_u64().unwrap() as usize,
                b["height"].as_u64().unwrap() as usize,
                -(b["x"].as_i64().unwrap() as i32),
                -(b["y"].as_i64().unwrap() as i32),
            )
            .map_err(|e| ActionError::new("trim", e))?;
    }
    Ok(s.state())
}
