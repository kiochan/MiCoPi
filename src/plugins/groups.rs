//! Explicit group structure and inspection share the document's cached composition.
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::{graph::BlendMode, groups::GroupItem};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn schema(op: Operation) -> Value {
    use Operation::*;
    if op == GroupQuery {
        return crate::application::layer_query::schema();
    }
    let id = json!({"type":"integer","minimum":1,"maximum":4294967295u64});
    let parent = json!({"type":"integer","minimum":0,"maximum":4294967295u64});
    let item = json!({"type":"object","properties":{"kind":{"type":"string","enum":["layer","group"]},"id":id},"required":["kind","id"],"additionalProperties":false});
    let name = json!({"type":"string","description":"An artist-owned, single-line name, at most 512 UTF-8 bytes."});
    let (properties, required) = match op {
        GroupCreate => (
            json!({"members":{"type":"array","maxItems":32,"items":item},"name":name,"parent_id":parent}),
            vec!["members"],
        ),
        GroupCreateSelected => (json!({"name":name}), vec![]),
        GroupProperties => (
            json!({"group_id":id,"name":name,"visible":{"type":"boolean"},"opacity":{"type":"number","minimum":0,"maximum":1},"blend_mode":{"type":"string","enum":BlendMode::ALL}}),
            vec!["group_id"],
        ),
        GroupMove => (
            json!({"items":{"type":"array","minItems":1,"maxItems":32,"items":item},"parent_id":parent,"index":{"type":"integer","minimum":0,"maximum":32}}),
            vec!["items", "parent_id", "index"],
        ),
        GroupMerge => {
            let mut target = id.clone();
            target["description"] = json!("Nonempty group to bake, including hidden descendants. The fresh raster retains this group's own name, visibility, opacity and blend mode; pixel/alpha-locked descendants reject the operation. Requires an idle contact.");
            (json!({"group_id":target}), vec!["group_id"])
        }
        GroupUngroupParent => (json!({}), vec![]),
        _ => (json!({"group_id":id}), vec!["group_id"]),
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn group_error(message: impl Into<String>) -> ActionError {
    ActionError::new("group_edit", message)
}
/// Literal validation cannot assume that earlier workflow steps already ran.
pub(crate) fn validate(op: Operation, args: &Value) -> Result<(), ActionError> {
    if op == Operation::GroupQuery {
        return crate::application::layer_query::validate(args);
    }
    if let Some(name) = args["name"].as_str() {
        if name.trim().is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
            return Err(group_error(
                "Group names must be nonblank, single-line and at most 512 UTF-8 bytes",
            ));
        }
    }
    for key in ["members", "items"] {
        if let Some(items) = args.get(key) {
            let items: Vec<GroupItem> = serde_json::from_value(items.clone())?;
            let unique: BTreeSet<_> = items.iter().copied().collect();
            if unique.len() != items.len() {
                return Err(group_error("Group entry references must be unique"));
            }
        }
    }
    Ok(())
}
fn require_idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before editing groups",
        ));
    }
    Ok(())
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    validate(op, &args)?;
    if op == GroupQuery {
        return query(s, &args);
    }
    require_idle(s)?;
    let id = args["group_id"].as_u64().unwrap_or(0);
    let name = args["name"].as_str().map(str::to_owned);
    let mut result_id = id;
    let mut merged_layer = None;
    match op {
        GroupCreate | GroupCreateSelected => {
            let members: Vec<GroupItem> = if op == GroupCreateSelected {
                s.doc
                    .selected_layer_ids()
                    .into_iter()
                    .map(|id| {
                        serde_json::from_value(json!({"kind":"layer","id":id}))
                            .expect("typed raster reference")
                    })
                    .collect()
            } else {
                serde_json::from_value(args["members"].clone())?
            };
            result_id = s
                .doc
                .group_create(&members, name, args["parent_id"].as_u64())
                .map_err(group_error)?;
        }
        GroupProperties => {
            let mode: Option<BlendMode> = args
                .get("blend_mode")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()?;
            s.doc
                .group_set(
                    id,
                    name,
                    args["visible"].as_bool(),
                    args["opacity"].as_f64().map(|v| (v * 255.).round() as u8),
                    mode,
                )
                .map_err(group_error)?;
        }
        GroupMove => {
            let items: Vec<GroupItem> = serde_json::from_value(args["items"].clone())?;
            s.doc
                .group_move(
                    &items,
                    args["parent_id"].as_u64().unwrap(),
                    args["index"].as_u64().unwrap() as usize,
                )
                .map_err(group_error)?;
        }
        GroupUngroup | GroupUngroupParent => {
            if op == GroupUngroupParent {
                result_id = s.doc.layer_parent_id(s.doc.active);
                if result_id == 0 {
                    return Err(group_error("The active raster layer has no parent group"));
                }
            }
            s.doc.group_ungroup(result_id).map_err(group_error)?;
        }
        GroupDelete => s.doc.group_delete(id).map_err(group_error)?,
        GroupDuplicate => result_id = s.doc.group_duplicate(id).map_err(group_error)?,
        GroupMerge => merged_layer = Some(s.doc.group_merge(id).map_err(group_error)?),
        GroupSelectLayers => {
            let ids = s.doc.group_layer_ids(id).map_err(group_error)?;
            if ids.is_empty() {
                return Err(group_error("An empty group has no raster layers to select"));
            }
            s.doc.set_layer_selection(&ids, None).map_err(group_error)?;
        }
        _ => return Err(group_error("Operation routed to the wrong group handler")),
    }
    if op != GroupSelectLayers {
        s.doc.compose();
        if let Some(error) = &s.doc.render_error {
            return Err(ActionError::new("group_render", error));
        }
    }
    let mut state = s.state();
    if result_id != 0 {
        state["group_id"] = json!(result_id);
    }
    if let Some(id) = merged_layer {
        state["layer_id"] = json!(id);
    }
    Ok(state)
}

