//! Node observations are fenced read-only evaluations of published graph views.
use super::{
    editing_tests::{call, session},
    *,
};
use crate::engine::graph::{Edge, Filter, Graph, Node, NodeKind};
use serde_json::{json, Value};

fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":1}));
    call(&mut s, "layer.fill_color", json!({"color":"#204060"}));
    s
}
fn graph(enabled: bool) -> Graph {
    Graph {
        enabled,
        linked: true,
        nodes: vec![
            Node {
                id: 11,
                node_type: None,
                position: [0., 0.],
                data: NodeKind::Layer { index: 0 },
            },
            Node {
                id: 22,
                node_type: Some("compositing.filter".into()),
                position: [200., 0.],
                data: NodeKind::Filter {
                    filter: Filter::Invert,
                    amount: 1.,
                },
            },
            Node {
                id: 33,
                node_type: None,
                position: [400., 0.],
                data: NodeKind::Output,
            },
        ],
        edges: vec![
            Edge {
                from: 11,
                to: 22,
                input: 0,
            },
            Edge {
                from: 22,
                to: 33,
                input: 0,
            },
        ],
    }
}
fn args(s: &Session, view: &str, node: u32, port: &str, input: Option<u32>) -> Value {
    let mut args = json!({"source":"node","graph_view":view,"node_id":node,"port":port,"document_identity":s.doc.identity,"document_revision":s.revision});
    if let Some(index) = input {
        args["input_index"] = json!(index);
    }
    args
}
fn observe(s: &mut Session, view: &str, node: u32, port: &str, input: Option<u32>) -> Value {
    let query = args(s, view, node, port, input);
    call(s, "histogram.query", query)
}
fn id(graph: &Value, kind: &str) -> u32 {
    graph["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["kind"] == kind)
        .unwrap()["id"]
        .as_u64()
        .unwrap() as u32
}
fn counts(result: &Value, r: usize, g: usize, b: usize, count: u64) {
    assert_eq!(result["nontransparent_pixels"], count);
    for (name, bin) in [("red", r), ("green", g), ("blue", b)] {
        assert_eq!(
            result[name],
            json!((0..256)
                .map(|i| if i == bin { count } else { 0 })
                .collect::<Vec<_>>())
        );
    }
}
#[test]
fn image_input_output_queries_preserve_pixels_history_events_and_exact_public_state() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let before = s.state();
    let composition = s.doc.composition();
    let raw = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let entries = s.journal.as_ref().unwrap().entries.len();
    let input = observe(&mut s, "source", 22, "input", Some(0));
    counts(&input, 32, 64, 96, 4);
    assert_eq!(input["resolved_node_id"], 11);
    let output = observe(&mut s, "source", 22, "output", None);
    counts(&output, 223, 191, 159, 4);
    assert_eq!(output["resolved_node_id"], 22);
    assert_eq!(output["evaluation"], "source_declaration");
    assert_eq!(output["graph_enabled"], true);
    crate::plugins::validate(
        s.registry.actions["histogram.query"]
            .output_schema
            .as_ref()
            .unwrap(),
        &output,
    )
    .unwrap();
    let actual = observe(&mut s, "actual", 33, "output", None);
    counts(&actual, 223, 191, 159, 4);
    assert_eq!(actual["evaluation"], "actual_plan");
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), entries);
}
#[test]
fn disabled_source_declarations_and_actual_flat_composition_have_distinct_discoverable_ids() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(false)}));
    let read = call(&mut s, "graph.read", json!({}));
    assert_eq!(read["document_identity"], s.doc.identity);
    assert_eq!(read["revision"], s.revision);
    assert_eq!(read["histogram_views"]["source"]["enabled"], false);
    assert!(read["histogram_views"]["actual"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["kind"] != "filter"));
    let source = observe(&mut s, "source", 22, "output", None);
    counts(&source, 223, 191, 159, 4);
    assert_eq!(source["graph_enabled"], false);
    let actual_id = id(&read["histogram_views"]["actual"], "output");
    let actual = observe(&mut s, "actual", actual_id, "output", None);
    counts(&actual, 32, 64, 96, 4);
    assert_eq!(actual["graph_enabled"], true);
    assert_eq!(s.doc.composite, vec![0xff204060; 4]);
}
#[test]
fn mask_image_ports_differ_from_coverage_and_source_filters_exclude_generated_gates() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":0}));
    let read = call(&mut s, "graph.read", json!({}));
    let actual = &read["histogram_views"]["actual"];
    let apply = id(actual, "apply_mask");
    let coverage = id(actual, "mask_source");
    let output = id(actual, "output");
    counts(
        &observe(&mut s, "source", 22, "output", None),
        223,
        191,
        159,
        4,
    );
    counts(
        &observe(&mut s, "actual", apply, "input", Some(0)),
        223,
        191,
        159,
        4,
    );
    let hidden = observe(&mut s, "actual", output, "output", None);
    assert_eq!(hidden["nontransparent_pixels"], 0);
    assert_eq!(hidden["ignored_transparent_pixels"], 4);
    for query in [
        args(&s, "actual", apply, "input", Some(1)),
        args(&s, "actual", coverage, "output", None),
    ] {
        assert_eq!(
            s.apply(Action::new("histogram.query", query))
                .unwrap_err()
                .code,
            "histogram_node"
        );
    }
    assert!((0..4).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 0));
}
#[test]
fn pass_through_mix_inputs_observe_background_and_foreground_before_group_strength() {
    let mut s = fixture();
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#80C040"}));
    let group = call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"compositing":"pass_through","opacity":128./255.}),
    );
    let read = call(&mut s, "graph.read", json!({}));
    let mix = id(&read["histogram_views"]["actual"], "mix");
    counts(
        &observe(&mut s, "actual", mix, "input", Some(0)),
        32,
        64,
        96,
        4,
    );
    counts(
        &observe(&mut s, "actual", mix, "input", Some(1)),
        128,
        192,
        64,
        4,
    );
    counts(
        &observe(&mut s, "actual", mix, "output", None),
        80,
        128,
        80,
        4,
    );
}
#[test]
fn stale_and_invalid_selectors_reject_without_executing_a_pipeline_prefix() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let mut stale = args(&s, "source", 22, "output", None);
    call(&mut s, "layer.rename", json!({"name":"Current"}));
    let before = s.state();
    let history = s.doc.history_bytes();
    assert_eq!(
        s.apply(Action::new("histogram.query", stale.clone()))
            .unwrap_err()
            .code,
        "histogram_stale"
    );
    stale["document_revision"] = json!(s.revision);
    stale["document_identity"] = json!(s.doc.identity + 1);
    assert_eq!(
        s.apply(Action::new("histogram.query", stale))
            .unwrap_err()
            .code,
        "histogram_stale"
    );
    for query in [
        args(&s, "source", 999, "output", None),
        args(&s, "source", 11, "input", Some(0)),
    ] {
        assert_eq!(
            s.apply(Action::new("histogram.query", query))
                .unwrap_err()
                .code,
            "histogram_node"
        );
    }
    let mut incomplete = args(&s, "source", 22, "input", Some(0));
    incomplete
        .as_object_mut()
        .unwrap()
        .remove("document_revision");
    let result=s.apply(Action::new("pipeline.run",json!({"steps":[{"id":"prefix","action":"layer.rename","args":{"name":"Must not run"}},{"id":"read","action":"histogram.query","args":incomplete}]})));
    assert!(result.is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
}

