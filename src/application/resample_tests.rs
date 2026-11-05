//! Image size contracts against the staged DLL and host outcome journal.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
#[test]
fn image_size_scales_every_layer_mask_and_preserves_structural_identity() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":1}));
    s.doc.replace_region(0, 0, 2, 1, &[0x00ff0000, 0xff0000ff]);
    call(&mut s, "layer.rename", json!({"name":"Ink 墨"}));
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true,"opacity":0.5}),
    );
    call(&mut s, "layer.add", json!({}));
    s.doc.replace_region(0, 0, 2, 1, &[0xff000000, 0xffffffff]);
    let mask = |v| crate::engine::selection::Mask::new(2, 1, v).unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask(vec![0, 255])),
            previous: Some(mask(vec![255, 0])),
        })
        .unwrap();
    let composition = s.doc.composition();
    s.replay_boundary().unwrap();
    let events = s.replay_journal(false).unwrap().entries.len();
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":3,"height":1,"lock_aspect":false}),
    );
    assert_eq!((s.doc.width, s.doc.height), (3, 1));
    assert_eq!(s.doc.layers[0].pixels, [0, 0x800000ff, 0xff0000ff]);
    assert_eq!(s.doc.layers[1].pixels, [0xff000000, 0xff808080, 0xffffffff]);
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [0, 128, 255]
    );
    assert_eq!(
        s.doc.selection().previous.as_ref().unwrap().coverage(),
        [255, 128, 0]
    );
    assert_eq!(s.doc.composition().graph, composition.graph);
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    assert!(!s.doc.layers[0].visible);
    assert!(s.doc.alpha_locked(0));
    assert_eq!(s.doc.layers[0].opacity, 128);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), events + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, [0x00ff0000, 0xff0000ff]);
    call(&mut s, "history.redo", json!({}));
    check(&mut s);
}
#[test]
fn nearest_preserves_exact_words_and_locked_aspect_uses_the_requested_axis() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    let source = [0x00112233, 0x80775533, 3, 4, 5, 6];
    s.doc.replace_region(0, 0, 3, 2, &source);
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":6,"method":"nearest"}),
    );
    assert_eq!((s.doc.width, s.doc.height), (6, 4));
    for y in 0..4 {
        for x in 0..6 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 6 + x],
                source[(y / 2) * 3 + x / 2]
            );
        }
    }
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.image_size",
        json!({"height":5,"method":"nearest"}),
    );
    assert_eq!((s.doc.width, s.doc.height), (8, 5));
    check(&mut s);
}
#[test]
fn identity_size_preserves_hidden_rgb_and_does_not_add_history() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":1}));
    s.doc.replace_region(0, 0, 2, 1, &[0x00112233, 0xffabcdef]);
    s.replay_boundary().unwrap();
    let history = s.doc.history_bytes();
    call(&mut s, "canvas.image_size", json!({"width":2}));
    assert_eq!(s.doc.layers[0].pixels, [0x00112233, 0xffabcdef]);
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}
#[test]
fn invalid_dimensions_arguments_budget_and_revisions_leave_source_intact() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let state = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    for args in [
        json!({}),
        json!({"width":3,"height":2}),
        json!({"width":3,"lock_aspect":false}),
        json!({"width":0}),
        json!({"width":5000}),
        json!({"width":2.5}),
        json!({"width":3,"method":"unknown"}),
    ] {
        assert!(s.apply(Action::new("canvas.image_size", args)).is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    let mut stale = Action::new("canvas.image_size", json!({"width":6}));
    stale.expected_revision = Some(s.revision + 1);
    assert!(s.apply(stale).is_err());
    call(&mut s, "canvas.new", json!({"width":4096,"height":1}));
    assert!(s
        .apply(Action::new("canvas.image_size", json!({"width":1})))
        .is_err());
    call(&mut s, "canvas.new", json!({"width":2,"height":1}));
    for _ in 0..4 {
        call(&mut s, "layer.add", json!({}));
    }
    let state = s.state();
    assert!(s
        .apply(Action::new(
            "canvas.image_size",
            json!({"width":4096,"height":4096,"lock_aspect":false})
        ))
        .is_err());
    assert_eq!(s.state(), state);
}
#[test]
fn image_size_atomic_and_partial_journal_failures_restore_source_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let state = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"scale","action":"canvas.image_size","args":{"width":24}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.image_size", json!({"width":24})))
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
fn resampling_workflow_editing_is_inert_and_project_reopens_with_verified_outcomes() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.canvas.image_size"));
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.image_size","position":[0.,0.],"parameters":{"width":18,"method":"nearest"}}],"edges":[]}}),
    );
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (18, 12));
    let path = std::env::temp_dir().join(format!("micopi-resample-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    assert_eq!((reopened.doc.width, reopened.doc.height), (18, 12));
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn enlarged_fractional_mask_budget_failure_preserves_original_document() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":256,"height":256}));
    let mut seed = 17u32;
    let bytes = (0..256 * 256)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    let mask = crate::engine::selection::Mask::new(256, 256, bytes).unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask),
            previous: None,
        })
        .unwrap();
    s.replay_boundary().unwrap();
    let state = s.state();
    let selection = s.doc.selection().clone();
    let history = s.doc.history_bytes();
    let error = s
        .apply(Action::new("canvas.image_size", json!({"width":512})))
        .unwrap_err();
    assert_eq!(error.code, "image_size");
    assert!(
        error.message.contains("compressed metadata limit"),
        "{error}"
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.selection(), &selection);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, vec![0; 256 * 256]);
}

#[test]
fn area_keeps_thin_line_energy_and_fractional_masks_in_one_verified_edit() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":8,"height":4}));
    let pixels: Vec<_> = (0..32)
        .map(|i| if i % 8 == 0 { 0xffffffff } else { 0xff000000 })
        .collect();
    s.doc.replace_region(0, 0, 8, 4, &pixels);
    let mask = crate::engine::selection::Mask::new(
        8,
        4,
        (0..32).map(|i| if i % 8 == 0 { 255 } else { 0 }).collect(),
    )
    .unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask.clone()),
            previous: Some(mask),
        })
        .unwrap();
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"visible":false}),
    );
    let composition = s.doc.composition();
    s.replay_boundary().unwrap();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":2,"height":1,"lock_aspect":false,"method":"area"}),
    );
    assert_eq!(s.doc.layers[0].pixels, [0xff404040, 0xff000000]);
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [64, 0]
    );
    assert_eq!(
        s.doc.selection().previous.as_ref().unwrap().coverage(),
        [64, 0]
    );
    assert_eq!(s.doc.composition().graph, composition.graph);
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.composition(), composition);
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":2,"height":1,"lock_aspect":false,"method":"bilinear"}),
    );
    assert_eq!(s.doc.layers[0].pixels, [0xff000000; 2]);
}
#[test]
fn area_workflow_and_journal_failure_preserve_atomic_outcomes() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":1}));
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.image_size","position":[0.,0.],"parameters":{"width":6,"method":"area"}}],"edges":[]}}),
    );
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (6, 4));
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"scale","action":"canvas.image_size","args":{"width":6,"method":"area"}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    check(&mut s);
    let composition = s.doc.composition();
    let state = s.state();
    let history = s.doc.history_bytes();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.image_size",
            json!({"width":6,"method":"area"})
        ))
        .is_err());
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, original);
}
