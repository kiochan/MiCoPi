//! Typed quantization intent shares native composition, history and transports.
use super::{
    editing_tests::{call, check, session},
    *,
};
use crate::engine::{graph::Renderer, replay::Journal, Document};
use serde_json::{json, Value};
fn cases() -> [(&'static str, u32, u32); 2] {
    [("threshold", 54, 0x80ffffff), ("posterize", 4, 0x80005555)]
}
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":1}));
    call(&mut s, "layer.fill_color", json!({"color":"#20406080"}));
    s
}
fn graph(effect: &str, value: u32, linked: bool) -> Value {
    json!({"enabled":true,"linked":linked,"nodes":[
        {"id":11,"kind":"layer","index":0,"position":[0,0]},
        {"id":22,"kind":"quantization","node_type":format!("compositing.{effect}"),"quantization":{"effect":effect,"value":value},"position":[200,0]},
        {"id":33,"kind":"output","position":[400,0]}],"edges":[{"from":11,"to":22,"input":0},{"from":22,"to":33,"input":0}]})
}
#[test]
fn quantization_source_graphs_preserve_raw_pixels_undo_and_fenced_observations() {
    for (effect, value, expected) in cases() {
        for linked in [false, true] {
            let mut s = fixture();
            let raw = s.doc.layers[0].pixels.clone();
            let revision = s.revision;
            call(
                &mut s,
                "graph.set",
                json!({"graph":graph(effect,value,linked)}),
            );
            assert_eq!(s.revision, revision + 1);
            assert_eq!(s.doc.layers[0].pixels, raw);
            assert_eq!(s.doc.composite, vec![expected; 4]);
            let identity = s.doc.identity;
            let revision = s.revision;
            let sampled = call(
                &mut s,
                "histogram.query",
                json!({"source":"node","graph_view":"source","node_id":22,"port":"output","document_identity":identity,"document_revision":revision}),
            );
            assert_eq!(sampled["red"][(expected >> 16 & 255) as usize], 4);
            assert_eq!(s.revision, revision);
            call(&mut s, "history.undo", json!({}));
            assert_eq!(s.doc.composite, raw);
            call(&mut s, "history.redo", json!({}));
            assert_eq!(s.doc.composite, vec![expected; 4]);
            check(&mut s);
        }
    }
}
#[test]
fn anonymous_disabled_quantization_still_validates_effect_specific_ranges() {
    for (effect, value) in [("threshold", 256), ("posterize", 1)] {
        let mut s = fixture();
        let before = s.state();
        let history = s.doc.history_bytes();
        let mut graph = graph(effect, value, false);
        graph["enabled"] = json!(false);
        graph["nodes"][1]
            .as_object_mut()
            .unwrap()
            .remove("node_type");
        assert!(s
            .apply(Action::new("graph.set", json!({"graph":graph})))
            .is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
}
#[test]
fn quantization_adjustments_masks_provider_switches_and_project_outcomes_agree() {
    for (effect, value, expected) in cases() {
        let mut s = fixture();
        let raw = s.doc.layers[0].pixels.clone();
        let group = call(
            &mut s,
            "group.create",
            json!({"members":[{"kind":"layer","id":1}]}),
        )["group_id"]
            .as_u64()
            .unwrap();
        call(
            &mut s,
            "group.set",
            json!({"group_id":group,"compositing":"pass_through"}),
        );
        let id=call(&mut s,"adjustment.create",json!({"node_type":format!("compositing.{effect}"),"parent_id":group,"parameters":{"quantization":{"value":value}}}))["adjustment_id"].as_u64().unwrap();
        assert_eq!(s.doc.composite, vec![expected; 4]);
        assert_eq!(s.doc.layers[0].pixels, raw);
        let item = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
        assert_eq!(
            item["parameters"]["quantization"],
            json!({"effect":effect,"value":value})
        );
        crate::plugins::validate(
            s.registry.actions["adjustment.get"]
                .output_schema
                .as_ref()
                .unwrap(),
            &item,
        )
        .unwrap();
        let mask = call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":"adjustment","id":id}}),
        )["mask_id"]
            .as_u64()
            .unwrap();
        call(&mut s, "mask.fill", json!({"mask_id":mask,"value":0}));
        assert_eq!(s.doc.composite, raw);
        call(&mut s, "mask.fill", json!({"mask_id":mask,"value":255}));
        assert_eq!(s.doc.composite, vec![expected; 4]);
        let composition = s.doc.composition();
        let history = s.doc.history_bytes();
        let revision = s.revision;
        let failed = call(
            &mut s,
            "pipeline.run",
            json!({"atomic":true,"steps":[
            {"id":"edit","action":"adjustment.set","args":{"adjustment_id":id,"parameters":{"quantization":{"value":128}}}},
            {"id":"reject","action":"layer.select","args":{"layer_id":99999}}]}),
        );
        assert_eq!(failed["ok"], false);
        assert_eq!(failed["completed"], json!(["edit"]));
        assert_eq!(failed["rolled_back"], true);
        assert_eq!(failed["error"]["code"], "invalid_layer");
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.history_bytes(), history);
        let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/quantization-document/{}/{effect}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("quantization.mcp");
        call(&mut s, "document.save", json!({"path":path}));
        let mut opened = fixture();
        call(&mut opened, "document.open", json!({"path":path}));
        assert_eq!(opened.doc.adjustments(), s.doc.adjustments());
        assert_eq!(opened.doc.composite, s.doc.composite);
        let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
        let mut journal = Journal::import(&bytes).unwrap();
        let state = journal.preview(journal.entries.len() - 1).unwrap();
        let mut restored = Document::new(1, 1);
        state.restore_document(&mut restored);
        assert!(restored.renderer.is_none());
        assert_eq!(restored.composite, s.doc.composite);
        assert_eq!(restored.adjustments(), s.doc.adjustments());
        call(
            &mut s,
            "adjustment.set",
            json!({"adjustment_id":id,"node_type":"compositing.hsl","parameters":{"hsl":{"hue":120}}}),
        );
        assert!(s.doc.adjustment(id).unwrap().quantization.is_none());
        assert_eq!(s.doc.composite, vec![0x80602040; 4]);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.composite, vec![expected; 4]);
        check(&mut s);
    }
}
#[test]
fn unsupported_hidden_disabled_and_unused_quantization_reject_before_publication() {
    use micopi_plugin_api as abi;
    struct Legacy {
        native: std::sync::Arc<crate::plugins::native::NativePlugin>,
    }
    impl Renderer for Legacy {
        fn supports_mix(&self) -> bool {
            true
        }
        fn supports_transparent(&self) -> bool {
            true
        }
        fn render(
            &self,
            nodes: &[abi::CompositeNode],
            layers: &[abi::CompositeLayer],
            width: u32,
            height: u32,
            output: &mut abi::Pixels,
        ) -> Result<(), String> {
            // Atomic checkpoints/rollback may compose the existing legacy canvas.
            assert!(!nodes.iter().any(|n| matches!(n.kind, 12 | 13)));
            self.native.render(nodes, layers, width, height, output)
        }
        fn render_quantization(
            &self,
            _: &[abi::CompositeNode],
            _: &[abi::CompositeLayer],
            _: &[abi::CompositeMask],
            _: &[abi::CompositeLut],
            _: &[abi::HslParameters],
            _: u32,
            _: u32,
            _: &mut abi::Pixels,
        ) -> Result<(), String> {
            panic!("Preflight must reject quantization before its renderer is invoked")
        }
    }
    for (effect, value, _) in cases() {
        let mut s = fixture();
        s.doc.renderer = Some(std::sync::Arc::new(Legacy {
            native: s.registry.native_plugins["compositing"].clone(),
        }));
        let composition = s.doc.composition();
        let revision = s.revision;
        let history = s.doc.history_bytes();
        for (enabled, unused) in [(true, false), (false, false), (true, true)] {
            let mut graph = graph(effect, value, false);
            graph["enabled"] = json!(enabled);
            if unused {
                graph["edges"][1]["from"] = json!(11);
            }
            assert_eq!(
                s.apply(Action::new("graph.set", json!({"graph":graph})))
                    .unwrap_err()
                    .code,
                "quantization_unavailable"
            );
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.revision, revision);
            assert_eq!(s.doc.history_bytes(), history);
        }
        for (node_type, parameters, label) in [
            (
                format!("compositing.{effect}"),
                json!({"quantization":{"value":value}}),
                "quantization",
            ),
            ("compositing.hsl".into(), json!({"hsl":{"hue":120}}), "HSL"),
        ] {
            for (visible, opacity) in [(false, 1.), (true, 0.), (true, 1.)] {
                let error=s.apply(Action::new("adjustment.create",json!({"node_type":node_type,"visible":visible,"opacity":opacity,"parameters":parameters}))).unwrap_err();
                assert!(error.message.contains(label), "{}", error.message);
                assert_eq!(s.doc.composition(), composition);
                assert_eq!(s.revision, revision);
                assert_eq!(s.doc.history_bytes(), history);
            }
        }
    }
}

