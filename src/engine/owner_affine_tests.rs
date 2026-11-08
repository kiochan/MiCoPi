use super::*;
use crate::engine::{
    groups::GroupItem,
    raster_masks::{PaintTarget, PlaneView},
    selection::{Mask, SelectionState},
    transform::Method,
};
use serde_json::json;

struct StorageRenderer;
impl Renderer for StorageRenderer {
    fn supports_masks(&self) -> bool {
        true
    }
    fn supports_mask_effects(&self) -> bool {
        true
    }
    fn supports_transparent(&self) -> bool {
        true
    }
    fn supports_mix(&self) -> bool {
        true
    }
    fn supports_blend_mode(&self, _: u32) -> bool {
        true
    }
    fn mask_effect_region(
        &self,
        _: &micopi_plugin_api::MaskEffectRequest,
        _: &micopi_plugin_api::CompositeMask,
        _: u32,
        _: u32,
        _: &mut micopi_plugin_api::A8Pixels,
    ) -> Result<(), String> {
        // Only the disabled unrelated effect is warmed by the contact fixture;
        // these storage tests do not consume or claim its derived pixel values.
        Ok(())
    }
    fn render(
        &self,
        _: &[micopi_plugin_api::CompositeNode],
        _: &[micopi_plugin_api::CompositeLayer],
        _: u32,
        _: u32,
        _: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        panic!("Owner publication must not render")
    }
}
fn fixture() -> Document {
    let mut doc = Document::new(3, 2);
    doc.renderer = Some(Arc::new(StorageRenderer));
    doc.try_add_layer().unwrap();
    doc.try_add_layer().unwrap();
    let group = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    for owner in [
        MaskOwner::Layer { id: 1 },
        MaskOwner::Layer { id: 2 },
        MaskOwner::Group { id: group },
        MaskOwner::Layer { id: 3 },
    ] {
        let id = doc.create_mask(owner, 91).unwrap();
        if id == 1 || id == 4 {
            doc.replace_mask_plane(
                id,
                SamplePlane::Dense((0..6).map(|i| i * 11 + id as u8).collect()),
            )
            .unwrap();
        }
    }
    let mut item = doc.raster_masks.items[0].clone();
    item.enabled = false;
    item.density = 173;
    item.feather_radius_px = 1;
    doc.update_mask_metadata(item).unwrap();
    let mut item = doc.raster_masks.items[1].clone();
    item.linked = false;
    doc.update_mask_metadata(item).unwrap();
    doc.layers[0].pixels = vec![
        0x00123456, 0xff123456, 0x80123456, 0xffabcdef, 0x40010203, 0,
    ];
    doc.layers[0].opacity = 137;
    doc.layers[1].pixels = vec![0xff223344; 6];
    doc.layers[2].pixels = vec![0xff556677; 6];
    doc.set_layer_selection(&[1, 3], Some(3)).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: 4 }).unwrap();
    doc.selection = SelectionState {
        current: Some(Mask::new(3, 2, vec![97; 6]).unwrap()),
        previous: Some(Mask::new(3, 2, vec![173; 6]).unwrap()),
    };
    doc.last_transform = Some(Recipe {
        matrix: [1., 0., 2., 0., 1., 0.],
        denominator: [0., 0., 1.],
        target: Target::Layer,
        method: Method::Nearest,
        owners: None,
    });
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_tiles.clear();
    doc.replay_mask_tiles.clear();
    doc.dirty = false;
    doc
}
fn recipe(scope: OwnerScope) -> Recipe {
    Recipe {
        matrix: [1., 0., 1., 0., 1., 0.],
        denominator: [0., 0., 1.],
        target: Target::Owners,
        method: Method::Nearest,
        owners: Some(scope),
    }
}
fn raw(doc: &Document, id: u64) -> Vec<u8> {
    let view = doc.mask_plane(id).unwrap();
    (0..6).map(|i| view.sample(i)).collect()
}
fn pointer(doc: &Document, id: u64) -> *const u8 {
    let PlaneView::Dense(values) = doc.mask_plane(id).unwrap() else {
        panic!("Expected dense plane")
    };
    values.as_ptr()
}
fn unchanged(
    doc: &Document,
    scope: &OwnerScope,
) -> Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)> {
    scope
        .layer_ids()
        .iter()
        .map(|id| {
            let index = doc.layer_index(*id).unwrap();
            let mask = doc
                .raster_masks
                .items
                .iter()
                .find(|mask| mask.linked && mask.owner == (MaskOwner::Layer { id: *id }))
                .map(|mask| {
                    (
                        mask.id,
                        doc.mask_planes
                            .iter()
                            .find(|plane| plane.id == mask.id)
                            .unwrap()
                            .samples
                            .clone(),
                    )
                });
            (*id, doc.layers[index].pixels.clone(), mask)
        })
        .collect()
}

