use super::*;
use crate::engine::{
    graph::{Edge, Filter, Node},
    raster_masks::{MaskOwner, PaintTarget},
};
use std::cell::{Cell, RefCell};

/// Deliberately naive full-frame oracle. Production kernels own their bounded
/// halo buffers; this reference calculates every requested sample independently.
fn reference(raw: &[u8], width: usize, height: usize, radius: usize, density: u8) -> Vec<u8> {
    let mut samples = raw.to_vec();
    for radius in [(radius + 2) / 3, (radius + 1) / 3, radius / 3] {
        for horizontal in [true, false] {
            let mut next = vec![0; samples.len()];
            for y in 0..height {
                for x in 0..width {
                    let mut sum = 0usize;
                    for delta in -(radius as isize)..=radius as isize {
                        let (sx, sy) = if horizontal {
                            (
                                (x as isize + delta).clamp(0, width as isize - 1) as usize,
                                y,
                            )
                        } else {
                            (
                                x,
                                (y as isize + delta).clamp(0, height as isize - 1) as usize,
                            )
                        };
                        sum += samples[sy * width + sx] as usize;
                    }
                    let count = 2 * radius + 1;
                    next[y * width + x] = ((sum + count / 2) / count) as u8;
                }
            }
            samples = next;
        }
    }
    samples
        .iter()
        .map(|value| 255 - (((255 - *value as u32) * density as u32 + 127) / 255) as u8)
        .collect()
}

struct Probe {
    native: Arc<dyn Renderer>,
    effects: Cell<bool>,
    fail_effect: Cell<bool>,
    fail_reserve: Cell<bool>,
    fail_render: Cell<bool>,
    reservations: RefCell<Vec<[u32; 3]>>,
    regions: RefCell<Vec<[u32; 4]>>,
}
impl Probe {
    fn new() -> Arc<Self> {
        let registry = crate::plugins::Registry::load(
            &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        Arc::new(Self {
            native: registry
                .renderer()
                .expect("Stage the matching compositor DLL"),
            effects: Cell::new(true),
            fail_effect: Cell::new(false),
            fail_reserve: Cell::new(false),
            fail_render: Cell::new(false),
            reservations: RefCell::new(Vec::new()),
            regions: RefCell::new(Vec::new()),
        })
    }
}
impl Renderer for Probe {
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.effects.get()
    }
    fn reserve_mask_effects(&self, width: u32, height: u32, radius: u32) -> Result<(), String> {
        self.reservations.borrow_mut().push([width, height, radius]);
        if self.fail_reserve.get() {
            Err("Injected reserve failure".into())
        } else {
            Ok(())
        }
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.native.supports_blend_mode(mode)
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
        if self.fail_render.get() {
            return Err("Injected staged renderer failure".into());
        }
        self.native
            .render_masked(nodes, layers, masks, width, height, output)
    }
    fn mask_effect_region(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        width: u32,
        height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.regions.borrow_mut().push([
            output.origin_x,
            output.origin_y,
            output.width,
            output.height,
        ]);
        if self.fail_effect.get() {
            return Err("Injected effect failure".into());
        }
        assert_eq!(request.flags, 0);
        let raw: Vec<u8> = (0..height as usize)
            .flat_map(|y| {
                (0..width as usize).map(move |x| {
                    if source.data.is_null() {
                        source.constant as u8
                    } else {
                        unsafe { *source.data.add(y * source.stride as usize + x) }
                    }
                })
            })
            .collect();
        let values = reference(
            &raw,
            width as usize,
            height as usize,
            request.feather_radius as usize,
            request.density as u8,
        );
        for y in 0..output.height as usize {
            for x in 0..output.width as usize {
                unsafe {
                    *output.data.add(y * output.stride as usize + x) =
                        values[(output.origin_y as usize + y) * width as usize
                            + output.origin_x as usize
                            + x];
                }
            }
        }
        Ok(())
    }
}
fn document(renderer: &Arc<Probe>, width: usize, height: usize) -> Document {
    let mut doc = Document::new(width, height);
    doc.renderer = Some(renderer.clone());
    doc.layers[0].pixels.fill(0xff336699);
    doc.invalidate();
    doc
}
fn raw(doc: &Document, id: u64) -> Vec<u8> {
    let view = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|i| view.sample(i))
        .collect()
}
fn effective(doc: &mut Document, id: u64) -> Vec<u8> {
    let area = doc.width * doc.height;
    let view = doc.effective_mask_plane(id).unwrap();
    (0..area).map(|i| view.sample(i)).collect()
}
fn properties(doc: &mut Document, id: u64, density: u8, radius: u16) {
    let mut metadata = doc
        .mask_state()
        .items
        .iter()
        .find(|m| m.id == id)
        .unwrap()
        .clone();
    metadata.density = density;
    metadata.feather_radius_px = radius;
    doc.update_mask_metadata(metadata).unwrap();
}

