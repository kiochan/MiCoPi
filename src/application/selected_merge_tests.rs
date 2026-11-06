//! Explicit selected-only baking preserves outside data and documents stacking changes.
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
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};

const OPAQUE: [u32; 26] = [
    0xff4080c0, 0xff202018, 0xffa0a0c8, 0xff414030, 0xff404020, 0xff8080c0, 0xffc0c0e0, 0xff4040a0,
    0xff000000, 0xff000000, 0xff804020, 0xffab8182, 0xff4080c0, 0xff60403c, 0xff404191, 0xff024041,
    0xff0141a1, 0xff804081, 0xff000000, 0xff8080b0, 0xff400000, 0xffff802b, 0xff295989, 0xff903b10,
    0xff1c5c9c, 0xffa46444,
];
fn source(s: &mut Session, colors: &[u32]) {
    call(s, "canvas.new", json!({"width":2,"height":1}));
    for (i, pixel) in colors.iter().enumerate() {
        if i > 0 {
            call(s, "layer.add", json!({}));
        }
        s.doc.replace_region(0, 0, 2, 1, &[*pixel; 2]);
        call(s, "layer.rename", json!({"name":format!("Cel {} 墨",i+1)}));
    }
    s.replay_boundary().unwrap();
}
fn ids(s: &Session) -> Vec<u64> {
    (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect()
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none(), "{:?}", s.doc.render_error);
    s.doc.composite.clone()
}
fn layer(id: u64) -> Value {
    json!({"kind":"layer","id":id})
}
fn create(s: &mut Session, members: Value, name: &str) -> u64 {
    call(s, "group.create", json!({"members":members,"name":name}))["group_id"]
        .as_u64()
        .unwrap()
}
fn plan(s: &mut Session, targets: &[u64]) -> Value {
    call(s, "layer.merge_selected.plan", json!({"layer_ids":targets}))
}
fn merge(s: &mut Session, targets: &[u64]) -> Value {
    call(s, "layer.merge_selected", json!({"layer_ids":targets}))
}
fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/selected-merge-units");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn invert(s: &mut Session, id: u64) -> u32 {
    let index = s.doc.layer_index(id).unwrap();
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|n| n.data == (NodeKind::Layer { index }))
        .unwrap()
        .id;
    let next = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = next;
        }
    }
    graph.edges.push(Edge {
        from: source,
        to: next,
        input: 0,
    });
    graph.nodes.push(Node {
        id: next,
        node_type: Some("compositing.filter".into()),
        position: [0., 0.],
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 1.,
        },
    });
    call(s, "graph.set", json!({"graph":graph}));
    next
}
fn attenuate(p: u32, t: u32) -> u32 {
    let a = (p >> 24) * t / 255;
    if a == 0 {
        0
    } else {
        (p & 0xffffff) | (a << 24)
    }
}
fn mix(before: u32, after: u32, t: u32) -> u32 {
    let (a, b) = ((before >> 24) as u64, (after >> 24) as u64);
    let w = a * (255 - t) as u64 + b * t as u64;
    let alpha = (w + 127) / 255;
    if alpha == 0 {
        return 0;
    }
    let mut out = (alpha as u32) << 24;
    for shift in [16, 8, 0] {
        let n = ((before >> shift) & 255) as u64 * a * (255 - t) as u64
            + ((after >> shift) & 255) as u64 * b * t as u64;
        out |= (((2 * n + w) / (2 * w)) as u32) << shift;
    }
    out
}
fn over(before: u32, after: u32, mode: u32) -> u32 {
    // Normalized straight-alpha reference with the established legacy byte stages.
    let (ab, aa) = ((before >> 24) as f64 / 255., (after >> 24) as f64 / 255.);
    if aa == 0. {
        return before;
    }
    let alpha = ((aa + ab * (1. - aa)) * 255. + 1e-9).round() as u32;
    let mut out = alpha << 24;
    for shift in [16, 8, 0] {
        let b = ((before >> shift) & 255) as f64;
        let a = ((after >> shift) & 255) as f64;
        let blend = match mode {
            0 => a,
            1 => a * b / 255.,
            2 => 255. - (255. - a) * (255. - b) / 255.,
            _ => panic!("reference mode"),
        };
        let blend = (blend + 1e-9).round();
        let n = (1. - aa) * ab * b + (1. - ab) * aa * a + aa * ab * blend;
        out |= ((n / (alpha as f64 / 255.) + 127. / 255. + 1e-9)
            .floor()
            .clamp(0., 255.) as u32)
            << shift;
    }
    out
}

