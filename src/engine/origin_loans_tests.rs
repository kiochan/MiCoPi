use super::*;
use crate::engine::raster_masks::{MaskOwner, PlaneView};

struct OriginRenderer;
impl Renderer for OriginRenderer {
    fn supports_masks(&self) -> bool {
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
    fn render(
        &self,
        _: &[micopi_plugin_api::CompositeNode],
        _: &[micopi_plugin_api::CompositeLayer],
        _: u32,
        _: u32,
        _: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        panic!("Original loans must not render")
    }
}

fn fixture() -> Document {
    let mut doc = Document::new(70, 67);
    doc.renderer = Some(Arc::new(OriginRenderer));
    doc.layers[0].pixels = (0..70 * 67).map(|i| 0xff000000 | i as u32).collect();
    doc
}

#[test]
fn rgba_origin_views_reuse_captured_tiles_and_keep_edge_stride_and_originals() {
    let mut doc = fixture();
    let before = doc.layers[0].pixels.clone();
    doc.begin_plugin_stroke();
    assert!(doc.prepare_plugin_region(62, 63, 8, 4));
    assert_eq!(doc.stroke.as_ref().unwrap().tiles.len(), 4);
    let origin = doc.plugin_origin_region(64, 64, 6, 3).unwrap();
    assert_eq!(
        (origin.width, origin.height, origin.stride, origin.len),
        (6, 3, 6, 18)
    );
    assert_eq!((origin.origin_x, origin.origin_y), (64, 64));
    let saved = &doc.stroke.as_ref().unwrap().tiles[&3];
    assert_eq!(origin.data, saved.as_ptr());
    assert_eq!(Arc::strong_count(saved), 1);
    assert_ne!(origin.data, unsafe {
        doc.layers[0].pixels.as_ptr().add(64 * 70 + 64)
    });
    let sub = doc.plugin_origin_region(65, 65, 3, 2).unwrap();
    assert_eq!((sub.stride, sub.len), (6, 9));
    assert_eq!(sub.data, unsafe { origin.data.add(7) });
    doc.layers[0].pixels[65 * 70 + 65] = 0;
    assert_eq!(unsafe { *sub.data }, before[65 * 70 + 65]);
    assert!(doc.prepare_plugin_region(64, 64, 6, 3));
    let again = doc.plugin_origin_region(65, 65, 3, 2).unwrap();
    assert_eq!(again.data, sub.data);
    assert_eq!(doc.stroke.as_ref().unwrap().tiles.len(), 4);
    doc.rollback_plugin_stroke();
    assert_eq!(doc.layers[0].pixels, before);
}

#[test]
fn rgba_origin_views_reject_missing_cross_tile_bounds_or_wrong_contact_without_capture() {
    let mut doc = fixture();
    assert!(doc.plugin_origin_region(0, 0, 1, 1).is_err());
    doc.try_add_layer().unwrap();
    doc.select_layer(0);
    doc.begin_plugin_stroke();
    for (x, y, w, h) in [
        (0, 0, 1, 1),
        (63, 0, 2, 1),
        (0, 63, 1, 2),
        (0, 0, 0, 1),
        (70, 0, 1, 1),
        (usize::MAX, 0, 2, 1),
    ] {
        assert!(doc.plugin_origin_region(x, y, w, h).is_err());
    }
    assert!(doc.stroke.as_ref().unwrap().tiles.is_empty());
    assert!(doc.prepare_plugin_region(0, 0, 1, 1));
    let pointer = doc.plugin_origin_region(0, 0, 1, 1).unwrap().data;
    doc.active = 1;
    assert!(doc.plugin_origin_region(0, 0, 1, 1).is_err());
    doc.active = 0;
    doc.layer_metadata[0].pixel_locked = true;
    assert!(doc.plugin_origin_region(0, 0, 1, 1).is_err());
    doc.layer_metadata[0].pixel_locked = false;
    assert_eq!(doc.plugin_origin_region(0, 0, 1, 1).unwrap().data, pointer);
}

#[test]
fn a8_origin_views_borrow_raw_originals_once_after_uniform_materialization() {
    let mut doc = fixture();
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 91).unwrap();
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert!(doc.plugin_origin_region(0, 0, 1, 1).is_err());
    doc.prepare_mask_region(lease, 62, 63, 8, 4).unwrap();
    let origin = doc.mask_origin_region(lease, 64, 64, 6, 3).unwrap();
    assert_eq!(
        (origin.width, origin.height, origin.stride, origin.len),
        (6, 3, 6, 18)
    );
    let saved = &doc.stroke.as_ref().unwrap().mask_tiles[&(id, 3)];
    assert_eq!(origin.data, saved.as_ptr());
    assert_eq!(Arc::strong_count(saved), 1);
    let current = doc.mask_region(lease, 65, 65, 3, 2).unwrap();
    assert_eq!(current.stride, 70);
    assert_ne!(origin.data, current.data.cast_const());
    unsafe {
        *current.data = 7;
    }
    let sub = doc.mask_origin_region(lease, 65, 65, 3, 2).unwrap();
    assert_eq!(sub.data, unsafe { origin.data.add(7) });
    assert_eq!(unsafe { *sub.data }, 91);
    assert_eq!(doc.stroke.as_ref().unwrap().mask_tiles.len(), 4);
    doc.rollback_plugin_stroke();
    match doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => assert_eq!(value, 91),
        PlaneView::Dense(values) => assert!(values.iter().all(|value| *value == 91)),
    }
    assert!(doc.mask_origin_region(lease, 64, 64, 6, 3).is_err());
}

#[test]
fn a8_origin_views_validate_exact_lease_single_tile_and_existing_snapshot() {
    let mut doc = fixture();
    let id = doc.create_mask(MaskOwner::Layer { id: 1 }, 177).unwrap();
    let lease = doc.begin_mask_stroke(id).unwrap();
    assert!(doc.mask_origin_region(lease, 0, 0, 1, 1).is_err());
    assert!(doc.stroke.as_ref().unwrap().mask_tiles.is_empty());
    doc.prepare_mask_region(lease, 0, 0, 70, 67).unwrap();
    for (x, y, w, h) in [(63, 0, 2, 1), (0, 63, 1, 2), (0, 0, 1, 0), (0, 67, 1, 1)] {
        assert!(doc.mask_origin_region(lease, x, y, w, h).is_err());
    }
    let mut stale = lease;
    stale.id += 1;
    assert!(doc.mask_origin_region(stale, 0, 0, 1, 1).is_err());
    stale = lease;
    stale.document_identity += 1;
    assert!(doc.mask_origin_region(stale, 0, 0, 1, 1).is_err());
    doc.bump_mask_storage_epoch();
    assert!(doc.mask_origin_region(lease, 0, 0, 1, 1).is_err());
}
