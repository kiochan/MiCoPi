//! Stable-ID layer selection and explicit batches share history and verified outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    replay::{Journal, Metadata},
    selection::{Mask, SelectionState},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn source() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":3}));
    for layer in 0..4 {
        if layer != 0 {
            call(&mut s, "layer.add", json!({}));
        }
        let pixels: Vec<_> = (0..12)
            .map(|i| 0xff000000 | ((layer as u32 + 1) << 16) | ((i as u32 + 1) * 0x101))
            .collect();
        s.doc.replace_region(0, 0, 4, 3, &pixels);
        call(
            &mut s,
            "layer.rename",
            json!({"name":format!("Ink {} 墨",layer+1)}),
        );
    }
    s.replay_boundary().unwrap();
    s
}
fn ids(s: &Session) -> Vec<u64> {
    (0..s.doc.layers.len()).map(|i| s.doc.layer_id(i)).collect()
}
fn selected(s: &Session) -> Value {
    s.state()["selected_layer_ids"].clone()
}
fn raw(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn output(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/layer-batch-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn batch(s: &mut Session, action: &str, layer_ids: Value, args: Value) -> Value {
    call(
        s,
        "layer.batch",
        json!({"action":format!("layer.{action}"),"layer_ids":layer_ids,"args":args}),
    )
}

#[test]
fn layer_batch_selection_is_canonical_persistent_state_without_pixel_undo() {
    let mut s = source();
    let all = ids(&s);
    let history = s.doc.history_bytes();
    let pixels = raw(&s);
    let count = s.journal.as_ref().unwrap().entries.len();
    let revision = s.revision;
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[2],all[0]]}),
    );
    assert_eq!(selected(&s), json!([all[0], all[2]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[0]);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    assert_eq!(s.revision, revision + 1);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[1],all[0]]}),
    );
    assert_eq!(s.doc.layer_id(s.doc.active), all[0]);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[1],all[0]],"active_layer_id":all[1]}),
    );
    assert_eq!(selected(&s), json!([all[0], all[1]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[1]);
    let state = s.state();
    for (i, row) in state["layers"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["selected"], i < 2);
        assert_eq!(row["active"], i == 1);
    }
    let before = s.state();
    let count = s.journal.as_ref().unwrap().entries.len();
    let query = call(&mut s, "layer.query", json!({"selected":true}));
    assert_eq!(query["selected_layer_ids"], json!([all[0], all[1]]));
    assert_eq!(query["active_layer_id"], all[1]);
    assert_eq!(query["matched"], 2);
    assert_eq!(
        call(&mut s, "layer.query", json!({"selected":false}))["matched"],
        2
    );
    let context = call(&mut s, "context.read", json!({"resources":["core.layers"]}));
    assert_eq!(
        context["resources"]["core.layers"]["value"]["selected_layer_ids"],
        selected(&s)
    );
    assert_eq!(s.state(), before);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(raw(&s), pixels);
    call(&mut s, "layer.select", json!({"layer_id":all[2]}));
    assert_eq!(selected(&s), json!([all[2]]));
    assert!(serde_json::to_value(s.doc.composition())
        .unwrap()
        .get("selected_layer_ids")
        .is_none());
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}

#[test]
fn layer_batch_invalid_targets_and_active_contact_rejections_preserve_pressure_continuation() {
    let mut s = source();
    let all = ids(&s);
    let before = s.state();
    let pixels = raw(&s);
    let history = s.doc.history_bytes();
    for args in [
        json!({}),
        json!({"layer_ids":[]}),
        json!({"layer_ids":[all[0],all[0]]}),
        json!({"layer_ids":[0]}),
        json!({"layer_ids":[99999]}),
        json!({"layer_ids":null}),
        json!({"layer_ids":[all[0]],"active_layer_id":all[1]}),
        json!({"layer_ids":[all[0]],"active_layer_id":null}),
        json!({"layer_ids":[all[0]],"extra":true}),
    ] {
        assert!(
            s.apply(Action::new("layer.selection.set", args.clone()))
                .is_err(),
            "{args}"
        );
        assert_eq!(s.state(), before);
        assert_eq!(raw(&s), pixels);
        assert_eq!(s.doc.history_bytes(), history);
    }
    call(
        &mut s,
        "brush.set",
        json!({"radius":1,"stabilizer":0.35,"pressure_size":true,"pressure_opacity":true}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":0.5,"y":0.5,"pressure":0.2,"pen":true,"time":0}),
    );
    let before = s.state();
    let pixels = raw(&s);
    let history = s.doc.history_bytes();
    let samples = s.replay_samples.len();
    let count = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        call(&mut s, "layer.query", json!({"selected":true}))["drawing"],
        true
    );
    assert_eq!(s.replay_samples.len(), samples);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    for (id, args) in [
        ("layer.selection.set", json!({"layer_ids":[all[0],all[2]]})),
        (
            "layer.batch",
            json!({"action":"layer.set","layer_ids":[all[0],all[2]],"args":{"color_label":"red"}}),
        ),
    ] {
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "stroke_active"
        );
        assert_eq!(s.state(), before);
        assert_eq!(raw(&s), pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(s.is_drawing());
    }
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"move","x":2,"y":1,"pressure":1,"pen":true,"time":30}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":3.5,"y":2.5,"pressure":0.4,"pen":true,"time":60}),
    );
    assert!(!s.is_drawing());
    assert_ne!(raw(&s), pixels);
    check(&mut s);
}

