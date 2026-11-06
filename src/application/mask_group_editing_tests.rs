//! Owner-preserving group creation/reparenting never rewrites raw A8 planes.
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
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-group-editing");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn raw(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            let view = s.doc.mask_plane(m.id).unwrap();
            (
                m.id,
                (0..s.doc.width * s.doc.height)
                    .map(|i| view.sample(i))
                    .collect(),
            )
        })
        .collect()
}
fn raster(s: &Session) -> BTreeMap<u64, Vec<u32>> {
    s.doc
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (s.doc.layer_id(i), l.pixels.clone()))
        .collect()
}
fn layer(id: u64) -> Value {
    json!({"kind":"layer","id":id})
}
fn group(id: u64) -> Value {
    json!({"kind":"group","id":id})
}
fn create(s: &mut Session, members: Value, parent: Option<u64>) -> u64 {
    let mut args = json!({"members":members,"name":"New container"});
    if let Some(parent) = parent {
        args["parent_id"] = json!(parent);
    }
    call(s, "group.create", args)["group_id"].as_u64().unwrap()
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
        rasters: raster(s),
        masks: raw(s),
    }
}
struct Before {
    content: Content,
    public: Value,
    surface: Vec<u32>,
    effective: Graph,
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
            effective: s.doc.effective_graph().unwrap(),
            history: s.doc.history_bytes(),
            events: s.journal.as_ref().unwrap().entries.len(),
            revision: s.revision,
        }
    }
    fn restored(&self, s: &Session) {
        assert_eq!(content(s), self.content);
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.doc.effective_graph().unwrap(), self.effective);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert_eq!(s.revision, self.revision);
        assert!(!s.doc.history_group_active());
    }
    fn audit(&self, s: &mut Session, action: &str) {
        self.restored(s);
        let journal = s.journal.as_mut().unwrap();
        assert_eq!(journal.entries.len(), self.events + 1);
        let events = journal.events(self.events, 1).unwrap();
        let event = &events["events"][0];
        assert_eq!(event["source"]["action"], action);
        assert_eq!(event["success"], false);
        assert_eq!(event["before"], event["after"]);
        assert_eq!(event["changed_tiles"], 0);
        assert_eq!(event["changed_mask_tiles"], 0);
    }
}
struct Preserved {
    raw: BTreeMap<u64, Vec<u8>>,
    rasters: BTreeMap<u64, Vec<u32>>,
    active: u64,
    selected: BTreeSet<u64>,
    metadata: Value,
    layer_properties: BTreeMap<u64, Value>,
    filters: BTreeMap<u32, Node>,
    order: Vec<u64>,
    filter_positions: BTreeMap<u32, [f32; 2]>,
    pointers: BTreeMap<u64, *const u32>,
    mask_pointers: BTreeMap<u64, *const u8>,
}
impl Preserved {
    fn capture(s: &Session) -> Self {
        Self {
            raw: raw(s),
            rasters: raster(s),
            active: s.doc.layer_id(s.doc.active),
            selected: s.doc.selected_layer_ids().into_iter().collect(),
            metadata: serde_json::to_value(s.doc.mask_state()).unwrap(),
            layer_properties: s.state()["layers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let mut row = row.clone();
                    for key in ["index", "parent_id", "effective_visible"] {
                        row.as_object_mut().unwrap().remove(key);
                    }
                    (row["id"].as_u64().unwrap(), row)
                })
                .collect(),
            // A raster permutation intentionally re-arranges the linked stack.
            // Stable filter identity, type and parameters remain independent
            // from that generated layout.
            filters: s
                .doc
                .graph
                .nodes
                .iter()
                .filter(|n| matches!(n.data, NodeKind::Filter { .. }))
                .map(|n| {
                    let mut value = n.clone();
                    value.position = [0., 0.];
                    (n.id, value)
                })
                .collect(),
            order: (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect(),
            filter_positions: s
                .doc
                .graph
                .nodes
                .iter()
                .filter(|n| matches!(n.data, NodeKind::Filter { .. }))
                .map(|n| (n.id, n.position))
                .collect(),
            pointers: s
                .doc
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| (s.doc.layer_id(i), l.pixels.as_ptr()))
                .collect(),
            mask_pointers: s
                .doc
                .mask_state()
                .items
                .iter()
                .filter_map(|m| match s.doc.mask_plane(m.id).unwrap() {
                    PlaneView::Dense(v) => Some((m.id, v.as_ptr())),
                    _ => None,
                })
                .collect(),
        }
    }
    fn unchanged(&self, s: &Session) {
        assert_eq!(raw(s), self.raw);
        assert_eq!(raster(s), self.rasters);
        assert_eq!(s.doc.layer_id(s.doc.active), self.active);
        assert_eq!(
            s.doc
                .selected_layer_ids()
                .into_iter()
                .collect::<BTreeSet<_>>(),
            self.selected
        );
        assert_eq!(
            serde_json::to_value(s.doc.mask_state()).unwrap(),
            self.metadata
        );
        let actual = Self::capture(s);
        assert_eq!(actual.layer_properties, self.layer_properties);
        assert_eq!(actual.filters, self.filters);
        if actual.order == self.order {
            assert_eq!(actual.filter_positions, self.filter_positions);
        }
        assert_eq!(actual.pointers, self.pointers);
        assert_eq!(actual.mask_pointers, self.mask_pointers);
    }
}
struct Fixture {
    s: Session,
    inner: u64,
    hidden: u64,
    target: u64,
}
fn fixture() -> Fixture {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    for (i, color) in ["#5874B7", "#E7ABC8", "#83CEC9", "#F1D69B"]
        .iter()
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
        json!({"layer_id":2,"opacity":137./255.,"pixel_locked":true,"position_locked":true,"color_label":"violet"}),
    );
    let inner = create(&mut s, json!([layer(2), layer(3)]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner,"opacity":173./255.}),
    );
    let hidden = create(&mut s, json!([]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":hidden,"visible":false,"compositing":"pass_through"}),
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
            amount: 0.25,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: id,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    let mut ids = vec![];
    for owner in [layer(1), layer(2), group(inner), group(hidden)] {
        ids.push(
            call(&mut s, "mask.create", json!({"owner":owner,"select":false}))["mask_id"]
                .as_u64()
                .unwrap(),
        );
    }
    s.doc
        .replace_mask_plane(ids[0], SamplePlane::Dense(vec![0, 31, 97, 128, 211, 255]))
        .unwrap();
    s.doc
        .replace_mask_plane(ids[1], SamplePlane::Dense(vec![255, 201, 128, 64, 1, 0]))
        .unwrap();
    s.doc
        .replace_mask_plane(ids[2], SamplePlane::Uniform(177))
        .unwrap();
    s.doc
        .replace_mask_plane(ids[3], SamplePlane::Uniform(99))
        .unwrap();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[2],"density":203,"feather_radius_px":1,"linked":false}),
    );
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[3],"enabled":false,"density":137,"feather_radius_px":1}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,4],"active_layer_id":4}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[0]}}),
    );
    s.doc.compose();
    outcome(&mut s);
    Fixture {
        s,
        inner,
        hidden,
        target: ids[0],
    }
}
fn one_edit(s: &mut Session, action: &str, args: Value) -> Value {
    let before = content(s);
    let preserved = Preserved::capture(s);
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = call(s, action, args);
    if matches!(action, "group.create" | "group.create_selected") {
        let schema = s.registry.actions[action].output_schema.as_ref().unwrap();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("group_id")));
        crate::plugins::validate(schema, &result).unwrap();
    }
    preserved.unchanged(s);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    let rows = s.journal.as_mut().unwrap().events(events, 1).unwrap();
    assert_eq!(rows["events"][0]["source"]["action"], action);
    assert_eq!(rows["events"][0]["success"], true);
    assert_eq!(rows["events"][0]["changed_mask_tiles"], 0);
    let after = content(s);
    outcome(s);
    call(s, "history.undo", json!({}));
    let mut expected = before;
    expected.composition.next_group_id = s.doc.composition().next_group_id;
    assert_eq!(content(s), expected);
    outcome(s);
    call(s, "history.redo", json!({}));
    assert_eq!(content(s), after);
    outcome(s);
    result
}

