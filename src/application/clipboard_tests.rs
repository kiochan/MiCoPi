//! Clipboard contracts through the staged native transfer DLL.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn rectangle(s: &mut Session) {
    call(
        s,
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":3}),
    );
}
#[test]
fn clipboard_copy_paste_is_one_undo_and_replays_without_live_clipboard() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    rectangle(&mut s);
    let source = s.doc.layers[0].pixels.clone();
    let mut before = s.doc.composition();
    let summary = call(&mut s, "clipboard.copy", json!({}));
    assert_eq!(
        (summary["width"].as_u64(), summary["height"].as_u64()),
        (Some(5), Some(3))
    );
    assert_eq!(summary["bytes"], 60);
    let revision = s.revision;
    assert_eq!(call(&mut s, "clipboard.state", json!({})), summary);
    assert_eq!(s.revision, revision);
    let count = s.replay_journal(false).unwrap().entries.len();
    let result = call(&mut s, "clipboard.paste_in_place", json!({}));
    assert_eq!(result["x"], 3);
    assert_eq!(s.doc.layers.len(), 2);
    assert!(s.doc.selection().current.is_none());
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[1].pixels[y * 12 + x],
                if (3..8).contains(&x) && (2..5).contains(&y) {
                    source[y * 12 + x]
                } else {
                    0
                }
            );
        }
    }
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    let wire = s.replay_journal(false).unwrap().events(count, 1).unwrap();
    assert!(wire
        .to_string()
        .contains(summary["sha256"].as_str().unwrap()));
    s.clipboard = None;
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    before.next_layer_id = s.doc.composition().next_layer_id;
    assert_eq!(s.doc.composition(), before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers.len(), 2);
    check(&mut s);
}
#[test]
fn clipboard_cut_is_undoable_and_fractional_coverage_is_applied_once() {
    let mut s = session();
    s.doc.layers[0].pixels.fill(0x80402010);
    let mut coverage = vec![0; 96];
    coverage[2 * 12 + 3] = 128;
    coverage[2 * 12 + 4] = 255;
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(crate::engine::selection::Mask::new(12, 8, coverage).unwrap()),
            previous: None,
        })
        .unwrap();
    call(&mut s, "clipboard.cut", json!({}));
    assert_eq!(s.doc.layers[0].pixels[27], 0x40402010);
    assert_eq!(s.doc.layers[0].pixels[28], 0);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0x80402010));
    call(&mut s, "clipboard.paste_in_place", json!({}));
    assert_eq!(s.doc.layers[1].pixels[27], 0x40402010);
    assert_eq!(s.doc.layers[1].pixels[28], 0x80402010);
    check(&mut s);
}
#[test]
fn clipboard_atomic_failure_restores_payload_and_document() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "clipboard.copy", json!({}));
    let original = s.clipboard.clone().unwrap();
    let pixels = s.doc.layers[0].pixels.clone();
    let result=s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[
        {"id":"select","action":"selection.shape","args":{"kind":"rectangle","x":1,"y":1,"width":2,"height":2}},
        {"id":"cut","action":"clipboard.cut","args":{}},
        {"id":"paste","action":"clipboard.paste","args":{}},
        {"id":"fail","action":"layer.select","args":{"index":15}}
    ]})));
    assert_eq!(result.unwrap()["result"]["rolled_back"], true);
    assert!(std::sync::Arc::ptr_eq(
        s.clipboard.as_ref().unwrap(),
        &original
    ));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert!(s.doc.selection().current.is_none());
    check(&mut s);
}
#[test]
fn clipboard_bounds_empty_and_locked_failures_preserve_payload() {
    let mut s = session();
    assert!(s.apply(Action::new("clipboard.paste", json!({}))).is_err());
    call(&mut s, "clipboard.copy", json!({}));
    let original = s.clipboard.clone().unwrap();
    for args in [
        json!({"x":2}),
        json!({"x":4096,"y":0}),
        json!({"x":0.5,"y":0}),
    ] {
        assert!(s.apply(Action::new("clipboard.paste", args)).is_err());
    }
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    assert!(s.apply(Action::new("clipboard.cut", json!({}))).is_err());
    call(&mut s, "selection.all", json!({}));
    call(&mut s, "selection.invert", json!({}));
    assert!(s.apply(Action::new("clipboard.copy", json!({}))).is_err());
    assert!(std::sync::Arc::ptr_eq(
        s.clipboard.as_ref().unwrap(),
        &original
    ));
    assert_eq!(s.doc.layers.len(), 1);
}
#[test]
fn clipboard_center_and_explicit_clipped_placement_are_exact() {
    let mut s = session();
    s.doc.layers[0].pixels.fill(0xff123456);
    rectangle(&mut s);
    call(&mut s, "clipboard.copy", json!({}));
    call(&mut s, "selection.deselect", json!({}));
    let result = call(&mut s, "clipboard.paste", json!({}));
    assert_eq!(
        (result["x"].as_i64(), result["y"].as_i64()),
        (Some(3), Some(2))
    );
    call(&mut s, "clipboard.paste", json!({"x":-4,"y":7}));
    assert_eq!(
        s.doc.layers[2].pixels.iter().filter(|p| **p != 0).count(),
        1
    );
    assert_eq!(s.doc.layers[2].pixels[84], 0xff123456);
    check(&mut s);
}

