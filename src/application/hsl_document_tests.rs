//! Typed HSL image/adjustment intent shares native plans, history and outcomes.
use super::{
    editing_tests::{call, check, session},
    *,
};
use crate::engine::{
    curves::Curves,
    graph::{Edge, Graph, Node, NodeKind},
    hsl::Hsl,
    levels::Levels,
    replay::Journal,
    Document,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":1}));
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
    s
}
fn graph(linked: bool) -> Graph {
    Graph {
        enabled: true,
        linked,
        nodes: vec![
            Node {
                id: 11,
                node_type: None,
                position: [0., 0.],
                data: NodeKind::Layer { index: 0 },
            },
            Node {
                id: 22,
                node_type: Some("compositing.hsl".into()),
                position: [200., 0.],
                data: NodeKind::Hsl {
                    hsl: Hsl {
                        hue: 120.,
                        ..Default::default()
                    },
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
#[test]
fn hsl_source_intents_keep_separate_stable_parameter_and_table_slots() {
    let mut s = fixture();
    let raw = s.doc.layers[0].pixels.clone();
    let mut g = graph(false);
    g.nodes.insert(
        0,
        Node {
            id: 44,
            node_type: Some("compositing.hsl".into()),
            position: [0., 200.],
            data: NodeKind::Hsl {
                hsl: Hsl {
                    hue: -120.,
                    ..Default::default()
                },
            },
        },
    );
    g.nodes.insert(
        1,
        Node {
            id: 88,
            node_type: Some("compositing.curves".into()),
            position: [0., 400.],
            data: NodeKind::Curves {
                curves: Curves::default(),
            },
        },
    );
    g.nodes.push(Node {
        id: 77,
        node_type: Some("compositing.levels".into()),
        position: [400., 0.],
        data: NodeKind::Levels {
            parameters: Levels::default(),
        },
    });
    g.nodes.push(Node {
        id: 55,
        node_type: Some("compositing.curves".into()),
        position: [600., 0.],
        data: NodeKind::Curves {
            curves: Curves::default(),
        },
    });
    g.edges.retain(|e| e.to != 33);
    g.edges.extend([
        Edge {
            from: 11,
            to: 44,
            input: 0,
        },
        Edge {
            from: 11,
            to: 88,
            input: 0,
        },
        Edge {
            from: 22,
            to: 77,
            input: 0,
        },
        Edge {
            from: 77,
            to: 55,
            input: 0,
        },
        Edge {
            from: 55,
            to: 33,
            input: 0,
        },
    ]);
    let plan = g.compile(1).unwrap();
    assert_eq!(plan.iter().find(|n| n.kind == 11).unwrap().mode, 1);
    assert_eq!(plan.iter().find(|n| n.kind == 9).unwrap().mode, 3);
    assert_eq!(plan.iter().find(|n| n.kind == 10).unwrap().mode, 4);
    let revision = s.revision;
    call(&mut s, "graph.set", json!({"graph":g}));
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, vec![0x8000ff00; 4]);
    let query = json!({"source":"node","graph_view":"source","node_id":22,"port":"output","document_identity":s.doc.identity,"document_revision":s.revision});
    let bins = call(&mut s, "histogram.query", query);
    assert_eq!(bins["green"][255], 4);
    let input = json!({"source":"node","graph_view":"source","node_id":22,"port":"input","input_index":0,"document_identity":s.doc.identity,"document_revision":s.revision});
    let bins = call(&mut s, "histogram.query", input);
    assert_eq!(bins["red"][255], 4);
    assert_eq!(s.revision, revision + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite, raw);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composite, vec![0x8000ff00; 4]);
}

#[test]
fn hsl_adjustment_partial_patches_masks_groups_and_dll_free_persistence_agree() {
    let mut s = fixture();
    let group = call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":1}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"compositing":"pass_through"}),
    );
    let id = call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"compositing.hsl","parent_id":group,"parameters":{"hsl":{"hue":120}}}),
    )["adjustment_id"]
        .as_u64()
        .unwrap();
    assert_eq!(s.doc.composite, vec![0x8000ff00; 4]);
    let raw = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"parameters":{"hsl":{"lightness":50}}}),
    );
    let item = s.doc.adjustment(id).unwrap();
    assert_eq!(
        item.hsl.unwrap(),
        Hsl {
            hue: 120.,
            saturation: 0.,
            lightness: 50.
        }
    );
    assert_eq!(s.doc.composite, vec![0x8080ff80; 4]);
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"adjustment","id":id}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":0}));
    assert_eq!(s.doc.composite, raw);
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":255}));
    let composite = s.doc.composite.clone();
    let composition = s.doc.composition();
    let revision = s.revision;
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"change","action":"adjustment.set","args":{"adjustment_id":id,"parameters":{"hsl":{"hue":0}}}},{"id":"reject","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    assert_eq!(result["failed_step"], "reject");
    assert_eq!(result["error"]["code"], "invalid_layer");
    assert_eq!(result["completed"], json!(["change"]));
    assert_eq!(result["committed"], json!([]));
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.composite, composite);
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/hsl-document/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("hsl.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composite, composite);
    assert_eq!(opened.doc.adjustments(), s.doc.adjustments());
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composite, composite);
    assert_eq!(restored.adjustments(), s.doc.adjustments());
    assert_eq!(s.doc.layers[0].pixels, raw);
}