#[test]
fn noncontiguous_selected_merge_excludes_intervening_pixels_retains_mask_and_restores_one_undo() {
    let mut s = session();
    source(&mut s, &[0xffff0000, 0xff00ff00, 0xff0000ff]);
    s.doc.replace_region(0, 0, 2, 1, &[0, 0xff0000ff]);
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":3,"color_label":"violet","position_locked":true}),
    );
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,3],"active_layer_id":1}),
    );
    let before = s.doc.composition();
    let original = raw(&s);
    let active = s.doc.layer_id(s.doc.active);
    let pointer = s.doc.layers[1].pixels.as_ptr();
    let old = pixels(&mut s);
    assert_eq!(old, [0xff00ff00, 0xff0000ff]);
    let p = plan(&mut s, &[3, 1]);
    assert_eq!(p["layer_ids"], json!([1, 3]));
    assert_eq!(p["common_parent_id"], 0);
    assert_eq!(p["parent_index"], 1);
    assert_eq!(p["layer_id"], 3);
    assert_eq!(p["interleaved_unselected_ids"], json!([2]));
    assert_eq!(p["consumed_hidden_ids"], json!([]));
    assert_eq!(p["changes_stacking"], true);
    assert_eq!(p["backdrop_policy"], "selected_only_transparent");
    assert_eq!(p["output_buffer_bytes"], 8);
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = merge(&mut s, &[3, 1]);
    assert_eq!(result["layer_id"], 3);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(ids(&s), vec![2, 3]);
    assert_eq!(s.doc.layers[0].pixels.as_ptr(), pointer);
    assert_eq!(s.doc.layers[0].pixels, original[1]);
    assert_eq!(s.doc.layers[1].pixels, [0xffff0000, 0xff0000ff]);
    assert_eq!(pixels(&mut s), [0xffff0000, 0xff0000ff]);
    assert_ne!(pixels(&mut s), old);
    assert_eq!(s.doc.layer_name(1), "Cel 3 墨");
    assert_eq!(s.state()["layers"][1]["color_label"], "violet");
    assert!(s.doc.position_locked(1));
    assert!(!s.doc.alpha_locked(1) && !s.doc.pixel_locked(1));
    assert_eq!(s.doc.layers[1].mode, BlendMode::Normal);
    assert_eq!(s.doc.layers[1].opacity, 255);
    assert_eq!(s.doc.selected_layer_ids(), vec![3]);
    assert_eq!(s.doc.composition().selection, before.selection);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.layer_id(s.doc.active), active);
    assert_eq!(s.doc.selected_layer_ids(), vec![1, 3]);
    assert_eq!(pixels(&mut s), old);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(ids(&s), vec![2, 3]);
    assert_eq!(s.doc.layers[1].pixels, [0xffff0000, 0xff0000ff]);
    check(&mut s);
}

#[test]
fn selected_only_baking_supports_all_26_modes_with_reversed_explicit_ids() {
    let mut s = session();
    for (mode, expected) in BlendMode::ALL.into_iter().zip(OPAQUE) {
        source(&mut s, &[0xff804020, 0xff17c832, 0xff4080c0]);
        call(&mut s, "layer.set", json!({"layer_id":3,"blend_mode":mode}));
        let unselected = s.doc.layers[1].pixels.clone();
        merge(&mut s, &[3, 1]);
        assert_eq!(ids(&s), vec![2, 3]);
        assert_eq!(s.doc.layers[1].pixels, [expected; 2], "{mode:?}");
        assert_eq!(s.doc.layers[0].pixels, unselected);
        check(&mut s);
    }
}

