//! Native trim bounds and atomic structural publication across shared clients.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn artwork() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":8,"height":6}));
    let mut pixels = vec![0x00112233; 48];
    for y in 1..5 {
        for x in 2..6 {
            pixels[y * 8 + x] = 0x80402010;
        }
    }
    s.doc.replace_region(0, 0, 8, 6, &pixels);
    s
}
#[test]
fn bounds_are_read_only_and_trim_preserves_raw_hidden_layers_masks_and_identity() {
    let mut s = artwork();
    call(&mut s, "layer.rename", json!({"name":"Ink 墨"}));
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    call(&mut s, "layer.add", json!({}));
    s.doc.replace_region(0, 0, 8, 6, &vec![0xffabcdef; 48]);
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true,"opacity":0.5}),
    );
    let mask = crate::engine::selection::Mask::new(8, 6, (0..48).map(|i| (i * 5) as u8).collect())
        .unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask.clone()),
            previous: Some(mask),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    let state = s.state();
    let composition = s.doc.composition();
    let pixels = s
        .doc
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect::<Vec<_>>();
    let history = s.doc.history_bytes();
    let events = s.replay_journal(false).unwrap().entries.len();
    let b = call(&mut s, "canvas.trim_bounds", json!({}));
    assert_eq!(
        (
            b["x"].as_u64(),
            b["y"].as_u64(),
            b["width"].as_u64(),
            b["height"].as_u64()
        ),
        (Some(2), Some(1), Some(4), Some(4))
    );
    assert_eq!(b["would_change"], true);
    assert_eq!(b["empty"], false);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), events);
    call(&mut s, "canvas.trim", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (4, 4));
    assert_eq!(s.doc.layers[0].pixels, vec![0x80402010; 16]);
    assert_eq!(s.doc.layers[1].pixels, vec![0xffabcdef; 16]);
    assert_eq!(s.doc.composition().graph, composition.graph);
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    assert_eq!(s.doc.active, 1);
    assert!(!s.doc.layers[1].visible);
    assert!(s.doc.alpha_locked(0));
    assert!(s.doc.alpha_locked(1));
    assert_eq!(s.doc.layers[1].opacity, 128);
    let coverage = s.doc.selection().current.as_ref().unwrap().coverage();
    for y in 0..4 {
        assert_eq!(
            &coverage[y * 4..y * 4 + 4],
            &(0..4)
                .map(|x| (((y + 1) * 8 + x + 2) * 5) as u8)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(s.doc.selection().current, s.doc.selection().previous);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), events + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    for (layer, source) in s.doc.layers.iter().zip(pixels) {
        assert_eq!(layer.pixels, source);
    }
    call(&mut s, "history.redo", json!({}));
    check(&mut s);
}
#[test]
fn corner_modes_tolerance_and_individual_sides_have_explicit_bounds() {
    let mut s = artwork();
    assert_eq!(
        call(
            &mut s,
            "canvas.trim_bounds",
            json!({"left":false,"bottom":false})
        ),
        json!({"x":0,"y":1,"width":6,"height":5,"source_width":8,"source_height":6,"empty":false,"would_change":true,"revision":s.revision,"drawing":false})
    );
    call(&mut s, "canvas.trim", json!({"left":false,"bottom":false}));
    assert_eq!((s.doc.width, s.doc.height), (6, 5));
    call(&mut s, "canvas.new", json!({"width":5,"height":1}));
    s.doc.replace_region(
        0,
        0,
        5,
        1,
        &[0xff102030, 0xff112031, 0xff909090, 0xff112031, 0xff102030],
    );
    let b = call(
        &mut s,
        "canvas.trim_bounds",
        json!({"mode":"top_left","tolerance":1}),
    );
    assert_eq!((b["x"].as_u64(), b["width"].as_u64()), (Some(2), Some(1)));
    call(
        &mut s,
        "canvas.trim",
        json!({"mode":"bottom_right","tolerance":1}),
    );
    assert_eq!(s.doc.layers[0].pixels, [0xff909090]);
}
#[test]
fn all_matching_or_full_bleed_results_are_safe_noops_and_invalid_args_reject() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    let b = call(&mut s, "canvas.trim_bounds", json!({}));
    assert_eq!(b["empty"], true);
    assert_eq!(b["would_change"], false);
    let history = s.doc.history_bytes();
    call(&mut s, "canvas.trim", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (3, 2));
    assert_eq!(s.doc.history_bytes(), history);
    call(&mut s, "layer.fill_color", json!({"color":"#ffffff"}));
    assert_eq!(
        call(&mut s, "canvas.trim_bounds", json!({}))["would_change"],
        false
    );
    assert_eq!(
        call(&mut s, "canvas.trim_bounds", json!({"mode":"top_left"}))["empty"],
        true
    );
    let state = s.state();
    let history = s.doc.history_bytes();
    for args in [
        json!({"mode":"invalid"}),
        json!({"tolerance":256}),
        json!({"tolerance":0.5}),
        json!({"left":false,"right":false,"top":false,"bottom":false}),
        json!({"width":1}),
    ] {
        assert!(s.apply(Action::new("canvas.trim", args)).is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let mut stale = Action::new("canvas.trim", json!({}));
    stale.expected_revision = Some(s.revision + 1);
    assert!(s.apply(stale).is_err());
    assert_eq!(s.state(), state);
}
#[test]
fn readonly_bounds_do_not_finish_a_provisional_typed_contact() {
    let mut s = session();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":4,"y":4,"pen":true,"pressure":0.7}),
    );
    let revision = s.revision;
    let samples = s.replay_samples.len();
    let entries = s.journal.as_ref().unwrap().entries.len();
    assert!(s.is_drawing());
    let b = call(&mut s, "canvas.trim_bounds", json!({}));
    assert_eq!(b["drawing"], true);
    assert!(s.is_drawing());
    assert_eq!(s.revision, revision);
    assert_eq!(s.replay_samples.len(), samples);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), entries);
    call(&mut s, "canvas.pointer", json!({"phase":"up","x":4,"y":4}));
    check(&mut s);
}
#[test]
fn trim_workflow_is_inert_until_run_and_roundtrips_project_and_outcomes() {
    let mut s = artwork();
    let original = s.doc.layers[0].pixels.clone();
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.canvas.trim_bounds"));
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.trim","position":[0.,0.],"parameters":{}}],"edges":[]}}),
    );
    assert_eq!(s.doc.layers[0].pixels, original);
    assert_eq!((s.doc.width, s.doc.height), (8, 6));
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (4, 4));
    let path = std::env::temp_dir().join(format!("micopi-trim-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    assert_eq!((reopened.doc.width, reopened.doc.height), (4, 4));
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn trim_atomic_and_partial_journal_failures_restore_source_and_history() {
    let mut s = artwork();
    s.replay_boundary().unwrap();
    let state = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"trim","action":"canvas.trim"},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s.apply(Action::new("canvas.trim", json!({}))).is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

#[test]
fn trim_samples_free_graph_rendering_without_baking_or_rewriting_nodes() {
    let mut s = artwork();
    let mut graph = s.doc.graph.clone();
    graph.linked = false;
    call(&mut s, "graph.set", json!({"graph":graph}));
    let composition = s.doc.composition();
    let b = call(&mut s, "canvas.trim_bounds", json!({}));
    assert_eq!(
        (
            b["x"].as_u64(),
            b["y"].as_u64(),
            b["width"].as_u64(),
            b["height"].as_u64()
        ),
        (Some(2), Some(1), Some(4), Some(4))
    );
    call(&mut s, "canvas.trim", json!({}));
    assert_eq!(s.doc.composition().graph, composition.graph);
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    check(&mut s);
}
