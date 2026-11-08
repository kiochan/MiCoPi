//! Weighted grayscale shares native selection, locking, history and outcomes.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
    s
}
#[test]
fn grayscale_preview_is_immutable_fenced_and_matches_soft_selection_masked_apply() {
    use crate::plugins::grayscale_preview::{self, Parameters, Request};
    let mut s = fixture();
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":127}));
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"position_locked":true,"alpha_locked":true}),
    );
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":2,"y":1,"width":6,"height":5}),
    );
    call(&mut s, "selection.feather", json!({"radius":1}));
    let request = Request {
        document_identity: s.doc.identity,
        document_revision: s.revision,
        layer_id: 1,
        parameters: Parameters { amount: 50. },
    };
    let state = s.state();
    let raw = s.doc.layers[0].pixels.clone();
    let composite = s.doc.composite.clone();
    let events = s.journal.as_ref().unwrap().entries.len();
    let history = s.doc.history_bytes();
    let (preview, cost) = grayscale_preview::render(&s, request).unwrap();
    assert_eq!(cost.pixel_scratch_bytes, 96 * 4 * 2);
    let result = call(&mut s, "layer.grayscale.preview", json!(request));
    let bytes = crate::application::png_io::encode(
        12,
        8,
        preview.iter().copied(),
        s.doc.info().export_density(),
    )
    .unwrap();
    assert_eq!(result["data"], codec::base64(&bytes));
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    let action: Action = serde_json::from_value(result["apply"].clone()).unwrap();
    assert_eq!(action.action, "layer.grayscale");
    assert_eq!(action.expected_revision, Some(request.document_revision));
    s.apply(action).unwrap();
    assert_eq!(s.doc.composite, preview);
    assert_eq!(s.revision, request.document_revision + 1);
    assert_eq!(
        s.apply(Action::new("layer.grayscale.preview", json!(request)))
            .unwrap_err()
            .code,
        "revision_conflict"
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    check(&mut s);
}
#[test]
fn grayscale_dialog_has_full_default_preserves_revision_and_rejects_headless_atomic_use() {
    let mut s = fixture();
    assert_eq!(
        s.apply(Action::new("layer.grayscale.open", json!({})))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    assert!(s.ui_effects.is_empty());
    s.runtime = "desktop";
    let state = s.state();
    call(&mut s, "layer.grayscale.open", json!({}));
    assert_eq!(s.state(), state);
    let effect = s.ui_effects.pop_front().unwrap();
    assert_eq!(effect["id"], "grayscale.open");
    assert_eq!(effect["value"], json!({"amount":100.}));
    assert_eq!(
        s.apply(Action::new(
            "pipeline.run",
            json!({"atomic":true,"steps":[{"id":"open","action":"layer.grayscale.open","args":{}}]})
        ))
        .unwrap_err()
        .code,
        "non_atomic_action"
    );
    assert!(s.ui_effects.is_empty());
}
#[test]
fn grayscale_selection_alpha_locks_one_undo_and_post_edit_rollback_agree() {
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
    let raw = s.doc.layers[0].pixels.clone();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    call(&mut s, "layer.grayscale", json!({}));
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if (2..5).contains(&x) && (1..3).contains(&y) {
                    0x80363636
                } else {
                    0x80ff0000
                }
            );
        }
    }
    let gray = s.doc.layers[0].pixels.clone();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, raw);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, gray);
    call(&mut s, "history.undo", json!({}));
    let state = s.state();
    let history = s.doc.history_bytes();
    let failed = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"edit","action":"layer.grayscale","args":{}},
        {"id":"reject","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["rolled_back"], true);
    assert_eq!(failed["completed"], json!(["edit"]));
    assert_eq!(failed["error"]["code"], "invalid_layer");
    assert_eq!(failed["committed"], json!([]));
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}
#[test]
fn grayscale_strength_matches_existing_source_filter_and_independent_red_witnesses() {
    for (amount, expected) in [(0., 0x80ff0000), (50., 0x809a1b1b), (100., 0x80363636)] {
        let mut direct = fixture();
        call(&mut direct, "layer.grayscale", json!({"amount":amount}));
        assert_eq!(direct.doc.layers[0].pixels, vec![expected; 96]);
        let mut source = fixture();
        call(
            &mut source,
            "graph.set",
            json!({"graph":{"enabled":true,"linked":true,"nodes":[
            {"id":1,"kind":"layer","index":0,"position":[0,0]},
            {"id":2,"kind":"filter","filter":"grayscale","amount":amount/100.,"position":[200,0]},
            {"id":3,"kind":"output","position":[400,0]}],"edges":[
            {"from":1,"to":2,"input":0},{"from":2,"to":3,"input":0}]}}),
        );
        assert_eq!(source.doc.composite, direct.doc.composite);
        assert_eq!(source.doc.layers[0].pixels, vec![0x80ff0000; 96]);
        check(&mut direct);
        check(&mut source);
    }
}
#[test]
fn grayscale_invalid_locks_mask_target_and_contact_reject_before_dispatch() {
    let mut s = fixture();
    for args in [
        json!({"amount":-1}),
        json!({"amount":101}),
        json!({"amount":null}),
        json!({"amount":"50"}),
    ] {
        let state = s.state();
        assert_eq!(
            s.apply(Action::new("layer.grayscale", args))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(s.state(), state);
    }
    call(&mut s, "layer.set", json!({"pixel_locked":true}));
    assert_eq!(
        s.apply(Action::new("layer.grayscale", json!({})))
            .unwrap_err()
            .code,
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
        s.apply(Action::new("layer.grayscale", json!({})))
            .unwrap_err()
            .code,
        "mask_target_pending"
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    call(&mut s, "layer.grayscale", json!({}));
    assert!((0..96).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 255));
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
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    let instances = s.native_instances.len();
    assert_eq!(
        s.apply(Action::new("layer.grayscale", json!({})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.native_instances.len(), instances);
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        s.apply(Action::new(
            "layer.grayscale.preview",
            json!({"document_identity":s.doc.identity,
        "document_revision":s.revision,"layer_id":1,"parameters":{"amount":100}})
        ))
        .unwrap_err()
        .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        time: 2,
        x: 4.,
        ..pointer
    })
    .unwrap();
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 3,
        x: 5.,
        ..pointer
    })
    .unwrap();
    check(&mut s);
}
