//! Copied subtree masks retain raw ownership under bounded structural history.
use super::*;
use crate::engine::groups::{GroupItem, GroupPolicy};
use crate::engine::raster_masks::{
    MaskMetadata, MaskOwner, MaskState, PaintTarget, PlaneView, SamplePlane,
};

struct StorageRenderer;
impl Renderer for StorageRenderer {
    fn supports_masks(&self) -> bool {
        true
    }
    fn supports_mask_effects(&self) -> bool {
        true
    }
    fn supports_mix(&self) -> bool {
        true
    }
    fn supports_transparent(&self) -> bool {
        true
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        mode <= micopi_plugin_api::BLEND_MODE_MAX
    }
    fn render(
        &self,
        _: &[micopi_plugin_api::CompositeNode],
        _: &[micopi_plugin_api::CompositeLayer],
        _: u32,
        _: u32,
        _: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        panic!("Structural storage/history must not render")
    }
}
fn fixture() -> Document {
    let mut doc = Document::new(7, 5);
    doc.renderer = Some(Arc::new(StorageRenderer));
    doc.try_add_layer().unwrap();
    doc.try_add_layer().unwrap();
    let inner = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    let empty = doc.group_create(&[], None, Some(inner)).unwrap();
    let outer = doc
        .group_create(&[GroupItem::group(inner), GroupItem::layer(3)], None, None)
        .unwrap();
    let unrelated = doc.group_create(&[], None, Some(0)).unwrap();
    assert_eq!((inner, empty, outer, unrelated), (1, 2, 3, 4));
    doc.group_set(
        outer,
        None,
        None,
        Some(173),
        None,
        Some(GroupPolicy::PassThrough),
    )
    .unwrap();
    doc.group_set(empty, None, Some(false), None, None, None)
        .unwrap();
    doc.set_layer_properties(1, true, 137, BlendMode::Normal)
        .unwrap();
    doc.set_layer_properties(2, false, 197, BlendMode::Normal)
        .unwrap();
    for owner in [
        MaskOwner::Layer { id: 2 },
        MaskOwner::Layer { id: 1 },
        MaskOwner::Group { id: inner },
        MaskOwner::Group { id: outer },
        MaskOwner::Group { id: empty },
        MaskOwner::Group { id: unrelated },
    ] {
        let id = doc.create_mask(owner, 91).unwrap();
        if ![3, 5].contains(&id) {
            doc.replace_mask_plane(
                id,
                SamplePlane::Dense((0..35).map(|i| (i * 7 + id as usize) as u8).collect()),
            )
            .unwrap();
        }
        let mut metadata = doc.raster_masks.items.last().unwrap().clone();
        metadata.linked = false;
        metadata.density = 177;
        metadata.feather_radius_px = 2;
        metadata.enabled = id != 3;
        doc.update_mask_metadata(metadata).unwrap();
    }
    doc.layer_metadata[1].pixel_locked = true;
    doc.layer_metadata[1].position_locked = true;
    doc.set_layer_selection(&[1, 2], Some(2)).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: 3 }).unwrap();
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_mask_tiles.clear();
    doc
}
fn pointer(doc: &Document, id: u64) -> *const u8 {
    let PlaneView::Dense(raw) = doc.mask_plane(id).unwrap() else {
        panic!("Expected dense raw samples")
    };
    raw.as_ptr()
}
fn bytes(doc: &Document, id: u64) -> Vec<u8> {
    let plane = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|i| plane.sample(i))
        .collect()
}

