use super::*;
use crate::{
    engine::{
        graph::{BlendMode, Edge, Filter, Graph, Node, NodeKind},
        Document,
    },
    plugins::Registry,
};
use serde_json::{json, Value};
fn fresh(w: usize, h: usize) -> Session {
    let mut s = Session::new(Registry::bundled()).unwrap();
    s.apply(Action::new("canvas.new", json!({"width":w,"height":h})))
        .unwrap();
    assert!(s.doc.renderer.is_some(), "build compositing DLL first");
    s
}
fn filtered(filter: Filter, amount: f32) -> Graph {
    Graph {
        enabled: true,
        linked: false,
        nodes: vec![
            Node {
                node_type: None,
                id: 10,
                position: [0., 0.],
                data: NodeKind::Layer { index: 0 },
            },
            Node {
                node_type: None,
                id: 20,
                position: [200., 0.],
                data: NodeKind::Filter { filter, amount },
            },
            Node {
                node_type: None,
                id: 30,
                position: [400., 0.],
                data: NodeKind::Output,
            },
        ],
        edges: vec![
            Edge {
                from: 10,
                to: 20,
                input: 0,
            },
            Edge {
                from: 20,
                to: 30,
                input: 0,
            },
        ],
    }
}
fn apply(s: &mut Session, graph: &Graph) {
    s.apply(Action::new("graph.set", json!({"graph":graph})))
        .unwrap();
}
#[test]
fn graph_roundtrips_json_and_filters_are_non_destructive_and_undoable() {
    let mut s = fresh(16, 16);
    s.doc.layers[0].pixels.fill(0x804477aa);
    s.doc.invalidate();
    let graph = filtered(Filter::Invert, 1.);
    let decoded: Graph = serde_json::from_value(json!(graph)).unwrap();
    assert_eq!(decoded, graph);
    assert_eq!(graph.compile(1).unwrap().len(), 3);
    apply(&mut s, &graph);
    s.doc.compose();
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0x804477aa));
    assert!(s.doc.composite.iter().all(|p| *p == 0x80bb8855));
    assert!(s.doc.history_bytes() > 0);
    s.doc.undo();
    s.doc.compose();
    assert!(s.doc.composite.iter().all(|p| *p == 0x804477aa));
    s.doc.redo();
    s.doc.compose();
    assert!(s.doc.composite.iter().all(|p| *p == 0x80bb8855));
}
#[test]
fn layer_and_node_blending_use_identical_dll_math_for_all_modes() {
    let expected = [
        0xff4080c0, 0xff202018, 0xffa0a0c8, 0xff414030, 0xff404020, 0xff8080c0, 0xffc0c0e0,
        0xff4040a0,
    ];
    for (mode, expected) in BlendMode::LEGACY.into_iter().zip(expected) {
        let mut s = fresh(2, 2);
        s.doc.layers[0].pixels.fill(0xff804020);
        s.doc.add_layer();
        s.doc.layers[1].pixels.fill(0xff4080c0);
        s.apply(Action::new(
            "layer.set",
            json!({"index":1,"blend_mode":mode}),
        ))
        .unwrap();
        s.doc.invalidate();
        s.doc.compose();
        assert_eq!(s.doc.composite[0], expected, "{mode:?}");
        let graph = Graph::from_layers(&s.doc.layers);
        apply(&mut s, &graph);
        s.doc.compose();
        assert_eq!(s.doc.composite[0], expected);
    }
}
#[test]
fn transparency_opacity_visibility_and_parameter_changes_preserve_sources() {
    let mut s = fresh(4, 4);
    s.doc.layers[0].pixels.fill(0x00337799);
    s.doc.add_layer();
    s.doc.layers[1].pixels.fill(0x804080c0);
    s.apply(Action::new(
        "layer.set",
        json!({"index":1,"opacity":0.5,"blend_mode":"multiply"}),
    ))
    .unwrap();
    s.doc.compose();
    // On transparent backdrop the original source color survives any blend mode.
    assert_eq!(s.doc.composite[0], 0x404080c0);
    let graph = Graph::from_layers(&s.doc.layers);
    apply(&mut s, &graph);
    s.doc.compose();
    assert_eq!(s.doc.composite[0], 0x404080c0);
    s.apply(Action::new("layer.set", json!({"index":1,"visible":false})))
        .unwrap();
    s.doc.compose();
    assert_eq!(s.doc.composite[0] >> 24, 0);
    s.doc.undo();
    s.doc.compose();
    assert_eq!(s.doc.composite[0], 0x404080c0);
    assert_eq!(s.doc.layers[1].pixels[0], 0x804080c0);
}
#[test]
fn invalid_graphs_fail_before_finishing_a_stroke_or_changing_the_document() {
    let mut s = fresh(64, 64);
    let before = s.doc.graph.clone();
    s.pointer(Pointer {
        phase: PointerPhase::Down,
        x: 30.,
        y: 30.,
        pressure: 1.,
        time: 1,
        pen: false,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    })
    .unwrap();
    let revision = s.revision;
    let mut invalid = filtered(Filter::Invert, 1.);
    invalid.edges[0].from = 999;
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":invalid})))
            .unwrap_err()
            .code,
        "invalid_graph"
    );
    assert!(s.is_drawing());
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.graph, before);
    let mut cycle = filtered(Filter::Invert, 1.);
    cycle.edges[0].from = 20;
    assert!(cycle.compile(1).is_err());
    let mut missing = filtered(Filter::Invert, 1.);
    missing.edges.remove(0);
    assert!(missing.compile(1).is_err());
    let mut multi = filtered(Filter::Invert, 1.);
    multi.edges.push(multi.edges[0].clone());
    assert!(multi.compile(1).is_err());
    let mut bad = filtered(Filter::Brightness, 2.);
    assert!(bad.compile(1).is_err());
    bad.nodes[0].position[0] = f32::NAN;
    assert!(bad.compile(1).is_err());
    s.finish_stroke();
}
#[test]
fn projects_persist_nodes_positions_modes_and_sources_and_load_legacy_files() {
    let mut s = fresh(3, 2);
    s.doc.layers[0].pixels.fill(0x80aabbcc);
    let graph = filtered(Filter::Invert, 0.75);
    s.apply(Action::new(
        "layer.set",
        json!({"index":0,"blend_mode":"screen"}),
    ))
    .unwrap();
    apply(&mut s, &graph);
    let graph = s.doc.graph.clone();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("nodes-{}.mcp", std::process::id()));
    s.apply(Action::new("document.save", json!({"path":path})))
        .unwrap();
    let mut second = fresh(1, 1);
    second
        .apply(Action::new("document.open", json!({"path":path})))
        .unwrap();
    assert_eq!(second.doc.graph, graph);
    assert_eq!(second.doc.layers[0].mode, BlendMode::Screen);
    assert_eq!(second.doc.layers[0].pixels, s.doc.layers[0].pixels);
    second.doc.compose();
    s.doc.compose();
    assert_eq!(second.doc.composite, s.doc.composite);
    let legacy = dir.join(format!("legacy-nodes-{}.mcp", std::process::id()));
    let mut bytes = b"MICOPI01".to_vec();
    for n in [1u32, 1, 1, 0] {
        bytes.extend_from_slice(&n.to_le_bytes());
    }
    bytes.extend([1, 255]);
    bytes.extend_from_slice(&0x80aa3355u32.to_le_bytes());
    std::fs::write(&legacy, &bytes).unwrap();
    let doc = Document::load(&legacy).unwrap();
    assert_eq!(doc.layers[0].pixels[0], 0x80aa3355);
    assert!(!doc.graph.enabled);
    let mut bad = std::fs::read(&path).unwrap();
    bad.push(0);
    std::fs::write(&path, bad).unwrap();
    let identity = second.doc.identity;
    assert!(second
        .apply(Action::new("document.open", json!({"path":path})))
        .is_err());
    assert_eq!(second.doc.identity, identity);
}
#[test]
fn all_filters_have_expected_endpoints_and_transparent_pixels_stay_canonical() {
    for (filter, amount, expected) in [
        (Filter::Invert, 0., 0xff4080c0),
        (Filter::Grayscale, 1., 0xff777777),
        (Filter::Brightness, 0.25, 0xff80c0ff),
        (Filter::Contrast, 1., 0xff0082ff),
        (Filter::Saturation, -1., 0xff777777),
    ] {
        let mut s = fresh(2, 1);
        s.doc.layers[0].pixels = vec![0xff4080c0, 0x00334455];
        apply(&mut s, &filtered(filter, amount));
        s.doc.compose();
        assert_eq!(s.doc.composite[0], expected, "{filter:?}");
        assert_eq!(s.doc.composite[1], 0);
    }
}
#[test]
fn enabled_graph_keeps_incremental_damage_and_missing_renderer_is_explicit() {
    let mut s = fresh(512, 512);
    let graph = filtered(Filter::Grayscale, 1.);
    apply(&mut s, &graph);
    let mut display = vec![];
    s.doc.update_display(&mut display);
    s.pointer(Pointer {
        phase: PointerPhase::Down,
        x: 100.,
        y: 100.,
        pressure: 0.5,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    })
    .unwrap();
    s.finish_stroke();
    let r = s.doc.update_display(&mut display).unwrap();
    assert!(r[2] - r[0] < 32 && r[3] - r[1] < 32);
    assert!(s.doc.render_error.is_none());
    let mut blank = fresh(2, 2);
    blank.doc.renderer = None;
    let before = blank.doc.graph.clone();
    assert!(blank
        .apply(Action::new("graph.set", json!({"graph":graph})))
        .is_err());
    assert_eq!(blank.doc.graph, before);
}
#[test]
fn graph_actions_and_public_resources_work_inside_the_unified_pipeline() {
    let mut s = fresh(2, 2);
    s.doc.layers[0].pixels.fill(0xff112233);
    let graph = filtered(Filter::Invert, 1.);
    let result=s.apply(Action::new("pipeline.run",json!({"steps":[{"id":"apply","action":"graph.set","args":{"graph":graph}},{"id":"inspect","action":"context.read","args":{"resources":["document.nodes","compositing.graph"]}}]}))).unwrap();
    assert_eq!(result["result"]["ok"], true);
    assert_eq!(
        result["result"]["results"]["inspect"]["result"]["resources"]["document.nodes"]["value"]
            ["graph"]["enabled"],
        true
    );
    let catalog = s.apply(Action::new("graph.catalog", json!({}))).unwrap();
    assert_eq!(catalog["result"]["blend_modes"], json!(BlendMode::ALL));
    assert_eq!(
        catalog["result"]["available_blend_modes"],
        json!(BlendMode::MENU)
    );
    let _typed: Value = json!(s.doc.graph);
}
