//! Image sizing orchestration; algorithms stay in the native painting plugin.
use crate::{
    application::{ActionError, ActionResult, Session},
    engine::selection::SelectionState,
};
use micopi_plugin_api as abi;
use serde_json::Value;
/// Round positive aspect dimensions half up; reject, rather than clamp, invalid results.
pub(crate) fn dimensions(
    source: [usize; 2],
    width: Option<usize>,
    height: Option<usize>,
    locked: bool,
) -> Result<[usize; 2], String> {
    let result = match (width, height, locked) {
        (Some(w), None, true) => [w, (w * source[1] + source[0] / 2) / source[0]],
        (None, Some(h), true) => [(h * source[0] + source[1] / 2) / source[1], h],
        (Some(w), Some(h), false) => [w, h],
        _ => {
            return Err(
                "With aspect lock supply exactly one dimension; otherwise supply both".into(),
            )
        }
    };
    if result.iter().any(|n| !(1..=4096).contains(n)) {
        return Err("Derived image dimensions must be between 1 and 4096 pixels".into());
    }
    Ok(result)
}
fn run(s: &mut Session, args: Value) -> Result<(), String> {
    let [w, h] = dimensions(
        [s.doc.width, s.doc.height],
        args["width"].as_f64().map(|v| v as usize),
        args["height"].as_f64().map(|v| v as usize),
        args["lock_aspect"].as_bool().unwrap_or(true),
    )?;
    if w * h * s.doc.layers.len() > 64 * 1024 * 1024 {
        return Err("Image size exceeds document pixel budget".into());
    }
    s.finish_stroke();
    // Preserve raw words and history for an identity resize, including old plugins.
    if (w, h) == (s.doc.width, s.doc.height) {
        return Ok(());
    }
    let options = abi::Resample {
        method: match args["method"].as_str() {
            Some("nearest") => 0,
            Some("area") => 2,
            _ => 1,
        },
    };
    let state = s.doc.selection();
    let transform = |mask: &Option<crate::engine::selection::Mask>| {
        mask.as_ref()
            .map(|m| super::native::resample_mask(s, &options, m, w, h))
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
        .map(|l| {
            super::native::resample_pixels(s, &options, &l.pixels, s.doc.width, s.doc.height, w, h)
        })
        .collect::<Result<Vec<_>, _>>()?;
    s.doc.replace_geometry(w, h, pixels, selection)
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    run(s, args).map_err(|e| ActionError::new("image_size", e))?;
    Ok(s.state())
}
