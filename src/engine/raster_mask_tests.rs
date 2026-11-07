//! Byte ownership/history and strict MICOPI04 persistence, independently of UI tools.
use super::*;
use crate::engine::raster_masks::{
    MaskMetadata, MaskOwner, MaskState, PaintTarget, PlaneView, SamplePlane,
};
struct AvailableRenderer;
impl Renderer for AvailableRenderer {
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
        panic!("Storage/history must not call a renderer")
    }
}
fn document(width: usize, height: usize) -> Document {
    let mut doc = Document::new(width, height);
    doc.renderer = Some(Arc::new(AvailableRenderer));
    doc
}
fn bytes(doc: &Document, id: u64) -> Vec<u8> {
    let plane = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|index| plane.sample(index))
        .collect()
}
fn path(label: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "micopi-mask-core-{}-{}-{}.mcp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        label
    ))
}

#[test]
fn raster_mask_metadata_keeps_namespaces_defaults_and_logical_budget_explicit() {
    let mut state = MaskState::default();
    assert!(state.is_default());
    let layers: Vec<_> = (1..=16).collect();
    let groups = layers.clone();
    for id in 1..=32 {
        state.items.push(MaskMetadata::new(
            id,
            if id <= 16 {
                MaskOwner::Layer { id }
            } else {
                MaskOwner::Group { id: id - 16 }
            },
        ));
    }
    state.next_id = 33;
    state.target = PaintTarget::Mask { id: 32 };
    state.validate(128, 128, &layers, &groups).unwrap();
    let mut promoted = state.clone();
    promoted.version = 2;
    promoted.validate(128, 128, &layers, &groups).unwrap();
    let encoded = serde_json::to_value(&state).unwrap();
    assert!(encoded["items"][0].get("enabled").is_none());
    assert!(encoded["items"][0].get("density").is_none());
    for change in [0, 1, 2, 3, 4] {
        let mut invalid = state.clone();
        match change {
            0 => invalid.items[1].owner = invalid.items[0].owner,
            1 => invalid.items.swap(0, 1),
            2 => invalid.target = PaintTarget::Mask { id: 99 },
            3 => invalid.items[0].feather_radius_px = 257,
            _ => invalid.version = 3,
        }
        assert!(invalid.validate(128, 128, &layers, &groups).is_err());
    }
    state.items.truncate(4);
    state.target = PaintTarget::LayerPixels;
    state.validate(4096, 4096, &layers, &groups).unwrap();
    state
        .items
        .push(MaskMetadata::new(5, MaskOwner::Layer { id: 5 }));
    assert!(state
        .validate(4096, 4096, &layers, &groups)
        .unwrap_err()
        .contains("budget"));
    let invalid_owner = MaskState {
        version: 1,
        items: vec![MaskMetadata::new(1, MaskOwner::Layer { id: u64::MAX })],
        next_id: 2,
        target: PaintTarget::LayerPixels,
    };
    assert!(invalid_owner.validate(4, 4, &[u64::MAX], &[]).is_err());
    assert!(serde_json::from_value::<MaskState>(
        serde_json::json!({"version":1,"items":[],"next_id":1,"unknown":true})
    )
    .is_err());
}

#[test]
fn raster_mask_stroke_captures_only_byte_tiles_once_and_undo_keeps_rgba_untouched() {
    let mut doc = document(130, 70);
    let rgba = doc.layers[0].pixels.clone();
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    assert!(matches!(
        doc.mask_plane(id).unwrap(),
        PlaneView::Uniform(255)
    ));
    doc.layer_metadata[0].pixel_locked = true;
    doc.layer_metadata[0].alpha_locked = true;
    let undo = doc.undo.len();
    let lease = doc.begin_mask_stroke(id).unwrap();
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!(),
    };
    let region = doc.mask_region(lease, 63, 1, 2, 65).unwrap();
    for y in 0..65 {
        for x in 0..2 {
            unsafe {
                *region.data.add(y * region.stride as usize + x) = 37;
            }
        }
    }
    assert_eq!(doc.stroke.as_ref().unwrap().mask_tiles.len(), 4);
    let cells: Vec<_> = doc
        .stroke
        .as_ref()
        .unwrap()
        .mask_tiles
        .values()
        .map(Arc::as_ptr)
        .collect();
    doc.prepare_mask_region(lease, 63, 2, 2, 64).unwrap();
    assert_eq!(
        doc.stroke
            .as_ref()
            .unwrap()
            .mask_tiles
            .values()
            .map(Arc::as_ptr)
            .collect::<Vec<_>>(),
        cells
    );
    assert_eq!(doc.mask_plane(id).unwrap().sample(1 * 130 + 63), 37);
    assert_eq!(doc.mask_plane(id).unwrap().sample(1 * 130 + 62), 255);
    assert_eq!(doc.mask_plane(id).unwrap().sample(66 * 130 + 63), 255);
    assert!(
        matches!(doc.mask_plane(id).unwrap(),PlaneView::Dense(samples) if samples.as_ptr()==pointer)
    );
    assert!(doc.stroke.as_ref().unwrap().tiles.is_empty());
    doc.finish();
    assert_eq!(doc.undo.len(), undo + 1);
    assert!(doc.mask_region(lease, 0, 0, 1, 1).is_err());
    let painted = bytes(&doc, id);
    doc.undo();
    assert!(bytes(&doc, id).iter().all(|sample| *sample == 255));
    doc.redo();
    assert_eq!(bytes(&doc, id), painted);
    assert_eq!(doc.layers[0].pixels, rgba);
    doc.validate_mask_storage().unwrap();
}

