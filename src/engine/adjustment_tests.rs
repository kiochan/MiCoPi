//! Small independent affect-below, ownership and compatibility witnesses.
use super::*;
use crate::engine::{
    graph::{BlendMode, Filter, Renderer},
    groups::{GroupItem, GroupPolicy},
    raster_masks::{MaskOwner, PaintTarget, PlaneView},
};

fn fixture(width: usize) -> Document {
    let mut doc = Document::new(width, 1);
    doc.renderer = Some(crate::plugins::Registry::bundled().renderer().unwrap());
    doc.layers[0].pixels.fill(0x80402010);
    doc.invalidate();
    doc.compose();
    doc
}
fn invert(opacity: u8) -> AdjustmentSpec {
    AdjustmentSpec {
        name: "Ink adjustment".into(),
        visible: true,
        opacity,
        node_type: "compositing.filter".into(),
        filter: Filter::Invert,
        amount: 1.,
        levels: None,
        curves: None,
        hsl: None,
        quantization: None,
        target_layer_ids: None,
    }
}
fn raw(doc: &Document, id: u64) -> Vec<u8> {
    (0..doc.width * doc.height)
        .map(|i| doc.mask_plane(id).unwrap().sample(i))
        .collect()
}

#[test]
fn adjustment_normal_is_alpha_preserving_affect_below_not_source_over() {
    for (mask, expected) in [
        (None, 0x80808080),
        (Some(SamplePlane::Uniform(128)), 0x80605048),
    ] {
        let mut doc = fixture(1);
        let pointer = doc.layers[0].pixels.as_ptr();
        let before = doc.composition();
        let history = doc.history_bytes();
        let (id, mask_id) = doc.create_adjustment(invert(128), 0, 1, mask).unwrap();
        doc.compose();
        assert_eq!(doc.composite, [expected]);
        assert_eq!(doc.layers.len(), 1);
        assert_eq!(doc.layers[0].pixels, [0x80402010]);
        assert_eq!(doc.layers[0].pixels.as_ptr(), pointer);
        assert_eq!(doc.active, 0);
        assert!(doc.selected_layer_ids().contains(&1));
        assert_eq!(doc.adjustment(id).unwrap().filter, Filter::Invert);
        assert_eq!(mask_id.is_some(), doc.has_raster_masks());
        assert!(doc.history_bytes() > history);
        doc.undo();
        doc.compose();
        assert_eq!(doc.composite, [0x80402010]);
        assert!(doc.adjustments().is_empty());
        assert_eq!(doc.composition().graph, before.graph);
        assert_eq!(doc.layers[0].pixels.as_ptr(), pointer);
        doc.redo();
        doc.compose();
        assert_eq!(doc.composite, [expected]);
    }
}

#[test]
fn adjustment_does_not_filter_later_siblings_and_pass_through_uses_backdrop_once() {
    let mut doc = fixture(1);
    doc.try_add_layer().unwrap();
    doc.layers[1].pixels[0] = 0xff336699;
    doc.create_adjustment(invert(255), 0, 1, None).unwrap();
    doc.compose();
    assert_eq!(doc.composite, [0xff336699]);
    let mut doc = fixture(1);
    let group = doc
        .group_create(&[], Some("Affect below".into()), Some(0))
        .unwrap();
    doc.create_adjustment(invert(255), group, 0, None).unwrap();
    doc.compose();
    assert_eq!(
        doc.composite,
        [0x80402010],
        "isolated empty backdrop remains transparent"
    );
    doc.group_set(
        group,
        None,
        None,
        Some(128),
        None,
        Some(GroupPolicy::PassThrough),
    )
    .unwrap();
    doc.compose();
    assert_eq!(
        doc.composite,
        [0x80808080],
        "group fades the complete backdrop change once"
    );
    assert_eq!(
        doc.composition().groups.unwrap().version,
        3,
        "Pass Through cannot downgrade v3"
    );
}

#[test]
fn adjustment_graph_disable_bypasses_effects_but_retains_group_boundary() {
    let mut doc = fixture(1);
    doc.try_add_layer().unwrap();
    doc.layers[1].pixels[0] = 0xff6080a0;
    let group = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    doc.group_set(group, None, None, Some(128), None, None)
        .unwrap();
    let id = doc
        .create_adjustment(invert(255), group, 1, None)
        .unwrap()
        .0;
    doc.compose();
    let adjusted = doc.composite.clone();
    let mut graph = doc.graph.clone();
    graph.enabled = false;
    doc.set_graph(graph).unwrap();
    doc.compose();
    assert_ne!(doc.composite, adjusted);
    assert_eq!(doc.composite, [over(0x80402010, 0x806080a0)]);
    assert!(doc.adjustment(id).is_ok());
    assert!(doc.has_groups());
}

