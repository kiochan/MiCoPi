//! Post-publication mask failures must restore the complete Session checkpoint.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    graph::Renderer,
    raster_masks::{PaintTarget, PlaneView},
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{cell::Cell, sync::Arc};

/// Forward the real DLL's capabilities and kernels, but fail one masked render
/// after damaging its writable output. No preflight/capability failure is used.
struct FailOnce {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    failures: Cell<usize>,
    effects: Cell<usize>,
}
impl FailOnce {
    fn install(s: &mut Session) -> Arc<Self> {
        let renderer = Arc::new(Self {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            failures: Cell::new(0),
            effects: Cell::new(0),
        });
        assert!(renderer.native.supports_masks());
        assert!(renderer.native.supports_mask_effects());
        s.doc.renderer = Some(renderer.clone());
        renderer
    }
}
impl Renderer for FailOnce {
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.native.supports_blend_mode(mode)
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn reserve_mask_effects(&self, width: u32, height: u32, radius: u32) -> Result<(), String> {
        self.native.reserve_mask_effects(width, height, radius)
    }
    fn mask_effect_region(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        width: u32,
        height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.effects.set(self.effects.get() + 1);
        self.native
            .mask_effect_region(request, source, width, height, output)
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
        if self.armed.replace(false) {
            assert!(!masks.is_empty());
            assert!(!output.data.is_null() && output.len > 0);
            assert!(output.width > 0 && output.height > 0);
            unsafe { output.data.write(0xdeadc0de) };
            self.failures.set(self.failures.get() + 1);
            return Err("Injected post-publication masked-render failure".into());
        }
        self.native
            .render_masked(nodes, layers, masks, width, height, output)
    }
}

fn raw_masks(s: &Session) -> Vec<(u64, Vec<u8>)> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|mask| {
            let samples = match s.doc.mask_plane(mask.id).unwrap() {
                PlaneView::Uniform(value) => vec![value; s.doc.width * s.doc.height],
                PlaneView::Dense(samples) => samples.to_vec(),
            };
            (mask.id, samples)
        })
        .collect()
}
fn rgba(s: &Session) -> Vec<Vec<u32>> {
    s.doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect()
}

struct Before {
    public: Value,
    composition: Composition,
    masks: Vec<(u64, Vec<u8>)>,
    rgba: Vec<Vec<u32>>,
    surface: Vec<u32>,
    revision: u64,
    next_id: u64,
    history: usize,
}
impl Before {
    fn capture(s: &Session) -> Self {
        assert!(!s.doc.history_group_active());
        Self {
            public: s.state(),
            composition: s.doc.composition(),
            masks: raw_masks(s),
            rgba: rgba(s),
            surface: s.doc.composite.clone(),
            revision: s.revision,
            next_id: s.doc.mask_state().next_id,
            history: s.doc.history_bytes(),
        }
    }
    fn assert_restored(&self, s: &Session) {
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composition(), self.composition);
        assert_eq!(raw_masks(s), self.masks);
        assert_eq!(rgba(s), self.rgba);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.revision, self.revision);
        assert_eq!(s.doc.mask_state().next_id, self.next_id);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert!(!s.doc.history_group_active());
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
        assert!(s.replay_error.is_none());
        assert!(s.doc.render_error.is_none());
    }
}

