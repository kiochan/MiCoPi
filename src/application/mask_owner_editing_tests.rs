//! Single-owner structure changes share raw-mask history and verified outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    graph::{Edge, Filter, Node, NodeKind, Renderer},
    raster_masks::{MaskMetadata, MaskOwner, MaskState, PaintTarget, PlaneView, SamplePlane},
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
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-owner-editing");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn samples(s: &Session, id: u64) -> Vec<u8> {
    let view = s.doc.mask_plane(id).unwrap();
    (0..s.doc.width * s.doc.height)
        .map(|i| view.sample(i))
        .collect()
}
fn raw(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| (m.id, samples(s, m.id)))
        .collect()
}
#[derive(Clone, Debug, PartialEq)]
struct Content {
    active: usize,
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
}
fn content(s: &Session) -> Content {
    Content {
        active: s.doc.active,
        composition: s.doc.composition(),
        pixels: s.doc.layers.iter().map(|l| l.pixels.clone()).collect(),
        masks: raw(s),
    }
}
impl Content {
    fn assert_after_undo(&self, s: &Session) {
        let mut expected = self.clone();
        let actual = content(s);
        // Committed Undo never reuses layer or mask identities.
        expected.composition.next_layer_id = actual.composition.next_layer_id;
        expected.composition.raster_masks.next_id = actual.composition.raster_masks.next_id;
        assert_eq!(actual, expected);
    }
}
struct Before {
    content: Content,
    public: Value,
    surface: Vec<u32>,
    revision: u64,
    history: usize,
    events: usize,
}
impl Before {
    fn capture(s: &Session) -> Self {
        Self {
            content: content(s),
            public: s.state(),
            surface: s.doc.composite.clone(),
            revision: s.revision,
            history: s.doc.history_bytes(),
            events: s.journal.as_ref().unwrap().entries.len(),
        }
    }
    fn restored(&self, s: &Session) {
        self.restored_state(s);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), self.events);
    }
    fn restored_state(&self, s: &Session) {
        assert_eq!(content(s), self.content);
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.revision, self.revision);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert!(!s.doc.history_group_active());
    }
    fn audited_no_change(&self, s: &mut Session, action: &str, success: bool) {
        self.restored_state(s);
        let journal = s.journal.as_mut().unwrap();
        assert_eq!(journal.entries.len(), self.events + 1);
        let events = journal.events(self.events, 1).unwrap();
        let event = &events["events"][0];
        assert_eq!(event["source"]["action"], action);
        assert_eq!(event["success"], success);
        assert_eq!(event["visual"], false);
        assert_eq!(event["changed_tiles"], 0);
        assert_eq!(event["changed_mask_tiles"], 0);
        assert_eq!(event["before"], event["after"]);
        assert_eq!(event["error"].is_null(), success);
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
    for (id, expected) in raw(s) {
        assert_eq!(
            (0..expected.len())
                .map(|i| restored.mask_plane(id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
struct Fixture {
    s: Session,
    paper: u64,
    source: Option<u64>,
    group: u64,
}
fn fixture(uniform: bool, owned: bool) -> Fixture {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":5,"height":3}));
    for (i, color) in ["#3D4A70", "#D2B5C8", "#8DD8C4"].iter().enumerate() {
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
    let group_id = call(
        &mut s,
        "group.create",
        json!({"name":"Cel folder","members":[{"kind":"layer","id":2},{"kind":"layer","id":3}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "group.set",
        json!({"group_id":group_id,"opacity":193./255.}),
    );
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|n| matches!(n.data, NodeKind::Layer { index: 1 }))
        .unwrap()
        .id;
    let filter = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = filter;
        }
    }
    graph.nodes.push(Node {
        id: filter,
        node_type: None,
        position: [0., 0.],
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 0.35,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: filter,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    let paper = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    let source = owned.then(|| {
        call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":"layer","id":2},"select":false}),
        )["mask_id"]
            .as_u64()
            .unwrap()
    });
    let group = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"group","id":group_id},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    s.doc
        .replace_mask_plane(
            paper,
            SamplePlane::Dense((0..15).map(|i| (i * 29 + 11) as u8).collect()),
        )
        .unwrap();
    s.doc
        .replace_mask_plane(
            group,
            SamplePlane::Dense((0..15).map(|i| (i * 13 + 73) as u8).collect()),
        )
        .unwrap();
    if let Some(id) = source {
        s.doc
            .replace_mask_plane(
                id,
                if uniform {
                    SamplePlane::Uniform(91)
                } else {
                    SamplePlane::Dense((0..15).map(|i| (i * 37 + 7) as u8).collect())
                },
            )
            .unwrap();
    }
    s.replay_boundary().unwrap();
    if let Some(id) = source {
        call(
            &mut s,
            "mask.set",
            json!({"mask_id":id,"enabled":false,"linked":false,"density":173,"feather_radius_px":1}),
        );
    }
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":group,"density":203,"feather_radius_px":2,"linked":false}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.,"alpha_locked":true,"pixel_locked":true,"position_locked":true,"color_label":"violet"}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":2}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":source.unwrap_or(group)}}),
    );
    s.doc.compose();
    outcome(&mut s);
    Fixture {
        s,
        paper,
        source,
        group,
    }
}

#[test]
fn mask_owner_duplicate_copies_dense_uniform_properties_filters_and_undo_once() {
    for uniform in [false, true] {
        let Fixture {
            mut s,
            paper,
            source,
            group,
        } = fixture(uniform, true);
        if uniform {
            call(&mut s, "layer.set", json!({"layer_id":2,"visible":false}));
        }
        let source = source.unwrap();
        let before = content(&s);
        let source_metadata = s
            .doc
            .mask_state()
            .items
            .iter()
            .find(|m| m.id == source)
            .unwrap()
            .clone();
        let retained: Vec<_> = (0..s.doc.layers.len())
            .map(|i| (s.doc.layer_id(i), s.doc.layers[i].pixels.as_ptr()))
            .collect();
        let retained_mask: Vec<_> = [paper, group]
            .into_iter()
            .map(|id| {
                (
                    id,
                    match s.doc.mask_plane(id).unwrap() {
                        PlaneView::Dense(v) => v.as_ptr(),
                        _ => panic!("Fixture must be dense"),
                    },
                )
            })
            .collect();
        let events = s.journal.as_ref().unwrap().entries.len();
        let revision = s.revision;
        let result = call(&mut s, "layer.duplicate", json!({"layer_id":2}));
        let copied = s.doc.layer_id(s.doc.active);
        assert_eq!(result["source_layer_id"], 2);
        assert_eq!(result["layer_id"], copied);
        assert_ne!(copied, 2);
        assert_eq!(s.doc.selected_layer_ids(), vec![copied]);
        assert_eq!(s.doc.layers[s.doc.active].pixels, before.pixels[1]);
        assert_eq!(
            s.doc.layers[s.doc.active].visible,
            before.composition.layers[1].0
        );
        assert!(
            s.doc.pixel_locked(s.doc.active)
                && s.doc.position_locked(s.doc.active)
                && s.doc.alpha_locked(s.doc.active)
        );
        assert_eq!(s.doc.layers[s.doc.active].opacity, 137);
        let mask = s
            .doc
            .mask_state()
            .items
            .iter()
            .find(|m| m.owner == MaskOwner::Layer { id: copied })
            .unwrap()
            .clone();
        assert_eq!(result["mask_ids"], json!([mask.id]));
        let mut expected_metadata = source_metadata;
        expected_metadata.id = mask.id;
        expected_metadata.owner = MaskOwner::Layer { id: copied };
        assert_eq!(mask, expected_metadata);
        assert_eq!(samples(&s, mask.id), before.masks[&source]);
        assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: mask.id });
        assert_eq!(
            matches!(s.doc.mask_plane(mask.id).unwrap(), PlaneView::Uniform(_)),
            uniform
        );
        if !uniform {
            assert!(
                matches!((s.doc.mask_plane(mask.id).unwrap(),s.doc.mask_plane(source).unwrap()),(PlaneView::Dense(a),PlaneView::Dense(b))if a.as_ptr()!=b.as_ptr())
            );
        }
        for (id, pointer) in retained {
            assert_eq!(
                s.doc.layers[s.doc.layer_index(id).unwrap()].pixels.as_ptr(),
                pointer
            );
        }
        for (id, pointer) in retained_mask {
            assert!(
                matches!(s.doc.mask_plane(id).unwrap(),PlaneView::Dense(data)if data.as_ptr()==pointer)
            );
        }
        let filters = s
            .doc
            .graph
            .nodes
            .iter()
            .filter(|n| matches!(n.data, NodeKind::Filter { .. }))
            .count();
        assert_eq!(filters, 2);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        let after = content(&s);
        outcome(&mut s);
        call(&mut s, "history.undo", json!({}));
        before.assert_after_undo(&s);
        outcome(&mut s);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), after);
        outcome(&mut s);
        call(&mut s, "mask.fill", json!({"mask_id":mask.id,"value":19}));
        assert_eq!(samples(&s, source), before.masks[&source]);
        assert!(samples(&s, mask.id).iter().all(|v| *v == 19));
    }
}

