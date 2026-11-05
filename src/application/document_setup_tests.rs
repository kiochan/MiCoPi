//! Document setup shares persistent metadata, atomic history and verified outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::document_info::Info;
use serde_json::json;
#[test]
fn document_setup_initializes_real_background_and_legacy_defaults_are_omitted() {
    let mut s = session();
    assert!(serde_json::to_value(s.doc.composition())
        .unwrap()
        .get("document_info")
        .is_none());
    call(
        &mut s,
        "canvas.new",
        json!({"width":300,"height":150,"resolution_ppi":300,"background":"#AbC012"}),
    );
    assert!(s.doc.layers[0].pixels.iter().all(|&p| p == 0xffabc012));
    assert_eq!(s.doc.history_bytes(), 0);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), 2);
    let state = call(&mut s, "canvas.info", json!({}));
    assert_eq!(state["resolution_ppi"], 300);
    assert_eq!(state["initial_background"], "#ABC012");
    assert!((state["width_mm"].as_f64().unwrap() - 25.4).abs() < 1e-10);
    assert!((state["height_mm"].as_f64().unwrap() - 12.7).abs() < 1e-10);
    let before = s.state();
    call(&mut s, "canvas.info", json!({}));
    assert_eq!(s.state(), before);
    check(&mut s);
    call(&mut s, "canvas.new", json!({"width":12,"height":8}));
    assert_eq!(s.doc.info(), Info::default());
    assert!(s.doc.layers[0].pixels.iter().all(|&p| p == 0));
    assert!(serde_json::to_value(s.doc.composition())
        .unwrap()
        .get("document_info")
        .is_none());
    check(&mut s);
}
#[test]
fn document_setup_resolution_undo_project_and_workflow_preserve_pixels() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let old = s.doc.layers[0].pixels.clone();
    let old_info = s.doc.info();
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("state.core.document_info"));
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[{"id":1,"type":"action.canvas.resolution","position":[0,0],"parameters":{"resolution_ppi":600}}],"edges":[]}}),
    );
    assert_eq!(s.doc.info(), old_info);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.info().resolution_ppi, 600);
    assert_eq!(s.doc.layers[0].pixels, old);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.info(), old_info);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.info().resolution_ppi, 600);
    check(&mut s);
    let path =
        std::env::temp_dir().join(format!("micopi-document-setup-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.info(), s.doc.info());
    assert_eq!(reopened.doc.layers[0].pixels, old);
    check(&mut reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn document_setup_invalid_args_atomic_and_append_failures_leave_state_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let before = s.state();
    let history = s.doc.history_bytes();
    let pixels = s.doc.layers[0].pixels.clone();
    for args in [
        json!({"width":12,"height":8,"background":"#FFFFFFFF"}),
        json!({"width":12,"height":8,"background":"#ééé"}),
        json!({"width":12,"height":8,"resolution_ppi":0}),
        json!({"width":12,"height":8,"resolution_ppi":2401}),
    ] {
        assert!(s.apply(Action::new("canvas.new", args)).is_err());
        assert_eq!(s.state(), before);
    }
    for ppi in [0, 2401] {
        assert!(s
            .apply(Action::new(
                "canvas.resolution",
                json!({"resolution_ppi":ppi})
            ))
            .is_err());
        assert_eq!(s.state(), before);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"resolution","action":"canvas.resolution","args":{"resolution_ppi":300}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    for (action, args) in [
        ("canvas.resolution", json!({"resolution_ppi":300})),
        (
            "canvas.new",
            json!({"width":16,"height":9,"resolution_ppi":300,"background":"#FFFFFF"}),
        ),
    ] {
        s.replay_error = None;
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        assert!(s.apply(Action::new(action, args)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
    }
    for value in [
        json!({"resolution_ppi":0}),
        json!({"resolution_ppi":1.5}),
        json!({"initial_background":"bad"}),
        json!({"extra":true}),
    ] {
        assert!(serde_json::from_value::<Info>(value).is_err());
    }
}
#[test]
fn document_setup_active_contact_is_preserved_on_rejected_changes() {
    let mut s = session();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.5,"pen":true,"time":0}),
    );
    let before = s.state();
    let history = s.doc.history_bytes();
    call(&mut s, "canvas.info", json!({}));
    for (id, args) in [
        ("canvas.new", json!({"width":10,"height":10})),
        ("canvas.resolution", json!({"resolution_ppi":300})),
    ] {
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "stroke_active"
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":3,"y":3,"pressure":0.5,"pen":true,"time":1}),
    );
    check(&mut s);
}