fn selection_mask(s: &mut Session, layer: u64) -> u64 {
    call(
        s,
        "selection.shape",
        json!({"kind":"ellipse","x":1.5,"y":1.,"width":7.5,"height":5.25}),
    );
    let id = call(
        s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":layer},"source":"selection","select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    assert!(matches!(s.doc.mask_plane(id).unwrap(), PlaneView::Dense(_)));
    assert!(raw_masks(s)
        .iter()
        .find(|(id_, _)| *id_ == id)
        .unwrap()
        .1
        .windows(2)
        .any(|pair| pair[0] != pair[1]));
    id
}

#[test]
fn mask_create_post_publication_render_failure_restores_raw_planes_history_and_unconsumed_id() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#5477CC"}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#A9789F"}));
    let old_mask = selection_mask(&mut s, 1);
    let renderer = FailOnce::install(&mut s);
    let before = Before::capture(&s);
    let arguments = json!({"owner":{"kind":"layer","id":2},"source":"selection","select":true});
    renderer.armed.set(true);
    let error = s
        .apply(Action::new("mask.create", arguments.clone()))
        .unwrap_err();
    assert_eq!(error.code, "mask_render");
    assert!(error.message.contains("Injected post-publication"));
    assert_eq!(renderer.failures.get(), 1);
    assert!(!renderer.armed.get());
    before.assert_restored(&s);
    check(&mut s);
    before.assert_restored(&s);

    let id = call(&mut s, "mask.create", arguments)["mask_id"]
        .as_u64()
        .unwrap();
    assert_eq!(id, before.next_id.max(1));
    assert_eq!(s.doc.mask_state().next_id, id + 1);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id });
    assert_eq!(rgba(&s), before.rgba);
    let created = raw_masks(&s);
    let created_surface = s.doc.composite.clone();
    assert_eq!(created.len(), before.masks.len() + 1);
    assert!(!s.doc.history_group_active());
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw_masks(&s), before.masks);
    assert_eq!(s.doc.mask_state().items[0].id, old_mask);
    assert_eq!(
        s.doc.mask_state().target,
        before.composition.raster_masks.target
    );
    assert_eq!(s.doc.mask_state().next_id, id + 1);
    assert_eq!(rgba(&s), before.rgba);
    assert_eq!(s.doc.composite, before.surface);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(raw_masks(&s), created);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id });
    assert_eq!(s.doc.mask_state().next_id, id + 1);
    assert_eq!(s.doc.composite, created_surface);
    assert_eq!(rgba(&s), before.rgba);
    assert!(!s.doc.history_group_active());
    assert_eq!(renderer.failures.get(), 1);
    check(&mut s);
}

#[test]
fn mask_set_post_publication_render_failure_restores_dense_raw_and_effect_metadata_then_retries() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#5477CC"}));
    let id = selection_mask(&mut s, 1);
    let renderer = FailOnce::install(&mut s);
    let before = Before::capture(&s);
    let arguments = json!({"mask_id":id,"density":203,"feather_radius_px":3});
    renderer.armed.set(true);
    let error = s
        .apply(Action::new("mask.set", arguments.clone()))
        .unwrap_err();
    assert_eq!(error.code, "mask_render");
    assert!(error.message.contains("Injected post-publication"));
    assert_eq!(renderer.failures.get(), 1);
    assert!(renderer.effects.get() > 0);
    assert!(!renderer.armed.get());
    before.assert_restored(&s);
    check(&mut s);
    before.assert_restored(&s);

    call(&mut s, "mask.set", arguments);
    let updated = s.doc.composition();
    assert_eq!(updated.raster_masks.items[0].density, 203);
    assert_eq!(updated.raster_masks.items[0].feather_radius_px, 3);
    assert_eq!(raw_masks(&s), before.masks);
    assert!(matches!(s.doc.mask_plane(id).unwrap(), PlaneView::Dense(_)));
    assert_eq!(rgba(&s), before.rgba);
    assert_eq!(s.doc.mask_state().next_id, before.next_id);
    let updated_surface = s.doc.composite.clone();
    assert_ne!(updated_surface, before.surface);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before.composition);
    assert_eq!(s.doc.composite, before.surface);
    assert_eq!(raw_masks(&s), before.masks);
    assert_eq!(rgba(&s), before.rgba);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), updated);
    assert_eq!(s.doc.composite, updated_surface);
    assert_eq!(raw_masks(&s), before.masks);
    assert_eq!(rgba(&s), before.rgba);
    assert_eq!(s.doc.mask_state().next_id, before.next_id);
    assert!(!s.doc.history_group_active());
    assert_eq!(renderer.failures.get(), 1);
    check(&mut s);
}