#[test]
fn mask_owner_duplicate_preserves_pixels_and_unrelated_targets_without_widening_selection() {
    for owned in [false, true] {
        for target in 0..3 {
            let Fixture {
                mut s,
                paper,
                group,
                ..
            } = fixture(false, owned);
            let target = match target {
                0 => PaintTarget::LayerPixels,
                1 => PaintTarget::Mask { id: paper },
                _ => PaintTarget::Mask { id: group },
            };
            call(&mut s, "mask.target", json!({"target":target}));
            let before = content(&s);
            let count = s.doc.mask_state().items.len();
            let next = s.doc.mask_state().next_id;
            call(&mut s, "layer.duplicate", json!({"layer_id":2}));
            assert_eq!(s.doc.layers.len(), 4);
            assert_eq!(s.doc.mask_state().items.len(), count + usize::from(owned));
            assert_eq!(s.doc.mask_state().next_id, next + u64::from(owned));
            assert_eq!(s.doc.mask_state().target, target);
            assert_eq!(
                s.doc.selected_layer_ids(),
                vec![s.doc.layer_id(s.doc.active)]
            );
            assert_eq!(samples(&s, paper), before.masks[&paper]);
            assert_eq!(samples(&s, group), before.masks[&group]);
            outcome(&mut s);
        }
    }
}

