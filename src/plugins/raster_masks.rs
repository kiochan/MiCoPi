//! Public mask contracts; the document owns storage/history and DLLs own pixel math.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::raster_masks::{MaskOwner, PaintTarget, PlaneView, SamplePlane};
use micopi_plugin_api as abi;
use serde_json::{json, Value};

fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("raster_mask", message)
}

pub(crate) fn schema(op: Operation) -> Value {
    use Operation::*;
    let id = json!({"type":"integer","minimum":1,"maximum":4294967295u64});
    let owner = json!({"type":"object","properties":{
        "kind":{"type":"string","enum":["layer","group","adjustment"]},"id":id},
        "required":["kind","id"],"additionalProperties":false});
    let (properties, required) = match op {
        MaskQuery => (json!({"mask_id":id}), vec![]),
        MaskCreate => (
            json!({"owner":owner,
            "source":{"type":"string","enum":["reveal_all","hide_all","selection"],"default":"reveal_all"},
            "select":{"type":"boolean","default":true}}),
            vec!["owner"],
        ),
        MaskProperties => (
            json!({"mask_id":id,"enabled":{"type":"boolean"},
            "linked":{"type":"boolean"},"density":{"type":"integer","minimum":0,"maximum":255},
            "feather_radius_px":{"type":"integer","minimum":0,"maximum":256}}),
            vec!["mask_id"],
        ),
        MaskTarget => (
            json!({"target":{"type":"object","properties":{
            "kind":{"type":"string","enum":["layer_pixels","mask"]},"id":id},
            "required":["kind"],"additionalProperties":false}}),
            vec!["target"],
        ),
        MaskFill => (
            json!({"mask_id":id,"value":{"type":"integer","minimum":0,"maximum":255}}),
            vec!["mask_id", "value"],
        ),
        MaskToSelection => (
            json!({"mask_id":id,"effective":{"type":"boolean","default":false,
            "description":"Use enabled, density and feather coverage instead of raw mask samples."}}),
            vec!["mask_id"],
        ),
        MaskSnapshot => (
            json!({"mask_id":id,
            "x":{"type":"integer","minimum":0,"maximum":4095,"default":0},
            "y":{"type":"integer","minimum":0,"maximum":4095,"default":0},
            "width":{"type":"integer","minimum":1,"maximum":256,"default":64},
            "height":{"type":"integer","minimum":1,"maximum":256,"default":64}}),
            vec!["mask_id"],
        ),
        _ => (json!({"mask_id":id}), vec!["mask_id"]),
    };
    let description = match op {
        MaskApply => "Bake effective coverage into the raster owner's raw alpha and remove the mask, retaining source filters, layer opacity and blend mode. Fractional 8-bit opacity can round differently from live mask composition. Requires an idle contact and unlocked pixels/transparency; group-mask application requires a separate subtree operation.",
        MaskFill | MaskInvert => "Edit the entire raw A8 plane while retaining density and feather. Brush and Eraser use the current pixel selection for clipped painting.",
        MaskSnapshot => "Read a bounded rectangle of raw A8 bytes, clipped to the canvas edge, with the current document revision. This query preserves active contacts.",
        MaskCreate => "Create one mask for an explicit stable layer/group owner. A selection source copies the current coverage; the default source reveals all pixels. Target selection is independent of the active painting layer and multi-layer selection.",
        _ => "Inspect or edit an explicit raster mask through the shared document, history and verified-outcome contracts.",
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false,"description":description})
}

/// Literal validation also runs before a workflow's earlier operations execute.
pub(crate) fn validate(op: Operation, args: &Value) -> Result<(), ActionError> {
    if op == Operation::MaskTarget {
        serde_json::from_value::<PaintTarget>(args["target"].clone())?;
        if args["target"]["kind"] == "layer_pixels" && args["target"].get("id").is_some() {
            return Err(error("A pixel painting target does not take a mask ID"));
        }
    }
    if op == Operation::MaskCreate {
        serde_json::from_value::<MaskOwner>(args["owner"].clone())?;
    }
    Ok(())
}

