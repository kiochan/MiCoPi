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
    call(&mut s, "document.save", json!({ "path": path }));
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
    call(&mut opened, "document.open", json!({ "path": renamed }));
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
        let plan = call(&mut s, "canvas.image_size.plan", json!({"width":8}));
        assert_eq!(plan["raster_mask_count"], 1);
        assert_eq!(plan["contact_active"], true);
        assert_eq!(plan["estimated_raster_mask_bytes"], 8 * 6);
        assert_eq!(s.state(), before);
        for (action, args, code) in [
            (
                "canvas.resize",
                json!({"width":8,"height":8}),
                "stroke_active",
            ),
            (
                "layer.merge_selected",
                json!({"layer_ids":[1,2]}),
                "mask_editing_pending",
            ),
            ("layer.duplicate", json!({}), "mask_editing_pending"),
            (
                "layer.batch",
                json!({"action":"layer.set","layer_ids":[1,2],"args":{"opacity":0.5}}),
                "mask_editing_pending",
            ),
            (
                "layer.translate",
                json!({"dx":1,"dy":0}),
                "mask_editing_pending",
            ),
            (
                "group.create",
                json!({"members":[{"kind":"layer","id":1}]}),
                "mask_editing_pending",
            ),
            ("clipboard.paste", json!({}), "mask_editing_pending"),
        ] {
            let error = s.apply(Action::new(action, args)).unwrap_err();
            assert_eq!(error.code, code, "{action}: {error}");
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

#[test]
fn raster_mask_add_layer_selects_pixels_with_one_structural_undo_and_verified_target_outcomes() {
    use super::editing_tests::check;

    for (grouped, group_mask, parent) in [
        (false, false, None),
        (true, false, None),
        (true, true, Some(1)),
        (true, true, Some(0)),
    ] {
        let mut s = session();
        call(&mut s, "layer.fill_color", json!({"color":"#336699"}));
        let owner_pixels = s.doc.layers[0].pixels.clone();
        if grouped {
            call(
                &mut s,
                "group.create",
                json!({"members":[{"kind":"layer","id":1}]}),
            );
        }
        call(
            &mut s,
            "selection.shape",
            json!({"kind":"rectangle","x":1,"y":1,"width":8,"height":5}),
        );
        let mask = call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":if group_mask {"group"} else {"layer"},"id":1},"source":"selection"}),
        )["mask_id"]
            .as_u64()
            .unwrap();
        call(&mut s, "selection.deselect", json!({}));
        let coverage = samples(&s, mask);
        let pointer = match s.doc.mask_plane(mask).unwrap() {
            PlaneView::Dense(data) => data.as_ptr(),
            _ => panic!("The nonuniform selection must remain a dense raw plane"),
        };
        let before = s.doc.composition();
        let revision = s.revision;
        let event = s.journal.as_ref().unwrap().entries.len();
        assert_eq!(before.raster_masks.target, PaintTarget::Mask { id: mask });
        let args = parent.map_or_else(|| json!({}), |id| json!({ "parent_id": id }));
        let created = call(&mut s, "layer.add", args.clone())["layer_id"]
            .as_u64()
            .unwrap();
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), event + 1);
        assert_eq!(s.doc.layers.len(), 2);
        assert_eq!(s.doc.layer_id(s.doc.active), created);
        assert_eq!(s.doc.selected_layer_ids(), vec![created]);
        assert_eq!(
            s.doc.layer_parent_id(s.doc.active),
            if grouped && parent != Some(0) { 1 } else { 0 }
        );
        assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
        assert_eq!(s.doc.mask_state().items, before.raster_masks.items);
        assert_eq!(s.doc.mask_state().next_id, before.raster_masks.next_id);
        assert_eq!(samples(&s, mask), coverage);
        assert!(s.doc.layers[s.doc.active].pixels.iter().all(|p| *p == 0));
        let recorded = call(&mut s, "replay.events", json!({"start":event,"limit":1}));
        assert_eq!(recorded["events"][0]["source"]["action"], "layer.add");
        assert_eq!(recorded["events"][0]["changed_mask_tiles"], 0);
        check(&mut s);

        // A single Undo restores both the former active row and its mask target.
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers.len(), 1);
        assert_eq!(s.doc.layer_id(s.doc.active), 1);
        assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: mask });
        let mut expected = before.clone();
        expected.next_layer_id = s.doc.composition().next_layer_id;
        assert!(expected.next_layer_id > created);
        assert_eq!(s.doc.composition(), expected);
        assert_eq!(samples(&s, mask), coverage);
        check(&mut s);

        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers.len(), 2);
        assert_eq!(s.doc.layer_id(s.doc.active), created);
        assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
        let last = s.journal.as_ref().unwrap().entries.len() - 1;
        let outcome = s.replay_preview(last, false).unwrap();
        assert_eq!(
            outcome.metadata.composition.raster_masks.target,
            PaintTarget::LayerPixels
        );
        let mut restored = crate::engine::Document::new(1, 1);
        assert!(restored.renderer.is_none());
        outcome.restore_document(&mut restored);
        assert_eq!(restored.mask_state(), s.doc.mask_state());
        assert_eq!(restored.layer_id(restored.active), created);
        assert_eq!(restored.layers[0].pixels, owner_pixels);
        assert_eq!(
            (0..coverage.len())
                .map(|i| restored.mask_plane(mask).unwrap().sample(i))
                .collect::<Vec<_>>(),
            coverage
        );
        assert!(restored.renderer.is_none());

        // The next typed brush contact must edit the fresh RGBA raster, even
        // when a group mask covers that raster's displayed result.
        call(
            &mut s,
            "brush.set",
            json!({"color":"#E04090","radius":2,"stabilizer":0.55,"pressure_size":true}),
        );
        call(
            &mut s,
            "canvas.stroke",
            json!({"points":[
                {"x":2,"y":2,"pressure":0.3,"time":0,"pen":true,"tool_override":"brush"},
                {"x":8,"y":5,"pressure":0.9,"time":30,"pen":true,"tool_override":"brush"}
            ]}),
        );
        assert!(s.doc.layers[s.doc.active].pixels.iter().any(|p| *p != 0));
        assert_eq!(s.doc.layers[0].pixels, owner_pixels);
        assert_eq!(samples(&s, mask), coverage);
        assert!(
            matches!(s.doc.mask_plane(mask).unwrap(), PlaneView::Dense(data) if data.as_ptr() == pointer)
        );
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert!(s.doc.layers[s.doc.active].pixels.iter().all(|p| *p == 0));
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: mask });
        let next = call(&mut s, "layer.add", args)["layer_id"]
            .as_u64()
            .unwrap();
        assert!(
            next > created,
            "Committed Undo must not reuse layer identities"
        );
        assert_eq!(s.doc.mask_state().next_id, before.raster_masks.next_id);
        assert_eq!(samples(&s, mask), coverage);
        assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
        check(&mut s);
    }
}