#[test]
fn mask_owner_duplicate_compiles_copied_opacity_in_its_existing_group() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#804020"}));
    call(&mut s, "layer.set", json!({"opacity":137./255.}));
    call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}]}),
    );
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":2},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":91}));
    call(&mut s, "layer.duplicate", json!({"layer_id":2}));
    // round(137*91/255)=49; Normal over of two alpha49 children yields89.
    // A candidate-plan opacity fallback of255 produces a different result.
    s.doc.compose();
    assert_eq!(s.doc.composite[0] >> 24, 89);
    outcome(&mut s);
}

#[test]
fn mask_owner_delete_removes_only_owned_plane_preserves_targets_and_restores_one_edit() {
    for target in 0..4 {
        let Fixture {
            mut s,
            paper,
            source,
            group,
        } = fixture(false, true);
        let source = source.unwrap();
        let target = match target {
            0 => PaintTarget::LayerPixels,
            1 => PaintTarget::Mask { id: source },
            2 => PaintTarget::Mask { id: paper },
            _ => PaintTarget::Mask { id: group },
        };
        call(&mut s, "mask.target", json!({"target":target}));
        let before = content(&s);
        let pointers: Vec<_> = [1, 3]
            .into_iter()
            .map(|id| {
                (
                    id,
                    s.doc.layers[s.doc.layer_index(id).unwrap()].pixels.as_ptr(),
                )
            })
            .collect();
        let events = s.journal.as_ref().unwrap().entries.len();
        let revision = s.revision;
        let result = call(&mut s, "layer.delete", json!({"layer_id":2}));
        assert_eq!(result["deleted_layer_id"], 2);
        assert_eq!(result["deleted_mask_ids"], json!([source]));
        assert!(s.doc.layer_index(2).is_none());
        assert!(s.doc.mask_plane(source).is_err());
        assert_eq!(s.doc.mask_state().items.len(), 2);
        assert_eq!(
            s.doc.mask_state().target,
            if target == (PaintTarget::Mask { id: source }) {
                PaintTarget::LayerPixels
            } else {
                target
            }
        );
        for (id, pointer) in pointers {
            assert_eq!(
                s.doc.layers[s.doc.layer_index(id).unwrap()].pixels.as_ptr(),
                pointer
            );
        }
        assert_eq!(samples(&s, paper), before.masks[&paper]);
        assert_eq!(samples(&s, group), before.masks[&group]);
        assert_eq!(s.doc.composition().groups.as_ref().unwrap().groups.len(), 1);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        let after = content(&s);
        outcome(&mut s);
        call(&mut s, "history.undo", json!({}));
        before.assert_after_undo(&s);
        outcome(&mut s);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), after);
        outcome(&mut s);
    }
    let Fixture { mut s, group, .. } = fixture(false, false);
    let masks = raw(&s);
    let result = call(&mut s, "layer.delete", json!({"layer_id":2}));
    assert_eq!(result["deleted_mask_ids"], json!([]));
    assert_eq!(raw(&s), masks);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: group });
    outcome(&mut s);
}

