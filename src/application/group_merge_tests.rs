//! Group consolidation bakes the intended scope while preserving hidden sources and history.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    graph::{BlendMode, Edge, Filter, Node, NodeKind, Renderer},
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

fn reset(s: &mut Session, colors: &[u32]) {
    call(s, "canvas.new", json!({"width":2,"height":1}));
    for (index, color) in colors.iter().enumerate() {
        if index != 0 {
            call(s, "layer.add", json!({}));
        }
        s.doc.replace_region(0, 0, 2, 1, &[*color; 2]);
        call(
            s,
            "layer.rename",
            json!({"name":format!("Cel {} 墨",index+1)}),
        );
    }
    s.replay_boundary().unwrap();
}
fn fresh(colors: &[u32]) -> Session {
    let mut s = session();
    reset(&mut s, colors);
    s
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
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn pixels(s: &mut Session) -> Vec<u32> {
    s.doc.compose();
    assert!(s.doc.render_error.is_none(), "{:?}", s.doc.render_error);
    s.doc.composite.clone()
}
fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/group-merge-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
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
fn attenuate(pixel: u32, opacity: u32) -> u32 {
    let alpha = (pixel >> 24) * opacity / 255;
    if alpha == 0 {
        0
    } else {
        (pixel & 0xffffff) | (alpha << 24)
    }
}
// Independent normalized straight-alpha equation with the established 8-bit
// blend/alpha stages and historical 127/255 final-channel truncation bias.
fn reference(dst: u32, src: u32, mode: BlendMode, opacity: u32) -> u32 {
    let dst = if dst >> 24 == 0 { 0 } else { dst };
    let src = attenuate(src, opacity);
    let sa = (src >> 24) as f64 / 255.;
    let da = (dst >> 24) as f64 / 255.;
    if sa == 0. {
        return dst;
    }
    let quantize = |v: f64| (v + 1e-9).round().clamp(0., 255.) as u32;
    let alpha = quantize((sa + da * (1. - sa)) * 255.);
    let mut result = alpha << 24;
    for shift in [16, 8, 0] {
        let s = ((src >> shift) & 255) as f64;
        let d = ((dst >> shift) & 255) as f64;
        let channel = match mode {
            BlendMode::Normal => s,
            BlendMode::Multiply => s * d / 255.,
            BlendMode::Screen => 255. - (255. - s) * (255. - d) / 255.,
            BlendMode::Overlay if d < 128. => 2. * s * d / 255.,
            BlendMode::Overlay => 255. - 2. * (255. - s) * (255. - d) / 255.,
            BlendMode::Darken => s.min(d),
            BlendMode::Lighten => s.max(d),
            BlendMode::Add => (s + d).min(255.),
            BlendMode::Difference => (s - d).abs(),
        };
        let premultiplied =
            (1. - sa) * da * d + (1. - da) * sa * s + sa * da * quantize(channel) as f64;
        let color = (premultiplied / (alpha as f64 / 255.) + 127. / 255. + 1e-9)
            .floor()
            .clamp(0., 255.) as u32;
        result |= color << shift;
    }
    result
}

#[test]
fn group_merge_bakes_only_internal_children_for_all_gate_modes_alpha_and_opacity_edges() {
    let mut s = session();
    for mode in BlendMode::ALL {
        for backdrop_alpha in [0, 128, 255] {
            for opacity in [0, 128, 255] {
                let backdrop = (backdrop_alpha << 24) | 0x804020;
                reset(&mut s, &[backdrop, 0x80ff0000, 0xc80000ff]);
                call(
                    &mut s,
                    "layer.set",
                    json!({"layer_id":2,"opacity":153./255.}),
                );
                call(
                    &mut s,
                    "layer.set",
                    json!({"layer_id":3,"opacity":179./255.}),
                );
                let inner = create(&mut s, json!([layer(3)]), "Nested");
                call(
                    &mut s,
                    "group.set",
                    json!({"group_id":inner,"opacity":128./255.,"blend_mode":"multiply"}),
                );
                let outer = create(&mut s, json!([layer(2), group(inner)]), "Merged cel 彩");
                call(
                    &mut s,
                    "group.set",
                    json!({"group_id":outer,"opacity":opacity as f64/255.,"blend_mode":mode}),
                );
                let internal = reference(
                    attenuate(0x80ff0000, 153),
                    attenuate(0xc80000ff, 179),
                    BlendMode::Multiply,
                    128,
                );
                let backdrop_sample = reference(0, backdrop, BlendMode::Normal, 255);
                let expected = reference(backdrop_sample, internal, mode, opacity);
                assert_eq!(
                    pixels(&mut s),
                    [expected; 2],
                    "before {mode:?} {backdrop_alpha} {opacity}"
                );
                let revision = s.revision;
                let count = s.journal.as_ref().unwrap().entries.len();
                let result = call(&mut s, "group.merge", json!({"group_id":outer}));
                let id = s.doc.layer_id(s.doc.active);
                assert_eq!(result["layer_id"], id);
                assert_eq!(result["group_id"], outer);
                assert!(id > 3);
                assert_eq!(s.doc.layers.len(), 2);
                assert_eq!(s.doc.layers[s.doc.active].pixels, [internal; 2]);
                assert_eq!(
                    pixels(&mut s),
                    [expected; 2],
                    "after {mode:?} {backdrop_alpha} {opacity}"
                );
                let row = s.state()["layers"][s.doc.active].clone();
                assert_eq!(row["name"], "Merged cel 彩");
                assert_eq!(row["blend_mode"], json!(mode));
                assert_eq!(s.doc.layers[s.doc.active].opacity, opacity as u8);
                assert_eq!(row["visible"], true);
                assert_eq!(row["parent_id"], 0);
                assert_eq!(s.state()["selected_layer_ids"], json!([id]));
                assert_eq!(s.revision, revision + 1);
                assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
            }
        }
    }
    check(&mut s);
}

#[test]
fn group_merge_preserves_slot_gate_hidden_outside_buffers_and_single_undo_redo() {
    let mut s = fresh(&[0xff804020, 0xffff0000, 0xff0000ff, 0xffffff00]);
    let baked_filter = invert(&mut s, 2);
    let hidden_filter = invert(&mut s, 4);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":4,"visible":false,"pixel_locked":true,"alpha_locked":true,"color_label":"red"}),
    );
    let nested = create(&mut s, json!([layer(3)]), "Hidden child");
    call(
        &mut s,
        "group.set",
        json!({"group_id":nested,"visible":false}),
    );
    let g = create(&mut s, json!([layer(2), group(nested)]), "Hidden gate 墨");
    call(
        &mut s,
        "group.set",
        json!({"group_id":g,"visible":false,"opacity":0.5,"blend_mode":"screen"}),
    );
    let before = s.doc.composition();
    let source = raw(&s);
    let expected = pixels(&mut s);
    let hidden = s.doc.layer_index(4).unwrap();
    let pointer = s.doc.layers[hidden].pixels.as_ptr();
    let parent = s.doc.layer_parent_id(s.doc.layer_index(2).unwrap());
    assert_eq!(parent, g);
    let result = call(&mut s, "group.merge", json!({"group_id":g}));
    let id = result["layer_id"].as_u64().unwrap();
    assert_eq!(
        s.doc.layers[s.doc.layer_index(id).unwrap()].pixels,
        [0xff00ffff; 2]
    );
    let row = s.state()["layers"][s.doc.active].clone();
    assert_eq!(row["visible"], false);
    assert_eq!(row["name"], "Hidden gate 墨");
    assert_eq!(row["blend_mode"], "screen");
    assert_eq!(s.doc.layers[s.doc.active].opacity, 128);
    assert_eq!(
        s.doc.layers[s.doc.layer_index(4).unwrap()].pixels,
        source[3]
    );
    assert_eq!(
        s.doc.layers[s.doc.layer_index(4).unwrap()].pixels.as_ptr(),
        pointer
    );
    assert!(s.doc.graph.nodes.iter().any(|n| n.id == hidden_filter));
    assert!(!s.doc.graph.nodes.iter().any(|n| n.id == baked_filter));
    assert_eq!(pixels(&mut s), expected);
    assert!(!s.doc.has_groups());
    call(&mut s, "history.undo", json!({}));
    let mut restored = s.doc.composition();
    restored.next_layer_id = before.next_layer_id;
    assert_eq!(restored, before);
    assert_eq!(raw(&s), source);
    assert_eq!(pixels(&mut s), expected);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layer_id(s.doc.active), id);
    assert_eq!(pixels(&mut s), expected);
    call(&mut s, "layer.set", json!({"layer_id":id,"visible":true}));
    assert_eq!(
        pixels(&mut s),
        [reference(0xff804020, 0xff00ffff, BlendMode::Screen, 128); 2]
    );
    check(&mut s);
}

