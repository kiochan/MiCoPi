use super::*;
use crate::engine::{
    document_info::{Density, DensityUnit, Info},
    graph::Renderer,
    groups::GroupItem,
    raster_masks::{MaskOwner, PaintTarget, PlaneView},
    replay::{Journal, Metadata, Source},
    selection::{Mask, SelectionState},
};
use micopi_plugin_api as abi;
use serde_json::json;
use std::cell::Cell;

fn renderer() -> Arc<dyn Renderer> {
    crate::plugins::Registry::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
    )
    .unwrap()
    .renderer()
    .expect("Stage the matching compositor DLL")
}
fn document(width: usize, height: usize) -> Document {
    let mut doc = Document::new(width, height);
    doc.renderer = Some(renderer());
    doc.layers[0].pixels.fill(0xff557799);
    doc
}
fn samples(doc: &Document, id: u64) -> Vec<u8> {
    match doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => vec![value; doc.width * doc.height],
        PlaneView::Dense(data) => data.to_vec(),
    }
}
fn staged(doc: &Document) -> Vec<(u64, SamplePlane)> {
    doc.mask_planes
        .iter()
        .map(|plane| (plane.id, plane.samples.clone()))
        .collect()
}
fn rgba(doc: &Document) -> Vec<Vec<u32>> {
    doc.layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect()
}
fn effective(doc: &mut Document, id: u64) -> Vec<u8> {
    let area = doc.width * doc.height;
    match doc.effective_mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => vec![value; area],
        PlaneView::Dense(data) => data.to_vec(),
    }
}
#[derive(Debug, PartialEq)]
struct Snapshot {
    width: usize,
    height: usize,
    active: usize,
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    masks: Vec<(u64, SamplePlane, u64)>,
    epoch: u64,
    contact: Option<crate::engine::raster_masks::MaskLease>,
    undo: usize,
    redo: usize,
    dirty: bool,
    changed: Option<[usize; 4]>,
    replay: std::collections::BTreeSet<(u64, usize)>,
}
fn snapshot(doc: &Document) -> Snapshot {
    Snapshot {
        width: doc.width,
        height: doc.height,
        active: doc.active,
        composition: doc.composition(),
        pixels: rgba(doc),
        masks: doc
            .mask_planes
            .iter()
            .map(|plane| (plane.id, plane.samples.clone(), plane.content_revision))
            .collect(),
        epoch: doc.mask_storage_epoch,
        contact: doc.mask_contact,
        undo: doc.undo.len(),
        redo: doc.redo.len(),
        dirty: doc.dirty,
        changed: doc.changed,
        replay: doc.replay_mask_tiles.clone(),
    }
}

#[test]
fn canvas_mask_frame_budget_counts_uniform_group_planes_without_allocating_outputs() {
    let mut doc = document(1, 1);
    let mut groups = vec![];
    let mut member = GroupItem::layer(1);
    for _ in 0..4 {
        let id = doc.group_create(&[member], None, None).unwrap();
        groups.push(id);
        member = GroupItem::group(id);
    }
    doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    for id in groups {
        doc.create_mask(MaskOwner::Group { id }, 255).unwrap();
    }
    let before = snapshot(&doc);
    assert!(doc.validate_canvas_geometry_frame(4096, 4096).is_err());
    assert!(doc.validate_canvas_geometry_frame(4096, 3072).is_ok());
    assert!(doc.validate_canvas_geometry_frame(usize::MAX, 2).is_err());
    assert!(doc.validate_canvas_geometry_frame(0, 1).is_err());
    assert_eq!(snapshot(&doc), before);
    assert!(doc
        .mask_planes
        .iter()
        .all(|plane| matches!(plane.samples, SamplePlane::Uniform(255))));
}

