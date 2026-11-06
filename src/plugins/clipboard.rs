//! Session-local bounded clipboard; replay records document outcomes, never payload copies.
use crate::application::{ActionError, ActionResult, Operation, Session};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub(crate) struct Payload {
    pub(super) pixels: Vec<u32>,
    pub(super) size: [usize; 2],
    pub(super) origin: [i32; 2],
    pub(super) source_size: [usize; 2],
    hash: String,
}
impl Payload {
    pub(super) fn new(
        pixels: Vec<u32>,
        size: [usize; 2],
        origin: [i32; 2],
        source_size: [usize; 2],
    ) -> Result<Arc<Self>, String> {
        if size.contains(&0)
            || size.iter().any(|n| *n > 4096)
            || pixels.len() != size[0] * size[1]
            || source_size.contains(&0)
            || source_size.iter().any(|n| *n > 4096)
            || origin.iter().any(|n| *n < 0)
            || origin[0] as usize + size[0] > source_size[0]
            || origin[1] as usize + size[1] > source_size[1]
        {
            return Err("Invalid clipboard payload bounds".into());
        }
        let mut hash = Sha256::new();
        hash.update(b"MiCoPi session clipboard v1");
        for n in [
            size[0],
            size[1],
            origin[0] as usize,
            origin[1] as usize,
            source_size[0],
            source_size[1],
        ] {
            hash.update((n as u32).to_le_bytes());
        }
        let mut buffer = [0u8; 16384];
        for chunk in pixels.chunks(buffer.len() / 4) {
            for (p, bytes) in chunk.iter().zip(buffer.chunks_exact_mut(4)) {
                bytes.copy_from_slice(&p.to_le_bytes());
            }
            hash.update(&buffer[..chunk.len() * 4]);
        }
        let payload = Arc::new(Payload {
            pixels,
            size,
            origin,
            source_size,
            hash: format!("{:x}", hash.finalize()),
        });
        Ok(payload)
    }