#[test]
fn mask_group_empty_wrapped_and_selected_creation_preserves_all_owners_and_one_history_edit() {
    for selected in [false, true] {
        let Fixture { mut s, inner, .. } = fixture();
        if selected {
            call(
                &mut s,
                "layer.selection.set",
                json!({"layer_ids":[2,3],"active_layer_id":2}),
            );
            let target = s.doc.mask_state().items[2].id;
            call(
                &mut s,
                "mask.target",
                json!({"target":{"kind":"mask","id":target}}),
            );
            let result = one_edit(
                &mut s,
                "group.create_selected",
                json!({"name":"Selected cels"}),
            );
            let id = result["group_id"].as_u64().unwrap();
            assert_eq!(s.doc.group_layer_ids(id).unwrap(), vec![2, 3]);
            assert_eq!(s.doc.layer_parent_id(s.doc.layer_index(2).unwrap()), id);
        } else {
            let result = one_edit(
                &mut s,
                "group.create",
                json!({"members":[],"parent_id":inner,"name":"Empty child"}),
            );
            let empty = result["group_id"].as_u64().unwrap();
            assert!(s.doc.group_layer_ids(empty).unwrap().is_empty());
            let result = one_edit(
                &mut s,
                "group.create",
                json!({"members":[layer(1),group(inner)],"name":"Wrapped owners"}),
            );
            let wrapper = result["group_id"].as_u64().unwrap();
            assert_eq!(s.doc.group_layer_ids(wrapper).unwrap(), vec![1, 2, 3]);
        }
    }
}