#[test]
fn raster_mask_structural_history_moves_dense_buffers_and_preserves_allocated_ids() {
    let mut doc = document(65, 67);
    doc.try_add_layer().unwrap();
    let first = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let dense: Vec<_> = (0..65 * 67).map(|index| (index * 19) as u8).collect();
    doc.replace_mask_plane(first, SamplePlane::Dense(dense.clone()))
        .unwrap();
    let original = match doc.mask_plane(first).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!(),
    };
    let second = doc.create_mask(MaskOwner::Layer { id: 2 }, 91).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: first })
        .unwrap();
    doc.delete_mask(first).unwrap();
    assert_eq!(doc.mask_state().target, PaintTarget::LayerPixels);
    doc.undo();
    assert_eq!(doc.mask_state().target, PaintTarget::Mask { id: first });
    assert!(
        matches!(doc.mask_plane(first).unwrap(),PlaneView::Dense(samples) if samples.as_ptr()==original)
    );
    assert_eq!(bytes(&doc, first), dense);
    doc.redo();
    assert!(doc.mask_plane(first).is_err());
    doc.undo();
    doc.undo();
    assert!(doc.mask_plane(second).is_err());
    assert!(doc.mask_state().next_id > second);
    let next = doc.create_mask(MaskOwner::Layer { id: 2 }, 44).unwrap();
    assert!(next > second);
    assert_eq!(bytes(&doc, first), dense);
}

#[test]
fn raster_mask_preflight_and_explicit_pixel_selection_preserve_contact_boundaries() {
    let mut doc = document(17, 11);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let before = doc.composition();
    let history = doc.history_bytes();
    assert!(doc.create_mask(MaskOwner::Layer { id: 1 }, 0).is_err());
    assert!(doc.create_mask(MaskOwner::Group { id: 1 }, 0).is_err());
    let mut properties = doc.mask_state().items[0].clone();
    properties.density = 128;
    assert!(doc.update_mask_metadata(properties).is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.history_bytes(), history);
    doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    let selected_history = doc.undo.len();
    doc.set_layer_selection(&[1], None).unwrap();
    assert_eq!(doc.mask_state().target, PaintTarget::Mask { id });
    doc.set_layer_selection(&[1], Some(1)).unwrap();
    assert_eq!(doc.mask_state().target, PaintTarget::LayerPixels);
    doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    doc.select_layer(0);
    assert_eq!(doc.mask_state().target, PaintTarget::LayerPixels);
    assert_eq!(doc.undo.len(), selected_history);
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert!(doc.delete_mask(id).is_err());
    assert!(doc
        .replace_mask_plane(id, SamplePlane::Uniform(12))
        .is_err());
    assert!(doc.set_mask_target(PaintTarget::Mask { id }).is_err());
    doc.mask_region(lease, 1, 1, 1, 1).unwrap();
    doc.rollback_plugin_stroke();
    assert!(bytes(&doc, id).iter().all(|sample| *sample == 255));
}

#[test]
fn raster_mask_verified_restore_reuses_dense_storage_without_rendering_or_intermediate_planes() {
    let mut doc = document(67, 65);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let lease = doc.begin_mask_stroke(id).unwrap();
    let region = doc.mask_region(lease, 2, 3, 1, 1).unwrap();
    unsafe {
        *region.data = 11;
    }
    let composition = doc.composition();
    let original = bytes(&doc, id);
    let checkpoint = doc.checkpoint_history();
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!(),
    };
    let next = doc.mask_region(lease, 20, 20, 1, 1).unwrap();
    unsafe {
        *next.data = 4;
    }
    doc.restore_composition(composition.clone());
    doc.restore_mask_storage(
        &composition.raster_masks,
        |_| None,
        |actual, samples| {
            assert_eq!(actual, id);
            samples.copy_from_slice(&original);
        },
    );
    doc.restore_history(checkpoint);
    assert!(
        matches!(doc.mask_plane(id).unwrap(),PlaneView::Dense(samples) if samples.as_ptr()==pointer)
    );
    assert_eq!(bytes(&doc, id), original);
    doc.mask_region(lease, 22, 22, 1, 1).unwrap();
    doc.finish();
}