#[test]
fn grouped_merge_visible_bakes_actual_output_and_retains_hidden_skeletons_metadata_and_filters() {
    let mut s = fresh(&[0xffff0000, 0xffffff00, 0x800000ff, 0xff00ff00, 0x8000ff00]);
    let hidden_filter = invert(&mut s, 2);
    let local_filter = invert(&mut s, 4);
    let baked_filter = invert(&mut s, 3);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"pixel_locked":true,"alpha_locked":true,"color_label":"red"}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":4,"visible":false,"pixel_locked":true,"alpha_locked":true}),
    );
    call(
        &mut s,
        "layer.rename",
        json!({"layer_id":5,"name":"Top ink 墨"}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":5,"position_locked":true,"color_label":"violet","opacity":153./255.,"blend_mode":"overlay"}),
    );
    let backdrop = create(&mut s, json!([layer(1)]), "Backdrop");
    call(
        &mut s,
        "group.set",
        json!({"group_id":backdrop,"opacity":0.5}),
    );
    let hidden = create(&mut s, json!([layer(2)]), "Hidden ancestor");
    call(
        &mut s,
        "group.set",
        json!({"group_id":hidden,"visible":false}),
    );
    let visible = create(&mut s, json!([layer(3), layer(5)]), "Visible");
    call(
        &mut s,
        "group.set",
        json!({"group_id":visible,"opacity":179./255.,"blend_mode":"screen"}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,5],"active_layer_id":1}),
    );
    let expected = reference(
        attenuate(0xffff0000, 128),
        reference(0x80ffff00, 0x8000ff00, BlendMode::Overlay, 153),
        BlendMode::Screen,
        179,
    );
    assert_eq!(pixels(&mut s), [expected; 2]);
    let before = s.doc.composition();
    let source = raw(&s);
    let original_ids: Vec<_> = (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect();
    let hidden_pointers: Vec<_> = [2, 4]
        .iter()
        .map(|id| {
            s.doc.layers[s.doc.layer_index(*id).unwrap()]
                .pixels
                .as_ptr()
        })
        .collect();
    call(&mut s, "layer.merge_visible", json!({}));
    assert_eq!(s.doc.layers.len(), 3);
    assert_eq!(s.doc.layer_id(s.doc.active), 5);
    assert_eq!(s.state()["selected_layer_ids"], json!([5]));
    assert_eq!(s.doc.layers[s.doc.active].pixels, [expected; 2]);
    assert_eq!(s.doc.layers[s.doc.active].opacity, 255);
    assert_eq!(s.doc.layers[s.doc.active].mode, BlendMode::Normal);
    let row = s.state()["layers"][s.doc.active].clone();
    assert_eq!(row["name"], "Top ink 墨");
    assert_eq!(row["color_label"], "violet");
    assert_eq!(row["position_locked"], true);
    assert_eq!(row["parent_id"], 0);
    for (at, id) in [2, 4].iter().enumerate() {
        let i = s.doc.layer_index(*id).unwrap();
        let old = original_ids.iter().position(|v| v == id).unwrap();
        assert_eq!(s.doc.layers[i].pixels, source[old]);
        assert_eq!(s.doc.layers[i].pixels.as_ptr(), hidden_pointers[at]);
    }
    assert!(s.doc.graph.nodes.iter().any(|n| n.id == hidden_filter));
    assert!(s.doc.graph.nodes.iter().any(|n| n.id == local_filter));
    assert!(!s.doc.graph.nodes.iter().any(|n| n.id == baked_filter));
    assert_eq!(s.doc.groups_state()["groups"].as_array().unwrap().len(), 3);
    assert!(s.doc.group_layer_ids(backdrop).unwrap().is_empty());
    assert!(s.doc.group_layer_ids(visible).unwrap().is_empty());
    assert_eq!(s.doc.group_layer_ids(hidden).unwrap(), [2]);
    assert_eq!(pixels(&mut s), [expected; 2]);
    call(
        &mut s,
        "group.set",
        json!({"group_id":hidden,"visible":true}),
    );
    assert_ne!(pixels(&mut s), [expected; 2]);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(raw(&s), source);
    assert_eq!(s.state()["selected_layer_ids"], json!([1, 5]));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(pixels(&mut s), [expected; 2]);
    check(&mut s);
}