#[test]
fn raster_mask_add_layer_preflight_failures_preserve_target_storage_and_allocators() {
    use super::editing_tests::check;

    let (mut s, mask) = masked(12, 8);
    let coverage: Vec<_> = (0..12 * 8).map(|i| (i * 37) as u8).collect();
    s.doc
        .replace_mask_plane(mask, SamplePlane::Dense(coverage.clone()))
        .unwrap();
    s.replay_boundary().unwrap();
    for _ in 1..crate::engine::MAX_LAYERS {
        call(&mut s, "layer.add", json!({}));
    }
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":mask}}),
    );
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let pointer = match s.doc.mask_plane(mask).unwrap() {
        PlaneView::Dense(data) => data.as_ptr(),
        _ => panic!("Dense fixture"),
    };
    for (args, code) in [
        (json!({}), "layer_limit"),
        (json!({"parent_id":99999}), "invalid_group"),
    ] {
        let error = s.apply(Action::new("layer.add", args)).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(samples(&s, mask), coverage);
        assert!(
            matches!(s.doc.mask_plane(mask).unwrap(), PlaneView::Dense(data) if data.as_ptr() == pointer)
        );
        assert!(s
            .doc
            .layers
            .iter()
            .zip(&pixels)
            .all(|(l, p)| l.pixels == *p));
        check(&mut s);
    }
}
