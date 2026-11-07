//! Subtree deletion removes only consumed owner planes under one history edit.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    graph::{Edge, Filter, Graph, Node, NodeKind, Renderer},
    raster_masks::{PaintTarget, PlaneView, SamplePlane},
    replay::Journal,
    Document,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-group-delete");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn layer(id: u64) -> Value {
    json!({"kind":"layer","id":id})
}
fn group(id: u64) -> Value {
    json!({"kind":"group","id":id})
}
fn create(s: &mut Session, members: Value, parent: Option<u64>) -> u64 {
    let mut args = json!({"members":members,"name":"Folder"});
    if let Some(parent) = parent {
        args["parent_id"] = json!(parent);
    }
    call(s, "group.create", args)["group_id"].as_u64().unwrap()
}
fn raw(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            let plane = s.doc.mask_plane(m.id).unwrap();
            (
                m.id,
                (0..s.doc.width * s.doc.height)
                    .map(|i| plane.sample(i))
                    .collect(),
            )
        })
        .collect()
}
fn filters(s: &Session) -> BTreeMap<u32, Node> {
    s.doc
        .graph
        .nodes
        .iter()
        .filter(|n| matches!(n.data, NodeKind::Filter { .. }))
        .map(|n| {
            // Existing linked-stack layout is arranged when raster identities change.
            let mut n = n.clone();
            n.position = [0., 0.];
            (n.id, n)
        })
        .collect()
}
#[derive(Clone, Debug, PartialEq)]
struct Content {
    active: u64,
    composition: Composition,
    rasters: BTreeMap<u64, Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
}
fn content(s: &Session) -> Content {
    Content {
        active: s.doc.layer_id(s.doc.active),
        composition: s.doc.composition(),
        masks: raw(s),
        rasters: s
            .doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (s.doc.layer_id(i), l.pixels.clone()))
            .collect(),
    }
}
impl Content {
    fn after_undo(&self, s: &Session) {
        let actual = content(s);
        let mut expected = self.clone();
        expected.composition.next_layer_id = actual.composition.next_layer_id;
        expected.composition.next_group_id = actual.composition.next_group_id;
        expected.composition.raster_masks.next_id = actual.composition.raster_masks.next_id;
        assert_eq!(actual, expected);
    }
}
struct Before {
    content: Content,
    public: Value,
    surface: Vec<u32>,
    graph: Graph,
    history: usize,
    events: usize,
    revision: u64,
}
impl Before {
    fn capture(s: &Session) -> Self {
        Self {
            content: content(s),
            public: s.state(),
            surface: s.doc.composite.clone(),
            graph: s.doc.effective_graph().unwrap(),
            history: s.doc.history_bytes(),
            events: s.journal.as_ref().unwrap().entries.len(),
            revision: s.revision,
        }
    }
    fn restored(&self, s: &Session) {
        assert_eq!(content(s), self.content);
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.doc.effective_graph().unwrap(), self.graph);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert_eq!(s.revision, self.revision);
        assert!(!s.doc.history_group_active());
    }
    fn audit(&self, s: &mut Session, action: &str, success: bool) {
        self.restored(s);
        let journal = s.journal.as_mut().unwrap();
        assert_eq!(journal.entries.len(), self.events + 1);
        let events = journal.events(self.events, 1).unwrap();
        let event = &events["events"][0];
        assert_eq!(event["source"]["action"], action);
        assert_eq!(event["success"], success);
        assert_eq!(event["before"], event["after"]);
        assert_eq!(event["changed_tiles"], 0);
        assert_eq!(event["changed_mask_tiles"], 0);
    }
}
fn outcome(s: &mut Session) {
    check(s);
    let last = s.journal.as_ref().unwrap().entries.len() - 1;
    let recorded = s.replay_preview(last, false).unwrap();
    assert!(recorded.matches_document(&s.doc));
    let mut restored = Document::new(1, 1);
    recorded.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, bytes) in raw(s) {
        assert_eq!(
            (0..bytes.len())
                .map(|i| restored.mask_plane(id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            bytes
        );
    }
}
struct Fixture {
    s: Session,
    outer: u64,
    inner: u64,
    survivor: u64,
    empty: u64,
    ids: Vec<u64>,
}
fn fixture() -> Fixture {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    for (i, color) in ["#5874B7", "#E7ABC8", "#83CEC9", "#F1D69B", "#AC8EC6"]
        .into_iter()
        .enumerate()
    {
        if i != 0 {
            call(&mut s, "layer.add", json!({}));
        }
        call(&mut s, "layer.fill_color", json!({"color":color}));
        call(
            &mut s,
            "layer.rename",
            json!({"name":format!("Cel {} 墨",i+1)}),
        );
    }
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"pixel_locked":true,"position_locked":true,"opacity":137./255.,"color_label":"violet"}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":5,"opacity":173./255.,"color_label":"blue"}),
    );
    let inner = create(&mut s, json!([layer(2), layer(3)]), None);
    let outer = create(&mut s, json!([group(inner), layer(4)]), None);
    let survivor = create(&mut s, json!([layer(5)]), None);
    let empty = create(&mut s, json!([]), Some(outer));
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner,"visible":false,"opacity":173./255.}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"opacity":193./255.,"compositing":"pass_through"}),
    );
    let mut graph = s.doc.graph.clone();
    for index in [1, 4] {
        let source = graph
            .nodes
            .iter()
            .find(|n| matches!(n.data,NodeKind::Layer { index:i } if i==index))
            .unwrap()
            .id;
        let id = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
        for edge in &mut graph.edges {
            if edge.from == source {
                edge.from = id;
            }
        }
        graph.nodes.push(Node {
            id,
            node_type: None,
            position: [33., 44.],
            data: NodeKind::Filter {
                filter: Filter::Invert,
                amount: 0.25,
            },
        });
        graph.edges.push(Edge {
            from: source,
            to: id,
            input: 0,
        });
    }
    call(&mut s, "graph.set", json!({"graph":graph}));
    let mut ids = vec![];
    // Deleted and retained mask slots intentionally alternate. The surviving
    // owner namespaces also overlap group/layer numeric IDs.
    for owner in [
        layer(1),
        group(inner),
        group(survivor),
        layer(2),
        group(outer),
        layer(5),
        group(empty),
        layer(4),
    ] {
        ids.push(
            call(&mut s, "mask.create", json!({"owner":owner,"select":false}))["mask_id"]
                .as_u64()
                .unwrap(),
        );
    }
    for (i, id) in ids.iter().enumerate() {
        let plane = if [0, 3, 4, 5].contains(&i) {
            SamplePlane::Dense(vec![17 + i as u8, 31, 97, 128, 211, 255])
        } else {
            SamplePlane::Uniform(177 - i as u8 * 9)
        };
        s.doc.replace_mask_plane(*id, plane).unwrap();
    }
    // Direct raw fixture writes must enter the verified journal before another
    // shared action; replace_mask_plane has already marked their A8 tiles.
    s.replay_boundary().unwrap();
    for index in [1, 2, 6, 7] {
        call(
            &mut s,
            "mask.set",
            json!({"mask_id":ids[index],"enabled":index!=2&&index!=7,"density":203,"feather_radius_px":1,"linked":false}),
        );
    }
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2,5],"active_layer_id":2}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[3]}}),
    );
    s.doc.compose();
    outcome(&mut s);
    Fixture {
        s,
        outer,
        inner,
        survivor,
        empty,
        ids,
    }
}
fn delete(s: &mut Session, id: u64) -> Value {
    call(s, "group.delete", json!({"group_id":id}))
}
fn retained_pointers(s: &Session) -> (BTreeMap<u64, *const u32>, BTreeMap<u64, *const u8>) {
    (
        s.doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (s.doc.layer_id(i), l.pixels.as_ptr()))
            .collect(),
        s.doc
            .mask_state()
            .items
            .iter()
            .filter_map(|m| match s.doc.mask_plane(m.id).unwrap() {
                PlaneView::Dense(p) => Some((m.id, p.as_ptr())),
                _ => None,
            })
            .collect(),
    )
}

