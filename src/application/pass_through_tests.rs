//! Pass Through uses the incoming backdrop and one premultiplied group gate.
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

fn source(s: &mut Session, colors: &[u32]) {
    call(s, "canvas.new", json!({"width":2,"height":1}));
    for (i, color) in colors.iter().enumerate() {
        if i != 0 {
            call(s, "layer.add", json!({}));
        }
        s.doc.replace_region(0, 0, 2, 1, &[*color; 2]);
    }
    s.replay_boundary().unwrap();
}
fn create(s: &mut Session, members: Value, name: &str) -> u64 {
    call(s, "group.create", json!({"members":members,"name":name}))["group_id"]
        .as_u64()
        .unwrap()
}
fn layer(id: u64) -> Value {
    json!({"kind":"layer","id":id})
}
fn group(id: u64) -> Value {
    json!({"kind":"group","id":id})
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none(), "{:?}", s.doc.render_error);
    s.doc.composite.clone()
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn row(s: &mut Session, id: u64) -> Value {
    call(s, "group.query", json!({}))["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == id)
        .unwrap()
        .clone()
}
fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/pass-through-units");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn mix(before: u32, after: u32, amount: u32) -> u32 {
    // Independent integer form of linear premultiplied interpolation. RGB is
    // divided by the unrounded weighted alpha; alpha is separately rounded.
    let (ab, aa) = ((before >> 24) as u64, (after >> 24) as u64);
    let (left, right) = (255 - amount as u64, amount as u64);
    let weight = ab * left + aa * right;
    let alpha = (weight + 127) / 255;
    if alpha == 0 {
        return 0;
    }
    let mut out = (alpha as u32) << 24;
    for shift in [16, 8, 0] {
        let numerator = ((before >> shift) & 255) as u64 * ab * left
            + ((after >> shift) & 255) as u64 * aa * right;
        let channel = (2 * numerator + weight) / (2 * weight);
        out |= (channel as u32) << shift;
    }
    out
}
fn mix_graph(amount: f32, enabled: bool, connected: bool) -> Graph {
    serde_json::from_value(json!({"enabled":enabled,"nodes":[
        {"id":0,"position":[0,0],"kind":"layer","index":0},
        {"id":1,"position":[0,200],"kind":"layer","index":1},
        {"id":2,"position":[300,0],"kind":"mix","opacity":amount},
        {"id":3,"position":[600,0],"kind":"output"}
    ],"edges":[{"from":0,"to":2,"input":0},{"from":1,"to":2,"input":1},
        {"from":if connected {2}else{0},"to":3,"input":0}]}))
    .unwrap()
}
fn invert(s: &mut Session, id: u64) {
    let index = s.doc.layer_index(id).unwrap();
    let mut graph = s.doc.graph.clone();
    let input = graph
        .nodes
        .iter()
        .find(|n| n.data == (NodeKind::Layer { index }))
        .unwrap()
        .id;
    let next = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
    for e in &mut graph.edges {
        if e.from == input {
            e.from = next;
        }
    }
    graph.edges.push(Edge {
        from: input,
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
}

#[test]
fn pass_through_at_full_opacity_matches_ungrouped_children_for_all_26_modes() {
    let mut s = session();
    for mode in BlendMode::ALL {
        source(&mut s, &[0x7f804020, 0xa04080c0, 0x60cc2244]);
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":2,"blend_mode":mode,"opacity":173./255.}),
        );
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":3,"blend_mode":"screen","opacity":211./255.}),
        );
        let expected = pixels(&mut s);
        let original = raw(&s);
        let id = create(&mut s, json!([layer(2), layer(3)]), "Pass ink 彩");
        call(
            &mut s,
            "group.set",
            json!({"group_id":id,"compositing":"pass_through","blend_mode":"color_burn"}),
        );
        assert_eq!(pixels(&mut s), expected, "{mode:?}");
        assert_eq!(raw(&s), original);
        let info = row(&mut s, id);
        assert_eq!(info["compositing"], "pass_through");
        assert_eq!(info["blend_mode"], "color_burn");
        assert_eq!(info["mergeable"], false);
        assert!(info["merge_error"].is_string());
        call(
            &mut s,
            "group.set",
            json!({"group_id":id,"blend_mode":"luminosity"}),
        );
        assert_eq!(
            pixels(&mut s),
            expected,
            "retained mode is ignored {mode:?}"
        );
        check(&mut s);
    }
}