#[test]
fn cross_parent_projection_keeps_intermediate_policies_and_surviving_buffers_and_filter_ids() {
    let mut s = session();
    for enabled in [true, false] {
        source(
            &mut s,
            &[0x80e04020, 0xff00ff00, 0x903399cc, 0xffeeee00, 0x40885522],
        );
        let selected_filter = invert(&mut s, 3);
        let untouched_filter = invert(&mut s, 2);
        let other_filter = invert(&mut s, 4);
        let left = create(&mut s, json!([layer(1), layer(2)]), "Left pass");
        call(
            &mut s,
            "group.set",
            json!({"group_id":left,"compositing":"pass_through","opacity":128./255.}),
        );
        let right = create(&mut s, json!([layer(3), layer(4)]), "Right isolated");
        call(
            &mut s,
            "group.set",
            json!({"group_id":right,"opacity":179./255.,"blend_mode":"screen"}),
        );
        let mut graph = s.doc.graph.clone();
        graph.enabled = enabled;
        call(&mut s, "graph.set", json!({"graph":graph}));
        let survivor: Vec<_> = [2, 4, 5]
            .map(|id| {
                let i = s.doc.layer_index(id).unwrap();
                (
                    id,
                    s.doc.layers[i].pixels.as_ptr(),
                    s.doc.layers[i].pixels.clone(),
                )
            })
            .into_iter()
            .collect();
        let before = s.doc.composition();
        let original = raw(&s);
        let p = plan(&mut s, &[3, 1]);
        assert_eq!(p["common_parent_id"], 0);
        assert_eq!(p["parent_index"], 2);
        assert_eq!(p["interleaved_unselected_ids"], json!([2, 4]));
        assert_eq!(p["changes_stacking"], true);
        let foreground = if enabled { 0x90cc6633 } else { 0x903399cc };
        let expected = over(mix(0, 0x80e04020, 128), attenuate(foreground, 179), 2);
        merge(&mut s, &[3, 1]);
        assert_eq!(ids(&s), vec![2, 4, 3, 5]);
        assert_eq!(s.doc.layer_parent_id(s.doc.layer_index(3).unwrap()), 0);
        assert_eq!(
            s.doc.layers[s.doc.layer_index(3).unwrap()].pixels,
            [expected; 2]
        );
        assert_eq!(s.doc.group_layer_ids(left).unwrap(), vec![2]);
        assert_eq!(s.doc.group_layer_ids(right).unwrap(), vec![4]);
        for (id, pointer, pixels) in survivor {
            let i = s.doc.layer_index(id).unwrap();
            assert_eq!(s.doc.layers[i].pixels.as_ptr(), pointer);
            assert_eq!(s.doc.layers[i].pixels, pixels);
        }
        assert!(!s.doc.graph.nodes.iter().any(|n| n.id == selected_filter));
        for id in [untouched_filter, other_filter] {
            assert!(s.doc.graph.nodes.iter().any(|n| n.id == id));
        }
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.composition(), before);
        assert_eq!(raw(&s), original);
        check(&mut s);
    }
}

#[test]
fn common_parent_gates_remain_outside_bake_and_hidden_local_or_intermediate_targets_bake_zero() {
    let mut s = session();
    source(&mut s, &[0x80ff0000, 0xff00ff00, 0x900000ff]);
    let parent = create(&mut s, json!([layer(1), layer(2), layer(3)]), "Hidden LCA");
    call(
        &mut s,
        "group.set",
        json!({"group_id":parent,"visible":false,"opacity":137./255.,"compositing":"pass_through"}),
    );
    let p = plan(&mut s, &[1, 3]);
    assert_eq!(p["common_parent_id"], parent);
    assert_eq!(p["consumed_hidden_ids"], json!([1, 3]));
    assert_eq!(p["parent_index"], 1);
    let expected = over(0x80ff0000, 0x900000ff, 0);
    merge(&mut s, &[1, 3]);
    let index = s.doc.layer_index(3).unwrap();
    assert_eq!(s.doc.layers[index].pixels, [expected; 2]);
    assert_eq!(s.doc.layer_parent_id(index), parent);
    assert_eq!(pixels(&mut s), [0; 2]);
    let info = call(&mut s, "group.query", json!({}))["groups"][0].clone();
    assert_eq!(info["opacity"], json!(137f32 / 255.));
    assert_eq!(info["compositing"], "pass_through");
    assert_eq!(info["visible"], false);
    source(&mut s, &[0xffff0000, 0xff00ff00, 0xff0000ff, 0xffffffff]);
    let hidden = create(&mut s, json!([layer(1), layer(2)]), "Hidden intermediate");
    call(
        &mut s,
        "group.set",
        json!({"group_id":hidden,"visible":false}),
    );
    call(&mut s, "layer.set", json!({"layer_id":3,"visible":false}));
    let p = plan(&mut s, &[1, 3, 4]);
    assert_eq!(p["consumed_hidden_ids"], json!([1, 3]));
    merge(&mut s, &[1, 3, 4]);
    assert_eq!(
        s.doc.layers[s.doc.layer_index(4).unwrap()].pixels,
        [0xffffffff; 2]
    );
    assert_eq!(s.doc.group_layer_ids(hidden).unwrap(), vec![2]);
    check(&mut s);
}

