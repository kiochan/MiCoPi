//! Explicit external desktop effects. Never used by outcome replay or atomic groups.
use super::clipboard::{self, Payload};
use crate::{
    application::{ActionError, ActionResult, Operation, Session},
    platform::clipboard::{ReadImage, View as ImageView},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    version: u32,
    origin: [i32; 2],
    source_size: [usize; 2],
    sha256: String,
}
fn imported(read: ReadImage) -> Result<(Arc<Payload>, bool, &'static str), String> {
    let size = [read.image.width, read.image.height];
    let origin = read
        .metadata
        .as_deref()
        .and_then(|bytes| {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            serde_json::from_slice::<Origin>(&bytes[..end]).ok()
        })
        .filter(|m| {
            m.version == 1
                && m.sha256.len() == 64
                && !m.source_size.contains(&0)
                && m.source_size.iter().all(|n| *n <= 4096)
                && m.origin.iter().all(|n| *n >= 0)
                && m.origin[0] as usize + size[0] <= m.source_size[0]
                && m.origin[1] as usize + size[1] <= m.source_size[1]
        });
    let p = if let Some(origin) = origin {
        let candidate = Payload::new(read.image.pixels, size, origin.origin, origin.source_size)?;
        if candidate.summary()["sha256"] == origin.sha256 {
            return Ok((candidate, true, read.format));
        }
        let candidate =
            Arc::try_unwrap(candidate).map_err(|_| "Unexpected shared clipboard import")?;
        Payload::new(candidate.pixels, size, [0, 0], size)?
    } else {
        Payload::new(read.image.pixels, size, [0, 0], size)?
    };
    Ok((p, false, read.format))
}
fn run(s: &mut Session, op: Operation) -> Result<Value, String> {
    use Operation::*;
    let backend = s
        .system_clipboard
        .as_ref()
        .ok_or("System clipboard requires a Windows desktop session")?;
    if op == SystemClipboardState {
        return backend.status();
    }
    if matches!(op, SystemClipboardPaste | SystemClipboardPasteInPlace) {
        clipboard::validate_paste(s)?;
        let (p, verified, format) = imported(backend.read()?)?;
        let result = clipboard::paste(s, &p, op == SystemClipboardPasteInPlace, &json!({}))?;
        s.system_clipboard_event = Some(
            json!({"clipboard":p.summary(),"system":{"direction":"read","format":format,"origin_verified":verified}}),
        );
        s.clipboard = Some(p);
        return Ok(result);
    }
    if op == SystemClipboardCut && s.doc.alpha_locked(s.doc.active) {
        return Err("Cannot cut an alpha-locked layer".into());
    }
    let p = clipboard::capture(s, op == SystemClipboardCopyMerged)?;
    let clear = if op == SystemClipboardCut {
        Some(clipboard::clear_pixels(s, &p)?)
    } else {
        None
    };
    let mut metadata=serde_json::to_vec(&json!({"version":1,"origin":p.origin,"source_size":p.source_size,"sha256":p.summary()["sha256"]})).map_err(|e|e.to_string())?;
    metadata.push(0);
    let result = s.system_clipboard.as_ref().unwrap().write(
        &ImageView {
            width: p.size[0],
            height: p.size[1],
            pixels: &p.pixels,
        },
        &metadata,
    )?;
    if let Some(clear) = clear {
        s.doc.replace_region(
            p.origin[0] as usize,
            p.origin[1] as usize,
            p.size[0],
            p.size[1],
            &clear,
        );
    }
    s.system_clipboard_event =
        Some(json!({"clipboard":p.summary(),"system":{"direction":"write","publication":result}}));
    s.clipboard = Some(p);
    Ok(json!({"clipboard":clipboard::state(s),"system":result}))
}
pub(crate) fn execute(s: &mut Session, op: Operation) -> ActionResult {
    if s.runtime != "desktop" || s.system_clipboard.is_none() {
        return Err(ActionError::new(
            "ui_unavailable",
            "System clipboard requires a Windows desktop session",
        ));
    }
    if op == Operation::SystemClipboardCut {
        super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
    }
    run(s, op).map_err(|e| ActionError::new("system_clipboard", e))
}
