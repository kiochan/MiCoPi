//! Extended blend modes share capabilities, history and verified outcomes across clients.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    graph::{BlendMode, Graph, NodeKind, Renderer},
    replay::Journal,
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};

const EXPECTED: [u32; 26] = [
    0xff4080c0, 0xff202018, 0xffa0a0c8, 0xff414030, 0xff404020, 0xff8080c0, 0xffc0c0e0, 0xff4040a0,
    0xff000000, 0xff000000, 0xff804020, 0xffab8182, 0xff4080c0, 0xff60403c, 0xff404191, 0xff024041,
    0xff0141a1, 0xff804081, 0xff000000, 0xff8080b0, 0xff400000, 0xffff802b, 0xff295989, 0xff903b10,
    0xff1c5c9c, 0xffa46444,
];
fn reset(s: &mut Session) {
    call(s, "canvas.new", json!({"width":2,"height":1}));
    s.doc.replace_region(0, 0, 2, 1, &[0xff804020; 2]);
    call(s, "layer.add", json!({}));
    s.doc.replace_region(0, 0, 2, 1, &[0xff4080c0; 2]);
    s.replay_boundary().unwrap();
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none(), "{:?}", s.doc.render_error);
    s.doc.composite.clone()
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn create(s: &mut Session) -> u64 {
    call(
        s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}],"name":"Ink 彩"}),
    )["group_id"]
        .as_u64()
        .unwrap()
}
fn graph(s: &Session, mode: BlendMode, enabled: bool) -> Graph {
    let mut graph = Graph::from_layers(&s.doc.layers);
    graph.enabled = enabled;
    for node in &mut graph.nodes {
        if let NodeKind::Blend { mode: selected, .. } = &mut node.data {
            *selected = mode;
        }
    }
    graph
}
fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/blend-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn blend_modes_have_stable_ids_and_independent_pixels_through_layer_graph_and_isolated_group() {
    let mut s = session();
    for (id, mode) in BlendMode::ALL.into_iter().enumerate() {
        assert_eq!(mode as usize, id);
        reset(&mut s);
        let source = raw(&s);
        call(&mut s, "layer.set", json!({"layer_id":2,"blend_mode":mode}));
        assert_eq!(pixels(&mut s), [EXPECTED[id]; 2], "layer {mode:?}");
        if mode != BlendMode::Normal {
            call(&mut s, "history.undo", json!({}));
            assert_eq!(pixels(&mut s), [EXPECTED[0]; 2]);
            call(&mut s, "history.redo", json!({}));
            assert_eq!(pixels(&mut s), [EXPECTED[id]; 2]);
            call(&mut s, "history.undo", json!({}));
        }
        let plan = graph(&s, mode, true);
        call(&mut s, "graph.set", json!({"graph":plan}));
        assert_eq!(pixels(&mut s), [EXPECTED[id]; 2], "graph {mode:?}");
        assert_eq!(raw(&s), source);
        call(&mut s, "history.undo", json!({}));
        let group = create(&mut s);
        call(
            &mut s,
            "group.set",
            json!({"group_id":group,"blend_mode":mode}),
        );
        assert_eq!(pixels(&mut s), [EXPECTED[id]; 2], "group {mode:?}");
        assert_eq!(raw(&s), source);
    }
    check(&mut s);
}