#[test]
fn effective_uniform_masks_keep_white_edges_and_default_dense_storage_borrows_raw() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 3, 2);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    properties(&mut doc, id, 128, 256);
    assert!(matches!(
        doc.effective_mask_plane(id).unwrap(),
        PlaneView::Uniform(255)
    ));
    assert!(renderer.regions.borrow().is_empty());
    assert!(matches!(
        doc.mask_plane(id).unwrap(),
        PlaneView::Uniform(255)
    ));
    doc.replace_mask_plane(id, SamplePlane::Uniform(73))
        .unwrap();
    let expected = reference(&[73; 6], 3, 2, 256, 128);
    assert_eq!(effective(&mut doc, id), expected);
    assert_eq!(renderer.regions.borrow().last(), Some(&[0, 0, 1, 1]));
    assert!(matches!(
        doc.effective_mask_plane(id).unwrap(),
        PlaneView::Uniform(_)
    ));
    doc.replace_mask_plane(id, SamplePlane::Dense(vec![0, 1, 127, 128, 254, 255]))
        .unwrap();
    properties(&mut doc, id, 255, 0);
    let pointer = match doc.mask_plane(id).unwrap() {
        PlaneView::Dense(v) => v.as_ptr(),
        _ => unreachable!(),
    };
    assert_eq!(
        match doc.effective_mask_plane(id).unwrap() {
            PlaneView::Dense(v) => v.as_ptr(),
            _ => unreachable!(),
        },
        pointer
    );
    assert!(doc.mask_effects[0].samples.is_none());
    properties(&mut doc, id, 0, 256);
    assert!(matches!(
        doc.effective_mask_plane(id).unwrap(),
        PlaneView::Uniform(255)
    ));
    doc.compose();
    doc.replay_mask_tiles.clear();
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert!(
        renderer.reservations.borrow().is_empty(),
        "Zero density needs no feather scratch"
    );
    let region = doc.mask_region(lease, 0, 0, 1, 1).unwrap();
    unsafe {
        *region.data = 42;
    }
    assert!(
        doc.changed.is_none(),
        "Zero-density painting cannot change the composite"
    );
    assert!(doc.replay_mask_tiles.contains(&(id, 0)));
    doc.finish();
}