#[test]
fn owner_recipe_scope_keeps_copy_and_exact_legacy_bytes_with_strict_bounded_wire() {
    fn copied<T: Copy>(value: T) -> T {
        value
    }
    let legacy = Recipe {
        matrix: [1., 0., 1., 0., 1., 0.],
        denominator: [0., 0., 1.],
        target: Target::Layer,
        method: Method::Nearest,
        owners: None,
    };
    let wire = r#"{"matrix":[1.0,0.0,1.0,0.0,1.0,0.0],"target":"layer","method":"nearest"}"#;
    assert_eq!(serde_json::to_string(&copied(legacy)).unwrap(), wire);
    assert_eq!(serde_json::from_str::<Recipe>(wire).unwrap(), legacy);
    let scope = OwnerScope::new(&[2, 1], &[1], None).unwrap();
    let value = json!(copied(recipe(scope)));
    assert_eq!(
        value["owners"],
        json!({"version":1,"layer_ids":[2,1],"mask_ids":[1]})
    );
    assert_eq!(
        serde_json::from_value::<Recipe>(value).unwrap(),
        recipe(scope)
    );
    let mask = OwnerScope::new(&[], &[7], Some(4)).unwrap();
    assert_eq!(
        json!(mask),
        json!({"version":1,"layer_ids":[],"mask_ids":[7],"mask_owner_layer_id":4})
    );
    assert!(std::mem::size_of::<OwnerScope>() <= 320 && std::mem::size_of::<Recipe>() <= 416);
}

#[test]
fn owner_recipe_scope_rejects_unknown_version_fields_bad_ids_and_unsupported_target_shapes() {
    for value in [
        json!({"version":2,"layer_ids":[1],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[1,1],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[0],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[4294967296u64],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[1.0],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[1],"mask_ids":[2,3]}),
        json!({"version":1,"layer_ids":[],"mask_ids":[]}),
        json!({"version":1,"layer_ids":[],"mask_ids":[1]}),
        json!({"version":1,"layer_ids":[],"mask_ids":[1,2],"mask_owner_layer_id":1}),
        json!({"version":1,"layer_ids":[1],"mask_ids":[],"mask_owner_layer_id":1}),
        json!({"version":1,"layer_ids":[1],"mask_ids":[],"unregistered":true}),
        json!({"version":1,"layer_ids":(1..=17).collect::<Vec<_>>(),"mask_ids":[]}),
    ] {
        assert!(
            serde_json::from_value::<OwnerScope>(value.clone()).is_err(),
            "{value}"
        );
    }
    let mut value = json!(recipe(OwnerScope::new(&[1], &[], None).unwrap()));
    value["target"] = json!("layer");
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    let mut value = json!(recipe(OwnerScope::new(&[1], &[], None).unwrap()));
    value.as_object_mut().unwrap().remove("owners");
    assert!(serde_json::from_value::<Recipe>(value).is_err());
    let mut projective = recipe(OwnerScope::new(&[1], &[], None).unwrap());
    projective.denominator = [0.001, 0., 1.];
    assert!(projective.validate().is_err());
}

