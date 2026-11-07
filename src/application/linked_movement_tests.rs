//! Shared link metadata and exact multi-owner movement through real DLL sampling.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    raster_masks::SamplePlane,
    replay::Journal,
    selection::{Mask, SelectionState},
    Document,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const W: usize = 7;
const H: usize = 5;
fn pixels(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn masks(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            let plane = s.doc.mask_plane(m.id).unwrap();
            (m.id, (0..W * H).map(|i| plane.sample(i)).collect())
        })
        .collect()
}
#[derive(Debug, PartialEq)]
struct Content {
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
    active: usize,
}
fn content(s: &Session) -> Content {
    Content {
        composition: s.doc.composition(),
        pixels: pixels(s),
        masks: masks(s),
        active: s.doc.active,
    }
}
fn fixture(masked: bool) -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    for _ in 1..4 {
        call(&mut s, "layer.add", json!({}));
    }
    for (layer, raster) in s.doc.layers.iter_mut().enumerate() {
        for (i, pixel) in raster.pixels.iter_mut().enumerate() {
            *pixel = ([0, 61, 128, 219, 255][(i + layer) % 5] as u32) << 24
                | ((i as u32 * 7919 + layer as u32 * 3571 + 19) & 0xffffff);
        }
        s.doc.replay_tiles.insert((layer, 0));
    }
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    if masked {
        for owner in 1..=3 {
            let id = call(
                &mut s,
                "mask.create",
                json!({"owner":{"kind":"layer","id":owner},"select":false}),
            )["mask_id"]
                .as_u64()
                .unwrap();
            s.doc
                .replace_mask_plane(
                    id,
                    SamplePlane::Dense(
                        (0..W * H)
                            .map(|i| (i * 37 + owner as usize * 19) as u8)
                            .collect(),
                    ),
                )
                .unwrap();
        }
        call(&mut s, "mask.set", json!({"mask_id":1,"enabled":false}));
        call(&mut s, "mask.set", json!({"mask_id":3,"linked":false}));
        call(
            &mut s,
            "mask.target",
            json!({"target":{"kind":"mask","id":2}}),
        );
    }
    s.doc
        .set_selection(SelectionState {
            current: Some(
                Mask::new(
                    W,
                    H,
                    (0..W * H)
                        .map(|i| if i % 3 == 0 { 128 } else { 0 })
                        .collect(),
                )
                .unwrap(),
            ),
            previous: Some(
                Mask::new(
                    W,
                    H,
                    (0..W * H)
                        .map(|i| if i % 2 == 0 { 79 } else { 255 })
                        .collect(),
                )
                .unwrap(),
            ),
        })
        .unwrap();
    call(&mut s, "layer.selection.set", json!({"layer_ids":[2,4]}));
    call(
        &mut s,
        "layer.links.set",
        json!({"layer_ids":[3,1],"linked":true}),
    );
    s.replay_boundary().unwrap();
    s.doc.compose();
    s
}
fn shifted<T: Copy>(src: &[T], dx: i32, dy: i32, fill: T) -> Vec<T> {
    (0..W * H)
        .map(|i| {
            let x = (i % W) as i32 - dx;
            let y = (i / W) as i32 - dy;
            if x >= 0 && y >= 0 && x < W as i32 && y < H as i32 {
                src[y as usize * W + x as usize]
            } else {
                fill
            }
        })
        .collect()
}
fn verify_outcome(s: &mut Session) {
    check(s);
    let n = s.journal.as_ref().unwrap().entries.len();
    let snapshot = s.replay_preview(n - 1, false).unwrap();
    assert!(snapshot.matches_document(&s.doc));
    let mut restored = Document::new(1, 1);
    snapshot.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, raw) in masks(s) {
        let view = restored.mask_plane(id).unwrap();
        assert_eq!((0..W * H).map(|i| view.sample(i)).collect::<Vec<_>>(), raw);
    }
}

