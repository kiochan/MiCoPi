//! Metadata-only affect-below items. Registered image filters own the kernels.
use super::{
    nodes::{Binding, Resolved},
    Registry,
};
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::{
    adjustments::{Adjustment, AdjustmentSpec, MAX_ADJUSTMENTS},
    curves::Curves,
    graph::Filter,
    levels::Levels,
    raster_masks::{MaskOwner, SamplePlane},
    Document,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("adjustment", message)
}

pub(crate) fn schema(op: Operation) -> Value {
    use Operation::*;
    let id = json!({"type":"integer","minimum":1,"maximum":4294967295u64});
    let mut properties = json!({});
    let mut required = vec![];
    if matches!(
        op,
        AdjustmentRead | AdjustmentSet | AdjustmentMove | AdjustmentDelete | AdjustmentInspect
    ) {
        properties["adjustment_id"] = id;
        required.push("adjustment_id");
    }
    if matches!(op, AdjustmentCreate | AdjustmentSet) {
        properties["name"] = json!({"type":"string","description":"Nonblank, single-line artist-owned name, at most 512 UTF-8 bytes."});
        properties["visible"] = json!({"type":"boolean"});
        properties["opacity"] = json!({"type":"number","minimum":0,"maximum":1});
        properties["node_type"] = json!({"type":"string","description":"Registered image Filter, Levels or Curves node ID; all parameters also obey that provider's schema."});
        properties["parameters"] = json!({"type":"object","properties":{
            "filter":{"type":"string","enum":Filter::ALL},
            "amount":{"type":"number","minimum":-1,"maximum":1}},"additionalProperties":false});
        let mut levels = Levels::schema();
        levels.as_object_mut().unwrap().remove("required");
        properties["parameters"]["properties"]["parameters"] = levels;
        let mut curves = Curves::schema();
        partial_objects(&mut curves);
        properties["parameters"]["properties"]["curves"] = curves;
    }
    if matches!(op, AdjustmentCreate | AdjustmentMove) {
        properties["parent_id"] = json!({"type":"integer","minimum":0,"maximum":4294967295u64});
        properties["index"] = json!({"type":"integer","minimum":0,"maximum":64,"description":"Bottom-to-top child position; Move counts remaining children after removing the source."});
    }
    if op == AdjustmentCreate {
        required.push("node_type");
        properties["mask_source"] = json!({"type":"string","enum":["selection"],"description":"Copy the current pixel selection once into a real A8 mask; omission creates no mask."});
    }
    if op == AdjustmentMove {
        required.extend(["parent_id", "index"]);
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilterParameters {
    filter: Filter,
    amount: f32,
}
struct ResolvedParameters {
    filter: Filter,
    amount: f32,
    levels: Option<Levels>,
    curves: Option<Curves>,
}
impl ResolvedParameters {
    fn value(&self) -> Value {
        if let Some(curves) = &self.curves {
            return curve_parameters(curves);
        }
        self.levels
            .map_or_else(|| parameters(self.filter, self.amount), level_parameters)
    }
    fn title(&self) -> &'static str {
        if self.curves.is_some() {
            "Curves"
        } else if self.levels.is_some() {
            "Levels"
        } else {
            self.filter.title()
        }
    }
}
pub(crate) fn level_parameters(parameters: Levels) -> Value {
    json!({"parameters":parameters})
}
pub(crate) fn curve_parameters(curves: &Curves) -> Value {
    json!({"curves":curves})
}
pub(crate) fn item_parameters(item: &Adjustment) -> Value {
    if let Some(curves) = &item.curves {
        return curve_parameters(curves);
    }
    item.levels
        .map_or_else(|| parameters(item.filter, item.amount), level_parameters)
}

/// Round-trip the typed serializer's shortest f32 decimal. Widening an f32 into
/// Value directly would reject valid provider bounds such as amount <= 0.1.
pub(crate) fn parameters(filter: Filter, amount: f32) -> Value {
    serde_json::from_str(&serde_json::to_string(&FilterParameters { filter, amount }).unwrap())
        .unwrap()
}

fn definition(registry: &Registry, id: &str) -> Result<Resolved, ActionError> {
    let (owner, node) = registry
        .nodes
        .get(id)
        .ok_or_else(|| error(format!("Registered adjustment filter is unavailable: {id}")))?;
    if !matches!(&node.binding, Binding::Image {operation,..} if operation == "filter" || operation == "levels" || operation == "curves")
    {
        return Err(error(
            "Adjustment nodes must bind a native scalar Filter, Levels or Curves kernel",
        ));
    }
    node.resolve(registry, owner).map_err(error)
}

fn merge_patch(target: &mut Value, patch: &Value) {
    if let (Some(target), Some(patch)) = (target.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            if let Some(old) = target.get_mut(key) {
                if old.is_object() && value.is_object() {
                    merge_patch(old, value);
                } else {
                    *old = value.clone();
                }
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}
fn partial_objects(schema: &mut Value) {
    if schema["type"] == "object" {
        schema.as_object_mut().unwrap().remove("required");
        if let Some(properties) = schema["properties"].as_object_mut() {
            for value in properties.values_mut() {
                partial_objects(value);
            }
        }
    }
}

fn resolve_parameters(
    registry: &Registry,
    id: &str,
    base: Option<&Adjustment>,
    patch: &Value,
) -> Result<ResolvedParameters, ActionError> {
    let def = definition(registry, id)?;
    let mut value = base
        .filter(|old| old.node_type == id)
        .map_or(def.defaults.clone(), item_parameters);
    if patch.is_object() {
        merge_patch(&mut value, patch);
    }
    // Validate the raw caller domain before rounding to the native f32 kernel.
    super::validate(&def.parameters_schema, &value)?;
    let typed = if matches!(&def.binding,Binding::Image {operation,..} if operation=="curves") {
        let curves: Curves = serde_json::from_value(value["curves"].clone())?;
        curves.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: None,
            curves: Some(curves),
        }
    } else if matches!(&def.binding,Binding::Image {operation,..} if operation=="levels") {
        let levels: Levels = serde_json::from_value(value["parameters"].clone())?;
        levels.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: Some(levels),
            curves: None,
        }
    } else {
        let scalar: FilterParameters = serde_json::from_value(value)?;
        ResolvedParameters {
            filter: scalar.filter,
            amount: scalar.amount,
            levels: None,
            curves: None,
        }
    };
    if !typed.amount.is_finite() || !(typed.filter.minimum()..=1.).contains(&typed.amount) {
        return Err(error("Filter amount is outside its native kernel range"));
    }
    // The persisted typed value must also remain valid under the provider.
    super::validate(&def.parameters_schema, &typed.value())?;
    Ok(typed)
}

/// Validate statically known literals without resolving document IDs that an
/// earlier pipeline step may create. Runtime validation remains authoritative.
pub(crate) fn validate_literal(
    registry: &Registry,
    op: Operation,
    args: &Value,
) -> Result<(), ActionError> {
    if let Some(name) = args["name"].as_str() {
        if name.trim().is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
            return Err(error(
                "Adjustment names must be nonblank, single-line and at most 512 UTF-8 bytes",
            ));
        }
    }
    if op == Operation::AdjustmentCreate {
        resolve_parameters(
            registry,
            args["node_type"].as_str().unwrap(),
            None,
            &args["parameters"],
        )?;
    } else if op == Operation::AdjustmentSet {
        if let Some(id) = args["node_type"].as_str() {
            let def = definition(registry, id)?;
            let mut partial = def.parameters_schema.clone();
            partial_objects(&mut partial);
            if args["parameters"].is_object() {
                super::validate(&partial, &args["parameters"])?;
            }
        }
    }
    Ok(())
}