#[test]
fn every_extended_mode_survives_group_merge_and_grouped_visible_baking_with_translucent_gates() {
    let mut s = session();
    for mode in BlendMode::ALL.into_iter().skip(8) {
        reset(&mut s);
        call(&mut s, "layer.select", json!({"layer_id":1}));
        s.doc.replace_region(0, 0, 2, 1, &[0x7f804020; 2]);
        call(&mut s, "layer.select", json!({"layer_id":2}));
        s.doc.replace_region(0, 0, 2, 1, &[0xc84080c0; 2]);
        s.replay_boundary().unwrap();
        let group = create(&mut s);
        let empty = call(
            &mut s,
            "group.create",
            json!({"members":[],"name":"Preserved empty group"}),
        )["group_id"]
            .as_u64()
            .unwrap();
        call(
            &mut s,
            "group.set",
            json!({"group_id":group,"blend_mode":mode,"opacity":137./255.}),
        );
        let original = s.doc.composition();
        let original_raw = raw(&s);
        let expected = pixels(&mut s);
        assert!(expected.iter().all(|p| p >> 24 > 0 && p >> 24 < 255));
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        let merged = call(&mut s, "group.merge", json!({"group_id":group}))["layer_id"]
            .as_u64()
            .unwrap();
        assert!(merged > 2);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        let index = s.doc.layer_index(merged).unwrap();
        assert_eq!(s.doc.layers[index].mode, mode);
        assert_eq!(s.doc.layers[index].opacity, 137);
        assert_eq!(s.doc.layers[index].pixels, [0xc84080c0; 2]);
        assert_eq!(s.doc.layer_name(index), "Ink 彩");
        assert_eq!(s.doc.selected_layer_ids(), vec![merged]);
        assert_eq!(pixels(&mut s), expected, "Group Merge {mode:?}");
        let merged_metadata = s.doc.composition();
        let merged_raw = raw(&s);
        call(&mut s, "history.undo", json!({}));
        let mut restored = s.doc.composition();
        assert_eq!(restored.next_layer_id, merged_metadata.next_layer_id);
        restored.next_layer_id = original.next_layer_id;
        assert_eq!(restored, original, "single merge Undo {mode:?}");
        assert_eq!(raw(&s), original_raw);
        assert_eq!(pixels(&mut s), expected);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.composition(), merged_metadata);
        assert_eq!(raw(&s), merged_raw);
        assert_eq!(pixels(&mut s), expected);

        // An empty group deliberately retains grouped rendering after the
        // nonempty source group is replaced by its fresh raster.
        assert!(s.doc.has_groups());
        assert!(s.doc.group_layer_ids(empty).unwrap().is_empty());
        let skeleton = merged_metadata.groups.as_ref().unwrap().groups.clone();
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        call(&mut s, "layer.merge_visible", json!({}));
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        assert_eq!(s.doc.layers.len(), 1);
        assert_eq!(s.doc.layer_id(0), merged);
        assert_eq!(s.doc.layers[0].mode, BlendMode::Normal);
        assert_eq!(s.doc.layers[0].opacity, 255);
        assert_eq!(s.doc.layer_parent_id(0), 0);
        assert_eq!(s.doc.layers[0].pixels, expected);
        assert_eq!(s.doc.selected_layer_ids(), vec![merged]);
        assert_eq!(s.doc.composition().groups.unwrap().groups, skeleton);
        assert!(s.doc.group_layer_ids(empty).unwrap().is_empty());
        assert_eq!(pixels(&mut s), expected, "grouped Visible {mode:?}");
        call(&mut s, "history.undo", json!({}));
        assert_eq!(
            s.doc.composition(),
            merged_metadata,
            "single Visible Undo {mode:?}"
        );
        assert_eq!(raw(&s), merged_raw);
        assert_eq!(pixels(&mut s), expected);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, expected);
        assert_eq!(pixels(&mut s), expected);
        check(&mut s);
    }
}

#[test]
fn blend_mode_schemas_catalogs_workflow_drafts_and_explicit_batch_targets_share_all_26_values() {
    let mut s = session();
    reset(&mut s);
    let modes = json!(BlendMode::ALL);
    for action in ["layer.set", "group.set"] {
        assert_eq!(
            s.registry.actions[action].schema()["properties"]["blend_mode"]["enum"],
            modes
        );
    }
    assert_eq!(
        crate::engine::graph::schema()["properties"]["nodes"]["items"]["properties"]["mode"]
            ["enum"],
        modes
    );
    let catalog = call(&mut s, "node.catalog", json!({}));
    let text = catalog.to_string();
    for mode in BlendMode::ALL {
        let name = serde_json::to_value(mode).unwrap();
        assert!(text.contains(name.as_str().unwrap()), "catalog {mode:?}");
    }
    assert!(text.contains("action.layer.set") && text.contains("compositing.blend"));
    let draft = json!({"version":1,"nodes":[{"id":1,"type":"action.layer.set","position":[0,0],"parameters":{"layer_id":2,"blend_mode":"hue"}}],"edges":[]});
    let mut expected = s.doc.composition();
    expected.workflow = serde_json::from_value(draft.clone()).unwrap();
    let source = raw(&s);
    call(&mut s, "workflow.set", json!({"graph":draft}));
    assert_eq!(s.doc.composition(), expected);
    assert_eq!(raw(&s), source);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(s.doc.composition(), expected);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(pixels(&mut s), [EXPECTED[22]; 2]);
    call(&mut s, "history.undo", json!({}));
    let count = s.journal.as_ref().unwrap().entries.len();
    let revision = s.revision;
    call(
        &mut s,
        "layer.batch",
        json!({"action":"layer.set","layer_ids":[2,1],"args":{"blend_mode":"color"}}),
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    assert!(s.doc.layers.iter().all(|l| l.mode == BlendMode::Color));
    assert_eq!(pixels(&mut s), [EXPECTED[24]; 2]);
    assert_eq!(raw(&s), source);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), expected);
    check(&mut s);
}

