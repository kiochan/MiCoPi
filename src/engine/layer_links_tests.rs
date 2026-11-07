use super::*;
use crate::engine::groups::GroupItem;
use crate::engine::raster_masks::PaintTarget;

struct MetadataRenderer;
impl Renderer for MetadataRenderer {
    fn supports_transparent(&self) -> bool {
        true
    }
    fn supports_mix(&self) -> bool {
        true
    }
    fn supports_blend_mode(&self, _: u32) -> bool {
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
        Ok(())
    }
}
fn fixture(count: usize) -> Document {
    let mut doc = Document::new(2, 1);
    doc.renderer = Some(Arc::new(MetadataRenderer));
    for _ in 1..count {
        doc.try_add_layer().unwrap();
    }
    for (i, layer) in doc.layers.iter_mut().enumerate() {
        layer.pixels = vec![0xff000001 + i as u32, 0x00102030 + i as u32];
    }
    doc.undo.clear();
    doc.redo.clear();
    doc.replay_tiles.clear();
    doc.dirty = false;
    doc
}
fn links(ids: &[u64]) -> Option<LayerLinks> {
    Some(LayerLinks {
        version: 1,
        layer_ids: ids.to_vec(),
    })
}

#[test]
fn layer_links_default_bytes_are_absent_and_versioned_membership_is_strict() {
    let doc = fixture(2);
    let old = serde_json::to_value(&doc.layer_metadata[0]).unwrap();
    assert_eq!(old, serde_json::json!({"id":1,"name":""}));
    let mut metadata = doc.layer_metadata.clone();
    for item in &mut metadata {
        item.links = links(&[1, 2]);
    }
    let value = serde_json::to_value(&metadata).unwrap();
    assert_eq!(
        value[0]["links"],
        serde_json::json!({"version":1,"layer_ids":[1,2]})
    );
    assert_eq!(
        serde_json::from_value::<Vec<LayerMetadata>>(value).unwrap(),
        metadata
    );
    for field in ["unknown", "revision"] {
        let mut value =
            serde_json::json!({"id":1,"name":"","links":{"version":1,"layer_ids":[1,2]}});
        value["links"][field] = serde_json::json!(0);
        assert!(serde_json::from_value::<LayerMetadata>(value).is_err());
    }
    assert!(serde_json::from_value::<LayerMetadata>(serde_json::json!({
        "id":1,"name":"","links":{"version":1,"layer_ids":[1,2.0]}
    }))
    .is_err());
}

#[test]
fn layer_links_reject_missing_asymmetric_overlapping_noncanonical_or_unknown_membership() {
    let doc = fixture(4);
    let mut metadata = doc.layer_metadata.clone();
    for item in &mut metadata {
        item.links = links(&[1, 2, 3, 4]);
    }
    validate_layer_links(&metadata).unwrap();
    for members in [
        vec![],
        vec![1],
        vec![2, 1],
        vec![1, 1],
        vec![1, 5],
        vec![0, 1],
        vec![1, u32::MAX as u64 + 1],
    ] {
        let mut invalid = metadata.clone();
        invalid[0].links = links(&members);
        assert!(validate_layer_links(&invalid).is_err(), "{members:?}");
    }
    let mut invalid = metadata.clone();
    invalid[0].links.as_mut().unwrap().version = 2;
    assert!(validate_layer_links(&invalid).is_err());
    let mut invalid = metadata.clone();
    invalid[1].links = None;
    assert!(validate_layer_links(&invalid).is_err());
    let mut invalid = metadata;
    invalid[0].links = links(&[1, 2]);
    invalid[1].links = links(&[1, 2]);
    assert!(validate_layer_links(&invalid).is_err());
    let mut state = doc.composition();
    state.layer_metadata = invalid;
    assert!(state.validate_layer_metadata().is_err());
}

