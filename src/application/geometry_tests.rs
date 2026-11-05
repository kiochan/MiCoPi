//! Exact geometry through staged DLLs, structural history, selection and replay.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
#[test]
fn canvas_quarter_turn_retains_hidden_locked_layers_ids_graphs_and_exact_words() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    s.doc
        .replace_region(0, 0, 3, 2, &[0x00112233, 0x80402010, 3, 4, 5, 6]);
    call(&mut s, "layer.rename", json!({"name":"Ink 墨"}));
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true,"opacity":0.5}),
    );
    call(&mut s, "layer.add", json!({}));
    s.doc.replace_region(0, 0, 3, 2, &[11, 12, 13, 14, 15, 16]);
    let original = s
        .doc
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect::<Vec<_>>();
    let composition = s.doc.composition();
    let active = s.doc.active;
    s.replay_boundary().unwrap();
    let events = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.rotate_90_cw", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (2, 3));
    assert_eq!(
        s.doc.layers[0].pixels,
        vec![4, 0x00112233, 5, 0x80402010, 6, 3]
    );
    assert_eq!(s.doc.layers[1].pixels, vec![14, 11, 15, 12, 16, 13]);
    assert_eq!(s.doc.composition().graph, composition.graph);
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    assert_eq!(s.doc.active, active);
    assert_eq!(s.doc.layers[0].opacity, 128);
    assert!(!s.doc.layers[0].visible);
    assert!(s.doc.alpha_locked(0));
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), events + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (3, 2));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, original[0]);
    call(&mut s, "history.redo", json!({}));
    call(&mut s, "canvas.rotate_90_ccw", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original[0]);
    assert_eq!(s.doc.layers[1].pixels, original[1]);
    check(&mut s);
}
#[test]
fn both_selection_masks_follow_global_geometry_with_fractional_coverage() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    let mask = |v| crate::engine::selection::Mask::new(3, 2, v).unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask(vec![0, 64, 128, 192, 254, 255])),
            previous: Some(mask(vec![255, 0, 128, 64, 0, 192])),
        })
        .unwrap();
    let original = s.doc.selection().clone();
    call(&mut s, "canvas.rotate_90_cw", json!({}));
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [192, 0, 254, 64, 255, 128]
    );
    assert_eq!(
        s.doc.selection().previous.as_ref().unwrap().coverage(),
        [64, 255, 0, 0, 192, 128]
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.selection(), &original);
    check(&mut s);
}
#[test]
fn canvas_flips_and_half_turn_restore_every_stored_pixel_after_inverse() {
    for id in [
        "canvas.flip_horizontal",
        "canvas.flip_vertical",
        "canvas.rotate_180",
    ] {
        let mut s = session();
        call(&mut s, "native.checkerboard", json!({"cell":3}));
        let original = s.doc.layers[0].pixels.clone();
        call(&mut s, id, json!({}));
        call(&mut s, id, json!({}));
        assert_eq!(s.doc.layers[0].pixels, original);
        check(&mut s);
    }
}
#[test]
fn canvas_geometry_atomic_and_journal_failures_restore_dimensions_pixels_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "selection.all", json!({}));
    let state = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"turn","action":"canvas.rotate_90_cw"},{"id":"flip","action":"canvas.flip_vertical"},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.rotate_90_cw", json!({})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}
#[test]
fn layer_turn_uses_integer_center_clipping_and_preserves_selection() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    s.doc.replace_region(0, 0, 3, 2, &[1, 2, 3, 4, 5, 6]);
    call(&mut s, "layer.rotate_90_cw", json!({}));
    assert_eq!(s.doc.layers[0].pixels, vec![5, 2, 0, 6, 3, 0]);
    assert_eq!((s.doc.width, s.doc.height), (3, 2));
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    let selection = s.doc.selection().clone();
    call(&mut s, "layer.rotate_90_cw", json!({}));
    assert_eq!(s.doc.layers[0].pixels, vec![5, 2, 3, 4, 5, 6]);
    assert_eq!(s.doc.selection(), &selection);
    check(&mut s);
}
#[test]
fn layer_visibility_and_alpha_guards_reject_without_mutation() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    s.doc.replace_region(0, 0, 3, 2, &[0xff123456; 6]);
    call(&mut s, "layer.set", json!({"visible":false}));
    let before = s.state();
    assert!(s
        .apply(Action::new("layer.rotate_90_cw", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    call(
        &mut s,
        "layer.set",
        json!({"visible":true,"alpha_locked":true}),
    );
    let before = s.state();
    let result = s.apply(Action::new("layer.rotate_90_ccw", json!({})));
    assert!(
        result.is_err(),
        "state={:?}, pixels={:?}",
        s.state(),
        s.doc.layers[0].pixels
    );
    assert_eq!(s.state(), before);
    call(&mut s, "canvas.new", json!({"width":2,"height":2}));
    s.doc.replace_region(
        0,
        0,
        2,
        2,
        &[0xff111111, 0xff222222, 0xff333333, 0xff444444],
    );
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    call(&mut s, "layer.rotate_90_cw", json!({}));
    assert_eq!(
        s.doc.layers[0].pixels,
        vec![0xff333333, 0xff111111, 0xff444444, 0xff222222]
    );
}
#[test]
fn geometry_project_and_nodes_preserve_outcomes_without_eager_execution() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let original = s.doc.layers[0].pixels.clone();
    let catalog = call(&mut s, "node.catalog", json!({}));
    assert!(catalog.to_string().contains("action.canvas.rotate_90_cw"));
    let graph = json!({"version":1,"nodes":[{"id":1,"type":"action.canvas.rotate_90_cw","position":[0.,0.],"parameters":{}}],"edges":[]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    let path = std::env::temp_dir().join(format!("micopi-geometry-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let pixels = s.doc.layers[0].pixels.clone();
    let composition = s.doc.composition();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!((reopened.doc.width, reopened.doc.height), (8, 12));
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    assert_eq!(reopened.doc.composition(), composition);
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