pub(crate) fn state(s: &Session) -> Value {
    let doc = &s.doc;
    let items: Vec<_> = doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            json!({"id":m.id,"owner":m.owner,"enabled":m.enabled,"linked":m.linked,
            "density":m.density,"feather_radius_px":m.feather_radius_px})
        })
        .collect();
    json!({"version":doc.mask_state().version,"items":items,"target":doc.mask_state().target,
        "width":doc.width,"height":doc.height,"next_id":doc.mask_state().next_id,
        "capabilities":{"composition":doc.renderer.as_ref().is_some_and(|r|r.supports_masks()),
            "effects":doc.renderer.as_ref().is_some_and(|r|r.supports_mask_effects())},
        "limits":{"count":crate::engine::raster_masks::MAX_MASKS,
            "logical_bytes":crate::engine::raster_masks::MASK_SAMPLE_LIMIT,"feather_radius_px":256}})
}

fn idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before editing raster masks",
        ));
    }
    Ok(())
}

fn raw_samples(s: &Session, id: u64) -> Result<Vec<u8>, ActionError> {
    Ok(match s.doc.mask_plane(id).map_err(error)? {
        PlaneView::Uniform(v) => vec![v; s.doc.width * s.doc.height],
        PlaneView::Dense(data) => data.to_vec(),
    })
}

fn selection_samples(s: &Session) -> Result<SamplePlane, ActionError> {
    let mask = s.doc.selection().current.as_ref().ok_or_else(|| {
        ActionError::new(
            "selection_empty",
            "An active selection is required to replace mask samples",
        )
    })?;
    Ok(SamplePlane::Dense(mask.coverage().to_vec()))
}

fn result(s: &Session, id: Option<u64>) -> Value {
    let mut value = json!({"raster_masks":state(s)});
    if let Some(id) = id {
        value["mask_id"] = json!(id);
    }
    value
}

fn snapshot(s: &Session, args: &Value) -> ActionResult {
    let id = args["mask_id"].as_u64().unwrap();
    let x = args["x"].as_u64().unwrap_or(0) as usize;
    let y = args["y"].as_u64().unwrap_or(0) as usize;
    if x >= s.doc.width || y >= s.doc.height {
        return Err(error("Mask snapshot origin is outside the canvas"));
    }
    let width = args["width"].as_u64().unwrap_or(64) as usize;
    let height = args["height"].as_u64().unwrap_or(64) as usize;
    let width = width.min(s.doc.width - x);
    let height = height.min(s.doc.height - y);
    let plane = s.doc.mask_plane(id).map_err(error)?;
    let mut samples = Vec::with_capacity(width * height);
    match plane {
        PlaneView::Uniform(v) => samples.resize(width * height, v),
        PlaneView::Dense(data) => {
            for row in y..y + height {
                samples
                    .extend_from_slice(&data[row * s.doc.width + x..row * s.doc.width + x + width]);
            }
        }
    }
    Ok(
        json!({"mask_id":id,"x":x,"y":y,"width":width,"height":height,
        "encoding":"a8","samples":samples,"revision":s.revision}),
    )
}

fn invert(s: &Session, id: u64) -> Result<SamplePlane, ActionError> {
    let source = match s.doc.mask_plane(id).map_err(error)? {
        PlaneView::Uniform(value) => abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: value as u32,
        },
        PlaneView::Dense(data) => abi::CompositeMask {
            data: data.as_ptr(),
            len: data.len(),
            stride: s.doc.width as u32,
            constant: 0,
        },
    };
    let renderer = s
        .doc
        .renderer
        .as_ref()
        .ok_or_else(|| error("The compositor plugin is unavailable"))?;
    let mut bytes = vec![0; s.doc.width * s.doc.height];
    let mut output = abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
        width: s.doc.width as u32,
        height: s.doc.height as u32,
    };
    renderer
        .mask_effect_region(
            &abi::MaskEffectRequest {
                flags: 1,
                density: 255,
                feather_radius: 0,
                reserved: 0,
            },
            &source,
            s.doc.width as u32,
            s.doc.height as u32,
            &mut output,
        )
        .map_err(error)?;
    if bytes.iter().all(|v| *v == bytes[0]) {
        Ok(SamplePlane::Uniform(bytes[0]))
    } else {
        Ok(SamplePlane::Dense(bytes))
    }
}

pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    validate(op, &args)?;
    let id = args["mask_id"].as_u64();
    if op == MaskQuery {
        let mut value = state(s);
        if let Some(id) = id {
            s.doc.mask_plane(id).map_err(error)?;
            value["items"]
                .as_array_mut()
                .unwrap()
                .retain(|v| v["id"] == id);
        }
        value["revision"] = json!(s.revision);
        return Ok(value);
    }
    if op == MaskSnapshot {
        return snapshot(s, &args);
    }
    idle(s)?;
    let mut applied_layer = None;
    match op {
        MaskCreate => {
            let owner: MaskOwner = serde_json::from_value(args["owner"].clone())?;
            let samples = match args["source"].as_str().unwrap_or("reveal_all") {
                "hide_all" => SamplePlane::Uniform(0),
                "selection" => selection_samples(s)?,
                _ => SamplePlane::Uniform(255),
            };
            s.doc.begin_history_group().map_err(error)?;
            let created = (|| {
                let initial = if let SamplePlane::Uniform(v) = &samples {
                    *v
                } else {
                    255
                };
                let id = s.doc.create_mask(owner, initial).map_err(error)?;
                if !matches!(samples, SamplePlane::Uniform(_)) {
                    s.doc.replace_mask_plane(id, samples).map_err(error)?;
                }
                if args["select"].as_bool().unwrap_or(true) {
                    s.doc
                        .set_mask_target(PaintTarget::Mask { id })
                        .map_err(error)?;
                }
                Ok::<_, ActionError>(id)
            })();
            match created {
                Ok(id) => {
                    s.doc.end_history_group().map_err(error)?;
                    compose(s)?;
                    return Ok(result(s, Some(id)));
                }
                Err(e) => {
                    s.doc.cancel_history_group();
                    return Err(e);
                }
            }
        }
        MaskTarget => s
            .doc
            .set_mask_target(serde_json::from_value(args["target"].clone())?)
            .map_err(error)?,
        MaskDelete => s.doc.delete_mask(id.unwrap()).map_err(error)?,
        MaskProperties => {
            let mut metadata = s
                .doc
                .mask_state()
                .items
                .iter()
                .find(|v| Some(v.id) == id)
                .cloned()
                .ok_or_else(|| error("Raster mask does not exist"))?;
            if let Some(v) = args["enabled"].as_bool() {
                metadata.enabled = v;
            }
            if let Some(v) = args["linked"].as_bool() {
                metadata.linked = v;
            }
            if let Some(v) = args["density"].as_u64() {
                metadata.density = v as u8;
            }
            if let Some(v) = args["feather_radius_px"].as_u64() {
                metadata.feather_radius_px = v as u16;
            }
            s.doc.update_mask_metadata(metadata).map_err(error)?;
        }
        MaskFill => {
            s.doc
                .replace_mask_plane(
                    id.unwrap(),
                    SamplePlane::Uniform(args["value"].as_u64().unwrap() as u8),
                )
                .map_err(error)?;
        }
        MaskInvert => {
            let samples = invert(s, id.unwrap())?;
            s.doc
                .replace_mask_plane(id.unwrap(), samples)
                .map_err(error)?;
        }
        MaskFromSelection => {
            let samples = selection_samples(s)?;
            s.doc
                .replace_mask_plane(id.unwrap(), samples)
                .map_err(error)?;
        }
        MaskToSelection => {
            let samples = if args["effective"].as_bool().unwrap_or(false) {
                match s.doc.effective_mask_plane(id.unwrap()).map_err(error)? {
                    PlaneView::Uniform(v) => vec![v; s.doc.width * s.doc.height],
                    PlaneView::Dense(data) => data.to_vec(),
                }
            } else {
                raw_samples(s, id.unwrap())?
            };
            let current = Some(
                crate::engine::selection::Mask::new(s.doc.width, s.doc.height, samples)
                    .map_err(error)?,
            );
            let old = s.doc.selection().clone();
            s.doc
                .set_selection(crate::engine::selection::SelectionState {
                    current,
                    previous: old.current.or(old.previous),
                })
                .map_err(error)?;
        }
        MaskApply => {
            applied_layer = Some(s.doc.apply_raster_mask(id.unwrap()).map_err(error)?);
        }
        _ => return Err(error("Operation routed to the wrong mask handler")),
    }
    if !matches!(op, MaskTarget | MaskToSelection) {
        compose(s)?;
    }
    let mut value = result(s, id);
    if let Some(layer_id) = applied_layer {
        value["layer_id"] = json!(layer_id);
        value["bake_mode"] = json!("raw_alpha");
    }
    Ok(value)
}

fn compose(s: &mut Session) -> Result<(), ActionError> {
    s.doc.compose();
    if let Some(message) = &s.doc.render_error {
        return Err(ActionError::new("mask_render", message));
    }
    Ok(())
}