#[test]
fn mask_contacts_warm_dense_effects_then_reuse_buffers_and_expand_only_exact_support() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 73, 67);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    properties(&mut doc, id, 173, 4);
    let mut display = Vec::new();
    doc.update_display(&mut display);
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert!(
        renderer.regions.borrow().is_empty(),
        "Known white coverage needs no full kernel at DOWN"
    );
    assert_eq!(&*renderer.reservations.borrow(), &[[73, 67, 4]]);
    let (pointer, capacity) = match doc.mask_effects[0].samples.as_ref().unwrap() {
        SamplePlane::Dense(data) => (data.as_ptr(), data.capacity()),
        _ => unreachable!(),
    };
    renderer.regions.borrow_mut().clear();
    let region = doc.mask_region(lease, 35, 32, 1, 1).unwrap();
    unsafe {
        *region.data = 0;
    }
    assert_eq!(doc.update_display(&mut display), Some([31, 28, 40, 37]));
    assert_eq!(&*renderer.regions.borrow(), &[[31, 28, 9, 9]]);
    let bytes = raw(&doc, id);
    let expected = reference(&bytes, 73, 67, 4, 173);
    assert_eq!(effective(&mut doc, id), expected);
    for (&actual, &sample) in doc.composite.iter().zip(&expected) {
        assert_eq!(
            actual,
            if sample == 0 {
                0
            } else {
                (sample as u32) << 24 | 0x336699
            }
        );
    }
    let region = doc.mask_region(lease, 0, 0, 1, 1).unwrap();
    unsafe {
        *region.data = 41;
    }
    assert_eq!(doc.update_display(&mut display), Some([0, 0, 5, 5]));
    assert_eq!(renderer.regions.borrow().last(), Some(&[0, 0, 5, 5]));
    assert_eq!(
        match doc.mask_effects[0].samples.as_ref().unwrap() {
            SamplePlane::Dense(data) => (data.as_ptr(), data.capacity()),
            _ => unreachable!(),
        },
        (pointer, capacity)
    );
    assert_eq!(doc.stroke.as_ref().unwrap().mask_tiles.len(), 1);
    let painted = raw(&doc, id);
    doc.finish();
    doc.undo();
    assert_eq!(raw(&doc, id), vec![255; 73 * 67]);
    assert_eq!(effective(&mut doc, id), vec![255; 73 * 67]);
    doc.redo();
    assert_eq!(raw(&doc, id), painted);
    assert_eq!(effective(&mut doc, id), reference(&painted, 73, 67, 4, 173));
}

#[test]
fn effect_metadata_undo_and_missing_hidden_disabled_capabilities_preserve_raw_planes() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 9, 7);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 127).unwrap();
    let bytes: Vec<_> = (0..63).map(|i| (i * 37) as u8).collect();
    doc.replace_mask_plane(id, SamplePlane::Dense(bytes.clone()))
        .unwrap();
    properties(&mut doc, id, 177, 3);
    assert_eq!(effective(&mut doc, id), reference(&bytes, 9, 7, 3, 177));
    doc.undo();
    assert_eq!(effective(&mut doc, id), bytes);
    doc.redo();
    assert_eq!(effective(&mut doc, id), reference(&bytes, 9, 7, 3, 177));
    assert_eq!(raw(&doc, id), bytes);
    renderer.effects.set(false);
    let mut hidden = doc.mask_state().items[0].clone();
    hidden.enabled = false;
    hidden.density = 0;
    hidden.feather_radius_px = 256;
    doc.layers[0].visible = false;
    let metadata = doc.composition();
    let history = doc.history_bytes();
    assert!(doc
        .update_mask_metadata(hidden)
        .unwrap_err()
        .contains("mask effects"));
    assert_eq!(doc.composition(), metadata);
    assert_eq!(doc.history_bytes(), history);
    assert_eq!(raw(&doc, id), bytes);
}

#[test]
fn failed_down_reservation_keeps_uniform_raw_storage_and_does_not_begin_a_contact() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 11, 9);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    properties(&mut doc, id, 201, 2);
    let metadata = doc.composition();
    let history = doc.history_bytes();
    let epoch = doc.mask_storage_epoch;
    renderer.fail_reserve.set(true);
    assert!(doc
        .begin_mask_stroke(id)
        .unwrap_err()
        .contains("Injected reserve"));
    assert!(matches!(
        doc.mask_plane(id).unwrap(),
        PlaneView::Uniform(255)
    ));
    assert_eq!(doc.composition(), metadata);
    assert_eq!(doc.history_bytes(), history);
    assert_eq!(doc.mask_storage_epoch, epoch);
    assert!(doc.stroke.is_none() && doc.mask_contact.is_none());
}

