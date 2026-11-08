//! Read-only native previews agree with Apply without publishing draft pixels.
use super::{
    editing_tests::{call, check, session},
    *,
};
use crate::engine::hsl::Hsl;
use crate::plugins::hsl_preview::{self, Request};
use serde_json::json;

fn request(s: &Session) -> Request {
    Request {
        document_identity: s.doc.identity,
        document_revision: s.revision,
        layer_id: s.doc.layer_id(s.doc.active),
        hsl: Hsl {
            hue: 120.,
            saturation: 0.,
            lightness: 25.,
        },
    }
}
#[test]
fn hsl_dialog_is_desktop_only_fenced_bounded_and_non_atomic() {
    let mut s = session();
    assert_eq!(
        s.apply(Action::new("layer.hue_saturation.open", json!({})))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    assert!(s.ui_effects.is_empty());
    s.runtime = "desktop";
    let state = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "layer.hue_saturation.open",
        json!({"parameters":{"hue":120}}),
    );
    assert_eq!(result["queued"], true);
    assert_eq!(result["revision"], s.revision);
    let effect = s.ui_effects.pop_front().unwrap();
    assert_eq!(effect["id"], "hsl.open");
    assert_eq!(effect["document_identity"], s.doc.identity);
    assert_eq!(effect["layer_id"], s.doc.layer_id(s.doc.active));
    assert_eq!(
        effect["value"],
        json!({"hue":120.,"saturation":0.,"lightness":0.})
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.apply(Action::new("pipeline.run", json!({"atomic":true,"steps":[{"id":"open","action":"layer.hue_saturation.open","args":{}}]}))).unwrap_err().code, "non_atomic_action");
    assert!(s.ui_effects.is_empty());
    for _ in 0..128 {
        s.ui_effects.push_back(effect.clone());
    }
    assert_eq!(
        s.apply(Action::new("layer.hue_saturation.open", json!({})))
            .unwrap_err()
            .code,
        "presentation_queue_full"
    );
    assert_eq!(s.ui_effects.len(), 128);
    assert_eq!(s.state(), state);
    s.ui_effects.clear();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.3,"pen":true,"time":1,"tool_override":"brush"}),
    );
    let revision = s.revision;
    let pixels = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "layer.hue_saturation.open",
        json!({"parameters":{"hue":120}}),
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":5,"y":3,"pressure":0.8,"pen":true,"time":2,"tool_override":"brush"}),
    );
    check(&mut s);
}
#[test]
fn hsl_preview_has_independent_primary_pixels_and_preserves_hidden_rgb_under_lock() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
    let mut draft = request(&s);
    draft.hsl.lightness = 0.;
    assert_eq!(
        hsl_preview::render(&s, draft).unwrap().0,
        vec![0x8000ff00; 96]
    );
    // An immutable host fixture exercises hidden bytes without creating an
    // artificial document event. This case makes no recording/history claim.
    s.doc.layers[0].pixels[0] = 0x00334455;
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"position_locked":true}),
    );
    draft = Request {
        hsl: draft.hsl,
        ..request(&s)
    };
    let raw = s.doc.layers[0].pixels.clone();
    let preview = hsl_preview::render(&s, draft).unwrap().0;
    assert_eq!(preview[0], 0);
    assert_eq!(preview[1], 0x8000ff00);
    assert_eq!(s.doc.layers[0].pixels, raw);
}
#[test]
fn hsl_preview_matches_selected_masked_filtered_apply_and_one_undo() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
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
        json!({"kind":"rectangle","x":3,"y":2,"width":6,"height":4}),
    );
    call(&mut s, "selection.feather", json!({"radius":1}));
    call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"compositing.hsl","parameters":{"hsl":{"hue":-120}}}),
    );
    let draft = request(&s);
    let before = s.state();
    let raw = s.doc.layers[0].pixels.clone();
    let composite = s.doc.composite.clone();
    let composition = s.doc.composition();
    let events = s.journal.as_ref().unwrap().entries.len();
    let history = s.doc.history_bytes();
    let instances = s.native_instances.len();
    let pointer = s.doc.layers[0].pixels.as_ptr();
    assert!(s
        .doc
        .preview_owner_transform(&[(draft.layer_id, &raw)], &[])
        .unwrap_err()
        .contains("Unlock layer position"));
    let (preview, cost) = hsl_preview::render(&s, draft).unwrap();
    assert_eq!(cost.source_scratch_bytes, 12 * 8 * 4);
    assert_eq!(cost.composite_bytes, cost.source_scratch_bytes);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels.as_ptr(), pointer);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(s.native_instances.len(), instances);
    assert!((0..96).all(|index| s.doc.mask_plane(mask).unwrap().sample(index) == 127));
    s.apply(draft.apply_action()).unwrap();
    s.doc.compose();
    assert_eq!(s.doc.composite, preview);
    assert_eq!(s.revision, draft.document_revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_ne!(s.doc.layers[0].pixels, raw);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, composite);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composite, preview);
    check(&mut s);
}
#[test]
fn hsl_preview_action_returns_png_apply_contract_and_no_document_event() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ff000080"}));
    let draft = request(&s);
    let before = s.state();
    let events = s.journal.as_ref().unwrap().entries.len();
    let expected = hsl_preview::render(&s, draft).unwrap().0;
    let png =
        crate::application::png_io::encode(12, 8, expected, s.doc.info().export_density()).unwrap();
    let result = call(&mut s, "layer.hue_saturation.preview", json!(draft));
    assert_eq!(result["mimeType"], "image/png");
    assert_eq!(result["data"], codec::base64(&png));
    assert_eq!(result["revision"], draft.document_revision);
    assert_eq!(result["layer_id"], draft.layer_id);
    let apply: Action = serde_json::from_value(result["apply"].clone()).unwrap();
    assert_eq!(apply.expected_revision, Some(draft.document_revision));
    assert_eq!(apply.action, "layer.hue_saturation");
    assert_eq!(apply.args, json!(draft.hsl));
    assert_eq!(s.state(), before);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    let catalog = call(&mut s, "node.catalog", json!({}));
    assert!(catalog["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == "action.layer.hue_saturation.preview"));
}
#[test]
fn hsl_preview_rejects_stale_targets_locks_masks_and_contacts_without_finishing_input() {
    let mut s = session();
    let draft = request(&s);
    for invalid in [
        Request {
            document_identity: draft.document_identity + 1,
            ..draft
        },
        Request {
            document_revision: draft.document_revision + 1,
            ..draft
        },
    ] {
        assert_eq!(
            hsl_preview::plan(&s, invalid).unwrap_err().code,
            "revision_conflict"
        );
    }
    assert_eq!(
        hsl_preview::plan(
            &s,
            Request {
                layer_id: 999,
                ..draft
            }
        )
        .unwrap_err()
        .code,
        "invalid_layer"
    );
    call(&mut s, "layer.set", json!({"pixel_locked":true}));
    assert_eq!(
        hsl_preview::plan(&s, request(&s)).unwrap_err().code,
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
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":mask}}),
    );
    assert_eq!(
        hsl_preview::plan(&s, request(&s)).unwrap_err().code,
        "mask_target_pending"
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.4,"pen":true,"time":1,"tool_override":"brush"}),
    );
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        s.apply(Action::new(
            "layer.hue_saturation.preview",
            json!(request(&s))
        ))
        .unwrap_err()
        .code,
        "stroke_active"
    );
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"move","x":4,"y":3,"pressure":0.8,"pen":true,"time":2,"tool_override":"brush"}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":5,"y":3,"pressure":0.2,"pen":true,"time":3,"tool_override":"brush"}),
    );
    check(&mut s);
}