#[test]
fn mask_owner_allocators_remain_monotonic_on_undo_and_exact_on_atomic_failure() {
    let Fixture { mut s, .. } = fixture(false, true);
    let before = content(&s);
    call(&mut s, "layer.duplicate", json!({"layer_id":2}));
    let old_layer = s.doc.layer_id(s.doc.active);
    let old_mask = s.doc.mask_state().items.last().unwrap().id;
    call(&mut s, "history.undo", json!({}));
    before.assert_after_undo(&s);
    call(&mut s, "layer.duplicate", json!({"layer_id":2}));
    assert!(s.doc.layer_id(s.doc.active) > old_layer);
    assert!(s.doc.mask_state().items.last().unwrap().id > old_mask);
    let before = Before::capture(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"copy","action":"layer.duplicate","args":{"layer_id":2}},
            {"id":"remove","action":"layer.delete","args":{"layer_id":1}},
            {"id":"fail","action":"mask.delete","args":{"mask_id":4294967295u64}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    before.audited_no_change(&mut s, "pipeline.run", false);
    outcome(&mut s);
}

fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 1. + time as f32 * 0.2,
        y: 1.,
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
fn mask_owner_live_brush_and_hand_rejections_preserve_cached_contact_and_samples() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let Fixture { mut s, source, .. } = fixture(false, true);
        call(
            &mut s,
            "brush.set",
            json!({"radius":1,"color":"#000000","stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 0)).unwrap();
        let source = source.unwrap();
        for (i, action) in ["layer.duplicate", "layer.delete"].into_iter().enumerate() {
            let before = Before::capture(&s);
            let last = s.stroke.last_sample().unwrap().time;
            let storage = match s.doc.mask_plane(source).unwrap() {
                PlaneView::Dense(data) => Some(data.as_ptr()),
                _ => None,
            };
            let error = s
                .apply(Action::new(action, json!({"layer_id":2})))
                .unwrap_err();
            assert_eq!(error.code, "stroke_active");
            before.restored_state(&s);
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            assert_eq!(s.stroke.last_sample().unwrap().time, last);
            if let Some(pointer) = storage {
                assert!(
                    matches!(s.doc.mask_plane(source).unwrap(),PlaneView::Dense(data)if data.as_ptr()==pointer)
                );
            }
            s.pointer(pointer(kind, PointerPhase::Move, i as u32 + 1))
                .unwrap();
        }
        s.pointer(pointer(kind, PointerPhase::Up, 3)).unwrap();
        assert!(!s.stroke.active());
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
        assert_eq!(times, vec![0, 1, 2, 3]);
    }
}

#[test]
fn mask_owner_exhausted_ids_and_layer_capacity_reject_without_state_or_history_changes() {
    for masks in [false, true] {
        let Fixture { mut s, .. } = fixture(true, true);
        let mut composition = s.doc.composition();
        if masks {
            composition.raster_masks.next_id = u32::MAX as u64 + 1;
        } else {
            composition.next_layer_id = u32::MAX as u64 + 1;
        }
        s.doc.restore_composition(composition);
        s.replay_boundary().unwrap();
        let before = Before::capture(&s);
        assert!(s
            .apply(Action::new("layer.duplicate", json!({"layer_id":2})))
            .is_err());
        before.audited_no_change(&mut s, "layer.duplicate", false);
    }
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    for _ in 1..16 {
        call(&mut s, "layer.add", json!({}));
    }
    call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"select":false}),
    );
    let before = Before::capture(&s);
    assert!(s
        .apply(Action::new("layer.duplicate", json!({"layer_id":1})))
        .is_err());
    before.audited_no_change(&mut s, "layer.duplicate", false);
}