#[test]
fn node_observations_borrow_cached_channel_levels_and_curves_tables() {
    use crate::engine::{curves::Curves, graph::Renderer, levels::Levels};
    use micopi_plugin_api as abi;
    use std::{cell::Cell, sync::Arc};
    struct Counted {
        inner: Arc<dyn Renderer>,
        preparations: Cell<usize>,
    }
    impl Renderer for Counted {
        fn supports_histogram_plan(&self) -> bool {
            self.inner.supports_histogram_plan()
        }
        fn supports_levels(&self) -> bool {
            self.inner.supports_levels()
        }
        fn supports_levels_channels(&self) -> bool {
            self.inner.supports_levels_channels()
        }
        fn supports_curves(&self) -> bool {
            self.inner.supports_curves()
        }
        fn supports_blend_mode(&self, m: u32) -> bool {
            self.inner.supports_blend_mode(m)
        }
        fn prepare_levels(&self, p: &abi::LevelsParameters) -> Result<abi::CompositeLut, String> {
            self.preparations.set(self.preparations.get() + 1);
            self.inner.prepare_levels(p)
        }
        fn prepare_levels_channels(
            &self,
            p: &abi::LevelsChannelsParameters,
        ) -> Result<[abi::CompositeLut; 3], String> {
            self.preparations.set(self.preparations.get() + 1);
            self.inner.prepare_levels_channels(p)
        }
        fn prepare_curves(&self, p: &abi::CurvesParameters) -> Result<abi::CompositeLut, String> {
            self.preparations.set(self.preparations.get() + 1);
            self.inner.prepare_curves(p)
        }
        fn render(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            w: u32,
            h: u32,
            o: &mut abi::Pixels,
        ) -> Result<(), String> {
            self.inner.render(n, l, w, h, o)
        }
        fn render_prepared(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            m: &[abi::CompositeMask],
            t: &[abi::CompositeLut],
            w: u32,
            h: u32,
            o: &mut abi::Pixels,
        ) -> Result<(), String> {
            self.inner.render_prepared(n, l, m, t, w, h, o)
        }
        fn histogram_plan(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            m: &[abi::CompositeMask],
            t: &[abi::CompositeLut],
            p: abi::HistogramRequest,
        ) -> Result<abi::HistogramResult, String> {
            self.inner.histogram_plan(n, l, m, t, p)
        }
    }
    let mut s = fixture();
    let counted = Arc::new(Counted {
        inner: s.doc.renderer.clone().unwrap(),
        preparations: Cell::new(0),
    });
    s.doc.renderer = Some(counted.clone());
    let mut g = graph(true);
    let mut parameters = Levels {
        gamma: 2.,
        ..Default::default()
    };
    parameters.channels.red.output_white = 128;
    g.nodes[1].data = NodeKind::Levels { parameters };
    g.nodes[1].node_type = Some("compositing.levels".into());
    g.nodes.push(Node {
        id: 44,
        node_type: Some("compositing.curves".into()),
        position: [300., 0.],
        data: NodeKind::Curves {
            curves: Curves::default(),
        },
    });
    g.edges.retain(|e| e.to != 33);
    g.edges.extend([
        Edge {
            from: 22,
            to: 44,
            input: 0,
        },
        Edge {
            from: 44,
            to: 33,
            input: 0,
        },
    ]);
    call(&mut s, "graph.set", json!({"graph":g}));
    let prepared = counted.preparations.get();
    assert_eq!(prepared, 5);
    for view in ["source", "actual"] {
        counts(
            &observe(&mut s, view, 44, "input", Some(0)),
            45,
            128,
            156,
            4,
        );
        counts(&observe(&mut s, view, 44, "output", None), 45, 128, 156, 4);
    }
    assert_eq!(counted.preparations.get(), prepared);
}

