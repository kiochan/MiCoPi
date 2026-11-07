use super::*;
use crate::{engine::workflow::Graph, plugins::Registry};
use serde_json::{json, Value};
fn fresh() -> Session {
    let registry =
        Registry::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    assert!(
        registry.diagnostics.is_empty(),
        "{:?}",
        registry.diagnostics
    );
    Session::new(registry).unwrap()
}
fn call(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()["result"].clone()
}
fn graph() -> Value {
    json!({"version":1,"nodes":[
    {"id":1,"type":"state.document.active","position":[0,0]},
    {"id":2,"type":"action.canvas.new","position":[420,0],"parameters":{}},
    {"id":3,"type":"action.native.checkerboard","position":[840,0],"parameters":{"cell":8}}
],"edges":[{"from":1,"output":"width","to":2,"input":"width"},{"from":1,"output":"height","to":2,"input":"height"},{"from":2,"output":"$exec","to":3,"input":"$exec"}]})
}
#[test]
fn all_plugin_actions_states_and_image_definitions_are_discovered_without_invocation() {
    let mut s = fresh();
    let revision = s.revision;
    let catalog = call(&mut s, "node.catalog", json!({}));
    let nodes = catalog["nodes"].as_array().unwrap();
    for action in s.registry.actions.keys() {
        assert!(
            nodes.iter().any(|n| n["id"] == format!("action.{action}")),
            "{action}"
        );
    }
    assert!(nodes.iter().any(|n| n["id"] == "native_paint.statistics"));
    assert!(nodes
        .iter()
        .any(|n| n["id"] == "state.native_paint.stats" && n["domain"] == "workflow"));
    let image_ids = nodes
        .iter()
        .filter(|n| n["domain"] == "image")
        .map(|n| n["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        image_ids,
        std::collections::BTreeSet::from([
            "compositing.layer",
            "compositing.filter",
            "compositing.levels",
            "compositing.blend",
            "compositing.mix",
            "compositing.output",
            "compositing.transparent",
        ])
    );
    let transparent = nodes
        .iter()
        .find(|n| n["id"] == "compositing.transparent")
        .unwrap();
    assert_eq!(transparent["domain"], "image");
    assert!(transparent["inputs"].as_array().unwrap().is_empty());
    let mix = nodes.iter().find(|n| n["id"] == "compositing.mix").unwrap();
    assert_eq!(mix["domain"], "image");
    let inputs = mix["inputs"].as_array().unwrap();
    assert_eq!(inputs.len(), 2);
    for (input, name) in inputs.iter().zip(["before", "after"]) {
        assert_eq!(input["id"], name);
        assert_eq!(input["data_type"], "image");
        assert_eq!(input["required"], true);
    }
    let outputs = mix["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0]["id"], "image");
    assert_eq!(outputs[0]["data_type"], "image");
    let opacity = &mix["parameters_schema"]["properties"]["opacity"];
    assert_eq!(opacity["type"], "number");
    assert_eq!(opacity["minimum"], 0);
    assert_eq!(opacity["maximum"], 1);
    assert_eq!(mix["defaults"]["opacity"], 1);
    let state = nodes
        .iter()
        .find(|n| n["id"] == "action.canvas.state")
        .unwrap();
    assert!(state["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["id"] == "width" && p["data_type"] == "integer"));
    assert_eq!(s.revision, revision);
    assert!(s.native_instances.is_empty());
}
#[test]
fn state_data_ports_compile_to_pipeline_and_control_edges_preserve_side_effect_order() {
    let mut s = fresh();
    call(&mut s, "canvas.new", json!({"width":48,"height":32}));
    let g = graph();
    let revision = s.revision;
    let checked = call(&mut s, "workflow.validate", json!({"graph":g}));
    assert_eq!(checked["order"], json!([1, 2, 3]));
    assert_eq!(s.revision, revision);
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "workflow.set", json!({"graph":g}));
    let revision = s.revision;
    let dry = call(&mut s, "workflow.run", json!({"dry_run":true}));
    assert_eq!(dry["ok"], true);
    assert_eq!(s.revision, revision);
    let run = call(
        &mut s,
        "workflow.run",
        json!({"expected_revision":revision}),
    );
    assert_eq!(run["ok"], true);
    assert_eq!(run["completed_nodes"], json!([1, 2, 3]));
    assert_eq!((s.doc.width, s.doc.height), (48, 32));
    assert_eq!(s.doc.layers[0].pixels[0], 0xff3d8ede);
    assert_eq!(s.doc.workflow, serde_json::from_value::<Graph>(g).unwrap());
    assert!(s.workflow_stack.is_empty());
}
#[test]
fn graph_editing_and_patches_are_atomic_and_undoable_without_running_actions() {
    let mut s = fresh();
    call(&mut s, "workflow.set", json!({"graph":graph()}));
    let before = s.doc.workflow.clone();
    let revision = s.revision;
    assert!(s.apply(Action::new("workflow.patch",json!({"operations":[{"op":"move_node","id":1,"position":[100,100]},{"op":"connect","edge":{"from":9,"output":"result","to":2,"input":"width"}}]}))).is_err());
    assert_eq!(s.doc.workflow, before);
    assert_eq!(s.revision, revision);
    call(
        &mut s,
        "workflow.patch",
        json!({"operations":[{"op":"move_node","id":1,"position":[100,100]}]}),
    );
    assert_ne!(s.doc.workflow, before);
    s.doc.undo();
    assert_eq!(s.doc.workflow, before);
    s.doc.redo();
    assert_eq!(s.doc.workflow.nodes[0].position, [100., 100.]);
    assert!(s.native_instances.is_empty());
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
}
#[test]
fn incompatible_ports_duplicate_inputs_cycles_and_missing_plugins_fail_before_execution() {
    let mut s = fresh();
    let revision = s.revision;
    let mut g = graph();
    g["edges"][0]["output"] = json!("dirty");
    assert!(s
        .apply(Action::new("workflow.set", json!({"graph":g})))
        .is_err());
    let mut g = graph();
    g["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"from":3,"output":"$exec","to":1,"input":"$exec"}));
    assert!(s
        .apply(Action::new("workflow.set", json!({"graph":g})))
        .is_err());
    let mut g = graph();
    g["nodes"][2]["type"] = json!("action.missing_plugin.run");
    assert_eq!(
        s.apply(Action::new("workflow.set", json!({"graph":g})))
            .unwrap_err()
            .code,
        "missing_node_type"
    );
    let mut g = graph();
    g["nodes"][1]["parameters"] = json!({"width":20});
    assert!(s
        .apply(Action::new("workflow.set", json!({"graph":g})))
        .is_err());
    assert_eq!(s.revision, revision);
}
#[test]
fn undeclared_outputs_remain_any_and_bad_declared_outputs_stop_the_next_node() {
    let mut s = fresh();
    let g = json!({"version":1,"nodes":[{"id":1,"type":"action.canvas.new","position":[0,0],"parameters":{"width":20,"height":20}},{"id":2,"type":"action.layer.add","position":[400,0]}],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"}]});
    s.registry
        .actions
        .get_mut("canvas.new")
        .unwrap()
        .output_schema = Some(json!({"type":"object","required":["missing"]}));
    let result = call(&mut s, "workflow.run", json!({"graph":g}));
    assert_eq!(result["ok"], false);
    assert_eq!(result["failed_node"], 1);
    assert_eq!(result["execution"]["error"]["code"], "invalid_node_output");
    assert_eq!(s.doc.width, 20);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(result["completed_nodes"], json!([]));
}
#[test]
fn workflows_persist_with_projects_and_missing_types_remain_inspectable() {
    let mut s = fresh();
    call(&mut s, "workflow.set", json!({"graph":graph()}));
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("workflow-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":file}));
    let mut second = fresh();
    call(&mut second, "document.open", json!({"path":file}));
    assert_eq!(s.doc.workflow, second.doc.workflow);
    second.registry.actions.remove("native.checkerboard");
    let state = call(&mut second, "workflow.read", json!({}));
    assert_eq!(state["validation"]["ok"], false);
    assert_eq!(state["graph"], json!(s.doc.workflow));
}
#[test]
fn revision_guards_and_state_failures_are_reported_to_headless_and_mcp() {
    let mut s = fresh();
    let g = json!({"version":1,"nodes":[{"id":1,"type":"action.workflow.state","position":[0,0],"parameters":{"resource":"unknown.state"}}],"edges":[]});
    assert_eq!(
        s.apply(Action::new(
            "workflow.run",
            json!({"graph":g,"expected_revision":42.0})
        ))
        .unwrap_err()
        .code,
        "invalid_arguments"
    );
    assert_eq!(
        s.apply(Action::new(
            "workflow.run",
            json!({"graph":g,"expected_revision":42})
        ))
        .unwrap_err()
        .code,
        "revision_conflict"
    );
    let result = call(&mut s, "workflow.run", json!({"graph":g}));
    assert_eq!(result["ok"], false);
    assert_eq!(result["failed_node"], 1);
    let mut server = crate::automation::mcp::Server::new(s);
    server.message(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}));
    server.message(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let r=server.message(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"workflow.run","arguments":{"graph":g}}})).unwrap();
    assert_eq!(r["result"]["isError"], true);
}
#[test]
fn plugin_node_registration_rejects_bad_contributions_without_leaking_actions() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/node-registration-{}",
        std::process::id()
    ));
    let _ = Registry::load(&dir).unwrap();
    let file = dir.join("bad.json");
    for node in [
        json!({"id":"bad.alias","title":"bad","binding":{"kind":"action","action":"missing"}}),
        json!({"id":"other.alias","title":"bad","binding":{"kind":"state","resource":"document.active"}}),
        json!({"id":"bad.image","title":"bad","binding":{"kind":"image","operation":"filter","defaults":42},"parameters_schema":{"type":"object"}}),
        json!({"id":"bad.image","title":"bad","binding":{"kind":"image","operation":"filter","defaults":{"filter":"invert","amount":2}},"parameters_schema":{"type":"object"}}),
    ] {
        std::fs::write(&file,json!({"api_version":1,"id":"bad","name":"bad","requires":["core"],"actions":[{"id":"bad.leaked","title":"bad","operation":"state"}],"nodes":[node]}).to_string()).unwrap();
        let registry = Registry::load(&dir).unwrap();
        assert!(!registry.actions.contains_key("bad.leaked"));
        assert!(!registry.loaded.contains(&"bad".into()));
        assert!(!registry.diagnostics.is_empty());
    }
}
#[test]
fn scalar_action_inputs_use_value_port_and_json_types_are_preserved() {
    let mut s = fresh();
    let mut scalar = s.registry.actions["native.stats"].clone();
    scalar.id = "native.scalar".into();
    scalar.input_schema = Some(json!({"type":"integer","minimum":0}));
    s.registry.actions.insert(scalar.id.clone(), scalar);
    s.registry.native_actions.insert(
        "native.scalar".into(),
        ("native_paint".into(), "stats".into()),
    );
    let g = json!({"version":1,"nodes":[{"id":1,"type":"action.native.scalar","position":[0,0],"parameters":{"value":123}}],"edges":[]});
    let result = call(&mut s, "workflow.run", json!({"graph":g}));
    assert_eq!(result["ok"], true);
    assert_eq!(
        s.registry.node_catalog()["action.native.scalar"].argument_mode,
        "value"
    );
}
