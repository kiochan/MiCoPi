//! Real native action admission, selection, history and verified outcomes.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::json;

#[test]
fn standalone_atomic_native_edits_publish_one_outer_revision() {
    let mut s = session();
    for (id, args) in [
        ("native.checkerboard", json!({"cell":2})),
        ("native.invert", json!({})),
    ] {
        let revision = s.revision;
        let entries = s.journal.as_ref().unwrap().entries.len();
        call(&mut s, id, args);
        assert_eq!(s.revision, revision + 1, "{id}");
        assert_eq!(
            s.journal.as_ref().unwrap().entries.len(),
            entries + 1,
            "{id}"
        );
        check(&mut s);
    }
}

#[test]
fn hue_saturation_uses_native_pixels_one_undo_selection_and_verified_outcome() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":2,"y":1,"width":3,"height":2}),
    );
    let before = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = call(&mut s, "layer.hue_saturation", json!({"hue":120}));
    assert_eq!(result["implementation"], "independent Rust DLL");
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if (2..5).contains(&x) && (1..3).contains(&y) {
                    0x8000ff00
                } else {
                    0x80ff0000
                }
            );
        }
    }
    let after = s.doc.layers[0].pixels.clone();
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, after);
    check(&mut s);
    let state = s.state();
    let history = s.doc.history_bytes();
    assert!(s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[{"action":"layer.hue_saturation","args":{"lightness":50}},{"action":"native.invert_region","args":{"x":100,"y":0,"width":1,"height":1}}]}))).is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, after);
}

#[test]
fn pixel_locks_invalid_parameters_and_live_contacts_reject_without_dispatch() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#102030"}));
    call(
        &mut s,
        "layer.set",
        json!({"position_locked":true,"alpha_locked":true}),
    );
    call(&mut s, "layer.hue_saturation", json!({"hue":120}));
    assert_eq!(s.doc.layers[0].pixels[0] >> 24, 255);
    call(&mut s, "layer.set", json!({"pixel_locked":true}));
    let state = s.state();
    assert_eq!(
        s.apply(Action::new("layer.hue_saturation", json!({"hue":120})))
            .unwrap_err()
            .code,
        "pixel_locked"
    );
    assert_eq!(s.state(), state);
    call(&mut s, "layer.set", json!({"pixel_locked":false}));
    for args in [
        json!({"hue":181}),
        json!({"saturation":null}),
        json!({"lightness":-101}),
    ] {
        let state = s.state();
        assert!(s.apply(Action::new("layer.hue_saturation", args)).is_err());
        assert_eq!(s.state(), state);
    }
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 1.5,
        pressure: 0.5,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(pointer).unwrap();
    let pixels = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
    let instances = s.native_instances.len();
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        s.apply(Action::new("layer.hue_saturation", json!({"hue":120})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.native_instances.len(), instances);
    assert!(s.journal.as_ref().unwrap().entries.len() > events);
    assert!(!s.journal.as_ref().unwrap().entries.last().unwrap().success);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        time: 2,
        ..pointer
    })
    .unwrap();
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 3,
        ..pointer
    })
    .unwrap();
    check(&mut s);
}

#[test]
fn direct_color_edit_retains_owned_raw_mask_and_explicit_paint_target() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff0000"}));
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":91}));
    let masked_target = s.doc.composition();
    assert_eq!(
        s.apply(Action::new("layer.hue_saturation", json!({"hue":120})))
            .unwrap_err()
            .code,
        "mask_target_pending"
    );
    assert_eq!(s.doc.composition(), masked_target);
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    let before = s.doc.composition();
    let samples = (0..96)
        .map(|i| s.doc.mask_plane(mask).unwrap().sample(i))
        .collect::<Vec<_>>();
    call(&mut s, "layer.hue_saturation", json!({"hue":120}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0xff00ff00));
    assert_eq!(
        (0..96)
            .map(|i| s.doc.mask_plane(mask).unwrap().sample(i))
            .collect::<Vec<_>>(),
        samples
    );
    // The action changes existing raster bytes only; mask metadata/target and
    // source composition stay untouched.
    assert_eq!(s.doc.composition(), before);
    check(&mut s);
}