struct CountRenderer {
    delegate: Arc<dyn Renderer>,
    calls: Arc<AtomicUsize>,
    armed: Arc<AtomicBool>,
}
impl Renderer for CountRenderer {
    fn supports_mix(&self) -> bool {
        self.delegate.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.delegate.supports_transparent()
    }
    fn supports_blend_mode(&self, m: u32) -> bool {
        self.delegate.supports_blend_mode(m)
    }
    fn render(
        &self,
        n: &[micopi_plugin_api::CompositeNode],
        l: &[micopi_plugin_api::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.armed.load(Ordering::Relaxed) {
            if !o.data.is_null() && o.len > 0 {
                unsafe {
                    *o.data = 0xff123456;
                }
            }
            return Err("Injected selected merge partial output".into());
        }
        self.delegate.render(n, l, w, h, o)
    }
}
#[test]
fn merge_plan_is_inert_during_pressure_and_hand_contacts_and_execution_preserves_live_contact_on_rejection(
) {
    let mut s = session();
    source(&mut s, &[0x80ff0000, 0xff00ff00, 0x900000ff]);
    let calls = Arc::new(AtomicUsize::new(0));
    s.doc.renderer = Some(Arc::new(CountRenderer {
        delegate: s.doc.renderer.clone().unwrap(),
        calls: calls.clone(),
        armed: Arc::new(AtomicBool::new(false)),
    }));
    for tool in ["brush", "hand"] {
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"down","x":0.5,"y":0.5,"pen":true,"pressure":0.2,"time":1,"tool_override":tool}),
        );
        let state = s.state();
        let composition = s.doc.composition();
        let original = raw(&s);
        let history = s.doc.history_bytes();
        let events = s.journal.as_ref().unwrap().entries.len();
        let revision = s.revision;
        calls.store(0, Ordering::Relaxed);
        let p = plan(&mut s, &[3, 1]);
        assert_eq!(p["layer_id"], 3);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.revision, revision);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
        assert_eq!(
            s.apply(Action::new(
                "layer.merge_selected",
                json!({"layer_ids":[1,3]})
            ))
            .unwrap_err()
            .code,
            "stroke_active"
        );
        assert!(s.stroke.active());
        assert_eq!(raw(&s), original);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.state(), state);
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"move","x":1.,"y":0.5,"pen":true,"pressure":1.,"time":20,"tool_override":tool}),
        );
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"up","x":1.5,"y":0.5,"pen":true,"pressure":0.4,"time":40,"tool_override":tool}),
        );
        assert!(!s.stroke.active());
    }
    check(&mut s);
}