#[test]
fn incomplete_or_invalid_canvas_mask_bundles_reject_before_any_publication() {
    let mut doc = document(3, 2);
    doc.try_add_layer().unwrap();
    let first = doc.create_mask(MaskOwner::Layer { id: 1 }, 12).unwrap();
    let second = doc.create_mask(MaskOwner::Layer { id: 2 }, 230).unwrap();
    let before = snapshot(&doc);
    for masks in [
        vec![],
        vec![(first, SamplePlane::Uniform(12))],
        vec![
            (first, SamplePlane::Uniform(12)),
            (first, SamplePlane::Uniform(230)),
        ],
        vec![
            (second, SamplePlane::Uniform(230)),
            (first, SamplePlane::Uniform(12)),
        ],
        vec![
            (first, SamplePlane::Dense(vec![12; 5])),
            (second, SamplePlane::Uniform(230)),
        ],
    ] {
        assert!(doc
            .replace_geometry_with_masks(3, 2, rgba(&doc), masks, doc.selection.clone(), doc.info())
            .is_err());
        assert_eq!(snapshot(&doc), before);
    }
    assert!(doc
        .replace_geometry_with_masks(
            3,
            2,
            vec![vec![0; 6]],
            staged(&doc),
            doc.selection.clone(),
            doc.info()
        )
        .is_err());
    assert!(doc
        .replace_geometry_with_masks(
            3,
            2,
            rgba(&doc),
            staged(&doc),
            SelectionState {
                current: Some(Mask::new(2, 3, vec![1; 6]).unwrap()),
                previous: None
            },
            doc.info(),
        )
        .is_err());
    let mut invalid_info = doc.info();
    invalid_info.resolution_ppi = 0;
    assert!(doc
        .replace_geometry_with_masks(
            3,
            2,
            rgba(&doc),
            staged(&doc),
            doc.selection.clone(),
            invalid_info
        )
        .is_err());
    assert_eq!(snapshot(&doc), before);
}