struct FailRender {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    failures: Cell<usize>,
}
impl Renderer for FailRender {
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.native.supports_blend_mode(mode)
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
        if self.armed.replace(false) {
            self.failures.set(self.failures.get() + 1);
            unsafe { o.data.write(0xdeadc0de) };
            Err("Injected owner-structure renderer failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
#[test]
fn mask_owner_renderer_and_partial_journal_failure_restore_ownership_allocator_and_history() {
    for action in ["layer.duplicate", "layer.delete"] {
        let Fixture { mut s, .. } = fixture(false, true);
        let renderer = Arc::new(FailRender {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            failures: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        let before = Before::capture(&s);
        renderer.armed.set(true);
        let error = s
            .apply(Action::new(action, json!({"layer_id":2})))
            .unwrap_err();
        assert_eq!(error.code, "render_error");
        assert_eq!(renderer.failures.get(), 1);
        before.audited_no_change(&mut s, action, false);
        outcome(&mut s);
        let before = Before::capture(&s);
        let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        assert!(s.apply(Action::new(action, json!({"layer_id":2}))).is_err());
        before.restored(&s);
        assert!(s.replay_error.is_some());
        assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
    }
}

#[test]
fn mask_owner_structural_pipeline_and_workflow_remain_inert_until_execution() {
    let Fixture { mut s, .. } = fixture(false, true);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.layer.duplicate","position":[0,0],"parameters":{"layer_id":2}},
        {"id":2,"type":"action.layer.rename","position":[360,0],"parameters":{"name":"Workflow copy"}},
        {"id":3,"type":"action.layer.delete","position":[720,0],"parameters":{"layer_id":1}}
    ],"edges":[{"from":1,"output":"layer_id","to":2,"input":"layer_id"},
        {"from":2,"output":"$exec","to":3,"input":"$exec"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    let before = Before::capture(&s);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"dry_run":true,"atomic":true})
        )["ok"],
        true
    );
    before.audited_no_change(&mut s, "workflow.run", true);
    let before = Before::capture(&s);
    let result = call(&mut s, "workflow.run", json!({"atomic":true}));
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.layers.len(), 3);
    assert!(s.doc.layer_index(1).is_none());
    assert_eq!(s.revision, before.revision + 3);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events + 1);
    assert_eq!(s.doc.custom_layer_name(s.doc.active), "Workflow copy");
    outcome(&mut s);
    call(&mut s, "history.undo", json!({}));
    // Atomic pipelines provide rollback, not a substitute for grouped Undo.
    assert_eq!(s.doc.layers.len(), 4);
    assert!(s.doc.layer_index(1).is_some());
    let next = s.doc.composition().next_layer_id;
    let next_mask = s.doc.mask_state().next_id;
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"copy","action":"layer.duplicate","args":{"layer_id":2}},
            {"id":"rename","action":"layer.rename","args":{"layer_id":{"$ref":"copy#/result/layer_id"},"name":"Bound copy"}},
            {"id":"fill","action":"mask.fill","args":{"mask_id":{"$ref":"copy#/result/mask_ids/0"},"value":33}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(
        s.doc.custom_layer_name(s.doc.layer_index(next).unwrap()),
        "Bound copy"
    );
    assert!(samples(&s, next_mask).iter().all(|value| *value == 33));
    outcome(&mut s);
}