#[test]
fn explicit_merge_validates_all_ids_literals_and_hidden_locks_and_rejects_actual_free_graphs() {
    let mut s = session();
    source(&mut s, &[0xffff0000, 0xff00ff00, 0xff0000ff]);
    for invalid in [
        json!({"layer_ids":[]}),
        json!({"layer_ids":[1]}),
        json!({"layer_ids":[1,1]}),
        json!({"layer_ids":[1,"3"]}),
        json!({"layer_ids":null}),
        json!({"layer_ids":[1,3],"extra":true}),
    ] {
        let before = s.doc.composition();
        assert!(s.apply(Action::new("pipeline.run",json!({"atomic":false,"steps":[{"id":"name","action":"layer.rename","args":{"name":"Must not execute"}},{"id":"bad","action":"layer.merge_selected","args":invalid}]}))).is_err());
        assert_eq!(s.doc.composition(), before);
    }
    for targets in [vec![1, 99999], vec![0, 3]] {
        let before = s.doc.composition();
        assert!(s
            .apply(Action::new(
                "layer.merge_selected",
                json!({"layer_ids":targets})
            ))
            .is_err());
        assert_eq!(s.doc.composition(), before);
    }
    call(&mut s, "layer.set", json!({"layer_id":1,"visible":false}));
    for lock in ["pixel_locked", "alpha_locked"] {
        let mut args = json!({"layer_id":1});
        args[lock] = json!(true);
        call(&mut s, "layer.set", args.clone());
        let before = s.doc.composition();
        let original = raw(&s);
        assert!(s
            .apply(Action::new(
                "layer.merge_selected",
                json!({"layer_ids":[1,3]})
            ))
            .is_err());
        assert_eq!(s.doc.composition(), before);
        assert_eq!(raw(&s), original);
        args[lock] = json!(false);
        call(&mut s, "layer.set", args);
    }
    for enabled in [true, false] {
        let mut free = Graph::from_layers(&s.doc.layers);
        free.enabled = enabled;
        call(&mut s, "graph.set", json!({"graph":free}));
        let before = s.doc.composition();
        assert!(s
            .apply(Action::new(
                "layer.merge_selected",
                json!({"layer_ids":[1,3]})
            ))
            .is_err());
        assert_eq!(s.doc.composition(), before);
    }
    s.doc.graph = Graph::default();
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    assert_eq!(plan(&mut s, &[1, 3])["common_parent_id"], 0);
    merge(&mut s, &[1, 3]);
    assert!(s.doc.graph.linked);
    check(&mut s);
}