#[test]
fn pass_through_nested_policies_partial_alpha_and_gate_endpoints_match_independent_mix() {
    let mut s = session();
    for background in [0x00804020, 0x7f804020, 0xff804020] {
        source(&mut s, &[background, 0x904080c0, 0x60cc2244]);
        let full = pixels(&mut s)[0];
        call(&mut s, "layer.set", json!({"layer_id":3,"visible":false}));
        let before_inner = pixels(&mut s)[0];
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":3,"visible":true,"opacity":89./255.,"blend_mode":"multiply"}),
        );
        let isolated_control = pixels(&mut s)[0];
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":3,"opacity":1.,"blend_mode":"normal"}),
        );
        let inner = create(&mut s, json!([layer(3)]), "Inner");
        call(
            &mut s,
            "group.set",
            json!({"group_id":inner,"opacity":89./255.,"blend_mode":"multiply"}),
        );
        let outer = create(&mut s, json!([layer(2), group(inner)]), "Outer");
        call(
            &mut s,
            "group.set",
            json!({"group_id":outer,"compositing":"pass_through","opacity":137./255.}),
        );
        let background = if background >> 24 == 0 { 0 } else { background };
        assert_eq!(pixels(&mut s), [mix(background, isolated_control, 137); 2]);
        call(
            &mut s,
            "group.set",
            json!({"group_id":inner,"compositing":"pass_through"}),
        );
        let inside = mix(before_inner, full, 89);
        assert_eq!(pixels(&mut s), [mix(background, inside, 137); 2]);
        for amount in [0u32, 1, 127, 128, 254, 255] {
            call(
                &mut s,
                "group.set",
                json!({"group_id":outer,"opacity":amount as f64/255.}),
            );
            assert_eq!(
                pixels(&mut s),
                [mix(background, inside, amount); 2],
                "amount {amount}"
            );
        }
        let expected = pixels(&mut s);
        let empty = create(&mut s, json!([]), "Empty pass");
        call(
            &mut s,
            "group.set",
            json!({"group_id":empty,"compositing":"pass_through","opacity":0.5}),
        );
        assert_eq!(pixels(&mut s), expected);
        call(
            &mut s,
            "group.set",
            json!({"group_id":inner,"visible":false}),
        );
        assert_eq!(pixels(&mut s), [before_inner; 2]);
        check(&mut s);
    }
    for (before, after) in [
        (0x00abcdef, 0x00010203),
        (0x010000ff, 0x00ffffff),
        (0x7f804020, 0xc84080c0),
    ] {
        source(&mut s, &[before, after]);
        for amount in [0u32, 1, 127, 128, 254, 255] {
            let graph = mix_graph(amount as f32 / 255., true, true);
            assert_eq!(NodeKind::Mix { opacity: 0.5 }.inputs(), 2);
            call(&mut s, "graph.set", json!({"graph":graph}));
            let canonical = |p: u32| if p >> 24 == 0 { 0 } else { p };
            assert_eq!(
                pixels(&mut s),
                [mix(canonical(before), canonical(after), amount); 2]
            );
        }
    }
}