#[test]
fn clipboard_merged_sampling_and_cross_document_placement_preserve_raw_contract() {
    let mut s = session();
    s.doc.layers[0].pixels.fill(0xff123456);
    call(&mut s, "layer.set", json!({"visible":false}));
    call(&mut s, "clipboard.copy", json!({}));
    let raw = call(&mut s, "clipboard.state", json!({}));
    call(&mut s, "clipboard.copy_merged", json!({}));
    let merged = call(&mut s, "clipboard.state", json!({}));
    assert_ne!(raw["sha256"], merged["sha256"]);
    call(&mut s, "clipboard.paste", json!({}));
    assert!(s.doc.layers[1].pixels.iter().all(|p| *p >> 24 == 0));
    call(&mut s, "layer.select", json!({"index":0}));
    rectangle(&mut s);
    call(&mut s, "clipboard.copy", json!({}));
    call(&mut s, "canvas.new", json!({"width":8,"height":6}));
    call(&mut s, "clipboard.paste_in_place", json!({}));
    assert_eq!(
        s.doc.layers[1]
            .pixels
            .iter()
            .filter(|p| **p == 0xff123456)
            .count(),
        15
    );
    check(&mut s);
    let path = std::env::temp_dir().join(format!("micopi-clipboard-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.layers[1].pixels, s.doc.layers[1].pixels);
    assert_eq!(
        call(&mut reopened, "clipboard.state", json!({}))["available"],
        false
    );
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn clipboard_layer_limit_and_free_graph_reject_without_document_changes() {
    let mut s = session();
    call(&mut s, "clipboard.copy", json!({}));
    let mut graph = s.doc.graph.clone();
    graph.linked = false;
    s.doc.set_graph(graph).unwrap();
    let before = s.doc.composition();
    let history = s.doc.history_bytes();
    assert!(s.apply(Action::new("clipboard.paste", json!({}))).is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.history_bytes(), history);
    call(&mut s, "canvas.new", json!({"width":12,"height":8}));
    for _ in 1..crate::engine::MAX_LAYERS {
        call(&mut s, "layer.add", json!({}));
    }
    let before = s.doc.composition();
    let history = s.doc.history_bytes();
    assert!(s.apply(Action::new("clipboard.paste", json!({}))).is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.history_bytes(), history);
}
#[test]
fn clipboard_nodes_are_discoverable_and_editing_does_not_copy_or_paste() {
    let mut s = session();
    let catalog = call(&mut s, "node.catalog", json!({}));
    assert!(catalog.to_string().contains("action.clipboard.paste"));
    assert!(catalog.to_string().contains("state.core.clipboard"));
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.clipboard.copy","position":[0,0],"parameters":{}},
        {"id":2,"type":"action.clipboard.paste","position":[400,0],"parameters":{"x":0,"y":0}}
    ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert!(s.clipboard.is_none());
    assert_eq!(s.doc.layers.len(), 1);
    let result = call(&mut s, "workflow.run", json!({}));
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.layers.len(), 2);
    check(&mut s);
}

#[test]
fn clipboard_journal_write_failure_restores_resource_pixels_and_structural_history() {
    for id in ["clipboard.copy", "clipboard.cut", "clipboard.paste"] {
        let mut s = session();
        call(&mut s, "native.checkerboard", json!({"cell":2}));
        call(&mut s, "clipboard.copy", json!({}));
        rectangle(&mut s);
        let payload = s.clipboard.clone().unwrap();
        let before = s.state();
        let composition = s.doc.composition();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        let bytes = s.journal.as_ref().unwrap().status()["bytes"].clone();
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        assert!(s.apply(Action::new(id, json!({}))).is_err(), "{id}");
        assert!(std::sync::Arc::ptr_eq(
            s.clipboard.as_ref().unwrap(),
            &payload
        ));
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().status()["bytes"], bytes);
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
    }
}
