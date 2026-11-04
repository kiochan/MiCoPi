//! Real compositor merges must preserve history, graph ownership and recorded outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::graph::{BlendMode, Edge, Filter, Node, NodeKind};
use serde_json::json;

fn fill(s: &mut Session, color: &str) {
    call(s, "layer.fill_color", json!({"color":color}));
}
fn filter(s: &mut Session, index: usize) -> u32 {
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|n| n.data == NodeKind::Layer { index })
        .unwrap()
        .id;
    let id = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = id;
        }
    }
    graph.edges.push(Edge {
        from: source,
        to: id,
        input: 0,
    });
    graph.nodes.push(Node {
        id,
        position: [260., 0.],
        node_type: Some("compositing.filter".into()),
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 0.7,
        },
    });
    call(s, "graph.set", json!({"graph":graph}));
    id
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none());
    s.doc.composite.clone()
}

#[test]
fn visible_merge_bakes_filters_once_preserves_hidden_buffers_and_undo_redo() {
    let mut s = session();
    fill(&mut s, "#10204080");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#12aa7790");
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true,"blend_mode":"screen"}),
    );
    let hidden = s.doc.layer_id(1);
    let hidden_pixels = s.doc.layers[1].pixels.clone();
    let pointer = s.doc.layers[1].pixels.as_ptr();
    let hidden_filter = filter(&mut s, 1);
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#e0803066");
    call(
        &mut s,
        "layer.set",
        json!({"blend_mode":"multiply","opacity":0.63}),
    );
    let baked_filter = filter(&mut s, 2);
    let target = s.doc.layer_id(2);
    let before = s.doc.composition();
    let expected = pixels(&mut s);
    call(&mut s, "layer.merge_visible", json!({}));
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.layer_id(s.doc.active), target);
    assert_eq!(
        s.doc.layers[s.doc.layer_index(hidden).unwrap()].pixels,
        hidden_pixels
    );
    assert_eq!(
        s.doc.layers[s.doc.layer_index(hidden).unwrap()]
            .pixels
            .as_ptr(),
        pointer
    );
    assert!(s.doc.graph.nodes.iter().any(|n| n.id == hidden_filter));
    assert!(!s.doc.graph.nodes.iter().any(|n| n.id == baked_filter));
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.active, 2);
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.active, 1);
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
}

#[test]
fn merge_down_bakes_a_two_layer_stack_and_rejects_unsafe_targets() {
    let mut s = session();
    fill(&mut s, "#204080a0");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#d0805060");
    call(
        &mut s,
        "layer.set",
        json!({"opacity":0.6,"blend_mode":"screen"}),
    );
    filter(&mut s, 1);
    let target = s.doc.layer_id(0);
    let removed = s.doc.layer_id(1);
    let original = s.doc.composition();
    let expected = pixels(&mut s);
    call(&mut s, "layer.merge_down", json!({"layer_id":removed}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layer_id(0), target);
    assert_eq!(s.doc.layers[0].opacity, 255);
    assert_eq!(s.doc.layers[0].mode, BlendMode::Normal);
    assert!(!s.doc.graph.has_filters());
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), original);
    check(&mut s);
    for args in [
        json!({"index":0}),
        json!({"layer_id":999999}),
        json!({"index":0,"layer_id":removed}),
    ] {
        let before = s.doc.composition();
        assert!(s.apply(Action::new("layer.merge_down", args)).is_err());
        assert_eq!(s.doc.composition(), before);
    }
    call(&mut s, "layer.set", json!({"index":0,"alpha_locked":true}));
    assert!(s.apply(Action::new("layer.merge_down", json!({}))).is_err());
    call(&mut s, "layer.set", json!({"index":0,"alpha_locked":false}));
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#ffffff80");
    call(&mut s, "layer.set", json!({"blend_mode":"multiply"}));
    assert!(s.apply(Action::new("layer.merge_down", json!({}))).is_err());
    check(&mut s);
}