#[test]
fn raster_mask_lease_nonce_idle_checkpoint_cannot_resurrect_an_abandoned_contact() {
    let mut doc = document(23, 17);
    let rgba = doc.layers[0].pixels.clone();
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let composition = doc.composition();
    let checkpoint = doc.checkpoint_history();
    let abandoned = doc.begin_mask_stroke(id).unwrap();
    let output = doc.mask_region(abandoned, 2, 3, 1, 1).unwrap();
    unsafe {
        *output.data = 37;
    }
    doc.rollback_plugin_stroke();
    // Match verified-state + history restoration at a failed Session contact.
    doc.restore_composition(composition.clone());
    doc.restore_mask_storage(
        &composition.raster_masks,
        |_| Some(255),
        |_, _| panic!("Uniform fixture"),
    );
    doc.restore_history(checkpoint);
    assert!(doc.mask_contact.is_none());
    let current = doc.begin_mask_stroke(id).unwrap();
    assert!(
        doc.mask_region(abandoned, 1, 1, 1, 1).is_err(),
        "Idle rollback reused the abandoned contact lease"
    );
    assert_ne!(current.storage_epoch, abandoned.storage_epoch);
    let output = doc.mask_region(current, 4, 5, 1, 1).unwrap();
    unsafe {
        *output.data = 91;
    }
    doc.finish();
    assert_eq!(doc.mask_plane(id).unwrap().sample(5 * 23 + 4), 91);
    doc.undo();
    assert_eq!(bytes(&doc, id), vec![255; 23 * 17]);
    assert_eq!(doc.layers[0].pixels, rgba);
}

#[test]
fn raster_mask_lease_nonce_active_checkpoint_keeps_original_but_never_reuses_later_contact() {
    let mut doc = document(11, 9);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 100).unwrap();
    let original = doc.begin_mask_stroke(id).unwrap();
    let output = doc.mask_region(original, 1, 1, 1, 1).unwrap();
    unsafe {
        *output.data = 23;
    }
    let composition = doc.composition();
    let raw = bytes(&doc, id);
    let checkpoint = doc.checkpoint_history();
    doc.finish();
    let abandoned = doc.begin_mask_stroke(id).unwrap();
    let output = doc.mask_region(abandoned, 2, 2, 1, 1).unwrap();
    unsafe {
        *output.data = 44;
    }
    doc.rollback_plugin_stroke();
    doc.restore_composition(composition.clone());
    doc.restore_mask_storage(
        &composition.raster_masks,
        |_| None,
        |_, samples| samples.copy_from_slice(&raw),
    );
    doc.restore_history(checkpoint);
    // An active checkpoint intentionally retains its already cached lease.
    let output = doc.mask_region(original, 3, 3, 1, 1).unwrap();
    unsafe {
        *output.data = 55;
    }
    assert!(doc.mask_region(abandoned, 2, 2, 1, 1).is_err());
    doc.finish();
    let current = doc.begin_mask_stroke(id).unwrap();
    assert!(
        doc.mask_region(abandoned, 2, 2, 1, 1).is_err(),
        "Active checkpoint rollback reused a later abandoned lease"
    );
    assert!(doc.mask_region(original, 1, 1, 1, 1).is_err());
    assert_ne!(current.storage_epoch, abandoned.storage_epoch);
    doc.mask_region(current, 4, 4, 1, 1).unwrap();
    doc.finish();
}

#[test]
fn raster_mask_project_roundtrip_has_bounded_binary_planes_and_retained_allocator_opt_in() {
    let file = path("roundtrip");
    let mut doc = document(513, 257);
    doc.try_add_layer().unwrap();
    let first = doc.create_mask(MaskOwner::Layer { id: 1 }, 17).unwrap();
    let second = doc.create_mask(MaskOwner::Layer { id: 2 }, 255).unwrap();
    let mut random = 0x5eed1234u32;
    let dense: Vec<_> = (0..513 * 257)
        .map(|_| {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            (random >> 24) as u8
        })
        .collect();
    doc.replace_mask_plane(second, SamplePlane::Dense(dense.clone()))
        .unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: second })
        .unwrap();
    doc.save(&file).unwrap();
    let encoded = std::fs::read(&file).unwrap();
    assert_eq!(&encoded[..8], b"MICOPI04");
    let offset = 24 + (2 + 513 * 257 * 4) * 2;
    let length = u32::from_le_bytes(encoded[offset..offset + 4].try_into().unwrap()) as usize;
    assert!(length < 8192, "Raw bytes must remain outside JSON");
    let loaded = Document::load(&file).unwrap();
    assert_eq!(loaded.mask_state(), doc.mask_state());
    assert!(matches!(
        loaded.mask_plane(first).unwrap(),
        PlaneView::Uniform(17)
    ));
    assert_eq!(bytes(&loaded, second), dense);
    doc.delete_mask(first).unwrap();
    doc.delete_mask(second).unwrap();
    doc.save(&file).unwrap();
    assert_eq!(&std::fs::read(&file).unwrap()[..8], b"MICOPI04");
    let loaded = Document::load(&file).unwrap();
    assert!(!loaded.has_raster_masks() && loaded.mask_storage_used());
    assert_eq!(loaded.mask_state().next_id, doc.mask_state().next_id);
    let legacy = path("legacy");
    let mut plain = Document::new(7, 5);
    plain.save(&legacy).unwrap();
    assert_eq!(&std::fs::read(&legacy).unwrap()[..8], b"MICOPI02");
    assert!(Document::load(&legacy).unwrap().mask_state().is_default());
    std::fs::remove_file(file).unwrap();
    std::fs::remove_file(legacy).unwrap();
}

