//! Affect-below channel curves retain shared parameters and outcome contracts.
use super::{editing_tests::call, *};
use crate::{
    engine::{
        adjustments::AdjustmentSpec, curves::Curves, graph::Filter, levels::Levels,
        replay::Journal, Document,
    },
    plugins::Registry,
};
use serde_json::json;
fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    call(&mut s, "canvas.new", json!({"width":3,"height":1}));
    s.doc.layers[0].pixels.fill(0x80404040);
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    s
}
fn create(s: &mut Session, parent: u64) -> u64 {
    call(s,"adjustment.create",json!({"node_type":"compositing.curves","parent_id":parent,"index":1,"parameters":{"curves":{
        "master":{"interpolation":"linear","points":[{"input":0,"output":0},{"input":128,"output":64},{"input":255,"output":255}]},
        "red":{"points":[{"input":0,"output":255},{"input":255,"output":0}]}}}}))["adjustment_id"].as_u64().unwrap()
}
#[test]
fn curves_adjustments_are_non_raster_and_publish_only_the_selected_effect_contract() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, vec![0x80404040; 3]);
    assert_eq!(s.doc.composite, vec![0x80df2020; 3]);
    let state = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    assert!(state["parameters"].get("filter").is_none());
    assert!(state["parameters"].get("parameters").is_none());
    assert_eq!(
        state["parameters"]["curves"]["red"]["points"][0]["output"],
        255
    );
    crate::plugins::validate(
        s.registry.actions["adjustment.get"]
            .output_schema
            .as_ref()
            .unwrap(),
        &state,
    )
    .unwrap();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite, vec![0x80404040; 3]);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composite, vec![0x80df2020; 3]);
}
#[test]
fn nested_curve_patches_replace_point_arrays_preserve_channels_and_switch_providers() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"node_type":"compositing.curves","parameters":{"curves":{"master":{
        "points":[{"input":0.0,"output":0},{"input":255,"output":255}]}}}}),
    );
    assert_eq!(s.doc.composite, vec![0x80bf4040; 3]);
    let curves = s.doc.adjustment(id).unwrap().curves.as_ref().unwrap();
    assert_eq!(curves.master.points.len(), 2);
    assert_eq!(curves.red.points[0].output, 255);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"parameters":{"curves":{"blue":{"interpolation":"linear"}}}}),
    );
    assert_eq!(s.doc.composite, vec![0x80bf4040; 3]);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"node_type":"compositing.levels","parameters":{"parameters":{"gamma":2}}}),
    );
    assert!(s.doc.adjustment(id).unwrap().curves.is_none());
    assert_eq!(s.doc.composite, vec![0x80808080; 3]);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1}}),
    );
    assert!(s.doc.adjustment(id).unwrap().levels.is_none());
    assert_eq!(s.doc.composite, vec![0x80bfbfbf; 3]);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"node_type":"compositing.curves"}),
    );
    assert_eq!(
        s.doc.adjustment(id).unwrap().curves.as_ref().unwrap(),
        &Curves::default()
    );
    assert_eq!(s.doc.composite, vec![0x80404040; 3]);
}
#[test]
fn curves_mask_and_opacity_mix_once_without_changing_alpha_or_raw_planes() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"adjustment","id":id}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":128}));
    assert_eq!(s.doc.composite, vec![0x80903030; 3]);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"opacity":128./255.}),
    );
    assert_eq!(s.doc.composite, vec![0x80683838; 3]);
    assert_eq!(s.doc.layers[0].pixels, vec![0x80404040; 3]);
    assert!((0..3).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 128));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite, vec![0x80903030; 3]);
}
#[test]
fn curves_adjustment_obeys_isolated_and_pass_through_group_scope() {
    for policy in ["isolated", "pass_through"] {
        let mut s = fixture();
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
            json!({"group_id":group,"compositing":policy,"opacity":128./255.}),
        );
        create(&mut s, group);
        assert_eq!(s.doc.composite, vec![0x40df2020; 3]);
        assert_eq!(s.doc.layers[0].pixels, vec![0x80404040; 3]);
    }
}
#[test]
fn hidden_unsupported_curves_invalid_points_and_ambiguous_intents_reject_before_changes() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"visible":false}),
    );
    let before = s.doc.composition();
    let revision = s.revision;
    let history = s.doc.history_bytes();
    s.doc.renderer = None;
    assert!(s
        .apply(Action::new(
            "adjustment.set",
            json!({"adjustment_id":id,"opacity":0.5})
        ))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.revision, revision);
    assert_eq!(s.doc.history_bytes(), history);
    let spec = AdjustmentSpec {
        name: "Legacy".into(),
        visible: true,
        opacity: 255,
        node_type: "compositing.filter".into(),
        filter: Filter::Invert,
        amount: 1.,
        levels: None,
        curves: None,
        hsl: None,
        quantization: None,
        target_layer_ids: None,
    };
    let value = serde_json::to_value(&spec).unwrap();
    assert!(value.get("curves").is_none());
    assert!(value.get("levels").is_none());
    assert!(value.get("hsl").is_none());
    assert!(AdjustmentSpec {
        amount: 0.,
        levels: Some(Levels::default()),
        curves: Some(Curves::default()),
        ..spec.clone()
    }
    .validate()
    .is_err());
    assert!(AdjustmentSpec {
        curves: Some(Curves::default()),
        ..spec
    }
    .validate()
    .is_err());
    let mut s = fixture();
    let id = create(&mut s, 0);
    let before = s.doc.composition();
    let revision = s.revision;
    for points in [
        json!([]),
        json!([{"input":0,"output":0},{"input":0,"output":255}]),
        json!([{"output":0},{"input":255,"output":255}]),
    ] {
        assert!(s.apply(Action::new("adjustment.set",json!({"adjustment_id":id,"node_type":"compositing.curves","parameters":{"curves":{"green":{"points":points}}}}))).is_err());
        assert_eq!(s.doc.composition(), before);
        assert_eq!(s.revision, revision);
    }
}
#[test]
fn curves_adjustment_atomic_rollback_projects_and_dll_free_outcomes_restore_exact_results() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    let before = s.doc.composition();
    let pixels = s.doc.composite.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"change","action":"adjustment.set","args":{"adjustment_id":id,"parameters":{"curves":{"red":{"points":[{"input":0,"output":0},{"input":255,"output":255}]}}}}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.composite, pixels);
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/curves-adjustment/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("curves.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.composite, pixels);
    assert_eq!(opened.doc.adjustments(), s.doc.adjustments());
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composite, pixels);
    assert_eq!(restored.adjustments(), s.doc.adjustments());
}