#[test]
fn nonwhite_uniform_down_retains_native_scalar_coverage_without_a_dense_feather_pass() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 17, 13);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 73).unwrap();
    properties(&mut doc, id, 177, 4);
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert_eq!(&*renderer.regions.borrow(), &[[0, 0, 1, 1]]);
    assert_eq!(&*renderer.reservations.borrow(), &[[17, 13, 4]]);
    let bytes = vec![73; 17 * 13];
    let expected = reference(&bytes, 17, 13, 4, 177);
    assert_eq!(effective(&mut doc, id), expected);
    let pointer = match doc.mask_effects[0].samples.as_ref().unwrap() {
        SamplePlane::Dense(samples) => samples.as_ptr(),
        _ => unreachable!(),
    };
    let region = doc.mask_region(lease, 8, 6, 1, 1).unwrap();
    unsafe {
        *region.data = 0;
    }
    assert_eq!(
        effective(&mut doc, id),
        reference(&raw(&doc, id), 17, 13, 4, 177)
    );
    assert_eq!(renderer.regions.borrow().last(), Some(&[4, 2, 9, 9]));
    assert_eq!(
        match doc.mask_effects[0].samples.as_ref().unwrap() {
            SamplePlane::Dense(samples) => samples.as_ptr(),
            _ => unreachable!(),
        },
        pointer
    );
}

#[test]
fn applying_effective_coverage_owns_one_edit_and_preserves_inactive_owner_properties() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 4, 2);
    doc.layers[0].pixels = vec![
        0xc8112233, 0x00abcdef, 0x80445566, 0xff778899, 0xc8112233, 0, 0x80445566, 0xff778899,
    ];
    doc.try_add_layer().unwrap();
    doc.layers[1].pixels.fill(0xffaabbcc);
    doc.layer_metadata[1].pixel_locked = true;
    doc.active = 1;
    doc.selected_layer_ids = vec![1, 2];
    doc.set_layer_properties(0, true, 137, BlendMode::Multiply)
        .unwrap();
    let mut source = doc.graph.clone();
    source.nodes.push(Node {
        id: 4,
        position: [100., 0.],
        node_type: None,
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 1.,
        },
    });
    for edge in &mut source.edges {
        if edge.from == 0 {
            edge.from = 4;
        }
    }
    source.edges.push(Edge {
        from: 0,
        to: 4,
        input: 0,
    });
    doc.set_graph(source.clone()).unwrap();
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 128).unwrap();
    properties(&mut doc, id, 173, 3);
    doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    let before = doc.composition();
    let original = doc.layers[0].pixels.clone();
    let other = doc.layers[1].pixels.clone();
    let undo = doc.undo.len();
    let sample = reference(&[128; 8], 4, 2, 3, 173)[0] as u32;
    let expected: Vec<_> = original
        .iter()
        .map(|pixel| {
            let alpha = (pixel >> 24) * sample / 255;
            if alpha == 0 {
                0
            } else {
                (pixel & 0xffffff) | (alpha << 24)
            }
        })
        .collect();
    assert_eq!(doc.apply_raster_mask(id).unwrap(), 1);
    assert_eq!(doc.undo.len(), undo + 1);
    assert_eq!(doc.layers[0].pixels, expected);
    assert_eq!(doc.layers[1].pixels, other);
    assert_eq!(doc.graph, source);
    assert_eq!(doc.layers[0].opacity, 137);
    assert_eq!(doc.layers[0].mode, BlendMode::Multiply);
    assert_eq!(doc.active, 1);
    assert_eq!(doc.selected_layer_ids(), vec![1, 2]);
    assert!(doc.mask_state().items.is_empty() && doc.mask_effects.is_empty());
    doc.undo();
    assert_eq!(doc.layers[0].pixels, original);
    assert_eq!(doc.composition(), before);
    assert_eq!(raw(&doc, id), vec![128; 8]);
    doc.redo();
    assert_eq!(doc.layers[0].pixels, expected);
    assert_eq!(doc.active, 1);
    assert_eq!(doc.selected_layer_ids(), vec![1, 2]);
}

#[test]
fn mask_apply_native_failure_and_explicit_owner_locks_do_not_publish_partial_edits() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 5, 3);
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 128).unwrap();
    let before = doc.composition();
    let original = doc.layers[0].pixels.clone();
    let history = doc.history_bytes();
    renderer.fail_render.set(true);
    assert!(doc
        .apply_raster_mask(id)
        .unwrap_err()
        .contains("Injected staged"));
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.layers[0].pixels, original);
    assert_eq!(doc.history_bytes(), history);
    renderer.fail_render.set(false);
    doc.layer_metadata[0].pixel_locked = true;
    assert!(doc
        .apply_raster_mask(id)
        .unwrap_err()
        .contains("layer pixels"));
    doc.layer_metadata[0].pixel_locked = false;
    doc.layer_metadata[0].alpha_locked = true;
    assert!(doc
        .apply_raster_mask(id)
        .unwrap_err()
        .contains("transparency"));
    assert_eq!(doc.layers[0].pixels, original);
    assert_eq!(raw(&doc, id), vec![128; 15]);
}

