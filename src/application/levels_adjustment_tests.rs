//! Affect-below Levels use the same adjustment, mask and verified-outcome path.
use super::editing_tests::call;
use super::*;
use crate::{
    engine::{
        adjustments::AdjustmentSpec, graph::Filter, levels::Levels, replay::Journal, Document,
    },
    plugins::Registry,
};
use serde_json::json;
use std::path::PathBuf;
fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
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
    call(
        s,
        "adjustment.create",
        json!({"node_type":"compositing.levels","parent_id":parent,"index":1,
        "parameters":{"parameters":{"gamma":2}}}),
    )["adjustment_id"]
        .as_u64()
        .unwrap()
}
#[test]
fn levels_adjustment_is_a_real_non_raster_alpha_preserving_item_with_shared_state() {
    let mut s = fixture();
    let raw = s.doc.layers[0].pixels.clone();
    let id = create(&mut s, 0);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layers[0].pixels, raw);
    assert_eq!(s.doc.composite, vec![0x80808080; 3]);
    assert_eq!(s.doc.adjustment(id).unwrap().levels.unwrap().gamma, 2.);
    let state = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    assert_eq!(state["parameters"]["parameters"]["gamma"], 2.);
    assert!(state["parameters"].get("filter").is_none());
    crate::plugins::validate(
        s.registry.actions["adjustment.get"]
            .output_schema
            .as_ref()
            .unwrap(),
        &state,
    )
    .unwrap();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite, raw);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composite, vec![0x80808080; 3]);
}
#[test]
fn levels_partial_parameters_and_provider_switches_keep_each_contract_complete() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"node_type":"compositing.levels","parameters":{"parameters":{"gamma":1,"input_black":0.0}}}),
    );
    assert_eq!(s.doc.composite, vec![0x80404040; 3]);
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
        json!({"adjustment_id":id,"node_type":"compositing.levels"}),
    );
    assert_eq!(
        s.doc.adjustment(id).unwrap().levels.unwrap(),
        Levels::default()
    );
    assert_eq!(s.doc.composite, vec![0x80404040; 3]);
}
#[test]
fn levels_adjustment_mask_and_strength_mix_once_without_reapplying_alpha() {
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
    assert_eq!(s.doc.composite, vec![0x80606060; 3]);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"opacity":128./255.}),
    );
    assert_eq!(s.doc.composite, vec![0x80505050; 3]);
    assert_eq!(s.doc.layers[0].pixels, vec![0x80404040; 3]);
    assert!((0..3).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 128));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composite, vec![0x80606060; 3]);
}
#[test]
fn levels_adjustment_respects_isolated_and_pass_through_group_boundaries() {
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
        assert_eq!(s.doc.composite, vec![0x40808080; 3]);
        assert_eq!(s.doc.layers[0].pixels, vec![0x80404040; 3]);
        assert_eq!(s.doc.active, 0);
    }
}
#[test]
fn levels_adjustment_rejects_hidden_unsupported_and_ambiguous_definitions_before_publication() {
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
    assert!(s
        .apply(Action::new(
            "adjustment.set",
            json!({"adjustment_id":id,"parameters":{"filter":"invert","parameters":{"gamma":1}}})
        ))
        .is_err());
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
    };
    let legacy = serde_json::to_value(&spec).unwrap();
    assert!(legacy.get("levels").is_none());
    assert_eq!(
        serde_json::from_value::<AdjustmentSpec>(legacy).unwrap(),
        spec
    );
    assert!(AdjustmentSpec {
        levels: Some(Levels::default()),
        ..spec
    }
    .validate()
    .is_err());
}
#[test]
fn levels_adjustment_atomic_failure_projects_and_dll_free_outcomes_keep_exact_effects() {
    let mut s = fixture();
    let id = create(&mut s, 0);
    let before = s.doc.composition();
    let surface = s.doc.composite.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"change","action":"adjustment.set","args":{"adjustment_id":id,"parameters":{"parameters":{"gamma":1}}}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.composite, surface);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/levels-adjustment/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("levels.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut loaded = fixture();
    call(&mut loaded, "document.open", json!({"path":path}));
    assert_eq!(loaded.doc.composition(), s.doc.composition());
    assert_eq!(loaded.doc.composite, surface);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composite, surface);
    assert_eq!(restored.composition(), s.doc.composition());
}
