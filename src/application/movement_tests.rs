//! Movement contracts against staged native kernels, shared history and settings.
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
fn movement_layer_and_selected_content_have_explicit_different_selection_contracts() {
    let mut s = fixture();
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":2,"height":1}),
    );
    let state = s.doc.composition();
    let selection = s.doc.selection().clone();
    let original = s.doc.layers[0].pixels.clone();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.move", json!({"dx":1,"dy":0}));
    assert_eq!(
        s.doc.layers[0].pixels,
        [0, 0xff0000ff, 0xffff0000, 0xff00ff00]
    );
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [0, 255, 255, 0]
    );
    assert_eq!(s.doc.selection().previous, selection.previous);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), state);
    assert_eq!(s.doc.layers[0].pixels, original);
    call(&mut s, "history.redo", json!({}));
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.move",
        json!({"dx":1,"dy":0,"target":"layer"}),
    );
    assert_eq!(
        s.doc.layers[0].pixels,
        [0, 0xff0000ff, 0xffff0000, 0x00112233]
    );
    assert_eq!(s.doc.selection(), &selection);
}
#[test]
fn fractional_selection_moves_clear_and_source_over_once_and_masks_can_move_alone() {
    let mut s = fixture();
    let mask = crate::engine::selection::Mask::new(4, 1, vec![128, 255, 0, 0]).unwrap();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(mask.clone()),
            previous: Some(mask),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    let old = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "canvas.move",
        json!({"dx":1,"dy":0,"target":"selection"}),
    );
    assert_eq!(
        s.doc.layers[0].pixels,
        [0x7f0000ff, 0x800000ff, 0xffff0000, 0xff00ff00]
    );
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [0, 128, 255, 0]
    );
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), old);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true}),
    );
    call(
        &mut s,
        "canvas.move",
        json!({"dx":-1,"dy":0,"target":"selection_mask"}),
    );
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(
        s.doc.selection().current.as_ref().unwrap().coverage(),
        [255, 0, 0, 0]
    );
    assert_eq!(s.doc.selection().previous, old.selection.previous);
    check(&mut s);
}
#[test]
fn movement_guards_identity_noops_and_full_frame_clipping_are_safe() {
    let mut s = fixture();
    let original = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    call(&mut s, "canvas.move", json!({"dx":0,"dy":0}));
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, original);
    for args in [
        json!({"dx":1,"dy":0,"target":"selection"}),
        json!({"dx":4097,"dy":0}),
        json!({"dx":1.5,"dy":0}),
        json!({"dx":1,"dy":0,"target":"bad"}),
    ] {
        assert!(s.apply(Action::new("canvas.move", args)).is_err());
        assert_eq!(s.doc.layers[0].pixels, original);
    }
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    let state = s.state();
    assert!(s
        .apply(Action::new("canvas.move", json!({"dx":1,"dy":0})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, original);
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":false,"visible":false}),
    );
    assert!(s
        .apply(Action::new("canvas.move", json!({"dx":1,"dy":0})))
        .is_err());
    call(&mut s, "layer.set", json!({"visible":true}));
    call(&mut s, "canvas.move", json!({"dx":4096,"dy":-4096}));
    assert_eq!(s.doc.layers[0].pixels, [0; 4]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    check(&mut s);
}
#[test]
fn nudge_uses_validated_preferences_explicit_target_and_compact_context() {
    let mut s = fixture();
    s.apply_movement_defaults(MovementPreferences {
        target: MoveTarget::Layer,
        step: 2,
        large_step: 3,
    });
    call(&mut s, "canvas.nudge", json!({"direction":"right"}));
    assert_eq!(s.doc.layers[0].pixels, [0, 0, 0xff0000ff, 0xffff0000]);
    let journal = s.replay_journal(false).unwrap();
    let event = journal.events(journal.entries.len() - 1, 1).unwrap();
    assert!(event.to_string().contains("movement"));
    assert!(event.to_string().contains(r#""dx":2"#));
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.nudge",
        json!({"direction":"left","large":true}),
    );
    assert_eq!(s.doc.layers[0].pixels, [0xff00ff00, 0, 0, 0]);
    check(&mut s);
}
#[test]
fn movement_atomic_and_partial_journal_failures_restore_both_pixels_and_masks() {
    let mut s = fixture();
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":2,"height":1}),
    );
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let state = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"move","action":"canvas.move","args":{"dx":1,"dy":0}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.move", json!({"dx":1,"dy":0})))
        .is_err());
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
fn movement_nodes_edit_inertly_and_projects_reopen_verified_results() {
    let mut s = fixture();
    let pixels = s.doc.layers[0].pixels.clone();
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.canvas.move"));
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.move","position":[0.,0.],"parameters":{"dx":1,"dy":0}}],"edges":[]}}),
    );
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    let path = std::env::temp_dir().join(format!("micopi-movement-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    assert_eq!(reopened.doc.composition(), composition);
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn movement_settings_validate_and_preserve_unknown_fields_and_inactive_profiles() {
    let path = std::env::temp_dir().join(format!(
        "micopi-movement-settings-{}.json",
        std::process::id()
    ));
    let mut s = session();
    s.settings_path = Some(path.clone());
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"document":{"custom":{"keep":42},"movement":{"target":"selection_mask","step":2,"large_step":20}}}),
    );
    assert_eq!(s.movement_defaults.step, 2);
    assert_eq!(s.state()["movement_defaults"]["target"], "selection_mask");
    let saved = std::fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&saved).contains("keep"));
    for patch in [
        json!({"step":0}),
        json!({"large_step":4097}),
        json!({"target":"invalid"}),
    ] {
        assert!(s
            .apply(Action::new(
                "settings.update",
                json!({"path":path,"movement":patch})
            ))
            .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), saved);
    }
    assert_eq!(
        s.apply(Action::new("toolbox.mode.move", json!({})))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    s.runtime = "desktop";
    call(
        &mut s,
        "toolbox.movement.options",
        json!({"target":"layer","step":3,"large_step":30}),
    );
    assert_eq!(s.movement_options().step, 3);
    let other = path.with_file_name(format!(
        "micopi-movement-inactive-{}.json",
        std::process::id()
    ));
    call(
        &mut s,
        "settings.update",
        json!({"path":other,"movement":{"step":11}}),
    );
    assert_eq!(s.movement_options().step, 3);
    assert_eq!(s.movement_defaults.step, 2);
    std::fs::remove_file(other).unwrap();
    assert!(s
        .apply(Action::new(
            "ui.dispatch",
            json!({"id":"movement.options","value":{"step":0}})
        ))
        .is_err());
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"ui":{"accent":"#778899"}}),
    );
    assert_eq!(s.movement_options().step, 3);
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"movement":{"step":4}}),
    );
    assert_eq!(s.movement_options().step, 4);
    assert_eq!(s.movement_options().target, MoveTarget::SelectionMask);
    std::fs::remove_file(path).unwrap();
}
