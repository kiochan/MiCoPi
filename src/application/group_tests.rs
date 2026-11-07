//! Isolated groups preserve raw sources, stable hierarchy and verified outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    graph::{BlendMode, Edge, Filter, Graph, Node, NodeKind, Renderer},
    replay::Journal,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

fn source(colors: &[u32]) -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":1}));
    for (i, color) in colors.iter().enumerate() {
        if i != 0 {
            call(&mut s, "layer.add", json!({}));
        }
        s.doc.replace_region(0, 0, 2, 1, &[*color; 2]);
        call(
            &mut s,
            "layer.rename",
            json!({"name":format!("Ink {} 墨",i+1)}),
        );
    }
    s.replay_boundary().unwrap();
    s
}
fn ids(s: &Session) -> Vec<u64> {
    (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect()
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn layer(id: u64) -> Value {
    json!({"kind":"layer","id":id})
}
fn group(id: u64) -> Value {
    json!({"kind":"group","id":id})
}
fn create(s: &mut Session, members: Value, name: &str) -> u64 {
    call(s, "group.create", json!({"members":members,"name":name}))["group_id"]
        .as_u64()
        .unwrap()
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none(), "{:?}", s.doc.render_error);
    s.doc.composite.clone()
}
fn output(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/group-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn filter(graph: &mut Graph, index: usize, amount: f32) {
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
        position: [0., 0.],
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount,
        },
    });
}

#[test]
fn groups_apply_opacity_once_after_overlap_and_nested_ungroup_is_explicit() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    let source = raw(&s);
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":all,"active_layer_id":all[1]}),
    );
    let revision = s.revision;
    let count = s.journal.as_ref().unwrap().entries.len();
    let inner = call(&mut s, "group.create_selected", json!({"name":"Colors 彩"}))["group_id"]
        .as_u64()
        .unwrap();
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    call(&mut s, "group.set", json!({"group_id":inner,"opacity":0.5}));
    assert_eq!(pixels(&mut s), [0x800000ff; 2]);
    assert_eq!(s.state()["selected_layer_ids"], json!(all));
    assert_eq!(s.doc.layer_id(s.doc.active), all[1]);
    let outer = create(&mut s, json!([group(inner)]), "Outer 外");
    call(&mut s, "group.set", json!({"group_id":outer,"opacity":0.5}));
    assert_eq!(pixels(&mut s), [0x400000ff; 2]);
    let composition = s.doc.composition();
    call(&mut s, "group.ungroup", json!({"group_id":outer}));
    assert_eq!(pixels(&mut s), [0x800000ff; 2]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(pixels(&mut s), [0x400000ff; 2]);
    assert_eq!(raw(&s), source);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    check(&mut s);
}

#[test]
fn groups_isolate_child_modes_and_apply_every_group_mode_against_the_parent() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":all[1],"blend_mode":"multiply"}),
    );
    assert_eq!(pixels(&mut s), [0xff000000; 2]);
    let g = create(&mut s, json!([layer(all[1])]), "Isolated");
    assert_eq!(pixels(&mut s), [0xff0000ff; 2]);
    call(
        &mut s,
        "group.set",
        json!({"group_id":g,"blend_mode":"multiply"}),
    );
    assert_eq!(pixels(&mut s), [0xff000000; 2]);
    let mut s = source(&[0xff804020, 0xffff0000, 0xff4080c0]);
    let all = ids(&s);
    let g = create(&mut s, json!([layer(all[1]), layer(all[2])]), "Modes");
    let expected = [
        0xff4080c0, 0xff202018, 0xffa0a0c8, 0xff414030, 0xff404020, 0xff8080c0, 0xffc0c0e0,
        0xff4040a0,
    ];
    for (mode, expected) in BlendMode::LEGACY.into_iter().zip(expected) {
        call(&mut s, "group.set", json!({"group_id":g,"blend_mode":mode}));
        assert_eq!(pixels(&mut s), [expected; 2], "{mode:?}");
    }
    assert_eq!(
        raw(&s),
        vec![
            vec![0xff804020; 2],
            vec![0xffff0000; 2],
            vec![0xff4080c0; 2]
        ]
    );
    check(&mut s);
}

