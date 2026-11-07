//! Subtree copies use fresh owner IDs and independent raw planes in one history edit.
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-group-duplicate");
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
            Err("Injected mask group duplicate failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
struct Fixture {
    s: Session,
    outer: u64,
    inner: u64,
    empty: u64,
    ids: Vec<u64>,
}
fn fixture() -> Fixture {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    for (i, color) in ["#3D4A70", "#D2B5C8", "#8DD8C4"].into_iter().enumerate() {
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
    let inner = create(&mut s, json!([layer(2), layer(3)]), None);
    let empty = create(&mut s, json!([]), Some(inner));
    let outer = create(&mut s, json!([group(inner)]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner,"opacity":173./255.,"compositing":"pass_through"}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"opacity":193./255.}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":empty,"visible":false}),
    );
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|n| matches!(n.data, NodeKind::Layer { index: 1 }))
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
            amount: 0.35,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: id,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    let mut ids = vec![];
    // Source mask order intentionally differs from group allocation/DFS order.
    for owner in [layer(1), group(empty), layer(2), group(outer), group(inner)] {
        ids.push(
            call(&mut s, "mask.create", json!({"owner":owner,"select":false}))["mask_id"]
                .as_u64()
                .unwrap(),
        );
    }
    for (i, id) in ids.iter().enumerate() {
        let plane = if [0, 2, 4].contains(&i) {
            SamplePlane::Dense(vec![11 + i as u8, 37, 91, 128, 203, 255])
        } else {
            SamplePlane::Uniform(177 - i as u8 * 13)
        };
        s.doc.replace_mask_plane(*id, plane).unwrap();
    }
    s.replay_boundary().unwrap();
    for i in [1, 2] {
        call(
            &mut s,
            "mask.set",
            json!({"mask_id":ids[i],"enabled":false,"linked":false,"density":173,"feather_radius_px":1}),
        );
    }
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[3],"density":203,"feather_radius_px":1,"linked":false}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.,"alpha_locked":true,"pixel_locked":true,"position_locked":true,"color_label":"violet"}),
    );
    call(&mut s, "layer.set", json!({"layer_id":3,"visible":false}));
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":2}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[2]}}),
    );
    s.doc.compose();
    outcome(&mut s);
    Fixture {
        s,
        outer,
        inner,
        empty,
        ids,
    }
}
fn duplicate(s: &mut Session, id: u64) -> Value {
    call(s, "group.duplicate", json!({"group_id":id}))
}
fn numbers(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect()
}
fn properties(s: &Session, id: u64) -> Value {
    let mut value = s.state()["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .unwrap()
        .clone();
    for key in [
        "id",
        "index",
        "parent_id",
        "active",
        "selected",
        "effective_visible",
    ] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}

#[test]
fn mask_group_duplicate_copies_ordered_owners_flags_filters_and_independent_raw_planes_once() {
    for disabled_filters in [false, true] {
        let Fixture {
            mut s,
            outer,
            inner,
            empty,
            ids,
        } = fixture();
        if disabled_filters {
            let mut graph = s.doc.graph.clone();
            graph.enabled = false;
            call(&mut s, "graph.set", json!({"graph":graph}));
        }
        let before = content(&s);
        let source_filters = filters(&s);
        let pixels: BTreeMap<_, _> = s
            .doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (s.doc.layer_id(i), l.pixels.as_ptr()))
            .collect();
        let mask_pointers: BTreeMap<_, _> = ids
            .iter()
            .filter_map(|id| match s.doc.mask_plane(*id).unwrap() {
                PlaneView::Dense(p) => Some((*id, p.as_ptr())),
                _ => None,
            })
            .collect();
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        let result = duplicate(&mut s, outer);
        assert_eq!(result["revision"], s.revision);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(result["source_group_id"], outer);
        assert_eq!(result["group_id"], 4);
        assert_eq!(result["group_ids"], json!([4, 5, 6]));
        assert_eq!(result["layer_ids"], json!([4, 5]));
        assert_eq!(result["mask_ids"], json!([6, 7, 8, 9]));
        assert_eq!(s.doc.selected_layer_ids(), vec![4, 5]);
        assert_eq!(s.doc.layer_id(s.doc.active), 4);
        assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: 7 });
        assert_eq!(s.doc.graph.enabled, !disabled_filters);
        let groups = s.doc.composition().groups.unwrap();
        assert_eq!(
            groups.groups.iter().map(|g| g.id).collect::<Vec<_>>(),
            vec![inner, empty, outer, 4, 5, 6]
        );
        assert_eq!(
            &groups.groups[..3],
            before
                .composition
                .groups
                .as_ref()
                .unwrap()
                .groups
                .as_slice()
        );
        for (old, new) in [(outer, 4), (inner, 5), (empty, 6)] {
            let mut expected = before
                .composition
                .groups
                .as_ref()
                .unwrap()
                .groups
                .iter()
                .find(|g| g.id == old)
                .unwrap()
                .clone();
            expected.id = new;
            for item in &mut expected.children {
                item.id = match item.kind {
                    crate::engine::groups::ItemKind::Layer => match item.id {
                        2 => 4,
                        3 => 5,
                        _ => panic!("Unexpected fixture layer"),
                    },
                    crate::engine::groups::ItemKind::Group => match item.id {
                        id if id == inner => 5,
                        id if id == empty => 6,
                        _ => panic!("Unexpected fixture group"),
                    },
                };
            }
            assert_eq!(
                groups.groups.iter().find(|g| g.id == new).unwrap(),
                &expected
            );
        }
        assert_eq!(
            groups.roots,
            vec![
                crate::engine::groups::GroupItem::layer(1),
                crate::engine::groups::GroupItem::group(outer),
                crate::engine::groups::GroupItem::group(4)
            ]
        );
        for (old, new) in [(2, 4), (3, 5)] {
            assert_eq!(properties(&s, old), properties(&s, new));
            assert_eq!(
                s.doc.layers[s.doc.layer_index(new).unwrap()].pixels,
                before.rasters[&old]
            );
            assert_ne!(
                s.doc.layers[s.doc.layer_index(new).unwrap()]
                    .pixels
                    .as_ptr(),
                pixels[&old]
            );
        }
        for (old, new, owner) in [
            (ids[1], 6, group(6)),
            (ids[2], 7, layer(4)),
            (ids[3], 8, group(4)),
            (ids[4], 9, group(5)),
        ] {
            let old_meta = s
                .doc
                .mask_state()
                .items
                .iter()
                .find(|m| m.id == old)
                .unwrap();
            let new_meta = s
                .doc
                .mask_state()
                .items
                .iter()
                .find(|m| m.id == new)
                .unwrap();
            let mut expected = old_meta.clone();
            expected.id = new;
            expected.owner = serde_json::from_value(owner).unwrap();
            assert_eq!(*new_meta, expected);
            assert_eq!(raw(&s)[&new], before.masks[&old]);
            assert_eq!(s.doc.mask_content_revision(new).unwrap(), 0);
            assert_eq!(
                matches!(s.doc.mask_plane(old).unwrap(), PlaneView::Uniform(_)),
                matches!(s.doc.mask_plane(new).unwrap(), PlaneView::Uniform(_))
            );
            if let Some(pointer) = mask_pointers.get(&old) {
                assert!(
                    matches!(s.doc.mask_plane(new).unwrap(),PlaneView::Dense(p)if p.as_ptr()!=*pointer)
                );
            }
        }
        for (id, pointer) in pixels {
            assert_eq!(
                s.doc.layers[s.doc.layer_index(id).unwrap()].pixels.as_ptr(),
                pointer
            );
        }
        for (id, pointer) in mask_pointers {
            assert!(
                matches!(s.doc.mask_plane(id).unwrap(),PlaneView::Dense(p)if p.as_ptr()==pointer)
            );
        }
        let copied_filters = filters(&s);
        assert_eq!(copied_filters.len(), 2);
        for (id, node) in &source_filters {
            assert_eq!(copied_filters[id], *node);
        }
        assert_eq!(
            copied_filters
                .values()
                .filter(|n| !source_filters.contains_key(&n.id))
                .next()
                .unwrap()
                .data,
            source_filters.values().next().unwrap().data
        );
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        assert_eq!(
            s.journal.as_mut().unwrap().events(events, 1).unwrap()["events"][0]
                ["changed_mask_tiles"],
            4
        );
        let after = content(&s);
        outcome(&mut s);
        call(&mut s, "history.undo", json!({}));
        before.after_undo(&s);
        outcome(&mut s);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), after);
        outcome(&mut s);
        call(&mut s, "mask.fill", json!({"mask_id":7,"value":19}));
        assert_eq!(raw(&s)[&ids[2]], before.masks[&ids[2]]);
        assert!(raw(&s)[&7].iter().all(|v| *v == 19));
        call(&mut s, "layer.select", json!({"layer_id":4}));
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":4,"alpha_locked":false,"pixel_locked":false,"position_locked":false}),
        );
        call(&mut s, "layer.clear", json!({}));
        assert!(s.doc.layers[s.doc.layer_index(4).unwrap()]
            .pixels
            .iter()
            .all(|p| *p == 0));
        assert_eq!(
            s.doc.layers[s.doc.layer_index(2).unwrap()].pixels,
            before.rasters[&2]
        );
        outcome(&mut s);
    }
}

