//! Independent encoded-byte references across shared document contracts.
use super::{editing_tests::call, *};
use crate::{
    engine::{
        curves::{Curve, Curves, Interpolation, Point},
        graph::{Edge, Graph, Node, NodeKind},
        levels::Levels,
        replay::Journal,
        Document,
    },
    plugins::Registry,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    call(&mut s, "canvas.new", json!({"width":3,"height":1}));
    s.doc.layers[0].pixels = vec![0x80404040, 0x7fffffff, 0x01000000];
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    s
}
fn intent() -> Curves {
    Curves {
        master: Curve {
            interpolation: Interpolation::Linear,
            points: vec![
                Point {
                    input: 0,
                    output: 0,
                },
                Point {
                    input: 128,
                    output: 64,
                },
                Point {
                    input: 255,
                    output: 255,
                },
            ],
        },
        red: Curve {
            interpolation: Interpolation::Linear,
            points: vec![
                Point {
                    input: 0,
                    output: 255,
                },
                Point {
                    input: 255,
                    output: 0,
                },
            ],
        },
        ..Default::default()
    }
}
fn graph(linked: bool) -> Graph {
    Graph {
        enabled: true,
        linked,
        nodes: vec![
            Node {
                id: 1,
                node_type: None,
                position: [0., 0.],
                data: NodeKind::Layer { index: 0 },
            },
            Node {
                id: 2,
                node_type: Some("compositing.curves".into()),
                position: [200., 0.],
                data: NodeKind::Curves { curves: intent() },
            },
            Node {
                id: 3,
                node_type: None,
                position: [400., 0.],
                data: NodeKind::Output,
            },
        ],
        edges: vec![
            Edge {
                from: 1,
                to: 2,
                input: 0,
            },
            Edge {
                from: 2,
                to: 3,
                input: 0,
            },
        ],
    }
}
const EXPECTED: [u32; 3] = [0x80df2020, 0x7f00ffff, 0x01ff0000];
#[test]
fn curves_channels_preserve_alpha_and_undo_in_linked_and_free_graphs() {
    for linked in [false, true] {
        let mut s = fixture();
        let original = s.doc.layers[0].pixels.clone();
        assert_eq!(
            s.registry.node_catalog()["compositing.curves"].domain,
            "image"
        );
        assert_eq!(
            call(&mut s, "graph.catalog", json!({}))["renderer_curves"],
            true
        );
        call(&mut s, "graph.set", json!({"graph":graph(linked)}));
        assert_eq!(s.doc.composite, EXPECTED);
        assert_eq!(s.doc.layers[0].pixels, original);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.composite, original);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.composite, EXPECTED);
        assert!(s.doc.render_error.is_none());
    }
}
#[test]
fn mixed_disconnected_tables_use_storage_order_and_preflight_the_full_budget() {
    let mut s = fixture();
    let mut g = graph(false);
    g.nodes.insert(
        0,
        Node {
            id: 99,
            node_type: None,
            position: [0., 100.],
            data: NodeKind::Levels {
                parameters: Levels::default(),
            },
        },
    );
    g.nodes.insert(
        1,
        Node {
            id: 98,
            node_type: None,
            position: [200., 100.],
            data: NodeKind::Curves {
                curves: Curves::default(),
            },
        },
    );
    g.edges.extend([
        Edge {
            from: 1,
            to: 99,
            input: 0,
        },
        Edge {
            from: 1,
            to: 98,
            input: 0,
        },
    ]);
    assert_eq!(
        g.compile(1)
            .unwrap()
            .iter()
            .find(|n| n.kind == 10)
            .unwrap()
            .mode,
        4
    );
    call(&mut s, "graph.set", json!({"graph":g}));
    assert_eq!(s.doc.composite, EXPECTED);
    let mut oversized = graph(false);
    for i in 0..21 {
        oversized.nodes.push(Node {
            id: 100 + i,
            node_type: None,
            position: [0., 0.],
            data: NodeKind::Curves {
                curves: Curves::default(),
            },
        });
        oversized.edges.push(Edge {
            from: 1,
            to: 100 + i,
            input: 0,
        });
    }
    assert!(oversized.compile(1).unwrap_err().contains("table limit"));
}
#[test]
fn invalid_disabled_curves_and_missing_capability_preserve_publication() {
    let mut s = fixture();
    let before = s.doc.composition();
    let revision = s.revision;
    let mut g = graph(false);
    g.enabled = false;
    if let NodeKind::Curves { curves } = &mut g.nodes[1].data {
        curves.blue.points[1].input = 0;
    }
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":g})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    s.doc.renderer = None;
    let mut g = graph(true);
    g.enabled = false;
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":g})))
            .unwrap_err()
            .code,
        "curves_unavailable"
    );
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    for points in [
        json!([{"input":0.5,"output":0},{"input":255,"output":255}]),
        json!([{"input":0,"output":0},{"input":256,"output":255}]),
    ] {
        let mut value = serde_json::to_value(graph(true)).unwrap();
        value["nodes"][1]["curves"]["master"]["points"] = points;
        assert!(s
            .apply(Action::new("graph.set", json!({"graph":value})))
            .is_err());
    }
}
#[test]
fn curves_masks_projects_and_verified_outcomes_preserve_planes_without_replay_dll() {
    let mut s = fixture();
    let raw = s.doc.layers[0].pixels.clone();
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":128}));
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    assert_eq!(s.doc.composite[0], 0x40df2020);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.mask_plane(mask).unwrap().sample(0), 128);
    let expected = s.doc.composite.clone();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/curves-node/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("curves.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composite, expected);
    assert_eq!(opened.doc.graph, s.doc.graph);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composite, expected);
    assert_eq!(restored.graph, s.doc.graph);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite[0], 0x40404040);
}
#[test]
fn curves_reject_live_contact_and_atomic_failure_restores_the_prior_plan() {
    let mut s = fixture();
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
        pressure: 1.,
        time: 1,
        pen: false,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":graph(true)})))
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
    let before = s.doc.composition();
    let pixels = s.doc.composite.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"curves","action":"graph.set","args":{"graph":graph(true)}},{"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.composite, pixels);
}

#[test]
fn curves_pointer_moves_borrow_the_three_published_tables() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    use std::{cell::Cell, sync::Arc};
    struct Counted {
        native: Arc<dyn Renderer>,
        preparations: Cell<usize>,
    }
    impl Renderer for Counted {
        fn supports_curves(&self) -> bool {
            self.native.supports_curves()
        }
        fn supports_transparent(&self) -> bool {
            self.native.supports_transparent()
        }
        fn supports_blend_mode(&self, mode: u32) -> bool {
            self.native.supports_blend_mode(mode)
        }
        fn prepare_curves(&self, p: &abi::CurvesParameters) -> Result<abi::CompositeLut, String> {
            self.preparations.set(self.preparations.get() + 1);
            self.native.prepare_curves(p)
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
            self.native.render_prepared(n, l, m, t, w, h, o)
        }
    }
    let mut s = fixture();
    let renderer = Arc::new(Counted {
        native: s.doc.renderer.clone().unwrap(),
        preparations: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    assert_eq!(renderer.preparations.get(), 4);
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
        pressure: 1.,
        time: 1,
        pen: false,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    s.doc.compose();
    for time in 2..16 {
        s.pointer(Pointer {
            phase: PointerPhase::Move,
            time,
            ..p
        })
        .unwrap();
        s.doc.compose();
    }
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 16,
        ..p
    })
    .unwrap();
    s.doc.compose();
    assert_eq!(renderer.preparations.get(), 4);
}