#[test]
fn linked_movement_metadata_has_shared_commands_public_state_and_inert_typed_nodes() {
    let mut s = fixture(true);
    let before = content(&s);
    let revision = s.revision;
    let count = s.journal.as_ref().unwrap().entries.len();
    let query = call(&mut s, "layer.links.query", json!({}));
    assert_eq!(query["layer_links"], json!([[1, 3]]));
    assert_eq!(query["active_linked_layer_ids"], json!([4]));
    assert_eq!(s.state()["layers"][0]["linked_layer_ids"], json!([1, 3]));
    assert_eq!(s.state()["layers"][1]["linked_layer_ids"], json!([]));
    assert_eq!(
        call(&mut s, "layer.query", json!({}))["layer_links"],
        query["layer_links"]
    );
    assert_eq!(s.revision, revision);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(content(&s), before);
    let catalog = call(&mut s, "node.catalog", json!({}));
    for id in [
        "action.layer.links.set",
        "action.layer.links.query",
        "state.layers.links",
    ] {
        assert!(catalog["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == id));
    }
    call(&mut s, "layer.select", json!({"layer_id":1}));
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"state.layers.links","position":[0,0],"parameters":{}},
        {"id":2,"type":"action.canvas.move","position":[420,0],"parameters":{"dx":1,"dy":0,"target":"layer"}}
    ],"edges":[{"from":1,"output":"active_linked_layer_ids","to":2,"input":"layer_ids"}]});
    let raw = pixels(&s);
    call(&mut s, "workflow.set", json!({"graph":graph}));
    call(&mut s, "workflow.validate", json!({}));
    assert_eq!(
        pixels(&s),
        raw,
        "Editing a workflow must not execute movement"
    );
    call(&mut s, "workflow.run", json!({}));
    assert_eq!(s.doc.layers[0].pixels, shifted(&raw[0], 1, 0, 0));
    assert_eq!(s.doc.layers[2].pixels, shifted(&raw[2], 1, 0, 0));
    verify_outcome(&mut s);
}

#[test]
fn linked_movement_explicit_ids_move_raw_pairs_once_without_changing_selection_or_targets() {
    for masked in [false, true] {
        let mut s = fixture(masked);
        let before = content(&s);
        let history = s.doc.history_bytes();
        let start = s.journal.as_ref().unwrap().entries.len();
        let revision = s.revision;
        let result = s
            .apply(Action::new(
                "canvas.move",
                json!({"layer_ids":[3.0,1.0],"dx":2,"dy":-1}),
            ))
            .unwrap();
        assert_eq!(result["revision"], result["result"]["revision"]);
        assert_eq!(s.revision, revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), start + 1);
        for i in 0..4 {
            assert_eq!(
                s.doc.layers[i].pixels,
                if i == 0 || i == 2 {
                    shifted(&before.pixels[i], 2, -1, 0)
                } else {
                    before.pixels[i].clone()
                }
            );
        }
        let raw = masks(&s);
        for (id, source) in &before.masks {
            assert_eq!(
                raw[id],
                if *id == 1 {
                    shifted(source, 2, -1, 255)
                } else {
                    source.clone()
                }
            );
        }
        assert_eq!(s.doc.composition(), before.composition);
        assert_eq!(s.doc.active, before.active);
        let event = s.journal.as_mut().unwrap().events(start, 1).unwrap()["events"][0].clone();
        assert_eq!(
            event["source"]["context"]["movement"],
            json!({"target":"layers","layer_ids":[3,1],"linked_mask_ids":if masked{vec![1]}else{vec![]},"dx":2,"dy":-1})
        );
        let moved = content(&s);
        assert!(s.doc.history_bytes() > history);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(
            content(&s),
            before,
            "One Undo restores every addressed raw domain"
        );
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), moved);
        verify_outcome(&mut s);
    }
}