    pub(crate) fn summary(&self) -> Value {
        json!({"available":true,"width":self.size[0],"height":self.size[1],
            "origin":self.origin,"source_size":self.source_size,"sha256":self.hash,
            "bytes":self.pixels.len()*4,"scope":"session"})
    }
}
pub(crate) fn state(s: &Session) -> Value {
    s.clipboard
        .as_ref()
        .map_or(json!({"available":false,"scope":"session"}), |p| {
            p.summary()
        })
}
pub(super) fn bounds(s: &Session) -> Result<[usize; 4], String> {
    let (w, h) = (s.doc.width, s.doc.height);
    let Some(mask) = &s.doc.selection().current else {
        return Ok([0, 0, w, h]);
    };
    let mut b = [w, h, 0, 0];
    for (i, c) in mask.coverage().iter().enumerate() {
        if *c != 0 {
            let (x, y) = (i % w, i / w);
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x + 1);
            b[3] = b[3].max(y + 1);
        }
    }
    if b[0] >= b[2] || b[1] >= b[3] {
        return Err("The active selection is empty".into());
    }
    Ok(b)
}
pub(super) fn validate_paste(s: &Session) -> Result<(), String> {
    s.doc.preflight_new_layer(None)?;
    // Preflight capacity before allocating a full destination layer.
    if s.doc.layers.len() >= crate::engine::MAX_LAYERS
        || s.doc.width * s.doc.height * (s.doc.layers.len() + 1) > 64 * 1024 * 1024
    {
        return Err("Layer count or document pixel limit reached".into());
    }
    if s.doc.graph.enabled && !s.doc.graph.linked {
        return Err("Link the layer stack before inserting pasted pixels".into());
    }
    Ok(())
}
pub(super) fn paste(
    s: &mut Session,
    p: &Payload,
    inplace: bool,
    args: &Value,
) -> Result<Value, String> {
    if args["x"].is_null() != args["y"].is_null() {
        return Err("Specify both x and y, or neither".into());
    }
    let (x, y) = if let (Some(x), Some(y)) = (args["x"].as_i64(), args["y"].as_i64()) {
        (x as i32, y as i32)
    } else if inplace {
        (p.origin[0], p.origin[1])
    } else {
        let b = bounds(s)?;
        (
            b[0] as i32 + (b[2] as i32 - b[0] as i32 - p.size[0] as i32).div_euclid(2),
            b[1] as i32 + (b[3] as i32 - b[1] as i32 - p.size[1] as i32).div_euclid(2),
        )
    };
    if x >= s.doc.width as i32
        || y >= s.doc.height as i32
        || x as i64 + p.size[0] as i64 <= 0
        || y as i64 + p.size[1] as i64 <= 0
    {
        return Err("Pasted pixels are entirely outside the canvas".into());
    }
    validate_paste(s)?;
    let mut pixels = vec![0; s.doc.width * s.doc.height];
    super::native::transfer(
        s,
        &abi::SelectionTransfer { kind: 1, x, y },
        &p.pixels,
        p.size,
        None,
        &mut pixels,
        [s.doc.width, s.doc.height],
    )?;
    s.finish_stroke();
    s.doc.insert_pixels(pixels)?;
    return Ok(
        json!({"clipboard":p.summary(),"x":x,"y":y,"layer_id":s.doc.layer_id(s.doc.active)}),
    );
}
pub(super) fn capture(s: &mut Session, merged: bool) -> Result<Arc<Payload>, String> {
    let b = bounds(s)?;
    let size = [b[2] - b[0], b[3] - b[1]];
    s.finish_stroke();
    if merged {
        s.doc.compose();
        if let Some(error) = &s.doc.render_error {
            return Err(error.clone());
        }
    }
    let source = if merged {
        &s.doc.composite
    } else {
        &s.doc.layers[s.doc.active].pixels
    };
    let mut pixels = vec![0; size[0] * size[1]];
    super::native::transfer(
        s,
        &abi::SelectionTransfer {
            kind: 0,
            x: b[0] as i32,
            y: b[1] as i32,
        },
        source,
        [s.doc.width, s.doc.height],
        s.doc.selection().current.as_ref(),
        &mut pixels,
        size,
    )?;
    Payload::new(
        pixels,
        size,
        [b[0] as i32, b[1] as i32],
        [s.doc.width, s.doc.height],
    )
}
pub(super) fn clear_pixels(s: &Session, p: &Payload) -> Result<Vec<u32>, String> {
    let mut cleared = vec![0; p.size[0] * p.size[1]];
    super::native::clip_region(
        s,
        p.origin[0] as usize,
        p.origin[1] as usize,
        p.size[0],
        p.size[1],
        &mut cleared,
    )?;
    Ok(cleared)
}
fn run(s: &mut Session, op: Operation, args: Value) -> Result<Value, String> {
    use Operation::*;
    if op == ClipboardState {
        return Ok(state(s));
    }
    if matches!(op, ClipboardPaste | ClipboardPasteInPlace) {
        let p = s.clipboard.clone().ok_or("Session clipboard is empty")?;
        return paste(s, &p, op == ClipboardPasteInPlace, &args);
    }
    if op == ClipboardCut && s.doc.alpha_locked(s.doc.active) {
        return Err("Cannot cut an alpha-locked layer".into());
    }
    let p = capture(s, op == ClipboardCopyMerged)?;
    if op == ClipboardCut {
        let cleared = clear_pixels(s, &p)?;
        s.doc.replace_region(
            p.origin[0] as usize,
            p.origin[1] as usize,
            p.size[0],
            p.size[1],
            &cleared,
        );
    }
    s.clipboard = Some(p);
    Ok(state(s))
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::ClipboardCut {
        super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
    }
    run(s, op, args).map_err(|e| ActionError::new("clipboard", e))
}
