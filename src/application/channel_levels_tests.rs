//! Channel Levels retain shared source/adjustment publication and verified outcomes.
use super::editing_tests::call;
use super::*;
use crate::{
    engine::{
        graph::{Edge, Graph, Node, NodeKind},
        levels::{Levels, LevelsChannels},
        replay::Journal,
        Document,
    },
    plugins::Registry,
};
use serde_json::json;
use std::path::PathBuf;

fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
    )
    .unwrap();
    call(&mut s, "canvas.new", json!({"width":3,"height":1}));
    s.doc.layers[0].pixels = vec![0x804080c0, 0xff808080, 0x01000000];
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    s
}
fn levels() -> Levels {
    let mut p = Levels {
        gamma: 2.,
        ..Default::default()
    };
    p.channels.red.output_white = 128;
    p.channels.green.input_black = 64;
    p.channels.green.input_white = 192;
    p.channels.blue.output_black = 255;
    p.channels.blue.output_white = 0;
    p
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
                node_type: Some("compositing.levels".into()),
                position: [200., 0.],
                data: NodeKind::Levels {
                    parameters: levels(),
                },
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
fn reference(pixel: u32) -> u32 {
    let master = |shift| ((((pixel >> shift) & 255) as f64 / 255.).sqrt() * 255.).round();
    let red = (master(16) * 128. / 255.).round() as u32;
    let green = (((master(8) - 64.) / 128.).clamp(0., 1.) * 255.).round() as u32;
    let blue = 255 - master(0) as u32;
    (pixel & 0xff000000) | (red << 16) | (green << 8) | blue
}

#[test]
fn channels_share_independent_source_adjustment_pixels_and_one_undo() {
    for mode in 0..3 {
        let mut s = fixture();
        let raw = s.doc.layers[0].pixels.clone();
        let revision = s.revision;
        if mode == 2 {
            call(
                &mut s,
                "adjustment.create",
                json!({"node_type":"compositing.levels","index":1,"parameters":{"parameters":levels()}}),
            );
        } else {
            call(&mut s, "graph.set", json!({"graph":graph(mode==1)}));
        }
        assert_eq!(s.revision, revision + 1);
        assert_eq!(
            s.doc.composite,
            raw.iter().copied().map(reference).collect::<Vec<_>>()
        );
        assert_eq!(s.doc.layers[0].pixels, raw);
        assert_eq!(
            call(&mut s, "graph.catalog", json!({}))["renderer_channel_levels"],
            true
        );
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.composite, raw);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.composite[0], reference(raw[0]));
    }
}

#[test]
fn channel_partial_patches_start_at_identity_preserve_neighbors_and_reset_explicitly() {
    let mut s = fixture();
    let raw = s.doc.layers[0].pixels.clone();
    let id = call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"compositing.levels","index":1,
        "parameters":{"parameters":{"channels":{"red":{"output_white":128}}}}}),
    )["adjustment_id"]
        .as_u64()
        .unwrap();
    let first = s.doc.adjustment(id).unwrap().levels.unwrap();
    assert_eq!(first.gamma, 1.);
    assert_eq!(first.channels.red.output_white, 128);
    assert_eq!(first.channels.green, Default::default());
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"parameters":{"parameters":{"channels":{"blue":{"output_black":255,"output_white":0}}}}}),
    );
    let second = s.doc.adjustment(id).unwrap().levels.unwrap();
    assert_eq!(second.channels.red, first.channels.red);
    assert_eq!(second.channels.blue.output_white, 0);
    let state = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    crate::plugins::validate(
        s.registry.actions["adjustment.get"]
            .output_schema
            .as_ref()
            .unwrap(),
        &state,
    )
    .unwrap();
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"parameters":{"parameters":{"channels":LevelsChannels::default()}}}),
    );
    assert_eq!(s.doc.composite, raw);
    assert_eq!(
        s.doc.adjustment(id).unwrap().levels.unwrap(),
        Levels::default()
    );
    let serialized = serde_json::to_value(s.doc.adjustment(id).unwrap()).unwrap();
    assert!(serialized["levels"].get("channels").is_none());
    assert_eq!(serialized["levels"].as_object().unwrap().len(), 5);
}