#[test]
fn groups_visibility_and_queries_preserve_typed_contact_and_cached_hierarchy() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    let inner = create(&mut s, json!([layer(all[1])]), "Ink 墨");
    let outer = create(&mut s, json!([group(inner)]), "Drawing 作画");
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"visible":false}),
    );
    assert_eq!(pixels(&mut s), [0xffff0000; 2]);
    let row = s.state()["layers"][1].clone();
    assert_eq!(row["visible"], true);
    assert_eq!(row["effective_visible"], false);
    assert_eq!(row["parent_id"], inner);
    let before = s.doc.composition();
    let source = raw(&s);
    for (id, args) in [
        ("native.invert", json!({})),
        (
            "canvas.stroke",
            json!({"points":[{"x":0.5,"y":0.5,"pressure":0.2,"pen":true},{"x":1.5,"y":0.5,"pressure":1,"pen":true}]}),
        ),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err(), "{id}");
        assert_eq!(s.doc.composition(), before);
        assert_eq!(raw(&s), source);
        assert!(!s.is_drawing());
    }
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"visible":true}),
    );
    let query = call(&mut s, "group.query", json!({"query":"Ink 2"}));
    assert_eq!(query["matching_layer_ids"], json!([all[1]]));
    assert_eq!(
        query["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "group" && r["context"] == true)
            .count(),
        2
    );
    let query = call(&mut s, "group.query", json!({"query":"作画"}));
    assert_eq!(query["matching_group_ids"], json!([outer]));
    assert!(query["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "layer" && r["id"] == all[1]));
    call(
        &mut s,
        "brush.set",
        json!({"radius":1,"stabilizer":0.35,"pressure_size":true,"pressure_opacity":true}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":0.5,"y":0.5,"pressure":0.2,"pen":true,"time":0}),
    );
    let state = s.state();
    let history = s.doc.history_bytes();
    let count = s.journal.as_ref().unwrap().entries.len();
    let samples = s.replay_samples.len();
    let cache = s.doc.groups_revision();
    assert_eq!(
        call(&mut s, "group.query", json!({"selected":true}))["drawing"],
        true
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(s.replay_samples.len(), samples);
    for (id, args) in [
        ("group.set", json!({"group_id":inner,"name":"Other"})),
        ("group.select_layers", json!({"group_id":inner})),
    ] {
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "stroke_active"
        );
        assert_eq!(s.state(), state);
        assert!(s.is_drawing());
    }
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"move","x":1,"y":0.5,"pressure":1,"pen":true,"time":20}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":1.5,"y":0.5,"pressure":0.4,"pen":true,"time":40}),
    );
    assert_eq!(s.doc.groups_revision(), cache);
    assert_ne!(raw(&s), source);
    check(&mut s);
}

#[test]
fn groups_keep_leaf_filters_separate_and_disabling_effects_preserves_group_opacity() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    let mut graph = s.doc.graph.clone();
    filter(&mut graph, 1, 1.);
    call(&mut s, "graph.set", json!({"graph":graph}));
    let g = create(&mut s, json!([layer(all[1])]), "Filtered");
    call(&mut s, "group.set", json!({"group_id":g,"opacity":0.5}));
    assert_eq!(pixels(&mut s), [0xffff8000; 2]);
    assert!(s.doc.graph.linked);
    assert!(!s
        .doc
        .graph
        .nodes
        .iter()
        .any(|n| n.data == NodeKind::Transparent));
    let effective = s.doc.effective_graph().unwrap();
    assert!(!effective.linked);
    assert_eq!(
        effective
            .nodes
            .iter()
            .filter(|n| n.data == NodeKind::Transparent)
            .count(),
        1
    );
    let mut disabled = s.doc.graph.clone();
    disabled.enabled = false;
    call(&mut s, "graph.set", json!({"graph":disabled}));
    assert_eq!(pixels(&mut s), [0xff7f0080; 2]);
    assert!(s
        .doc
        .graph
        .nodes
        .iter()
        .any(|n| matches!(n.data, NodeKind::Filter { .. })));
    assert!(!s
        .doc
        .effective_graph()
        .unwrap()
        .nodes
        .iter()
        .any(|n| matches!(n.data, NodeKind::Filter { .. })));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let mut free = s.doc.graph.clone();
    free.linked = false;
    free.enabled = true;
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":free})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    check(&mut s);
}