#[test]
fn hsl_preview_rejects_missing_capability_and_discards_failed_private_render() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    use std::sync::Arc;
    struct Failing(bool);
    impl Renderer for Failing {
        fn supports_hsl(&self) -> bool {
            self.0
        }
        fn render(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            panic!("Preview failure must not compose committed storage")
        }
        fn render_hsl(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: &[abi::CompositeMask],
            _: &[abi::CompositeLut],
            _: &[abi::HslParameters],
            _: u32,
            _: u32,
            output: &mut abi::Pixels,
        ) -> Result<(), String> {
            unsafe {
                *output.data = 0xff112233;
            }
            Err("Injected private HSL render failure".into())
        }
    }
    let mut s = session();
    let pixels = s.doc.layers[0].pixels.clone();
    let composite = s.doc.composite.clone();
    let events = s.journal.as_ref().unwrap().entries.len();
    s.doc.renderer = Some(Arc::new(Failing(false)));
    // Installing the fixture intentionally changes advertised renderer
    // capabilities. Compare against that source, not the original DLL's state.
    let before = s.state();
    assert_eq!(
        hsl_preview::render(&s, request(&s)).unwrap_err().code,
        "hsl_unavailable"
    );
    s.doc.renderer = Some(Arc::new(Failing(true)));
    assert_eq!(
        hsl_preview::render(&s, request(&s)).unwrap_err().message,
        "Injected private HSL render failure"
    );
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.composite, composite);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
}