#[test]
fn owner_transform_read_capture_preserves_active_rgba_and_a8_contacts() {
    let mut doc = fixture();
    doc.begin_plugin_stroke();
    let before = doc.composition();
    let epoch = doc.mask_storage_epoch;
    let source = doc.layers[0].pixels.as_ptr();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    assert_eq!(scope.layer_ids(), &[2, 1]);
    assert_eq!(scope.mask_ids(), &[1]);
    assert_eq!(
        doc.validate_owner_transform_scope(&scope).unwrap(),
        vec![(2, None), (1, Some(1))]
    );
    assert!(doc.stroke.is_some());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert_eq!(doc.layers[0].pixels.as_ptr(), source);
    assert!(doc
        .apply_owner_transform(unchanged(&doc, &scope), None, recipe(scope))
        .unwrap_err()
        .contains("contact"));
    assert!(doc.stroke.is_some());
    doc.rollback_plugin_stroke();
    let lease = doc.begin_mask_stroke(4).unwrap();
    let epoch = doc.mask_storage_epoch;
    let mask_scope = doc.capture_owner_transform_scope(&[], Some(4)).unwrap();
    assert_eq!(mask_scope.mask_owner_layer_id(), Some(3));
    doc.validate_owner_transform_scope(&mask_scope).unwrap();
    assert_eq!(doc.mask_contact, Some(lease));
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc
        .apply_owner_transform(
            vec![],
            Some((4, SamplePlane::Uniform(255))),
            recipe(mask_scope)
        )
        .is_err());
    assert_eq!(doc.mask_contact, Some(lease));
    doc.rollback_plugin_stroke();
}

#[test]
fn owner_transform_repeat_keeps_exact_ids_across_changed_raster_links_and_active_selection() {
    let mut doc = fixture();
    doc.set_layer_links(&[1, 2, 3], true).unwrap();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    doc.set_layer_links(&[2], false).unwrap();
    doc.set_layer_selection(&[2, 3], Some(2)).unwrap();
    assert_eq!(
        doc.validate_owner_transform_scope(&scope).unwrap(),
        vec![(2, None), (1, Some(1))]
    );
    assert_eq!(scope.layer_ids(), &[2, 1]);
    assert!(doc.capture_owner_transform_scope(&[1], Some(1)).is_err());
    assert!(doc.capture_owner_transform_scope(&[], Some(3)).is_err());
}

#[test]
fn owner_transform_repeat_rejects_changed_mask_companions_and_captured_mask_owner() {
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[1], None).unwrap();
    let mut item = doc.raster_masks.items[0].clone();
    item.linked = false;
    doc.update_mask_metadata(item).unwrap();
    assert!(doc
        .validate_owner_transform_scope(&scope)
        .unwrap_err()
        .contains("pairings"));
    let scope = doc.capture_owner_transform_scope(&[2], None).unwrap();
    let mut item = doc.raster_masks.items[1].clone();
    item.linked = true;
    doc.update_mask_metadata(item).unwrap();
    assert!(doc.validate_owner_transform_scope(&scope).is_err());
    let scope = doc.capture_owner_transform_scope(&[], Some(2)).unwrap();
    doc.raster_masks.items[1].owner = MaskOwner::Layer { id: 3 };
    doc.raster_masks.items[3].owner = MaskOwner::Layer { id: 2 };
    assert!(doc
        .validate_owner_transform_scope(&scope)
        .unwrap_err()
        .contains("owner"));
}

#[test]
fn owner_transform_raw_pairs_and_repeat_metadata_share_one_undo_without_unrelated_copies() {
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    let next = recipe(scope);
    let before = doc.composition();
    let rgba: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let a8 = raw(&doc, 1);
    let fixed = doc.layers[2].pixels.as_ptr();
    let fixed_mask = pointer(&doc, 4);
    let epoch = doc.mask_storage_epoch;
    let outputs = vec![
        (2, vec![0xffabcdef; 6], None),
        (1, vec![0xff123456; 6], Some((1, SamplePlane::Uniform(255)))),
    ];
    assert!(doc.apply_owner_transform(outputs, None, next).unwrap());
    let mut expected = before.clone();
    expected.last_transform = Some(next);
    assert_eq!(doc.composition(), expected);
    assert_eq!(doc.layers[0].pixels, vec![0xff123456; 6]);
    assert_eq!(doc.layers[1].pixels, vec![0xffabcdef; 6]);
    assert_eq!(doc.layers[2].pixels.as_ptr(), fixed);
    assert_eq!(pointer(&doc, 4), fixed_mask);
    assert_ne!(doc.mask_storage_epoch, epoch);
    assert_eq!(doc.undo.len(), 1);
    let group = doc.undo[0].group.as_ref().unwrap();
    assert_eq!(group.edits.len(), 3);
    assert!(group.edits[..2]
        .iter()
        .all(|edit| edit.composition.is_none()));
    assert!(group.edits[2].composition.is_some());
    assert_eq!(
        doc.replay_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(0, 0), (1, 0)]
    );
    assert_eq!(
        doc.replay_mask_tiles.iter().copied().collect::<Vec<_>>(),
        vec![(1, 0)]
    );
    doc.undo();
    assert_eq!(doc.composition(), before);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        rgba
    );
    assert_eq!(raw(&doc, 1), a8);
    doc.redo();
    assert_eq!(doc.composition(), expected);
    assert_eq!(doc.layers[0].pixels, vec![0xff123456; 6]);
    assert_eq!(doc.layers[1].pixels, vec![0xffabcdef; 6]);
    assert_eq!(doc.layers[2].pixels.as_ptr(), fixed);
    assert_eq!(pointer(&doc, 4), fixed_mask);
}