#[test]
fn groups_structure_moves_buffers_and_duplicate_subtrees_copy_flags_filters_and_selection() {
    let mut s = source(&[0xff110000, 0xff220000, 0xff330000, 0xff440000]);
    let all = ids(&s);
    let source = raw(&s);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":all[0],"pixel_locked":true,"position_locked":true,"color_label":"violet"}),
    );
    let mut graph = s.doc.graph.clone();
    filter(&mut graph, 0, 0.5);
    call(&mut s, "graph.set", json!({"graph":graph}));
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[2],all[0]],"active_layer_id":all[2]}),
    );
    let inner = call(&mut s, "group.create_selected", json!({"name":"Ink 墨"}))["group_id"]
        .as_u64()
        .unwrap();
    assert_eq!(ids(&s), [all[0], all[2], all[1], all[3]]);
    for (i, id) in all.iter().enumerate() {
        assert_eq!(
            s.doc.layers[s.doc.layer_index(*id).unwrap()]
                .pixels
                .as_ptr(),
            pointers[i]
        );
    }
    assert_eq!(s.state()["selected_layer_ids"], json!([all[0], all[2]]));
    let outer = create(&mut s, json!([group(inner), layer(all[1])]), "Drawing");
    let before = s.doc.composition();
    let copied = call(&mut s, "group.duplicate", json!({"group_id":outer}))["group_id"]
        .as_u64()
        .unwrap();
    assert_ne!(copied, outer);
    let copied_ids = s.doc.group_layer_ids(copied).unwrap();
    assert_eq!(copied_ids.len(), 3);
    assert!(copied_ids.iter().all(|id| !all.contains(id)));
    assert_eq!(s.state()["selected_layer_ids"], json!(copied_ids));
    assert!(copied_ids.contains(&s.doc.layer_id(s.doc.active)));
    for (copy, original) in copied_ids.iter().zip([all[0], all[2], all[1]]) {
        let index = s.doc.layer_index(*copy).unwrap();
        let old = all.iter().position(|id| *id == original).unwrap();
        assert_eq!(s.doc.layers[index].pixels, source[old]);
    }
    let copied_first = s.doc.layer_index(copied_ids[0]).unwrap();
    assert!(s.doc.pixel_locked(copied_first) && s.doc.position_locked(copied_first));
    assert_eq!(s.state()["layers"][copied_first]["color_label"], "violet");
    assert_eq!(
        s.doc
            .graph
            .nodes
            .iter()
            .filter(|n| matches!(n.data, NodeKind::Filter { .. }))
            .count(),
        2
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition().groups, before.groups);
    assert_eq!(ids(&s), [all[0], all[2], all[1], all[3]]);
    assert_eq!(s.state()["selected_layer_ids"], json!([all[0], all[2]]));
    call(
        &mut s,
        "group.move",
        json!({"items":[group(inner)],"parent_id":0,"index":1}),
    );
    assert_eq!(ids(&s), [all[1], all[0], all[2], all[3]]);
    assert_eq!(
        s.doc.layer_parent_id(s.doc.layer_index(all[0]).unwrap()),
        inner
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(ids(&s), [all[0], all[2], all[1], all[3]]);
    check(&mut s);
}