struct PrefixRenderer {
    delegate: Arc<dyn Renderer>,
    renders: Arc<AtomicUsize>,
}
impl Renderer for PrefixRenderer {
    fn supports_transparent(&self) -> bool {
        true
    }
    fn render(
        &self,
        n: &[micopi_plugin_api::CompositeNode],
        l: &[micopi_plugin_api::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        self.renders.fetch_add(1, Ordering::Relaxed);
        self.delegate.render(n, l, w, h, o)
    }
}

#[test]
fn old_capability_prefix_preflights_hidden_disabled_batch_and_staged_project_metadata() {
    let mut s = session();
    reset(&mut s);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"blend_mode":"soft_light"}),
    );
    let new_project = output("extended.mcp");
    call(&mut s, "document.save", json!({"path":new_project}));
    reset(&mut s);
    let group = create(&mut s);
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"visible":false}),
    );
    let source = raw(&s);
    let committed = pixels(&mut s);
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let delegate = s.doc.renderer.clone().unwrap();
    let renders = Arc::new(AtomicUsize::new(0));
    s.doc.renderer = Some(Arc::new(PrefixRenderer {
        delegate: delegate.clone(),
        renders: renders.clone(),
    }));
    let mut disabled = graph(&s, BlendMode::Hue, false);
    // Group documents retain the raw linked leaf-filter store. Test a disabled
    // extended blend in that store rather than a structurally invalid free DAG.
    disabled.linked = true;
    for (id, args) in [
        (
            "layer.set",
            json!({"layer_id":2,"visible":false,"blend_mode":"soft_light"}),
        ),
        (
            "group.set",
            json!({"group_id":group,"visible":false,"blend_mode":"hue"}),
        ),
        ("graph.set", json!({"graph":disabled})),
        (
            "layer.batch",
            json!({"action":"layer.set","layer_ids":[1,2],"args":{"blend_mode":"divide"}}),
        ),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err(), "unsupported {id}");
        assert_eq!(s.state(), before, "state {id}");
        assert_eq!(s.doc.composition(), composition, "metadata {id}");
        assert_eq!(raw(&s), source);
        assert_eq!(s.doc.composite, committed);
        assert_eq!(s.doc.history_bytes(), history);
    }
    // Project Open intentionally reacquires its renderer from the registry.
    // Exercise its shared staged-candidate gate with this deterministic mock;
    // the smoke script checks actual Open against a previous compositor DLL.
    let mut candidate = codec::open(&new_project).unwrap();
    assert_eq!(candidate.layers[1].mode, BlendMode::SoftLight);
    candidate.renderer = s.doc.renderer.clone();
    assert!(candidate
        .ensure_graph_renderer()
        .unwrap_err()
        .contains("blend mode"));
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    assert_eq!(s.doc.composite, committed);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        renders.load(Ordering::Relaxed),
        0,
        "no unsupported plan is rendered"
    );
    s.doc.renderer = Some(delegate);
    check(&mut s);
    std::fs::remove_file(new_project).unwrap();
}