#[test]
fn selected_merge_partial_render_atomic_and_journal_failures_restore_sources_selection_history_and_caches(
) {
    let mut s = session();
    source(&mut s, &[0x80ff0000, 0xff00ff00, 0x900000ff]);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":1}),
    );
    let armed = Arc::new(AtomicBool::new(false));
    s.doc.renderer = Some(Arc::new(CountRenderer {
        delegate: s.doc.renderer.clone().unwrap(),
        calls: Arc::new(AtomicUsize::new(0)),
        armed: armed.clone(),
    }));
    let path = output("clean.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let before = s.state();
    let composition = s.doc.composition();
    let original = raw(&s);
    let committed = pixels(&mut s);
    let graph = s.doc.effective_graph().unwrap();
    let history = s.doc.history_bytes();
    armed.store(true, Ordering::Relaxed);
    assert!(s
        .apply(Action::new(
            "layer.merge_selected",
            json!({"layer_ids":[3,1]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.composite, committed);
    assert_eq!(s.doc.effective_graph().unwrap(), graph);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(!s.doc.dirty);
    assert!(s.replay_error.is_none());
    armed.store(false, Ordering::Relaxed);
    merge(&mut s, &[3, 1]);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    let before = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"merge","action":"layer.merge_selected","args":{"layer_ids":[3,1]}},{"id":"bad","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "layer.merge_selected",
            json!({"layer_ids":[3,1]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.composite, committed);
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn selected_merge_schema_workflow_result_identity_projects_and_outcome_replay_share_one_contract() {
    let mut s = session();
    source(&mut s, &[0x80ff0000, 0xff00ff00, 0x900000ff]);
    let id = create(
        &mut s,
        json!([layer(1), layer(2), layer(3)]),
        "Persisted LCA",
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"pass_through","opacity":137./255.}),
    );
    let schema = s.registry.actions["layer.merge_selected"].schema();
    assert_eq!(schema["properties"]["layer_ids"]["minItems"], 2);
    assert_eq!(schema["properties"]["layer_ids"]["maxItems"], 16);
    assert!(s.registry.actions["layer.merge_selected"].is_document_atomic());
    assert!(s.registry.actions["layer.merge_selected.plan"].is_read_only());
    let queried = call(&mut s, "group.query", json!({}));
    assert_eq!(queried["version"], 2);
    crate::plugins::validate(
        s.registry.actions["group.query"]
            .output_schema
            .as_ref()
            .unwrap(),
        &queried,
    )
    .unwrap();
    let public = call(&mut s, "context.read", json!({"resources":["core.groups"]}));
    let public = &public["resources"]["core.groups"]["value"];
    assert_eq!(public["version"], 2);
    crate::plugins::validate(&s.registry.states["core.groups"].output_schema, public).unwrap();
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(
        catalog.contains("action.layer.merge_selected") && catalog.contains("common_parent_id")
    );
    let mut expected = s.doc.composition();
    let original = raw(&s);
    let png = codec::png(&mut s.doc).unwrap();
    let draft = json!({"version":1,"nodes":[{"id":1,"type":"action.layer.merge_selected","position":[0,0],"parameters":{"layer_ids":[3,1]}},{"id":2,"type":"action.layer.rename","position":[400,0],"parameters":{"name":"Merged selected 彩"}}],"edges":[{"from":1,"output":"layer_id","to":2,"input":"layer_id"}]});
    expected.workflow = serde_json::from_value(draft.clone()).unwrap();
    call(&mut s, "workflow.set", json!({"graph":draft}));
    assert_eq!(s.doc.composition(), expected);
    assert_eq!(raw(&s), original);
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
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
    assert_eq!(s.doc.layer_id(s.doc.active), 3);
    assert_eq!(s.doc.layer_name(s.doc.active), "Merged selected 彩");
    let composition = s.doc.composition();
    let png = codec::png(&mut s.doc).unwrap();
    let path = output("selected.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composition(), composition);
    assert_eq!(codec::png(&mut opened.doc).unwrap(), png);
    check(&mut opened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    let replayed = journal.preview(last).unwrap();
    assert_eq!(replayed.metadata.composition, composition);
    assert_eq!(replay::png(&replayed).unwrap(), png);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn contextual_merge_uses_current_multi_selection_or_legacy_singleton_down_and_review_is_presentation_only(
) {
    let mut s = session();
    source(&mut s, &[0xffff0000, 0xff00ff00, 0xff0000ff]);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,3],"active_layer_id":1}),
    );
    let before = s.doc.composition();
    let revision = s.revision;
    assert_eq!(call(&mut s, "layer.merge", json!({}))["layer_id"], 3);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(ids(&s), vec![2, 3]);
    assert_eq!(s.doc.selected_layer_ids(), vec![3]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.layer_id(s.doc.active), 1);
    call(&mut s, "layer.select", json!({"layer_id":3}));
    assert_eq!(call(&mut s, "layer.merge", json!({}))["layer_id"], 2);
    assert_eq!(ids(&s), vec![1, 2]);
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":0.5,"y":0.5,"pen":true,"tool_override":"hand"}),
    );
    let before = s.state();
    assert_eq!(
        s.apply(Action::new("layer.merge", json!({})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert_eq!(
        s.apply(Action::new(
            "layer.merge_selected.open",
            json!({"layer_ids":[1,3]})
        ))
        .unwrap_err()
        .code,
        "ui_unavailable"
    );
    assert_eq!(s.state(), before);
    assert!(s.stroke.active());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":0.5,"y":0.5,"pen":true,"tool_override":"hand"}),
    );
    check(&mut s);
}

#[test]
#[ignore = "Release selected-merge diagnostic; no CI timing threshold"]
fn selected_merge_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Run this diagnostic with --release"
    );
    const SIDE: usize = 1024;
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":SIDE,"height":SIDE}));
    let mut random = 0x5eed1234u32;
    let originals: Vec<Vec<u32>> = (0..4)
        .map(|layer| {
            (0..SIDE * SIDE)
                .map(|_| {
                    random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                    if layer == 0 {
                        random | 0xff000000
                    } else {
                        random
                    }
                })
                .collect()
        })
        .collect();
    for (index, source) in originals.iter().enumerate() {
        if index != 0 {
            call(&mut s, "layer.add", json!({}));
        }
        s.doc.replace_region(0, 0, SIDE, SIDE, source);
        call(
            &mut s,
            "layer.rename",
            json!({"name":format!("Prepared {}",["A","U","B","V"][index])}),
        );
    }
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"pixel_locked":true,"position_locked":true,"color_label":"blue"}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":4,"alpha_locked":true,"position_locked":true,"color_label":"yellow"}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,3],"active_layer_id":1}),
    );

    // Opaque A makes independent Normal SRCOVER an exact weighted byte sum.
    // The intervening U and upper V are absent from this selected-only reference.
    let expected: Vec<u32> = originals[0]
        .iter()
        .zip(&originals[2])
        .map(|(&a, &b)| {
            let alpha = b >> 24;
            let mut pixel = 0xff000000;
            for shift in [16, 8, 0] {
                let d = (a >> shift) & 255;
                let src = (b >> shift) & 255;
                pixel |= ((d * (255 - alpha) + src * alpha + 127) / 255) << shift;
            }
            pixel
        })
        .collect();
    s.replay_boundary().unwrap();
    pixels(&mut s);
    let composition = s.doc.composition();
    let before = s.state();
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let attributes = |s: &Session, id: u64| {
        let index = s.doc.layer_index(id).unwrap();
        let mut row = s.layer_state(index);
        row.as_object_mut().unwrap().remove("index");
        row
    };
    let survivors: Vec<_> = [2, 4]
        .into_iter()
        .map(|id| {
            let index = s.doc.layer_index(id).unwrap();
            (id, s.doc.layers[index].pixels.as_ptr(), attributes(&s, id))
        })
        .collect();

    let mut plan_samples = Vec::new();
    for _ in 0..5 {
        let action = Action::new("layer.merge_selected.plan", json!({"layer_ids":[3,1]}));
        let start = std::time::Instant::now();
        let result = s.apply(action).unwrap();
        plan_samples.push(start.elapsed());
        assert_eq!(result["result"]["layer_id"], 3);
    }
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    let action = Action::new("layer.merge_selected", json!({"layer_ids":[3,1]}));
    let start = std::time::Instant::now();
    let result = s.apply(action).unwrap();
    let merge_elapsed = start.elapsed();

    assert_eq!(result["result"]["layer_id"], 3);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(ids(&s), vec![2, 3, 4]);
    let baked = &s.doc.layers[s.doc.layer_index(3).unwrap()].pixels;
    assert_eq!(baked.len(), expected.len());
    if let Some(index) = baked.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "selected_merge_cost pixel {index}: {:08x} != {:08x}",
            baked[index], expected[index]
        );
    }
    for (id, pointer, metadata) in survivors {
        let index = s.doc.layer_index(id).unwrap();
        assert_eq!(s.doc.layers[index].pixels.as_ptr(), pointer);
        assert_eq!(attributes(&s, id), metadata);
        assert!(
            s.doc.layers[index].pixels == originals[(id - 1) as usize],
            "survivor {id} raw pixels changed"
        );
    }
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(ids(&s), vec![1, 2, 3, 4]);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layer_id(s.doc.active), 1);
    assert_eq!(s.doc.selected_layer_ids(), vec![1, 3]);
    assert!(
        s.doc
            .layers
            .iter()
            .zip(&originals)
            .all(|(layer, source)| layer.pixels == *source),
        "one Undo must restore every raw source"
    );
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    let sample_ms: Vec<_> = plan_samples
        .iter()
        .map(|t| t.as_secs_f64() * 1000.)
        .collect();
    plan_samples.sort();
    println!("selected_merge_cost 1024x1024, four ARGB rasters A/U/B/V, targets[1,3], opaque A + full-range ARGB U/B/V, Normal/full opacity; plan_median_ms={:.3}, plan_samples_ms={sample_ms:?}, merge_apply_ms={:.3}. Timed Session::apply includes validation, staged native baking, final composition, structural history, replay append/encoding and result JSON; plan queries include metadata validation/compilation and result JSON. Excludes action construction, source/reference preparation, replay_boundary, verification/Undo, PNG encoding, transports, GUI and physical pen input. No CI threshold or peak-memory claim.",plan_samples[2].as_secs_f64()*1000.,merge_elapsed.as_secs_f64()*1000.);
}