#[test]
fn groups_empty_count_depth_cycle_and_instruction_limits_reject_before_changes() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    for index in 0..16 {
        create(&mut s, json!([]), &format!("Empty {index}"));
    }
    let before = s.state();
    let history = s.doc.history_bytes();
    let original_pixels = raw(&s);
    assert!(s
        .apply(Action::new(
            "group.create",
            json!({"members":[],"name":"Seventeen"})
        ))
        .is_err());
    assert!(s
        .apply(Action::new("group.select_layers", json!({"group_id":1})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(raw(&s), original_pixels);
    call(&mut s, "group.delete", json!({"group_id":1}));
    assert_eq!(raw(&s), original_pixels);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.groups_state()["groups"].as_array().unwrap().len(), 16);
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let first = s.doc.layer_id(0);
    let mut outer = create(&mut s, json!([layer(first)]), "Depth 1");
    let inner = outer;
    for depth in 2..=8 {
        outer = create(&mut s, json!([group(outer)]), &format!("Depth {depth}"));
    }
    let before = s.state();
    let composition = s.doc.composition();
    for (id, args) in [
        (
            "group.create",
            json!({"members":[group(outer)],"name":"Depth 9"}),
        ),
        (
            "group.move",
            json!({"items":[group(outer)],"parent_id":inner,"index":0}),
        ),
        (
            "group.move",
            json!({"items":[group(outer),layer(first)],"parent_id":0,"index":0}),
        ),
        ("group.set", json!({"group_id":inner,"name":"bad\nname"})),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err(), "{id}");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
    }
    for invalid in [
        json!({"members":[],"name":" "}),
        json!({"members":[],"name":"bad\nname"}),
        json!({"members":[],"name":"墨".repeat(171)}),
        json!({"members":[layer(first),layer(first)],"name":"Duplicate refs"}),
    ] {
        let result = s.apply(Action::new(
            "pipeline.run",
            json!({"atomic":false,"steps":[
                {"id":"earlier","action":"layer.set","args":{"color_label":"red"}},
                {"id":"invalid","action":"group.create","args":invalid}
            ]}),
        ));
        assert!(result.is_err(), "{result:?}");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
    }
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    create(&mut s, json!([layer(all[0]), layer(all[1])]), "Budget");
    let mut graph = s.doc.graph.clone();
    for _ in 0..58 {
        filter(&mut graph, 0, 0.);
    }
    assert!(graph.compile(2).is_ok());
    let before = s.state();
    let composition = s.doc.composition();
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":graph})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    check(&mut s);
}

struct LegacyRenderer(Arc<dyn Renderer>);
impl Renderer for LegacyRenderer {
    fn render(
        &self,
        nodes: &[micopi_plugin_api::CompositeNode],
        layers: &[micopi_plugin_api::CompositeLayer],
        w: u32,
        h: u32,
        out: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        self.0.render(nodes, layers, w, h, out)
    }
}