#[test]
fn mask_group_duplicate_targets_and_active_selection_follow_source_maps_including_empty_groups() {
    for (target_kind, outside) in [(0, false), (1, false), (2, true), (3, true), (4, false)] {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        call(
            &mut s,
            "layer.selection.set",
            json!({"layer_ids":[1,2],"active_layer_id":if outside{1}else{2}}),
        );
        let target = match target_kind {
            0 => PaintTarget::LayerPixels,
            1 => PaintTarget::Mask { id: ids[2] },
            2 => PaintTarget::Mask { id: ids[3] },
            3 => PaintTarget::Mask { id: ids[0] },
            _ => PaintTarget::Mask { id: ids[1] },
        };
        call(&mut s, "mask.target", json!({"target":target}));
        duplicate(&mut s, outer);
        let expected = match target_kind {
            1 => PaintTarget::Mask { id: 7 },
            2 => PaintTarget::Mask { id: 8 },
            4 => PaintTarget::Mask { id: 6 },
            _ => target,
        };
        assert_eq!(s.doc.mask_state().target, expected);
        assert_eq!(s.doc.layer_id(s.doc.active), if outside { 5 } else { 4 });
        assert_eq!(s.doc.selected_layer_ids(), vec![4, 5]);
        outcome(&mut s);
    }
    let Fixture {
        mut s, empty, ids, ..
    } = fixture();
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[1]}}),
    );
    let before = content(&s);
    let result = duplicate(&mut s, empty);
    assert_eq!(result["group_ids"], json!([4]));
    assert_eq!(result["layer_ids"], json!([]));
    assert_eq!(result["mask_ids"], json!([6]));
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: 6 });
    assert_eq!(s.doc.layer_id(s.doc.active), before.active);
    assert_eq!(
        s.doc.selected_layer_ids(),
        before.composition.selected_layer_ids
    );
    assert_eq!(content(&s).rasters, before.rasters);
    call(&mut s, "history.undo", json!({}));
    before.after_undo(&s);
    outcome(&mut s);
    let unmasked = create(&mut s, json!([]), None);
    let mut composition = s.doc.composition();
    composition.next_layer_id = u32::MAX as u64 + 1;
    composition.raster_masks.next_id = u32::MAX as u64 + 1;
    s.doc.restore_composition(composition);
    s.replay_boundary().unwrap();
    let before = s.doc.composition();
    let result = duplicate(&mut s, unmasked);
    assert_eq!(result["layer_ids"], json!([]));
    assert_eq!(result["mask_ids"], json!([]));
    assert_eq!(s.doc.composition().next_layer_id, before.next_layer_id);
    assert_eq!(s.doc.mask_state().next_id, before.raster_masks.next_id);
    outcome(&mut s);
}