#[test]
fn mask_owner_projects_and_archives_roundtrip_and_other_edit_boundaries_stay_guarded() {
    let Fixture { mut s, .. } = fixture(false, true);
    call(&mut s, "layer.duplicate", json!({"layer_id":2}));
    call(&mut s, "layer.delete", json!({"layer_id":1}));
    let expected = content(&s);
    let path = output("owners.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(content(&opened), expected);
    outcome(&mut opened);
    let mut imported = Journal::import(&archive).unwrap();
    assert_eq!(imported.verify(None).unwrap()["valid"], true);
    let recorded = imported.preview(imported.entries.len() - 1).unwrap();
    assert_eq!(replay::png(&recorded).unwrap(), png);
    let mut restored = Document::new(1, 1);
    recorded.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    for (id, data) in raw(&s) {
        assert_eq!(
            (0..data.len())
                .map(|i| restored.mask_plane(id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            data
        );
    }
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    for (action, args) in [
        (
            "layer.batch",
            json!({"action":"layer.duplicate","layer_ids":[2,3]}),
        ),
        ("layer.via_copy", json!({})),
        ("layer.translate", json!({"dx":1,"dy":0})),
        ("layer.merge_selected", json!({"layer_ids":[2,3]})),
        ("group.duplicate", json!({"group_id":1})),
        ("clipboard.paste", json!({})),
    ] {
        let before = Before::capture(&s);
        let error = s.apply(Action::new(action, args)).unwrap_err();
        assert_eq!(error.code, "mask_editing_pending", "{action}: {error}");
        before.audited_no_change(&mut s, action, false);
    }
}

#[test]
fn mask_owner_uniform_planes_use_the_full_scalar_logical_budget() {
    // A hypothetical one-raster/three-group document fits the four-plane limit
    // at 4096 square. The raw planes remain scalar constants; no raster or
    // 4096-square sample buffer is allocated for this boundary check.
    let mut state = MaskState {
        items: vec![
            MaskMetadata::new(1, MaskOwner::Layer { id: 1 }),
            MaskMetadata::new(2, MaskOwner::Group { id: 1 }),
            MaskMetadata::new(3, MaskOwner::Group { id: 2 }),
            MaskMetadata::new(4, MaskOwner::Group { id: 3 }),
        ],
        next_id: 5,
        ..Default::default()
    };
    let planes: [SamplePlane; 4] = std::array::from_fn(|_| SamplePlane::Uniform(255));
    state.validate_owners(&[1], &[1, 2, 3, 4]).unwrap();
    state.validate_dimensions(4096, 4096).unwrap();
    state
        .items
        .push(MaskMetadata::new(5, MaskOwner::Group { id: 4 }));
    state.next_id = 6;
    state.validate_owners(&[1], &[1, 2, 3, 4]).unwrap();
    assert!(state
        .validate_dimensions(4096, 4096)
        .unwrap_err()
        .contains("logical"));
    assert!(planes
        .iter()
        .all(|plane| matches!(plane, SamplePlane::Uniform(255))));
}

#[test]
fn mask_owner_integer_selectors_honor_json_float_numbers_instead_of_the_active_layer() {
    let Fixture { mut s, .. } = fixture(false, true);
    call(&mut s, "layer.select", json!({"layer_id":3}));
    assert_eq!(s.doc.layer_id(s.doc.active), 3);
    let result = call(
        &mut s,
        "layer.duplicate",
        json!({"layer_id":2.0,"index":1.0}),
    );
    assert_eq!(result["source_layer_id"], 2);
    assert_eq!(result["layer_id"], 4);
    assert_eq!(result["mask_ids"].as_array().unwrap().len(), 1);
    call(&mut s, "layer.delete", json!({"layer_id":1.0,"index":0.0}));
    assert!(s.doc.layer_index(1).is_none());
    assert!(s.doc.layer_index(4).is_some());
    call(&mut s, "layer.delete", json!({"index":0.0}));
    assert!(s.doc.layer_index(2).is_none());
    assert!(s.doc.layer_index(4).is_some());
    call(
        &mut s,
        "layer.rename",
        json!({"layer_id":3.0,"name":"Numeric target"}),
    );
    assert_eq!(
        s.doc.custom_layer_name(s.doc.layer_index(3).unwrap()),
        "Numeric target"
    );
    assert_ne!(
        s.doc.custom_layer_name(s.doc.layer_index(4).unwrap()),
        "Numeric target"
    );
    call(&mut s, "layer.set", json!({"layer_id":3.0,"opacity":0.5}));
    assert_eq!(s.doc.layers[s.doc.layer_index(3).unwrap()].opacity, 128);
    outcome(&mut s);

    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.add", json!({}));
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2,3],"active_layer_id":2.0}),
    );
    assert_eq!(s.doc.layer_id(s.doc.active), 2);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[3.0,1.0],"active_layer_id":1.0}),
    );
    assert_eq!(s.doc.layer_id(s.doc.active), 1);
    assert_eq!(s.doc.selected_layer_ids(), vec![1, 3]);
    call(&mut s, "layer.move", json!({"index":2.0,"to":0.0}));
    assert_eq!(s.doc.layer_id(0), 3);
    call(&mut s, "layer.select", json!({"index":1.0}));
    assert_eq!(s.doc.layer_id(s.doc.active), 1);
    let group = call(&mut s, "group.create", json!({"members":[]}))["group_id"]
        .as_u64()
        .unwrap();
    let added = call(&mut s, "layer.add", json!({"parent_id":group as f64}));
    let composition = s.doc.composition();
    let tree = composition.groups.as_ref().unwrap();
    assert_eq!(
        tree.parent(crate::engine::groups::GroupItem::layer(
            added["layer_id"].as_u64().unwrap()
        ))
        .unwrap(),
        group
    );
    let root_layer = call(&mut s, "layer.add", json!({"parent_id":0.0}))["layer_id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        s.doc
            .composition()
            .groups
            .as_ref()
            .unwrap()
            .parent(crate::engine::groups::GroupItem::layer(root_layer))
            .unwrap(),
        0
    );
}
