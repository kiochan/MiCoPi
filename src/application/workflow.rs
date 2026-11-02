//! Schema-driven node graphs compile to the existing pipeline, never a second executor.
use super::{Action, ActionError, ActionResult, Session};
use crate::{
    engine::workflow::{Graph, Patch},
    plugins::{
        nodes::{compatible, Binding, Resolved},
        validate,
    },
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("invalid_workflow", message)
}
fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn inspect_arguments(schema: &Value, value: &Value) -> Result<(), ActionError> {
    if value
        .as_object()
        .is_some_and(|m| m.len() == 1 && m.contains_key("$ref"))
    {
        return Ok(());
    }
    let mut partial = schema.clone();
    if let Some(map) = value.as_object() {
        let mut literal = map.clone();
        for (key, v) in map {
            if let Some(field) = schema["properties"].get(key) {
                inspect_arguments(field, v)?;
            }
            if has_reference(v) {
                literal.remove(key);
            }
        }
        if let Some(required) = schema["required"].as_array() {
            for key in required {
                if let Some(key) = key.as_str() {
                    if !map.contains_key(key) {
                        return Err(error(format!("Missing argument {key}")));
                    }
                }
            }
        }
        partial.as_object_mut().unwrap().remove("required");
        validate(&partial, &Value::Object(literal))?;
    } else {
        validate(schema, value)?;
    }
    Ok(())
}
fn has_reference(value: &Value) -> bool {
    match value {
        Value::Object(m) => m.contains_key("$ref") || m.values().any(has_reference),
        Value::Array(a) => a.iter().any(has_reference),
        _ => false,
    }
}
pub struct Plan {
    pub steps: Value,
    pub order: Vec<u32>,
    pub deferred: Vec<String>,
}
pub fn compile(s: &Session, graph: &Graph) -> Result<Plan, ActionError> {
    graph.validate_structure()?;
    let catalog = s.registry.node_catalog();
    let mut nodes = BTreeMap::new();
    let mut defs = BTreeMap::<u32, &Resolved>::new();
    for node in &graph.nodes {
        let def = catalog
            .get(&node.node_type)
            .ok_or_else(|| ActionError::new("missing_node_type", &node.node_type))?;
        if def.domain != "workflow" {
            return Err(error("Image nodes cannot connect to JSON workflows"));
        }
        if has_reference(&node.parameters) {
            return Err(error(
                "$ref is reserved in node parameters; use edges for dependencies",
            ));
        }
        nodes.insert(node.id, node);
        defs.insert(node.id, def);
    }
    let mut indegree: BTreeMap<_, usize> = nodes.keys().map(|id| (*id, 0)).collect();
    let mut targets = BTreeSet::new();
    for edge in &graph.edges {
        let from = defs[&edge.from]
            .outputs
            .iter()
            .find(|p| p.id == edge.output)
            .ok_or_else(|| error(format!("Unknown output port {}", edge.output)))?;
        let to = defs[&edge.to]
            .inputs
            .iter()
            .find(|p| p.id == edge.input)
            .ok_or_else(|| error(format!("Unknown input port {}", edge.input)))?;
        if !compatible(from, to) {
            return Err(error(format!(
                "Incompatible port types: {} → {}",
                from.data_type, to.data_type
            )));
        }
        if edge.input != "$exec" {
            if !targets.insert((edge.to, edge.input.clone())) {
                return Err(error("A parameter cannot connect to multiple outputs"));
            }
            if nodes[&edge.to].parameters.get(&edge.input).is_some() {
                return Err(error("A parameter cannot use both a literal and an edge"));
            }
        }
        *indegree.get_mut(&edge.to).unwrap() += 1;
    }
    let mut ready: BTreeSet<_> = indegree
        .iter()
        .filter(|(_, v)| **v == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = vec![];
    while let Some(id) = ready.pop_first() {
        order.push(id);
        for edge in graph.edges.iter().filter(|e| e.from == id) {
            let degree = indegree.get_mut(&edge.to).unwrap();
            *degree -= 1;
            if *degree == 0 {
                ready.insert(edge.to);
            }
        }
    }
    if order.len() != nodes.len() {
        return Err(error("The graph contains a cycle"));
    }
    let mut steps = vec![];
    let mut deferred = vec![];
    for id in &order {
        let node = nodes[id];
        let def = defs[id];
        let mut args = def.defaults.clone();
        for (key, value) in node.parameters.as_object().unwrap() {
            args[key] = value.clone();
        }
        for edge in graph
            .edges
            .iter()
            .filter(|e| e.to == *id && e.input != "$exec")
        {
            let from = defs[&edge.from];
            let path = if edge.output == "result" {
                "/result".into()
            } else {
                format!("/result/{}", escape(&edge.output))
            };
            let _ = from;
            args[&edge.input] = json!({"$ref":format!("node_{}#{path}",edge.from)});
        }
        inspect_arguments(&def.parameters_schema, &args).map_err(|e| {
            ActionError::new(&e.code, format!("Node {id} ({}): {}", def.title, e.message))
        })?;
        let (action, args) = match &def.binding {
            Binding::Action { action } => (
                action.clone(),
                if def.argument_mode == "value" {
                    args["value"].clone()
                } else {
                    args
                },
            ),
            Binding::State { resource } => ("workflow.state".into(), json!({"resource":resource})),
            _ => return Err(error("Invalid execution binding")),
        };
        if has_reference(&args) {
            deferred.push(format!("node_{id}"));
        } else {
            s.validate_action(&Action::new(&action, args.clone()))?;
        }
        steps.push(json!({"id":format!("node_{id}"),"action":action,"args":args,"output_schema":def.output_schema}));
    }
    Ok(Plan {
        steps: json!(steps),
        order,
        deferred,
    })
}
pub fn execute(s: &mut Session, op: super::Operation, args: Value) -> ActionResult {
    use super::Operation::*;
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
    match op {
        NodeCatalog => Ok(
            json!({"version":1,"nodes":s.registry.node_catalog().values().collect::<Vec<_>>(),"workflow_schema":crate::engine::workflow::schema(),"image_graph_schema":crate::engine::graph::schema(),"unknown_outputs":"any; validated at runtime"}),
        ),
        WorkflowRead => Ok(
            json!({"graph":s.doc.workflow,"validation":match compile(s,&s.doc.workflow){Ok(plan)=>json!({"ok":true,"order":plan.order}),Err(error)=>json!({"ok":false,"error":error})}}),
        ),
        WorkflowSet | WorkflowPatch => {
            let graph = if op == WorkflowSet {
                serde_json::from_value(args["graph"].clone())?
            } else {
                let patches: Vec<Patch> = serde_json::from_value(args["operations"].clone())?;
                s.doc.workflow.patched(patches)?
            };
            compile(s, &graph)?;
            s.finish_stroke();
            s.doc.set_workflow(graph)?;
            Ok(json!({"graph":s.doc.workflow}))
        }
        WorkflowValidate => {
            let graph = graph_arg(s, &args)?;
            let plan = compile(s, &graph)?;
            Ok(
                json!({"ok":true,"order":plan.order,"steps":plan.steps,"deferred_validation":plan.deferred,"executed":false}),
            )
        }
        WorkflowState => {
            let id = args["resource"].as_str().unwrap();
            let response = s.apply(Action::new("context.read", json!({"resources":[id]})))?;
            let resource = &response["result"]["resources"][id];
            if resource["ok"] != true {
                return Err(ActionError::new(
                    "state_query_failed",
                    resource["error"].to_string(),
                ));
            }
            Ok(resource["value"].clone())
        }
        WorkflowRun => {
            let graph = graph_arg(s, &args)?;
            let plan = compile(s, &graph)?;
            let mut request =
                json!({"steps":plan.steps,"dry_run":args["dry_run"].as_bool().unwrap_or(false)});
            if let Some(revision) = args.get("expected_revision") {
                request["expected_revision"] = revision.clone();
            }
            s.workflow_stack.push(graph);
            let result = s.apply(Action::new("pipeline.run", request));
            s.workflow_stack.pop();
            let mut report = result?["result"].clone();
            let parse_id = |value: &Value| {
                value
                    .as_str()
                    .and_then(|id| id.strip_prefix("node_"))
                    .and_then(|id| id.parse::<u32>().ok())
            };
            let completed = report["completed"]
                .as_array()
                .map(|a| a.iter().filter_map(parse_id).collect::<Vec<_>>())
                .unwrap_or_default();
            let failed = report.get("failed_step").and_then(parse_id);
            let results = report
                .as_object_mut()
                .unwrap()
                .remove("results")
                .unwrap_or(json!({}));
            Ok(
                json!({"ok":report["ok"],"dry_run":args["dry_run"].as_bool().unwrap_or(false),"completed_nodes":completed,"failed_node":failed,"results":results,"execution":report}),
            )
        }
        _ => Err(error("Invalid node operation")),
    }
}
fn graph_arg(s: &Session, args: &Value) -> Result<Graph, ActionError> {
    match args.get("graph") {
        Some(g) => Ok(serde_json::from_value(g.clone())?),
        None => Ok(s.doc.workflow.clone()),
    }
}