#[test]
fn layer_batch_structural_normalization_retains_surviving_ids_and_restores_selection_on_undo() {
    let mut s = source();
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]],"active_layer_id":all[2]}),
    );
    call(&mut s, "layer.move", json!({"layer_id":all[0],"to":3}));
    assert_eq!(selected(&s), json!([all[2], all[0]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[2]);
    call(&mut s, "layer.delete", json!({"layer_id":all[1]}));
    assert_eq!(selected(&s), json!([all[2], all[0]]));
    call(&mut s, "layer.delete", json!({"layer_id":all[2]}));
    assert_eq!(selected(&s), json!([all[0]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[0]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(selected(&s), json!([all[2], all[0]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[2]);
    let before = s.doc.composition();
    let active = s.doc.layer_id(s.doc.active);
    call(&mut s, "layer.add", json!({}));
    let added = s.doc.layer_id(s.doc.active);
    assert_eq!(selected(&s), json!([added]));
    call(&mut s, "history.undo", json!({}));
    let mut restored = s.doc.composition();
    assert!(restored.next_layer_id > before.next_layer_id);
    restored.next_layer_id = before.next_layer_id;
    assert_eq!(restored, before);
    assert_eq!(s.doc.layer_id(s.doc.active), active);
    assert_eq!(selected(&s), json!([all[2], all[0]]));
    check(&mut s);
}

#[test]
fn layer_batch_set_and_exact_pixel_edits_have_one_undo_event_and_revision() {
    let mut s = source();
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[1],all[3]],"active_layer_id":all[1]}),
    );
    let before = s.doc.composition();
    let pixels = raw(&s);
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    let events = s.journal.as_ref().unwrap().entries.len();
    let revision = s.revision;
    batch(
        &mut s,
        "set",
        json!([all[2], all[0]]),
        json!({"color_label":"red","pixel_locked":true,"position_locked":true,"opacity":0.5}),
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(selected(&s), json!([all[1], all[3]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[1]);
    for (i, row) in s.state()["layers"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["pixel_locked"], i == 0 || i == 2);
        assert_eq!(
            row["color_label"],
            if i == 0 || i == 2 { "red" } else { "none" }
        );
    }
    assert_eq!(raw(&s), pixels);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    let after = s.doc.composition();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), after);
    call(&mut s, "history.undo", json!({}));
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    batch(
        &mut s,
        "flip_horizontal",
        json!([all[2], all[0]]),
        json!({}),
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    for (i, (layer, original)) in s.doc.layers.iter().zip(&pixels).enumerate() {
        for y in 0..3 {
            for x in 0..4 {
                assert_eq!(
                    layer.pixels[y * 4 + x],
                    original[y * 4 + if i == 0 || i == 2 { 3 - x } else { x }]
                );
            }
        }
    }
    assert_eq!(selected(&s), json!([all[1], all[3]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[1]);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.doc.composition(), before);
    check(&mut s);
}

#[test]
fn layer_batch_duplicate_delete_and_block_move_use_original_stable_targets() {
    let mut s = source();
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]],"active_layer_id":all[2]}),
    );
    let before = s.doc.composition();
    let pixels = raw(&s);
    batch(&mut s, "duplicate", json!([all[2], all[0]]), json!({}));
    let after = ids(&s);
    assert_eq!(after.len(), 6);
    assert_eq!(
        [after[0], after[2], after[3], after[5]],
        [all[0], all[1], all[2], all[3]]
    );
    assert_ne!(after[1], after[4]);
    assert!(after[1] > all[3] && after[4] > all[3]);
    assert_eq!(s.doc.layers[1].pixels, pixels[0]);
    assert_eq!(s.doc.layers[4].pixels, pixels[2]);
    assert_eq!(selected(&s), json!([after[1], after[4]]));
    assert!([after[1], after[4]].contains(&s.doc.layer_id(s.doc.active)));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(ids(&s), all);
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.doc.composition().layer_metadata, before.layer_metadata);
    assert_eq!(selected(&s), json!([all[0], all[2]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[2]);
    batch(&mut s, "delete", json!([all[0], all[2]]), json!({}));
    assert_eq!(ids(&s), [all[1], all[3]]);
    assert_eq!(s.doc.layers[0].pixels, pixels[1]);
    assert_eq!(s.doc.layers[1].pixels, pixels[3]);
    assert_eq!(selected(&s).as_array().unwrap().len(), 1);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(ids(&s), all);
    assert_eq!(raw(&s), pixels);
    assert_eq!(selected(&s), json!([all[0], all[2]]));
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    batch(&mut s, "move", json!([all[2], all[0]]), json!({"to":1}));
    assert_eq!(ids(&s), [all[1], all[0], all[2], all[3]]);
    assert_eq!(selected(&s), json!([all[0], all[2]]));
    assert_eq!(s.doc.layer_id(s.doc.active), all[2]);
    for (i, id) in all.iter().enumerate() {
        let moved = s.doc.layer_index(*id).unwrap();
        assert_eq!(s.doc.layers[moved].pixels.as_ptr(), pointers[i]);
    }
    call(&mut s, "history.undo", json!({}));
    assert_eq!(ids(&s), all);
    assert_eq!(raw(&s), pixels);
    assert_eq!(selected(&s), json!([all[0], all[2]]));
    check(&mut s);
}

#[test]
fn layer_batch_block_move_matches_independent_subset_insertion_oracle() {
    let mut s = session();
    for count in 1..=5 {
        for mask in 1usize..1usize << count {
            let targets: Vec<_> = (0..count).filter(|i| mask & (1 << i) != 0).collect();
            for destination in 0..=count - targets.len() {
                call(&mut s, "canvas.new", json!({"width":2,"height":1}));
                for layer in 0..count {
                    if layer != 0 {
                        call(&mut s, "layer.add", json!({}));
                    }
                    s.doc.replace_region(
                        0,
                        0,
                        2,
                        1,
                        &[0xff000000 | layer as u32, 0xff110000 | layer as u32],
                    );
                }
                s.replay_boundary().unwrap();
                let original = ids(&s);
                let pixels = raw(&s);
                let canonical: Vec<_> = targets.iter().map(|&i| original[i]).collect();
                let reversed: Vec<_> = canonical.iter().rev().copied().collect();
                call(&mut s, "layer.selection.set", json!({"layer_ids":reversed}));
                let active = s.doc.layer_id(s.doc.active);
                let remaining: Vec<_> = original
                    .iter()
                    .copied()
                    .filter(|id| !canonical.contains(id))
                    .collect();
                let expected: Vec<_> = remaining[..destination]
                    .iter()
                    .chain(&canonical)
                    .chain(&remaining[destination..])
                    .copied()
                    .collect();
                batch(&mut s, "move", json!(reversed), json!({"to":destination}));
                assert_eq!(
                    ids(&s),
                    expected,
                    "count={count}, targets={canonical:?}, to={destination}"
                );
                assert_eq!(selected(&s), json!(canonical));
                assert_eq!(s.doc.layer_id(s.doc.active), active);
                for (index, id) in original.iter().enumerate() {
                    assert_eq!(
                        s.doc.layers[s.doc.layer_index(*id).unwrap()].pixels,
                        pixels[index]
                    );
                }
                // This noncontiguous case caught gathering/placement algorithms
                // that leave an unselected layer inside the requested block.
                if count == 5 && targets == [0, 3, 4] && destination == 1 {
                    call(&mut s, "history.undo", json!({}));
                    assert_eq!(ids(&s), original);
                    assert_eq!(raw(&s), pixels);
                    assert_eq!(selected(&s), json!(canonical));
                    assert_eq!(s.doc.layer_id(s.doc.active), active);
                    call(&mut s, "history.redo", json!({}));
                    assert_eq!(ids(&s), expected);
                    assert_eq!(selected(&s), json!(canonical));
                    assert_eq!(s.doc.layer_id(s.doc.active), active);
                }
                check(&mut s);
            }
        }
    }
}

#[test]
fn layer_batch_preflights_all_locks_masks_arguments_and_capacity_before_publication() {
    let mut s = source();
    let all = ids(&s);
    s.doc
        .set_selection(SelectionState {
            current: Some(
                Mask::new(
                    4,
                    3,
                    vec![0, 64, 128, 255, 255, 128, 64, 0, 0, 64, 128, 255],
                )
                .unwrap(),
            ),
            previous: Some(Mask::new(4, 3, vec![255; 12]).unwrap()),
        })
        .unwrap();
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":all[2],"pixel_locked":true}),
    );
    let before = s.state();
    let composition = s.doc.composition();
    let pixels = raw(&s);
    let history = s.doc.history_bytes();
    for (action, args) in [
        ("clear", json!({})),
        ("flip_horizontal", json!({})),
        ("translate", json!({"dx":1,"dy":0})),
    ] {
        let error = s
            .apply(Action::new(
                "layer.batch",
                json!({"action":format!("layer.{action}"),"layer_ids":[all[0],all[2]],"args":args}),
            ))
            .unwrap_err();
        assert_eq!(error.code, "pixel_locked");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), pixels);
        assert_eq!(s.doc.history_bytes(), history);
    }
    for args in [
        json!({"action":"layer.clear","layer_ids":[]}),
        json!({"action":"layer.clear","layer_ids":[all[0],all[0]]}),
        json!({"action":"layer.clear","layer_ids":[all[0],99999]}),
        json!({"action":"layer.delete","layer_ids":all}),
        json!({"action":"layer.move","layer_ids":[all[0],all[2]],"args":{"to":3}}),
        json!({"action":"layer.set","layer_ids":[all[0]],"args":{"index":1,"color_label":"red"}}),
        json!({"action":"layer.clear","layer_ids":[all[0]],"args":{"radius":1}}),
        json!({"action":"rename","layer_ids":[all[0]],"args":{"name":"Unsupported"}}),
    ] {
        assert!(
            s.apply(Action::new("layer.batch", args.clone())).is_err(),
            "{args}"
        );
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), pixels);
        assert_eq!(s.doc.history_bytes(), history);
    }
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":all[2],"pixel_locked":false,"position_locked":true}),
    );
    let before = s.state();
    assert_eq!(
        s.apply(Action::new(
            "layer.batch",
            json!({"action":"layer.translate","layer_ids":[all[0],all[2]],"args":{"dx":1,"dy":0}})
        ))
        .unwrap_err()
        .code,
        "position_locked"
    );
    assert_eq!(s.state(), before);
    let masks = s.doc.selection().clone();
    batch(&mut s, "clear", json!([all[0], all[2]]), json!({}));
    assert_eq!(s.doc.layers[1].pixels, pixels[1]);
    assert_eq!(s.doc.layers[3].pixels, pixels[3]);
    assert_ne!(s.doc.layers[0].pixels, pixels[0]);
    assert_ne!(s.doc.layers[2].pixels, pixels[2]);
    assert_eq!(s.doc.selection(), &masks);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw(&s), pixels);
    // A batch duplicate must reserve space for every original, not duplicate
    // a first source and discover the layer limit at a later source.
    for _ in 4..15 {
        call(&mut s, "layer.add", json!({}));
    }
    let before = s.state();
    let pixels = raw(&s);
    let history = s.doc.history_bytes();
    assert!(s
        .apply(Action::new(
            "layer.batch",
            json!({"action":"layer.duplicate","layer_ids":[all[0],all[2]]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}

#[test]
fn layer_batch_atomic_dynamic_references_and_partial_journal_failure_restore_selection_and_redo() {
    let mut s = source();
    let all = ids(&s);
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]],"active_layer_id":all[2]}),
    );
    batch(
        &mut s,
        "set",
        json!([all[0], all[2]]),
        json!({"color_label":"blue"}),
    );
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let composition = s.doc.composition();
    let pixels = raw(&s);
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"select","action":"layer.selection.set","args":{"layer_ids":[all[1],all[3]],"active_layer_id":all[1]}},
            {"id":"edit","action":"layer.batch","args":{"action":"layer.set","layer_ids":[all[0],all[2]],"args":{"color_label":"red"}}},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "layer.batch",
            json!({"action":"layer.delete","layer_ids":[all[0],all[2]]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    // Exercise dynamic IDs on a healthy session; the queried selection is a
    // snapshot, independently of which layer becomes active during execution.
    let mut s = source();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]]}),
    );
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"find","action":"layer.query","args":{"selected":true}},
            {"id":"edit","action":"layer.batch","args":{"action":"layer.set","layer_ids":{"$ref":"find#/result/selected_layer_ids"},"args":{"color_label":"green"}}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(s.state()["layers"][0]["color_label"], "green");
    assert_eq!(s.state()["layers"][2]["color_label"], "green");
    check(&mut s);
}