#[test]
fn node_queries_reject_contacts_and_missing_capability_without_closing_input() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
        pressure: 0.7,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    let query = args(&s, "source", 22, "input", Some(0));
    assert_eq!(
        s.apply(Action::new("histogram.query", query))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 2,
        ..p
    })
    .unwrap();
    s.doc.renderer = None;
    let before = s.state();
    let query = args(&s, "source", 22, "output", None);
    assert_eq!(
        s.apply(Action::new("histogram.query", query))
            .unwrap_err()
            .code,
        "histogram_plan_unavailable"
    );
    assert_eq!(s.state(), before);
}

#[test]
fn pipelines_and_schema_nodes_bind_discovered_document_identity_and_revision() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"read","action":"graph.read","args":{}},
        {"id":"sample","action":"histogram.query","args":{"source":"node","graph_view":"source","node_id":22,"port":"input","input_index":0,
            "document_identity":{"$ref":"read#/result/document_identity"},"document_revision":{"$ref":"read#/result/revision"}}}]}),
    );
    assert_eq!(result["ok"], true);
    counts(&result["results"]["sample"]["result"], 32, 64, 96, 4);
    for read_type in ["action.graph.read", "state.compositing.graph"] {
        let workflow = json!({"version":1,"nodes":[
        {"id":1,"type":read_type,"position":[0,0],"parameters":{}},
        {"id":2,"type":"action.histogram.query","position":[300,0],"parameters":{"source":"node","graph_view":"source","node_id":22,"port":"output"}}
    ],"edges":[{"from":1,"output":"document_identity","to":2,"input":"document_identity"},{"from":1,"output":"revision","to":2,"input":"document_revision"}]});
        let run = call(
            &mut s,
            "workflow.run",
            json!({"graph":workflow,"atomic":true}),
        );
        assert_eq!(run["ok"], true);
    }
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
}