#[test]
fn mask_group_duplicate_compiles_copied_partial_opacity_at_each_nested_boundary() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#FFFFFF"}));
    call(&mut s, "layer.set", json!({"opacity":137./255.}));
    let inner = create(&mut s, json!([layer(2)]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner,"opacity":173./255.}),
    );
    let outer = create(&mut s, json!([group(inner)]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":outer,"opacity":101./255.}),
    );
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":layer(2),"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":91}));
    assert_eq!(s.doc.composite[0] >> 24, 13);
    duplicate(&mut s, outer);
    // round(137*91/255)=49, isolated floors give33 then13; two alpha13
    // roots over transparency have alpha25 under the legacy Normal arithmetic.
    assert_eq!(s.doc.composite[0] >> 24, 25);
    outcome(&mut s);
}

#[test]
fn mask_group_duplicate_undo_allocators_and_atomic_resurrection_keep_exact_source_bytes() {
    let Fixture {
        mut s, outer, ids, ..
    } = fixture();
    let before = content(&s);
    let first = duplicate(&mut s, outer);
    call(&mut s, "history.undo", json!({}));
    before.after_undo(&s);
    let second = duplicate(&mut s, outer);
    for key in ["group_ids", "layer_ids", "mask_ids"] {
        assert!(numbers(&second[key])[0] > *numbers(&first[key]).last().unwrap());
    }
    call(&mut s, "history.undo", json!({}));
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"copy","action":"group.duplicate","args":{"group_id":outer}},
            {"id":"undo","action":"history.undo","args":{}},
            {"id":"source","action":"mask.fill","args":{"mask_id":ids[2],"value":19}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["failed_step"], "fail");
    assert_eq!(result["rolled_back"], true);
    before.audit(&mut s, "pipeline.run", false);
    outcome(&mut s);
    // Redo existed before the checkpoint, pinning the copied-plane history.
    let before_redo = Before::capture(&s);
    let copied_mask = numbers(&second["mask_ids"])[1];
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"redo","action":"history.redo","args":{}},
            {"id":"fill","action":"mask.fill","args":{"mask_id":copied_mask,"value":29}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["failed_step"], "fail");
    assert_eq!(result["rolled_back"], true);
    before_redo.audit(&mut s, "pipeline.run", false);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(raw(&s)[&copied_mask], before.content.masks[&ids[2]]);
    outcome(&mut s);
}