#[test]
fn group_duplicate_masks_are_independent_ordered_and_one_structural_history_edit() {
    let mut doc = fixture();
    let before = doc.composition();
    let rgba: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect();
    let dense: Vec<_> = [1, 2, 4, 6]
        .into_iter()
        .map(|id| (id, pointer(&doc, id)))
        .collect();
    let epoch = doc.mask_storage_epoch;
    let copied = doc.group_duplicate(3).unwrap();
    assert_eq!(copied, 5);
    assert_eq!(doc.group_layer_ids(copied).unwrap(), vec![4, 5]);
    assert_eq!(doc.selected_layer_ids(), vec![4, 5]);
    assert_eq!(doc.layer_id(doc.active), 4);
    assert_eq!(doc.raster_masks.target, PaintTarget::Mask { id: 8 });
    assert_ne!(doc.mask_storage_epoch, epoch);
    assert_eq!(
        doc.raster_masks
            .items
            .iter()
            .map(|mask| mask.id)
            .collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    for (old, new, owner) in [
        (1, 7, MaskOwner::Layer { id: 4 }),
        (3, 8, MaskOwner::Group { id: 6 }),
        (4, 9, MaskOwner::Group { id: 5 }),
        (5, 10, MaskOwner::Group { id: 7 }),
    ] {
        let mut expected = before.raster_masks.items[(old - 1) as usize].clone();
        expected.id = new;
        expected.owner = owner;
        assert_eq!(doc.raster_masks.items[(new - 1) as usize], expected);
        assert_eq!(bytes(&doc, old), bytes(&doc, new));
        assert_eq!(doc.mask_content_revision(new).unwrap(), 0);
    }
    assert_ne!(pointer(&doc, 1), pointer(&doc, 7));
    assert!(matches!(doc.mask_plane(8).unwrap(), PlaneView::Uniform(91)));
    assert!(matches!(
        doc.mask_plane(10).unwrap(),
        PlaneView::Uniform(91)
    ));
    for (index, original) in rgba.iter().enumerate() {
        assert_eq!(
            doc.layers[doc.layer_index(index as u64 + 1).unwrap()]
                .pixels
                .as_ptr(),
            *original
        );
    }
    for (id, original) in &dense {
        assert_eq!(pointer(&doc, *id), *original);
    }
    let graph = doc.effective_graph().unwrap();
    let mask_source = graph
        .nodes
        .iter()
        .find(|node| node.data == (NodeKind::MaskSource { mask_id: 7 }))
        .unwrap()
        .id;
    let apply = graph
        .edges
        .iter()
        .find(|edge| edge.from == mask_source)
        .unwrap()
        .to;
    assert!(
        matches!(graph.nodes.iter().find(|node| node.id == apply).unwrap().data, NodeKind::ApplyMask { opacity } if opacity == 137. / 255.)
    );
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(7, 0), (8, 0), (9, 0), (10, 0)]
    );
    assert_eq!(doc.undo.len(), 1);
    let Some(MaskChange::Structural(change)) = &doc.undo[0].mask_change else {
        panic!("Expected compound structural masks")
    };
    assert_eq!(change.current, vec![6, 7, 8, 9]);
    assert!(change.saved.is_empty());
    let after = doc.composition();
    let copied_pointer = pointer(&doc, 7);
    doc.undo();
    let mut expected = before;
    expected.next_layer_id = after.next_layer_id;
    expected.next_group_id = after.next_group_id;
    expected.raster_masks.next_id = after.raster_masks.next_id;
    assert_eq!(doc.composition(), expected);
    assert!(doc.replay_mask_tiles.is_empty());
    doc.redo();
    assert_eq!(doc.composition(), after);
    assert_eq!(pointer(&doc, 7), copied_pointer);
    doc.replace_mask_plane(7, SamplePlane::Uniform(17)).unwrap();
    assert_ne!(bytes(&doc, 1), bytes(&doc, 7));
    for (id, original) in &dense {
        assert_eq!(pointer(&doc, *id), *original);
    }
}

#[test]
fn group_duplicate_empty_owner_preserves_selection_and_remaps_only_its_mask_target() {
    for target in [
        PaintTarget::LayerPixels,
        PaintTarget::Mask { id: 5 },
        PaintTarget::Mask { id: 2 },
    ] {
        let mut doc = fixture();
        doc.set_mask_target(target).unwrap();
        let active = doc.layer_id(doc.active);
        let selected = doc.selected_layer_ids();
        let rgba: Vec<_> = doc
            .layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect();
        doc.next_layer_id = u32::MAX as u64 + 1;
        assert_eq!(doc.group_duplicate(2).unwrap(), 5);
        assert_eq!(doc.next_layer_id, u32::MAX as u64 + 1);
        assert_eq!(doc.group_layer_ids(5).unwrap(), Vec::<u64>::new());
        assert_eq!(doc.layer_id(doc.active), active);
        assert_eq!(doc.selected_layer_ids(), selected);
        assert_eq!(
            doc.raster_masks.target,
            if target == (PaintTarget::Mask { id: 5 }) {
                PaintTarget::Mask { id: 7 }
            } else {
                target
            }
        );
        assert_eq!(
            doc.layers
                .iter()
                .map(|layer| layer.pixels.as_ptr())
                .collect::<Vec<_>>(),
            rgba
        );
        assert!(matches!(doc.mask_plane(7).unwrap(), PlaneView::Uniform(91)));
        assert_eq!(doc.undo.len(), 1);
    }
}

