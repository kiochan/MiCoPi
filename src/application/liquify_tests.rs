//! Shared-owner Liquify drafts, publication and verified outcome regression gates.
use super::editing_tests::{call, check, session};
use super::*;
use crate::{
    engine::{
        raster_masks::SamplePlane,
        selection::{Mask, SelectionState},
    },
    plugins::liquify::{Draft, Push, Request},
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.add", json!({}));
    for (i, layer) in s.doc.layers.iter_mut().enumerate() {
        layer.pixels = (0..96)
            .map(|p| 0xff000000 | ((p * 3791 + i * 21761) as u32 & 0xffffff))
            .collect();
    }
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let id = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    s.doc
        .replace_mask_plane(
            id,
            SamplePlane::Dense((0..96).map(|i| (i * 37) as u8).collect()),
        )
        .unwrap();
    call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":2},"select":false}),
    );
    call(&mut s, "mask.set", json!({"mask_id":2,"linked":false}));
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(12, 8, vec![0; 96]).unwrap()),
            previous: Some(Mask::new(12, 8, vec![79; 96]).unwrap()),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    s
}
fn request(s: &Session) -> Request {
    serde_json::from_value(json!({"document_identity":s.doc.identity,"document_revision":s.revision,"layer_ids":[2,1],"spacing":1,"method":"bilinear","steps":[[{"x":5.,"y":4.,"dx":1.2,"dy":0.4,"radius":4.,"strength":0.8}],[{"x":6.,"y":4.,"dx":-0.5,"dy":0.8,"radius":4.,"strength":0.7}]]})).unwrap()
}
fn raw(s: &Session) -> Value {
    json!({"layers":s.doc.layers.iter().map(|l|l.pixels.clone()).collect::<Vec<_>>(),"masks":s.doc.mask_state().items.iter().map(|m|(m.id,(0..96).map(|i|s.doc.mask_plane(m.id).unwrap().sample(i)).collect::<Vec<_>>())).collect::<Vec<_>>(),"selection":s.doc.selection(),"recipe":s.doc.last_transform(),"active":s.doc.active})
}

