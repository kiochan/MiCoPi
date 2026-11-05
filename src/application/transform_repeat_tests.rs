//! Repeat uses the original document-space matrix, never recomputed content pivots.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;

fn rotated() -> Session {
    let mut s = session();
    let mut p = vec![0; 12 * 8];
    p[12 + 2] = 0xff4477aa;
    s.doc.replace_region(0, 0, 12, 8, &p);
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"layer","angle":90,"pivot_x":4,"pivot_y":3,"method":"nearest"}),
    );
    assert_eq!(s.doc.layers[0].pixels[12 + 5], 0xff4477aa);
    s
}
#[test]
fn repeat_keeps_world_matrix_and_recipe_with_one_undo_and_event() {
    let mut s = rotated();
    let recipe = s.doc.last_transform();
    let original = s.doc.layers[0].pixels.clone();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.transform.repeat", json!({}));
    assert_eq!(s.doc.layers[0].pixels[4 * 12 + 5], 0xff4477aa);
    assert_eq!(s.doc.last_transform(), recipe);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(s.doc.last_transform(), recipe);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.last_transform().is_none());
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.last_transform(), recipe);
}
#[test]
fn repeat_copy_preserves_source_and_branches_and_undo_removes_whole_copy() {
    let mut s = rotated();
    call(&mut s, "layer.rename", json!({"name":"Ink"}));
    call(
        &mut s,
        "layer.set",
        json!({"opacity":0.5,"blend_mode":"multiply","alpha_locked":true}),
    );
    let before = s.doc.composition();
    let old = s.doc.layers[0].pixels.clone();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.transform.repeat_copy", json!({}));
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.layers[0].pixels, old);
    assert_eq!(s.doc.layers[1].pixels[4 * 12 + 5], 0xff4477aa);
    assert_eq!(s.doc.layers[1].opacity, s.doc.layers[0].opacity);
    assert_eq!(s.doc.layers[1].mode, s.doc.layers[0].mode);
    assert!(!s.doc.alpha_locked(1));
    assert_ne!(s.doc.layer_id(0), s.doc.layer_id(1));
    s.doc.composition().validate_layer_metadata().unwrap();
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    let after = s.doc.composition();
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.active, 0);
    // Stable identity allocation is monotonic through undo.
    let mut restored = s.doc.composition();
    restored.next_layer_id = before.next_layer_id;
    assert_eq!(restored, before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), after);
    check(&mut s);
}
#[test]
fn repeat_selected_copy_extracts_only_selected_raw_pixels_and_moves_mask_once() {
    let mut s = session();
    s.doc.replace_region(
        0,
        0,
        4,
        1,
        &[0xff0000ff, 0xffff0000, 0xff00ff00, 0xff777777],
    );
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    call(
        &mut s,
        "canvas.transform",
        json!({"translate_x":1,"method":"nearest"}),
    );
    let source = s.doc.layers[0].pixels.clone();
    let mask = s.doc.selection().clone();
    call(&mut s, "canvas.transform.repeat", json!({"duplicate":true}));
    assert_eq!(s.doc.layers[0].pixels, source);
    assert_eq!(
        s.doc.layers[1].pixels.iter().filter(|p| **p != 0).count(),
        1
    );
    assert_eq!(s.doc.layers[1].pixels[2], 0xff0000ff);
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage()[2],
        255
    );
    assert_eq!(s.doc.selection().previous, mask.previous);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.selection(), &mask);
    assert_eq!(s.doc.layers.len(), 1);

    // Reapplying a fractional selection extracts once, not again at the destination.
    let mut s = session();
    s.doc.replace_region(0, 0, 1, 1, &[0xff0000ff]);
    let mut coverage = vec![0; 12 * 8];
    coverage[0] = 128;
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(crate::engine::selection::Mask::new(12, 8, coverage).unwrap()),
            previous: None,
        })
        .unwrap();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "canvas.transform",
        json!({"translate_x":1,"method":"nearest"}),
    );
    assert_eq!(s.doc.layers[0].pixels[1], 0x800000ff);
    call(&mut s, "canvas.transform.repeat_copy", json!({}));
    assert_eq!(s.doc.layers[1].pixels[2], 0x400000ff);
    assert_eq!(
        s.doc.layers[1].pixels.iter().filter(|p| **p != 0).count(),
        1
    );
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage()[2],
        128
    );
    check(&mut s);
}
#[test]
fn repeat_recipe_queries_are_inert_and_save_open_and_geometry_restore_it() {
    let mut s = rotated();
    let state = s.state();
    let history = s.doc.history_bytes();
    let count = s.replay_journal(false).unwrap().entries.len();
    let recipe = call(&mut s, "canvas.transform.state", json!({}));
    assert_eq!(recipe["available"], true);
    assert_eq!(state, s.state());
    assert_eq!(history, s.doc.history_bytes());
    assert_eq!(count, s.replay_journal(false).unwrap().entries.len());
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("state.core.transform"));
    let path = std::env::temp_dir().join(format!("micopi-repeat-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(
        call(&mut reopened, "canvas.transform.state", json!({})),
        recipe
    );
    call(&mut reopened, "canvas.transform.repeat", json!({}));
    assert_eq!(reopened.doc.layers[0].pixels[4 * 12 + 5], 0xff4477aa);
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
    for (id, args) in [
        ("canvas.resize", json!({"width":13,"height":8})),
        ("canvas.rotate_90_cw", json!({})),
        ("canvas.image_size", json!({"width":6})),
    ] {
        call(&mut s, id, args);
        assert_eq!(
            call(&mut s, "canvas.transform.state", json!({})),
            json!({"available":false})
        );
        call(&mut s, "history.undo", json!({}));
        assert_eq!(call(&mut s, "canvas.transform.state", json!({})), recipe);
    }
    call(&mut s, "canvas.transform", json!({"target":"layer"}));
    assert_eq!(call(&mut s, "canvas.transform.state", json!({})), recipe);
}
#[test]
fn repeat_failures_restore_recipe_history_layer_count_and_outcomes() {
    let mut s = rotated();
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pixels = s.doc.layers[0].pixels.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"copy","action":"canvas.transform.repeat_copy","args":{}},{"id":"fail","action":"layer.select","args":{"layer_id":999999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.transform.repeat_copy", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
}
#[test]
fn repeat_missing_recipe_outline_copy_and_missing_mask_fail_without_new_history() {
    let mut s = session();
    assert!(s
        .apply(Action::new("canvas.transform.repeat", json!({})))
        .is_err());
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":2,"height":2}),
    );
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"selection_mask","translate_x":1}),
    );
    let h = s.doc.history_bytes();
    assert!(s
        .apply(Action::new("canvas.transform.repeat_copy", json!({})))
        .is_err());
    assert_eq!(s.doc.history_bytes(), h);
    call(&mut s, "selection.deselect", json!({}));
    let before = s.state();
    assert!(s
        .apply(Action::new("canvas.transform.repeat", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
}
#[test]
fn repeat_persisted_matrices_reject_singular_excessive_and_unknown_fields() {
    use crate::engine::transform::Recipe;
    let valid = json!({"matrix":[1,0,2,0,1,3],"target":"layer","method":"nearest"});
    assert!(serde_json::from_value::<Recipe>(valid.clone()).is_ok());
    for matrix in [
        json!([0, 0, 0, 0, 1, 0]),
        json!([257, 0, 0, 0, 1, 0]),
        json!([1, 0, 4194305, 0, 1, 0]),
        json!([1, 0, 0, 0, 1]),
        json!([1, 0, null, 0, 1, 0]),
    ] {
        let mut v = valid.clone();
        v["matrix"] = matrix;
        assert!(serde_json::from_value::<Recipe>(v).is_err());
    }
    let mut unknown = valid;
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<Recipe>(unknown).is_err());
}

#[test]
fn repeat_state_node_drives_copy_and_native_or_layer_limits_leave_no_partial_copy() {
    let mut s = rotated();
    let old = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[
        {"id":1,"type":"state.core.transform","position":[0,0],"parameters":{}},
        {"id":2,"type":"action.canvas.transform.repeat","position":[400,0],"parameters":{}}
    ],"edges":[{"from":1,"output":"available","to":2,"input":"duplicate"}]}}),
    );
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.layers.len(), 2);
    assert_eq!(s.doc.layers[0].pixels, old);
    check(&mut s);
    s.registry.native_plugins.remove("painting");
    let before = s.state();
    let history = s.doc.history_bytes();
    assert!(s
        .apply(Action::new("canvas.transform.repeat_copy", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);

    let mut s = rotated();
    while s.doc.layers.len() < crate::engine::MAX_LAYERS {
        call(&mut s, "layer.duplicate", json!({}));
    }
    let before = s.state();
    let history = s.doc.history_bytes();
    assert!(s
        .apply(Action::new("canvas.transform.repeat_copy", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
}