#[test]
fn group_mask_copy_metadata_preflight_bounds_count_logical_bytes_and_allocator() {
    let state = MaskState {
        version: 1,
        items: (1..=3)
            .map(|id| MaskMetadata::new(id, MaskOwner::Group { id }))
            .collect(),
        next_id: 4,
        target: PaintTarget::Mask { id: 2 },
    };
    state.validate(4096, 4096, &[1], &[1, 2, 3]).unwrap();
    assert!(
        mask_group_editing::plan_mask_copy(&state, &[], &[(1, 4), (2, 5)], 4096, 4096)
            .err()
            .unwrap()
            .contains("budget")
    );
    let mut full = state.clone();
    full.items = (1..=32)
        .map(|id| {
            MaskMetadata::new(
                id,
                if id <= 16 {
                    MaskOwner::Layer { id }
                } else {
                    MaskOwner::Group { id: id - 16 }
                },
            )
        })
        .collect();
    full.next_id = 33;
    assert!(
        mask_group_editing::plan_mask_copy(&full, &[], &[(1, 17)], 1, 1)
            .err()
            .unwrap()
            .contains("count")
    );
    let mut exhausted = state.clone();
    exhausted.next_id = u32::MAX as u64 + 1;
    assert!(
        mask_group_editing::plan_mask_copy(&exhausted, &[], &[(1, 4)], 1, 1)
            .err()
            .unwrap()
            .contains("identity")
    );
    assert_eq!(
        mask_group_editing::plan_mask_copy(&exhausted, &[], &[], 1, 1)
            .unwrap()
            .state,
        exhausted
    );
    assert!(
        mask_group_editing::plan_mask_copy(&MaskState::default(), &[], &[(1, 2)], 1, 1)
            .unwrap()
            .state
            .is_default()
    );
}

#[test]
fn group_duplicate_source_contact_and_invalid_raw_plane_reject_without_publication() {
    let mut doc = fixture();
    doc.begin_plugin_stroke();
    let epoch = doc.mask_storage_epoch;
    assert!(doc.group_duplicate(3).unwrap_err().contains("contact"));
    assert!(doc.stroke.is_some());
    assert_eq!(doc.mask_storage_epoch, epoch);
    doc.finish();
    doc.mask_planes[0].samples = SamplePlane::Dense(vec![0; 34]);
    let before = doc.composition();
    let rgba: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect();
    assert!(doc.group_duplicate(3).unwrap_err().contains("sample count"));
    assert_eq!(doc.composition(), before);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect::<Vec<_>>(),
        rgba
    );
    assert_eq!(doc.mask_planes.len(), 6);
    assert!(doc.undo.is_empty());
}

fn filtered_document(filters: u32) -> Document {
    use crate::engine::graph::{Edge, Filter};
    let mut doc = Document::new(1, 1);
    doc.renderer = Some(Arc::new(StorageRenderer));
    doc.try_add_layer().unwrap();
    doc.group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    doc.create_mask(MaskOwner::Layer { id: 2 }, 91).unwrap();
    let mut graph = doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|node| node.data == (NodeKind::Layer { index: 1 }))
        .unwrap()
        .id;
    let next = graph.nodes.iter().map(|node| node.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = next + filters - 1;
        }
    }
    let mut previous = source;
    for id in next..next + filters {
        graph.nodes.push(Node {
            id,
            node_type: None,
            position: [0., 0.],
            data: NodeKind::Filter {
                filter: Filter::Invert,
                amount: 1.,
            },
        });
        graph.edges.push(Edge {
            from: previous,
            to: id,
            input: 0,
        });
        previous = id;
    }
    doc.set_graph(graph).unwrap();
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_mask_tiles.clear();
    doc
}

#[test]
fn group_duplicate_preflights_editable_and_generated_graph_budgets_before_raw_copy() {
    for filters in [28, 31] {
        let mut doc = filtered_document(filters);
        if filters == 28 {
            // The duplicated editable stack still fits; generated group/mask
            // gates make the complete candidate exceed the instruction limit.
            let candidate = doc
                .graph
                .sync_stack_mapped(&[BlendMode::Normal; 3], &[1, 2], &[1, 2, 3], &[(2, 3)], &[])
                .unwrap();
            assert_eq!(candidate.nodes.len(), 62);
        } else {
            assert!(doc
                .graph
                .sync_stack_mapped(&[BlendMode::Normal; 3], &[1, 2], &[1, 2, 3], &[(2, 3)], &[],)
                .is_err());
        }
        let before = doc.composition();
        let rgba: Vec<_> = doc
            .layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect();
        let raw = bytes(&doc, 1);
        let epoch = doc.mask_storage_epoch;
        assert!(doc.group_duplicate(1).unwrap_err().contains("limit"));
        assert_eq!(doc.composition(), before);
        assert_eq!(
            doc.layers
                .iter()
                .map(|layer| layer.pixels.as_ptr())
                .collect::<Vec<_>>(),
            rgba
        );
        assert_eq!(bytes(&doc, 1), raw);
        assert_eq!(doc.mask_storage_epoch, epoch);
        assert!(doc.undo.is_empty());
        assert!(doc.replay_mask_tiles.is_empty());
    }
}