#[test]
fn channels_compile_stable_variable_offsets_and_preflight_disabled_declarations() {
    let mut g = graph(false);
    g.nodes.insert(
        0,
        Node {
            id: 99,
            node_type: Some("compositing.levels".into()),
            position: [0., 200.],
            data: NodeKind::Levels {
                parameters: Levels::default(),
            },
        },
    );
    g.edges.push(Edge {
        from: 1,
        to: 99,
        input: 0,
    });
    assert_eq!(
        g.compile(1)
            .unwrap()
            .iter()
            .find(|n| n.kind == 10)
            .unwrap()
            .mode,
        1
    );
    let mut s = fixture();
    call(&mut s, "graph.set", json!({"graph":g}));
    assert_eq!(s.doc.composite[0], reference(s.doc.layers[0].pixels[0]));
    let before = s.doc.composition();
    let revision = s.revision;
    let mut invalid = graph(true);
    invalid.enabled = false;
    if let NodeKind::Levels { parameters } = &mut invalid.nodes[1].data {
        parameters.channels.blue.input_black = 255;
    }
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":invalid})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    let mut oversized = graph(false);
    for i in 0..21 {
        oversized.nodes.push(Node {
            id: 100 + i,
            node_type: None,
            position: [0., 0.],
            data: NodeKind::Levels {
                parameters: levels(),
            },
        });
        oversized.edges.push(Edge {
            from: 1,
            to: 100 + i,
            input: 0,
        });
    }
    assert!(matches!(oversized.compile(1),Err(e) if e.contains("table limit")));
}

#[test]
fn scalar_only_renderer_rejects_hidden_channel_intent_before_publication() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    use std::sync::Arc;
    struct ScalarOnly(Arc<dyn Renderer>);
    impl Renderer for ScalarOnly {
        fn supports_levels(&self) -> bool {
            self.0.supports_levels()
        }
        fn supports_mix(&self) -> bool {
            self.0.supports_mix()
        }
        fn supports_transparent(&self) -> bool {
            self.0.supports_transparent()
        }
        fn supports_blend_mode(&self, mode: u32) -> bool {
            self.0.supports_blend_mode(mode)
        }
        fn prepare_levels(&self, p: &abi::LevelsParameters) -> Result<abi::CompositeLut, String> {
            self.0.prepare_levels(p)
        }
        fn render(
            &self,
            n: &[abi::CompositeNode],
            l: &[abi::CompositeLayer],
            w: u32,
            h: u32,
            o: &mut abi::Pixels,
        ) -> Result<(), String> {
            self.0.render(n, l, w, h, o)
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
            self.0.render_prepared(n, l, m, t, w, h, o)
        }
    }
    let mut s = fixture();
    s.doc.renderer = Some(Arc::new(ScalarOnly(s.doc.renderer.clone().unwrap())));
    let before = s.doc.composition();
    let revision = s.revision;
    let history = s.doc.history_bytes();
    let mut g = graph(true);
    g.enabled = false;
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":g})))
            .unwrap_err()
            .code,
        "levels_unavailable"
    );
    assert!(s.apply(Action::new("adjustment.create",json!({"node_type":"compositing.levels","visible":false,"index":1,"parameters":{"parameters":levels()}}))).is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.history_bytes(), history);
}

#[test]
fn channel_actions_preserve_live_contact_until_explicit_up() {
    let mut s = fixture();
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
    s.doc.compose();
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    for action in [
        Action::new("graph.set", json!({"graph":graph(true)})),
        Action::new(
            "adjustment.create",
            json!({"node_type":"compositing.levels","parameters":{"parameters":levels()}}),
        ),
    ] {
        assert_eq!(s.apply(action).unwrap_err().code, "stroke_active");
        assert!(s.stroke.active());
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 2,
        ..p
    })
    .unwrap();
}

#[test]
fn channels_masks_projects_atomic_rollback_and_replay_preserve_exact_state() {
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
    assert_eq!(
        s.doc.composite[0],
        (reference(raw[0]) & 0xffffff) | 0x40000000
    );
    let id = call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"compositing.levels","index":1,"parameters":{"parameters":levels()}}),
    )["adjustment_id"]
        .as_u64()
        .unwrap();
    let before = s.doc.composition();
    let surface = s.doc.composite.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"change","action":"adjustment.set","args":{"adjustment_id":id,"parameters":{"parameters":{"channels":{"red":{"gamma":2}}}}}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.composite, surface);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/channel-levels/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("channels.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composition(), before);
    assert_eq!(opened.doc.composite, surface);
    assert_eq!(opened.doc.layers[0].pixels, raw);
    assert!((0..3).all(|i| opened.doc.mask_plane(mask).unwrap().sample(i) == 128));
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composite, surface);
    assert_eq!(restored.composition(), before);
    assert_eq!(restored.layers[0].pixels, raw);
}