#[test]
fn complete_canvas_geometry_retains_owners_targets_selection_info_and_owned_undo_buffers() {
    let mut doc = document(3, 2);
    doc.try_add_layer().unwrap();
    let group = doc
        .group_create(&[GroupItem::layer(2)], None, None)
        .unwrap();
    doc.group_set(group, None, Some(false), None, None, None)
        .unwrap();
    doc.set_layer_selection(&[1, 2], Some(1)).unwrap();
    let layer = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    doc.replace_mask_plane(layer, SamplePlane::Dense(vec![10, 20, 30, 40, 50, 60]))
        .unwrap();
    let folder = doc.create_mask(MaskOwner::Group { id: group }, 17).unwrap();
    let mut properties = doc.raster_masks.items[1].clone();
    properties.enabled = false;
    properties.linked = false;
    properties.density = 203;
    properties.feather_radius_px = 2;
    doc.update_mask_metadata(properties).unwrap();
    doc.set_mask_target(PaintTarget::Mask { id: folder })
        .unwrap();
    let info = Info {
        png_density: Some(Density {
            xppu: 100,
            yppu: 200,
            unit: DensityUnit::Unspecified,
        }),
        ..Default::default()
    };
    doc.initialize_info(info);
    let original_selection = SelectionState {
        current: Some(Mask::new(3, 2, vec![1, 2, 3, 4, 5, 6]).unwrap()),
        previous: Some(Mask::new(3, 2, vec![11, 12, 13, 14, 15, 16]).unwrap()),
    };
    doc.set_selection(original_selection.clone()).unwrap();
    let before = snapshot(&doc);
    let original_pixel_pointer = doc.layers[0].pixels.as_ptr();
    let original_mask_pointer = match doc.mask_plane(layer).unwrap() {
        PlaneView::Dense(data) => data.as_ptr(),
        _ => unreachable!(),
    };
    let selected = doc.selected_layer_ids().to_vec();
    let active = doc.active;
    let next_selection = SelectionState {
        current: Some(Mask::new(2, 3, vec![4, 1, 5, 2, 6, 3]).unwrap()),
        previous: Some(Mask::new(2, 3, vec![14, 11, 15, 12, 16, 13]).unwrap()),
    };
    let pixels = vec![vec![0xff334455; 6], vec![0x00776655; 6]];
    let masks = vec![
        (layer, SamplePlane::Dense(vec![40, 10, 50, 20, 60, 30])),
        (folder, SamplePlane::Uniform(17)),
    ];
    doc.replace_geometry_with_masks(
        2,
        3,
        pixels.clone(),
        masks,
        next_selection.clone(),
        info.rotated_quarter(),
    )
    .unwrap();
    assert_eq!((doc.width, doc.height), (2, 3));
    assert_eq!(doc.undo.len(), before.undo + 1);
    assert_eq!(doc.active, active);
    assert_eq!(doc.selected_layer_ids(), selected);
    assert_eq!(doc.raster_masks, before.composition.raster_masks);
    assert_eq!(doc.graph, before.composition.graph);
    assert_eq!(doc.groups, before.composition.groups);
    assert_eq!(doc.selection, next_selection);
    assert_eq!(doc.info(), info.rotated_quarter());
    assert_eq!(samples(&doc, layer), vec![40, 10, 50, 20, 60, 30]);
    assert_eq!(samples(&doc, folder), vec![17; 6]);
    assert_eq!(
        doc.replay_mask_tiles,
        [(layer, 0), (folder, 0)].into_iter().collect()
    );
    let next_epoch = doc.mask_storage_epoch;
    doc.replay_mask_tiles.clear();
    doc.undo();
    assert_eq!((doc.width, doc.height), (3, 2));
    assert_eq!(doc.layers[0].pixels.as_ptr(), original_pixel_pointer);
    assert_eq!(doc.mask_plane(layer).unwrap().sample(0), 10);
    if let PlaneView::Dense(data) = doc.mask_plane(layer).unwrap() {
        assert_eq!(data.as_ptr(), original_mask_pointer);
    }
    assert_eq!(doc.selection, original_selection);
    assert_eq!(doc.info(), info);
    assert_eq!(doc.raster_masks, before.composition.raster_masks);
    assert_ne!(doc.mask_storage_epoch, next_epoch);
    assert_eq!(
        doc.replay_mask_tiles,
        [(layer, 0), (folder, 0)].into_iter().collect()
    );
    doc.replay_mask_tiles.clear();
    doc.redo();
    assert_eq!(rgba(&doc), pixels);
    assert_eq!(doc.selection, next_selection);
    assert_eq!(samples(&doc, layer), vec![40, 10, 50, 20, 60, 30]);
    assert_eq!(
        doc.replay_mask_tiles,
        [(layer, 0), (folder, 0)].into_iter().collect()
    );
    doc.validate_mask_storage().unwrap();
}

