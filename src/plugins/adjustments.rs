//! Metadata-only stack items and explicit shared per-source controllers. Registered image filters own the kernels.
use super::{
    nodes::{Binding, Resolved},
    Registry,
};
use crate::application::{ActionError, ActionResult, Operation, Session};
use crate::engine::{
    adjustments::{Adjustment, AdjustmentSpec, MAX_ADJUSTMENTS},
    curves::Curves,
    graph::Filter,
    hsl::Hsl,
    levels::Levels,
    quantization::Quantization,
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
        AdjustmentRead
            | AdjustmentSet
            | AdjustmentMove
            | AdjustmentDelete
            | AdjustmentInspect
            | AdjustmentPreview
    ) {
        properties["adjustment_id"] = id;
        required.push("adjustment_id");
    }
    if matches!(op, AdjustmentCreate | AdjustmentSet | AdjustmentPreview) {
        properties["scope"] = json!({"type":"string","enum":["affect_below","targets"],"description":"Omitted create uses affect-below; omitted set preserves the existing scope. targets applies independently to explicit source rasters in ascending controller-ID order. affect_below cannot be combined with target_layer_ids."});
        properties["target_layer_ids"] = json!({"type":"array","maxItems":16,"items":{"type":"integer","minimum":1,"maximum":4294967295u64},"description":"Exact unique existing raster IDs. Supplying an array selects targets scope; [] is an explicitly unbound controller and never affects the backdrop. Targeted controllers must remain at the root."});
        properties["name"] = json!({"type":"string","description":"Nonblank, single-line artist-owned name, at most 512 UTF-8 bytes."});
        properties["visible"] = json!({"type":"boolean"});
        properties["opacity"] = json!({"type":"number","minimum":0,"maximum":1});
        properties["node_type"] = json!({"type":"string","description":"Registered image Filter, Levels, Curves, HSL or quantization node ID; all parameters also obey that provider's schema."});
        properties["parameters"] = json!({"type":"object","properties":{
            "filter":{"type":"string","enum":Filter::ALL},
            "amount":{"type":"number","minimum":-1,"maximum":1}},"additionalProperties":false});
        let mut levels = Levels::schema();
        partial_objects(&mut levels);
        properties["parameters"]["properties"]["parameters"] = levels;
        let mut curves = Curves::schema();
        partial_objects(&mut curves);
        properties["parameters"]["properties"]["curves"] = curves;
        let mut hsl = Hsl::schema();
        partial_objects(&mut hsl);
        properties["parameters"]["properties"]["hsl"] = hsl;
        let mut quantization = Quantization::schema();
        partial_objects(&mut quantization);
        properties["parameters"]["properties"]["quantization"] = quantization;
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
    if op == AdjustmentPreview {
        properties["document_identity"] = json!({"type":"integer","minimum":1});
        properties["document_revision"] = json!({"type":"integer","minimum":0});
        required.extend(["document_identity", "document_revision"]);
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
    hsl: Option<Hsl>,
    quantization: Option<Quantization>,
}
impl ResolvedParameters {
    fn value(&self) -> Value {
        if let Some(quantization) = self.quantization {
            return quantization_parameters(quantization);
        }
        if let Some(hsl) = self.hsl {
            return hsl_parameters(hsl);
        }
        if let Some(curves) = &self.curves {
            return curve_parameters(curves);
        }
        self.levels
            .map_or_else(|| parameters(self.filter, self.amount), level_parameters)
    }
    fn title(&self) -> &'static str {
        if let Some(quantization) = self.quantization {
            return quantization.title();
        }
        if self.hsl.is_some() {
            return "Hue/Saturation";
        }
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
pub(crate) fn hsl_parameters(hsl: Hsl) -> Value {
    json!({"hsl":hsl})
}
pub(crate) fn quantization_parameters(value: Quantization) -> Value {
    json!({"quantization":value})
}
pub(crate) fn item_parameters(item: &Adjustment) -> Value {
    if let Some(quantization) = item.quantization {
        return quantization_parameters(quantization);
    }
    if let Some(hsl) = item.hsl {
        return hsl_parameters(hsl);
    }
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
    if !matches!(&node.binding, Binding::Image {operation,..} if operation == "filter" || operation == "levels" || operation == "curves" || operation == "hsl" || operation == "quantization")
    {
        return Err(error(
            "Adjustment nodes must bind a native scalar Filter, Levels, Curves, HSL or quantization kernel",
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
    // Optional channel intent starts from identity when first patched. Keep
    // old aliases unchanged; their provider schema still rejects this tail.
    if patch["parameters"]["channels"].is_object()
        && value["parameters"].is_object()
        && value["parameters"].get("channels").is_none()
        && def.parameters_schema["properties"]["parameters"]["properties"]
            .get("channels")
            .is_some()
    {
        value["parameters"]["channels"] = json!(crate::engine::levels::LevelsChannels::default());
    }
    if patch.is_object() {
        merge_patch(&mut value, patch);
    }
    // Validate the raw caller domain before rounding to the native f32 kernel.
    super::validate(&def.parameters_schema, &value)?;
    let typed = if matches!(&def.binding,Binding::Image {operation,..} if operation=="quantization")
    {
        let quantization: Quantization = serde_json::from_value(value["quantization"].clone())?;
        quantization.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: None,
            curves: None,
            hsl: None,
            quantization: Some(quantization),
        }
    } else if matches!(&def.binding,Binding::Image {operation,..} if operation=="hsl") {
        let hsl: Hsl = serde_json::from_value(value["hsl"].clone())?;
        hsl.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: None,
            curves: None,
            hsl: Some(hsl),
            quantization: None,
        }
    } else if matches!(&def.binding,Binding::Image {operation,..} if operation=="curves") {
        let curves: Curves = serde_json::from_value(value["curves"].clone())?;
        curves.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: None,
            curves: Some(curves),
            hsl: None,
            quantization: None,
        }
    } else if matches!(&def.binding,Binding::Image {operation,..} if operation=="levels") {
        let levels: Levels = serde_json::from_value(value["parameters"].clone())?;
        levels.validate().map_err(error)?;
        ResolvedParameters {
            filter: Filter::Invert,
            amount: 0.,
            levels: Some(levels),
            curves: None,
            hsl: None,
            quantization: None,
        }
    } else {
        let scalar: FilterParameters = serde_json::from_value(value)?;
        ResolvedParameters {
            filter: scalar.filter,
            amount: scalar.amount,
            levels: None,
            curves: None,
            hsl: None,
            quantization: None,
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
    if matches!(
        op,
        Operation::AdjustmentCreate | Operation::AdjustmentSet | Operation::AdjustmentPreview
    ) {
        bindings(args, None)?;
    }
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
    } else if matches!(op, Operation::AdjustmentSet | Operation::AdjustmentPreview) {
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
fn bindings(args: &Value, old: Option<&Adjustment>) -> Result<Option<Vec<u64>>, ActionError> {
    if args["scope"] == "affect_below" {
        if args.get("target_layer_ids").is_some() {
            return Err(error(
                "affect_below cannot be combined with target_layer_ids",
            ));
        }
        return Ok(None);
    }
    if let Some(value) = args.get("target_layer_ids") {
        let values = value
            .as_array()
            .ok_or_else(|| error("target_layer_ids requires an array"))?;
        if values.len() > 16 {
            return Err(error("Too many shared adjustment targets"));
        }
        let mut ids = Vec::with_capacity(values.len());
        for value in values {
            let id = super::bounded_integer(value)
                .filter(|id| *id > 0)
                .ok_or_else(|| error("Shared targets require positive raster IDs"))?;
            if ids.contains(&id) {
                return Err(error("Shared adjustment targets must be unique"));
            }
            ids.push(id);
        }
        return Ok(Some(ids));
    }
    if args["scope"] == "targets" {
        return Ok(Some(
            old.and_then(|item| item.target_layer_ids.clone())
                .unwrap_or_default(),
        ));
    }
    Ok(old.and_then(|item| item.target_layer_ids.clone()))
}
fn scope_state(item: &Adjustment) -> Value {
    json!({"scope":if item.target_layer_ids.is_some(){"targets"}else{"affect_below"},"target_layer_ids":item.target_layer_ids.as_deref().unwrap_or(&[]),"evaluation_order":if item.target_layer_ids.is_some(){"ascending_controller_id"}else{"stack_position"}})
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
    let mut state = json!({"id":item.id,"name":item.name,"visible":item.visible,"opacity":item.opacity as f64/255.,
        "node_type":item.node_type,"parameters":item_parameters(item),"parent_id":parent,"index":index,
        "mask_ids":mask_ids(&s.doc,item.id),"provider_available":definition(&s.registry,&item.node_type).is_ok()});
    for (key, value) in scope_state(item).as_object().unwrap() {
        state[key] = value.clone();
    }
    state
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

fn updated_spec(
    registry: &Registry,
    old: &Adjustment,
    args: &Value,
) -> Result<AdjustmentSpec, ActionError> {
    let node = args["node_type"].as_str().unwrap_or(&old.node_type);
    let parameters = resolve_parameters(registry, node, Some(old), &args["parameters"])?;
    Ok(AdjustmentSpec {
        name: args["name"].as_str().unwrap_or(&old.name).to_owned(),
        visible: args["visible"].as_bool().unwrap_or(old.visible),
        opacity: opacity(&args, old.opacity),
        node_type: node.to_owned(),
        filter: parameters.filter,
        amount: parameters.amount,
        levels: parameters.levels,
        curves: parameters.curves,
        hsl: parameters.hsl,
        quantization: parameters.quantization,
        target_layer_ids: bindings(&args, Some(old))?,
    })
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
    if op == AdjustmentPreview {
        let identity = args["document_identity"].as_u64();
        let revision = args["document_revision"].as_u64();
        if identity != Some(s.doc.identity) || revision != Some(s.revision) {
            return Err(ActionError::new(
                "revision_conflict",
                "Adjustment preview source changed; reload the draft",
            ));
        }
        let adjustment_id = id(&args);
        let old = s.doc.adjustment(adjustment_id).map_err(error)?;
        let spec = updated_spec(&s.registry, old, &args)?;
        let resolved = Adjustment::new(adjustment_id, spec.clone());
        let cost = s.doc.owner_preview_cost(&[]).map_err(error)?;
        let pixels = s
            .doc
            .preview_adjustment(adjustment_id, spec)
            .map_err(error)?;
        let bytes = crate::application::png_io::encode(
            s.doc.width,
            s.doc.height,
            pixels.into_iter(),
            s.doc.info().export_density(),
        )?;
        let mut apply_args = json!({"adjustment_id":adjustment_id,"name":resolved.name,"visible":resolved.visible,"opacity":resolved.opacity as f64/255.,"node_type":resolved.node_type,"parameters":item_parameters(&resolved),"scope":if resolved.target_layer_ids.is_some(){"targets"}else{"affect_below"}});
        if let Some(ids) = &resolved.target_layer_ids {
            apply_args["target_layer_ids"] = json!(ids);
        }
        let action = crate::application::Action {
            action: "adjustment.set".into(),
            args: apply_args,
            expected_revision: Some(s.revision),
        };
        let mut response = json!({"document_identity":s.doc.identity,"revision":s.revision,"adjustment_id":adjustment_id,"width":s.doc.width,"height":s.doc.height,"mimeType":"image/png","data":crate::application::codec::base64(&bytes),"cost":cost,"apply":action});
        for (key, value) in scope_state(&resolved).as_object().unwrap() {
            response[key] = value.clone();
        }
        return Ok(response);
    }
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
                hsl: parameters.hsl,
                quantization: parameters.quantization,
                target_layer_ids: bindings(&args, None)?,
            };
            let mut placement_args = args.clone();
            if spec.target_layer_ids.is_some() && args.get("parent_id").is_none() {
                placement_args["parent_id"] = json!(0);
            }
            let (parent, index) = placement(s, &placement_args)?;
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
            let spec = updated_spec(&s.registry, &old, &args)?;
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
    let mut result = result;
    if matches!(op, AdjustmentCreate | AdjustmentSet | AdjustmentMove) {
        let id = super::bounded_integer(&result["adjustment_id"]).unwrap();
        for (key, value) in scope_state(s.doc.adjustment(id).map_err(error)?)
            .as_object()
            .unwrap()
        {
            result[key] = value.clone();
        }
    }
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

#[cfg(test)]
#[path = "../application/shared_adjustment_tests.rs"]
mod shared_tests;
