//! Linked stacks must agree with ordinary layers across structural edits and persisted outcomes.
use super::*;
use crate::{
    engine::{
        graph::{BlendMode, Edge, Filter, Graph, Node, NodeKind},
        replay::Journal,
        Document,
    },
    plugins::Registry,
};
use serde_json::{json, Value};
fn session() -> Session {
    let registry =
        Registry::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    let mut s = Session::new(registry).unwrap();
    call(&mut s, "canvas.new", json!({"width":7,"height":5}));
    assert!(s.doc.renderer.is_some());
    s
}
fn call(s: &mut Session, action: &str, args: Value) -> Value {
    s.apply(Action::new(action, args)).unwrap()["result"].clone()
}
fn fill(s: &mut Session, color: &str) {
    call(s, "layer.fill", json!({"x":0,"y":0,"color":color}));
}
fn add_filter(graph: &mut Graph, index: usize, filter: Filter, amount: f32) -> u32 {
    let source = graph
        .nodes
        .iter()
        .find(|n| n.data == (NodeKind::Layer { index }))
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
        node_type: Some("compositing.filter".into()),
        position: [260., 0.],
        data: NodeKind::Filter { filter, amount },
    });
    id
}
fn verify(s: &mut Session) {
    assert_eq!(call(s, "replay.verify", json!({}))["valid"], true);
    let last = s.replay_journal(false).unwrap().entries.len() - 1;
    let expected = crate::application::codec::png(&mut s.doc).unwrap();
    assert_eq!(
        replay::png(&s.replay_preview(last, false).unwrap()).unwrap(),
        expected
    );
    s.doc.composition().validate_layer_metadata().unwrap();
}
#[test]
fn linked_layers_appear_and_reorder_without_rebuilding_and_undo_replays_exactly() {
    let mut s = session();
    assert!(s.doc.graph.linked);
    fill(&mut s, "#ff0000");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#0000ff");
    s.doc.compose();
    assert!(s.doc.composite.iter().all(|p| *p == 0xff0000ff));
    let before = s.doc.composition();
    call(&mut s, "layer.move", json!({"index":0,"to":1}));
    s.doc.compose();
    assert!(s.doc.composite.iter().all(|p| *p == 0xffff0000));
    verify(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    verify(&mut s);
    call(&mut s, "history.redo", json!({}));
    verify(&mut s);
}
#[test]
fn per_layer_filters_follow_identity_duplicate_and_delete_with_pixels_intact() {
    let mut s = session();
    fill(&mut s, "#ff0000");
    let red_id = s.doc.layer_id(0);
    let raw = s.doc.layers[0].pixels.clone();
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#0000ff");
    let mut graph = s.doc.graph.clone();
    let filter_id = add_filter(&mut graph, 0, Filter::Invert, 1.);
    call(&mut s, "graph.set", json!({"graph":graph}));
    call(&mut s, "layer.move", json!({"layer_id":red_id,"to":1}));
    s.doc.compose();
    assert!(s.doc.composite.iter().all(|p| *p == 0xff00ffff));
    assert_eq!(s.doc.layers[1].pixels, raw);
    assert_eq!(
        s.doc.graph.stack_branches(2).unwrap()[1].filters[0].id,
        filter_id
    );
    call(&mut s, "layer.duplicate", json!({"layer_id":red_id}));
    let copied = s.doc.layer_id(s.doc.active);
    let branches = s.doc.graph.stack_branches(3).unwrap();
    assert_eq!(branches[2].filters[0].data, branches[1].filters[0].data);
    assert_ne!(branches[2].filters[0].id, filter_id);
    call(&mut s, "layer.delete", json!({"layer_id":red_id}));
    assert_eq!(s.doc.layer_index(copied), Some(1));
    assert_eq!(s.doc.graph.stack_branches(2).unwrap()[1].filters.len(), 1);
    verify(&mut s);
    call(&mut s, "history.undo", json!({}));
    verify(&mut s);
    assert_eq!(
        s.doc.graph.stack_branches(3).unwrap()[1].filters[0].id,
        filter_id
    );
}
#[test]
fn linked_blend_and_layer_properties_share_modes_without_double_opacity() {
    let mut s = session();
    fill(&mut s, "#804020");
    call(&mut s, "layer.add", json!({}));
    fill(&mut s, "#4080c0");
    let mut graph = s.doc.graph.clone();
    for node in &mut graph.nodes {
        if let NodeKind::Blend { mode, .. } = &mut node.data {
            *mode = BlendMode::Multiply;
        }
    }
    call(&mut s, "graph.set", json!({"graph":graph}));
    assert_eq!(s.doc.layers[1].mode, BlendMode::Multiply);
    call(
        &mut s,
        "layer.set",
        json!({"index":1,"blend_mode":"screen","opacity":0.5}),
    );
    assert!(matches!(
        s.doc.graph.stack_branches(2).unwrap()[1]
            .blend
            .as_ref()
            .unwrap()
            .data,
        NodeKind::Blend {
            mode: BlendMode::Screen,
            opacity: 1.
        }
    ));
    // Force the graph path with a neutral filter and compare to the ordinary stack path.
    s.doc.compose();
    let expected = s.doc.composite.clone();
    let mut graph = s.doc.graph.clone();
    add_filter(&mut graph, 1, Filter::Brightness, 0.);
    call(&mut s, "graph.set", json!({"graph":graph}));
    s.doc.compose();
    assert_eq!(s.doc.composite, expected);
    call(&mut s, "layer.set", json!({"index":1,"visible":false}));
    s.doc.compose();
    assert_eq!(s.doc.composite[0], 0xff804020);
    verify(&mut s);
}
#[test]
fn neutral_linked_filter_matches_stack_math_for_all_modes_and_alpha_edges() {
    for mode in BlendMode::ALL {
        let mut s = session();
        call(&mut s, "layer.add", json!({}));
        for (i, layer) in s.doc.layers.iter_mut().enumerate() {
            for (j, p) in layer.pixels.iter_mut().enumerate() {
                *p = (j as u32)
                    .wrapping_mul(0x794bad91)
                    .wrapping_add(i as u32 * 0x5f327a91);
            }
        }
        s.doc.set_layer_properties(1, true, 137, mode).unwrap();
        s.doc.invalidate();
        s.doc.compose();
        let expected = s.doc.composite.clone();
        let mut graph = s.doc.graph.clone();
        add_filter(&mut graph, 1, Filter::Brightness, 0.);
        s.doc.set_graph(graph).unwrap();
        s.doc.compose();
        assert_eq!(s.doc.composite, expected, "{mode:?}");
    }
}
#[test]
fn invalid_linked_topology_is_rejected_and_legacy_free_graphs_keep_their_meaning() {
    let mut s = session();
    let mut subset = s.doc.graph.clone();
    call(&mut s, "layer.add", json!({}));
    let before = s.doc.composition();
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":subset})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    let mut doubled = s.doc.graph.clone();
    for n in &mut doubled.nodes {
        if let NodeKind::Blend { opacity, .. } = &mut n.data {
            *opacity = 0.5;
        }
    }
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":doubled})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    subset.linked = false;
    let legacy = serde_json::to_value(&subset).unwrap();
    assert!(legacy.get("linked").is_none());
    call(&mut s, "graph.set", json!({"graph":legacy}));
    assert!(!s.doc.graph.linked);
    call(&mut s, "layer.add", json!({}));
    assert_eq!(s.doc.graph.nodes.len(), 2);
    assert!(!s.doc.layer_is_rendered(2));
    verify(&mut s);
}
#[test]
fn linked_node_budget_and_failed_atomic_edits_leave_the_whole_stack_unchanged() {
    let mut s = session();
    let mut graph = s.doc.graph.clone();
    for _ in 0..62 {
        add_filter(&mut graph, 0, Filter::Brightness, 0.);
    }
    call(&mut s, "graph.set", json!({"graph":graph}));
    let before = s.doc.composition();
    assert!(s.apply(Action::new("layer.add", json!({}))).is_err());
    assert!(s.apply(Action::new("layer.duplicate", json!({}))).is_err());
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.composition(), before);
    let mut s = session();
    let before = s.doc.composition();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"add","action":"layer.add"}, {"id":"mode","action":"layer.set","args":{"blend_mode":"multiply"}},
            {"id":"fail","action":"layer.move","args":{"to":12}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    verify(&mut s);
}
#[test]
fn linked_projects_and_archives_preserve_filters_identity_and_verified_outcomes() {
    let mut s = session();
    fill(&mut s, "#336699");
    let mut graph = s.doc.graph.clone();
    add_filter(&mut graph, 0, Filter::Invert, 1.);
    call(&mut s, "graph.set", json!({"graph":graph}));
    call(&mut s, "layer.duplicate", json!({}));
    call(&mut s, "layer.set", json!({"opacity":0.3}));
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/linked-graphs");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("linked.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.composition(), s.doc.composition());
    let mut archive = Journal::import(loaded.replay_data.as_ref().unwrap()).unwrap();
    assert_eq!(archive.verify(None).unwrap()["valid"], true);
    assert_eq!(
        replay::png(&archive.state).unwrap(),
        codec::png(&mut s.doc).unwrap()
    );
    call(&mut s, "document.open", json!({"path":path}));
    call(&mut s, "layer.add", json!({}));
    assert_eq!(s.doc.graph.stack_branches(3).unwrap().len(), 3);
    verify(&mut s);
}
