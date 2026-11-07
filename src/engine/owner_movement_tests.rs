//! Whole-domain movement owns one raster/mask pair without metadata snapshots.
use super::*;
use crate::engine::groups::GroupItem;
use crate::engine::raster_masks::{MaskMetadata, PaintTarget, PlaneView};

struct StorageRenderer {
    effects: bool,
}
impl Renderer for StorageRenderer {
    fn supports_masks(&self) -> bool {
        true
    }
    fn supports_mask_effects(&self) -> bool {
        self.effects
    }
    fn supports_transparent(&self) -> bool {
        true
    }
    fn supports_mix(&self) -> bool {
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
        panic!("Staged storage/history must not render")
    }
}
fn raw(doc: &Document, id: u64) -> Vec<u8> {
    let view = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|i| view.sample(i))
        .collect()
}
fn pointer(doc: &Document, id: u64) -> *const u8 {
    match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!("Expected a dense plane"),
    }
}
fn fixture() -> Document {
    let mut doc = Document::new(3, 2);
    doc.renderer = Some(Arc::new(StorageRenderer { effects: true }));
    doc.try_add_layer().unwrap();
    let group = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    for owner in [
        MaskOwner::Layer { id: 1 },
        MaskOwner::Group { id: group },
        MaskOwner::Layer { id: 2 },
    ] {
        let id = doc.create_mask(owner, 213).unwrap();
        if id != 2 {
            doc.replace_mask_plane(
                id,
                SamplePlane::Dense((0..6).map(|i| i * 11 + id as u8).collect()),
            )
            .unwrap();
        }
    }
    let mut metadata = doc.raster_masks.items[0].clone();
    metadata.enabled = false;
    metadata.density = 179;
    metadata.feather_radius_px = 2;
    doc.update_mask_metadata(metadata).unwrap();
    doc.layers[0].pixels = vec![
        0x00123456, 0xff123456, 0x80123456, 0xffabcdef, 0x40010203, 0,
    ];
    doc.layers[0].opacity = 137;
    doc.layers[1].pixels.fill(0xff203040);
    doc.set_layer_selection(&[1, 2], Some(1)).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: 3 }).unwrap();
    doc.selection = crate::engine::selection::SelectionState {
        current: Some(crate::engine::selection::Mask::new(3, 2, vec![97; 6]).unwrap()),
        previous: Some(crate::engine::selection::Mask::new(3, 2, vec![173; 6]).unwrap()),
    };
    doc.last_transform = Some(crate::engine::transform::Recipe {
        matrix: [1., 0., 1., 0., 1., 0.],
        denominator: [0., 0., 1.],
        target: crate::engine::transform::Target::Layer,
        method: crate::engine::transform::Method::Nearest,
    });
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_tiles.clear();
    doc.replay_mask_tiles.clear();
    doc.dirty = false;
    doc
}

#[test]
fn owner_move_linked_disabled_mask_and_pixels_share_one_raw_only_undo() {
    let mut doc = fixture();
    let before = doc.composition();
    let pixels = doc.layers[0].pixels.clone();
    let bytes = raw(&doc, 1);
    let original_revision = doc.mask_content_revision(1).unwrap();
    let unrelated_raw = pointer(&doc, 3);
    let unrelated_rgba = doc.layers[1].pixels.as_ptr();
    let epoch = doc.mask_storage_epoch;
    doc.validate_owner_move(Some(1), Some(1)).unwrap();
    assert_eq!(doc.mask_storage_epoch, epoch);
    let mut output = pixels.clone();
    output.reverse();
    let moved = vec![255, 255, 255, 12, 23, 34];
    assert!(doc
        .apply_owner_move(
            Some(1),
            Some(output.clone()),
            Some((1, SamplePlane::Dense(moved.clone())))
        )
        .unwrap());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.layers[0].pixels, output);
    assert_eq!(raw(&doc, 1), moved);
    assert_eq!(pointer(&doc, 3), unrelated_raw);
    assert_eq!(doc.layers[1].pixels.as_ptr(), unrelated_rgba);
    assert_eq!(doc.mask_content_revision(1).unwrap(), original_revision + 1);
    assert_ne!(doc.mask_storage_epoch, epoch);
    assert_eq!(doc.undo.len(), 1);
    assert!(doc.undo[0].composition.is_none());
    assert!(doc.undo[0].layer_change.is_none());
    assert!(doc.undo[0].mask_tiles.is_empty());
    assert!(matches!(
        &doc.undo[0].mask_change,
        Some(MaskChange::Replace { slot: 0, .. })
    ));
    assert_eq!(doc.history_bytes(), 6 * 4 + 6);
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(1, 0)]
    );
    assert_eq!(
        doc.replay_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(0, 0)]
    );
    doc.replay_mask_tiles.clear();
    doc.undo();
    assert_eq!(doc.layers[0].pixels, pixels);
    assert_eq!(raw(&doc, 1), bytes);
    assert_eq!(doc.mask_content_revision(1).unwrap(), original_revision);
    assert_eq!(doc.composition(), before);
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(1, 0)]
    );
    doc.redo();
    assert_eq!(doc.layers[0].pixels, output);
    assert_eq!(raw(&doc, 1), moved);
    assert_eq!(doc.composition(), before);
    assert_eq!(pointer(&doc, 3), unrelated_raw);
}