#[test]
fn linked_movement_explicit_subset_and_nudge_override_preferences_without_expanding_links() {
    let mut s = fixture(true);
    s.apply_movement_defaults(MovementPreferences {
        target: MoveTarget::SelectionMask,
        step: 2,
        large_step: 3,
    });
    let before = content(&s);
    call(
        &mut s,
        "canvas.nudge",
        json!({"layer_ids":[3],"direction":"left","large":true}),
    );
    assert_eq!(s.doc.layers[2].pixels, shifted(&before.pixels[2], -3, 0, 0));
    assert_eq!(
        s.doc.layers[0].pixels, before.pixels[0],
        "Explicit IDs never expand persistent relationships"
    );
    assert_eq!(masks(&s), before.masks, "Unlinked mask stays fixed");
    assert_eq!(s.doc.composition(), before.composition);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), before);
    let history = s.doc.history_bytes();
    call(
        &mut s,
        "canvas.move",
        json!({"layer_ids":[1,3],"dx":0,"dy":0}),
    );
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(content(&s), before);
}

fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 1. + (time - 100) as f32 * 0.02,
        y: 1.,
        pressure: 0.3 + (time - 100) as f32 * 0.005,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(kind),
        view_x: Some(40. + time as f32),
        view_y: Some(40.),
    }
}
fn samples(s: &mut Session) -> Vec<Value> {
    let values = s.journal.as_mut().unwrap().events(0, 256).unwrap();
    let mut samples: Vec<_> = values["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    samples.extend(
        s.replay_samples
            .iter()
            .map(|sample| serde_json::to_value(sample).unwrap()),
    );
    samples
}

