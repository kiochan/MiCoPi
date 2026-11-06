//! Built-in layers plugin action handlers; callable without a GUI.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::MAX_LAYERS;
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    let index = || -> Result<usize, ActionError> {
        let by_index = args["index"].as_u64().map(|v| v as usize);
        let by_id = args["layer_id"]
            .as_u64()
            .map(|id| {
                s.doc
                    .layer_index(id)
                    .ok_or_else(|| ActionError::new("invalid_layer", "Layer ID does not exist"))
            })
            .transpose()?;
        if by_index.is_some() && by_id.is_some() && by_index != by_id {
            return Err(ActionError::new(
                "invalid_layer",
                "Layer index and ID refer to different layers",
            ));
        }
        let i = by_id.or(by_index).unwrap_or(s.doc.active);
        if i >= s.doc.layers.len() {
            return Err(ActionError::new("invalid_layer", "Layer does not exist"));
        }
        Ok(i)
    };
    match op {
        LayerSelection => {
            if s.stroke.active() || s.pointer_checkpoint.is_some() {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before selecting layers",
                ));
            }
            let ids: Vec<u64> = serde_json::from_value(args["layer_ids"].clone())?;
            s.doc
                .set_layer_selection(&ids, args["active_layer_id"].as_u64())
                .map_err(|e| ActionError::new("invalid_layer_selection", e))?;
            Ok(s.state())
        }
        MergeDown | MergeVisible | FlattenImage | StampVisible => {
            if op == MergeVisible && s.doc.has_groups() {
                if s.stroke.active() || s.pointer_checkpoint.is_some() {
                    return Err(ActionError::new(
                        "stroke_active",
                        "Finish the current contact before merging grouped layers",
                    ));
                }
            }
            use crate::engine::document::MergeLayers;
            let i = if op == MergeDown {
                index()?
            } else {
                s.doc.active
            };
            let operation = match op {
                MergeDown => MergeLayers::Down,
                MergeVisible => MergeLayers::Visible,
                FlattenImage => MergeLayers::Flatten,
                _ => MergeLayers::Stamp,
            };
            let consumed: Vec<_> = match op {
                MergeDown if i > 0 => vec![i - 1, i],
                MergeVisible => {
                    let v: Vec<_> = (0..s.doc.layers.len())
                        .filter(|&i| s.doc.layer_visible(i))
                        .collect();
                    if v.len() > 1 || s.doc.has_groups() {
                        v
                    } else {
                        vec![]
                    }
                }
                FlattenImage => (0..s.doc.layers.len()).collect(),
                _ => vec![],
            };
            for target in consumed {
                super::PixelAccess::Pixels.require(&s.doc, target)?;
                if s.doc.alpha_locked(target) {
                    return Err(ActionError::new(
                        "alpha_locked",
                        "Unlock layer transparency before merging these layers",
                    ));
                }
            }
            s.finish_stroke();
            s.doc
                .merge_layers(operation, i)
                .map_err(|e| ActionError::new("layer_merge", e))?;
            Ok(s.state())
        }
        AddLayer => {
            let parent = args.get("parent_id").and_then(Value::as_u64);
            if let Some(id) = parent.filter(|id| *id != 0) {
                s.doc
                    .group_layer_ids(id)
                    .map_err(|e| ActionError::new("invalid_group", e))?;
            }
            s.doc
                .preflight_new_layer(parent)
                .map_err(|e| ActionError::new("layer_limit", e))?;
            if s.doc.layers.len() >= MAX_LAYERS
                || s.doc.width * s.doc.height * (s.doc.layers.len() + 1) > 64 * 1024 * 1024
            {
                return Err(ActionError::new(
                    "layer_limit",
                    "Layer count or document pixel limit reached",
                ));
            }
            s.finish_stroke();
            let count = s.doc.layers.len();
            s.doc
                .try_add_layer_in(parent)
                .map_err(|e| ActionError::new("layer_limit", e))?;
            if s.doc.layers.len() == count {
                return Err(ActionError::new(
                    "layer_limit",
                    "Layer identity limit reached",
                ));
            }
            Ok(json!({"active_layer":s.doc.active,"layer_id":s.doc.layer_id(s.doc.active)}))
        }
        RenameLayer | DuplicateLayer | MoveLayer | DeleteLayer => {
            let i = index()?;
            s.finish_stroke();
            let result = match op {
                RenameLayer => s
                    .doc
                    .rename_layer(i, args["name"].as_str().unwrap().to_owned()),
                DuplicateLayer => s.doc.duplicate_layer(i),
                MoveLayer => s.doc.move_layer(i, args["to"].as_u64().unwrap() as usize),
                DeleteLayer => s.doc.delete_layer(i),
                _ => unreachable!(),
            };
            result.map_err(|e| ActionError::new("layer_edit", e))?;
            Ok(s.state())
        }
        SelectLayer | LayerProperties => {
            let i = index()?;
            if i >= s.doc.layers.len() {
                return Err(ActionError::new("invalid_layer", "Layer does not exist"));
            }
            if op == LayerProperties
                && ["pixel_locked", "position_locked", "color_label"]
                    .iter()
                    .any(|key| args.get(*key).is_some())
                && (s.stroke.active() || s.pointer_checkpoint.is_some())
            {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before changing layer organization",
                ));
            }
            s.finish_stroke();
            if op == SelectLayer {
                s.doc.select_layer(i);
            } else {
                let l = &s.doc.layers[i];
                let visible = args["visible"].as_bool().unwrap_or(l.visible);
                let opacity = args["opacity"]
                    .as_f64()
                    .map(|v| (v * 255.).round() as u8)
                    .unwrap_or(l.opacity);
                let mode = if args.get("blend_mode").is_some() {
                    serde_json::from_value(args["blend_mode"].clone())?
                } else {
                    l.mode
                };
                s.doc
                    .set_layer_properties_organized(
                        i,
                        visible,
                        opacity,
                        mode,
                        args["alpha_locked"]
                            .as_bool()
                            .unwrap_or(s.doc.alpha_locked(i)),
                        args["pixel_locked"]
                            .as_bool()
                            .unwrap_or(s.doc.pixel_locked(i)),
                        args["position_locked"]
                            .as_bool()
                            .unwrap_or(s.doc.position_locked(i)),
                        args.get("color_label")
                            .map(|v| serde_json::from_value(v.clone()))
                            .transpose()?
                            .unwrap_or(s.doc.color_label(i)),
                    )
                    .map_err(|e| ActionError::new("layer_properties", e))?;
            }
            Ok(s.state())
        }
        ClearLayer => {
            super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
            if s.doc.alpha_locked(s.doc.active) {
                return Err(ActionError::new(
                    "alpha_locked",
                    "Unlock layer transparency before clearing it",
                ));
            }
            s.finish_stroke();
            if s.doc.selection().current.is_some() {
                let (w, h) = (s.doc.width, s.doc.height);
                let mut pixels = vec![0; w * h];
                super::native::clip_region(s, 0, 0, w, h, &mut pixels)
                    .map_err(|e| ActionError::new("selection_clip", e))?;
                s.doc.replace_region(0, 0, w, h, &pixels);
            } else {
                s.doc.clear_layer();
            }
            Ok(s.state())
        }

        _ => Err(ActionError::new(
            "internal",
            "operation routed to wrong plugin",
        )),
    }
}
