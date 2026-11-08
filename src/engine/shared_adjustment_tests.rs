//! Independent shared-target composition, ownership and metadata compatibility witnesses.
use super::*;
use crate::engine::{
    graph::Filter,
    groups::GroupItem,
    raster_masks::{MaskOwner, SamplePlane},
};
use serde_json::json;

fn fixture() -> Document {
    let mut doc = Document::new(2, 1);
    doc.renderer = Some(crate::plugins::Registry::bundled().renderer().unwrap());
    doc.try_add_layer().unwrap();
    doc.try_add_layer().unwrap();
    doc.layers[0].pixels = vec![0xff102030, 0xff102030];
    doc.layers[1].pixels = vec![0x806080a0, 0x806080a0];
    doc.layers[2].pixels = vec![0x4080a0c0, 0x4080a0c0];
    doc.invalidate();
    doc.compose();
    doc
}
fn invert(targets: Option<Vec<u64>>, opacity: u8) -> AdjustmentSpec {
    AdjustmentSpec {
        name: "Shared correction".into(),
        visible: true,
        opacity,
        node_type: "compositing.filter".into(),
        filter: Filter::Invert,
        amount: 1.,
        levels: None,
        curves: None,
        hsl: None,
        quantization: None,
        target_layer_ids: targets,
    }
}
fn words(doc: &Document) -> Vec<Vec<u32>> {
    doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
#[test]
fn shared_sources_are_filtered_separately_before_blending_and_preserve_raw_owners() {
    let mut doc = fixture();
    let raw = words(&doc);
    let pointers: Vec<_> = doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    let (id, _) = doc
        .create_adjustment(invert(Some(vec![3, 1]), 255), 0, 0, None)
        .unwrap();
    doc.compose();
    let expected = over(over(0xffefdfcf, 0x806080a0), 0x407f5f3f);
    assert_eq!(doc.composite, [expected; 2]);
    assert_eq!(words(&doc), raw);
    assert_eq!(
        doc.layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    assert!(!doc.adjustment_nodes.contains_key(&id));
    let tree = doc.groups.as_ref().unwrap();
    let graph = tree
        .compile_stack_details(
            &doc.graph,
            &[1, 2, 3],
            &doc.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
            &doc.layers.iter().map(|l| l.opacity).collect::<Vec<_>>(),
            doc.mask_state(),
            false,
        )
        .unwrap()
        .0;
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| matches!(
                n.data,
                NodeKind::Filter {
                    filter: Filter::Invert,
                    ..
                }
            ))
            .count(),
        2
    );
    doc.undo();
    doc.compose();
    assert_eq!(words(&doc), raw);
    assert_ne!(doc.composite, [expected; 2]);
    doc.redo();
    doc.compose();
    assert_eq!(doc.composite, [expected; 2]);
}
#[test]
fn root_controller_cross_group_targets_and_own_mask_mix_precede_owner_gates() {
    let mut doc = fixture();
    let group = doc
        .group_create(&[GroupItem::layer(3)], None, None)
        .unwrap();
    doc.group_set(group, None, None, Some(128), None, None)
        .unwrap();
    let (id, mask) = doc
        .create_adjustment(
            invert(Some(vec![3, 1]), 255),
            0,
            1,
            Some(SamplePlane::Dense(vec![0, 255])),
        )
        .unwrap();
    doc.compose();
    let lower = over(0xff102030, 0x806080a0);
    let corrected = over(0xffefdfcf, 0x806080a0);
    // Group opacity halves the source alpha after per-source correction exactly once.
    assert_eq!(
        doc.composite,
        [over(lower, 0x2080a0c0), over(corrected, 0x207f5f3f)]
    );
    assert_eq!(
        doc.mask_state()
            .items
            .iter()
            .find(|m| m.id == mask.unwrap())
            .unwrap()
            .owner,
        MaskOwner::Adjustment { id }
    );
    let before = doc.composition();
    let history = doc.history_bytes();
    assert!(doc.move_adjustment(id, group, 0).is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.history_bytes(), history);
}
#[test]
fn targeted_empty_is_inert_legacy_bytes_omit_binding_and_missing_ids_reject_atomically() {
    let mut doc = fixture();
    let pixels = doc.composite.clone();
    let legacy = invert(None, 255);
    assert!(json!(legacy).get("target_layer_ids").is_none());
    let (id, _) = doc
        .create_adjustment(invert(Some(vec![]), 255), 0, 3, None)
        .unwrap();
    doc.compose();
    assert_eq!(doc.composite, pixels);
    assert_eq!(doc.adjustment(id).unwrap().target_layer_ids, Some(vec![]));
    let before = doc.composition();
    let history = doc.history_bytes();
    for ids in [vec![1, 1], vec![0], vec![99]] {
        assert!(doc.set_adjustment(id, invert(Some(ids), 255)).is_err());
        assert_eq!(doc.composition(), before);
        assert_eq!(doc.history_bytes(), history);
    }
    doc.set_adjustment(id, invert(Some(vec![1]), 255)).unwrap();
    let before = doc.composition();
    let raw = words(&doc);
    assert!(doc.delete_layer(0).is_err());
    assert_eq!(doc.composition(), before);
    assert_eq!(words(&doc), raw);
    doc.set_adjustment(id, invert(Some(vec![]), 255)).unwrap();
    doc.delete_layer(0).unwrap();
}
#[test]
fn controller_id_order_is_stable_across_stack_moves_and_copy_is_not_implicitly_bound() {
    let mut doc = fixture();
    let (first, _) = doc
        .create_adjustment(invert(Some(vec![1]), 255), 0, 0, None)
        .unwrap();
    let mut brightness = invert(Some(vec![1]), 255);
    brightness.filter = Filter::Brightness;
    brightness.amount = 0.2;
    let (second, _) = doc.create_adjustment(brightness, 0, 4, None).unwrap();
    doc.compose();
    let expected = doc.composite.clone();
    doc.move_adjustment(first, 0, 4).unwrap();
    doc.compose();
    assert_eq!(doc.composite, expected);
    assert!(second > first);
    doc.duplicate_layer(0).unwrap();
    assert_eq!(
        doc.adjustment(first).unwrap().target_layer_ids,
        Some(vec![1])
    );
    assert_eq!(
        doc.adjustment(second).unwrap().target_layer_ids,
        Some(vec![1])
    );
}
#[test]
fn candidate_preview_reuses_native_color_family_and_raw_mask_context_without_publication() {
    let mut doc = fixture();
    let (id, _) = doc
        .create_adjustment(invert(Some(vec![]), 255), 0, 3, None)
        .unwrap();
    let mask = doc.create_mask(MaskOwner::Layer { id: 1 }, 128).unwrap();
    let _ = mask;
    doc.layers[0].opacity = 128;
    doc.refresh_renderer_plan().unwrap();
    doc.invalidate();
    doc.compose();
    let mut spec = invert(Some(vec![1, 3]), 255);
    spec.node_type = "compositing.hsl".into();
    spec.filter = Filter::Invert;
    spec.amount = 0.;
    spec.hsl = Some(crate::engine::hsl::Hsl {
        hue: 120.,
        ..Default::default()
    });
    let before = doc.composition();
    let raw = words(&doc);
    let history = doc.history_bytes();
    let preview = doc.preview_adjustment(id, spec.clone()).unwrap();
    assert_eq!(doc.composition(), before);
    assert_eq!(words(&doc), raw);
    assert_eq!(doc.history_bytes(), history);
    doc.set_adjustment(id, spec).unwrap();
    doc.compose();
    assert_eq!(doc.composite, preview);
}
#[test]
fn shared_targets_persist_and_hidden_expansion_budget_is_checked_before_publication() {
    let mut doc = fixture();
    let (id, _) = doc
        .create_adjustment(invert(Some(vec![1, 3]), 255), 0, 0, None)
        .unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/shared-adjustments")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("binding.mcp");
    doc.save(&path).unwrap();
    let restored = Document::load(&path).unwrap();
    assert_eq!(
        restored.adjustment(id).unwrap().target_layer_ids,
        Some(vec![1, 3])
    );
    assert_eq!(restored.layers.len(), 3);
    let mut hidden = invert(Some(vec![1, 2, 3]), 255);
    hidden.visible = false;
    for _ in 0..16 {
        let before = doc.composition();
        let raw = words(&doc);
        match doc.create_adjustment(hidden.clone(), 0, doc.tree().roots.len(), None) {
            Ok(_) => {}
            Err(error) => {
                assert!(error.contains("Node or edge limit"), "{error}");
                assert_eq!(doc.composition(), before);
                assert_eq!(words(&doc), raw);
                return;
            }
        }
    }
    panic!("Hidden shared declarations must still meet the generated node budget");
}
