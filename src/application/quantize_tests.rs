//! Direct quantization contracts across actual DLLs, undo and outcome replay.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::{json, Value};

fn cases() -> [(&'static str, Value, u32); 2] {
    [
        ("layer.threshold", json!({"level":60}), 0x80000000),
        ("layer.posterize", json!({"levels":4}), 0x80005555),
    ]
}
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#20406080"}));
    s
}
#[test]
fn selected_quantization_preserves_alpha_locks_and_one_undo_revision_outcome() {
    for (action, args, expected) in cases() {
        let mut s = fixture();
        call(
            &mut s,
            "selection.shape",
            json!({"kind":"rectangle","x":2,"y":1,"width":3,"height":2}),
        );
        call(
            &mut s,
            "layer.set",
            json!({"position_locked":true,"alpha_locked":true}),
        );
        let before = s.doc.layers[0].pixels.clone();
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        call(&mut s, action, args);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        for y in 0..8 {
            for x in 0..12 {
                assert_eq!(
                    s.doc.layers[0].pixels[y * 12 + x],
                    if (2..5).contains(&x) && (1..3).contains(&y) {
                        expected
                    } else {
                        0x80204060
                    }
                );
            }
        }
        let edited = s.doc.layers[0].pixels.clone();
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, before);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, edited);
        check(&mut s);
    }
}
#[test]
fn quantization_runtime_failure_rolls_back_completed_edit_and_workflows_share_defaults() {
    for (action, args, expected) in cases() {
        let mut s = fixture();
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        let failed = call(
            &mut s,
            "pipeline.run",
            json!({"atomic":true,"steps":[
            {"id":"edit","action":action,"args":args},
            {"id":"reject","action":"layer.select","args":{"layer_id":99999}}]}),
        );
        assert_eq!(failed["ok"], false);
        assert_eq!(failed["completed"], json!(["edit"]));
        assert_eq!(failed["rolled_back"], true);
        assert_eq!(failed["committed"], json!([]));
        assert_eq!(failed["error"]["code"], "invalid_layer");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        call(
            &mut s,
            "workflow.set",
            json!({"graph":{"version":1,"nodes":[{
            "id":1,"type":format!("action.{action}"),"position":[0,0],"parameters":{}}],"edges":[]}}),
        );
        assert_eq!(call(&mut s, "workflow.validate", json!({}))["ok"], true);
        assert_eq!(
            call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
            true
        );
        assert_eq!(s.doc.layers[0].pixels, vec![expected; 96]);
        check(&mut s);
    }
}
#[test]
fn quantization_rejects_invalid_parameters_pixel_locks_and_mask_targets() {
    for (action, _, _) in cases() {
        let mut s = fixture();
        let key = if action == "layer.threshold" {
            "level"
        } else {
            "levels"
        };
        for args in [
            json!({(key):-1}),
            json!({(key):257}),
            json!({(key):1.5}),
            json!({(key):null}),
            json!({"unknown":3}),
        ] {
            let before = s.state();
            assert_eq!(
                s.apply(Action::new(action, args)).unwrap_err().code,
                "invalid_arguments"
            );
            assert_eq!(s.state(), before);
        }
        call(&mut s, "layer.set", json!({"pixel_locked":true}));
        assert_eq!(
            s.apply(Action::new(action, json!({}))).unwrap_err().code,
            "pixel_locked"
        );
        call(&mut s, "layer.set", json!({"pixel_locked":false}));
        let mask = call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":"layer","id":1}}),
        )["mask_id"]
            .as_u64()
            .unwrap();
        assert_eq!(
            s.apply(Action::new(action, json!({}))).unwrap_err().code,
            "mask_target_pending"
        );
        call(
            &mut s,
            "mask.target",
            json!({"target":{"kind":"layer_pixels"}}),
        );
        call(&mut s, action, json!({}));
        assert!((0..96).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 255));
        check(&mut s);
    }
}
#[test]
fn quantization_rejection_preserves_pressure_contact_and_native_instances() {
    for (action, args, _) in cases() {
        let mut s = fixture();
        let down = Pointer {
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
        s.pointer(down).unwrap();
        let revision = s.revision;
        let pixels = s.doc.layers[0].pixels.clone();
        let instances = s.native_instances.len();
        assert_eq!(
            s.apply(Action::new(action, args)).unwrap_err().code,
            "stroke_active"
        );
        assert!(s.stroke.active());
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.native_instances.len(), instances);
        s.pointer(Pointer {
            phase: PointerPhase::Move,
            time: 2,
            x: 4.,
            ..down
        })
        .unwrap();
        s.pointer(Pointer {
            phase: PointerPhase::Up,
            time: 3,
            x: 5.,
            ..down
        })
        .unwrap();
        check(&mut s);
    }
}