#[test]
fn multi_owner_liquify_preview_apply_one_undo_redo_and_outcome_restore_exact_raw_owners() {
    let mut s = fixture();
    let r = request(&s);
    let before = raw(&s);
    let state = s.state();
    let history = s.doc.history_bytes();
    let entries = s.replay_journal(false).unwrap().entries.len();
    let untouched = s.doc.layers[2].pixels.as_ptr();
    let plan = call(&mut s, "canvas.liquify.plan", json!(r));
    assert_eq!(plan["layer_ids"], json!([2, 1]));
    assert_eq!(plan["mask_ids"], json!([1]));
    let draft = Draft::begin(&s, r.clone()).unwrap();
    let expected = draft.preview(&s).unwrap();
    drop(draft);
    let png = call(&mut s, "canvas.liquify.preview", json!(r));
    assert_eq!(raw(&s), before);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), entries);
    let encoded =
        png_io::encode(12, 8, expected.into_iter(), s.doc.info().export_density()).unwrap();
    assert_eq!(png["data"], codec::base64(&encoded));
    let apply: Action = serde_json::from_value(png["apply"].clone()).unwrap();
    s.apply(apply).unwrap();
    let after = raw(&s);
    assert_ne!(after["layers"][0], before["layers"][0]);
    assert_ne!(after["layers"][1], before["layers"][1]);
    assert_eq!(after["layers"][2], before["layers"][2]);
    assert_eq!(s.doc.layers[2].pixels.as_ptr(), untouched);
    assert_eq!(after["selection"], before["selection"]);
    assert_eq!(after["recipe"], before["recipe"]);
    assert_eq!(after["masks"][1], before["masks"][1]);
    assert_eq!(s.revision, r.document_revision + 1);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), entries + 1);
    check(&mut s);
    let end = s.replay_journal(false).unwrap().entries.len() - 1;
    let outcome = s.replay_preview(end, false).unwrap();
    let mut restored = crate::engine::Document::new(1, 1);
    outcome.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for i in 0..restored.layers.len() {
        assert_eq!(restored.layers[i].pixels, s.doc.layers[i].pixels);
    }
    for m in &restored.mask_state().items {
        assert_eq!(
            (0..96)
                .map(|i| restored.mask_plane(m.id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            (0..96)
                .map(|i| s.doc.mask_plane(m.id).unwrap().sample(i))
                .collect::<Vec<_>>()
        );
    }
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw(&s), before);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(raw(&s), after);
    check(&mut s);
}
#[test]
fn typed_liquify_draft_is_cancelable_and_stale_or_invalid_steps_preserve_source() {
    let mut s = fixture();
    let mut r = request(&s);
    r.steps.clear();
    let before = raw(&s);
    let history = s.doc.history_bytes();
    let mut draft = Draft::begin(&s, r.clone()).unwrap();
    let bad = Push {
        x: 5.,
        y: 4.,
        dx: 10.,
        dy: 0.,
        radius: 4.,
        strength: 1.,
    };
    assert!(draft.push(&s, &[bad]).is_err());
    let good = Push { dx: 1., ..bad };
    draft.push(&s, &[good]).unwrap();
    let _ = draft.preview(&s).unwrap();
    drop(draft);
    assert_eq!(raw(&s), before);
    assert_eq!(s.doc.history_bytes(), history);
    let mut stale = Draft::begin(&s, r.clone()).unwrap();
    call(
        &mut s,
        "layer.rename",
        json!({"layer_id":1,"name":"Changed source"}),
    );
    assert!(stale.push(&s, &[good]).is_err());
    assert!(stale.preview(&s).is_err());
    assert!(s.apply(r.apply_action()).is_err());
    let mut locked = request(&s);
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":1,"position_locked":true}),
    );
    locked.document_revision = s.revision;
    assert!(s.apply(locked.apply_action()).is_err());
    assert_eq!(raw(&s), before);
}
static CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn fail_second(
    _: *const std::ffi::c_void,
    _: u32,
    _: *const abi::Pixels,
    out: *mut abi::Pixels,
) -> i32 {
    if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        (*out).data.write(0xff123456);
        abi::OK
    } else {
        (*out).data.write(0xffabcdef);
        abi::ERROR
    }
}
#[test]
fn missing_capability_and_late_owner_sampling_failure_publish_nothing() {
    let mut s = fixture();
    let r = request(&s);
    let before = raw(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let original = native.replace_liquify_for_test(None).unwrap();
    assert!(s.apply(r.apply_action()).is_err());
    assert_eq!(raw(&s), before);
    assert_eq!(s.state(), public);
    let mut broken = original;
    broken.pixels = Some(fail_second);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_liquify_for_test(Some(broken));
    CALLS.store(0, Ordering::SeqCst);
    assert!(s.apply(r.apply_action()).is_err());
    assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(raw(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_liquify_for_test(Some(original));
    check(&mut s);
}

#[test]
fn liquify_post_publication_render_failure_restores_raw_history_and_surface() {
    let mut s = fixture();
    let r = request(&s);
    let _ = codec::png(&mut s.doc).unwrap();
    let renderer = Arc::new(super::owner_affine_tests::FailRender {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: std::cell::Cell::new(true),
        failures: std::cell::Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    let before = raw(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let surface = s.doc.composite.clone();
    assert!(s.apply(r.apply_action()).is_err());
    assert_eq!(renderer.failures.get(), 1);
    assert_eq!(raw(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composite, surface);
    check(&mut s);
}

#[test]
fn liquify_plan_observes_live_contact_while_preview_apply_and_open_preserve_it() {
    let mut s = fixture();
    let p = Pointer {
        phase: PointerPhase::Down,
        x: 3.,
        y: 3.,
        pressure: 0.4,
        time: 100,
        pen: true,
        eraser: false,
        tool_override: Some(ToolKind::Brush),
        view_x: Some(30.),
        view_y: Some(30.),
    };
    s.pointer(p).unwrap();
    let r = request(&s);
    let before = raw(&s);
    let public = s.state();
    assert_eq!(
        call(&mut s, "canvas.liquify.plan", json!(r))["contact_active"],
        true
    );
    for (id, args) in [
        ("canvas.liquify", json!(r)),
        ("canvas.liquify.preview", json!(r)),
        ("canvas.liquify.open", json!({})),
    ] {
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "stroke_active"
        );
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        assert_eq!(raw(&s), before);
        assert_eq!(s.state(), public);
    }
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: 116,
        ..p
    })
    .unwrap();
    check(&mut s);
}