#[test]
fn layer_batch_schema_workflow_projects_replay_and_malformed_metadata_preserve_identity_contracts()
{
    let mut s = source();
    let all = ids(&s);
    assert!(s.registry.actions.contains_key("layer.selection.set"));
    assert!(s.registry.actions.contains_key("layer.batch"));
    assert_eq!(
        s.registry.actions["layer.query"].schema()["properties"]["selected"]["type"],
        "boolean"
    );
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(
        catalog.contains("action.layer.selection.set") && catalog.contains("action.layer.batch")
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[all[0],all[2]],"active_layer_id":all[2]}),
    );
    let pixels = raw(&s);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.layer.query","position":[0,0],"parameters":{"selected":true}},
        {"id":2,"type":"action.layer.batch","position":[400,0],"parameters":{"action":"layer.set","args":{"color_label":"violet"}}}
    ],"edges":[{"from":1,"output":"selected_layer_ids","to":2,"input":"layer_ids"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(raw(&s), pixels);
    assert_eq!(s.state()["layers"][0]["color_label"], "none");
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(s.state()["layers"][0]["color_label"], "none");
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.state()["layers"][0]["color_label"], "violet");
    let path = output("batch.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let state = s.state();
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    assert_eq!(selected(&reopened), json!([all[0], all[2]]));
    assert_eq!(reopened.doc.layer_id(reopened.doc.active), all[2]);
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), png);
    check(&mut reopened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    assert_eq!(
        journal.preview(last).unwrap().metadata.composition,
        composition
    );
    assert_eq!(replay::png(&journal.preview(last).unwrap()).unwrap(), png);
    for invalid in [
        json!([all[0], all[0]]),
        json!([99999]),
        json!([0]),
        json!([all[0], all[1]]),
    ] {
        let mut value = serde_json::to_value(s.replay_metadata()).unwrap();
        value["composition"]["selected_layer_ids"] = invalid.clone();
        let metadata: Metadata = serde_json::from_value(value).unwrap();
        assert!(metadata.validate().is_err(), "{invalid}");
        let stored = std::fs::read(&path).unwrap();
        let offset = 24 + (2 + 4 * 3 * 4) * 4;
        let length = u32::from_le_bytes(stored[offset..offset + 4].try_into().unwrap()) as usize;
        let mut metadata: Value =
            serde_json::from_slice(&stored[offset + 4..offset + 4 + length]).unwrap();
        metadata["selected_layer_ids"] = invalid;
        let encoded = serde_json::to_vec(&metadata).unwrap();
        let mut malformed = stored[..offset].to_vec();
        malformed[..8].copy_from_slice(b"MICOPI02");
        malformed.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
        malformed.extend_from_slice(&encoded);
        let bad = output("malformed.mcp");
        std::fs::write(&bad, malformed).unwrap();
        assert!(s
            .apply(Action::new("document.open", json!({"path":bad})))
            .is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(raw(&s), pixels);
        std::fs::remove_file(bad).unwrap();
    }
    let mut legacy =
        Journal::import(include_bytes!("../../tests/fixtures/replay/v1.mcreplay")).unwrap();
    let head = legacy.status()["head_hash"].clone();
    assert_eq!(legacy.verify(None).unwrap()["head_hash"], head);
    std::fs::remove_file(path).unwrap();
}
