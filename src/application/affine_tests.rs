//! Shared transform contracts against real staged native callbacks.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":1}));
    s.doc.replace_region(
        0,
        0,
        4,
        1,
        &[0xff0000ff, 0xffff0000, 0x00112233, 0xff00ff00],
    );
    s.replay_boundary().unwrap();
    s
}
#[test]
fn affine_selected_pixels_masks_and_resolved_source_context_restore_in_one_undo() {
    let mut s = fixture();
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":2,"height":1}),
    );
    let composition = s.doc.composition();
    let original = s.doc.layers[0].pixels.clone();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(
        &mut s,
        "canvas.transform",
        json!({"translate_x":1,"method":"nearest"}),
    );
    assert_eq!(
        s.doc.layers[0].pixels,
        [0, 0xff0000ff, 0xffff0000, 0xff00ff00]
    );
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [0, 255, 255, 0]
    );
    assert_eq!(s.doc.selection().previous, composition.selection.previous);
    let journal = s.replay_journal(false).unwrap();
    assert_eq!(journal.entries.len(), count + 1);
    let events = journal.events(count, 1).unwrap();
    assert_eq!(
        events["events"][0]["source"]["context"]["transform"]["pivot"],
        json!([1., 0.5])
    );
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(s.doc.composition(), composition);
    call(&mut s, "history.redo", json!({}));
    check(&mut s);
}
#[test]
fn affine_flip_layer_and_fractional_translation_keep_explicit_edge_semantics() {
    let mut s = fixture();
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "canvas.transform",
        json!({"flip_x":true,"target":"layer"}),
    );
    assert_eq!(
        s.doc.layers[0].pixels,
        original.iter().copied().rev().collect::<Vec<_>>()
    );
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.transform",
        json!({"translate_x":0.5,"target":"layer"}),
    );
    assert_eq!(s.doc.layers[0].pixels[0], 0x800000ff);
    assert_eq!(s.doc.layers[0].pixels[1], 0xff800080);
    check(&mut s);
}
#[test]
fn affine_defaults_validation_locks_masks_and_atomic_failure_preserve_sources() {
    let mut s = fixture();
    let original = s.doc.layers[0].pixels.clone();
    for id in ["canvas.transform.open", "canvas.transform.configure"] {
        let args = if id.ends_with("configure") {
            json!({"parameters":{"angle":20}})
        } else {
            json!({})
        };
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "ui_unavailable"
        );
    }
    let history = s.doc.history_bytes();
    call(&mut s, "canvas.transform", json!({}));
    assert_eq!(s.doc.history_bytes(), history);
    for args in [
        json!({"scale_x":0}),
        json!({"scale_y":101}),
        json!({"pivot_x":0}),
        json!({"method":"area"}),
        json!({"target":"selection_mask"}),
    ] {
        assert!(s.apply(Action::new("canvas.transform", args)).is_err());
        assert_eq!(s.doc.layers[0].pixels, original);
    }
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    assert!(s
        .apply(Action::new("canvas.transform", json!({"translate_x":1})))
        .is_err());
    assert_eq!(s.doc.layers[0].pixels, original);
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":2,"height":1}),
    );
    call(&mut s, "layer.set", json!({"visible":false}));
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"selection_mask","translate_x":1}),
    );
    assert_eq!(s.doc.layers[0].pixels, original);
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let state = s.state();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"move","action":"canvas.transform","args":{"target":"selection_mask","translate_x":1}},{"id":"fail","action":"layer.select","args":{"layer_id":9999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"target":"selection_mask","translate_x":1})
        ))
        .is_err());
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(s.doc.history_bytes(), history);
}
#[test]
fn affine_workflow_is_inert_until_run_and_project_replays_the_outcome() {
    let mut s = fixture();
    let old = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.transform","position":[0,0],"parameters":{"flip_x":true,"target":"layer"}}],"edges":[]}}),
    );
    assert_eq!(s.doc.layers[0].pixels, old);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    let path = std::env::temp_dir().join(format!("micopi-affine-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.layers[0].pixels, s.doc.layers[0].pixels);
    assert_eq!(reopened.doc.composition(), s.doc.composition());
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
#[cfg(feature = "desktop")]
fn affine_preview_borrows_sources_and_matches_the_committed_filtered_composite() {
    let mut s = fixture();
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#edaa7780"}));
    call(
        &mut s,
        "layer.set",
        json!({"opacity":0.6,"blend_mode":"multiply"}),
    );
    let graph = json!({"enabled":true,"nodes":[{"id":0,"kind":"layer","index":0,"position":[0,0]},{"id":1,"kind":"filter","filter":"brightness","amount":0.125,"position":[200,0]},{"id":2,"kind":"layer","index":1,"position":[0,200]},{"id":3,"kind":"blend","mode":"multiply","opacity":0.75,"position":[400,0]},{"id":4,"kind":"output","position":[600,0]}],"edges":[{"from":0,"to":1,"input":0},{"from":1,"to":3,"input":0},{"from":2,"to":3,"input":1},{"from":3,"to":4,"input":0}]});
    call(&mut s, "graph.set", json!({"graph":graph}));
    let original = s.doc.layers[0].pixels.clone();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let count = s.replay_journal(false).unwrap().entries.len();
    let p = crate::plugins::affine::Parameters {
        translate_x: 0.5,
        ..Default::default()
    };
    let (preview, _) = crate::plugins::affine::preview(&s, p).unwrap();
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count);
    call(&mut s, "canvas.transform", serde_json::to_value(p).unwrap());
    s.doc.compose();
    assert_eq!(s.doc.composite, preview);
    check(&mut s);
}
#[test]
fn affine_preview_request_preserves_revision_state_and_rejects_atomic_effects() {
    let mut s = fixture();
    s.runtime = "desktop";
    let state = s.state();
    let history = s.doc.history_bytes();
    assert_eq!(
        call(
            &mut s,
            "canvas.transform.configure",
            json!({"parameters":{"angle":20}})
        )["queued"],
        true
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.ui_effects.back().unwrap()["id"], "transform.parameters");
    assert!(s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[{"id":"preview","action":"canvas.transform.configure","args":{"parameters":{"angle":20}}}]}))).is_err());
    assert!(s
        .apply(Action::new(
            "canvas.transform.configure",
            json!({"parameters":{"pivot_x":1}})
        ))
        .is_err());
}
#[test]
fn affine_layer_plan_uses_raw_nontransparent_content_and_queries_do_not_mutate() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":32,"height":24}));
    assert!(s
        .apply(Action::new(
            "canvas.transform.plan",
            json!({"target":"layer"})
        ))
        .unwrap_err()
        .message
        .contains("nontransparent"));
    let mut pixels = vec![0; 32 * 24];
    for y in 4..6 {
        for x in 5..9 {
            pixels[y * 32 + x] = 0xff7788aa;
        }
    }
    pixels[4 * 32 + 5] = 0xffff0000;
    pixels[4 * 32 + 8] = 0xff00ff00;
    pixels[20] = 0x00112233;
    s.doc.replace_region(0, 0, 32, 24, &pixels);
    s.replay_boundary().unwrap();
    let state = s.state();
    let count = s.replay_journal(false).unwrap().entries.len();
    let history = s.doc.history_bytes();
    let plan = call(
        &mut s,
        "canvas.transform.plan",
        json!({"target":"layer","flip_x":true}),
    );
    assert_eq!(plan["source_bounds"], json!([5, 4, 9, 6]));
    assert_eq!(plan["pivot_x"], 7.);
    assert_eq!(plan["pivot_y"], 5.);
    assert_eq!(plan["drawing"], false);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count);
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"layer","flip_x":true}),
    );
    assert_eq!(s.doc.layers[0].pixels[4 * 32 + 8], 0xffff0000);
    assert_eq!(s.doc.layers[0].pixels[4 * 32 + 5], 0xff00ff00);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
}
#[test]
fn affine_oversized_transformed_mask_rejects_before_publication() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1024,"height":1024}));
    let mask = crate::engine::selection::Mask::new(
        1024,
        1024,
        (0..1024 * 1024)
            .map(|i| {
                if (i % 1024 + i / 1024) % 2 == 0 {
                    255
                } else {
                    0
                }
            })
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
    let state = s.state();
    let before = s.doc.composition();
    let history = s.doc.history_bytes();
    let error = s
        .apply(Action::new(
            "canvas.transform",
            json!({"target":"selection_mask","angle":17.3,"scale_x":0.9,"scale_y":0.9}),
        ))
        .unwrap_err();
    assert!(
        error.message.contains("compressed metadata limit"),
        "{error:?}"
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.history_bytes(), history);
}
#[test]
fn affine_geometry_query_observes_a_contact_without_finishing_or_revising_it() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":32,"height":24}));
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":8,"y":8,"pressure":0.5,"pen":true,"time":0}),
    );
    assert!(s.is_drawing());
    let before = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "canvas.transform.plan",
        json!({"target":"layer","angle":15}),
    );
    assert_eq!(result["drawing"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(s.is_drawing());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":8,"y":8,"pressure":0.5,"pen":true,"time":1}),
    );
}
#[test]
fn affine_geometry_node_outputs_bind_to_the_mutating_node_in_one_atomic_run() {
    let mut s = fixture();
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.transform.plan","position":[0,0],"parameters":{"target":"layer"}},{"id":2,"type":"action.canvas.transform","position":[400,0],"parameters":{"target":"layer","angle":17,"translate_x":0.5}}],"edges":[{"from":1,"output":"pivot_x","to":2,"input":"pivot_x"},{"from":1,"output":"pivot_y","to":2,"input":"pivot_y"}]}}),
    );
    assert_eq!(s.doc.layers[0].pixels, original);
    let count = s.replay_journal(false).unwrap().entries.len();
    let result = call(&mut s, "workflow.run", json!({"atomic":true}));
    assert_eq!(result["ok"], true, "{result}");
    assert_ne!(s.doc.layers[0].pixels, original);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
}