#[test]
fn mask_group_move_reorders_and_reparents_masked_rasters_groups_and_empty_groups_without_raw_damage(
) {
    let Fixture {
        mut s,
        inner,
        hidden,
        ..
    } = fixture();
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[layer(4)],"parent_id":inner,"index":1}),
    );
    assert_eq!(s.doc.group_layer_ids(inner).unwrap(), vec![2, 4, 3]);
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[group(hidden)],"parent_id":inner,"index":0}),
    );
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[group(inner)],"parent_id":0,"index":0}),
    );
    assert_eq!(s.doc.layer_id(0), 2);
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[layer(2)],"parent_id":0,"index":2}),
    );
    assert_eq!(s.doc.layer_parent_id(s.doc.layer_index(2).unwrap()), 0);
}

#[test]
fn mask_group_partial_opacity_is_applied_once_after_masked_child_reparenting() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#FFFFFF"}));
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.}),
    );
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":layer(2),"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":91}));
    let a = create(&mut s, json!([]), None);
    let b = create(&mut s, json!([]), None);
    call(
        &mut s,
        "group.set",
        json!({"group_id":a,"opacity":173./255.}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":b,"opacity":101./255.}),
    );
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[layer(2)],"parent_id":a,"index":0}),
    );
    // Child coverage round(137*91/255)=49, then isolated opacity floor(49*173/255)=33.
    assert_eq!(s.doc.composite[0] >> 24, 33);
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[group(a)],"parent_id":b,"index":0}),
    );
    assert_eq!(s.doc.composite[0] >> 24, 13);
    outcome(&mut s);
}

#[test]
fn mask_group_integral_float_scalars_items_selection_and_explicit_root_follow_the_same_ids() {
    let Fixture {
        mut s,
        inner,
        hidden,
        ..
    } = fixture();
    let result = one_edit(
        &mut s,
        "group.create",
        json!({"members":[],"parent_id":0.0,"name":"Explicit root"}),
    );
    let root = result["group_id"].as_u64().unwrap();
    assert_ne!(root, inner);
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[{"kind":"layer","id":4.0}],"parent_id":inner as f64,"index":1.0}),
    );
    assert_eq!(s.doc.group_layer_ids(inner).unwrap(), vec![2, 4, 3]);
    call(
        &mut s,
        "group.set",
        json!({"group_id":inner as f64,"visible":false}),
    );
    assert!(!s.doc.layer_visible(s.doc.layer_index(2).unwrap()));
    call(
        &mut s,
        "group.select_layers",
        json!({"group_id":inner as f64}),
    );
    assert_eq!(
        s.doc
            .selected_layer_ids()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        [2, 3, 4].into_iter().collect()
    );
    let result = one_edit(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"group","id":inner as f64}],"parent_id":0.0}),
    );
    assert_ne!(result["group_id"], hidden);
    for (action, args) in [
        (
            "group.create",
            json!({"members":[{"kind":"layer","id":1},{"kind":"layer","id":1.0}]}),
        ),
        (
            "group.move",
            json!({"items":[group(root),{"kind":"group","id":root as f64}],"parent_id":0,"index":0}),
        ),
        ("group.create", json!({"members":[],"parent_id":null})),
        (
            "group.move",
            json!({"items":[{"kind":"layer","id":4.5}],"parent_id":0,"index":0}),
        ),
        (
            "group.set",
            json!({"group_id":inner as f64+0.5,"visible":true}),
        ),
    ] {
        let before = Before::capture(&s);
        assert!(s.apply(Action::new(action, args)).is_err());
        before.audit(&mut s, action);
    }
}