#[test]
fn grouped_merge_visible_with_one_source_still_bakes_filters_and_group_gate() {
    let mut s = fresh(&[0xff0000ff, 0xff884422]);
    let baked = invert(&mut s, 1);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"visible":false,"pixel_locked":true,"alpha_locked":true}),
    );
    let g = create(&mut s, json!([layer(1)]), "Half");
    call(&mut s, "group.set", json!({"group_id":g,"opacity":0.5}));
    assert_eq!(pixels(&mut s), [0x80ffff00; 2]);
    let before = s.doc.composition();
    let source = raw(&s);
    call(&mut s, "layer.merge_visible", json!({}));
    let i = s.doc.layer_index(1).unwrap();
    assert_eq!(s.doc.layers[i].pixels, [0x80ffff00; 2]);
    assert_eq!(s.doc.layers[i].opacity, 255);
    assert_eq!(s.doc.layer_parent_id(i), 0);
    assert!(!s.doc.graph.nodes.iter().any(|n| n.id == baked));
    assert!(s.doc.group_layer_ids(g).unwrap().is_empty());
    assert_eq!(pixels(&mut s), [0x80ffff00; 2]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    assert_eq!(raw(&s), source);
    call(&mut s, "group.set", json!({"group_id":g,"visible":false}));
    let state = s.state();
    let history = s.doc.history_bytes();
    assert!(s
        .apply(Action::new("layer.merge_visible", json!({})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}

#[test]
fn group_merge_preflights_all_descendant_locks_capability_empty_groups_and_active_contacts() {
    let mut s = fresh(&[0xffff0000, 0xff0000ff]);
    let g = create(&mut s, json!([layer(1), layer(2)]), "Locked");
    for lock in ["pixel_locked", "alpha_locked"] {
        let mut properties = json!({"layer_id":1});
        properties[lock] = json!(true);
        call(&mut s, "layer.set", properties);
        let before = s.state();
        let composition = s.doc.composition();
        let source = raw(&s);
        let history = s.doc.history_bytes();
        assert!(s
            .apply(Action::new("group.merge", json!({"group_id":g})))
            .is_err());
        assert!(s
            .apply(Action::new("layer.merge_visible", json!({})))
            .is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), source);
        assert_eq!(s.doc.history_bytes(), history);
        let mut properties = json!({"layer_id":1});
        properties[lock] = json!(false);
        call(&mut s, "layer.set", properties);
    }
    let empty = create(&mut s, json!([]), "Empty");
    let before = s.state();
    assert!(s
        .apply(Action::new("group.merge", json!({"group_id":empty})))
        .is_err());
    assert_eq!(s.state(), before);
    for tool in ["brush", "hand"] {
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"down","x":0.5,"y":0.5,"pen":true,"pressure":0.2,"time":0,"tool_override":tool}),
        );
        assert!(s.stroke.active());
        if tool == "hand" {
            assert!(!s.is_drawing());
            assert!(s.is_panning());
        }
        let before = s.state();
        let source = raw(&s);
        let history = s.doc.history_bytes();
        for (id, args) in [
            ("group.merge", json!({"group_id":g})),
            ("layer.merge_visible", json!({})),
        ] {
            assert_eq!(
                s.apply(Action::new(id, args)).unwrap_err().code,
                "stroke_active"
            );
            assert_eq!(s.state(), before);
            assert_eq!(raw(&s), source);
            assert_eq!(s.doc.history_bytes(), history);
            assert!(s.stroke.active());
        }
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"move","x":1,"y":0.5,"pen":true,"pressure":1,"time":20,"tool_override":tool}),
        );
        call(
            &mut s,
            "canvas.pointer",
            json!({"phase":"up","x":1.5,"y":0.5,"pen":true,"pressure":0.4,"time":40,"tool_override":tool}),
        );
        assert!(!s.stroke.active());
    }
    let renderer = s.doc.renderer.clone().unwrap();
    s.doc.renderer = Some(Arc::new(LegacyRenderer(renderer.clone())));
    let before = s.state();
    assert!(s
        .apply(Action::new("group.merge", json!({"group_id":g})))
        .is_err());
    assert!(s
        .apply(Action::new("layer.merge_visible", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    s.doc.renderer = Some(renderer);
    check(&mut s);
}

struct LegacyRenderer(Arc<dyn Renderer>);
impl Renderer for LegacyRenderer {
    fn render(
        &self,
        n: &[micopi_plugin_api::CompositeNode],
        l: &[micopi_plugin_api::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        self.0.render(n, l, w, h, o)
    }
}
struct FailingRenderer {
    delegate: Arc<dyn Renderer>,
    armed: Arc<AtomicBool>,
}
impl Renderer for FailingRenderer {
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
            return Err("Injected group merge render failure".into());
        }
        self.delegate.render(n, l, w, h, o)
    }
}
#[test]
fn group_merge_render_atomic_and_storage_failures_restore_committed_sources_history_and_ids() {
    let mut s = fresh(&[0xff804020, 0xffff0000, 0xff0000ff]);
    let g = create(&mut s, json!([layer(2), layer(3)]), "Retry");
    call(&mut s, "group.set", json!({"group_id":g,"opacity":0.5}));
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,3],"active_layer_id":1}),
    );
    let armed = Arc::new(AtomicBool::new(false));
    s.doc.renderer = Some(Arc::new(FailingRenderer {
        delegate: s.doc.renderer.clone().unwrap(),
        armed: armed.clone(),
    }));
    let path = output("failure.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let committed = pixels(&mut s);
    let history = s.doc.history_bytes();
    armed.store(true, Ordering::Relaxed);
    for (id, args) in [
        ("group.merge", json!({"group_id":g})),
        ("layer.merge_visible", json!({})),
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
    let result = call(&mut s, "group.merge", json!({"group_id":g}));
    assert_eq!(result["layer_id"], composition.next_layer_id);
    assert_eq!(pixels(&mut s), committed);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let composition = s.doc.composition();
    let source = raw(&s);
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"merge","action":"group.merge","args":{"group_id":g}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
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
        .apply(Action::new("group.merge", json!({"group_id":g})))
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
fn group_merge_workflow_editing_is_inert_and_project_outcomes_replay_without_resampling_sources() {
    let mut s = fresh(&[0xffff0000, 0xff0000ff]);
    let g = create(&mut s, json!([layer(1), layer(2)]), "Cel 彩");
    call(&mut s, "group.set", json!({"group_id":g,"opacity":0.5}));
    let empty = create(&mut s, json!([]), "Preserved empty group");
    assert!(s.registry.actions["group.merge"].is_document_atomic());
    assert_eq!(
        s.registry.actions["group.merge"].schema()["properties"]["group_id"]["type"],
        "integer"
    );
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.group.merge"));
    let before = raw(&s);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.group.merge","position":[0,0],"parameters":{"group_id":g}},
        {"id":2,"type":"action.layer.rename","position":[400,0],"parameters":{"name":"Baked via workflow 彩"}},
        {"id":3,"type":"action.layer.merge_visible","position":[800,0],"parameters":{}}
    ],"edges":[{"from":1,"output":"layer_id","to":2,"input":"layer_id"},{"from":2,"output":"$exec","to":3,"input":"$exec"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(raw(&s), before);
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(raw(&s), before);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, [0x800000ff; 2]);
    assert_eq!(s.doc.layers[0].opacity, 255);
    assert_eq!(s.doc.layer_name(0), "Baked via workflow 彩");
    assert_eq!(s.doc.layer_parent_id(0), 0);
    assert!(s.doc.group_layer_ids(empty).unwrap().is_empty());
    let path = output("merged.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composition(), composition);
    assert_eq!(codec::png(&mut opened.doc).unwrap(), png);
    check(&mut opened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    assert_eq!(
        journal.preview(last).unwrap().metadata.composition,
        composition
    );
    assert_eq!(replay::png(&journal.preview(last).unwrap()).unwrap(), png);
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "Release cold-command measurement; no timing threshold"]
fn group_bake_cost() {
    fn prepare(s: &mut Session) -> (u64, Vec<u32>, Vec<u32>, Vec<Vec<u32>>) {
        call(s, "canvas.new", json!({"width":1024,"height":1024}));
        for layer in 0..4 {
            if layer != 0 {
                call(s, "layer.add", json!({}));
            }
            let alpha = [128u32, 255, 192, 64][layer];
            let words: Vec<_> = (0..1024 * 1024)
                .map(|index| {
                    let x = index % 1024;
                    let y = index / 1024;
                    (alpha << 24)
                        | ((((x * 17 + y * 3 + layer * 41) & 255) as u32) << 16)
                        | ((((x * 5 + y * 11 + layer * 23) & 255) as u32) << 8)
                        | (((x * 7 + y * 13 + layer * 59) & 255) as u32)
                })
                .collect();
            s.doc.replace_region(0, 0, 1024, 1024, &words);
        }
        call(
            s,
            "layer.set",
            json!({"layer_id":3,"blend_mode":"multiply"}),
        );
        call(s, "layer.set", json!({"layer_id":4,"blend_mode":"overlay"}));
        let nested = create(s, json!([layer(2), layer(3)]), "Nested workload");
        call(
            s,
            "group.set",
            json!({"group_id":nested,"opacity":191./255.}),
        );
        let outer = create(s, json!([group(nested), layer(4)]), "Bake workload");
        call(
            s,
            "group.set",
            json!({"group_id":outer,"opacity":153./255.,"blend_mode":"screen"}),
        );
        let originals = raw(s);
        let internal: Vec<_> = (0..1024 * 1024)
            .map(|i| {
                let nested = reference(originals[1][i], originals[2][i], BlendMode::Multiply, 255);
                reference(
                    attenuate(nested, 191),
                    originals[3][i],
                    BlendMode::Overlay,
                    255,
                )
            })
            .collect();
        let expected: Vec<_> = internal
            .iter()
            .enumerate()
            .map(|(i, p)| reference(originals[0][i], *p, BlendMode::Screen, 153))
            .collect();
        assert_eq!(pixels(s), expected);
        s.replay_boundary().unwrap();
        (outer, internal, expected, originals)
    }
    let mut s = session();
    for action in ["group.merge", "layer.merge_visible"] {
        let (group, internal, expected, originals) = prepare(&mut s);
        let command = Action::new(
            action,
            if action == "group.merge" {
                json!({"group_id":group})
            } else {
                json!({})
            },
        );
        let start = std::time::Instant::now();
        let result = s.apply(command).unwrap();
        let elapsed = start.elapsed();
        assert!(result.get("result").is_some());
        assert_eq!(pixels(&mut s), expected);
        if action == "group.merge" {
            assert_eq!(s.doc.layers.len(), 2);
            assert_eq!(s.doc.layers[0].pixels, originals[0]);
            assert_eq!(s.doc.layers[s.doc.active].pixels, internal);
        } else {
            assert_eq!(s.doc.layers.len(), 1);
            assert_eq!(s.doc.layers[0].pixels, expected);
        }
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(raw(&s), originals);
        assert_eq!(pixels(&mut s), expected);
        println!("group_bake_cost {action}: {:.3} ms; 1024x1024, 4 rasters, 2 nested groups; Session::apply includes validation, native bake/composition, structural history, replay encoding, temporary journal append and result JSON. Excludes fixture/reference preparation, PNG encoding and transport. No pen-latency or peak-memory claim.",elapsed.as_secs_f64()*1000.);
    }
}