struct PartialFailureRenderer {
    delegate: Arc<dyn Renderer>,
    armed: Arc<AtomicBool>,
    failures: Arc<AtomicU64>,
}
impl Renderer for PartialFailureRenderer {
    fn supports_transparent(&self) -> bool {
        true
    }
    fn render(
        &self,
        nodes: &[micopi_plugin_api::CompositeNode],
        layers: &[micopi_plugin_api::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        if self.armed.load(Ordering::Relaxed) {
            self.failures.fetch_add(1, Ordering::Relaxed);
            if output.width != 0 && output.height != 0 && output.len != 0 && !output.data.is_null()
            {
                unsafe {
                    *output.data = 0xff123456;
                }
            }
            return Err("Injected partial group composite failure".into());
        }
        self.delegate.render(nodes, layers, width, height, output)
    }
}

#[test]
fn groups_renderer_failure_restores_effective_plan_and_committed_composite_before_retry() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":all,"active_layer_id":all[1]}),
    );
    let inner = create(&mut s, json!([layer(all[0]), layer(all[1])]), "Committed");
    call(&mut s, "group.set", json!({"group_id":inner,"opacity":0.5}));
    let armed = Arc::new(AtomicBool::new(false));
    let failures = Arc::new(AtomicU64::new(0));
    s.doc.renderer = Some(Arc::new(PartialFailureRenderer {
        delegate: s.doc.renderer.clone().unwrap(),
        armed: armed.clone(),
        failures: failures.clone(),
    }));
    let path = output("renderer-failure.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    assert!(!s.doc.dirty);
    let committed = pixels(&mut s);
    s.replay_boundary().unwrap();
    let before = s.state();
    let composition = s.doc.composition();
    let effective = s.doc.effective_graph().unwrap();
    let sources = raw(&s);
    let history = s.doc.history_bytes();
    let prefix: Vec<_> = s
        .journal
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.hash)
        .collect();
    armed.store(true, Ordering::Relaxed);
    assert!(s
        .apply(Action::new(
            "group.create",
            json!({"members":[group(inner)],"name":"Retry outer"})
        ))
        .is_err());
    assert!(failures.load(Ordering::Relaxed) > 0);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.effective_graph().unwrap(), effective);
    assert_eq!(raw(&s), sources);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(!s.doc.dirty);
    assert!(s.replay_error.is_none());
    // Inspect the committed cache while the failing renderer is still armed;
    // recomposing after disarming would hide a partial-output rollback defect.
    assert_eq!(s.doc.composite, committed);
    assert_eq!(
        s.journal
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .take(prefix.len())
            .map(|e| e.hash)
            .collect::<Vec<_>>(),
        prefix
    );
    // graph.set reaches composition through journal preparation rather than
    // the explicit group handler's earlier render check.
    let mut disabled = s.doc.graph.clone();
    disabled.enabled = false;
    let failed_renders = failures.load(Ordering::Relaxed);
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":disabled})))
        .is_err());
    assert!(failures.load(Ordering::Relaxed) > failed_renders);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.effective_graph().unwrap(), effective);
    assert_eq!(raw(&s), sources);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composite, committed);
    assert!(!s.doc.dirty);
    assert!(s.replay_error.is_none());
    armed.store(false, Ordering::Relaxed);
    call(&mut s, "graph.set", json!({"graph":disabled}));
    assert!(!s.doc.graph.enabled);
    assert_eq!(pixels(&mut s), committed);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.effective_graph().unwrap(), effective);
    assert_eq!(pixels(&mut s), committed);
    let retried = create(&mut s, json!([group(inner)]), "Retry outer");
    assert_eq!(retried, composition.next_group_id);
    call(
        &mut s,
        "group.set",
        json!({"group_id":retried,"opacity":0.5}),
    );
    assert_eq!(pixels(&mut s), [0x400000ff; 2]);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(pixels(&mut s), committed);
    assert_eq!(s.doc.effective_graph().unwrap(), effective);
    assert_eq!(raw(&s), sources);
    check(&mut s);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn groups_require_optional_renderer_capability_without_breaking_original_graphs() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let original = s.doc.renderer.clone().unwrap();
    s.doc.renderer = Some(Arc::new(LegacyRenderer(original.clone())));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    assert!(s
        .apply(Action::new(
            "group.create",
            json!({"members":[],"name":"Unsupported"})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    let graph = Graph {
        enabled: true,
        linked: false,
        nodes: vec![
            Node {
                id: 0,
                node_type: Some("compositing.transparent".into()),
                position: [0., 0.],
                data: NodeKind::Transparent,
            },
            Node {
                id: 1,
                node_type: Some("compositing.output".into()),
                position: [100., 0.],
                data: NodeKind::Output,
            },
        ],
        edges: vec![Edge {
            from: 0,
            to: 1,
            input: 0,
        }],
    };
    assert!(s
        .apply(Action::new("graph.set", json!({"graph":graph})))
        .is_err());
    assert_eq!(s.doc.composition(), composition);
    s.doc.renderer = Some(original);
    let all = ids(&s);
    create(&mut s, json!([layer(all[1])]), "Saved");
    let path = output("capability.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = session();
    let before = opened.state();
    let composition = opened.doc.composition();
    opened.registry.native_plugins.clear();
    assert!(opened
        .apply(Action::new("document.open", json!({"path":path})))
        .is_err());
    assert_eq!(opened.state(), before);
    assert_eq!(opened.doc.composition(), composition);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn groups_atomic_and_partial_journal_failure_restore_selection_buffers_and_allocator() {
    let mut s = source(&[0xff110000, 0xff220000, 0xff330000, 0xff440000]);
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]],"active_layer_id":all[2]}),
    );
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"create","action":"group.create_selected","args":{"name":"Temporary"}},
            {"id":"set","action":"group.set","args":{"group_id":{"$ref":"create#/result/group_id"},"opacity":0.5}},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "group.create_selected",
            json!({"name":"Rollback"})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), source);
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
}