#[test]
fn mask_group_delete_noncontiguous_owner_slots_preserve_survivors_and_one_compound_undo() {
    let Fixture {
        mut s,
        outer,
        inner,
        survivor,
        empty,
        ids,
    } = fixture();
    let before = content(&s);
    let pointers = retained_pointers(&s);
    let old_filters = filters(&s);
    let old_survivor = s
        .doc
        .composition()
        .groups
        .unwrap()
        .groups
        .into_iter()
        .find(|g| g.id == survivor)
        .unwrap();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = delete(&mut s, outer);
    assert_eq!(result["deleted_group_ids"], json!([inner, outer, empty]));
    assert_eq!(result["deleted_layer_ids"], json!([2, 3, 4]));
    assert_eq!(
        result["deleted_mask_ids"],
        json!([ids[1], ids[3], ids[4], ids[6], ids[7]])
    );
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    assert_eq!(s.doc.layer_id(s.doc.active), 1);
    assert_eq!(s.doc.selected_layer_ids(), vec![1, 5]);
    assert_eq!(
        s.doc.composition().groups.unwrap().groups,
        vec![old_survivor]
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(
        s.journal.as_mut().unwrap().events(events, 1).unwrap()["events"][0]["changed_mask_tiles"],
        0
    );
    for id in [1, 5] {
        assert_eq!(
            s.doc.layers[s.doc.layer_index(id).unwrap()].pixels,
            before.rasters[&id]
        );
        assert_eq!(
            s.doc.layers[s.doc.layer_index(id).unwrap()].pixels.as_ptr(),
            pointers.0[&id]
        );
    }
    for id in [ids[0], ids[2], ids[5]] {
        assert_eq!(raw(&s)[&id], before.masks[&id]);
        if let Some(pointer) = pointers.1.get(&id) {
            assert!(
                matches!(s.doc.mask_plane(id).unwrap(),PlaneView::Dense(p)if p.as_ptr()==*pointer)
            );
        }
    }
    assert_eq!(filters(&s).len(), 1);
    for (id, node) in filters(&s) {
        assert_eq!(node, old_filters[&id]);
    }
    let after = content(&s);
    outcome(&mut s);
    call(&mut s, "history.undo", json!({}));
    before.after_undo(&s);
    outcome(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(content(&s), after);
    outcome(&mut s);
}

#[test]
fn mask_group_delete_targets_selection_and_empty_masked_group_follow_explicit_fallbacks() {
    for target in 0..4 {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        let target = match target {
            0 => PaintTarget::LayerPixels,
            1 => PaintTarget::Mask { id: ids[1] },
            2 => PaintTarget::Mask { id: ids[0] },
            _ => PaintTarget::Mask { id: ids[2] },
        };
        call(&mut s, "mask.target", json!({"target":target}));
        call(
            &mut s,
            "layer.selection.set",
            json!({"layer_ids":[2,3,5],"active_layer_id":3}),
        );
        // Selection changes reset the explicit paint target, so restore it last.
        call(&mut s, "mask.target", json!({"target":target}));
        delete(&mut s, outer);
        assert_eq!(s.doc.layer_id(s.doc.active), 5);
        assert_eq!(s.doc.selected_layer_ids(), vec![5]);
        assert_eq!(
            s.doc.mask_state().target,
            if target == (PaintTarget::Mask { id: ids[1] }) {
                PaintTarget::LayerPixels
            } else {
                target
            }
        );
        outcome(&mut s);
    }
    let Fixture {
        mut s, empty, ids, ..
    } = fixture();
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[6]}}),
    );
    let before = content(&s);
    let result = delete(&mut s, empty);
    assert_eq!(result["deleted_group_ids"], json!([empty]));
    assert_eq!(result["deleted_layer_ids"], json!([]));
    assert_eq!(result["deleted_mask_ids"], json!([ids[6]]));
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    assert_eq!(s.doc.layer_id(s.doc.active), 2);
    assert_eq!(content(&s).rasters, before.rasters);
    call(&mut s, "history.undo", json!({}));
    before.after_undo(&s);
    outcome(&mut s);
    let Fixture { mut s, outer, .. } = fixture();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[2,3,4],"active_layer_id":3}),
    );
    delete(&mut s, outer);
    assert_eq!(s.doc.layer_id(s.doc.active), 5);
    assert_eq!(s.doc.selected_layer_ids(), vec![5]);
    outcome(&mut s);
}

