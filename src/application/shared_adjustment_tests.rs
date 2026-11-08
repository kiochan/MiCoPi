//! Public shared controller schemas, atomic outcomes and source-fenced preview gates.
use crate::application::{Action, Session};
use crate::plugins::Registry;
use serde_json::{json, Value};
use std::path::PathBuf;
fn call(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()["result"].clone()
}
fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
    )
    .unwrap();
    call(&mut s, "canvas.new", json!({"width":4,"height":3}));
    call(&mut s, "native.checkerboard", json!({"cell":1}));
    call(&mut s, "layer.duplicate", json!({"layer_id":1}));
    s
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn create(s: &mut Session) -> u64 {
    call(s,"adjustment.create",json!({"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1},"target_layer_ids":[1,2]}))["adjustment_id"].as_u64().unwrap()
}
#[test]
fn shared_controller_public_state_matches_workflow_and_state_schemas() {
    let mut s = fixture();
    let id = create(&mut s);
    for targets in [json!([1, 2]), json!([])] {
        call(
            &mut s,
            "adjustment.set",
            json!({"adjustment_id":id,"target_layer_ids":targets}),
        );
        let state = call(&mut s, "canvas.state", json!({}));
        crate::plugins::validate(&s.registry.actions["canvas.state"].output_schema, &state)
            .unwrap();
        crate::plugins::validate(
            &s.registry.states["core.groups"].output_schema,
            &state["groups"],
        )
        .unwrap();
        let result = call(
            &mut s,
            "pipeline.run",
            json!({"atomic":true,"steps":[{"id":"state","action":"canvas.state","args":{}}]}),
        );
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["results"]["state"]["result"], state);
    }
}
#[test]
fn shared_controller_binding_is_explicit_unbound_and_independent_of_selection() {
    let mut s = fixture();
    let before = raw(&s);
    let id = create(&mut s);
    let state = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    assert_eq!(state["scope"], "targets");
    assert_eq!(state["target_layer_ids"], json!([1, 2]));
    call(&mut s, "layer.select", json!({"layer_id":1}));
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"name":"Shared inks"}),
    );
    assert_eq!(
        s.doc.adjustment(id).unwrap().target_layer_ids,
        Some(vec![1, 2])
    );
    assert_eq!(raw(&s), before);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"target_layer_ids":[]}),
    );
    assert_eq!(s.doc.adjustment(id).unwrap().target_layer_ids, Some(vec![]));
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"scope":"affect_below"}),
    );
    assert!(s.doc.adjustment(id).unwrap().target_layer_ids.is_none());
    for args in [
        json!({"adjustment_id":id,"scope":"affect_below","target_layer_ids":[]}),
        json!({"adjustment_id":id,"target_layer_ids":[1,1]}),
        json!({"adjustment_id":id,"target_layer_ids":[99]}),
    ] {
        let state = s.state();
        assert!(s.apply(Action::new("adjustment.set", args)).is_err());
        assert_eq!(s.state(), state);
    }
}
#[test]
fn shared_preview_apply_undo_and_dll_free_outcomes_keep_raw_sources_and_binding() {
    let mut s = fixture();
    let id = create(&mut s);
    let before = raw(&s);
    let old = s.doc.adjustment(id).unwrap().clone();
    let revision = s.revision;
    let history = s.doc.history_bytes();
    let state = s.state();
    let args = json!({"document_identity":s.doc.identity,"document_revision":revision,"adjustment_id":id,"node_type":"compositing.hsl","parameters":{"hsl":{"hue":120,"saturation":0,"lightness":0}},"target_layer_ids":[1]});
    let preview = call(&mut s, "adjustment.preview", args);
    assert_eq!(s.state(), state);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(raw(&s), before);
    let action: Action = serde_json::from_value(preview["apply"].clone()).unwrap();
    assert!(action.args.get("document_identity").is_none());
    assert_eq!(action.expected_revision, Some(revision));
    s.apply(action).unwrap();
    assert_eq!(s.revision, revision + 1);
    let after = s.doc.adjustment(id).unwrap().clone();
    assert_eq!(after.target_layer_ids, Some(vec![1]));
    let png = call(&mut s, "canvas.snapshot", json!({}));
    assert_eq!(preview["data"], png["data"]);
    assert_eq!(raw(&s), before);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    let n = s.replay_journal(false).unwrap().entries.len() - 1;
    let result = s.replay_preview(n, false).unwrap();
    let mut restored = crate::engine::Document::new(1, 1);
    result.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.adjustment(id).unwrap(), &after);
    assert_eq!(restored.composite, s.doc.composite);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.adjustment(id).unwrap(), &old);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.adjustment(id).unwrap(), &after);
    let mut stale = preview["apply"].clone();
    stale["expected_revision"] = json!(revision);
    assert!(s.apply(serde_json::from_value(stale).unwrap()).is_err());
}
#[test]
fn shared_edit_is_one_atomic_child_and_later_failure_restores_controller_and_revision() {
    let mut s = fixture();
    let id = create(&mut s);
    let old = s.doc.adjustment(id).unwrap().clone();
    let revision = s.revision;
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"binding","action":"adjustment.set","args":{"adjustment_id":id,"target_layer_ids":[1]}},{"id":"late","action":"adjustment.set","args":{"adjustment_id":9999,"name":"Missing controller"}}]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(s.doc.adjustment(id).unwrap(), &old);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.history_bytes(), history);
    let registry = call(&mut s, "node.catalog", json!({}));
    assert!(registry["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["id"] == "action.adjustment.preview"));
}

struct FailColor {
    inner: std::sync::Arc<dyn crate::engine::graph::Renderer>,
    armed: std::cell::Cell<bool>,
    failures: std::cell::Cell<usize>,
}
impl crate::engine::graph::Renderer for FailColor {
    fn supports_masks(&self) -> bool {
        self.inner.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.inner.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.inner.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.inner.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.inner.supports_blend_mode(mode)
    }
    fn supports_levels(&self) -> bool {
        self.inner.supports_levels()
    }
    fn supports_levels_channels(&self) -> bool {
        self.inner.supports_levels_channels()
    }
    fn supports_curves(&self) -> bool {
        self.inner.supports_curves()
    }
    fn supports_hsl(&self) -> bool {
        self.inner.supports_hsl()
    }
    fn supports_quantization(&self) -> bool {
        self.inner.supports_quantization()
    }
    fn render(
        &self,
        nodes: &[micopi_plugin_api::CompositeNode],
        layers: &[micopi_plugin_api::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        self.inner.render(nodes, layers, width, height, output)
    }
    fn render_color(
        &self,
        nodes: &[micopi_plugin_api::CompositeNode],
        layers: &[micopi_plugin_api::CompositeLayer],
        masks: &[micopi_plugin_api::CompositeMask],
        luts: &[micopi_plugin_api::CompositeLut],
        hsl: &[micopi_plugin_api::HslParameters],
        width: u32,
        height: u32,
        output: &mut micopi_plugin_api::Pixels,
    ) -> Result<(), String> {
        if self.armed.replace(false) {
            unsafe { output.data.write(0xdeadc0de) };
            self.failures.set(self.failures.get() + 1);
            return Err("Injected shared adjustment compositor failure".into());
        }
        self.inner
            .render_color(nodes, layers, masks, luts, hsl, width, height, output)
    }
}
#[test]
fn shared_private_preview_and_post_publication_render_failure_restore_all_source_state() {
    let mut s = fixture();
    let id = create(&mut s);
    call(&mut s, "canvas.snapshot", json!({}));
    let renderer = std::sync::Arc::new(FailColor {
        inner: s.doc.renderer.as_ref().unwrap().clone(),
        armed: std::cell::Cell::new(true),
        failures: std::cell::Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    let before = s.state();
    let raw = raw(&s);
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let surface = s.doc.composite.clone();
    let args = json!({"adjustment_id":id,"document_identity":s.doc.identity,"document_revision":s.revision,"target_layer_ids":[1],"parameters":{"amount":0.5}});
    assert!(s.apply(Action::new("adjustment.preview", args)).is_err());
    assert_eq!(renderer.failures.get(), 1);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composite, surface);
    assert_eq!(s.doc.composition(), composition);
    renderer.armed.set(true);
    assert!(s
        .apply(Action::new(
            "adjustment.set",
            json!({"adjustment_id":id,"target_layer_ids":[1],"parameters":{"amount":0.5}})
        ))
        .is_err());
    assert_eq!(renderer.failures.get(), 2);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composite, surface);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        raw
    );
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}