#[test]
fn raster_mask_project_canonicalizes_constant_dense_only_during_cold_write() {
    let mut doc = document(65, 67);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 173).unwrap();
    let mut uniform = Vec::new();
    doc.write_mask_project_suffix(&mut uniform, None).unwrap();
    doc.mask_planes[0].samples = SamplePlane::Dense(vec![173; 65 * 67]);
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!(),
    };
    let metadata = doc.composition();
    let dirty = doc.dirty;
    let history = doc.history_bytes();
    let mut dense = Vec::new();
    doc.write_mask_project_suffix(&mut dense, None).unwrap();
    assert_eq!(dense, uniform);
    assert_eq!(dense.len(), 4 + 8 + 1 + 4 + 1 + 8);
    assert!(
        matches!(doc.mask_plane(id).unwrap(), PlaneView::Dense(samples) if samples.as_ptr() == pointer)
    );
    assert_eq!(doc.composition(), metadata);
    assert_eq!(doc.dirty, dirty);
    assert_eq!(doc.history_bytes(), history);
    let uniform_file = path("uniform-encoding");
    let dense_file = path("dense-encoding");
    doc.mask_planes[0].samples = SamplePlane::Uniform(173);
    doc.save(&uniform_file).unwrap();
    doc.mask_planes[0].samples = SamplePlane::Dense(vec![173; 65 * 67]);
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!(),
    };
    doc.save(&dense_file).unwrap();
    assert_eq!(
        std::fs::read(&uniform_file).unwrap(),
        std::fs::read(&dense_file).unwrap()
    );
    assert!(
        matches!(doc.mask_plane(id).unwrap(), PlaneView::Dense(samples) if samples.as_ptr() == pointer)
    );
    assert!(!doc.dirty);
    assert_eq!(doc.history_bytes(), history);
    std::fs::remove_file(uniform_file).unwrap();
    std::fs::remove_file(dense_file).unwrap();
}

#[test]
fn raster_mask_project_rejects_malformed_directory_trailing_bytes_and_failed_save_before_replacement(
) {
    let file = path("strict");
    let corrupt = path("corrupt");
    let mut doc = document(7, 5);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 29).unwrap();
    doc.save_with_replay(&file, Some(b"bounded archive payload"))
        .unwrap();
    let valid = std::fs::read(&file).unwrap();
    let offset = 24 + (2 + 7 * 5 * 4);
    let metadata = u32::from_le_bytes(valid[offset..offset + 4].try_into().unwrap()) as usize;
    let suffix = offset + 4 + metadata;
    for change in 0..8 {
        let mut invalid = valid.clone();
        match change {
            0 => invalid[suffix..suffix + 4].copy_from_slice(&0u32.to_le_bytes()),
            1 => invalid[suffix + 4..suffix + 12].copy_from_slice(&99u64.to_le_bytes()),
            2 => invalid[suffix + 12] = 2,
            3 => invalid[suffix + 13..suffix + 17].copy_from_slice(&0u32.to_le_bytes()),
            4 => invalid[suffix + 13..suffix + 17].copy_from_slice(&35u32.to_le_bytes()),
            5 => invalid.push(0),
            6 => invalid.truncate(suffix + 17),
            _ => invalid[..8].copy_from_slice(b"MICOPI03"),
        }
        std::fs::write(&corrupt, invalid).unwrap();
        assert!(Document::load(&corrupt).is_err(), "malformation {change}");
    }
    let loaded = Document::load(&file).unwrap();
    assert_eq!(
        loaded.replay_data.as_deref(),
        Some(&b"bounded archive payload"[..])
    );
    doc.mask_planes[0].samples = SamplePlane::Dense(vec![1]);
    assert!(doc.save(&file).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), valid);
    assert_eq!(id, 1);
    std::fs::remove_file(file).unwrap();
    std::fs::remove_file(corrupt).unwrap();
}
