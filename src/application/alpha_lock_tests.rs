use super::editing_tests::{call, check, session};
use super::{Action, Session};
use serde_json::json;
fn shape(s: &mut Session) -> Vec<u32> {
    call(
        s,
        "brush.set",
        json!({"radius":2,"opacity":0.5,"color":"#112233","hardness":1,"stabilizer":0,"pressure_size":false}),
    );
    call(s, "canvas.stroke", json!({"points":[{"x":4.5,"y":4.5}]}));
    s.doc.layers[0].pixels.clone()
}
fn same_coverage(before: &[u32], after: &[u32]) {
    for (a, b) in before.iter().zip(after) {
        assert_eq!(a >> 24, b >> 24);
        if a >> 24 == 0 {
            assert_eq!(a, b);
        }
    }
}
#[test]
fn alpha_lock_recolors_with_brush_fill_gradient_and_roundtrips_projects() {
    let mut s = session();
    let original = shape(&mut s);
    assert!(
        serde_json::to_value(s.doc.composition()).unwrap()["layer_metadata"][0]
            .get("alpha_locked")
            .is_none()
    );
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"opacity":0.5}),
    );
    call(&mut s, "history.undo", json!({}));
    assert!(!s.doc.alpha_locked(0));
    assert_eq!(s.doc.layers[0].opacity, 255);
    call(&mut s, "history.redo", json!({}));
    assert!(s.doc.alpha_locked(0));
    assert_eq!(s.doc.layers[0].opacity, 128);
    call(
        &mut s,
        "brush.set",
        json!({"radius":4,"opacity":0.5,"color":"#00ff00"}),
    );
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":4.5,"y":4.5}]}),
    );
    assert_eq!(s.doc.layers[0].pixels[4 * 12 + 4], 0x80089119);
    same_coverage(&original, &s.doc.layers[0].pixels);
    check(&mut s);
    let painted = s.doc.layers[0].pixels.clone();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, painted);
    for (id, args) in [
        ("layer.fill_color", json!({"color":"#4455ff"})),
        (
            "layer.gradient",
            json!({"x0":0,"y0":0,"x1":11,"y1":7,"start_color":"#ff0000","end_color":"#0000ff00","spread":"reflect"}),
        ),
        (
            "layer.fill",
            json!({"x":4,"y":4,"color":"#aa00ee","tolerance":255,"contiguous":false,"expand":1,"feather":2}),
        ),
    ] {
        call(&mut s, id, args);
        same_coverage(&original, &s.doc.layers[0].pixels);
        check(&mut s);
    }
    let pixels = s.doc.layers[0].pixels.clone();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/alpha-lock-{}.mcp",
        std::process::id()
    ));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert!(reopened.doc.alpha_locked(0));
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    check(&mut reopened);
    call(&mut reopened, "layer.set", json!({"alpha_locked":false}));
    call(
        &mut reopened,
        "brush.set",
        json!({"radius":1,"opacity":1,"color":"#ff0000"}),
    );
    call(
        &mut reopened,
        "canvas.stroke",
        json!({"points":[{"x":9.5,"y":4.5}]}),
    );
    assert_eq!(reopened.doc.layers[0].pixels[4 * 12 + 9], 0xffff0000);
    check(&mut reopened);
}
#[test]
fn alpha_lock_rejects_coverage_changes_and_atomic_failures_restore_lock_state() {
    let mut s = session();
    shape(&mut s);
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    for (id, args) in [
        ("layer.clear", json!({})),
        ("native.checkerboard", json!({"cell":2})),
        ("layer.translate", json!({"dx":1,"dy":0})),
    ] {
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    call(&mut s, "tool.select", json!({"tool":"eraser"}));
    let before = s.state();
    assert!(s
        .apply(Action::new(
            "canvas.stroke",
            json!({"points":[{"x":4,"y":4}]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    call(&mut s, "tool.select", json!({"tool":"brush"}));
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"color","action":"layer.fill_color","args":{"color":"#00ff00"}},
            {"id":"unlock","action":"layer.set","args":{"alpha_locked":false}},
            {"id":"fail","action":"layer.fill_color","args":{"color":"#bad"}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    check(&mut s);
}
#[test]
fn lock_metadata_follows_identity_through_duplicate_and_reorder() {
    let mut s = session();
    let id = s.doc.layer_id(0);
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    call(&mut s, "layer.duplicate", json!({}));
    assert!(s.doc.alpha_locked(1));
    call(&mut s, "layer.add", json!({}));
    let unlocked = s.doc.layer_id(2);
    assert!(!s.doc.alpha_locked(2));
    call(&mut s, "layer.move", json!({"layer_id":id,"to":2}));
    assert!(s.doc.alpha_locked(s.doc.layer_index(id).unwrap()));
    assert!(!s.doc.alpha_locked(s.doc.layer_index(unlocked).unwrap()));
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.alpha_locked(0));
    assert!(!s.doc.alpha_locked(2));
    check(&mut s);
}

#[test]
fn locked_transparent_strokes_do_not_allocate_undo_tiles() {
    let mut s = session();
    call(&mut s, "layer.set", json!({"alpha_locked":true}));
    let bytes = s.doc.history_bytes();
    let before = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":4,"y":4},{"x":8,"y":4}]}),
    );
    assert_eq!(s.doc.history_bytes(), bytes);
    assert_eq!(s.doc.layers[0].pixels, before);
    check(&mut s);
}
