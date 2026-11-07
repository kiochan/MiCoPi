//! Preview equals real composition while source storage/caches remain unchanged.
use super::*;
use crate::engine::{graph::Renderer, groups::GroupItem, raster_masks::MaskOwner};
use std::cell::{Cell, RefCell};

struct Probe {
    native: Arc<dyn Renderer>,
    effects: Cell<usize>,
    renders: Cell<usize>,
    fail: Cell<bool>,
    layers: RefCell<Vec<usize>>,
    masks: RefCell<Vec<usize>>,
}
impl Renderer for Probe {
    fn supports_mask_effects(&self) -> bool {
        self.native.supports_mask_effects()
    }
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.native.supports_blend_mode(mode)
    }
    fn reserve_mask_effects(&self, w: u32, h: u32, r: u32) -> Result<(), String> {
        self.native.reserve_mask_effects(w, h, r)
    }
    fn mask_effect_region(
        &self,
        r: &abi::MaskEffectRequest,
        s: &abi::CompositeMask,
        w: u32,
        h: u32,
        o: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.effects.set(self.effects.get() + 1);
        if self.fail.get() {
            return Err("Injected isolated preview effect failure".into());
        }
        self.native.mask_effect_region(r, s, w, h, o)
    }
    fn render(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.native.render(n, l, w, h, o)
    }
    fn render_masked(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        m: &[abi::CompositeMask],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.renders.set(self.renders.get() + 1);
        *self.layers.borrow_mut() = l.iter().map(|l| l.data as usize).collect();
        *self.masks.borrow_mut() = m.iter().map(|m| m.data as usize).collect();
        self.native.render_masked(n, l, m, w, h, o)
    }
}
fn fixture() -> (Document, Arc<Probe>) {
    let registry = crate::plugins::Registry::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
    )
    .unwrap();
    let probe = Arc::new(Probe {
        native: registry.renderer().unwrap(),
        effects: Cell::new(0),
        renders: Cell::new(0),
        fail: Cell::new(false),
        layers: RefCell::new(Vec::new()),
        masks: RefCell::new(Vec::new()),
    });
    let mut doc = Document::new(3, 2);
    doc.renderer = Some(probe.clone());
    doc.layers[0].pixels = vec![
        0xff6a3355, 0x80774499, 0x00123456, 0xff4c88ba, 0x40221166, 0,
    ];
    doc.try_add_layer().unwrap();
    doc.layers[1].pixels = vec![0xff264973; 6];
    doc.layers[1].opacity = 137;
    let group = doc
        .group_create(&[GroupItem::layer(2)], Some("Flats".into()), None)
        .unwrap();
    doc.group_set(group, None, None, Some(193), None, None)
        .unwrap();
    for (owner, bytes, density) in [
        (
            MaskOwner::Layer { id: 1 },
            vec![17, 45, 73, 101, 129, 157],
            183,
        ),
        (
            MaskOwner::Group { id: group },
            vec![88, 106, 124, 142, 160, 178],
            211,
        ),
    ] {
        let id = doc.create_mask(owner, 255).unwrap();
        doc.replace_mask_plane(id, SamplePlane::Dense(bytes))
            .unwrap();
        let mut metadata = doc
            .mask_state()
            .items
            .iter()
            .find(|m| m.id == id)
            .unwrap()
            .clone();
        metadata.density = density;
        metadata.feather_radius_px = 1;
        doc.update_mask_metadata(metadata).unwrap();
    }
    doc.set_layer_selection(&[1], Some(1)).unwrap();
    doc.compose();
    (doc, probe)
}
struct Before {
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    raw: Vec<Vec<u8>>,
    surface: Vec<u32>,
    history: usize,
    raw_pointers: Vec<usize>,
    layer_pointers: Vec<usize>,
    undo: usize,
    redo: usize,
}
impl Before {
    fn capture(doc: &Document) -> Self {
        Self {
            composition: doc.composition(),
            pixels: doc.layers.iter().map(|l| l.pixels.clone()).collect(),
            raw: doc
                .mask_state()
                .items
                .iter()
                .map(|m| {
                    (0..6)
                        .map(|i| doc.mask_plane(m.id).unwrap().sample(i))
                        .collect()
                })
                .collect(),
            surface: doc.composite.clone(),
            history: doc.history_bytes(),
            raw_pointers: doc
                .mask_state()
                .items
                .iter()
                .map(|m| match doc.mask_plane(m.id).unwrap() {
                    PlaneView::Dense(v) => v.as_ptr() as usize,
                    _ => 0,
                })
                .collect(),
            layer_pointers: doc
                .layers
                .iter()
                .map(|l| l.pixels.as_ptr() as usize)
                .collect(),
            undo: doc.undo.len(),
            redo: doc.redo.len(),
        }
    }
    fn same(&self, doc: &Document) {
        assert_eq!(doc.composition(), self.composition);
        assert_eq!(
            doc.layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>(),
            self.pixels
        );
        assert_eq!(
            doc.mask_state()
                .items
                .iter()
                .map(|m| (0..6)
                    .map(|i| doc.mask_plane(m.id).unwrap().sample(i))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            self.raw
        );
        assert_eq!(doc.composite, self.surface);
        assert_eq!(doc.history_bytes(), self.history);
        assert_eq!(
            doc.mask_state()
                .items
                .iter()
                .map(|m| match doc.mask_plane(m.id).unwrap() {
                    PlaneView::Dense(v) => v.as_ptr() as usize,
                    _ => 0,
                })
                .collect::<Vec<_>>(),
            self.raw_pointers
        );
        assert_eq!(
            doc.layers
                .iter()
                .map(|l| l.pixels.as_ptr() as usize)
                .collect::<Vec<_>>(),
            self.layer_pointers
        );
        assert_eq!((doc.undo.len(), doc.redo.len()), (self.undo, self.redo));
    }
}

#[test]
fn owner_preview_matches_committed_group_mask_composition_and_borrows_untouched_caches() {
    let (doc, probe) = fixture();
    let (mut reference, _) = fixture();
    let before = Before::capture(&doc);
    let unchanged = match doc.effective_mask_view(1).unwrap() {
        PlaneView::Dense(v) => v.as_ptr() as usize,
        _ => panic!("dense group effect"),
    };
    let original_effect = match doc.effective_mask_view(0).unwrap() {
        PlaneView::Dense(v) => v.as_ptr() as usize,
        _ => panic!("dense raster effect"),
    };
    let pixels = vec![0, 0xff6a3355, 0x80774499, 0, 0xff4c88ba, 0x40221166];
    let raw = vec![255, 17, 45, 255, 101, 129];
    let effects = probe.effects.get();
    let cost = doc.owner_preview_cost(&[1]).unwrap();
    assert_eq!(
        cost,
        OwnerPreviewCost {
            composite_bytes: 24,
            effect_bytes: 6,
            extra_bytes: 30
        }
    );
    let result = doc
        .preview_owner_transform(&[(1, &pixels)], &[(1, PlaneView::Dense(&raw))])
        .unwrap();
    reference
        .apply_owner_move(
            Some(1),
            Some(pixels.clone()),
            Some((1, SamplePlane::Dense(raw.clone()))),
        )
        .unwrap();
    reference.compose();
    assert_eq!(result, reference.composite);
    assert_eq!(probe.effects.get(), effects + 1);
    assert_eq!(
        probe.layers.borrow()[1],
        doc.layers[1].pixels.as_ptr() as usize
    );
    assert_eq!(probe.masks.borrow()[1], unchanged);
    assert_ne!(probe.masks.borrow()[0], original_effect);
    assert_eq!(
        match doc.effective_mask_view(0).unwrap() {
            PlaneView::Dense(v) => v.as_ptr() as usize,
            _ => 0,
        },
        original_effect
    );
    before.same(&doc);
}

#[test]
fn owner_preview_cold_effects_are_isolated_and_cost_plans_preserve_live_contacts() {
    let (mut doc, probe) = fixture();
    doc.invalidate_mask_effects();
    let before = Before::capture(&doc);
    let effects = probe.effects.get();
    assert!(doc.effective_mask_view(0).is_err());
    assert!(doc.effective_mask_view(1).is_err());
    doc.begin_plugin_stroke();
    assert_eq!(doc.owner_preview_cost(&[]).unwrap().effect_bytes, 12);
    assert!(doc.stroke.is_some());
    before.same(&doc);
    doc.finish();
    let result = doc.preview_owner_transform(&[], &[]).unwrap();
    assert_eq!(result, doc.composite);
    assert_eq!(probe.effects.get(), effects + 2);
    assert!(doc.effective_mask_view(0).is_err());
    assert!(doc.effective_mask_view(1).is_err());
    before.same(&doc);
}

#[test]
fn owner_preview_rejects_bad_overrides_and_effect_failure_without_publishing_storage() {
    let (doc, probe) = fixture();
    let before = Before::capture(&doc);
    let effects = probe.effects.get();
    let renders = probe.renders.get();
    assert!(doc.owner_preview_cost(&[1, 1]).is_err());
    assert!(doc.preview_owner_transform(&[(1, &[0; 5])], &[]).is_err());
    assert!(doc
        .preview_owner_transform(&[(1, &[0; 6]), (1, &[0; 6])], &[])
        .is_err());
    assert!(doc
        .preview_owner_transform(&[], &[(1, PlaneView::Dense(&[0; 5]))])
        .is_err());
    assert_eq!(probe.effects.get(), effects);
    assert_eq!(probe.renders.get(), renders);
    probe.fail.set(true);
    assert!(doc
        .preview_owner_transform(&[], &[(1, PlaneView::Dense(&[0; 6]))])
        .unwrap_err()
        .contains("Injected"));
    before.same(&doc);
}