#[test]
fn mask_group_duplicate_preflights_identity_and_small_owner_count_limits() {
    for counter in 0..3 {
        let Fixture { mut s, outer, .. } = fixture();
        let mut composition = s.doc.composition();
        match counter {
            0 => composition.next_group_id = u32::MAX as u64 + 1,
            1 => composition.next_layer_id = u32::MAX as u64 + 1,
            _ => composition.raster_masks.next_id = u32::MAX as u64 + 1,
        };
        s.doc.restore_composition(composition);
        s.replay_boundary().unwrap();
        let before = Before::capture(&s);
        assert!(s
            .apply(Action::new("group.duplicate", json!({"group_id":outer})))
            .is_err());
        before.audit(&mut s, "group.duplicate", false);
    }
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(
        &mut s,
        "mask.create",
        json!({"owner":layer(1),"select":false}),
    );
    let mut last = 0;
    for _ in 0..16 {
        last = create(&mut s, json!([]), None);
    }
    let before = Before::capture(&s);
    assert!(s
        .apply(Action::new("group.duplicate", json!({"group_id":last})))
        .is_err());
    before.audit(&mut s, "group.duplicate", false);
}

#[test]
fn mask_group_duplicate_capability_renderer_and_journal_failures_leave_no_copies() {
    for capability in 1..=4 {
        let Fixture {
            mut s, outer, ids, ..
        } = fixture();
        if capability == 2 {
            call(
                &mut s,
                "mask.set",
                json!({"mask_id":ids[3],"density":255,"feather_radius_px":0}),
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
            .apply(Action::new("group.duplicate", json!({"group_id":outer})))
            .is_err());
        before.audit(&mut s, "group.duplicate", false);
        assert_eq!(renderer.calls.get(), 0);
    }
    let Fixture { mut s, outer, .. } = fixture();
    let renderer = Arc::new(ControlledRenderer {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(true),
        capability: 0,
        calls: Cell::new(0),
    });
    s.doc.renderer = Some(renderer);
    let before = Before::capture(&s);
    let error = s
        .apply(Action::new("group.duplicate", json!({"group_id":outer})))
        .unwrap_err();
    assert_eq!(error.code, "group_render");
    before.audit(&mut s, "group.duplicate", false);
    outcome(&mut s);
    let before = Before::capture(&s);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("group.duplicate", json!({"group_id":outer})))
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
fn mask_group_duplicate_contact_rejection_preserves_recorded_and_pending_pressure_samples() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let Fixture { mut s, outer, .. } = fixture();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1,"color":"#000000","stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 0)).unwrap();
        s.pointer(pointer(kind, PointerPhase::Move, 1)).unwrap();
        s.replay_boundary().unwrap();
        let before = Before::capture(&s);
        assert_eq!(
            s.apply(Action::new("group.duplicate", json!({"group_id":outer})))
                .unwrap_err()
                .code,
            "stroke_active"
        );
        before.restored(&s);
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        assert_eq!(s.stroke.last_sample().unwrap().time, 1);
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
            .map(|p| p["time"].as_u64().unwrap())
            .collect();
        assert_eq!(times, vec![0, 1, 2, 3]);
    }
}

