use super::*;
use crate::engine::{
    graph::{Filter, Renderer},
    groups::{GroupItem, GroupPolicy},
    raster_masks::{MaskMetadata, SamplePlane},
};

fn renderer() -> Arc<dyn Renderer> {
    crate::plugins::Registry::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
    )
    .unwrap()
    .renderer()
    .expect("Stage the matching compositor DLL")
}
fn document(renderer: &Arc<dyn Renderer>, pixels: &[u32]) -> Document {
    let mut doc = Document::new(pixels.len(), 1);
    doc.renderer = Some(renderer.clone());
    doc.layers[0].pixels.copy_from_slice(pixels);
    doc.invalidate();
    doc
}
fn pixels(doc: &mut Document) -> Vec<u32> {
    doc.compose();
    assert!(doc.render_error.is_none(), "{:?}", doc.render_error);
    doc.composite.clone()
}
fn masks_for(ids: &[u64]) -> MaskState {
    MaskState {
        version: 1,
        items: ids
            .iter()
            .enumerate()
            .map(|(i, id)| MaskMetadata::new(i as u64 + 1, MaskOwner::Layer { id: *id }))
            .collect(),
        next_id: ids.len() as u64 + 1,
        target: Default::default(),
    }
}
#[test]
fn flat_sixteen_masked_rasters_fit_exactly_the_existing_instruction_budget() {
    let layers: Vec<_> = (0..16)
        .map(|_| Layer {
            pixels: vec![0],
            visible: true,
            opacity: 255,
            mode: BlendMode::Normal,
        })
        .collect();
    let source = Graph::linked_from_layers(&layers);
    let original = source.clone();
    let ids: Vec<_> = (1..=16).collect();
    let state = masks_for(&ids);
    let effective = masked_flat_graph(
        &source,
        &ids,
        &vec![BlendMode::Normal; 16],
        &vec![255; 16],
        &state,
    )
    .unwrap();
    let plan = effective
        .compile_with_masks(16, &(1..=16).collect::<Vec<_>>())
        .unwrap();
    assert_eq!(plan.len(), 64);
    assert_eq!(plan.iter().filter(|n| n.kind == 6).count(), 16);
    assert_eq!(plan.iter().filter(|n| n.kind == 7).count(), 16);
    assert!(!plan.iter().any(|n| n.kind == 4));
    assert_eq!(
        source, original,
        "Source/filter identities must not be replaced by actual composition"
    );
    assert!(
        effective.compile(16).is_err(),
        "Generated gates need the document descriptor context"
    );
}
#[test]
fn white_layer_masks_preserve_all_existing_filter_and_opacity_bytes() {
    let render = renderer();
    for filter in [
        Filter::Invert,
        Filter::Grayscale,
        Filter::Brightness,
        Filter::Contrast,
        Filter::Saturation,
    ] {
        for opacity in [0, 1, 127, 128, 254, 255] {
            let mut doc = document(&render, &[0x00776655, 0x01020304, 0x80667788, 0xffaabbcc]);
            doc.set_layer_properties(0, true, opacity, BlendMode::Normal)
                .unwrap();
            let mut source = doc.graph.clone();
            let layer = source
                .nodes
                .iter()
                .find(|n| matches!(n.data, NodeKind::Layer { .. }))
                .unwrap()
                .id;
            let output = source
                .nodes
                .iter()
                .find(|n| matches!(n.data, NodeKind::Output))
                .unwrap()
                .id;
            let id = source.nodes.iter().map(|n| n.id).max().unwrap() + 1;
            source.nodes.push(Node {
                id,
                position: [200., 0.],
                node_type: None,
                data: NodeKind::Filter {
                    filter,
                    amount: 0.35,
                },
            });
            source.edges = vec![
                super::super::super::graph::Edge {
                    from: layer,
                    to: id,
                    input: 0,
                },
                super::super::super::graph::Edge {
                    from: id,
                    to: output,
                    input: 0,
                },
            ];
            doc.set_graph(source.clone()).unwrap();
            let before = pixels(&mut doc);
            let raw = doc.layers[0].pixels.clone();
            let mask = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
            assert_eq!(pixels(&mut doc), before, "{filter:?}/{opacity}");
            assert_eq!(doc.layers[0].pixels, raw);
            assert_eq!(doc.graph, source);
            let generated = doc.effective_graph().unwrap();
            let gate = generated
                .nodes
                .iter()
                .find(|n| matches!(n.data, NodeKind::ApplyMask { .. }))
                .unwrap()
                .id;
            assert!(
                generated
                    .edges
                    .iter()
                    .any(|e| e.from == id && e.to == gate && e.input == 0),
                "The mask must follow enabled filters"
            );
            doc.delete_mask(mask).unwrap();
            assert_eq!(pixels(&mut doc), before);
        }
    }
}
#[test]
fn spatial_layer_masks_use_the_staged_opacity_and_remain_active_with_filters_disabled() {
    let render = renderer();
    let mut doc = document(&render, &[0xff112233, 0xff445566, 0x80778899, 0x00abcdef]);
    let mask = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    doc.replace_mask_plane(mask, SamplePlane::Dense(vec![0, 255, 128, 255]))
        .unwrap();
    doc.set_layer_properties(0, true, 128, BlendMode::Normal)
        .unwrap();
    assert_eq!(pixels(&mut doc), vec![0, 0x80445566, 0x20778899, 0]);
    let mut graph = doc.graph.clone();
    graph.enabled = false;
    doc.set_graph(graph).unwrap();
    assert_eq!(pixels(&mut doc), vec![0, 0x80445566, 0x20778899, 0]);
    let mut metadata = doc.mask_state().items[0].clone();
    metadata.enabled = false;
    doc.update_mask_metadata(metadata.clone()).unwrap();
    assert_eq!(
        pixels(&mut doc),
        vec![0x80112233, 0x80445566, 0x40778899, 0]
    );
    metadata.enabled = true;
    doc.update_mask_metadata(metadata).unwrap();
    doc.set_layer_properties(0, false, 128, BlendMode::Normal)
        .unwrap();
    assert_eq!(pixels(&mut doc), vec![0; 4]);
}
fn masked_reorder_document(render: &Arc<dyn Renderer>) -> Document {
    let mut doc = document(render, &[0xffff0000, 0xff00ff00]);
    doc.try_add_layer().unwrap();
    doc.layers[1]
        .pixels
        .copy_from_slice(&[0xff0000ff, 0xffff00ff]);
    doc.set_layer_properties(1, true, 128, BlendMode::Normal)
        .unwrap();
    for (owner, samples) in [(1, vec![255, 128]), (2, vec![128, 255])] {
        let mask = doc
            .create_mask(MaskOwner::Layer { id: owner }, 255)
            .unwrap();
        doc.replace_mask_plane(mask, SamplePlane::Dense(samples))
            .unwrap();
    }
    doc
}
#[test]
fn flat_layer_move_and_reorder_preserve_mask_owners_and_composition_through_history() {
    let render = renderer();
    for use_move in [true, false] {
        let mut doc = masked_reorder_document(&render);
        let before = doc.composition();
        let raw: Vec<_> = doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect();
        assert_eq!(pixels(&mut doc), vec![0xffbf0040, 0xc0aa55aa]);
        doc.preflight_layer_order(&[2, 1]).unwrap();
        if use_move {
            doc.move_layer(1, 0).unwrap();
        } else {
            doc.reorder_layers(&[2, 1]).unwrap();
        }
        assert_eq!((doc.layer_id(0), doc.layer_id(1)), (2, 1));
        assert_eq!(pixels(&mut doc), vec![0xffff0000, 0xc055aa55]);
        assert_eq!(doc.layers[0].pixels, raw[1]);
        assert_eq!(doc.layers[1].pixels, raw[0]);
        assert_eq!(doc.mask_state(), &before.raster_masks);
        for (id, expected) in [(1, [255, 128]), (2, [128, 255])] {
            let plane = doc.mask_plane(id).unwrap();
            assert_eq!([plane.sample(0), plane.sample(1)], expected);
        }
        let effective = doc.effective_graph().unwrap();
        assert_eq!(
            effective
                .nodes
                .iter()
                .filter(|node| matches!(node.data, NodeKind::ApplyMask { .. }))
                .count(),
            2
        );
        let moved = doc.composition();
        doc.undo();
        assert_eq!(doc.composition(), before);
        assert_eq!(pixels(&mut doc), vec![0xffbf0040, 0xc0aa55aa]);
        doc.redo();
        assert_eq!(doc.composition(), moved);
        assert_eq!(doc.effective_graph().unwrap(), effective);
        assert_eq!(pixels(&mut doc), vec![0xffff0000, 0xc055aa55]);
    }
}
#[test]
fn flat_masked_order_preflight_rejects_missing_renderer_before_any_edit() {
    let render = renderer();
    let mut doc = masked_reorder_document(&render);
    pixels(&mut doc);
    doc.renderer = None;
    let composition = doc.composition();
    let effective = doc.effective_graph().unwrap();
    let history = doc.history_bytes();
    let raw: Vec<_> = doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let composite = doc.composite.clone();
    assert!(doc.preflight_layer_order(&[2, 1]).is_err());
    assert!(doc.move_layer(1, 0).is_err());
    assert!(doc.reorder_layers(&[2, 1]).is_err());
    assert_eq!(doc.composition(), composition);
    assert_eq!(doc.effective_graph().unwrap(), effective);
    assert_eq!(doc.history_bytes(), history);
    assert_eq!(
        doc.layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        raw
    );
    assert_eq!(doc.composite, composite);
    for (id, expected) in [(1, [255, 128]), (2, [128, 255])] {
        let plane = doc.mask_plane(id).unwrap();
        assert_eq!([plane.sample(0), plane.sample(1)], expected);
    }
}
#[test]
fn isolated_and_pass_through_group_masks_gate_the_completed_change_once() {
    let render = renderer();
    let mut isolated = document(&render, &[0xff0000ff]);
    isolated.try_add_layer().unwrap();
    isolated.layers[1].pixels[0] = 0xff00ff00;
    let group = isolated
        .group_create(&[GroupItem::layer(1), GroupItem::layer(2)], None, None)
        .unwrap();
    isolated
        .group_set(group, None, None, Some(128), None, None)
        .unwrap();
    isolated
        .create_mask(MaskOwner::Group { id: group }, 128)
        .unwrap();
    assert_eq!(pixels(&mut isolated), vec![0x4000ff00]);
    let before_copy = isolated.composition();
    let history = isolated.history_bytes();
    assert_eq!(
        isolated.group_duplicate(group).unwrap_err(),
        crate::engine::MASK_EDITING_PENDING
    );
    assert_eq!(isolated.composition(), before_copy);
    assert_eq!(isolated.layers.len(), 2);
    assert_eq!(isolated.history_bytes(), history);
    assert_eq!(pixels(&mut isolated), vec![0x4000ff00]);
    let effective = isolated.effective_graph().unwrap();
    assert_eq!(
        effective
            .nodes
            .iter()
            .filter(|n| matches!(n.data, NodeKind::ApplyMask { .. }))
            .count(),
        1
    );
    let mut pass = document(&render, &[0xff204060]);
    pass.try_add_layer().unwrap();
    pass.layers[1].pixels[0] = 0xfff08030;
    pass.try_add_layer().unwrap();
    pass.layers[2].pixels[0] = 0xff80e040;
    let group = pass
        .group_create(&[GroupItem::layer(2), GroupItem::layer(3)], None, None)
        .unwrap();
    pass.group_set(
        group,
        None,
        None,
        Some(128),
        None,
        Some(GroupPolicy::PassThrough),
    )
    .unwrap();
    pass.create_mask(MaskOwner::Group { id: group }, 128)
        .unwrap();
    assert_eq!(pixels(&mut pass), vec![0xff386858]);
    let effective = pass.effective_graph().unwrap();
    assert_eq!(
        effective
            .nodes
            .iter()
            .filter(|n| matches!(n.data, NodeKind::MaskedMix { .. }))
            .count(),
        1
    );
    assert!(
        !effective
            .nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::ApplyMask { .. })),
        "Do not attenuate each Pass Through child"
    );
}
#[test]
fn white_group_masks_preserve_every_blend_mode_and_nested_boundary() {
    let render = renderer();
    for mode in BlendMode::ALL {
        let mut doc = document(&render, &[0x80335577, 0xffe1c283]);
        doc.try_add_layer().unwrap();
        doc.layers[1]
            .pixels
            .copy_from_slice(&[0xc0f18037, 0x9000cc88]);
        let group = doc
            .group_create(&[GroupItem::layer(2)], None, None)
            .unwrap();
        doc.group_set(group, None, None, Some(137), Some(mode), None)
            .unwrap();
        let before = pixels(&mut doc);
        doc.create_mask(MaskOwner::Group { id: group }, 255)
            .unwrap();
        assert_eq!(pixels(&mut doc), before, "{mode:?}");
    }
}
struct NoRendering;
impl Renderer for NoRendering {
    fn supports_masks(&self) -> bool {
        true
    }
    fn render(
        &self,
        _: &[abi::CompositeNode],
        _: &[abi::CompositeLayer],
        _: u32,
        _: u32,
        _: &mut abi::Pixels,
    ) -> Result<(), String> {
        panic!("Metadata planning must not render")
    }
}
#[test]
fn planning_is_metadata_only_and_unsupported_effects_reject_before_publish() {
    let mut doc = Document::new(2, 2);
    doc.renderer = Some(Arc::new(NoRendering));
    let state = masks_for(&[1]);
    assert!(
        doc.prepare_raster_mask_plan(&state, true).is_ok(),
        "Candidate planes need not exist before validation"
    );
    let mut unsupported = state.clone();
    unsupported.items[0].enabled = false;
    unsupported.items[0].density = 128;
    assert!(doc
        .prepare_raster_mask_plan(&unsupported, true)
        .err()
        .unwrap()
        .contains("mask effects support"));
    assert!(
        doc.prepare_raster_mask_plan(&unsupported, false).is_ok(),
        "Verified restoration does not execute effects"
    );
    let before = doc.composition();
    let mask = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let mut metadata = doc.mask_state().items[0].clone();
    metadata.feather_radius_px = 1;
    let stable = doc.composition();
    assert!(doc.update_mask_metadata(metadata).is_err());
    assert_eq!(doc.composition(), stable);
    doc.delete_mask(mask).unwrap();
    assert_eq!(doc.mask_state().items, before.raster_masks.items);
}