#[test]
fn adjustment_first_undo_retains_strict_floor_but_exact_checkpoint_restore_does_not() {
    let mut doc = fixture(1);
    let original = doc.composition();
    let checkpoint = doc.checkpoint_history();
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("groups").is_none());
    let (id, mask) = doc
        .create_adjustment(invert(255), 0, 1, Some(SamplePlane::Uniform(173)))
        .unwrap();
    assert_eq!((id, mask), (1, Some(1)));
    doc.undo();
    let tree = doc.composition().groups.unwrap();
    assert_eq!(tree.version, 3);
    assert_eq!(tree.next_adjustment_id, 2);
    assert!(tree.adjustments.is_empty() && tree.groups.is_empty());
    assert_eq!(tree.roots, [GroupItem::layer(1)]);
    assert_eq!(doc.mask_state().version, 2);
    assert_eq!(doc.mask_state().next_id, 2);
    assert!(doc.mask_state().items.is_empty());
    let (next, next_mask) = doc
        .create_adjustment(invert(255), 0, 1, Some(SamplePlane::Uniform(128)))
        .unwrap();
    assert_eq!((next, next_mask), (2, Some(2)));
    doc.undo();
    doc.restore_composition(original.clone());
    doc.restore_history(checkpoint);
    assert_eq!(doc.composition(), original);
    assert_eq!(doc.next_adjustment_id, 0);
    assert!(!doc.mask_storage_used());
    let fresh = doc.create_adjustment(invert(255), 0, 1, None).unwrap();
    assert_eq!(fresh, (1, None));
}

#[test]
fn adjustment_import_rejects_unknown_or_orphaned_items_and_never_used_tree_bytes_stay_legacy() {
    let mut tree = GroupTree::flat(&[1]);
    tree.groups.push(crate::engine::groups::Group {
        id: 1,
        name: "Group".into(),
        visible: true,
        opacity: 255,
        blend_mode: BlendMode::Normal,
        compositing: GroupPolicy::Isolated,
        children: vec![],
    });
    tree.roots.push(GroupItem::group(1));
    let wire = serde_json::to_value(&tree).unwrap();
    assert!(wire.get("adjustments").is_none());
    assert!(wire.get("next_adjustment_id").is_none());
    tree.version = 3;
    tree.next_adjustment_id = 2;
    tree.adjustments.push(Adjustment::new(1, invert(255)));
    assert!(tree.validate(&[1], 2).is_err());
    tree.roots.push(GroupItem::adjustment(1));
    tree.validate(&[1], 2).unwrap();
    tree.version = 2;
    assert!(tree.validate(&[1], 2).is_err());
    tree.version = 3;
    tree.roots.push(GroupItem::adjustment(1));
    assert!(tree.validate(&[1], 2).is_err());
    tree.roots.pop();
    tree.adjustments[0].amount = f32::NAN;
    assert!(tree.validate(&[1], 2).is_err());
}

#[test]
fn adjustment_subtree_copy_delete_owns_independent_masks_and_preserves_raster_projection() {
    let mut doc = fixture(3);
    doc.try_add_layer().unwrap();
    doc.layers[1].pixels.fill(0xff667788);
    let group = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    let empty = doc.group_create(&[], None, Some(group)).unwrap();
    let (a, mask) = doc
        .create_adjustment(
            invert(128),
            group,
            1,
            Some(SamplePlane::Dense(vec![17, 33, 99])),
        )
        .unwrap();
    let (b, other) = doc
        .create_adjustment(invert(255), empty, 0, Some(SamplePlane::Uniform(91)))
        .unwrap();
    let mask = mask.unwrap();
    let other = other.unwrap();
    let before_pixels = doc.layers[0].pixels.as_ptr();
    let source_pointer = match doc.mask_plane(mask).unwrap() {
        PlaneView::Dense(p) => p.as_ptr(),
        _ => panic!("dense source"),
    };
    let mut metadata = doc
        .mask_state()
        .items
        .iter()
        .find(|m| m.id == mask)
        .unwrap()
        .clone();
    metadata.enabled = false;
    metadata.linked = false;
    doc.update_mask_metadata(metadata).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: mask }).unwrap();
    let copy = doc.group_duplicate(group).unwrap();
    assert_eq!(copy, 3);
    assert_eq!(
        doc.adjustments().iter().map(|a| a.id).collect::<Vec<_>>(),
        [a, b, 3, 4]
    );
    assert_eq!(doc.mask_state().target, PaintTarget::Mask { id: 3 });
    assert_eq!(raw(&doc, 3), [17, 33, 99]);
    assert_eq!(raw(&doc, 4), [91; 3]);
    assert_eq!(
        doc.mask_state()
            .items
            .iter()
            .find(|m| m.id == 3)
            .unwrap()
            .owner,
        MaskOwner::Adjustment { id: 3 }
    );
    assert_ne!(
        match doc.mask_plane(3).unwrap() {
            PlaneView::Dense(p) => p.as_ptr(),
            _ => panic!("dense copy"),
        },
        source_pointer
    );
    assert_eq!(doc.layers[0].pixels.as_ptr(), before_pixels);
    assert_eq!(
        doc.composition()
            .groups
            .as_ref()
            .unwrap()
            .layer_ids()
            .unwrap(),
        [1, 2, 3]
    );
    doc.group_delete(group).unwrap();
    assert_eq!(
        doc.adjustments().iter().map(|a| a.id).collect::<Vec<_>>(),
        [3, 4]
    );
    assert!(doc.mask_plane(mask).is_err() && doc.mask_plane(other).is_err());
    assert_eq!(doc.mask_state().target, PaintTarget::Mask { id: 3 });
    doc.undo();
    assert_eq!(raw(&doc, mask), [17, 33, 99]);
    assert_eq!(raw(&doc, other), [91; 3]);
    doc.undo();
    assert_eq!(
        doc.adjustments().iter().map(|a| a.id).collect::<Vec<_>>(),
        [a, b]
    );
    assert!(doc.mask_plane(3).is_err());
    assert_eq!(doc.composition().groups.unwrap().next_adjustment_id, 5);
    doc.redo();
    assert_eq!(raw(&doc, 3), [17, 33, 99]);
}