#[test]
fn pass_through_retains_filters_hidden_empty_groups_and_readonly_queries_during_contact() {
    let mut s = session();
    source(&mut s, &[0x7f804020, 0xa04080c0]);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"blend_mode":"multiply"}),
    );
    let plain = pixels(&mut s)[0];
    invert(&mut s, 2);
    let filtered = pixels(&mut s)[0];
    assert_ne!(plain, filtered);
    let id = create(&mut s, json!([layer(2)]), "Filtered pass");
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"pass_through","opacity":137./255.}),
    );
    assert_eq!(pixels(&mut s), [mix(0x7f804020, filtered, 137); 2]);
    let mut disabled = s.doc.graph.clone();
    disabled.enabled = false;
    call(&mut s, "graph.set", json!({"graph":disabled}));
    assert_eq!(pixels(&mut s), [mix(0x7f804020, plain, 137); 2]);
    call(&mut s, "group.set", json!({"group_id":id,"visible":false}));
    assert_eq!(pixels(&mut s), [0x7f804020; 2]);
    call(&mut s, "layer.select", json!({"layer_id":1}));
    for tool in ["hand", "brush"] {
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"down","x":0.5,"y":0.5,"pressure":0.2,"pen":true,"time":1,"tool_override":tool}),
        );
        let before = s.state();
        let history = s.doc.history_bytes();
        let count = s.journal.as_ref().unwrap().entries.len();
        let revision = s.revision;
        assert_eq!(row(&mut s, id)["compositing"], "pass_through");
        assert_eq!(s.revision, revision);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
        assert_eq!(
            s.apply(Action::new(
                "group.set",
                json!({"group_id":id,"compositing":"isolated"})
            ))
            .unwrap_err()
            .code,
            "stroke_active"
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(s.stroke.active());
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"move","x":1,"y":0.5,"pressure":1,"pen":true,"time":20,"tool_override":tool}),
        );
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"up","x":1.5,"y":0.5,"pressure":0.5,"pen":true,"time":40,"tool_override":tool}),
        );
        assert!(!s.stroke.active());
    }
    check(&mut s);
}

#[test]
fn isolated_defaults_keep_v1_bytes_while_policy_undo_and_explicit_revert_preserve_version_rules() {
    let mut s = session();
    source(&mut s, &[0xff804020, 0x804080c0]);
    let id = create(&mut s, json!([layer(2)]), "Ink");
    let legacy = s.doc.composition();
    let encoded = serde_json::to_string(legacy.groups.as_ref().unwrap()).unwrap();
    assert_eq!(
        encoded,
        r#"{"version":1,"roots":[{"kind":"layer","id":1},{"kind":"group","id":1}],"groups":[{"id":1,"name":"Ink","visible":true,"opacity":255,"blend_mode":"normal","children":[{"kind":"layer","id":2}]}]}"#
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"isolated"}),
    );
    assert_eq!(s.doc.composition(), legacy);
    assert_eq!(row(&mut s, id)["compositing"], "isolated");
    assert_eq!(row(&mut s, id)["mergeable"], true);
    assert!(row(&mut s, id)["merge_error"].is_null());
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"pass_through"}),
    );
    let pass = s.doc.composition();
    assert_eq!(pass.groups.as_ref().unwrap().version, 2);
    assert!(serde_json::to_string(pass.groups.as_ref().unwrap())
        .unwrap()
        .contains("\"compositing\":\"pass_through\""));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), legacy);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), pass);
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"isolated"}),
    );
    let reverted = s.doc.composition();
    assert_eq!(reverted.groups.as_ref().unwrap().version, 2);
    assert!(!serde_json::to_string(reverted.groups.as_ref().unwrap())
        .unwrap()
        .contains("compositing"));
    let mut bad = s.replay_metadata();
    bad.composition = pass.clone();
    bad.composition.groups.as_mut().unwrap().version = 1;
    assert!(bad.validate().is_err());
    bad.composition.groups.as_mut().unwrap().version = 3;
    assert!(bad.validate().is_err());
    let before = s.doc.composition();
    let source = raw(&s);
    for invalid in [json!(null), json!("unknown"), json!(true)] {
        assert!(s
            .apply(Action::new(
                "pipeline.run",
                json!({"atomic":false,"steps":[
                    {"id":"rename","action":"layer.rename","args":{"name":"Must not execute"}},
                    {"id":"bad","action":"group.set","args":{"group_id":id,"compositing":invalid}}
                ]})
            ))
            .is_err());
        assert_eq!(s.doc.composition(), before);
        assert_eq!(raw(&s), source);
    }
    assert_eq!(
        Journal::import(include_bytes!("../../tests/fixtures/replay/v1.mcreplay"))
            .unwrap()
            .verify(None)
            .unwrap()["valid"],
        true
    );
    check(&mut s);
}

