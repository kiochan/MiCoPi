//! Statistics are bounded observations, not editing/replay events.
use super::{
    editing_tests::{call, session},
    *,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":2}));
    s.doc.layers[0].pixels = vec![0x00112233, 0x010a141e, 0x800a1428, 0xff001428];
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    s
}
#[test]
fn exact_histogram_contract_is_discoverable_and_leaves_document_history_and_events_unchanged() {
    let mut s = fixture();
    let before = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = call(
        &mut s,
        "histogram.query",
        json!({"source":"layer","layer_id":1}),
    );
    assert_eq!(result["sampled_pixels"], 4);
    assert_eq!(result["nontransparent_pixels"], 3);
    assert_eq!(result["ignored_transparent_pixels"], 1);
    assert_eq!(result["exact"], true);
    assert_eq!(result["red"][10], 2);
    assert_eq!(result["red"][0], 1);
    assert_eq!(result["green"][20], 3);
    assert_eq!(result["blue"][40], 2);
    assert_eq!(
        result["rgb"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .sum::<u64>(),
        9
    );
    assert_eq!(result["revision"], s.revision);
    assert_eq!(result["document_identity"], s.doc.identity);
    crate::plugins::validate(
        s.registry.actions["histogram.query"]
            .output_schema
            .as_ref()
            .unwrap(),
        &result,
    )
    .unwrap();
    let nodes = s.registry.node_catalog();
    assert_eq!(nodes["action.histogram.query"].domain, "workflow");
    assert!(nodes["action.histogram.query"].read_only);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
}
#[test]
fn raw_and_composite_sources_use_explicit_scope_and_preserve_pixel_locks() {
    let mut s = fixture();
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":1,"visible":false,"pixel_locked":true,"position_locked":true}),
    );
    let raw = call(
        &mut s,
        "histogram.query",
        json!({"source":"layer","layer_id":1.0,"max_samples":4.0}),
    );
    assert_eq!(raw["nontransparent_pixels"], 3);
    let composite = call(&mut s, "histogram.query", json!({}));
    assert_eq!(composite["nontransparent_pixels"], 0);
    assert!(composite.get("layer_id").is_none());
    for args in [
        json!({"source":"layer"}),
        json!({"layer_id":1}),
        json!({"source":"layer","layer_id":99}),
        json!({"max_samples":0}),
        json!({"max_samples":65537}),
        json!({"max_samples":1.5}),
    ] {
        let before = s.state();
        assert!(s.apply(Action::new("histogram.query", args)).is_err());
        assert_eq!(s.state(), before);
    }
}
#[test]
fn sampled_grid_reports_approximation_with_independent_center_bins() {
    let mut s = fixture();
    call(&mut s, "canvas.new", json!({"width":4,"height":4}));
    s.doc.layers[0].pixels = (0..16).map(|v| 0xff000000 | v).collect();
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let r = call(
        &mut s,
        "histogram.query",
        json!({"source":"layer","layer_id":1,"max_samples":4}),
    );
    assert_eq!(r["exact"], false);
    assert_eq!(r["total_pixels"], 16);
    assert_eq!(r["grid_width"], 2);
    assert_eq!(r["grid_height"], 2);
    for i in 0..256 {
        assert_eq!(r["blue"][i], u64::from([5, 7, 13, 15].contains(&i)));
    }
}
#[test]
fn missing_histogram_provider_rejects_without_document_changes() {
    let mut s = fixture();
    s.registry.native_plugins.remove("compositing");
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    assert_eq!(
        s.apply(Action::new("histogram.query", json!({})))
            .unwrap_err()
            .code,
        "histogram_unavailable"
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
}
#[test]
fn histogram_rejects_active_contacts_without_finishing_or_recording_them() {
    let mut s = fixture();
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 0.5,
        y: 0.5,
        pressure: 1.,
        time: 1,
        pen: false,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    let pixels = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        s.apply(Action::new("histogram.query", json!({})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.revision, revision);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 2,
        ..p
    })
    .unwrap();
    call(&mut s, "histogram.query", json!({}));
}