#[test]
fn adjustment_single_raster_edits_keep_anchored_items_and_owned_mask_visibility() {
    let mut doc = fixture(1);
    doc.try_add_layer().unwrap();
    let (id, mask) = doc
        .create_adjustment(invert(128), 0, 1, Some(SamplePlane::Uniform(255)))
        .unwrap();
    doc.reorder_layers(&[2, 1]).unwrap();
    assert_eq!(
        doc.composition().groups.unwrap().roots,
        [
            GroupItem::layer(2),
            GroupItem::adjustment(id),
            GroupItem::layer(1)
        ]
    );
    doc.delete_layer(doc.layer_index(2).unwrap()).unwrap();
    assert!(doc.adjustment(id).is_ok());
    doc.try_add_layer().unwrap();
    assert!(doc.adjustment(id).is_ok());
    let group = doc
        .group_create(&[GroupItem::adjustment(id)], None, None)
        .unwrap();
    doc.begin_mask_stroke(mask.unwrap()).unwrap();
    doc.finish();
    doc.group_set(group, None, Some(false), None, None, None)
        .unwrap();
    assert!(!doc.adjustment_visible(id));
    assert!(doc.begin_mask_stroke(mask.unwrap()).is_err());
}

#[test]
fn adjustment_preflight_is_contact_safe_and_failed_candidates_do_not_publish() {
    let mut doc = fixture(1);
    doc.begin_plugin_stroke();
    let before = doc.composition();
    let bytes = doc.history_bytes();
    doc.preflight_create_adjustment(&invert(255), 0, 1, true)
        .unwrap();
    assert!(doc.stroke.is_some());
    assert!(doc.create_adjustment(invert(255), 0, 1, None).is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.history_bytes(), bytes);
    doc.finish();
    let mut bad = invert(255);
    bad.amount = 2.;
    assert!(doc.preflight_create_adjustment(&bad, 0, 1, true).is_err());
    assert!(doc.create_adjustment(invert(255), 0, 2, None).is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.next_adjustment_id, 0);
    let (id, mask) = doc
        .create_adjustment(invert(255), 0, 1, Some(SamplePlane::Uniform(128)))
        .unwrap();
    assert!(doc.capture_owner_transform_scope(&[], mask).is_err());
    assert!(doc.apply_raster_mask(mask.unwrap()).is_err());
    let group = doc.group_create(&[], None, Some(0)).unwrap();
    assert!(doc.group_ungroup(group).is_err());
    assert_eq!(doc.delete_adjustment(id).unwrap(), mask);
    assert!(!doc.has_adjustments());
}

