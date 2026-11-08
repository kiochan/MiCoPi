//! Direct quantization drafts are immutable queries until guarded Apply.
use super::{
    editing_tests::{call, check, session},
    *,
};
use serde_json::{json, Value};
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#20406080"}));
    s
}
fn cases() -> [(&'static str, Value); 2] {
    [
        ("threshold", json!({"level":54})),
        ("posterize", json!({"levels":4})),
    ]
}
fn request(s: &Session, parameters: Value) -> Value {
    json!({"document_identity":s.doc.identity,"document_revision":s.revision,"layer_id":s.doc.layer_id(s.doc.active),"parameters":parameters})
}
#[test]
fn quantization_png_previews_preserve_soft_selection_masked_state_and_match_apply() {
    for (effect, parameters) in cases() {
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
        let args = request(&s, parameters);
        let state = s.state();
        let raw = s.doc.layers[0].pixels.clone();
        let composite = s.doc.composite.clone();
        let history = s.doc.history_bytes();
        let events = s.journal.as_ref().unwrap().entries.len();
        let result = call(&mut s, &format!("layer.{effect}.preview"), args.clone());
        assert_eq!(result["cost"]["pixel_scratch_bytes"], 96 * 4 * 2);
        assert_eq!(result["mimeType"], "image/png");
        crate::plugins::validate(
            s.registry.actions[&format!("layer.{effect}.preview")]
                .output_schema
                .as_ref()
                .unwrap(),
            &result,
        )
        .unwrap();
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.layers[0].pixels, raw);
        assert_eq!(s.doc.composite, composite);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
        let action: Action = serde_json::from_value(result["apply"].clone()).unwrap();
        assert_eq!(action.expected_revision, Some(s.revision));
        s.apply(action).unwrap();
        assert_eq!(
            codec::base64(&codec::png(&mut s.doc).unwrap()),
            result["data"].as_str().unwrap()
        );
        assert_eq!(s.revision, args["document_revision"].as_u64().unwrap() + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        assert_eq!(
            s.apply(Action::new(format!("layer.{effect}.preview"), args))
                .unwrap_err()
                .code,
            "revision_conflict"
        );
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, raw);
        assert_eq!(s.doc.composite, composite);
        check(&mut s);
    }
}
#[test]
fn quantization_controls_queue_defaults_without_revision_and_reject_atomic_headless_use() {
    for (effect, default) in [
        ("threshold", json!({"level":128})),
        ("posterize", json!({"levels":4})),
    ] {
        let mut s = fixture();
        let action = format!("layer.{effect}.open");
        assert_eq!(
            s.apply(Action::new(&action, json!({}))).unwrap_err().code,
            "ui_unavailable"
        );
        assert!(s.ui_effects.is_empty());
        s.runtime = "desktop";
        let state = s.state();
        let result = call(&mut s, &action, json!({}));
        assert_eq!(s.state(), state);
        let queued = s.ui_effects.pop_front().unwrap();
        assert_eq!(queued["id"], format!("{effect}.open"));
        assert_eq!(queued["value"], default);
        crate::plugins::validate(
            s.registry.actions[&action].output_schema.as_ref().unwrap(),
            &result,
        )
        .unwrap();
        assert_eq!(
            s.apply(Action::new(
                "pipeline.run",
                json!({"atomic":true,"steps":[{"id":"open","action":action,"args":{}}]})
            ))
            .unwrap_err()
            .code,
            "non_atomic_action"
        );
        assert!(s.ui_effects.is_empty());
    }
}
#[test]
fn quantization_preview_missing_capability_rejects_without_rendering_or_state_changes() {
    use crate::engine::graph::Renderer;
    use micopi_plugin_api as abi;
    struct Legacy;
    impl Renderer for Legacy {
        fn render(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            panic!("Missing capability must reject before rendering")
        }
    }
    for (effect, parameters) in cases() {
        let mut s = fixture();
        s.doc.renderer = Some(std::sync::Arc::new(Legacy));
        let state = s.state();
        let args = request(&s, parameters);
        assert_eq!(
            s.apply(Action::new(format!("layer.{effect}.preview"), args))
                .unwrap_err()
                .code,
            "quantization_unavailable"
        );
        assert_eq!(s.state(), state);
    }
}
#[test]
fn quantization_read_only_rejections_never_flush_or_finish_pressure_contacts() {
    for (effect, parameters) in cases() {
        let mut s = fixture();
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
        let args = request(&s, parameters);
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        let instances = s.native_instances.len();
        let raw = s.doc.layers[0].pixels.clone();
        assert_eq!(
            s.apply(Action::new(format!("layer.{effect}.preview"), args))
                .unwrap_err()
                .code,
            "stroke_active"
        );
        assert!(s.stroke.active());
        assert_eq!(s.revision, revision);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
        assert_eq!(s.native_instances.len(), instances);
        assert_eq!(s.doc.layers[0].pixels, raw);
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
}