#[test]
fn mask_group_delete_compiles_retained_partial_opacity_after_raster_removal() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    let deleted = create(&mut s, json!([layer(1)]), None);
    call(&mut s, "layer.add", json!({"parent_id":0}));
    call(&mut s, "layer.fill_color", json!({"color":"#FFFFFF"}));
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.}),
    );
    let retained = create(&mut s, json!([layer(2)]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":retained,"opacity":173./255.}),
    );
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":layer(2),"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":91}));
    delete(&mut s, deleted);
    assert_eq!(s.doc.composite[0] >> 24, 33);
    outcome(&mut s);
}

#[test]
fn mask_group_delete_undo_keeps_all_allocators_monotonic_and_atomic_failure_restores_exactly() {
    let Fixture { mut s, outer, .. } = fixture();
    let before = content(&s);
    delete(&mut s, outer);
    let fresh_group = create(&mut s, json!([]), None);
    let copied = call(&mut s, "layer.duplicate", json!({"layer_id":5}));
    let fresh_layer = copied["layer_id"].as_u64().unwrap();
    let fresh_mask = copied["mask_ids"][0].as_u64().unwrap();
    for _ in 0..3 {
        call(&mut s, "history.undo", json!({}));
    }
    before.after_undo(&s);
    assert!(s.doc.composition().next_group_id > fresh_group);
    assert!(s.doc.composition().next_layer_id > fresh_layer);
    assert!(s.doc.mask_state().next_id > fresh_mask);
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"delete","action":"group.delete","args":{"group_id":outer}},
            {"id":"group","action":"group.create","args":{"members":[]}},
            {"id":"copy","action":"layer.duplicate","args":{"layer_id":5}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    before.audit(&mut s, "pipeline.run", false);
    outcome(&mut s);
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"delete","action":"group.delete","args":{"group_id":outer}},
            {"id":"undo","action":"history.undo","args":{}},
            {"id":"rename","action":"layer.rename","args":{"layer_id":{"$ref":"delete#/result/deleted_layer_ids/0"},"name":"Resurrected temporary"}},
            {"id":"fill","action":"mask.fill","args":{"mask_id":{"$ref":"delete#/result/deleted_mask_ids/0"},"value":17}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    before.audit(&mut s, "pipeline.run", false);
    outcome(&mut s);
}