#[test]
fn applying_a_group_mask_rejects_before_baking_or_changing_owned_storage() {
    let renderer = Probe::new();
    let mut doc = document(&renderer, 5, 3);
    let group = doc
        .group_create(&[crate::engine::groups::GroupItem::layer(1)], None, None)
        .unwrap();
    let id = doc
        .create_mask(MaskOwner::Group { id: group }, 128)
        .unwrap();
    let before = doc.composition();
    let pixels = doc.layers[0].pixels.clone();
    let history = doc.history_bytes();
    renderer.fail_render.set(true);
    assert!(doc
        .apply_raster_mask(id)
        .unwrap_err()
        .contains("subtree baking"));
    assert_eq!(doc.composition(), before);
    assert_eq!(doc.layers[0].pixels, pixels);
    assert_eq!(raw(&doc, id), vec![128; 15]);
    assert_eq!(doc.history_bytes(), history);
}

#[test]
fn verified_effect_outcomes_restore_raw_planes_without_effect_caches_or_renderer_calls() {
    let renderer = Probe::new();
    let mut session =
        crate::application::Session::new(crate::plugins::Registry::bundled()).unwrap();
    session.doc = document(&renderer, 9, 7);
    let id = session
        .doc
        .create_mask(MaskOwner::Layer { id: 1 }, 255)
        .unwrap();
    let bytes: Vec<_> = (0..63).map(|i| (i * 29 + 7) as u8).collect();
    session
        .doc
        .replace_mask_plane(id, SamplePlane::Dense(bytes.clone()))
        .unwrap();
    properties(&mut session.doc, id, 193, 4);
    session.replay_boundary().unwrap();
    let count = renderer.regions.borrow().len();
    let state = session.replay_preview(0, false).unwrap();
    let archive = session.replay_journal(false).unwrap().bytes(None).unwrap();
    let mut loaded = crate::engine::replay::Journal::import(&archive).unwrap();
    assert_eq!(loaded.verify(None).unwrap()["valid"], true);
    let mut restored = Document::new(9, 7);
    state.restore_document(&mut restored);
    restored.compose();
    assert!(restored.renderer.is_none());
    assert!(state.matches_document(&restored));
    assert_eq!(raw(&restored, id), bytes);
    assert_eq!(renderer.regions.borrow().len(), count);
    assert!(restored.mask_effects.is_empty());
    assert!(!serde_json::to_string(&state.metadata)
        .unwrap()
        .contains("mask_effects"));

    // Same-frame rollback may reuse both raw and derived allocations. A stale
    // derived result must stay invalid until an explicit cold preparation.
    let mut reused = document(&renderer, 9, 7);
    let same_id = reused.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    reused
        .replace_mask_plane(same_id, SamplePlane::Dense(vec![41; 63]))
        .unwrap();
    properties(&mut reused, same_id, 193, 4);
    effective(&mut reused, same_id);
    let pointer = match reused.mask_effects[0].samples.as_ref().unwrap() {
        SamplePlane::Dense(samples) => samples.as_ptr(),
        _ => unreachable!(),
    };
    reused.renderer = None;
    let calls = renderer.regions.borrow().len();
    state.restore_document(&mut reused);
    reused.compose();
    assert!(state.matches_document(&reused));
    assert_eq!(renderer.regions.borrow().len(), calls);
    assert_eq!(reused.mask_effects[0].dirty, Some([0, 0, 9, 7]));
    assert_eq!(
        match reused.mask_effects[0].samples.as_ref().unwrap() {
            SamplePlane::Dense(samples) => samples.as_ptr(),
            _ => unreachable!(),
        },
        pointer
    );
    reused.renderer = Some(renderer.clone());
    assert_eq!(effective(&mut reused, id), reference(&bytes, 9, 7, 4, 193));
}