#[test]
fn layer_links_union_only_touched_sets_and_unlink_projects_residual_members() {
    let mut doc = fixture(6);
    assert!(doc.set_layer_links(&[2, 1], true).unwrap());
    assert!(doc.set_layer_links(&[4, 3], true).unwrap());
    assert!(doc.set_layer_links(&[6, 5], true).unwrap());
    assert_eq!(
        doc.layer_link_sets(),
        vec![vec![1, 2], vec![3, 4], vec![5, 6]]
    );
    assert!(doc.set_layer_links(&[2, 3], true).unwrap());
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3, 4], vec![5, 6]]);
    assert!(!doc.set_layer_links(&[4, 1], true).unwrap());
    assert!(doc.set_layer_links(&[2], false).unwrap());
    assert_eq!(doc.linked_layer_ids(2).unwrap(), vec![2]);
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 3, 4], vec![5, 6]]);
    assert!(doc.set_layer_links(&[1, 3, 5], false).unwrap());
    assert_eq!(doc.layer_link_sets(), Vec::<Vec<u64>>::new());
    assert!(!doc.set_layer_links(&[4], false).unwrap());
}

#[test]
fn layer_links_metadata_undo_preserves_context_raw_buffers_and_damage() {
    let mut doc = fixture(3);
    doc.set_layer_selection(&[1, 3], Some(1)).unwrap();
    let context = doc.composition();
    let pointers: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect();
    assert!(doc.set_layer_links(&[3, 2], true).unwrap());
    let linked = doc.composition();
    assert_eq!(doc.active, 0);
    assert_eq!(doc.selected_layer_ids(), vec![1, 3]);
    assert_eq!(doc.raster_masks.target, PaintTarget::LayerPixels);
    assert!(doc.replay_tiles.is_empty());
    assert_eq!(doc.undo.len(), 1);
    assert!(doc.undo[0].tiles.is_empty());
    assert!(doc.undo[0].mask_change.is_none());
    assert!(
        doc.history_bytes()
            >= context
                .layer_metadata
                .iter()
                .map(LayerMetadata::memory_bytes)
                .sum::<usize>()
    );
    doc.undo();
    assert_eq!(doc.composition(), context);
    doc.redo();
    assert_eq!(doc.composition(), linked);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
}

#[test]
fn layer_links_failures_do_not_finish_contacts_or_repair_invalid_source() {
    let mut doc = fixture(3);
    let before = doc.composition();
    for (ids, linked) in [
        (vec![], false),
        (vec![1], true),
        (vec![1, 1], true),
        (vec![1, 99], true),
    ] {
        assert!(doc.set_layer_links(&ids, linked).is_err());
    }
    doc.begin_plugin_stroke();
    assert!(doc
        .set_layer_links(&[1, 2], true)
        .unwrap_err()
        .contains("contact"));
    assert!(doc.stroke.is_some());
    doc.finish();
    assert_eq!(doc.composition(), before);
    assert!(doc.undo.is_empty());
    assert!(!doc.dirty);
    doc.layer_metadata[0].links = links(&[1, 2]);
    let invalid = doc.composition();
    assert!(doc.set_layer_links(&[1, 2], false).is_err());
    assert_eq!(doc.composition(), invalid);
    assert!(doc.linked_layer_ids(1).is_err());
    assert!(doc.linked_layer_ids(99).is_err());
}

#[test]
fn layer_links_projection_retains_only_original_survivors_and_rejects_bad_source() {
    let mut doc = fixture(4);
    doc.set_layer_links(&[1, 2, 3], true).unwrap();
    let mut candidate = vec![
        doc.layer_metadata[2].clone(),
        doc.layer_metadata[0].clone(),
        doc.layer_metadata[1].clone(),
    ];
    candidate[2].id = 5;
    project_layer_links(&doc.layer_metadata, &mut candidate).unwrap();
    assert_eq!(candidate[0].links, links(&[1, 3]));
    assert_eq!(candidate[1].links, links(&[1, 3]));
    assert_eq!(candidate[2].links, None);
    candidate.remove(0);
    project_layer_links(&doc.layer_metadata, &mut candidate).unwrap();
    assert!(candidate.iter().all(|item| item.links.is_none()));
    let mut bad = doc.layer_metadata.clone();
    bad[0].links = None;
    assert!(project_layer_links(&bad, &mut candidate).is_err());
}