struct ControlledRenderer {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    capability: u8,
    calls: Cell<usize>,
}
impl Renderer for ControlledRenderer {
    fn supports_masks(&self) -> bool {
        self.capability != 1 && self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.capability != 2 && self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.capability != 3 && self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.capability != 4 && self.native.supports_transparent()
    }
    fn supports_blend_mode(&self, m: u32) -> bool {
        self.native.supports_blend_mode(m)
    }
    fn reserve_mask_effects(&self, w: u32, h: u32, r: u32) -> Result<(), String> {
        self.native.reserve_mask_effects(w, h, r)
    }
    fn mask_effect_region(
        &self,
        r: &abi::MaskEffectRequest,
        s: &abi::CompositeMask,
        w: u32,
        h: u32,
        o: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.native.mask_effect_region(r, s, w, h, o)
    }
    fn render(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        self.native.render(n, l, w, h, o)
    }
    fn render_masked(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        m: &[abi::CompositeMask],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if self.armed.replace(false) {
            unsafe { o.data.write(0xdeadc0de) };
            Err("Injected mask group delete failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
#[test]
fn mask_group_delete_preflights_hidden_disabled_source_and_retained_candidate_capabilities() {
    for capability in 1..=4 {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        // Only deleted owners now need effects/Mix. Admission must validate the
        // complete source rather than merely the smaller candidate graph.
        if capability == 2 {
            call(
                &mut s,
                "mask.set",
                json!({"mask_id":ids[2],"density":255,"feather_radius_px":0}),
            );
        }
        let renderer = Arc::new(ControlledRenderer {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            capability,
            calls: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        let before = Before::capture(&s);
        assert!(s
            .apply(Action::new("group.delete", json!({"group_id":outer})))
            .is_err());
        before.audit(&mut s, "group.delete", false);
        assert_eq!(renderer.calls.get(), 0);
    }
    let Fixture {
        mut s,
        outer,
        survivor,
        ..
    } = fixture();
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"compositing":"isolated"}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":survivor,"compositing":"pass_through","visible":false}),
    );
    let renderer = Arc::new(ControlledRenderer {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(false),
        capability: 3,
        calls: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    let before = Before::capture(&s);
    assert!(s
        .apply(Action::new("group.delete", json!({"group_id":outer})))
        .is_err());
    before.audit(&mut s, "group.delete", false);
    assert_eq!(renderer.calls.get(), 0);
}

#[test]
fn mask_group_delete_renderer_and_partial_journal_failures_restore_every_removed_plane_and_slot() {
    let Fixture { mut s, outer, .. } = fixture();
    let renderer = Arc::new(ControlledRenderer {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(true),
        capability: 0,
        calls: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    let before = Before::capture(&s);
    let error = s
        .apply(Action::new("group.delete", json!({"group_id":outer})))
        .unwrap_err();
    assert_eq!(error.code, "group_render");
    assert!(error.message.contains("Injected mask group delete"));
    before.audit(&mut s, "group.delete", false);
    outcome(&mut s);
    let before = Before::capture(&s);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("group.delete", json!({"group_id":outer})))
        .is_err());
    before.restored(&s);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert!(s.replay_error.is_some());
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

#[test]
fn mask_group_delete_preexisting_history_survives_failed_undo_and_checkpoint_pinned_resurrection() {
    for atomic in [false, true] {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        let original = content(&s);
        delete(&mut s, outer);
        // The deletion predates the rollback checkpoint. Unlike a delete made
        // inside a new pipeline, its shared structural history is pinned here.
        let deleted = Before::capture(&s);
        if atomic {
            let result = call(
                &mut s,
                "pipeline.run",
                json!({"atomic":true,"steps":[
                    {"id":"restore","action":"history.undo","args":{}},
                    {"id":"fill","action":"mask.fill","args":{"mask_id":ids[3],"value":29}},
                    {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
                ]}),
            );
            assert_eq!(result["ok"], false);
            assert_eq!(result["rolled_back"], true);
            deleted.audit(&mut s, "pipeline.run", false);
        } else {
            let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
            s.journal.as_mut().unwrap().fail_next_write_after(7);
            assert!(s.apply(Action::new("history.undo", json!({}))).is_err());
            deleted.restored(&s);
            assert_eq!(s.journal.as_ref().unwrap().entries.len(), deleted.events);
            assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
            assert!(s.replay_error.is_some());
            assert_eq!(
                s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
                true
            );
            let blocked = s.apply(Action::new("history.undo", json!({}))).unwrap_err();
            assert_eq!(blocked.code, "replay_storage");
            deleted.restored(&s);
            assert_eq!(s.journal.as_ref().unwrap().entries.len(), deleted.events);
            assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
            // Like replay_tests::partial_journal_write_rolls_back_action, simulate
            // recovered storage explicitly. Production recording stays faulted;
            // this fixture resumes only to inspect the retained Undo record.
            s.replay_error = None;
        }
        assert!(s.doc.mask_plane(ids[3]).is_err());
        call(&mut s, "history.undo", json!({}));
        original.after_undo(&s);
        outcome(&mut s);
        // A restored mutable plane must not alias the deleted history retained
        // by either checkpoint path. Undo/Redo retains the original byte slots.
        call(&mut s, "mask.fill", json!({"mask_id":ids[3],"value":29}));
        assert!(raw(&s)[&ids[3]].iter().all(|sample| *sample == 29));
        call(&mut s, "history.undo", json!({}));
        original.after_undo(&s);
        outcome(&mut s);
        call(&mut s, "history.redo", json!({}));
        let mut expected = original.masks.clone();
        expected.get_mut(&ids[3]).unwrap().fill(29);
        assert_eq!(raw(&s), expected);
        outcome(&mut s);
    }
}

fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 0.7 + time as f32 * 0.1,
        y: 0.7,
        pressure: 0.3 + time as f32 * 0.05,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(kind),
        view_x: Some(40. + time as f32),
        view_y: Some(40.),
    }
}
#[test]
fn mask_group_delete_live_brush_and_hand_rejection_preserves_recorded_and_pending_samples() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        call(&mut s, "layer.select", json!({"layer_id":5}));
        call(
            &mut s,
            "mask.target",
            json!({"target":{"kind":"mask","id":ids[5]}}),
        );
        call(
            &mut s,
            "brush.set",
            json!({"radius":1,"color":"#000000","stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 0)).unwrap();
        s.pointer(pointer(kind, PointerPhase::Move, 1)).unwrap();
        s.replay_boundary().unwrap();
        let before = Before::capture(&s);
        let pointer_before = retained_pointers(&s).1[&ids[5]];
        let error = s
            .apply(Action::new("group.delete", json!({"group_id":outer})))
            .unwrap_err();
        assert_eq!(error.code, "stroke_active");
        before.restored(&s);
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        assert_eq!(s.stroke.last_sample().unwrap().time, 1);
        assert_eq!(retained_pointers(&s).1[&ids[5]], pointer_before);
        s.pointer(pointer(kind, PointerPhase::Move, 2)).unwrap();
        s.pointer(pointer(kind, PointerPhase::Up, 3)).unwrap();
        outcome(&mut s);
        let events = call(&mut s, "replay.events", json!({"limit":256}));
        let times: Vec<_> = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e["source"]["samples"].as_array())
            .flatten()
            .map(|v| v["time"].as_u64().unwrap())
            .collect();
        assert_eq!(times, vec![0, 1, 2, 3]);
    }
}

#[test]
fn mask_group_delete_result_arrays_are_required_schema_outputs_and_bind_typed_drafts() {
    let Fixture { mut s, outer, .. } = fixture();
    let schema = s.registry.actions["group.delete"]
        .output_schema
        .as_ref()
        .unwrap();
    for key in ["deleted_group_ids", "deleted_layer_ids", "deleted_mask_ids"] {
        assert!(schema["required"].as_array().unwrap().contains(&json!(key)));
        assert_eq!(schema["properties"][key]["type"], "array");
    }
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.group.delete","position":[0,0],"parameters":{"group_id":outer as f64}},
        {"id":2,"type":"action.history.undo","position":[360,0],"parameters":{}},
        {"id":3,"type":"action.layer.selection.set","position":[720,0],"parameters":{}}
    ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"},
        {"from":2,"output":"$exec","to":3,"input":"$exec"},
        {"from":1,"output":"deleted_layer_ids","to":3,"input":"layer_ids"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    let before = Before::capture(&s);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    before.audit(&mut s, "workflow.run", true);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.selected_layer_ids(), vec![2, 3, 4]);
    assert_eq!(raw(&s), before.content.masks);
    outcome(&mut s);
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"remove","action":"group.delete","args":{"group_id":outer as f64}},
            {"id":"consumed","action":"mask.fill","args":{"mask_id":{"$ref":"remove#/result/deleted_mask_ids/0"},"value":17}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    before.audit(&mut s, "pipeline.run", false);
    let result = delete(&mut s, outer);
    let schema = s.registry.actions["group.delete"]
        .output_schema
        .as_ref()
        .unwrap();
    crate::plugins::validate(schema, &result).unwrap();
    outcome(&mut s);
}

#[test]
fn mask_group_delete_projects_and_dll_free_replay_keep_survivors_and_remaining_guards() {
    let Fixture {
        mut s,
        outer,
        survivor,
        ..
    } = fixture();
    delete(&mut s, outer);
    let expected = content(&s);
    let path = output("deleted.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(content(&opened), expected);
    outcome(&mut opened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let recorded = journal.preview(journal.entries.len() - 1).unwrap();
    assert_eq!(replay::png(&recorded).unwrap(), png);
    let mut restored = Document::new(1, 1);
    recorded.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    call(
        &mut s,
        "mask.target",
        json!({"target":PaintTarget::LayerPixels}),
    );
    for (action, args) in [
        ("group.duplicate", json!({"group_id":survivor})),
        ("group.ungroup", json!({"group_id":survivor})),
        ("group.merge", json!({"group_id":survivor})),
        (
            "layer.batch",
            json!({"action":"layer.set","layer_ids":[1,5],"args":{"opacity":0.5}}),
        ),
        ("layer.translate", json!({"dx":1,"dy":0})),
        ("layer.via_copy", json!({})),
        ("clipboard.paste", json!({})),
    ] {
        let before = Before::capture(&s);
        let error = s.apply(Action::new(action, args)).unwrap_err();
        assert_eq!(error.code, "mask_editing_pending");
        before.audit(&mut s, action, false);
    }
    let all = create(&mut s, json!([layer(1), group(survivor)]), None);
    let before = Before::capture(&s);
    let error = s
        .apply(Action::new("group.delete", json!({"group_id":all})))
        .unwrap_err();
    assert!(error.message.contains("final raster"));
    before.audit(&mut s, "group.delete", false);
}
