//! Levels source nodes share Session publication, rendering, history and replay.
use super::editing_tests::call;
use super::*;
use crate::{
    engine::{
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
fn graph(linked: bool) -> Graph {
    Graph {
        enabled: true,
        linked,
        nodes: vec![
            Node {
                id: 10,
                node_type: None,
                position: [0., 0.],
                data: NodeKind::Layer { index: 0 },
            },
            Node {
                id: 20,
                node_type: Some("compositing.levels".into()),
                position: [200., 0.],
                data: NodeKind::Levels {
                    parameters: Levels {
                        gamma: 2.,
                        ..Default::default()
                    },
                },
            },
            Node {
                id: 30,
                node_type: None,
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
#[test]
fn levels_nodes_are_registered_non_destructive_and_undoable_in_free_and_linked_graphs() {
    for linked in [false, true] {
        let mut s = fixture();
        let original = s.doc.layers[0].pixels.clone();
        let catalog = s.registry.node_catalog();
        assert_eq!(catalog["compositing.levels"].domain, "image");
        let result = call(&mut s, "graph.set", json!({"graph":graph(linked)}));
        assert_eq!(result["graph"]["nodes"][1]["kind"], "levels");
        assert_eq!(s.doc.layers[0].pixels, original);
        assert_eq!(s.doc.composite, vec![0x80808080, 0x7fffffff, 0x01000000]);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.composite, original);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.composite[0], 0x80808080);
        assert!(s.doc.render_error.is_none());
    }
}
#[test]
fn disconnected_levels_have_stable_table_indices_and_all_parameters_validate() {
    let mut s = fixture();
    let mut g = graph(false);
    g.nodes.insert(
        0,
        Node {
            id: 99,
            node_type: Some("compositing.levels".into()),
            position: [0., 200.],
            data: NodeKind::Levels {
                parameters: Levels {
                    gamma: 0.5,
                    ..Default::default()
                },
            },
        },
    );
    // A disconnected draft needs an input, but is not evaluated by the output.
    g.edges.push(Edge {
        from: 10,
        to: 99,
        input: 0,
    });
    assert_eq!(
        g.compile(1)
            .unwrap()
            .iter()
            .find(|n| n.kind == 9)
            .unwrap()
            .mode,
        1
    );
    call(&mut s, "graph.set", json!({"graph":g}));
    assert_eq!(s.doc.composite[0], 0x80808080);
    let before = s.doc.composition();
    let revision = s.revision;
    g.enabled = false;
    g.nodes[0].data = NodeKind::Levels {
        parameters: Levels {
            gamma: 0.09,
            ..Default::default()
        },
    };
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":g})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
}
#[test]
fn levels_capability_rejection_includes_disabled_graphs_before_mutation() {
    let mut s = fixture();
    s.doc.renderer = None;
    let before = s.doc.composition();
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    let mut g = graph(true);
    g.enabled = false;
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":g})))
            .unwrap_err()
            .code,
        "levels_unavailable"
    );
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.layers[0].pixels, pixels);
}
#[test]
fn levels_edits_preserve_active_contact_and_atomic_failure_restores_the_old_plan() {
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
    s.doc.compose();
    let pixels = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
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
    let surface = s.doc.composite.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"levels","action":"graph.set","args":{"graph":graph(true)}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.composite, surface);
}
#[test]
fn levels_project_and_verified_outcome_restore_without_executing_the_dll() {
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    let expected = s.doc.composite.clone();
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/levels-node/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("levels.mcp");
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
}

#[test]
fn levels_source_node_uses_document_mask_coverage_without_mutating_either_plane() {
    let mut s = fixture();
    let pixels = s.doc.layers[0].pixels.clone();
    let id = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":id,"value":128}));
    call(&mut s, "graph.set", json!({"graph":graph(true)}));
    assert_eq!(s.doc.composite[0], 0x40808080);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert!((0..3).all(|index| s.doc.mask_plane(id).unwrap().sample(index) == 128));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite[0], 0x40404040);
}

#[test]
fn levels_pointer_moves_reuse_the_published_native_table() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    use std::{cell::Cell, sync::Arc};
    struct Counted {
        native: Arc<dyn Renderer>,
        preparations: Cell<usize>,
    }
    impl Renderer for Counted {
        fn supports_levels(&self) -> bool {
            self.native.supports_levels()
        }
        fn supports_transparent(&self) -> bool {
            self.native.supports_transparent()
        }
        fn supports_blend_mode(&self, mode: u32) -> bool {
            self.native.supports_blend_mode(mode)
        }
        fn prepare_levels(&self, p: &abi::LevelsParameters) -> Result<abi::CompositeLut, String> {
            self.preparations.set(self.preparations.get() + 1);
            self.native.prepare_levels(p)
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
    assert_eq!(renderer.preparations.get(), 1);
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
    assert_eq!(renderer.preparations.get(), 1);
}
