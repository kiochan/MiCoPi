//! Local shear order, shared action validation and native outcome contracts.
use super::editing_tests::{call, check, session};
use super::*;
use crate::plugins::affine::Parameters;
use serde_json::json;

#[test]
fn skew_matrix_uses_scale_then_horizontal_vertical_shear_then_rotation() {
    let p = Parameters {
        scale_x: 2.,
        scale_y: 3.,
        skew_x: 45.,
        skew_y: 45.,
        angle: 90.,
        pivot_x: Some(0.),
        pivot_y: Some(0.),
        ..Default::default()
    };
    let (matrix, _) = p.matrix([0, 0, 12, 8]).unwrap();
    for (a, b) in matrix.into_iter().zip([-2., -6., 0., 2., 3., 0.]) {
        assert!((a - b).abs() < 1e-12, "{matrix:?}");
    }
    let saved = serde_json::to_value(Parameters::default()).unwrap();
    assert!(saved.get("skew_x").is_none() && saved.get("skew_y").is_none());
    for p in [
        Parameters {
            skew_x: f64::NAN,
            ..Default::default()
        },
        Parameters {
            skew_y: 80.01,
            ..Default::default()
        },
        Parameters {
            scale_x: 100.,
            scale_y: 100.,
            skew_x: 80.,
            skew_y: 80.,
            ..Default::default()
        },
    ] {
        assert!(p.matrix([0, 0, 12, 8]).is_err());
    }
}
#[test]
fn skew_pixels_mask_and_repeat_copy_preserve_one_atomic_undo() {
    let mut s = session();
    let mut pixels = vec![0; 12 * 8];
    pixels[12 + 1] = 0xffaa66cc;
    s.doc.replace_region(0, 0, 12, 8, &pixels);
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":1,"y":1,"width":1,"height":1}),
    );
    let before = s.doc.composition();
    let count = s.replay_journal(false).unwrap().entries.len();
    let args = json!({"skew_x":45,"pivot_x":0.5,"pivot_y":0.5,"method":"nearest"});
    let plan = call(&mut s, "canvas.transform.plan", args.clone());
    assert!((plan["matrix"][1].as_f64().unwrap() - 1.).abs() < 1e-12);
    call(&mut s, "canvas.transform", args);
    assert_eq!(s.doc.layers[0].pixels[12 + 2], 0xffaa66cc);
    assert_eq!(s.doc.layers[0].pixels[12 + 1], 0);
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage()[12 + 2],
        255
    );
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    let transformed = s.doc.layers[0].pixels.clone();
    call(&mut s, "canvas.transform.repeat_copy", json!({}));
    assert_eq!(s.doc.layers[0].pixels, transformed);
    assert_eq!(s.doc.layers[1].pixels[12 + 3], 0xffaa66cc);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    let mut restored = s.doc.composition();
    restored.next_layer_id = before.next_layer_id;
    assert_eq!(restored, before);
}
#[test]
fn skew_schema_coefficient_and_atomic_failures_preserve_pixels_recipe_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let before = s.state();
    let history = s.doc.history_bytes();
    let pixels = s.doc.layers[0].pixels.clone();
    for args in [
        json!({"skew_x":81}),
        json!({"skew_y":-81}),
        json!({"scale_x":100,"scale_y":100,"skew_x":80,"skew_y":80}),
    ] {
        assert!(s
            .apply(Action::new("canvas.transform", args.clone()))
            .is_err());
        assert!(s.apply(Action::new("canvas.transform.plan", args)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"skew","action":"canvas.transform","args":{"skew_x":25,"skew_y":-12}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.transform", json!({"skew_y":25})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
}
