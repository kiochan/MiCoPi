//! Selection extraction uses the real painting DLL and shared history/replay contracts.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn select(s: &mut Session) {
    call(
        s,
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":3}),
    );
}
#[test]
fn selected_copy_and_cut_preserve_clipboard_and_have_single_structural_undo() {
    for id in ["layer.via_copy", "layer.via_cut"] {
        let mut s = session();
        call(&mut s, "native.checkerboard", json!({"cell":3}));
        call(&mut s, "clipboard.copy", json!({}));
        let clipboard = s.clipboard.clone().unwrap();
        select(&mut s);
        let original = s.doc.layers[0].pixels.clone();
        let selection = s.doc.selection().clone();
        let events = s.replay_journal(false).unwrap().entries.len();
        let source_id = s.doc.layer_id(0);
        call(&mut s, id, json!({}));
        assert_eq!(s.doc.layers.len(), 2);
        assert_eq!(s.doc.active, 1);
        assert_eq!(s.doc.layer_id(0), source_id);
        assert_ne!(s.doc.layer_id(1), source_id);
        assert!(s.doc.selection().current.is_none());
        assert_eq!(s.doc.selection().previous, selection.current);
        assert!(std::sync::Arc::ptr_eq(
            s.clipboard.as_ref().unwrap(),
            &clipboard
        ));
        assert_eq!(s.replay_journal(false).unwrap().entries.len(), events + 1);
        let expected = s.doc.layers[1].pixels.clone();
        for y in 0..8 {
            for x in 0..12 {
                let selected = (3..8).contains(&x) && (2..5).contains(&y);
                assert_eq!(
                    expected[y * 12 + x],
                    if selected { original[y * 12 + x] } else { 0 }
                );
                assert_eq!(
                    s.doc.layers[0].pixels[y * 12 + x],
                    if selected && id == "layer.via_cut" {
                        0
                    } else {
                        original[y * 12 + x]
                    }
                );
            }
        }
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers.len(), 1);
        assert_eq!(s.doc.active, 0);
        assert_eq!(s.doc.layers[0].pixels, original);
        assert_eq!(s.doc.selection(), &selection);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers.len(), 2);
        assert_eq!(s.doc.layers[1].pixels, expected);
        s.clipboard = None;
        check(&mut s);
    }
}
#[test]
fn fractional_cut_applies_coverage_once_to_each_output() {
    let mut s = session();
    s.doc.layers[0].pixels.fill(0x80402010);
    let mut mask = vec![0; 96];
    mask[27] = 128;
    mask[28] = 255;
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(crate::engine::selection::Mask::new(12, 8, mask).unwrap()),
            previous: None,
        })
        .unwrap();
    call(&mut s, "layer.via_cut", json!({}));
    assert_eq!(s.doc.layers[0].pixels[27], 0x40402010);
    assert_eq!(s.doc.layers[0].pixels[28], 0);
    assert_eq!(s.doc.layers[1].pixels[27], 0x40402010);
    assert_eq!(s.doc.layers[1].pixels[28], 0x80402010);
    assert!(s.clipboard.is_none());
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0x80402010));
    check(&mut s);
}
#[test]
fn whole_layer_copy_retains_existing_properties_and_cut_requires_selection() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "layer.rename", json!({"name":"Ink 墨"}));
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"opacity":0.5,"alpha_locked":true,"blend_mode":"multiply"}),
    );
    let pixels = s.doc.layers[0].pixels.clone();
    let source = s.doc.composition();
    call(&mut s, "layer.via_copy", json!({}));
    assert_eq!(s.doc.layers[1].pixels, pixels);
    assert_eq!(s.doc.custom_layer_name(1), "Ink 墨");
    assert!(s.doc.alpha_locked(1));
    assert_eq!(s.doc.layers[1].opacity, s.doc.layers[0].opacity);
    assert_eq!(s.doc.layers[1].visible, s.doc.layers[0].visible);
    assert_eq!(s.doc.layers[1].mode, s.doc.layers[0].mode);
    let before = s.state();
    assert!(s.apply(Action::new("layer.via_cut", json!({}))).is_err());
    assert_eq!(s.state(), before);
    call(&mut s, "history.undo", json!({}));
    let restored = s.doc.composition();
    assert_eq!(restored.graph, source.graph);
    assert_eq!(restored.layer_metadata, source.layer_metadata);
    check(&mut s);
}
#[test]
fn failures_preserve_pixels_masks_history_and_selection_state() {
    let mut s = session();
    for id in ["layer.via_copy", "layer.via_cut"] {
        select(&mut s);
        call(&mut s, "selection.invert", json!({}));
        call(&mut s, "selection.all", json!({}));
        call(&mut s, "selection.invert", json!({}));
        let before = s.state();
        let history = s.doc.history_bytes();
        assert!(s.apply(Action::new(id, json!({}))).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    select(&mut s);
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    let before = s.state();
    assert!(s.apply(Action::new("layer.via_cut", json!({}))).is_err());
    assert_eq!(s.state(), before);
    call(&mut s, "layer.via_copy", json!({}));
    assert!(!s.doc.alpha_locked(1));
    select(&mut s);
    let mut graph = s.doc.graph.clone();
    graph.linked = false;
    s.doc.set_graph(graph).unwrap();
    let before = s.state();
    let composition = s.doc.composition();
    for id in ["layer.via_copy", "layer.via_cut"] {
        assert!(s.apply(Action::new(id, json!({}))).is_err());
    }
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
}
#[test]
fn atomic_and_journal_failures_restore_both_sides_of_layer_cut() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    select(&mut s);
    let state = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"cut","action":"layer.via_cut","args":{}},
            {"id":"fail","action":"layer.select","args":{"layer_id":9999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    let bytes = s.journal.as_ref().unwrap().status()["bytes"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(8);
    assert!(s.apply(Action::new("layer.via_cut", json!({}))).is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().status()["bytes"], bytes);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}
#[test]
fn cut_across_tile_boundaries_in_middle_layer_restores_adjacent_layers() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":140,"height":130}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let original = s.doc.layers[1].pixels.clone();
    let source_id = s.doc.layer_id(1);
    call(&mut s, "layer.add", json!({}));
    let top_id = s.doc.layer_id(2);
    call(&mut s, "layer.select", json!({"index":1}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":60,"y":62,"width":75,"height":65}),
    );
    call(&mut s, "layer.via_cut", json!({}));
    let result = s.doc.layers[2].pixels.clone();
    assert_eq!(s.doc.layer_id(3), top_id);
    for _ in 0..3 {
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers.len(), 3);
        assert_eq!(s.doc.active, 1);
        assert_eq!(s.doc.layer_id(1), source_id);
        assert_eq!(s.doc.layer_id(2), top_id);
        assert_eq!(s.doc.layers[1].pixels, original);
        assert!(s.doc.layers[2].pixels.iter().all(|p| *p == 0));
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers.len(), 4);
        assert_eq!(s.doc.active, 2);
        assert_eq!(s.doc.layers[2].pixels, result);
        assert_eq!(s.doc.layer_id(3), top_id);
        check(&mut s);
    }
}

#[test]
fn selected_transfer_retains_source_filter_identity_and_roundtrips_project_outcomes() {
    use crate::engine::graph::{Edge, Filter, Node, NodeKind};
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let mut graph = s.doc.graph.clone();
    graph.nodes.push(Node {
        id: 2,
        position: [260., 0.],
        node_type: Some("compositing.filter".into()),
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 0.7,
        },
    });
    graph.edges = vec![
        Edge {
            from: 0,
            to: 2,
            input: 0,
        },
        Edge {
            from: 2,
            to: 1,
            input: 0,
        },
    ];
    call(&mut s, "graph.set", json!({"graph":graph}));
    select(&mut s);
    let source = s.doc.layers[0].pixels.clone();
    call(&mut s, "layer.via_cut", json!({}));
    assert!(s.doc.graph.nodes.iter().any(|n| n.id == 2
        && matches!(
            n.data,
            NodeKind::Filter {
                filter: Filter::Invert,
                ..
            }
        )));
    assert!(s.doc.graph.edges.iter().any(|e| e.from == 0 && e.to == 2));
    assert_eq!(s.doc.layers[1].pixels[27], source[27]);
    let metadata = s.doc.composition();
    let first = s.doc.layers[0].pixels.clone();
    let second = s.doc.layers[1].pixels.clone();
    let path = std::env::temp_dir().join(format!("micopi-via-layer-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), metadata);
    assert_eq!(reopened.doc.layers[0].pixels, first);
    assert_eq!(reopened.doc.layers[1].pixels, second);
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn layer_transfer_nodes_are_discoverable_and_never_execute_while_editing() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    select(&mut s);
    let pixels = s.doc.layers[0].pixels.clone();
    let selection = s.doc.selection().clone();
    let catalog = call(&mut s, "node.catalog", json!({}));
    for id in ["action.layer.via_copy", "action.layer.via_cut"] {
        assert!(catalog.to_string().contains(id));
    }
    let graph = json!({"version":1,"nodes":[{"id":1,"type":"action.layer.via_cut","position":[0.0,0.0],"parameters":{}}],"edges":[]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.selection(), &selection);
    let result = call(&mut s, "workflow.run", json!({"atomic":true}));
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.layers.len(), 2);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.selection(), &selection);
    assert_eq!(serde_json::to_value(&s.doc.workflow).unwrap(), graph);
    check(&mut s);
}