#[test]
fn flatten_discards_hidden_layers_and_free_graph_but_restores_both_on_undo() {
    let mut s = session();
    fill(&mut s, "#33448880");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#ff804080");
    call(&mut s, "layer.set", json!({"visible":false}));
    let mut graph = s.doc.graph.clone();
    graph.linked = false;
    call(&mut s, "graph.set", json!({"graph":graph}));
    let before = s.doc.composition();
    let expected = pixels(&mut s);
    assert!(s
        .apply(Action::new("layer.merge_visible", json!({})))
        .is_err());
    assert!(s
        .apply(Action::new("layer.stamp_visible", json!({})))
        .is_err());
    call(&mut s, "layer.flatten", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    assert!(s.doc.graph.linked);
    assert!(s.doc.layers[0].pixels.iter().all(|p| p >> 24 == 128));
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.active, 1);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    check(&mut s);
}

#[test]
fn stamp_creates_one_undoable_layer_without_modifying_sources_and_checks_limits() {
    let mut s = session();
    fill(&mut s, "#123456");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#ff804088");
    let expected = pixels(&mut s);
    let originals: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let ids: Vec<_> = (0..2).map(|i| s.doc.layer_id(i)).collect();
    call(&mut s, "layer.stamp_visible", json!({}));
    let stamp = s.doc.layer_id(2);
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.doc.active, 2);
    assert_eq!(s.doc.layers[2].pixels, expected);
    for i in 0..2 {
        assert_eq!(s.doc.layer_id(i), ids[i]);
        assert_eq!(s.doc.layers[i].pixels, originals[i]);
    }
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.active, 1);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layer_id(2), stamp);
    check(&mut s);
    while s.doc.layers.len() < 16 {
        call(&mut s, "layer.add", json!({}));
    }
    assert!(s
        .apply(Action::new("layer.stamp_visible", json!({})))
        .is_err());
    check(&mut s);
}

#[test]
fn merge_failures_and_atomic_batch_failure_restore_document_history_and_pixels() {
    let mut s = session();
    fill(&mut s, "#012345");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#aabbcc80");
    let before = s.doc.composition();
    let expected = pixels(&mut s);
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let result = s.apply(Action::new(
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"merge","action":"layer.merge_visible"},
            {"id":"fail","action":"layer.select","args":{"layer_id":999999}}
        ]}),
    ));
    let result = result.unwrap();
    assert_eq!(result["result"]["ok"], false);
    assert_eq!(result["result"]["rolled_back"], true);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    let renderer = s.doc.renderer.take();
    assert!(s
        .apply(Action::new("layer.merge_visible", json!({})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    s.doc.renderer = renderer;
    check(&mut s);
    struct FailsAfterWriting;
    impl crate::engine::graph::Renderer for FailsAfterWriting {
        fn render(
            &self,
            _: &[micopi_plugin_api::CompositeNode],
            _: &[micopi_plugin_api::CompositeLayer],
            _: u32,
            _: u32,
            out: &mut micopi_plugin_api::Pixels,
        ) -> Result<(), String> {
            if out.len > 0 {
                unsafe {
                    *out.data = 0xffffffff;
                }
            }
            Err("Injected compositor failure".into())
        }
    }
    let renderer = s
        .doc
        .renderer
        .replace(std::sync::Arc::new(FailsAfterWriting));
    assert!(s
        .apply(Action::new("layer.merge_visible", json!({})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    s.doc.renderer = renderer;
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[1].pixels.iter().all(|p| *p == 0));
    check(&mut s);
}

#[test]
fn merged_projects_reopen_and_replay_without_loading_the_compositor() {
    let mut s = session();
    fill(&mut s, "#123456");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#ff000080");
    call(&mut s, "layer.merge_visible", json!({}));
    let expected = pixels(&mut s);
    let composition = s.doc.composition();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/merge");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("merged.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    call(&mut s, "document.open", json!({"path":path}));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
    let last = s.replay_journal(false).unwrap().entries.len() - 1;
    let state = s.replay_preview(last, false).unwrap();
    let mut restored = crate::engine::Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    restored.compose();
    assert_eq!(restored.composite, expected);
}