#[test]
fn same_size_raw_only_geometry_invalidates_effects_and_marks_every_tile_on_undo_redo() {
    let mut doc = document(2, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    doc.replace_mask_plane(id, SamplePlane::Dense(vec![165, 4, 135, 79]))
        .unwrap();
    let mut properties = doc.raster_masks.items[0].clone();
    properties.feather_radius_px = 1;
    doc.update_mask_metadata(properties).unwrap();
    assert_eq!(effective(&mut doc, id), vec![113, 71, 114, 85]);
    let history = doc.undo.len();
    doc.replay_mask_tiles.clear();
    doc.replace_geometry_with_masks(
        2,
        2,
        rgba(&doc),
        vec![(id, SamplePlane::Dense(vec![135, 165, 79, 4]))],
        doc.selection.clone(),
        doc.info(),
    )
    .unwrap();
    assert_eq!(
        doc.undo.len(),
        history + 1,
        "Mask samples participate in identity checks"
    );
    assert_eq!(effective(&mut doc, id), vec![115, 113, 84, 71]);
    assert_eq!(doc.replay_mask_tiles, [(id, 0)].into_iter().collect());
    doc.replay_mask_tiles.clear();
    doc.undo();
    assert_eq!(effective(&mut doc, id), vec![113, 71, 114, 85]);
    assert_eq!(doc.replay_mask_tiles, [(id, 0)].into_iter().collect());
    doc.replay_mask_tiles.clear();
    doc.redo();
    assert_eq!(effective(&mut doc, id), vec![115, 113, 84, 71]);
    assert_eq!(doc.replay_mask_tiles, [(id, 0)].into_iter().collect());
}

#[test]
fn uniform_dense_equivalent_geometry_is_a_true_noop_in_both_directions() {
    let mut doc = document(3, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 77).unwrap();
    let before = snapshot(&doc);
    doc.replace_geometry_with_masks(
        3,
        2,
        rgba(&doc),
        vec![(id, SamplePlane::Dense(vec![77; 6]))],
        doc.selection.clone(),
        doc.info(),
    )
    .unwrap();
    assert_eq!(snapshot(&doc), before);
    let lease = doc.begin_mask_stroke(id).unwrap();
    doc.finish();
    let before = snapshot(&doc);
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(data) => data.as_ptr(),
        _ => unreachable!(),
    };
    doc.reframe_with_masks(3, 2, 0, 0, vec![(id, SamplePlane::Uniform(77))])
        .unwrap();
    assert_eq!(snapshot(&doc), before);
    if let PlaneView::Dense(data) = doc.mask_plane(id).unwrap() {
        assert_eq!(data.as_ptr(), pointer);
    }
    assert!(doc.prepare_mask_region(lease, 0, 0, 1, 1).is_err());
}