#[test]
fn deformation_publication_keeps_affine_recipe_and_groups_raw_owners_once() {
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    let before = doc.composition();
    let rgba: Vec<_> = doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let mask = raw(&doc, 1);
    let outputs = vec![
        (2, vec![0xffabcdef; 6], None),
        (1, vec![0xff123456; 6], Some((1, SamplePlane::Uniform(255)))),
    ];
    assert!(doc.apply_owner_deformation(outputs, scope).unwrap());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.undo.len(), 1);
    assert!(doc.undo[0]
        .group
        .as_ref()
        .unwrap()
        .edits
        .iter()
        .all(|e| e.composition.is_none()));
    doc.undo();
    assert_eq!(doc.composition(), before);
    assert_eq!(
        doc.layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        rgba
    );
    assert_eq!(raw(&doc, 1), mask);
    doc.redo();
    assert_eq!(doc.layers[0].pixels, vec![0xff123456; 6]);
    assert_eq!(doc.layers[1].pixels, vec![0xffabcdef; 6]);
    assert_eq!(doc.composition(), before);
}

#[test]
fn owner_transform_mask_only_retains_other_raw_domains_and_ignores_owner_pixel_alpha_locks() {
    let mut doc = fixture();
    doc.layer_metadata[1].pixel_locked = true;
    doc.layer_metadata[1].alpha_locked = true;
    let scope = doc.capture_owner_transform_scope(&[], Some(2)).unwrap();
    let next = recipe(scope);
    let before = doc.composition();
    let pointers: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect();
    let fixed = pointer(&doc, 1);
    assert_eq!(doc.owner_transform_staging_bytes(&scope).unwrap(), 6);
    assert!(doc
        .apply_owner_transform(vec![], Some((2, SamplePlane::Uniform(255))), next)
        .unwrap());
    let mut expected = before.clone();
    expected.last_transform = Some(next);
    assert_eq!(doc.composition(), expected);
    assert_eq!(raw(&doc, 2), vec![255; 6]);
    assert_eq!(pointer(&doc, 1), fixed);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    doc.undo();
    assert_eq!(doc.composition(), before);
    assert_eq!(raw(&doc, 2), vec![91; 6]);
    doc.redo();
    assert_eq!(raw(&doc, 2), vec![255; 6]);
    doc.layer_metadata[1].position_locked = true;
    assert!(doc
        .validate_owner_transform_scope(&scope)
        .unwrap_err()
        .contains("position"));
    doc.layer_metadata[1].position_locked = false;
    doc.group_set(1, None, Some(false), None, None, None)
        .unwrap();
    assert!(doc
        .validate_owner_transform_scope(&scope)
        .unwrap_err()
        .contains("visible"));
}

#[test]
fn owner_transform_equal_outputs_keep_recipe_changes_undoable_and_exact_repeats_noop() {
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    let next = recipe(scope);
    let old = doc.last_transform;
    let epoch = doc.mask_storage_epoch;
    assert!(doc
        .apply_owner_transform(unchanged(&doc, &scope), None, next)
        .unwrap());
    assert_eq!(doc.undo.len(), 1);
    assert_eq!(doc.undo[0].group.as_ref().unwrap().edits.len(), 1);
    assert!(doc.replay_tiles.is_empty() && doc.replay_mask_tiles.is_empty());
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(!doc
        .apply_owner_transform(unchanged(&doc, &scope), None, next)
        .unwrap());
    assert_eq!(doc.undo.len(), 1);
    doc.undo();
    assert_eq!(doc.last_transform, old);
    doc.redo();
    assert_eq!(doc.last_transform, Some(next));
}