#[test]
fn mask_group_duplicate_required_ids_bind_workflows_and_pipeline_edits_without_running_drafts() {
    let Fixture { mut s, outer, .. } = fixture();
    let schema = s.registry.actions["group.duplicate"]
        .output_schema
        .as_ref()
        .unwrap();
    for key in [
        "source_group_id",
        "group_id",
        "group_ids",
        "layer_ids",
        "mask_ids",
    ] {
        assert!(schema["required"].as_array().unwrap().contains(&json!(key)));
    }
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.group.duplicate","position":[0,0],"parameters":{"group_id":outer as f64}},
        {"id":2,"type":"action.group.set","position":[360,0],"parameters":{"name":"Bound group"}},
        {"id":3,"type":"action.layer.selection.set","position":[720,0],"parameters":{}}
    ],"edges":[{"from":1,"output":"group_id","to":2,"input":"group_id"},{"from":2,"output":"$exec","to":3,"input":"$exec"},{"from":1,"output":"layer_ids","to":3,"input":"layer_ids"}]});
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
    let result = call(&mut s, "workflow.run", json!({"atomic":true}));
    assert_eq!(result["ok"], true);
    let copied = &result["results"]["node_1"]["result"];
    let schema = s.registry.actions["group.duplicate"]
        .output_schema
        .as_ref()
        .unwrap();
    crate::plugins::validate(schema, copied).unwrap();
    assert_eq!(s.doc.selected_layer_ids(), numbers(&copied["layer_ids"]));
    outcome(&mut s);
    let Fixture {
        mut s, outer, ids, ..
    } = fixture();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"copy","action":"group.duplicate","args":{"group_id":outer as f64}},
            {"id":"group","action":"group.set","args":{"group_id":{"$ref":"copy#/result/group_id"},"name":"Bound root"}},
            {"id":"layer","action":"layer.rename","args":{"layer_id":{"$ref":"copy#/result/layer_ids/0"},"name":"Bound cel"}},
            {"id":"mask","action":"mask.fill","args":{"mask_id":{"$ref":"copy#/result/mask_ids/0"},"value":19}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.layer_name(s.doc.layer_index(4).unwrap()), "Bound cel");
    assert!(raw(&s)[&6].iter().all(|v| *v == 19));
    assert!(raw(&s)[&ids[1]].iter().any(|v| *v != 19));
    outcome(&mut s);
}

#[test]
fn mask_group_duplicate_projects_and_outcomes_restore_new_raw_ids_without_dlls() {
    let Fixture { mut s, outer, .. } = fixture();
    duplicate(&mut s, outer);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "history.redo", json!({}));
    let expected = content(&s);
    let path = output("copy.mcp");
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
    for (id, bytes) in raw(&s) {
        assert_eq!(
            (0..bytes.len())
                .map(|i| restored.mask_plane(id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            bytes
        );
    }
}

#[test]
fn mask_group_duplicate_result_group_ids_follow_allocation_order_in_a_branching_tree() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    let root = create(&mut s, json!([layer(1)]), None);
    let a = create(&mut s, json!([]), Some(root));
    let a1 = create(&mut s, json!([]), Some(a));
    let b = create(&mut s, json!([]), Some(root));
    assert_eq!((root, a, a1, b), (1, 2, 3, 4));
    let mask = call(&mut s, "mask.create", json!({"owner":group(a1)}))["mask_id"]
        .as_u64()
        .unwrap();
    assert_eq!(mask, 1);
    let result = duplicate(&mut s, root);
    assert_eq!(result["source_group_id"], root);
    assert_eq!(result["group_id"], 5);
    assert_eq!(result["group_ids"], json!([5, 6, 7, 8]));
    assert_eq!(result["layer_ids"], json!([2]));
    assert_eq!(result["mask_ids"], json!([2]));
    let copied_public_order: Vec<_> = result["groups"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_u64().unwrap())
        .filter(|id| *id >= 5)
        .collect();
    // Public DFS visits the topmost sibling B before A and its child A1.
    assert_eq!(copied_public_order, vec![5, 7, 6, 8]);
    assert_eq!(
        s.doc
            .mask_state()
            .items
            .iter()
            .find(|mask| mask.id == 2)
            .unwrap()
            .owner,
        crate::engine::raster_masks::MaskOwner::Group { id: 8 }
    );
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: 2 });
    outcome(&mut s);
}
