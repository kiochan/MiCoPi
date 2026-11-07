//! Built-in published state revisions agree with their authoritative envelope.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn response(s: &mut Session, action: &str, args: Value) -> Value {
    let value = s.apply(Action::new(action, args)).unwrap();
    assert_eq!(value["revision"], s.revision, "{action} envelope");
    value
}
fn published(s: &mut Session, action: &str, args: Value) -> Value {
    let value = response(s, action, args);
    assert_eq!(
        value["result"]["revision"], value["revision"],
        "{action} state"
    );
    let current = call(s, "canvas.state", json!({}));
    assert_eq!(
        value["result"]["revision"], current["revision"],
        "{action} query"
    );
    value["result"].clone()
}
fn output() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/result-revisions");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}.png",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn published_full_state_revision_is_usable_as_the_next_optimistic_precondition() {
    let mut s = session();
    for (action, args) in [
        ("canvas.new", json!({"width":6,"height":4})),
        ("layer.add", json!({})),
        ("layer.rename", json!({"layer_id":1,"name":"Ink"})),
        (
            "layer.set",
            json!({"layer_id":1,"opacity":0.5,"color_label":"violet"}),
        ),
        (
            "layer.selection.set",
            json!({"layer_ids":[1,2],"active_layer_id":1}),
        ),
        (
            "group.create",
            json!({"members":[{"kind":"layer","id":2}],"name":"Cels"}),
        ),
        ("group.set", json!({"group_id":1,"opacity":0.75})),
        (
            "group.move",
            json!({"items":[{"kind":"group","id":1}],"parent_id":0,"index":0}),
        ),
    ] {
        let before = s.revision;
        let value = if action == "layer.add" {
            let value = response(&mut s, action, args);
            assert!(value["result"].get("revision").is_none());
            value
        } else {
            published(&mut s, action, args)
        };
        assert_eq!(s.revision, before + 1);
        let mut next = Action::new("layer.rename", json!({"layer_id":1,"name":"Guarded ink"}));
        next.expected_revision = Some(value["revision"].as_u64().unwrap());
        let next = s.apply(next).unwrap();
        assert_eq!(next["result"]["revision"], next["revision"]);
    }
    check(&mut s);
}