struct Capabilities {
    native: Arc<dyn Renderer>,
    effects: Cell<bool>,
    masks: Cell<bool>,
    extended: Cell<bool>,
}
impl Renderer for Capabilities {
    fn supports_masks(&self) -> bool {
        self.masks.get()
    }
    fn supports_mask_effects(&self) -> bool {
        self.effects.get()
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        (mode < 8 || self.extended.get()) && self.native.supports_blend_mode(mode)
    }
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.native.render(nodes, layers, width, height, output)
    }
    fn render_masked(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.native
            .render_masked(nodes, layers, masks, width, height, output)
    }
}
#[test]
fn hidden_disabled_capabilities_preflight_even_semantically_identical_geometry() {
    let mut doc = document(2, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    let mut properties = doc.raster_masks.items[0].clone();
    properties.enabled = false;
    properties.density = 100;
    doc.update_mask_metadata(properties).unwrap();
    doc.set_layer_properties(0, false, 255, BlendMode::Hue)
        .unwrap();
    let probe = Arc::new(Capabilities {
        native: renderer(),
        effects: Cell::new(true),
        masks: Cell::new(true),
        extended: Cell::new(true),
    });
    doc.renderer = Some(probe.clone());
    let before = snapshot(&doc);
    probe.effects.set(false);
    assert!(doc.reframe_with_masks(2, 2, 0, 0, staged(&doc)).is_err());
    assert_eq!(snapshot(&doc), before);
    probe.effects.set(true);
    probe.masks.set(false);
    assert!(doc
        .replace_geometry_with_masks(
            2,
            2,
            rgba(&doc),
            staged(&doc),
            doc.selection.clone(),
            doc.info()
        )
        .is_err());
    assert_eq!(snapshot(&doc), before);
    probe.masks.set(true);
    probe.extended.set(false);
    assert!(doc.reframe_with_masks(2, 2, 0, 0, staged(&doc)).is_err());
    assert_eq!(snapshot(&doc), before);
    assert_eq!(samples(&doc, id), vec![255; 4]);
}

#[test]
fn masked_canvas_geometry_rejection_preserves_an_active_lease_and_legacy_guards() {
    let mut doc = document(3, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 80).unwrap();
    let lease = doc.begin_mask_stroke(id).unwrap();
    doc.prepare_mask_region(lease, 0, 0, 1, 1).unwrap();
    let before = snapshot(&doc);
    assert!(doc.reframe_with_masks(3, 2, 0, 0, staged(&doc)).is_err());
    assert!(doc
        .replace_geometry_with_masks(
            3,
            2,
            rgba(&doc),
            staged(&doc),
            doc.selection.clone(),
            doc.info()
        )
        .is_err());
    assert_eq!(snapshot(&doc), before);
    doc.prepare_mask_region(lease, 0, 0, 1, 1).unwrap();
    doc.finish();
    let before = snapshot(&doc);
    assert_eq!(
        doc.reframe(3, 2, 0, 0).unwrap_err(),
        crate::engine::MASK_EDITING_PENDING
    );
    assert_eq!(
        doc.replace_geometry_with_info(3, 2, rgba(&doc), doc.selection.clone(), doc.info())
            .unwrap_err(),
        crate::engine::MASK_EDITING_PENDING
    );
    assert_eq!(snapshot(&doc), before);
    doc.reframe_with_masks(
        4,
        2,
        1,
        0,
        vec![(
            id,
            SamplePlane::Dense(vec![255, 80, 80, 80, 255, 80, 80, 80]),
        )],
    )
    .unwrap();
    assert_eq!(
        doc.layers[0].pixels,
        vec![0, 0xff557799, 0xff557799, 0xff557799, 0, 0xff557799, 0xff557799, 0xff557799]
    );
    assert_eq!(samples(&doc, id), vec![255, 80, 80, 80, 255, 80, 80, 80]);
    let next = doc.begin_mask_stroke(id).unwrap();
    assert_ne!(lease.storage_epoch, next.storage_epoch);
    assert!(doc.prepare_mask_region(lease, 0, 0, 1, 1).is_err());
    assert!(doc.prepare_mask_region(next, 0, 0, 1, 1).is_ok());
}

fn metadata(doc: &Document, revision: u64) -> Metadata {
    Metadata {
        width: doc.width,
        height: doc.height,
        active: doc.active,
        composition: doc.composition(),
        selected_tool: "brush".into(),
        tool: crate::application::ToolKind::Brush,
        parameters: Default::default(),
        view: Default::default(),
        revision,
        dirty: doc.dirty,
        file: None,
        source_file: None,
        ui: serde_json::Value::Null,
        drawing: false,
        render_error: None,
    }
}
#[test]
fn same_frame_and_dimension_changed_geometry_record_raw_outcomes_for_dll_free_playback() {
    let mut doc = document(2, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    doc.replace_mask_plane(id, SamplePlane::Dense(vec![11, 22, 33, 44]))
        .unwrap();
    let first = metadata(&doc, 0);
    let mut journal = Journal::new(&mut doc, first, json!({})).unwrap();
    for (revision, width, height, raw) in [
        (1, 2, 2, vec![33, 11, 44, 22]),
        (2, 3, 2, vec![255, 33, 11, 255, 44, 22]),
    ] {
        doc.replace_geometry_with_masks(
            width,
            height,
            vec![vec![0xff557799; width * height]],
            vec![(id, SamplePlane::Dense(raw))],
            SelectionState::default(),
            doc.info(),
        )
        .unwrap();
        let current = metadata(&doc, revision);
        let prepared = journal
            .prepare(
                &mut doc,
                current,
                Source::Observed {
                    reason: "Complete canvas geometry".into(),
                },
                true,
                None,
            )
            .unwrap();
        journal.commit(prepared).unwrap();
        assert!(journal
            .preview(revision as usize)
            .unwrap()
            .matches_document(&doc));
    }
    let events = journal.events(1, 2).unwrap();
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["changed_mask_tiles"].as_u64().unwrap() > 0));
    let bytes = journal.bytes(None).unwrap();
    let mut loaded = Journal::import(&bytes).unwrap();
    assert_eq!(loaded.verify(None).unwrap()["valid"], true);
    let state = loaded.preview(2).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert!(state.matches_document(&restored));
    assert_eq!(samples(&restored, id), vec![255, 33, 11, 255, 44, 22]);
}