#[test]
fn owner_move_mask_only_uses_its_owner_locks_and_retains_active_pixels_and_target() {
    let mut doc = fixture();
    doc.layer_metadata[1].pixel_locked = true;
    doc.layer_metadata[1].alpha_locked = true;
    let before = doc.composition();
    let rgba: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let raw_owner = pointer(&doc, 1);
    doc.validate_owner_move(None, Some(3)).unwrap();
    assert!(doc
        .apply_owner_move(None, None, Some((3, SamplePlane::Uniform(255))))
        .unwrap());
    assert_eq!(doc.composition(), before);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        rgba
    );
    assert_eq!(pointer(&doc, 1), raw_owner);
    assert!(doc.undo[0].tiles.is_empty());
    assert!(doc.undo[0].composition.is_none());
    doc.undo();
    doc.layer_metadata[1].position_locked = true;
    doc.validate_owner_move_source().unwrap();
    assert!(doc
        .validate_owner_move(None, Some(3))
        .unwrap_err()
        .contains("position"));
    doc.layer_metadata[1].position_locked = false;
    doc.group_set(1, None, Some(false), None, None, None)
        .unwrap();
    assert!(doc
        .validate_owner_move(None, Some(3))
        .unwrap_err()
        .contains("visible"));
    assert!(doc
        .validate_owner_move(None, Some(2))
        .unwrap_err()
        .contains("Group"));
}

#[test]
fn owner_move_rejects_missing_or_unrelated_linked_outputs_and_retains_legacy_guards() {
    let mut doc = fixture();
    let before = doc.composition();
    let epoch = doc.mask_storage_epoch;
    let raw_before = raw(&doc, 1);
    for (layer, mask) in [
        (Some(1), None),
        (Some(1), Some(3)),
        (Some(2), Some(3)),
        (None, None),
        (None, Some(99)),
    ] {
        assert!(doc.validate_owner_move(layer, mask).is_err());
    }
    assert!(doc
        .apply_owner_move(Some(1), None, Some((1, SamplePlane::Uniform(255))))
        .is_err());
    assert!(doc
        .apply_owner_move(None, Some(vec![0; 6]), Some((1, SamplePlane::Uniform(255))))
        .is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(raw(&doc, 1), raw_before);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.undo.is_empty());
    let selection = doc.selection.clone();
    let pixels = doc.layers[0].pixels.clone();
    assert!(doc.apply_movement(Some(&pixels), selection).is_err());
    doc.raster_masks.items[0].linked = false;
    doc.validate_owner_move(Some(1), None).unwrap();
    assert!(doc.validate_owner_move(Some(1), Some(1)).is_err());
    let mut output = pixels;
    output.reverse();
    assert!(doc.apply_owner_move(Some(1), Some(output), None).unwrap());
    assert_eq!(raw(&doc, 1), raw_before);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.replay_mask_tiles.is_empty());
}

