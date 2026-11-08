//! Mirror is presentation state, independent of document pixels and undo.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::json;
#[test]
fn view_mirror_is_desktop_only_reversible_and_preserves_document_bytes_and_history() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#20406080"}));
    let view = s.view;
    for (action, args) in [
        ("view.mirror_horizontal", json!({})),
        ("view.set", json!({"mirror_x":true,"zoom":2})),
    ] {
        assert_eq!(
            s.apply(Action::new(action, args)).unwrap_err().code,
            "ui_unavailable"
        );
        assert_eq!(s.view, view);
    }
    s.runtime = "desktop";
    let raw = s.doc.layers[0].pixels.clone();
    let png = codec::png(&mut s.doc).unwrap();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let result = call(&mut s, "view.mirror_horizontal", json!({}));
    crate::plugins::validate(
        s.registry.actions["view.mirror_horizontal"]
            .output_schema
            .as_ref()
            .unwrap(),
        &result,
    )
    .unwrap();
    assert_eq!(result["mirror_x"], true);
    assert!(s.view.mirror_x);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
    let result = call(&mut s, "view.mirror_horizontal", json!({}));
    assert_eq!(result["mirror_x"], false);
    assert_eq!(s.view, view);
    assert_eq!(s.revision, revision + 2);
    assert_eq!(s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[{"id":"mirror","action":"view.mirror_horizontal","args":{}}]}))).unwrap_err().code,"non_atomic_action");
    assert_eq!(s.view, view);
    check(&mut s);
}
#[test]
fn view_mirror_rejects_live_contact_without_partial_view_changes() {
    let mut s = session();
    s.runtime = "desktop";
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 1.5,
        pressure: 0.3,
        time: 1,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(pointer).unwrap();
    let view = s.view;
    let revision = s.revision;
    for (action, args) in [
        ("view.mirror_horizontal", json!({})),
        ("view.set", json!({"mirror_x":true,"zoom":2})),
    ] {
        assert_eq!(
            s.apply(Action::new(action, args)).unwrap_err().code,
            "stroke_active"
        );
        assert_eq!(s.view, view);
        assert_eq!(s.revision, revision);
        assert!(s.stroke.active());
    }
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        x: 4.,
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
fn legacy_view_metadata_keeps_its_serialized_shape() {
    let value = json!({"zoom":1.,"pan_x":0.,"pan_y":0.,"fit":true});
    let view: View = serde_json::from_value(value.clone()).unwrap();
    assert!(!view.mirror_x);
    assert_eq!(serde_json::to_value(view).unwrap(), value);
}
