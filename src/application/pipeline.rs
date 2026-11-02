//! Discoverable, transport-independent orchestration. Pointer hot paths stay typed.
use super::{Action, ActionError, ActionResult, Operation, Session};
use crate::plugins::{valid_id, validate};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const RESOURCES: &[(&str, &str)] = &[
    ("document.workflow", "Document plugin workflow graph"),
    (
        "document.nodes",
        "Non-destructive image graph, default layer graph and compositor state",
    ),
    (
        "document.active",
        "Active canvas, layers, source file and save location",
    ),
    (
        "workspace.documents",
        "Open documents; this version supports one document per session",
    ),
    (
        "painting.tool",
        "Selected tool, pressure, stabilizer parameters and view",
    ),
    ("input.pointer", "Latest input and pen support information"),
    (
        "plugins.registry",
        "Plugins, tools, actions and loading diagnostics",
    ),
    ("workspace.ui", "UI state and runtime mode"),
];
pub fn describe(s: &Session) -> Value {
    let mut resources: Vec<Value> = RESOURCES
        .iter()
        .map(|(id, title)| {
            json!({
                "id":id,"title":title,"owner":"core","uri":format!("micopi://state/{id}"),
            "outputSchema":resource_schema(id)
            })
        })
        .collect();
    resources.extend(s.registry.states.values().map(|r| {
        json!({
            "id":r.id,"title":r.title,"action":r.action,"owner":s.registry.loaded.iter().filter(|id|r.id.starts_with(&format!("{id}."))).max_by_key(|id|id.len()),
            "uri":format!("micopi://state/{}",r.id),"outputSchema":r.output_schema
        })
    }));
    json!({"pipeline_version":1,"version":env!("CARGO_PKG_VERSION"),"runtime":s.runtime,
        "revision":s.revision,"document_model":"single_document_session",
        "entrypoints":{"discover":"app.describe","query":"context.read","execute":"pipeline.run","node_catalog":"node.catalog","workflow_read":"workflow.read","workflow_edit":"workflow.patch","workflow_execute":"workflow.run","events":"events.list","image":"canvas.snapshot"},
        "actions":s.registry.actions.values().map(|a|json!({"id":a.id,"title":a.title,
            "owner":s.registry.owners.get(&a.id),"inputSchema":a.schema(),"outputSchema":a.output_schema,
            "readOnly":a.is_read_only()})).collect::<Vec<_>>(),
        "resources":resources,"limits":{"steps":128,"nested_calls":16,"action_budget":1024,"events":512},
        "execution":{"order":"sequential","transactional":false,"stop_on_error":true,
            "reference":{"$ref":"step-id#/result/field"},"dry_run":"validate_only_no_plugin_execution",
            "dynamic_arguments":"validated_after_reference_resolution","expected_revision":true},
        "plugin_sdk":{"action_abi":1,"tool_abi":1,"descriptor_states":true,"cross_plugin_calls":true,
            "documentation":"docs/ai-pipeline.md","example":"plugins/ai-assistant"}})
}
pub(crate) fn resource_schema(id: &str) -> Value {
    let document = json!({"type":"object","properties":{
        "id":{"type":"string"},"name":{"description":"File name or null for untitled canvas"},
        "source_path":{"description":"Imported/opened source path or null"},"save_path":{"description":"Project save target or null"},
        "width":{"type":"integer"},"height":{"type":"integer"},"dirty":{"type":"boolean"},
        "active_layer":{"type":"integer"},"layers":{"type":"array","items":{"type":"object"}}
    },"required":["id","source_path","save_path","width","height","dirty","layers"]});
    match id {
        "document.active" => document.clone(),
        "workspace.documents" => {
            json!({"type":"object","properties":{"active":{"type":"string"},"documents":{"type":"array","items":document},"multiple_documents":{"type":"boolean"}},"required":["active","documents","multiple_documents"]})
        }
        "painting.tool" => {
            json!({"type":"object","properties":{"selected":{"type":"string"},"kind":{"type":"string"},"parameters":{"type":"object"},"view":{"type":"object"},"drawing":{"type":"boolean"}},"required":["selected","kind","parameters","view","drawing"]})
        }
        "input.pointer" => {
            json!({"type":"object","properties":{"last":{"description":"Last accepted Pointer or null"},"pen_observed":{"type":"boolean"},"windows_ink_supported":{"type":"boolean"},"device_enumeration":{"type":"boolean"}},"required":["last","pen_observed","windows_ink_supported","device_enumeration"]})
        }
        _ => json!({"type":"object"}),
    }
}
fn document(s: &Session) -> Value {
    let path = s.file.as_ref().or(s.source_file.as_ref());
    json!({"id":"active","name":path.and_then(|p|p.file_name()).map(|p|p.to_string_lossy()),
        "source_path":s.source_file,"save_path":s.file,"dirty":s.doc.dirty,
        "width":s.doc.width,"height":s.doc.height,"active_layer":s.doc.active,
        "layers":s.state()["layers"],"snapshot_action":"canvas.snapshot"})
}
pub fn context(s: &mut Session, args: Value) -> ActionResult {
    let ids: Vec<String> = match args.get("resources") {
        Some(v) => serde_json::from_value(v.clone())?,
        None => RESOURCES.iter().map(|(id, _)| id.to_string()).collect(),
    };
    let mut resources = BTreeMap::new();
    for id in ids {
        let value = match id.as_str() {
            "document.workflow" => s
                .apply(Action::new("workflow.read", json!({})))
                .map(|r| r["result"].clone()),
            "document.nodes" => s
                .apply(Action::new("graph.read", json!({})))
                .map(|r| r["result"].clone()),
            "document.active" => Ok(document(s)),
            "workspace.documents" => {
                Ok(json!({"active":"active","documents":[document(s)],"multiple_documents":false}))
            }
            "painting.tool" => Ok(
                json!({"selected":s.selected_tool,"kind":s.tool,"parameters":s.parameters,"view":s.view,"drawing":s.is_drawing()}),
            ),
            "input.pointer" => Ok(json!({"last":s.last_pointer,"pen_observed":s.pen_observed,
                "windows_ink_supported":cfg!(windows),"device_enumeration":false,
                "note":"last is the most recently received sample, not a device list or current hardware pressure"})),
            "plugins.registry" => Ok(s.registry.describe()),
            "workspace.ui" => Ok(json!({"runtime":s.runtime,"state":s.ui})),
            _ => read_plugin_state(s, &id),
        };
        resources.insert(
            id,
            match value {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"error":error}),
            },
        );
    }
    Ok(json!({"revision":s.revision,"resources":resources}))
}
fn read_plugin_state(s: &mut Session, id: &str) -> ActionResult {
    let provider = s
        .registry
        .states
        .get(id)
        .cloned()
        .ok_or_else(|| ActionError::new("unknown_resource", id))?;
    // Also protect direct callers of context(), beyond the outer read-only action.
    s.read_depth += 1;
    let outcome = s.apply(Action::new(provider.action, provider.args));
    s.read_depth -= 1;
    let output = outcome?["result"].clone();
    validate(&provider.output_schema, &output)
        .map_err(|e| ActionError::new("invalid_state_output", e.to_string()))?;
    Ok(output)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    action: String,
    #[serde(default = "super::actions_empty_object")]
    args: Value,
    #[serde(default)]
    output_schema: Option<Value>,
}
// Nested steps have their own result-reference scope; options may use parent refs.
fn local_arguments(s: &Session, step: &Step) -> (Value, Option<Value>) {
    let mut args = step.args.clone();
    let nested = if s.registry.actions[&step.action].operation == Some(Operation::PipelineRun) {
        args.as_object_mut().and_then(|o| o.remove("steps"))
    } else {
        None
    };
    (args, nested)
}
fn reference(value: &Value) -> Result<Option<(&str, &str)>, ActionError> {
    if let Some(map) = value.as_object() {
        if let Some(v) = map.get("$ref") {
            if map.len() != 1 {
                return Err(ActionError::new(
                    "invalid_reference",
                    "$ref must be the only field in its object",
                ));
            }
            let (id, path) = v.as_str().and_then(|r| r.split_once('#')).ok_or_else(|| {
                ActionError::new("invalid_reference", "Use step-id#/result/field")
            })?;
            if !valid_id(id) || (!path.is_empty() && !path.starts_with('/')) {
                return Err(ActionError::new(
                    "invalid_reference",
                    "Invalid step ID or JSON Pointer",
                ));
            }
            return Ok(Some((id, path)));
        }
    }
    Ok(None)
}
fn inspect_refs(value: &Value, seen: &BTreeSet<String>, depth: usize) -> Result<bool, ActionError> {
    if depth > 64 {
        return Err(ActionError::new(
            "pipeline_limit",
            "Arguments are nested too deeply",
        ));
    }
    if let Some((id, _)) = reference(value)? {
        if !seen.contains(id) {
            return Err(ActionError::new(
                "invalid_reference",
                format!("Cannot reference future or missing step {id}"),
            ));
        }
        return Ok(true);
    }
    let mut dynamic = false;
    match value {
        Value::Object(m) => {
            for v in m.values() {
                dynamic |= inspect_refs(v, seen, depth + 1)?;
            }
        }
        Value::Array(a) => {
            for v in a {
                dynamic |= inspect_refs(v, seen, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(dynamic)
}
fn resolve(value: &Value, results: &BTreeMap<String, Value>) -> Result<Value, ActionError> {
    if let Some((id, path)) = reference(value)? {
        return results
            .get(id)
            .and_then(|r| r.pointer(path))
            .cloned()
            .ok_or_else(|| {
                ActionError::new(
                    "missing_reference",
                    format!("Result {id}#{path} does not exist"),
                )
            });
    }
    Ok(match value {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), resolve(v, results)?)))
                .collect::<Result<_, ActionError>>()?,
        ),
        Value::Array(a) => Value::Array(
            a.iter()
                .map(|v| resolve(v, results))
                .collect::<Result<_, _>>()?,
        ),
        _ => value.clone(),
    })
}
pub fn run(s: &mut Session, args: Value) -> ActionResult {
    if let Some(revision) = args.get("expected_revision") {
        let revision = revision.as_u64().ok_or_else(|| {
            ActionError::new(
                "invalid_arguments",
                "expected_revision must be a u64 integer",
            )
        })?;
        if revision != s.revision {
            return Err(ActionError::new(
                "revision_conflict",
                format!("Current revision is {}", s.revision),
            ));
        }
    }
    let steps: Vec<Step> = serde_json::from_value(args["steps"].clone())?;
    if steps.is_empty() || steps.len() > 128 {
        return Err(ActionError::new(
            "pipeline_limit",
            "A pipeline requires 1..128 steps",
        ));
    }
    let mut seen = BTreeSet::new();
    let mut deferred = vec![];
    for step in &steps {
        if let Some(schema) = &step.output_schema {
            crate::plugins::registry::check_schema(schema)
                .map_err(|e| ActionError::new("invalid_output_schema", e))?;
        }
        if !valid_id(&step.id) || seen.contains(&step.id) {
            return Err(ActionError::new(
                "invalid_step",
                "Invalid or duplicate step ID",
            ));
        }
        if !s.registry.actions.contains_key(&step.action) {
            return Err(ActionError::new("unknown_action", &step.action));
        }
        let (bindings, _) = local_arguments(s, step);
        let dynamic = inspect_refs(&bindings, &seen, 0)?;
        if !dynamic {
            s.validate_action(&Action::new(&step.action, step.args.clone()))?;
        }
        if dynamic {
            deferred.push(step.id.clone());
        }
        seen.insert(step.id.clone());
    }
    if args["dry_run"].as_bool() == Some(true) {
        return Ok(
            json!({"ok":true,"dry_run":true,"steps":steps.len(),"deferred_validation":deferred,
            "revision":s.revision,"transactional":false}),
        );
    }
    let started = s.revision;
    let mut results = BTreeMap::new();
    let mut completed = vec![];
    for step in steps {
        let (bindings, nested_steps) = local_arguments(s, &step);
        let outcome = resolve(&bindings, &results)
            .map(|mut args| {
                if let Some(steps) = nested_steps {
                    args["steps"] = steps;
                }
                args
            })
            .and_then(|args| s.apply(Action::new(&step.action, args)));
        match outcome {
            Ok(result)
                if step
                    .output_schema
                    .iter()
                    .chain(s.registry.actions[&step.action].output_schema.iter())
                    .any(|schema| validate(schema, &result["result"]).is_err()) =>
            {
                return Ok(
                    json!({"ok":false,"failed_step":step.id,"action":step.action,"error":{"code":"invalid_node_output","message":"Action result does not match the output schema"},"failed_result":result,"completed":completed,"results":results,"start_revision":started,"revision":s.revision,"transactional":false}),
                );
            }
            Ok(result) if result["result"]["ok"] == false => {
                return Ok(
                    json!({"ok":false,"failed_step":step.id,"action":step.action,
                "error":{"code":"nested_pipeline_failed","message":"Nested action failed; inspect failed_result"},"failed_result":result,
                "completed":completed,"results":results,"start_revision":started,"revision":s.revision,"transactional":false}),
                )
            }
            Ok(result) => {
                results.insert(step.id.clone(), result);
                completed.push(step.id);
            }
            Err(error) => {
                return Ok(
                    json!({"ok":false,"failed_step":step.id,"action":step.action,
                "error":error,"completed":completed,"results":results,"start_revision":started,
                "revision":s.revision,"transactional":false}),
                )
            }
        }
    }
    Ok(
        json!({"ok":true,"completed":completed,"results":results,"start_revision":started,
        "revision":s.revision,"transactional":false}),
    )
}