#[test]
fn unsupported_mode_rejection_preserves_pressure_contact_and_subsequent_stabilized_samples() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":64,"height":32}));
    call(
        &mut s,
        "brush.set",
        json!({"radius":3,"stabilizer":0.45,"pressure_size":true,"pressure_opacity":true}),
    );
    let delegate = s.doc.renderer.clone().unwrap();
    s.doc.renderer = Some(Arc::new(PrefixRenderer {
        delegate: delegate.clone(),
        renders: Arc::new(AtomicUsize::new(0)),
    }));
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":4,"y":12,"pressure":0.2,"pen":true,"time":1}),
    );
    let before = s.state();
    let source = raw(&s);
    assert_eq!(
        s.apply(Action::new("layer.set", json!({"blend_mode":"hue"})))
            .unwrap_err()
            .code,
        "blend_mode_unavailable"
    );
    assert!(s.stroke.active());
    assert_eq!(s.state(), before);
    assert_eq!(raw(&s), source);
    for (phase, x, pressure, time) in [
        ("move", 20, 0.55, 20),
        ("move", 40, 1., 40),
        ("up", 55, 0.7, 60),
    ] {
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":phase,"x":x,"y":12,"pressure":pressure,"pen":true,"time":time}),
        );
    }
    assert!(!s.stroke.active());
    assert_ne!(
        raw(&s),
        source,
        "accepted MOVE/UP continue the same contact"
    );
    assert!(s.doc.layers[0].pixels.iter().any(|p| p >> 24 > 0));
    s.doc.renderer = Some(delegate);
    check(&mut s);
}

struct FailingRenderer {
    delegate: Arc<dyn Renderer>,
    armed: Arc<AtomicBool>,
}
impl Renderer for FailingRenderer {
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.delegate.supports_blend_mode(mode)
    }
    fn supports_transparent(&self) -> bool {
        true
    }
    fn render(
        &self,
        n: &[micopi_plugin_api::CompositeNode],
        l: &[micopi_plugin_api::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        if self.armed.load(Ordering::Relaxed) {
            if !o.data.is_null() && o.len != 0 {
                unsafe {
                    *o.data = 0xff123456;
                }
            }
            return Err("Injected extended-blend partial output failure".into());
        }
        self.delegate.render(n, l, w, h, o)
    }
}

#[test]
fn extended_blend_render_atomic_and_partial_journal_failures_restore_pixels_metadata_and_history() {
    let mut s = session();
    reset(&mut s);
    let group = create(&mut s);
    let armed = Arc::new(AtomicBool::new(false));
    s.doc.renderer = Some(Arc::new(FailingRenderer {
        delegate: s.doc.renderer.clone().unwrap(),
        armed: armed.clone(),
    }));
    let path = output("clean.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let committed = pixels(&mut s);
    let history = s.doc.history_bytes();
    armed.store(true, Ordering::Relaxed);
    for (id, args) in [
        ("layer.set", json!({"layer_id":2,"blend_mode":"soft_light"})),
        ("group.set", json!({"group_id":group,"blend_mode":"color"})),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), source);
        assert_eq!(s.doc.composite, committed);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(!s.doc.dirty);
        assert!(s.replay_error.is_none());
    }
    armed.store(false, Ordering::Relaxed);
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"blend_mode":"color"}),
    );
    assert_eq!(pixels(&mut s), [EXPECTED[24]; 2]);
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let committed = pixels(&mut s);
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"edit","action":"layer.set","args":{"layer_id":2,"blend_mode":"hue"}},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "layer.batch",
            json!({"action":"layer.set","layer_ids":[2,1],"args":{"blend_mode":"luminosity"}})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    assert_eq!(s.doc.composite, committed);
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn extended_mode_projects_and_verified_outcomes_preserve_new_metadata_without_executing_a_renderer()
{
    let mut s = session();
    reset(&mut s);
    let group = create(&mut s);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"blend_mode":"soft_light"}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"blend_mode":"color"}),
    );
    let expected = pixels(&mut s);
    let composition = s.doc.composition();
    let path = output("color.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composition(), composition);
    assert_eq!(pixels(&mut opened), expected);
    assert_eq!(codec::png(&mut opened.doc).unwrap(), png);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    let replayed = journal.preview(last).unwrap();
    assert_eq!(replayed.metadata.composition, composition);
    assert_eq!(replayed.pixels(), expected);
    assert_eq!(replay::png(&replayed).unwrap(), png);
    let legacy = include_bytes!("../../tests/fixtures/replay/v1.mcreplay");
    assert_eq!(
        Journal::import(legacy).unwrap().verify(None).unwrap()["valid"],
        true
    );
    std::fs::remove_file(path).unwrap();
}