#[test]
fn owner_transform_late_staging_failures_do_not_publish_any_raw_or_recipe_child() {
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[2, 1], None).unwrap();
    let next = recipe(scope);
    let before = doc.composition();
    let pixels: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let bytes = raw(&doc, 1);
    let epoch = doc.mask_storage_epoch;
    for case in 0..5 {
        let mut output = unchanged(&doc, &scope);
        output[0].1.fill(0xff123456);
        match case {
            0 => output[1].1.pop().map(|_| ()).unwrap(),
            1 => output[1].2 = Some((1, SamplePlane::Dense(vec![0; 5]))),
            2 => output[1].2 = Some((4, SamplePlane::Uniform(255))),
            3 => output.reverse(),
            _ => output.pop().map(|_| ()).unwrap(),
        }
        assert!(doc.apply_owner_transform(output, None, next).is_err());
        assert_eq!(doc.composition(), before);
        assert_eq!(
            doc.layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
        );
        assert_eq!(raw(&doc, 1), bytes);
        assert_eq!(doc.mask_storage_epoch, epoch);
        assert!(
            doc.undo.is_empty()
                && doc.history_group.is_none()
                && doc.replay_tiles.is_empty()
                && doc.replay_mask_tiles.is_empty()
        );
    }
    doc.layer_metadata[0].alpha_locked = true;
    let mut output = unchanged(&doc, &scope);
    output[1].1.fill(0);
    assert!(doc.apply_owner_transform(output, None, next).is_err());
    assert!(doc.undo.is_empty());
    assert_eq!(doc.last_transform, before.last_transform);
}

#[test]
fn owner_transform_rejects_open_history_group_and_keeps_stale_recipes_serializable_after_deletion()
{
    let mut doc = fixture();
    let scope = doc.capture_owner_transform_scope(&[1], None).unwrap();
    let next = recipe(scope);
    doc.begin_history_group().unwrap();
    doc.validate_owner_transform_scope(&scope).unwrap();
    assert!(doc
        .apply_owner_transform(unchanged(&doc, &scope), None, next)
        .is_err());
    assert!(doc.history_group.is_some());
    doc.end_history_group().unwrap();
    assert!(doc.undo.is_empty());
    doc.apply_owner_transform(unchanged(&doc, &scope), None, next)
        .unwrap();
    doc.delete_layer(doc.layer_index(1).unwrap()).unwrap();
    doc.composition().validate_layer_metadata().unwrap();
    let stored: Composition = serde_json::from_value(json!(doc.composition())).unwrap();
    stored.validate_layer_metadata().unwrap();
    assert_eq!(stored.last_transform, Some(next));
    assert!(doc.validate_owner_transform_scope(&scope).is_err());
    doc.undo();
    doc.validate_owner_transform_scope(&scope).unwrap();
    assert_eq!(doc.last_transform, Some(next));
}

#[test]
fn owner_transform_staging_limit_counts_full_dense_rgba_and_a8_without_allocating_frames() {
    let mut doc = Document::new(1, 1);
    doc.width = 4096;
    doc.height = 4096;
    let three = OwnerScope::new(&[1, 2, 3], &[], None).unwrap();
    assert_eq!(
        doc.owner_transform_staging_bytes(&three).unwrap(),
        OWNER_MOVE_STAGING_LIMIT
    );
    let plus = OwnerScope::new(&[1, 2, 3], &[7], None).unwrap();
    assert!(doc.owner_transform_staging_bytes(&plus).is_err());
    let mask = OwnerScope::new(&[], &[7], Some(2)).unwrap();
    assert_eq!(
        doc.owner_transform_staging_bytes(&mask).unwrap(),
        16 * 1024 * 1024
    );
    doc.width = usize::MAX;
    assert!(doc.owner_transform_staging_bytes(&mask).is_err());
    assert_eq!(doc.layers[0].pixels.len(), 1);
}