#[test]
fn adjustment_hidden_definitions_still_require_mix_and_complete_graph_budget() {
    struct OldRenderer;
    impl Renderer for OldRenderer {
        fn supports_transparent(&self) -> bool {
            true
        }
        fn render(
            &self,
            _: &[micopi_plugin_api::CompositeNode],
            _: &[micopi_plugin_api::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut micopi_plugin_api::Pixels,
        ) -> Result<(), String> {
            panic!("preflight must not render")
        }
    }
    let mut doc = Document::new(1, 1);
    doc.renderer = Some(Arc::new(OldRenderer));
    let mut hidden = invert(255);
    hidden.visible = false;
    assert!(doc
        .preflight_create_adjustment(&hidden, 0, 1, false)
        .is_err());
    assert!(doc.groups.is_none());
    let mut doc = fixture(1);
    while doc.layers.len() < MAX_LAYERS {
        doc.try_add_layer().unwrap();
    }
    let mut graph = doc.graph.clone();
    graph.enabled = false;
    doc.set_graph(graph).unwrap();
    for i in 0..15 {
        doc.create_adjustment(hidden.clone(), 0, doc.tree().roots.len(), None)
            .unwrap();
        assert_eq!(doc.adjustments().len(), i + 1);
    }
    let before = doc.composition();
    let bytes = doc.history_bytes();
    let next = doc.next_adjustment_id;
    assert!(doc
        .preflight_create_adjustment(&hidden, 0, doc.tree().roots.len(), false)
        .is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.history_bytes(), bytes);
    assert_eq!(doc.next_adjustment_id, next);
}

#[test]
fn adjustment_fresh_group_copies_do_not_reuse_allocator_and_uniform_masks_remain_raw() {
    let mut doc = fixture(1);
    let group = doc.group_create(&[], None, Some(0)).unwrap();
    let (id, mask) = doc
        .create_adjustment(invert(255), group, 0, Some(SamplePlane::Uniform(91)))
        .unwrap();
    let copy = doc.group_duplicate(group).unwrap();
    let copied = doc
        .composition()
        .groups
        .unwrap()
        .adjustment_ids(GroupItem::group(copy))
        .unwrap();
    assert_eq!(copied, [2]);
    doc.undo();
    let floor = doc.next_adjustment_id;
    assert_eq!(floor, 3);
    let fresh = doc.create_adjustment(invert(255), 0, 1, None).unwrap();
    assert_eq!(fresh, (3, None));
    assert_eq!(doc.adjustment(id).unwrap().id, id);
    assert!(matches!(
        doc.mask_plane(mask.unwrap()).unwrap(),
        PlaneView::Uniform(91)
    ));
    doc.next_adjustment_id = u32::MAX as u64 + 1;
    let tree = doc.groups.as_mut().unwrap();
    tree.next_adjustment_id = doc.next_adjustment_id;
    let before = doc.composition();
    assert!(doc.group_duplicate(group).is_err());
    assert_eq!(doc.composition(), before);
    assert!(doc.create_adjustment(invert(255), 0, 0, None).is_err());
    assert_eq!(doc.composition(), before);
}

#[test]
fn adjustment_only_group_delete_falls_back_only_for_consumed_mask_and_one_undo_restores_it() {
    let mut doc = fixture(1);
    let group = doc.group_create(&[], None, Some(0)).unwrap();
    let (id, mask) = doc
        .create_adjustment(invert(128), group, 0, Some(SamplePlane::Uniform(91)))
        .unwrap();
    let mask = mask.unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: mask }).unwrap();
    let pixels = doc.layers[0].pixels.as_ptr();
    let count = doc.undo.len();
    doc.group_delete(group).unwrap();
    assert_eq!(doc.undo.len(), count + 1);
    assert!(doc.adjustments().is_empty());
    assert_eq!(doc.mask_state().target, PaintTarget::LayerPixels);
    assert_eq!(doc.layers[0].pixels.as_ptr(), pixels);
    assert_eq!(doc.composition().groups.unwrap().version, 3);
    doc.undo();
    assert_eq!(doc.adjustment(id).unwrap().id, id);
    assert_eq!(doc.mask_state().target, PaintTarget::Mask { id: mask });
    assert_eq!(raw(&doc, mask), [91]);
    doc.redo();
    assert!(doc.mask_plane(mask).is_err());
}

#[test]
fn adjustment_project_round_trip_keeps_strict_records_and_owner_agnostic_a8_payload() {
    let mut doc = fixture(3);
    let (id, mask) = doc
        .create_adjustment(
            invert(128),
            0,
            1,
            Some(SamplePlane::Dense(vec![17, 128, 255])),
        )
        .unwrap();
    doc.compose();
    let expected = doc.composite.clone();
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/adjustments-engine/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("owner.mcp");
    doc.save(&path).unwrap();
    let mut loaded = Document::load(&path).unwrap();
    assert_eq!(loaded.composition(), doc.composition());
    assert_eq!(loaded.adjustment(id).unwrap(), doc.adjustment(id).unwrap());
    assert_eq!(raw(&loaded, mask.unwrap()), [17, 128, 255]);
    assert_eq!(loaded.mask_state().version, 2);
    assert!(loaded.renderer.is_none());
    loaded.renderer = doc.renderer.clone();
    loaded.compose();
    assert_eq!(loaded.composite, expected);
}