#[test]
fn hsl_invalid_and_live_contact_edits_reject_before_publication() {
    let mut s = fixture();
    let before = s.doc.composition();
    let revision = s.revision;
    let mut invalid = graph(true);
    invalid.enabled = false;
    if let NodeKind::Hsl { hsl } = &mut invalid.nodes[1].data {
        hsl.saturation = 101.;
    }
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":invalid})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
        pressure: 0.4,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(pointer).unwrap();
    let revision = s.revision;
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":graph(true)})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 2,
        ..pointer
    })
    .unwrap();
}

#[test]
fn unsupported_hsl_rejects_disabled_and_unused_declarations_before_mutation() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    struct Legacy;
    impl Renderer for Legacy {
        fn render(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            panic!("Unsupported HSL must reject before rendering")
        }
    }
    let mut s = fixture();
    s.doc.renderer = Some(std::sync::Arc::new(Legacy));
    let before = s.doc.composition();
    let history = s.doc.history_bytes();
    let revision = s.revision;
    for (enabled, unused) in [(true, false), (false, false), (true, true)] {
        let mut graph = graph(false);
        graph.enabled = enabled;
        if unused {
            graph.edges[1].from = 11;
        }
        assert_eq!(
            s.apply(Action::new("graph.set", json!({"graph":graph})))
                .unwrap_err()
                .code,
            "hsl_unavailable"
        );
        assert_eq!(s.doc.composition(), before);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.revision, revision);
    }
}

#[test]
fn painting_and_node_observation_reuse_the_same_borrowed_hsl_parameter_storage() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    use std::{cell::Cell, rc::Rc, sync::Arc};
    struct Recorder {
        native: Arc<crate::plugins::native::NativePlugin>,
        pointer: Rc<Cell<usize>>,
        calls: Rc<Cell<usize>>,
    }
    impl Renderer for Recorder {
        fn supports_hsl(&self) -> bool {
            true
        }
        fn supports_histogram_plan(&self) -> bool {
            true
        }
        fn render(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            w: u32,
            h: u32,
            o: &mut abi::Pixels,
        ) -> Result<(), String> {
            self.native.render(n, l, w, h, o)
        }
        fn render_hsl(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            m: &[abi::CompositeMask],
            t: &[abi::CompositeLut],
            p: &[abi::HslParameters],
            w: u32,
            h: u32,
            o: &mut abi::Pixels,
        ) -> Result<(), String> {
            self.pointer.set(p.as_ptr() as usize);
            self.calls.set(self.calls.get() + 1);
            self.native.render_hsl(n, l, m, t, p, w, h, o)
        }
        fn histogram_hsl(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            m: &[abi::CompositeMask],
            t: &[abi::CompositeLut],
            p: &[abi::HslParameters],
            r: abi::HistogramRequest,
        ) -> Result<abi::HistogramResult, String> {
            self.pointer.set(p.as_ptr() as usize);
            self.calls.set(self.calls.get() + 1);
            self.native.histogram_hsl(n, l, m, t, p, r)
        }
    }
    let mut s = fixture();
    let pointer = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    s.doc.renderer = Some(Arc::new(Recorder {
        native: s.registry.native_plugins["compositing"].clone(),
        pointer: pointer.clone(),
        calls: calls.clone(),
    }));
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let original = pointer.get();
    assert_ne!(original, 0);
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
        pressure: 0.4,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    for (phase, time) in [
        (PointerPhase::Down, 1),
        (PointerPhase::Move, 2),
        (PointerPhase::Up, 3),
    ] {
        s.pointer(Pointer { phase, time, ..p }).unwrap();
        s.doc.compose();
        assert_eq!(pointer.get(), original);
    }
    assert!(calls.get() > 1);
    let before = calls.get();
    let query = json!({"source":"node","graph_view":"source","node_id":22,"port":"output","document_identity":s.doc.identity,"document_revision":s.revision});
    call(&mut s, "histogram.query", query);
    assert!(calls.get() > before);
    assert_eq!(pointer.get(), original);
}