#[test]
fn published_mask_owner_group_delete_and_transform_state_preserve_payload_boundaries() {
    let mut s = session();
    call(&mut s, "layer.add", json!({}));
    let group = published(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    for owner in [
        json!({"kind":"layer","id":2}),
        json!({"kind":"group","id":group}),
    ] {
        let before = s.revision;
        let value = response(&mut s, "mask.create", json!({"owner":owner,"select":false}));
        assert_eq!(s.revision, before + 1);
        // Existing mask mutation payloads have no Session revision field.
        assert!(value["result"].get("revision").is_none());
        assert!(value["result"]["raster_masks"].get("revision").is_none());
    }
    let removed = published(&mut s, "group.delete", json!({"group_id":group}));
    assert_eq!(removed["deleted_layer_ids"], json!([2]));
    assert!(removed["raster_masks"].get("revision").is_none());
    published(&mut s, "history.undo", json!({}));
    published(&mut s, "history.redo", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#8ABED1"}));
    let transformed = published(
        &mut s,
        "canvas.transform",
        json!({"target":"layer","translate_x":1,"method":"nearest"}),
    );
    assert_eq!(transformed["last_transform"]["available"], true);
    assert!(transformed["last_transform"].get("revision").is_none());
    check(&mut s);
}

#[test]
fn published_layer_batch_has_one_revision_without_fabricated_child_increments() {
    let mut s = session();
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.add", json!({}));
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = published(
        &mut s,
        "layer.batch",
        json!({"action":"layer.set","layer_ids":[1,3],"args":{"opacity":0.5,"color_label":"blue"}}),
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(s.doc.layers[0].opacity, 128);
    assert_eq!(s.doc.layers[2].opacity, 128);
    assert_eq!(s.doc.layers[1].opacity, 255);
    published(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers.iter().all(|layer| layer.opacity == 255));
    check(&mut s);
}

#[test]
fn published_noops_queries_and_export_follow_existing_revision_rules() {
    let mut s = session();
    let history = s.doc.history_bytes();
    let before = s.revision;
    published(&mut s, "canvas.resize", json!({"width":12,"height":8}));
    assert_eq!(s.revision, before + 1);
    assert_eq!(s.doc.history_bytes(), history);
    call(&mut s, "layer.fill_color", json!({"color":"#8ABED1"}));
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    let before = s.revision;
    for (action, args) in [
        ("canvas.state", json!({})),
        ("context.read", json!({"resources":["document.active"]})),
        ("canvas.new.plan", json!({"width":5,"height":3})),
        ("canvas.export.plan", json!({})),
        ("canvas.export.preview", json!({})),
        ("canvas.snapshot", json!({})),
        ("mask.query", json!({"mask_id":mask})),
        ("mask.snapshot", json!({"mask_id":mask})),
    ] {
        let value = published(&mut s, action, args);
        assert_eq!(value["revision"], before);
        assert_eq!(s.revision, before);
    }
    let value = published(&mut s, "canvas.export", json!({"path":output()}));
    assert_eq!(value["revision"], before + 1);
    assert_eq!(s.revision, before + 1);
    assert!(value.get("sha256").is_some());
    check(&mut s);
}

#[test]
fn published_pointer_and_stroke_envelopes_keep_typed_sample_counts_and_payload_shapes() {
    let mut s = session();
    let before = s.revision;
    for (i, phase) in ["down", "move", "up"].into_iter().enumerate() {
        let value = response(
            &mut s,
            "canvas.pointer",
            json!({"phase":phase,"x":2+i,"y":2,"pressure":0.3+i as f64*0.2,"pen":true,"time":i}),
        );
        assert_eq!(s.revision, before + i as u64 + 1);
        assert_eq!(value["result"], json!({"drawing":phase!="up"}));
    }
    let before = s.revision;
    let value = response(
        &mut s,
        "canvas.stroke",
        json!({"tool":"brush","points":[{"x":1,"y":1,"pressure":0.2},{"x":3,"y":2,"pressure":0.8},{"x":5,"y":3,"pressure":1.0}]}),
    );
    // Three samples and the synthetic UP keep their existing four increments;
    // the enclosing Stroke action retains its separate successful-action bump.
    assert_eq!(s.revision, before + 5);
    assert_eq!(value["result"], json!({"points":3}));
    assert_eq!(
        call(&mut s, "canvas.state", json!({}))["revision"],
        s.revision
    );
    check(&mut s);
}

fn macro_action(s: &mut Session, id: &str, steps: Value) {
    let definition =
        serde_json::from_value(json!({"id":id,"title":"Revision contract fixture","steps":steps}))
            .unwrap();
    s.registry.actions.insert(id.into(), definition);
}
#[test]
fn published_terminal_macro_operations_are_classified_without_rewriting_native_reports() {
    let mut s = session();
    let artist_value = json!({"revision":4242,"nested":{"revision":"artist-owned"}});
    let value = response(
        &mut s,
        "ui.dispatch",
        json!({"id":"test.revision","value":artist_value}),
    );
    assert_eq!(value["result"]["value"], artist_value);
    let state = published(&mut s, "canvas.state", json!({}));
    assert_eq!(state["ui"]["test.revision"], artist_value);
    macro_action(
        &mut s,
        "test.state_macro",
        json!([
            {"action":"layer.rename","args":{"name":"Macro ink"}},
            {"action":"layer.set","args":{"opacity":0.75}}
        ]),
    );
    let before = s.revision;
    published(&mut s, "test.state_macro", json!({}));
    assert_eq!(s.revision, before + 2);
    macro_action(
        &mut s,
        "test.native_macro",
        json!([{"action":"assistant.checkerboard","args":{"cell":2}}]),
    );
    for action in ["assistant.checkerboard", "test.native_macro"] {
        let value = response(
            &mut s,
            action,
            if action == "assistant.checkerboard" {
                json!({"cell":2})
            } else {
                json!({})
            },
        );
        let report = &value["result"];
        assert_eq!(report["ok"], true);
        assert_eq!(report["revision"], report["results"]["after"]["revision"]);
        assert_eq!(
            report["results"]["after"]["result"]["revision"],
            report["revision"]
        );
        assert!(
            report["revision"].as_u64().unwrap() < value["revision"].as_u64().unwrap(),
            "Native-owned revision must remain the DLL's observed version"
        );
    }
    check(&mut s);
}

#[test]
fn published_nested_pipeline_reports_keep_each_return_time_revision() {
    for atomic in [false, true] {
        let mut s = session();
        let before = s.revision;
        let value = response(
            &mut s,
            "pipeline.run",
            json!({"atomic":atomic,"steps":[
                {"id":"first","action":"layer.rename","args":{"name":"First"}},
                {"id":"inner","action":"pipeline.run","args":{"steps":[
                    {"id":"second","action":"layer.set","args":{"opacity":0.5}},
                    {"id":"observed","action":"canvas.state"}
                ]}},
                {"id":"final","action":"canvas.state"}
            ]}),
        );
        assert_eq!(s.revision, before + 2);
        let report = &value["result"];
        assert_eq!(report["revision"], before + 2);
        assert_eq!(report["results"]["first"]["revision"], before + 1);
        assert_eq!(report["results"]["first"]["result"]["revision"], before + 1);
        let inner = &report["results"]["inner"]["result"];
        assert_eq!(inner["start_revision"], before + 1);
        assert_eq!(inner["revision"], before + 2);
        assert_eq!(inner["results"]["second"]["result"]["revision"], before + 2);
        assert_eq!(
            inner["results"]["observed"]["result"]["revision"],
            before + 2
        );
        assert_eq!(report["results"]["final"]["result"]["revision"], before + 2);
        check(&mut s);
    }
}

#[test]
fn published_workflow_reports_keep_historical_node_revisions_in_both_execution_modes() {
    for atomic in [false, true] {
        let mut s = session();
        let graph = json!({"version":1,"nodes":[
            {"id":1,"type":"action.layer.rename","position":[0,0],"parameters":{"name":"Node ink"}},
            {"id":2,"type":"action.layer.set","position":[360,0],"parameters":{"opacity":0.5}}
        ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"}]});
        call(&mut s, "workflow.set", json!({"graph":graph}));
        let before = s.revision;
        let value = response(&mut s, "workflow.run", json!({"atomic":atomic}));
        let report = &value["result"];
        assert_eq!(s.revision, before + 2);
        assert_eq!(report["execution"]["revision"], before + 2);
        assert_eq!(
            report["results"]["node_1"]["result"]["revision"],
            before + 1
        );
        assert_eq!(
            report["results"]["node_2"]["result"]["revision"],
            before + 2
        );
        if atomic {
            assert_eq!(report["revision"], before + 2);
        } else {
            assert!(report.get("revision").is_none());
        }
        check(&mut s);
    }
}

#[test]
fn published_failed_atomic_reports_preserve_attempted_step_versions_and_rollback_version() {
    for atomic in [false, true] {
        let mut s = session();
        let before = s.revision;
        let name = s.doc.layer_name(0).to_owned();
        let value = response(
            &mut s,
            "pipeline.run",
            json!({"atomic":atomic,"steps":[
                {"id":"changed","action":"layer.rename","args":{"name":"Attempt"}},
                {"id":"fail","action":"layer.delete","args":{"layer_id":4294967295u64}}
            ]}),
        );
        let report = &value["result"];
        assert_eq!(report["ok"], false);
        assert_eq!(report["results"]["changed"]["revision"], before + 1);
        assert_eq!(
            report["results"]["changed"]["result"]["revision"],
            before + 1
        );
        assert_eq!(s.revision, before + u64::from(!atomic));
        assert_eq!(report["revision"], s.revision);
        assert_eq!(
            s.doc.layer_name(0),
            if atomic { name.as_str() } else { "Attempt" }
        );
        check(&mut s);
    }
    let mut s = session();
    s.doc.renderer = None;
    let before = s.revision;
    let composition = s.doc.composition();
    assert!(s
        .apply(Action::new("group.create", json!({"members":[]})))
        .is_err());
    assert_eq!(s.revision, before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(
        published(&mut s, "canvas.state", json!({}))["revision"],
        before
    );
}

#[test]
fn published_revision_is_not_exposed_as_success_when_journal_publication_fails() {
    let mut s = session();
    let before = s.revision;
    let state = s.state();
    let history = s.doc.history_bytes();
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("layer.rename", json!({"name":"Not committed"})))
        .is_err());
    assert_eq!(s.revision, before);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert_eq!(
        published(&mut s, "canvas.state", json!({}))["revision"],
        before
    );
}

#[test]
fn published_settings_and_workflow_providers_preserve_custom_root_revision_values() {
    let mut s = session();
    let path = output().with_extension("settings.json");
    let resource = "test.settings_revision";
    let provider = serde_json::from_value(json!({
        "id":resource,"title":"Custom profile revision fixture","action":"settings.read",
        "args":{"path":path},"output_schema":{"type":"object"}
    }))
    .unwrap();
    s.registry.states.insert(resource.into(), provider);
    let before = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    for custom in [json!(424242), json!("profile-owned")] {
        let document = json!({"version":1,"revision":custom,"extension":{"revision":17}});
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
        let direct = response(&mut s, "settings.read", json!({"path":path}));
        assert_eq!(direct["result"]["revision"], custom);
        assert_eq!(direct["result"]["extension"]["revision"], 17);
        let state = response(&mut s, "workflow.state", json!({"resource":resource}));
        assert_eq!(state["result"], direct["result"]);
        let context = response(&mut s, "context.read", json!({"resources":[resource]}));
        assert_eq!(context["result"]["revision"], before);
        assert_eq!(
            context["result"]["resources"][resource]["value"],
            direct["result"]
        );
        assert_eq!(s.revision, before);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    }
    check(&mut s);
}

#[test]
fn published_open_adopts_embedded_revision_before_publication_and_next_guard() {
    let mut source = session();
    call(&mut source, "canvas.new", json!({"width":7,"height":5}));
    call(&mut source, "layer.fill_color", json!({"color":"#A5C7DC"}));
    for name in ["Draft", "Clean ink", "Saved cel"] {
        call(&mut source, "layer.rename", json!({"name":name}));
    }
    let path = output().with_extension("mcp");
    let saved = response(&mut source, "document.save", json!({"path":path}));
    let embedded = source.journal.as_ref().unwrap().state.metadata.revision;
    assert_eq!(embedded, saved["revision"].as_u64().unwrap());
    let mut receiver = session();
    assert!(embedded > receiver.revision);
    let opened = published(&mut receiver, "document.open", json!({"path":path}));
    assert_eq!(receiver.revision, embedded + 1);
    assert_eq!(opened["revision"], embedded + 1);
    assert_eq!(receiver.doc.composition(), source.doc.composition());
    assert_eq!(receiver.doc.layers[0].pixels, source.doc.layers[0].pixels);
    let mut next = Action::new("layer.rename", json!({"name":"Guarded opened cel"}));
    next.expected_revision = Some(opened["revision"].as_u64().unwrap());
    let next = receiver.apply(next).unwrap();
    assert_eq!(next["revision"], embedded + 2);
    assert_eq!(next["result"]["revision"], next["revision"]);
    check(&mut receiver);
}