#[test]
fn mask_group_cycles_cross_parent_targets_indices_depth_count_and_instruction_limits_are_preflighted(
) {
    let Fixture { mut s, inner, .. } = fixture();
    for (action, args) in [
        (
            "group.move",
            json!({"items":[group(inner)],"parent_id":inner,"index":0}),
        ),
        (
            "group.move",
            json!({"items":[group(inner),layer(2)],"parent_id":0,"index":0}),
        ),
        (
            "group.move",
            json!({"items":[layer(4)],"parent_id":inner,"index":3}),
        ),
        ("group.create", json!({"members":[layer(1),layer(2)]})),
        (
            "group.create",
            json!({"members":[],"parent_id":4294967295u64}),
        ),
    ] {
        let before = Before::capture(&s);
        assert!(s.apply(Action::new(action, args)).is_err());
        before.audit(&mut s, action);
    }
    for depth in [false, true] {
        let mut s = session();
        call(&mut s, "canvas.new", json!({"width":1,"height":1}));
        call(
            &mut s,
            "mask.create",
            json!({"owner":layer(1),"select":false}),
        );
        let mut item = layer(1);
        for _ in 0..if depth { 8 } else { 16 } {
            let id = create(&mut s, if depth { json!([item]) } else { json!([]) }, None);
            item = group(id);
        }
        let before = Before::capture(&s);
        assert!(s
            .apply(Action::new(
                "group.create",
                json!({"members":if depth {json!([item])}else{json!([])}})
            ))
            .is_err());
        before.audit(&mut s, "group.create");
    }
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    for _ in 1..16 {
        call(&mut s, "layer.add", json!({}));
    }
    for id in 1..=16 {
        call(
            &mut s,
            "mask.create",
            json!({"owner":layer(id),"select":false}),
        );
    }
    let before = Before::capture(&s);
    let error = s
        .apply(Action::new("group.create", json!({"members":[]})))
        .unwrap_err();
    assert!(
        error.message.to_lowercase().contains("node")
            || error.message.to_lowercase().contains("instruction")
    );
    before.audit(&mut s, "group.create");
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
fn mask_group_live_brush_and_hand_rejection_preserves_all_samples_and_native_contact() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let Fixture {
            mut s,
            inner,
            target,
            ..
        } = fixture();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1,"color":"#000000","stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 0)).unwrap();
        for (i, (action, args)) in [
            ("group.create", json!({"members":[layer(1)]})),
            ("group.create_selected", json!({})),
            (
                "group.move",
                json!({"items":[layer(4)],"parent_id":inner,"index":0}),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let before = Before::capture(&s);
            let storage = match s.doc.mask_plane(target).unwrap() {
                PlaneView::Dense(v) => Some(v.as_ptr()),
                _ => None,
            };
            let last = s.stroke.last_sample().unwrap().time;
            let error = s.apply(Action::new(action, args)).unwrap_err();
            assert_eq!(error.code, "stroke_active");
            before.restored(&s);
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            assert_eq!(s.stroke.last_sample().unwrap().time, last);
            if let Some(storage) = storage {
                assert!(
                    matches!(s.doc.mask_plane(target).unwrap(),PlaneView::Dense(v)if v.as_ptr()==storage)
                );
            }
            s.pointer(pointer(kind, PointerPhase::Move, i as u32 + 1))
                .unwrap();
        }
        s.pointer(pointer(kind, PointerPhase::Up, 4)).unwrap();
        outcome(&mut s);
        let events = call(&mut s, "replay.events", json!({"limit":256}));
        let times: Vec<_> = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|event| event["source"]["samples"].as_array())
            .flatten()
            .map(|sample| sample["time"].as_u64().unwrap())
            .collect();
        assert_eq!(times, vec![0, 1, 2, 3, 4]);
    }
}