#[test]
fn quantization_mixed_slots_and_contact_rejection_preserve_document_contracts() {
    let mut s = fixture();
    let mut graph = graph("posterize", 4, false);
    graph["nodes"].as_array_mut().unwrap().extend([
        json!({"id":44,"kind":"hsl","node_type":"compositing.hsl","hsl":{"hue":120,"saturation":0,"lightness":0},"position":[0,200]}),
        json!({"id":55,"kind":"curves","node_type":"compositing.curves","curves":crate::engine::curves::Curves::default(),"position":[200,200]})]);
    graph["edges"] = json!([{"from":11,"to":44,"input":0},{"from":44,"to":55,"input":0},{"from":55,"to":22,"input":0},{"from":22,"to":33,"input":0}]);
    call(&mut s, "graph.set", json!({"graph":graph}));
    assert_eq!(s.doc.composite, vec![0x80550055; 4]);
    let plan = s.doc.graph.compile(1).unwrap();
    assert_eq!(plan.iter().find(|n| n.kind == 13).unwrap().mode, 4);
    assert_eq!(plan.iter().find(|n| n.kind == 11).unwrap().mode, 0);
    assert_eq!(plan.iter().find(|n| n.kind == 10).unwrap().mode, 0);
    let directory = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/quantization-source/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("source.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.graph, s.doc.graph);
    assert_eq!(opened.doc.composite, s.doc.composite);
    let pointer = Pointer {
        phase: PointerPhase::Down,
        x: 1.5,
        y: 0.5,
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
    assert_eq!(
        s.apply(Action::new("graph.set", json!({"graph":graph})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    let error = s
        .apply(Action::new(
            "adjustment.create",
            json!({"node_type":"compositing.threshold"}),
        ))
        .unwrap_err();
    assert_eq!(error.code, "stroke_active");
    assert!(s.stroke.active());
    assert_eq!(s.revision, revision);
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        time: 2,
        x: 2.5,
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
