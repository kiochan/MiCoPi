//! Native tonal edits preserve shared selection, locking, history and replay.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#20406080"}));
    s
}
#[test]
fn tonal_dialog_is_desktop_only_normalizes_parameters_and_rejects_atomic_groups() {
    let mut s = fixture();
    assert_eq!(
        s.apply(Action::new("layer.brightness_contrast.open", json!({})))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    assert!(s.ui_effects.is_empty());
    s.runtime = "desktop";
    let state = s.state();
    let result = call(
        &mut s,
        "layer.brightness_contrast.open",
        json!({"parameters":{"brightness":25}}),
    );
    assert_eq!(result["queued"], true);
    assert_eq!(result["revision"], s.revision);
    let effect = s.ui_effects.pop_front().unwrap();
    assert_eq!(effect["id"], "tonal.open");
    assert_eq!(effect["value"], json!({"brightness":25.,"contrast":0.}));
    assert_eq!(s.state(), state);
    assert_eq!(s.apply(Action::new("pipeline.run", json!({"atomic":true,"steps":[{"id":"open","action":"layer.brightness_contrast.open","args":{}}]}))).unwrap_err().code, "non_atomic_action");
    assert!(s.ui_effects.is_empty());
}
#[test]
fn tonal_preview_is_fenced_read_only_and_matches_selected_masked_apply() {
    use crate::plugins::tonal_preview::{self, Parameters, Request};
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
        layer_id: s.doc.layer_id(s.doc.active),
        parameters: Parameters {
            brightness: 25.,
            contrast: 100.,
        },
    };
    let state = s.state();
    let raw = s.doc.layers[0].pixels.clone();
    let composite = s.doc.composite.clone();
    let events = s.journal.as_ref().unwrap().entries.len();
    let history = s.doc.history_bytes();
    let (preview, cost) = tonal_preview::render(&s, request).unwrap();
    assert_eq!(cost.pixel_scratch_bytes, 96 * 4 * 2);
    let result = call(&mut s, "layer.brightness_contrast.preview", json!(request));
    let bytes = crate::application::png_io::encode(
        12,
        8,
        preview.iter().copied(),
        s.doc.info().export_density(),
    )
    .unwrap();
    assert_eq!(result["data"], codec::base64(&bytes));
    assert_eq!(result["parameters"], json!(request.parameters));
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    let action: Action = serde_json::from_value(result["apply"].clone()).unwrap();
    assert_eq!(action.expected_revision, Some(request.document_revision));
    s.apply(action).unwrap();
    assert_eq!(s.doc.composite, preview);
    assert_eq!(s.revision, request.document_revision + 1);
    assert!(
        s.apply(Action::new(
            "layer.brightness_contrast.preview",
            json!(request)
        ))
        .unwrap_err()
        .code
            == "revision_conflict"
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    check(&mut s);
}
#[test]
fn tonal_preview_uses_registered_access_and_reports_missing_renderer_before_staging() {
    use crate::plugins::{
        tonal_preview::{self, Parameters, Request},
        PixelAccess,
    };
    let mut s = fixture();
    call(&mut s, "layer.set", json!({"position_locked":true}));
    let request = Request {
        document_identity: s.doc.identity,
        document_revision: s.revision,
        layer_id: 1,
        parameters: Parameters {
            brightness: 25.,
            contrast: 100.,
        },
    };
    let pixels = s.doc.layers[0].pixels.clone();
    for (access, code) in [
        (PixelAccess::Geometry, "position_locked"),
        (PixelAccess::None, "pixel_access"),
    ] {
        s.registry
            .actions
            .get_mut("layer.brightness_contrast")
            .unwrap()
            .pixel_access = Some(access);
        assert_eq!(tonal_preview::render(&s, request).unwrap_err().code, code);
    }
    s.registry
        .actions
        .get_mut("layer.brightness_contrast")
        .unwrap()
        .pixel_access = Some(PixelAccess::Pixels);
    assert!(tonal_preview::render(&s, request).is_ok());
    s.doc.renderer = None;
    assert_eq!(
        tonal_preview::render(&s, request).unwrap_err().code,
        "renderer_missing"
    );
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.revision, request.document_revision);
}
#[test]
fn brightness_contrast_selection_locks_atomic_rollback_and_single_undo_agree() {
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
    let result = call(
        &mut s,
        "layer.brightness_contrast",
        json!({"brightness":25,"contrast":100}),
    );
    assert_eq!(result["implementation"], "independent Rust DLL");
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if (2..5).contains(&x) && (1..3).contains(&y) {
                    0x800282ff
                } else {
                    0x80204060
                }
            );
        }
    }
    let after = s.doc.layers[0].pixels.clone();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, after);
    let state = s.state();
    let history = s.doc.history_bytes();
    let failed = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"edit","action":"layer.brightness_contrast","args":{"brightness":-50}},
        {"id":"reject","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["rolled_back"], true);
    assert_eq!(failed["completed"], json!(["edit"]));
    assert_eq!(failed["error"]["code"], "invalid_layer");
    assert_eq!(failed["committed"], json!([]));
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, after);
    check(&mut s);
}
#[test]
fn brightness_contrast_matches_existing_ordered_source_filters_without_changing_them() {
    let mut direct = fixture();
    call(
        &mut direct,
        "layer.brightness_contrast",
        json!({"brightness":25,"contrast":100}),
    );
    let mut source = fixture();
    let raw = source.doc.layers[0].pixels.clone();
    call(
        &mut source,
        "graph.set",
        json!({"graph":{"enabled":true,"linked":true,"nodes":[
        {"id":1,"kind":"layer","index":0,"position":[0,0]},
        {"id":2,"kind":"filter","filter":"brightness","amount":0.25,"position":[200,0]},
        {"id":3,"kind":"filter","filter":"contrast","amount":1,"position":[400,0]},
        {"id":4,"kind":"output","position":[600,0]}],"edges":[
        {"from":1,"to":2,"input":0},{"from":2,"to":3,"input":0},{"from":3,"to":4,"input":0}]}}),
    );
    assert_eq!(direct.doc.layers[0].pixels, vec![0x800282ff; 96]);
    assert_eq!(source.doc.composite, direct.doc.composite);
    assert_eq!(source.doc.layers[0].pixels, raw);
    check(&mut direct);
    check(&mut source);
}
#[test]
fn brightness_contrast_invalid_locked_mask_and_contact_targets_reject_without_dispatch() {
    let mut s = fixture();
    for args in [
        json!({"brightness":101}),
        json!({"contrast":-101}),
        json!({"brightness":null}),
        json!({"contrast":"20"}),
    ] {
        let state = s.state();
        assert_eq!(
            s.apply(Action::new("layer.brightness_contrast", args))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(s.state(), state);
    }
    call(&mut s, "layer.set", json!({"pixel_locked":true}));
    assert_eq!(
        s.apply(Action::new(
            "layer.brightness_contrast",
            json!({"brightness":20})
        ))
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
        s.apply(Action::new(
            "layer.brightness_contrast",
            json!({"brightness":20})
        ))
        .unwrap_err()
        .code,
        "mask_target_pending"
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    call(
        &mut s,
        "layer.brightness_contrast",
        json!({"brightness":25,"contrast":100}),
    );
    assert!((0..96).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 255));
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 1.5,
        pressure: 0.4,
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
        s.apply(Action::new(
            "layer.brightness_contrast",
            json!({"contrast":30})
        ))
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
            "layer.brightness_contrast.preview",
            json!({
        "document_identity":s.doc.identity,"document_revision":s.revision,"layer_id":1,
        "parameters":{"brightness":25,"contrast":100}})
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