struct NoMix {
    delegate: Arc<dyn Renderer>,
    renders: Arc<AtomicUsize>,
}
impl Renderer for NoMix {
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
        self.renders.fetch_add(1, Ordering::Relaxed);
        self.delegate.render(n, l, w, h, o)
    }
}
#[test]
fn old_compositor_preflight_rejects_hidden_policy_and_disabled_or_disconnected_mix_before_render() {
    let mut s = session();
    source(&mut s, &[0xff804020, 0x804080c0]);
    let id = create(&mut s, json!([layer(2)]), "Hidden");
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"pass_through"}),
    );
    let path = output("pass.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"isolated","visible":false}),
    );
    let delegate = s.doc.renderer.clone().unwrap();
    let renders = Arc::new(AtomicUsize::new(0));
    s.doc.renderer = Some(Arc::new(NoMix {
        delegate: delegate.clone(),
        renders: renders.clone(),
    }));
    let before = s.state();
    let composition = s.doc.composition();
    let original = raw(&s);
    let history = s.doc.history_bytes();
    assert!(s
        .apply(Action::new(
            "group.set",
            json!({"group_id":id,"compositing":"pass_through","visible":false})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    let mut candidate = codec::open(&path).unwrap();
    candidate.renderer = s.doc.renderer.clone();
    assert!(candidate.ensure_graph_renderer().is_err());
    assert_eq!(renders.load(Ordering::Relaxed), 0);
    s.doc.renderer = Some(delegate);
    std::fs::remove_file(path).unwrap();
    source(&mut s, &[0xff804020, 0x804080c0]);
    let delegate = s.doc.renderer.clone().unwrap();
    let renders = Arc::new(AtomicUsize::new(0));
    s.doc.renderer = Some(Arc::new(NoMix {
        delegate: delegate.clone(),
        renders: renders.clone(),
    }));
    let before = s.state();
    let composition = s.doc.composition();
    let original = raw(&s);
    for (enabled, connected) in [(true, true), (false, true), (true, false), (false, false)] {
        let graph = mix_graph(0.5, enabled, connected);
        assert!(s
            .apply(Action::new("graph.set", json!({"graph":graph})))
            .is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), original);
    }
    assert_eq!(renders.load(Ordering::Relaxed), 0);
    s.doc.renderer = Some(delegate);
    check(&mut s);
}

#[test]
fn pass_target_rejects_group_merge_but_isolated_ancestors_and_full_output_bakes_preserve_pixels() {
    let mut s = session();
    source(&mut s, &[0x7f804020, 0x904080c0, 0x60cc2244]);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":3,"blend_mode":"multiply"}),
    );
    let inner = create(&mut s, json!([layer(2), layer(3)]), "Pass target");
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner,"compositing":"pass_through","opacity":137./255.}),
    );
    let before = s.doc.composition();
    let original = raw(&s);
    let expected = pixels(&mut s);
    let history = s.doc.history_bytes();
    assert_eq!(row(&mut s, inner)["mergeable"], false);
    assert!(s
        .apply(Action::new("group.merge", json!({"group_id":inner})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(pixels(&mut s), expected);
    for action in [
        "layer.stamp_visible",
        "layer.merge_visible",
        "layer.flatten",
    ] {
        let before = s.doc.composition();
        let original = raw(&s);
        let expected = pixels(&mut s);
        call(&mut s, action, json!({}));
        if action == "layer.stamp_visible" {
            assert_eq!(s.doc.layers[s.doc.active].pixels, expected);
        } else {
            assert_eq!(pixels(&mut s), expected, "{action}");
        }
        call(&mut s, "history.undo", json!({}));
        let mut restored = s.doc.composition();
        restored.next_layer_id = before.next_layer_id;
        assert_eq!(restored, before);
        assert_eq!(raw(&s), original);
        assert_eq!(pixels(&mut s), expected);
    }
    let outer = create(&mut s, json!([group(inner)]), "Isolated ancestor");
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"opacity":179./255.,"blend_mode":"screen"}),
    );
    assert_eq!(row(&mut s, outer)["mergeable"], true);
    let expected = pixels(&mut s);
    call(&mut s, "group.merge", json!({"group_id":outer}));
    assert_eq!(pixels(&mut s), expected);
    check(&mut s);
}