struct ControlledRenderer {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    capability: Cell<u8>,
    calls: Cell<usize>,
}
impl Renderer for ControlledRenderer {
    fn supports_masks(&self) -> bool {
        self.capability.get() != 1 && self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.capability.get() != 2 && self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.capability.get() != 3 && self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.capability.get() != 4 && self.native.supports_transparent()
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
            Err("Injected masked group composition failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
#[test]
fn mask_group_source_and_candidate_capabilities_include_hidden_disabled_masks_and_pass_through() {
    for capability in 1..=4 {
        let Fixture { mut s, inner, .. } = fixture();
        if capability == 2 {
            let enabled_group_mask = s.doc.mask_state().items[2].id;
            call(
                &mut s,
                "mask.set",
                json!({"mask_id":enabled_group_mask,"density":255,"feather_radius_px":0}),
            );
        }
        let renderer = Arc::new(ControlledRenderer {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            capability: Cell::new(capability),
            calls: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        for (action, args) in [
            ("group.create", json!({"members":[]})),
            (
                "group.move",
                json!({"items":[layer(4)],"parent_id":inner,"index":0}),
            ),
        ] {
            let before = Before::capture(&s);
            assert!(s.apply(Action::new(action, args)).is_err());
            before.audit(&mut s, action);
            assert_eq!(
                renderer.calls.get(),
                0,
                "Source capability rejection must precede native composition"
            );
        }
    }
}

#[test]
fn mask_group_render_journal_and_atomic_failures_restore_tree_allocators_raw_data_and_history() {
    for action in ["group.create", "group.move"] {
        let Fixture { mut s, inner, .. } = fixture();
        let args = if action == "group.create" {
            json!({"members":[group(inner)],"name":"Staged wrapper"})
        } else {
            json!({"items":[layer(4)],"parent_id":inner,"index":0})
        };
        let renderer = Arc::new(ControlledRenderer {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(true),
            capability: Cell::new(0),
            calls: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        let before = Before::capture(&s);
        let error = s.apply(Action::new(action, args.clone())).unwrap_err();
        assert_eq!(error.code, "group_render");
        assert!(error.message.contains("Injected masked group"));
        before.audit(&mut s, action);
        outcome(&mut s);
        let before = Before::capture(&s);
        let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        assert!(s.apply(Action::new(action, args)).is_err());
        before.restored(&s);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events);
        assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
        assert!(s.replay_error.is_some());
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
    }
    let Fixture { mut s, inner, .. } = fixture();
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"empty","action":"group.create","args":{"members":[]}},
            {"id":"move","action":"group.move","args":{"items":[layer(4)],"parent_id":inner,"index":0}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    before.audit(&mut s, "pipeline.run");
    outcome(&mut s);
}

#[test]
fn mask_group_projects_and_dll_free_outcomes_preserve_owners_while_other_group_edits_remain_guarded(
) {
    let Fixture {
        mut s,
        inner,
        hidden,
        ..
    } = fixture();
    let wrapper = create(&mut s, json!([layer(1), group(inner)]), None);
    one_edit(
        &mut s,
        "group.move",
        json!({"items":[layer(4)],"parent_id":wrapper,"index":1}),
    );
    let expected = content(&s);
    let path = output("group-owners.mcp");
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
        ("group.duplicate", json!({"group_id":inner})),
        ("group.delete", json!({"group_id":hidden})),
        ("group.ungroup", json!({"group_id":wrapper})),
        ("group.merge", json!({"group_id":inner})),
        (
            "layer.batch",
            json!({"action":"layer.set","layer_ids":[2,3],"args":{"opacity":0.5}}),
        ),
        ("layer.via_copy", json!({})),
        ("layer.translate", json!({"dx":1,"dy":0})),
        ("clipboard.paste", json!({})),
    ] {
        let before = Before::capture(&s);
        let error = s.apply(Action::new(action, args)).unwrap_err();
        assert_eq!(error.code, "mask_editing_pending", "{action}: {error}");
        before.audit(&mut s, action);
    }
}

#[test]
fn mask_group_required_result_ids_bind_workflow_reparenting_without_executing_drafts() {
    let Fixture { mut s, .. } = fixture();
    for id in ["group.create", "group.create_selected"] {
        let schema = s.registry.actions[id].output_schema.as_ref().unwrap();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("group_id")));
    }
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.group.create","position":[0,0],"parameters":{"members":[],"name":"Bound parent","parent_id":0}},
        {"id":2,"type":"action.group.move","position":[360,0],"parameters":{"items":[layer(4)],"index":0}}
    ],"edges":[{"from":1,"output":"group_id","to":2,"input":"parent_id"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    let before = Before::capture(&s);
    let preserved = Preserved::capture(&s);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    before.restored(&s);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events + 1);
    let dry = s
        .journal
        .as_mut()
        .unwrap()
        .events(before.events, 1)
        .unwrap();
    assert_eq!(dry["events"][0]["success"], true);
    assert_eq!(dry["events"][0]["changed_mask_tiles"], 0);
    let before = Before::capture(&s);
    let parent = s.doc.composition().next_group_id;
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.layer_parent_id(s.doc.layer_index(4).unwrap()), parent);
    assert_eq!(s.revision, before.revision + 2);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events + 1);
    assert_eq!(
        s.journal
            .as_mut()
            .unwrap()
            .events(before.events, 1)
            .unwrap()["events"][0]["changed_mask_tiles"],
        0
    );
    preserved.unchanged(&s);
    outcome(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layer_parent_id(s.doc.layer_index(4).unwrap()), 0);
    call(&mut s, "history.undo", json!({}));
    let mut expected = before.content;
    expected.composition.next_group_id = s.doc.composition().next_group_id;
    assert_eq!(content(&s), expected);
}
