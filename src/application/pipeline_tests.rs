use super::*;
use crate::{
    automation::{batch::Backend, mcp::Server},
    plugins::Registry,
};
use serde_json::{json, Value};
fn fresh() -> Session {
    let registry =
        Registry::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    assert!(
        registry.actions.contains_key("assistant.context"),
        "build DLLs first: {:?}",
        registry.diagnostics
    );
    Session::new(registry).unwrap()
}
fn call(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()["result"].clone()
}
#[test]
fn discovery_and_cross_dll_states_are_read_only_and_do_not_finish_a_stroke() {
    let mut s = fresh();
    let capabilities = call(&mut s, "app.describe", json!({}));
    assert_eq!(capabilities["pipeline_version"], 1);
    assert!(capabilities["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "ai_assistant.context"));
    assert_eq!(
        s.actions()
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "native.stats")
            .unwrap()["readOnly"],
        true
    );
    s.pointer(Pointer {
        phase: PointerPhase::Down,
        x: 50.,
        y: 50.,
        pressure: 0.4,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    })
    .unwrap();
    let revision = s.revision;
    let data = call(
        &mut s,
        "context.read",
        json!({"resources":["ai_assistant.context","unknown.state","input.pointer"]}),
    );
    assert_eq!(data["resources"]["ai_assistant.context"]["ok"], true);
    let resources = &data["resources"]["ai_assistant.context"]["value"]["resources"];
    assert_eq!(resources["native_paint.stats"]["ok"], true);
    assert_eq!(resources["document.active"]["value"]["width"], 1200);
    assert_eq!(
        data["resources"]["unknown.state"]["error"]["code"],
        "unknown_resource"
    );
    assert_eq!(
        data["resources"]["input.pointer"]["value"]["pen_observed"],
        true
    );
    assert_eq!(s.revision, revision);
    assert!(s.is_drawing());
    s.finish_stroke();
}
#[test]
fn a_dll_can_execute_another_dll_through_the_pipeline_and_keep_undo() {
    let mut s = fresh();
    call(&mut s, "canvas.new", json!({"width":64,"height":64}));
    let result = call(&mut s, "assistant.checkerboard", json!({"cell":8}));
    assert_eq!(result["ok"], true);
    assert_eq!(result["completed"], json!(["before", "paint", "after"]));
    assert_eq!(s.doc.layers[0].pixels[0], 0xff3d8ede);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    assert!(s.native_stack.is_empty());
}
#[test]
fn references_are_typed_dry_run_is_inert_and_revision_checks_prevent_stale_plans() {
    let mut s = fresh();
    let args = json!({"steps":[
        {"id":"inspect","action":"canvas.state"},
        {"id":"new","action":"canvas.new","args":{"width":{"$ref":"inspect#/result/height"},"height":37}}
    ]});
    let revision = s.revision;
    let mut dry = args.clone();
    dry["dry_run"] = json!(true);
    let result = call(&mut s, "pipeline.run", dry);
    assert_eq!(result["deferred_validation"], json!(["new"]));
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.width, 1200);
    let result = call(&mut s, "pipeline.run", args);
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.width, 900);
    assert_eq!(s.doc.height, 37);
    let args =
        json!({"expected_revision":revision,"steps":[{"id":"clear","action":"layer.clear"}]});
    assert_eq!(
        s.apply(Action::new("pipeline.run", args)).unwrap_err().code,
        "revision_conflict"
    );
    assert_eq!(
        s.apply(Action::new(
            "pipeline.run",
            json!({"expected_revision":42.0,"steps":[{"id":"read","action":"canvas.state"}]})
        ))
        .unwrap_err()
        .code,
        "invalid_arguments"
    );
}
#[test]
fn static_errors_fail_before_writes_and_dynamic_failure_reports_completed_steps() {
    let mut s = fresh();
    let revision = s.revision;
    for bad in [
        json!({"id":"bad","action":"canvas.new","args":{"width":0,"height":4}}),
        json!({"id":"bad","action":"canvas.new","args":{"width":{"$ref":"future#/result/width"},"height":4}}),
        json!({"id":"first","action":"canvas.state"}),
    ] {
        assert!(s.apply(Action::new("pipeline.run",json!({"steps":[{"id":"first","action":"canvas.new","args":{"width":20,"height":20}},bad]}))).is_err());
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.width, 1200);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"steps":[
            {"id":"new","action":"canvas.new","args":{"width":20,"height":20}},
            {"id":"bad","action":"canvas.new","args":{"width":{"$ref":"new#/result/missing"},"height":10}},
            {"id":"never","action":"canvas.new","args":{"width":30,"height":30}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["failed_step"], "bad");
    assert_eq!(result["completed"], json!(["new"]));
    assert_eq!(s.doc.width, 20);
    assert_eq!(s.revision, revision + 1);
}
#[test]
fn readonly_scopes_and_cycles_fail_without_poisoning_the_session() {
    let mut s = fresh();
    s.read_depth = 1;
    assert_eq!(
        s.apply(Action::new("canvas.new", json!({"width":8,"height":8})))
            .unwrap_err()
            .code,
        "read_only"
    );
    s.read_depth = 0;
    s.native_stack.push("native_paint".into());
    assert_eq!(
        s.apply(Action::new("native.stats", json!({})))
            .unwrap_err()
            .code,
        "native_reentrancy"
    );
    s.native_stack.clear();
    s.execution_depth = 16;
    assert_eq!(
        s.apply(Action::new("canvas.state", json!({})))
            .unwrap_err()
            .code,
        "execution_limit"
    );
    s.execution_depth = 0;
    assert!(s.apply(Action::new("assistant.context", json!({}))).is_ok());
    assert_eq!(s.read_depth, 0);
    assert_eq!(s.execution_depth, 0);
}
#[test]
fn imported_images_are_visible_as_open_documents_without_becoming_save_targets() {
    let mut s = fresh();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/pipeline-file");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("来源.png");
    call(&mut s, "canvas.export", json!({"path":path}));
    call(&mut s, "document.open", json!({"path":path}));
    let data = call(
        &mut s,
        "context.read",
        json!({"resources":["workspace.documents"]}),
    );
    let doc = &data["resources"]["workspace.documents"]["value"]["documents"][0];
    assert_eq!(doc["source_path"], json!(path));
    assert!(doc["save_path"].is_null());
    assert_eq!(doc["name"], "来源.png");
}
#[test]
fn mcp_resources_and_pipeline_errors_share_the_same_backend() {
    let mut mcp = Server::new(fresh());
    mcp.message(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25"}}));
    mcp.message(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let listing = mcp
        .message(json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}))
        .unwrap();
    assert!(listing["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["uri"] == "micopi://state/ai_assistant.context"));
    let read=mcp.message(json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"micopi://state/ai_assistant.context"}})).unwrap();
    let contents: Value =
        serde_json::from_str(read["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(contents["resources"]["native_paint.stats"]["ok"], true);
    let failed=mcp.message(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"pipeline.run","arguments":{"steps":[
        {"id":"read","action":"canvas.state"},
        {"id":"bad","action":"canvas.new","args":{"width":{"$ref":"read#/result/nope"},"height":1}}
    ]}}})).unwrap();
    assert_eq!(failed["result"]["isError"], true);
    assert_eq!(
        failed["result"]["structuredContent"]["result"]["completed"],
        json!(["read"])
    );
}
#[test]
fn nested_pipeline_failures_stop_outer_pipelines_and_jsonl_batches() {
    let mut s = fresh();
    let nested = json!({"steps":[{"id":"read","action":"canvas.state"},{"id":"bad","action":"canvas.new","args":{"width":{"$ref":"read#/result/missing"},"height":5}}]});
    let args = json!({"steps":[{"id":"nested","action":"pipeline.run","args":nested},{"id":"never","action":"canvas.new","args":{"width":5,"height":5}}]});
    let result = call(&mut s, "pipeline.run", args.clone());
    assert_eq!(result["ok"], false);
    assert_eq!(result["failed_step"], "nested");
    assert_eq!(result["failed_result"]["result"]["failed_step"], "bad");
    assert_eq!(s.doc.width, 1200);
    let text = format!(
        "{}\n{}\n",
        json!({"action":"pipeline.run","args":args}),
        json!({"action":"canvas.new","args":{"width":5,"height":5}})
    );
    let mut output = Vec::new();
    assert!(
        crate::automation::batch::run(&mut s, &mut std::io::Cursor::new(text), &mut output)
            .is_err()
    );
    let lines = String::from_utf8(output).unwrap();
    assert_eq!(lines.lines().count(), 1);
    let line: Value = serde_json::from_str(lines.lines().next().unwrap()).unwrap();
    assert_eq!(line["ok"], false);
    assert_eq!(s.doc.width, 1200);
    let macro_definition = serde_json::from_value(
        json!({"id":"test.failure","title":"failure propagation","steps":[
            {"action":"pipeline.run","args":args},
            {"action":"canvas.new","args":{"width":5,"height":5}}
        ]}),
    )
    .unwrap();
    s.registry
        .actions
        .insert("test.failure".into(), macro_definition);
    let result = call(&mut s, "test.failure", json!({}));
    assert_eq!(result["ok"], false);
    assert_eq!(s.doc.width, 1200);
    assert_eq!(
        serde_json::to_value(s.events.back().unwrap()).unwrap()["success"],
        false
    );
}
#[test]
fn public_state_output_is_validated_and_bad_providers_are_rejected_atomically() {
    let mut s = fresh();
    s.registry
        .states
        .get_mut("native_paint.stats")
        .unwrap()
        .output_schema = json!({"type":"object","required":["missing"]});
    let result = call(
        &mut s,
        "context.read",
        json!({"resources":["native_paint.stats","document.active"]}),
    );
    assert_eq!(
        result["resources"]["native_paint.stats"]["error"]["code"],
        "invalid_state_output"
    );
    assert_eq!(result["resources"]["document.active"]["ok"], true);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/pipeline-bad-provider");
    std::fs::create_dir_all(&dir).unwrap();
    Registry::load(&dir.join("plugins")).unwrap();
    let root = dir.join("plugins");
    let path = root.join("bad.json");
    for (id, action) in [
        ("bad.state", "canvas.new"),
        ("document.active", "canvas.state"),
    ] {
        std::fs::write(&path,json!({"api_version":1,"id":"bad","name":"bad","requires":["core"],"actions":[{"id":"bad.partial","title":"must not leak","operation":"state"}],"states":[{"id":id,"title":"bad","action":action,"output_schema":{"type":"object"}}]}).to_string()).unwrap();
        let registry = Registry::load(&root).unwrap();
        assert!(!registry.loaded.contains(&"bad".into()));
        assert!(!registry.actions.contains_key("bad.partial"));
        assert!(!registry.diagnostics.is_empty());
    }
}