/// Filtering retains hierarchy context and never changes collapse or selection state.
pub(crate) fn query(s: &Session, args: &Value) -> ActionResult {
    let layers = crate::application::layer_query::execute(s, args)?;
    let matched_layers: BTreeSet<u64> = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row["id"].as_u64())
        .collect();
    let mut state = s.doc.groups_state();
    let groups = state["groups"].as_array().cloned().unwrap_or_default();
    let parents: BTreeMap<u64, u64> = groups
        .iter()
        .filter_map(|row| Some((row["id"].as_u64()?, row["parent_id"].as_u64().unwrap_or(0))))
        .collect();
    let name = args["query"].as_str().unwrap_or("").to_lowercase();
    let filtered = args.as_object().is_some_and(|a| {
        a.iter()
            .any(|(k, v)| k != "query" || v.as_str().is_some_and(|q| !q.is_empty()))
    });
    let names_only = !name.is_empty()
        && args
            .as_object()
            .is_some_and(|a| a.keys().all(|k| k == "query"));
    let matched_groups: BTreeSet<u64> = if names_only {
        groups
            .iter()
            .filter(|row| {
                row["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&name)
            })
            .filter_map(|row| row["id"].as_u64())
            .collect()
    } else {
        BTreeSet::new()
    };
    let mut leaves = matched_layers.clone();
    let mut included = matched_groups.clone();
    for row in &groups {
        let id = row["id"].as_u64().unwrap();
        let mut parent = id;
        for _ in 0..=16 {
            if matched_groups.contains(&parent) {
                included.insert(id);
                if let Some(ids) = row["layer_ids"].as_array() {
                    leaves.extend(ids.iter().filter_map(Value::as_u64));
                }
                break;
            }
            parent = *parents.get(&parent).unwrap_or(&0);
            if parent == 0 {
                break;
            }
        }
    }
    let all = state["rows"].as_array().cloned().unwrap_or_default();
    for row in &all {
        if (row["kind"] == "layer" && leaves.contains(&row["id"].as_u64().unwrap_or(0)))
            || (row["kind"] == "group" && included.contains(&row["id"].as_u64().unwrap_or(0)))
        {
            let mut parent = row["parent_id"].as_u64().unwrap_or(0);
            for _ in 0..16 {
                if parent == 0 {
                    break;
                }
                included.insert(parent);
                parent = *parents.get(&parent).unwrap_or(&0);
            }
        }
    }
    let rows: Vec<_> = all
        .iter()
        .filter_map(|row| {
            let id = row["id"].as_u64().unwrap_or(0);
            let group = row["kind"] == "group";
            if filtered
                && !(if group {
                    included.contains(&id)
                } else {
                    leaves.contains(&id)
                })
            {
                return None;
            }
            let mut row = row.clone();
            let matched = !filtered
                || if group {
                    matched_groups.contains(&id)
                } else {
                    matched_layers.contains(&id)
                };
            row["matched"] = json!(matched);
            row["context"] = json!(!matched);
            Some(row)
        })
        .collect();
    state["total"] = json!(all.len());
    state["matched"] = json!(rows.iter().filter(|r| r["matched"] == true).count());
    state["rows"] = json!(rows);
    state["filter_active"] = json!(filtered);
    state["matching_layer_ids"] = json!(matched_layers);
    state["matching_group_ids"] = json!(matched_groups);
    state["revision"] = json!(s.revision);
    state["drawing"] = json!(s.is_drawing());
    state["selected_layer_ids"] = layers["selected_layer_ids"].clone();
    state["active_layer_id"] = layers["active_layer_id"].clone();
    state["renderer_transparent"] = json!(s
        .doc
        .renderer
        .as_ref()
        .is_some_and(|r| r.supports_transparent()));
    Ok(state)
}
