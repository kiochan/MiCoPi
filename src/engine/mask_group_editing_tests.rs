//! Structural mask history owns only removed planes, including empty folders.
use super::*;
use crate::engine::groups::GroupItem;
use crate::engine::raster_masks::{MaskOwner, PaintTarget, PlaneView, SamplePlane};

struct StorageRenderer;
impl Renderer for StorageRenderer {
    fn supports_masks(&self) -> bool {
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
    doc.add_layer();
    doc.add_layer();
    assert_eq!(doc.layers.len(), 3);
    let inner = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    let outer = doc
        .group_create(&[GroupItem::group(inner), GroupItem::layer(3)], None, None)
        .unwrap();
    let empty = doc.group_create(&[], None, Some(0)).unwrap();
    assert_eq!((inner, outer, empty), (1, 2, 3));
    for owner in [
        MaskOwner::Layer { id: 2 },
        MaskOwner::Layer { id: 1 },
        MaskOwner::Group { id: inner },
        MaskOwner::Group { id: outer },
        MaskOwner::Group { id: empty },
    ] {
        let id = doc.create_mask(owner, 91).unwrap();
        if id != 3 {
            doc.replace_mask_plane(
                id,
                SamplePlane::Dense((0..35).map(|i| (i * 7 + id as usize) as u8).collect()),
            )
            .unwrap();
        }
    }
    doc.layer_metadata[1].pixel_locked = true;
    doc.layer_metadata[1].position_locked = true;
    doc.set_layer_selection(&[1, 2], Some(2)).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: 5 }).unwrap();
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_mask_tiles.clear();
    doc
}
fn dense_pointer(doc: &Document, id: u64) -> *const u8 {
    let PlaneView::Dense(samples) = doc.mask_plane(id).unwrap() else {
        panic!("Expected a dense raw plane")
    };
    samples.as_ptr()
}

#[test]
fn group_delete_structural_masks_restore_interleaved_slots_without_copying_survivors() {
    let mut doc = fixture();
    let before = serde_json::to_value(doc.composition()).unwrap();
    let pointers: Vec<_> = [1, 2, 4, 5]
        .into_iter()
        .map(|id| (id, dense_pointer(&doc, id)))
        .collect();
    let retained_rgba = doc.layers[0].pixels.as_ptr();
    let epoch = doc.mask_storage_epoch;
    doc.group_delete(2).unwrap();
    assert_ne!(doc.mask_storage_epoch, epoch);
    assert_eq!(doc.layers.len(), 1);
    assert_eq!(doc.layers[0].pixels.as_ptr(), retained_rgba);
    assert_eq!(doc.raster_masks.target, PaintTarget::Mask { id: 5 });
    assert_eq!(
        doc.mask_planes
            .iter()
            .map(|plane| plane.id)
            .collect::<Vec<_>>(),
        vec![2, 5]
    );
    assert!(doc.replay_mask_tiles.is_empty());
    assert_eq!(doc.undo.len(), 1);
    let Some(MaskChange::Structural(change)) = &doc.undo[0].mask_change else {
        panic!("Subtree masks need one bounded structural history record")
    };
    assert_eq!(
        change
            .saved
            .iter()
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>(),
        vec![0, 2, 3]
    );
    assert_eq!(
        change
            .saved
            .iter()
            .map(|(_, plane)| plane.id)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
    assert_eq!(
        doc.undo[0].bytes() - doc.undo[0].metadata_bytes - 2 * 35 * 4,
        std::mem::size_of::<MaskStructuralChange>()
            + 3 * std::mem::size_of::<(usize, Arc<crate::engine::raster_masks::MaskPlane>)>()
            + 2 * 35
            + 1
    );
    let snapshot = doc.undo.clone();
    let Some(MaskChange::Structural(shared)) = &snapshot[0].mask_change else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(change, shared));
    drop(snapshot);
    doc.undo();
    assert_eq!(serde_json::to_value(doc.composition()).unwrap(), before);
    for (id, pointer) in &pointers {
        assert_eq!(dense_pointer(&doc, *id), *pointer);
    }
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(1, 0), (3, 0), (4, 0)]
    );
    doc.redo();
    assert!(doc.replay_mask_tiles.is_empty());
    for id in [2, 5] {
        assert_eq!(
            dense_pointer(&doc, id),
            pointers.iter().find(|(saved, _)| *saved == id).unwrap().1
        );
    }
    assert!(std::mem::size_of::<MaskChange>() <= 3 * std::mem::size_of::<usize>());
}

#[test]
fn group_delete_empty_masked_folder_is_one_edit_with_target_fallback() {
    let mut doc = fixture();
    doc.set_mask_target(PaintTarget::Mask { id: 5 }).unwrap();
    let before = serde_json::to_value(doc.composition()).unwrap();
    let rgba: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect();
    doc.group_delete(3).unwrap();
    assert_eq!(doc.raster_masks.target, PaintTarget::LayerPixels);
    assert_eq!(doc.raster_masks.next_id, 6);
    assert_eq!(doc.next_group_id, 4);
    assert_eq!(doc.next_layer_id, 4);
    assert_eq!(doc.undo.len(), 1);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect::<Vec<_>>(),
        rgba
    );
    assert!(doc.replay_mask_tiles.is_empty());
    doc.undo();
    assert_eq!(serde_json::to_value(doc.composition()).unwrap(), before);
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(5, 0)]
    );
}

#[test]
fn group_delete_preflight_preserves_contact_and_rejects_invalid_removed_raw_storage() {
    let mut doc = fixture();
    let lease = doc.begin_mask_stroke(2).unwrap();
    let epoch = doc.mask_storage_epoch;
    assert!(doc.group_delete(2).unwrap_err().contains("contact"));
    assert_eq!(doc.mask_contact, Some(lease));
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.stroke.is_some());
    doc.finish();
    doc.mask_planes[0].samples = SamplePlane::Dense(vec![0; 34]);
    let state = serde_json::to_value(doc.composition()).unwrap();
    assert!(doc.group_delete(2).is_err());
    assert_eq!(serde_json::to_value(doc.composition()).unwrap(), state);
    assert_eq!(doc.mask_planes.len(), 5);
    assert!(doc.undo.is_empty());
}