struct FailingMix {
    delegate: Arc<dyn Renderer>,
    armed: Arc<AtomicBool>,
}
impl Renderer for FailingMix {
    fn supports_mix(&self) -> bool {
        true
    }
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
            return Err("Injected partial Mix output failure".into());
        }
        self.delegate.render(n, l, w, h, o)
    }
}
#[test]
fn pass_policy_render_atomic_and_partial_append_failures_restore_version_cache_history_and_sources()
{
    let mut s = session();
    source(&mut s, &[0x7f804020, 0x904080c0]);
    let id = create(&mut s, json!([layer(2)]), "Retry");
    let armed = Arc::new(AtomicBool::new(false));
    s.doc.renderer = Some(Arc::new(FailingMix {
        delegate: s.doc.renderer.clone().unwrap(),
        armed: armed.clone(),
    }));
    let path = output("clean.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let before = s.state();
    let composition = s.doc.composition();
    let original = raw(&s);
    let committed = pixels(&mut s);
    let effective = s.doc.effective_graph().unwrap();
    let history = s.doc.history_bytes();
    armed.store(true, Ordering::Relaxed);
    assert!(s
        .apply(Action::new(
            "group.set",
            json!({"group_id":id,"compositing":"pass_through","opacity":137./255.})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.composite, committed);
    assert_eq!(s.doc.effective_graph().unwrap(), effective);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(!s.doc.dirty);
    assert!(s.replay_error.is_none());
    armed.store(false, Ordering::Relaxed);
    call(
        &mut s,
        "group.set",
        json!({"group_id":id,"compositing":"pass_through","opacity":137./255.}),
    );
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    let before = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"pass","action":"group.set","args":{"group_id":id,"compositing":"pass_through"}},
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
            "group.set",
            json!({"group_id":id,"compositing":"pass_through"})
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
fn pass_schemas_workflow_projects_and_outcome_replay_preserve_context_without_reexecuting_plugins()
{
    let mut s = session();
    source(&mut s, &[0x7f804020, 0x904080c0]);
    let id = create(&mut s, json!([layer(2)]), "Workflow pass");
    assert_eq!(
        s.registry.actions["group.set"].schema()["properties"]["compositing"]["enum"],
        json!(["isolated", "pass_through"])
    );
    assert_eq!(call(&mut s, "graph.read", json!({}))["renderer_mix"], true);
    assert_eq!(
        call(&mut s, "graph.catalog", json!({}))["renderer_mix"],
        true
    );
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(catalog.contains("compositing.mix") && catalog.contains("pass_through"));
    let mut expected = s.doc.composition();
    let original = raw(&s);
    let png = codec::png(&mut s.doc).unwrap();
    let draft = json!({"version":1,"nodes":[{"id":1,"type":"action.group.set","position":[0,0],"parameters":{"group_id":id,"compositing":"pass_through","opacity":137./255.}}],"edges":[]});
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
    let composition = s.doc.composition();
    let final_png = codec::png(&mut s.doc).unwrap();
    assert_eq!(composition.groups.as_ref().unwrap().version, 2);
    let path = output("pass.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composition(), composition);
    assert_eq!(codec::png(&mut opened.doc).unwrap(), final_png);
    check(&mut opened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    let replayed = journal.preview(last).unwrap();
    assert_eq!(replayed.metadata.composition, composition);
    assert_eq!(replay::png(&replayed).unwrap(), final_png);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn group_inspection_reports_headless_unavailability_and_queues_bound_desktop_focus_without_finishing_contact(
) {
    let mut s = session();
    source(&mut s, &[0x7f804020, 0x904080c0]);
    let id = create(&mut s, json!([layer(2)]), "Inspect");
    let before = s.state();
    let composition = s.doc.composition();
    let original = raw(&s);
    let history = s.doc.history_bytes();
    assert_eq!(
        s.apply(Action::new("group.inspect", json!({"group_id":id as f64})))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    assert_eq!(s.state(), before);
    assert!(s.ui_effects.is_empty());
    s.runtime = "desktop";
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":0.5,"y":0.5,"pen":true,"tool_override":"hand"}),
    );
    let identity = s.doc.identity;
    call(&mut s, "group.inspect", json!({"group_id":id as f64}));
    assert!(s.stroke.active());
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.ui["group.inspect"], id);
    let effect = s.ui_effects.back().unwrap();
    assert_eq!(effect["id"], "group.inspect");
    assert_eq!(effect["value"], id);
    assert_eq!(effect["document_identity"], identity);
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":0.5,"y":0.5,"pen":true,"tool_override":"hand"}),
    );
    check(&mut s);
}
