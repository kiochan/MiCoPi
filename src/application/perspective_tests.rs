//! Real DLL perspective outcomes, persistence and atomic failure contracts.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":4}));
    let pixels: Vec<_> = (0..16).map(|i| 0xff000000 | i).collect();
    s.doc.replace_region(0, 0, 4, 4, &pixels);
    s.replay_boundary().unwrap();
    s
}
fn args() -> serde_json::Value {
    json!({"target":"layer","quad":[[0,0],[2,0],[2,2],[0,4]],"method":"nearest"})
}
#[test]
fn projective_nearest_matches_analytic_inverse_and_repeat_preserves_world_homography() {
    let mut s = fixture();
    let old = s.doc.layers[0].pixels.clone();
    let before = s.doc.composition();
    let plan = call(&mut s, "canvas.transform.plan", args());
    assert_eq!(plan["matrix"], json!([1., 0., 0., 0., 1., 0.]));
    assert_eq!(plan["denominator"], json!([0.25, 0., 1.]));
    assert_eq!(plan["identity_matrix"], false);
    let count = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.transform", args());
    assert_eq!(
        s.doc.layers[0].pixels,
        [old[0], old[2], 0, 0, old[4], old[10], 0, 0, old[8], 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    let warped = s.doc.layers[0].pixels.clone();
    let recipe = s.doc.last_transform();
    call(&mut s, "canvas.transform.repeat_copy", json!({}));
    assert_eq!(s.doc.layers[0].pixels, warped);
    assert_eq!(
        s.doc.layers[1].pixels,
        [old[0], 0, 0, 0, old[4], 0, 0, 0, old[8], 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(s.doc.last_transform(), recipe);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, old);
    let mut restored = s.doc.composition();
    restored.next_layer_id = before.next_layer_id;
    assert_eq!(restored, before);
}
#[test]
fn projective_selected_pixels_and_outline_share_coverage_and_preserve_reselect() {
    for mask_only in [false, true] {
        let mut s = fixture();
        call(&mut s, "selection.all", json!({}));
        let before = s.doc.composition();
        let old = s.doc.layers[0].pixels.clone();
        let mut a = args();
        a["target"] = json!(if mask_only {
            "selection_mask"
        } else {
            "selection"
        });
        if mask_only {
            call(
                &mut s,
                "layer.set",
                json!({"visible":false,"alpha_locked":true}),
            );
        }
        call(&mut s, "canvas.transform", a);
        assert_eq!(
            s.doc.selection().current.as_ref().unwrap().coverage(),
            [255, 255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0]
        );
        assert_eq!(s.doc.selection().previous, before.selection.previous);
        if mask_only {
            assert_eq!(s.doc.layers[0].pixels, old);
        } else {
            assert_eq!(s.doc.layers[0].pixels[5], old[10]);
        }
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, old);
        assert_eq!(s.doc.selection(), &before.selection);
    }
}
#[test]
fn projective_project_and_recording_roundtrip_restore_the_recipe_and_outcomes() {
    let mut s = fixture();
    call(&mut s, "canvas.transform", args());
    let recipe = call(&mut s, "canvas.transform.state", json!({}));
    assert_eq!(recipe["denominator"], json!([0.25, 0., 1.]));
    let path = std::env::temp_dir().join(format!("micopi-projective-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(
        call(&mut reopened, "canvas.transform.state", json!({})),
        recipe
    );
    assert_eq!(reopened.doc.layers[0].pixels, s.doc.layers[0].pixels);
    check(&mut reopened);
    call(&mut reopened, "canvas.transform.repeat", json!({}));
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn projective_bad_geometry_locks_atomic_and_journal_failures_restore_all_state() {
    let mut s = fixture();
    let old = s.doc.layers[0].pixels.clone();
    let before = s.state();
    let history = s.doc.history_bytes();
    for a in [
        json!({"quad":[[0,0],[4,4],[4,0],[0,4]]}),
        json!({"quad":[[0,0],[4,0],[1,1],[0,4]]}),
        json!({"quad":[[0,0],[2,0],[2,2],[0,4]],"angle":15}),
    ] {
        assert!(s.apply(Action::new("canvas.transform", a)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    assert!(s.apply(Action::new("canvas.transform", args())).is_err());
    assert_eq!(s.doc.layers[0].pixels, old);
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let history = s.doc.history_bytes();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"warp","action":"canvas.transform","args":args()},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(r["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, old);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s.apply(Action::new("canvas.transform", args())).is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, old);
    assert_eq!(s.doc.history_bytes(), history);
}

#[test]
fn projective_geometry_query_observes_active_contacts_and_recipes_reject_bad_rows() {
    let mut s = fixture();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":1,"y":1,"pressure":0.5,"pen":true,"time":0}),
    );
    let before = s.state();
    let history = s.doc.history_bytes();
    let count = s.replay_journal(false).unwrap().entries.len();
    let plan = call(&mut s, "canvas.transform.plan", args());
    assert_eq!(plan["drawing"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count);
    assert_eq!(
        s.apply(Action::new("canvas.transform", args()))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(s.is_drawing());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":1,"y":1,"pressure":0.5,"pen":true,"time":1}),
    );
    for row in [
        json!([0, 0, 0]),
        json!([17, 0, 1]),
        json!([0, 0]),
        json!([0, null, 1]),
    ] {
        let value =
            json!({"matrix":[1,0,0,0,1,0],"denominator":row,"target":"layer","method":"nearest"});
        assert!(serde_json::from_value::<crate::engine::transform::Recipe>(value).is_err());
    }
}
#[test]
fn projective_oversized_compressed_mask_rejects_without_publishing_pixels_or_recipe() {
    let mut s = session();
    let n = 1024;
    call(&mut s, "canvas.new", json!({"width":n,"height":n}));
    let mask = crate::engine::selection::Mask::new(
        n,
        n,
        (0..n * n)
            .map(|i| if (i % n + i / n) % 2 == 0 { 255 } else { 0 })
            .collect(),
    )
    .unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask.clone()),
            previous: Some(mask),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let err = s
        .apply(Action::new(
            "canvas.transform",
            json!({"target":"selection_mask","quad":[[30,0],[980,50],[1010,1000],[0,980]]}),
        ))
        .unwrap_err();
    assert!(err.message.contains("compressed metadata limit"), "{err:?}");
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
}

#[test]
fn projective_repeat_rejects_an_active_contact_without_ending_its_native_state() {
    let mut s = fixture();
    call(&mut s, "canvas.transform", args());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":1,"y":1,"pressure":0.5,"pen":true,"time":0}),
    );
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    for id in ["canvas.transform.repeat", "canvas.transform.repeat_copy"] {
        assert_eq!(
            s.apply(Action::new(id, json!({}))).unwrap_err().code,
            "stroke_active"
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(s.is_drawing());
    }
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":1,"y":1,"pressure":0.5,"pen":true,"time":1}),
    );
    assert!(!s.is_drawing());
    check(&mut s);
}