/// Cold publication/contact-start guard; never called for typed pointer MOVE.
pub(crate) fn validate_document(registry: &Registry, doc: &Document) -> Result<(), ActionError> {
    for item in doc.adjustments() {
        item.validate().map_err(error)?;
        let def = definition(registry, &item.node_type)?;
        super::validate(&def.parameters_schema, &item_parameters(item))?;
    }
    Ok(())
}

fn mask_ids(doc: &Document, id: u64) -> Vec<u64> {
    doc.mask_state()
        .items
        .iter()
        .filter(|mask| mask.owner == (MaskOwner::Adjustment { id }))
        .map(|mask| mask.id)
        .collect()
}
fn item_state(s: &Session, item: &Adjustment) -> Value {
    let tree = s.doc.groups_state();
    let parent = s.doc.adjustment_parent_id(item.id).unwrap_or(0);
    let children = if parent == 0 {
        tree["roots"].as_array()
    } else {
        tree["groups"]
            .as_array()
            .and_then(|groups| groups.iter().find(|group| group["id"] == parent))
            .and_then(|group| group["children"].as_array())
    };
    let index = children
        .and_then(|children| {
            children
                .iter()
                .position(|child| child["kind"] == "adjustment" && child["id"] == item.id)
        })
        .unwrap_or(0);
    json!({"id":item.id,"name":item.name,"visible":item.visible,"opacity":item.opacity as f64/255.,
        "node_type":item.node_type,"parameters":item_parameters(item),"parent_id":parent,"index":index,
        "mask_ids":mask_ids(&s.doc,item.id),"provider_available":definition(&s.registry,&item.node_type).is_ok()})
}
fn idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before editing adjustments",
        ));
    }
    Ok(())
}
fn id(args: &Value) -> u64 {
    super::bounded_integer(&args["adjustment_id"]).unwrap()
}

pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    if op == AdjustmentCatalog {
        let items = s.registry.nodes.keys().filter_map(|id| definition(&s.registry, id).ok()).map(|def|
            json!({"node_type":def.id,"owner":def.owner,"title":def.title,"parameters_schema":def.parameters_schema,"defaults":def.defaults})).collect::<Vec<_>>();
        return Ok(json!({"items":items,"revision":s.revision}));
    }
    if op == AdjustmentQuery {
        return Ok(
            json!({"items":s.doc.adjustments().iter().map(|item|item_state(s,item)).collect::<Vec<_>>(),"limits":{"count":MAX_ADJUSTMENTS},"revision":s.revision}),
        );
    }
    if op == AdjustmentRead {
        let mut result = item_state(s, s.doc.adjustment(id(&args)).map_err(error)?);
        result["revision"] = json!(s.revision);
        return Ok(result);
    }
    if op == AdjustmentInspect {
        if s.runtime != "desktop" {
            return Err(ActionError::new("ui_unavailable", "Adjustment inspection requires the desktop; use adjustment.get in headless clients"));
        }
        let id = id(&args);
        s.doc.adjustment(id).map_err(error)?;
        if s.ui_effects.len() >= 128 {
            return Err(ActionError::new("ui_queue_full", "UI effect queue is full"));
        }
        s.ui_effects.push_back(
            json!({"id":"adjustment.inspect","value":id,"document_identity":s.doc.identity}),
        );
        return Ok(json!({"queued":true,"adjustment_id":id}));
    }
    idle(s)?;
    validate_document(&s.registry, &s.doc)?;
    let result = match op {
        AdjustmentCreate => {
            let node = args["node_type"].as_str().unwrap();
            let parameters = resolve_parameters(&s.registry, node, None, &args["parameters"])?;
            let spec = AdjustmentSpec {
                name: args["name"]
                    .as_str()
                    .unwrap_or(parameters.title())
                    .to_owned(),
                visible: args["visible"].as_bool().unwrap_or(true),
                opacity: opacity(&args, 255),
                node_type: node.to_owned(),
                filter: parameters.filter,
                amount: parameters.amount,
                levels: parameters.levels,
                curves: parameters.curves,
            };
            let (parent, index) = placement(s, &args)?;
            let with_mask = args["mask_source"] == "selection";
            s.doc
                .preflight_create_adjustment(&spec, parent, index, with_mask)
                .map_err(error)?;
            let mask = if with_mask {
                let selection = s.doc.selection().current.as_ref().ok_or_else(|| {
                    ActionError::new(
                        "selection_empty",
                        "An active pixel selection is required to create an adjustment mask",
                    )
                })?;
                Some(SamplePlane::Dense(selection.coverage().to_vec()))
            } else {
                None
            };
            let (id, mask) = s
                .doc
                .create_adjustment(spec, parent, index, mask)
                .map_err(error)?;
            Ok(
                json!({"adjustment_id":id,"mask_ids":mask.into_iter().collect::<Vec<_>>(),"revision":s.revision}),
            )
        }
        AdjustmentSet => {
            let id = id(&args);
            let old = s.doc.adjustment(id).map_err(error)?.clone();
            let node = args["node_type"].as_str().unwrap_or(&old.node_type);
            let parameters =
                resolve_parameters(&s.registry, node, Some(&old), &args["parameters"])?;
            let spec = AdjustmentSpec {
                name: args["name"].as_str().unwrap_or(&old.name).to_owned(),
                visible: args["visible"].as_bool().unwrap_or(old.visible),
                opacity: opacity(&args, old.opacity),
                node_type: node.to_owned(),
                filter: parameters.filter,
                amount: parameters.amount,
                levels: parameters.levels,
                curves: parameters.curves,
            };
            let changed = s.doc.set_adjustment(id, spec).map_err(error)?;
            Ok(
                json!({"adjustment_id":id,"mask_ids":mask_ids(&s.doc,id),"changed":changed,"revision":s.revision}),
            )
        }
        AdjustmentMove => {
            let id = id(&args);
            let changed = s
                .doc
                .move_adjustment(
                    id,
                    super::bounded_integer(&args["parent_id"]).unwrap(),
                    args["index"].as_f64().unwrap() as usize,
                )
                .map_err(error)?;
            Ok(
                json!({"adjustment_id":id,"mask_ids":mask_ids(&s.doc,id),"changed":changed,"revision":s.revision}),
            )
        }
        AdjustmentDelete => {
            let id = id(&args);
            let mask = s.doc.delete_adjustment(id).map_err(error)?;
            Ok(
                json!({"deleted_adjustment_ids":[id],"deleted_mask_ids":mask.into_iter().collect::<Vec<_>>(),"revision":s.revision}),
            )
        }
        _ => Err(error("Operation routed to the wrong handler")),
    }?;
    s.doc.compose();
    if let Some(error) = &s.doc.render_error {
        return Err(ActionError::new("adjustment_render", error));
    }
    Ok(result)
}
fn opacity(args: &Value, default: u8) -> u8 {
    args["opacity"]
        .as_f64()
        .map_or(default, |value| (value * 255.).round() as u8)
}
fn placement(s: &Session, args: &Value) -> Result<(u64, usize), ActionError> {
    let state = s.doc.groups_state();
    let active = s.doc.layer_id(s.doc.active);
    let row = state["rows"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| row["kind"] == "layer" && row["id"] == active)
    });
    let parent = args
        .get("parent_id")
        .map(super::bounded_integer)
        .flatten()
        .unwrap_or_else(|| row.and_then(|row| row["parent_id"].as_u64()).unwrap_or(0));
    let children = if parent == 0 {
        state["roots"].as_array()
    } else {
        state["groups"]
            .as_array()
            .and_then(|groups| groups.iter().find(|group| group["id"] == parent))
            .and_then(|group| group["children"].as_array())
    }
    .ok_or_else(|| error("Adjustment parent group does not exist"))?;
    let index = args["index"]
        .as_f64()
        .map(|n| n as usize)
        .unwrap_or_else(|| {
            if args.get("parent_id").is_some() {
                children.len()
            } else {
                children
                    .iter()
                    .position(|item| item["kind"] == "layer" && item["id"] == active)
                    .map_or(children.len(), |i| i + 1)
            }
        });
    Ok((parent, index))
}