#[test]
fn linked_movement_and_link_metadata_preserve_live_brush_and_hand_phase_pressure_streams() {
    for masked in [false, true] {
        for kind in [ToolKind::Brush, ToolKind::Hand] {
            let mut s = fixture(masked);
            call(
                &mut s,
                "brush.set",
                json!({"radius":0.5,"stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
            );
            s.pointer(pointer(kind, PointerPhase::Down, 100)).unwrap();
            s.replay_boundary().unwrap();
            s.pointer(pointer(kind, PointerPhase::Move, 120)).unwrap();
            s.doc.compose();
            for (action, args) in [
                ("canvas.move", json!({"layer_ids":[1,3],"dx":1,"dy":0})),
                ("canvas.nudge", json!({"layer_ids":[3],"direction":"right"})),
                ("layer.links.set", json!({"layer_ids":[2,4],"linked":true})),
            ] {
                let before = content(&s);
                let revision = s.revision;
                let history = s.doc.history_bytes();
                let stream = samples(&mut s);
                assert_eq!(
                    s.apply(Action::new(action, args)).unwrap_err().code,
                    "stroke_active"
                );
                assert_eq!(content(&s), before);
                assert_eq!(s.revision, revision);
                assert_eq!(s.doc.history_bytes(), history);
                assert_eq!(samples(&mut s), stream);
                assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            }
            s.pointer(pointer(kind, PointerPhase::Move, 140)).unwrap();
            s.pointer(pointer(kind, PointerPhase::Up, 160)).unwrap();
            assert_eq!(
                samples(&mut s),
                [
                    PointerPhase::Down,
                    PointerPhase::Move,
                    PointerPhase::Move,
                    PointerPhase::Up
                ]
                .into_iter()
                .zip([100, 120, 140, 160])
                .map(|(phase, time)| serde_json::to_value(pointer(kind, phase, time)).unwrap())
                .collect::<Vec<_>>()
            );
            verify_outcome(&mut s);
        }
    }
}

#[test]
fn linked_movement_literal_conflicts_precede_macro_prefix_and_every_owner_is_preflighted() {
    let mut s = fixture(true);
    for args in [
        json!({"layer_ids":[1,1],"dx":1,"dy":0}),
        json!({"layer_ids":[1,3],"mask_id":1,"dx":1,"dy":0}),
        json!({"layer_ids":[1,3],"target":"selection_mask","dx":1,"dy":0}),
    ] {
        let descriptor=serde_json::from_value(json!({"id":"test.invalid_links","title":"Invalid linked movement",
            "steps":[{"action":"layer.rename","args":{"name":"must not run"}}, {"action":"canvas.move","args":args}]})).unwrap();
        s.registry
            .actions
            .insert("test.invalid_links".into(), descriptor);
        let before = content(&s);
        assert_eq!(
            s.apply(Action::new("test.invalid_links", json!({})))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(content(&s), before);
    }
    for property in ["pixel_locked", "position_locked", "alpha_locked"] {
        call(&mut s, "layer.set", json!({"layer_id":3,(property):true}));
        let before = content(&s);
        let revision = s.revision;
        assert!(s
            .apply(Action::new(
                "canvas.move",
                json!({"layer_ids":[1,3],"dx":1,"dy":0})
            ))
            .is_err());
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        call(&mut s, "layer.set", json!({"layer_id":3,(property):false}));
    }
    let before = content(&s);
    assert!(s
        .apply(Action::new(
            "canvas.move",
            json!({"layer_ids":[1,99],"dx":1,"dy":0})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    verify_outcome(&mut s);
}

static MOVE_CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn fail_second_move(
    _: *const abi::Move,
    _: *const abi::Pixels,
    _: *const abi::Coverage,
    output: *mut abi::Pixels,
) -> i32 {
    if MOVE_CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        std::ptr::write_bytes((*output).data, 0, (*output).len);
        abi::OK
    } else {
        abi::ERROR
    }
}
#[test]
fn linked_movement_late_native_failure_atomic_rollback_and_journal_failure_publish_no_subset() {
    let mut s = fixture(true);
    let before = content(&s);
    let history = s.doc.history_bytes();
    let public = s.state();
    let original = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_movement_pixels_for_test(Some(fail_second_move));
    assert!(original.is_some());
    MOVE_CALLS.store(0, Ordering::SeqCst);
    assert!(s
        .apply(Action::new(
            "canvas.move",
            json!({"layer_ids":[1,3],"dx":1,"dy":0})
        ))
        .is_err());
    assert_eq!(MOVE_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_movement_pixels_for_test(original);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"link","action":"layer.links.set","args":{"layer_ids":[3,4],"linked":true}},
            {"id":"move","action":"canvas.move","args":{"layer_ids":[1,3,4],"dx":1,"dy":0}},
            {"id":"fail","action":"layer.move","args":{"to":15}}
        ]}),
    );
    assert_eq!(result["failed_step"], "fail");
    assert_eq!(result["completed"], json!(["link", "move"]));
    assert_eq!(result["rolled_back"], true);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.move",
            json!({"layer_ids":[1,3],"dx":1,"dy":0})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert!(s.journal.as_mut().unwrap().verify(None).unwrap()["valid"]
        .as_bool()
        .unwrap());
}

#[test]
fn linked_movement_projects_and_verified_archives_restore_relationships_without_dlls() {
    let mut s = fixture(true);
    call(
        &mut s,
        "canvas.move",
        json!({"layer_ids":[1,3],"dx":-1,"dy":2}),
    );
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/linked-movement");
    std::fs::create_dir_all(&output).unwrap();
    let project = output.join("roundtrip.mcp");
    let archive = output.join("roundtrip.mcr");
    call(&mut s, "document.save", json!({"path":project}));
    let before = content(&s);
    call(&mut s, "replay.export", json!({"path":archive}));
    call(&mut s, "canvas.new", json!({"width":2,"height":2}));
    call(&mut s, "document.open", json!({"path":project}));
    assert_eq!(content(&s), before);
    let mut journal = Journal::import(&std::fs::read(&archive).unwrap()).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.layer_link_sets(), vec![vec![1, 3]]);
    verify_outcome(&mut s);
}