#[test]
fn layer_links_full_sixteen_member_union_is_canonical_and_bounded() {
    let mut doc = fixture(MAX_LAYERS);
    let low: Vec<_> = (1..=8).collect();
    let high: Vec<_> = (9..=16).rev().collect();
    doc.set_layer_links(&low, true).unwrap();
    doc.set_layer_links(&high, true).unwrap();
    doc.set_layer_links(&[16, 1], true).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![(1..=16).collect::<Vec<_>>()]);
    for item in &doc.layer_metadata {
        assert_eq!(item.links, links(&(1..=16).collect::<Vec<_>>()));
    }
    let mut invalid = doc.layer_metadata.clone();
    invalid[0].links.as_mut().unwrap().layer_ids.push(17);
    assert!(validate_layer_links(&invalid).is_err());
}

#[test]
fn layer_links_reorder_copy_and_delete_use_stable_original_membership() {
    let mut doc = fixture(4);
    doc.set_layer_links(&[1, 2, 3], true).unwrap();
    doc.reorder_layers(&[3, 1, 4, 2]).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3]]);
    let source = doc.layer_index(1).unwrap();
    doc.duplicate_layer(source).unwrap();
    assert_eq!(doc.linked_layer_ids(5).unwrap(), vec![5]);
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3]]);
    let state = doc.composition();
    doc.delete_layer(doc.layer_index(2).unwrap()).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 3]]);
    doc.delete_layer(doc.layer_index(1).unwrap()).unwrap();
    assert!(doc.layer_link_sets().is_empty());
    doc.undo();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 3]]);
    doc.undo();
    assert_eq!(doc.composition(), state);
    doc.try_add_layer().unwrap();
    assert!(doc.layer_metadata.last().unwrap().links.is_none());
}

#[test]
fn layer_links_group_copy_delete_and_reparent_preserve_original_relationships() {
    let mut doc = fixture(4);
    doc.set_layer_links(&[1, 2, 3], true).unwrap();
    let group = doc
        .group_create(&[GroupItem::layer(2), GroupItem::layer(3)], None, None)
        .unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3]]);
    let copy = doc.group_duplicate(group).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3]]);
    assert!(doc
        .group_layer_ids(copy)
        .unwrap()
        .iter()
        .all(|id| doc.linked_layer_ids(*id).unwrap() == vec![*id]));
    doc.group_delete(group).unwrap();
    assert!(doc.layer_link_sets().is_empty());
    assert_eq!(doc.linked_layer_ids(1).unwrap(), vec![1]);
    doc.undo();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 2, 3]]);
}

#[test]
fn layer_links_merge_retains_only_surviving_ids_and_group_replacement_is_unlinked() {
    let mut doc = fixture(4);
    doc.set_layer_links(&[1, 2, 4], true).unwrap();
    doc.merge_layers(MergeLayers::Down, 1).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 4]]);
    doc.undo();
    doc.merge_selected(&[2, 3]).unwrap();
    assert_eq!(doc.layer_link_sets(), vec![vec![1, 4]]);
    doc.undo();
    let group = doc
        .group_create(&[GroupItem::layer(1), GroupItem::layer(2)], None, None)
        .unwrap();
    let fresh = doc.group_merge(group).unwrap();
    assert!(doc.layer_link_sets().is_empty());
    assert_eq!(doc.linked_layer_ids(fresh).unwrap(), vec![fresh]);
}

#[test]
fn layer_links_metadata_round_trip_uses_existing_composition_serialization() {
    let mut doc = fixture(3);
    doc.set_layer_links(&[3, 1], true).unwrap();
    let encoded = serde_json::to_vec(&doc.composition()).unwrap();
    let decoded: Composition = serde_json::from_slice(&encoded).unwrap();
    decoded.validate_layer_metadata().unwrap();
    assert_eq!(decoded, doc.composition());
    let mut restored = fixture(3);
    restored.restore_composition(decoded);
    assert_eq!(restored.layer_link_sets(), vec![vec![1, 3]]);
    assert_eq!(restored.composition(), doc.composition());
}
