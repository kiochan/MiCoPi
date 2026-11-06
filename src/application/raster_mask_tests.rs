//! Shared-session boundaries for the mask document foundation, before public tools.
use super::editing_tests::{call, session};
use super::*;
use crate::engine::raster_masks::{MaskOwner, PaintTarget, PlaneView, SamplePlane};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-core-session");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn samples(s: &Session, id: u64) -> Vec<u8> {
    match s.doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(v) => vec![v; s.doc.width * s.doc.height],
        PlaneView::Dense(data) => data.to_vec(),
    }
}
fn masked(width: usize, height: usize) -> (Session, u64) {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":width,"height":height}));
    call(&mut s, "layer.fill_color", json!({"color":"#336699"}));
    let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    s.replay_boundary().unwrap();
    (s, id)
}

#[test]
fn raster_mask_project_signature_and_export_guard_preserve_native_bytes_and_composed_pixels() {
    let (mut s, id) = masked(3, 2);
    let coverage = vec![0, 1, 127, 128, 254, 255];
    s.doc
        .replace_mask_plane(id, SamplePlane::Dense(coverage.clone()))
        .unwrap();
    s.replay_boundary().unwrap();
    let path = output("mask.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..8], b"MICOPI04");
    let renamed = output("signature-only.dat");
    std::fs::write(&renamed, &bytes).unwrap();
    assert!(codec::is_project(&renamed).unwrap());
    assert_eq!(
        codec::guard_png_destination(&renamed).unwrap_err().code,
        "project_overwrite"
    );
    assert_eq!(std::fs::read(&renamed).unwrap(), bytes);
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":renamed}));
    opened.doc.compose();
    assert!(
        opened.doc.render_error.is_none(),
        "{:?}",
        opened.doc.render_error
    );
    let expected: Vec<u32> = coverage
        .iter()
        .map(|&v| {
            if v == 0 {
                0
            } else {
                (v as u32) << 24 | 0x336699
            }
        })
        .collect();
    assert_eq!(opened.doc.composite, expected);
    assert_eq!(samples(&opened, id), coverage);
    assert_eq!(call(&mut opened, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn raster_mask_pending_edits_reject_before_finishing_live_brush_or_hand_contacts() {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        let mut s = session();
        call(&mut s, "canvas.new", json!({"width":16,"height":12}));
        call(&mut s, "layer.add", json!({}));
        let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 128).unwrap();
        s.replay_boundary().unwrap();
        s.parameters.stabilizer = 0.55;
        let sample = Pointer {
            phase: PointerPhase::Down,
            x: 4.,
            y: 4.,
            pressure: 0.35,
            time: 0,
            pen: true,
            eraser: false,
            tool_override: Some(tool),
            view_x: Some(40.),
            view_y: Some(40.),
        };
        s.pointer(sample).unwrap();
        let before = s.state();
        let metadata = s.doc.composition();
        let raw: Vec<_> = s
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect();
        let mask = samples(&s, id);
        let history = s.doc.history_bytes();
        for (action, args) in [
            ("canvas.resize", json!({"width":8,"height":8})),
            ("canvas.image_size.plan", json!({"width":8})),
            ("layer.merge_selected", json!({"layer_ids":[1,2]})),
            ("layer.duplicate", json!({})),
            (
                "layer.batch",
                json!({"action":"layer.set","layer_ids":[1,2],"args":{"opacity":0.5}}),
            ),
            ("layer.translate", json!({"dx":1,"dy":0})),
            ("group.create", json!({"members":[{"kind":"layer","id":1}]})),
            ("clipboard.paste", json!({})),
        ] {
            let error = s.apply(Action::new(action, args)).unwrap_err();
            assert_eq!(error.code, "mask_editing_pending", "{action}: {error}");
            assert_eq!(s.state(), before, "{action}");
            assert_eq!(s.doc.composition(), metadata, "{action}");
            assert!(s.stroke.active(), "{action}");
            assert_eq!(s.doc.history_bytes(), history, "{action}");
            assert_eq!(samples(&s, id), mask, "{action}");
            assert!(
                s.doc
                    .layers
                    .iter()
                    .zip(&raw)
                    .all(|(layer, p)| &layer.pixels == p),
                "{action}"
            );
        }
        s.pointer(Pointer {
            phase: PointerPhase::Move,
            x: 8.,
            y: 6.,
            pressure: 0.8,
            time: 12,
            view_x: Some(80.),
            view_y: Some(60.),
            ..sample
        })
        .unwrap();
        s.pointer(Pointer {
            phase: PointerPhase::Up,
            x: 8.,
            y: 6.,
            pressure: 0.8,
            time: 24,
            view_x: Some(80.),
            view_y: Some(60.),
            ..sample
        })
        .unwrap();
        assert!(!s.stroke.active());
        assert_eq!(samples(&s, id), mask);
        assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    }
}

#[test]
fn raster_mask_target_rejects_rgba_loans_until_an_explicit_layer_selection_restores_pixels() {
    let (mut s, id) = masked(8, 8);
    s.doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    s.replay_boundary().unwrap();
    let raw = s.doc.layers[0].pixels.clone();
    let coverage = samples(&s, id);
    let error = s
        .apply(Action::new("layer.fill_color", json!({"color":"#FFFFFF"})))
        .unwrap_err();
    assert_eq!(error.code, "mask_target_pending");
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(samples(&s, id), coverage);
    call(&mut s, "layer.selection.set", json!({"layer_ids":[1]}));
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id });
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1],"active_layer_id":1}),
    );
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    call(&mut s, "layer.fill_color", json!({"color":"#FFFFFF"}));
    assert!(s.doc.layers[0]
        .pixels
        .iter()
        .all(|&pixel| pixel == 0xffffffff));
    assert_eq!(samples(&s, id), coverage);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn raster_mask_atomic_checkpoint_restores_byte_planes_metadata_history_and_allocator_without_rendering(
) {
    let (mut s, id) = masked(65, 67);
    let original: Vec<_> = (0..65 * 67).map(|i| ((i * 37 + 11) % 256) as u8).collect();
    s.doc
        .replace_mask_plane(id, SamplePlane::Dense(original.clone()))
        .unwrap();
    s.replay_boundary().unwrap();
    let state = s.state();
    let metadata = s.doc.composition();
    let history = s.doc.history_bytes();
    let snapshot = super::atomic::Checkpoint::capture(&s);
    s.doc
        .replace_mask_plane(id, SamplePlane::Uniform(0))
        .unwrap();
    s.doc.delete_mask(id).unwrap();
    let fresh = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 5).unwrap();
    assert!(fresh > id);
    s.doc.renderer = None;
    snapshot.restore(&mut s);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), metadata);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(samples(&s, id), original);
    s.doc.validate_mask_storage().unwrap();
    assert!(s.doc.renderer.is_some());
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}