#[test]
fn owner_move_staging_failure_and_contact_preflight_publish_no_partial_pair() {
    let mut doc = fixture();
    let before = doc.composition();
    let pixels = doc.layers[0].pixels.clone();
    let raw_before = raw(&doc, 1);
    let epoch = doc.mask_storage_epoch;
    let mut output = pixels.clone();
    output.reverse();
    assert!(doc
        .apply_owner_move(
            Some(1),
            Some(output.clone()),
            Some((1, SamplePlane::Dense(vec![0; 5])))
        )
        .is_err());
    assert!(doc
        .apply_owner_move(
            Some(1),
            Some(vec![0; 5]),
            Some((1, SamplePlane::Uniform(255)))
        )
        .is_err());
    doc.layer_metadata[0].alpha_locked = true;
    assert!(doc
        .apply_owner_move(Some(1), Some(output), Some((1, SamplePlane::Uniform(255))))
        .unwrap_err()
        .contains("transparency"));
    doc.layer_metadata[0].alpha_locked = false;
    doc.begin_plugin_stroke();
    assert!(doc
        .validate_owner_move(Some(1), Some(1))
        .unwrap_err()
        .contains("contact"));
    assert!(doc.stroke.is_some());
    doc.finish();
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.layers[0].pixels, pixels);
    assert_eq!(raw(&doc, 1), raw_before);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.undo.is_empty());
    assert!(!doc.dirty);
}

#[test]
fn owner_move_equal_raw_samples_keep_noop_storage_epoch_and_replay_damage() {
    let mut doc = fixture();
    doc.mask_planes[0].samples = SamplePlane::Uniform(91);
    let revision = doc.mask_content_revision(1).unwrap();
    let epoch = doc.mask_storage_epoch;
    let pixels = doc.layers[0].pixels.clone();
    assert!(!doc
        .apply_owner_move(
            Some(1),
            Some(pixels.clone()),
            Some((1, SamplePlane::Dense(vec![91; 6])))
        )
        .unwrap());
    assert!(matches!(doc.mask_plane(1).unwrap(), PlaneView::Uniform(91)));
    assert_eq!(doc.mask_content_revision(1).unwrap(), revision);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.undo.is_empty());
    assert!(doc.replay_mask_tiles.is_empty());
    assert!(doc.replay_tiles.is_empty());
    let mut output = pixels;
    output.reverse();
    assert!(doc
        .apply_owner_move(Some(1), Some(output), Some((1, SamplePlane::Uniform(91))))
        .unwrap());
    assert!(doc.undo[0].mask_change.is_none());
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.replay_mask_tiles.is_empty());
}

#[test]
fn owner_move_raw_only_history_retains_pinned_saved_planes_across_redo_and_edits() {
    let mut doc = fixture();
    let moved = vec![19, 29, 39, 49, 59, 69];
    assert!(doc
        .apply_owner_move(None, None, Some((3, SamplePlane::Dense(moved.clone()))))
        .unwrap());
    doc.undo();
    let saved = doc.checkpoint_history();
    let Some(MaskChange::Replace { saved: plane, .. }) = &saved.redo[0].mask_change else {
        panic!("Expected one retained plane")
    };
    let pinned = plane.samples.clone();
    assert_eq!(pinned, SamplePlane::Dense(moved.clone()));
    doc.redo();
    assert_eq!(raw(&doc, 3), moved);
    assert!(doc
        .apply_owner_move(None, None, Some((3, SamplePlane::Uniform(17))))
        .unwrap());
    assert_eq!(plane.samples, pinned);
    assert!(saved.redo[0].composition.is_none());
    assert!(saved.redo[0].tiles.is_empty());
    assert_eq!(saved.redo[0].bytes(), 6);
}

#[test]
fn owner_move_preflight_checks_complete_raw_storage_effect_capabilities_and_budgets() {
    let mut doc = fixture();
    doc.mask_planes[2].samples = SamplePlane::Dense(vec![0; 5]);
    assert!(doc
        .validate_owner_move(Some(1), Some(1))
        .unwrap_err()
        .contains("sample count"));
    doc.mask_planes[2].samples = SamplePlane::Uniform(255);
    doc.renderer = Some(Arc::new(StorageRenderer { effects: false }));
    assert!(doc
        .validate_owner_move(Some(1), Some(1))
        .unwrap_err()
        .contains("effects"));
    doc.renderer = None;
    assert!(doc.validate_owner_move(None, Some(3)).is_err());
    // Deliberately malformed engine-only metadata exercises scalar budget
    // admission before source/output image allocation or raw-plane inspection.
    let mut doc = Document::new(1, 1);
    doc.width = 4096;
    doc.height = 4096;
    doc.raster_masks.items = (1..=5)
        .map(|id| MaskMetadata::new(id, MaskOwner::Layer { id }))
        .collect();
    doc.raster_masks.next_id = 6;
    assert!(doc
        .validate_owner_move(Some(1), Some(1))
        .unwrap_err()
        .contains("budget"));
    assert_eq!(doc.layers[0].pixels.len(), 1);
    assert!(doc.undo.is_empty());
}