#[test]
fn groups_malformed_metadata_and_small_projects_reject_before_document_replacement() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    let g = create(
        &mut s,
        json!([layer(all[0]), layer(all[1])]),
        "Valid grouped document",
    );
    call(&mut s, "group.set", json!({"group_id":g,"opacity":0.5}));
    let valid = serde_json::to_value(s.doc.composition()).unwrap();
    let mut cases = Vec::new();
    let mut invalid = valid.clone();
    invalid["groups"]["version"] = json!(4);
    cases.push(("unknown-version", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["version"] = json!(3);
    cases.push(("v3-missing-adjustment-allocator", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["version"] = json!(1);
    invalid["groups"]["groups"][0]["compositing"] = json!("pass_through");
    cases.push(("v1-pass-through-policy", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["groups"][0]["children"] =
        json!([layer(all[0]), layer(all[0]), layer(all[1])]);
    cases.push(("duplicate-membership", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["groups"][0]["children"] = json!([layer(all[0]), group(g), layer(all[1])]);
    cases.push(("cyclic-membership", invalid));
    let mut invalid = valid.clone();
    invalid["next_group_id"] = json!(g);
    cases.push(("invalid-allocator", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["groups"][0]["children"] = json!([layer(all[0])]);
    cases.push(("missing-raster", invalid));
    let mut invalid = valid.clone();
    invalid["groups"]["groups"][0]["children"] = json!([layer(all[0]), layer(99999)]);
    cases.push(("nonmatching-raster", invalid));
    let mut invalid = valid;
    invalid["layer_metadata"] = json!([]);
    invalid["next_layer_id"] = json!(0);
    invalid.as_object_mut().unwrap().remove("groups");
    invalid["next_group_id"] = json!(u64::MAX);
    cases.push(("legacy-invalid-group-allocator", invalid));

    let png = codec::png(&mut s.doc).unwrap();
    let before = s.state();
    let composition = s.doc.composition();
    let sources = raw(&s);
    let history = s.doc.history_bytes();
    let identity = s.doc.identity;
    for (name, invalid) in cases {
        let mut metadata = s.replay_metadata();
        metadata.composition = serde_json::from_value(invalid.clone()).unwrap();
        assert!(metadata.validate().is_err(), "{name}");
        // A two-pixel MICOPI02 fixture exercises the project reader separately
        // from replay Metadata validation, without an archive or large buffers.
        let mut project = b"MICOPI02".to_vec();
        for dimension in [s.doc.width, s.doc.height, s.doc.layers.len(), s.doc.active] {
            project.extend_from_slice(&(dimension as u32).to_le_bytes());
        }
        for layer in &s.doc.layers {
            project.extend([layer.visible as u8, layer.opacity]);
            for pixel in &layer.pixels {
                project.extend_from_slice(&pixel.to_le_bytes());
            }
        }
        let encoded = serde_json::to_vec(&invalid).unwrap();
        project.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        project.extend_from_slice(&encoded);
        let path = output(&format!("{name}.mcp"));
        std::fs::write(&path, project).unwrap();
        assert!(
            s.apply(Action::new("document.open", json!({"path":path})))
                .is_err(),
            "{name}"
        );
        assert_eq!(s.state(), before, "{name}");
        assert_eq!(s.doc.composition(), composition, "{name}");
        assert_eq!(raw(&s), sources, "{name}");
        assert_eq!(s.doc.history_bytes(), history, "{name}");
        assert_eq!(s.doc.identity, identity, "{name}");
        assert_eq!(codec::png(&mut s.doc).unwrap(), png, "{name}");
        std::fs::remove_file(path).unwrap();
    }
    check(&mut s);
}

#[test]
fn groups_nodes_workflow_projects_and_outcome_replay_preserve_sparse_legacy_defaults() {
    let mut s = source(&[0xffff0000, 0xff0000ff]);
    let all = ids(&s);
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(
        catalog.contains("action.group.create")
            && catalog.contains("action.group.query")
            && catalog.contains("compositing.transparent")
    );
    let defaults = serde_json::to_value(s.doc.composition()).unwrap();
    assert!(defaults.get("groups").is_none());
    assert!(defaults.get("next_group_id").is_none());
    let raw_before = raw(&s);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.group.create","position":[0,0],"parameters":{"members":[layer(all[0]),layer(all[1])],"name":"Colors 彩"}},
        {"id":2,"type":"action.group.set","position":[400,0],"parameters":{"opacity":0.5}}
    ],"edges":[{"from":1,"output":"group_id","to":2,"input":"group_id"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert!(!s.doc.has_groups());
    assert_eq!(raw(&s), raw_before);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert!(!s.doc.has_groups());
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(pixels(&mut s), [0x800000ff; 2]);
    let path = output("groups.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    assert_eq!(raw(&reopened), raw_before);
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), png);
    check(&mut reopened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    assert_eq!(
        journal.preview(last).unwrap().metadata.composition,
        composition
    );
    assert_eq!(replay::png(&journal.preview(last).unwrap()).unwrap(), png);
    let mut legacy =
        Journal::import(include_bytes!("../../tests/fixtures/replay/v1.mcreplay")).unwrap();
    let head = legacy.status()["head_hash"].clone();
    assert_eq!(legacy.verify(None).unwrap()["head_hash"], head);
    std::fs::remove_file(path).unwrap();
}
