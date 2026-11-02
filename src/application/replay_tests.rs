use super::*;
use crate::{engine::replay::Journal, plugins::Registry};
use serde_json::{json, Value};
use std::path::PathBuf;
fn session() -> Session {
    let registry =
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap();
    assert!(
        registry.diagnostics.is_empty(),
        "{:?}",
        registry.diagnostics
    );
    let mut s = Session::new(registry).unwrap();
    call(&mut s, "canvas.new", json!({"width":128,"height":96}));
    s
}
fn call(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()["result"].clone()
}
fn output(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/replay");
    std::fs::create_dir_all(&path).unwrap();
    path.join(format!("{}-{name}", std::process::id()))
}
fn paint(s: &mut Session) {
    call(
        s,
        "brush.set",
        json!({"radius":6,"color":"#4477dd","stabilizer":0.35,"pressure_size":true}),
    );
    call(
        s,
        "canvas.stroke",
        json!({"points":[{"x":8,"y":20,"pen":true,"pressure":0.2,"time":0},{"x":60,"y":70,"pen":true,"pressure":1.,"time":30},{"x":110,"y":15,"pen":true,"pressure":0.4,"time":60}]}),
    );
}
#[test]
fn exact_outcomes_cover_native_plugins_pressure_undo_and_redo_without_reexecution() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":8}));
    paint(&mut s);
    let painted = codec::png(&mut s.doc).unwrap();
    call(&mut s, "history.undo", json!({}));
    let undone = codec::png(&mut s.doc).unwrap();
    assert_ne!(painted, undone);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(painted, codec::png(&mut s.doc).unwrap());
    let events = s.journal.as_ref().unwrap().entries.len();
    let before = s.revision;
    let calls = call(&mut s, "native.stats", json!({}))["calls"].clone();
    let status = call(&mut s, "replay.verify", json!({}));
    assert_eq!(status["valid"], true);
    let frame = s.replay_preview(events - 1, false).unwrap();
    assert_eq!(painted, replay::png(&frame).unwrap());
    for index in (0..events).rev() {
        s.replay_preview(index, false).unwrap();
    }
    assert_eq!(s.revision, before);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    // The statistics query itself increments the fixture's invocation counter.
    assert_eq!(
        call(&mut s, "native.stats", json!({}))["calls"].as_u64(),
        calls.as_u64().map(|n| n + 1)
    );
}
#[test]
fn pointer_samples_are_buffered_without_serializing_or_hashing_every_move() {
    let mut s = session();
    call(&mut s, "brush.set", json!({"radius":5,"stabilizer":0.7}));
    let count = s.journal.as_ref().unwrap().entries.len();
    let mut p = Pointer {
        phase: PointerPhase::Down,
        x: 8.,
        y: 20.,
        pressure: 0.3,
        time: 0,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    for n in 0..50 {
        p.phase = PointerPhase::Move;
        p.x = 8. + n as f32 * 2.;
        p.y = 30. + (n as f32 * 0.2).sin() * 15.;
        p.time = n * 8;
        p.pressure = 0.2 + n as f32 / 100.;
        s.pointer(p).unwrap();
    }
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(s.replay_samples.len(), 51);
    let pending = call(&mut s, "replay.verify", json!({}));
    assert_eq!(pending["pending_samples"], 51);
    assert_eq!(pending["stroke_in_progress"], true);
    p.phase = PointerPhase::Up;
    s.pointer(p).unwrap();
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    assert!(s.replay_samples.is_empty());
    let event = call(&mut s, "replay.events", json!({"start":count}));
    assert_eq!(
        event["events"][0]["source"]["samples"]
            .as_array()
            .unwrap()
            .len(),
        52
    );
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}
#[test]
fn archives_reject_payload_tampering_truncation_missing_events_and_wrong_external_heads() {
    let mut s = session();
    paint(&mut s);
    let mut bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let head = call(&mut s, "replay.status", json!({}))["head_hash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        Journal::import(&bytes)
            .unwrap()
            .verify(Some(&head))
            .unwrap()["valid"],
        true
    );
    assert!(Journal::import(&bytes[..bytes.len() - 1]).is_err());
    let footer = bytes.len() - 44;
    let first = 8 + 36 + u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let mut removed = bytes[..first].to_vec();
    removed.extend_from_slice(&bytes[footer..]);
    assert!(Journal::import(&removed).is_err());
    bytes[48] ^= 1;
    assert!(Journal::import(&bytes).is_err());
    assert!(s
        .apply(Action::new(
            "replay.verify",
            json!({"expected_head":"0".repeat(64)})
        ))
        .is_err());
}
#[test]
fn project_save_embeds_verified_replay_and_reopening_continues_the_same_history() {
    let mut s = session();
    paint(&mut s);
    let path = output("project.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..8], b"MICOPI03");
    let count = s.journal.as_ref().unwrap().entries.len();
    let pixels = codec::png(&mut s.doc).unwrap();
    let mut second = session();
    call(&mut second, "document.open", json!({"path":path}));
    assert!(second.journal.as_ref().unwrap().entries.len() > count);
    assert_eq!(codec::png(&mut second.doc).unwrap(), pixels);
    assert_eq!(call(&mut second, "replay.verify", json!({}))["valid"], true);
    paint(&mut second);
    assert_eq!(call(&mut second, "replay.verify", json!({}))["valid"], true);
    let mut corrupt = bytes;
    corrupt[26] ^= 1; // First layer pixel; embedded journal remains untouched.
    let bad = output("mismatched-project.mcp");
    std::fs::write(&bad, corrupt).unwrap();
    let before = codec::png(&mut second.doc).unwrap();
    assert!(second
        .apply(Action::new("document.open", json!({"path":bad})))
        .is_err());
    assert_eq!(codec::png(&mut second.doc).unwrap(), before);
}
#[test]
fn playback_does_not_repeat_file_writes_and_load_is_atomic_on_corrupt_input() {
    let mut s = session();
    paint(&mut s);
    let project = output("side-effects.mcp");
    call(&mut s, "document.save", json!({"path":project}));
    let archive = output("outcome.mcreplay");
    call(&mut s, "replay.export", json!({"path":archive}));
    std::fs::remove_file(&project).unwrap();
    let before = codec::png(&mut s.doc).unwrap();
    call(&mut s, "replay.load", json!({"path":archive}));
    let count = s.loaded_replay.as_ref().unwrap().entries.len();
    for index in 0..count {
        call(
            &mut s,
            "replay.seek",
            json!({"event":index,"source":"loaded"}),
        );
    }
    assert!(!project.exists());
    assert_eq!(codec::png(&mut s.doc).unwrap(), before);
    let old_head = s.loaded_replay.as_ref().unwrap().status()["head_hash"].clone();
    let bad = output("corrupt.mcreplay");
    std::fs::write(&bad, b"MCRPLY01bad").unwrap();
    assert!(s
        .apply(Action::new("replay.load", json!({"path":bad})))
        .is_err());
    assert_eq!(
        s.loaded_replay.as_ref().unwrap().status()["head_hash"],
        old_head
    );
}
#[test]
fn failed_and_nested_operations_keep_a_verifiable_partial_history() {
    let mut s = session();
    assert!(s
        .apply(Action::new("brush.set", json!({"radius":-1})))
        .is_err());
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"steps":[{"id":"paint","action":"native.checkerboard","args":{"cell":8}},{"id":"fail","action":"layer.select","args":{"index":12}}]}),
    );
    assert_eq!(result["ok"], false);
    let events = call(&mut s, "replay.events", json!({}));
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["success"] == false));
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}
#[test]
fn failed_project_save_restores_dirty_and_file_state_and_keeps_the_existing_project() {
    let mut s = session();
    paint(&mut s);
    let before = s.file.clone();
    let path = output("missing-parent").join("project.mcp");
    assert!(s
        .apply(Action::new("document.save", json!({"path":path})))
        .is_err());
    assert_eq!(s.file, before);
    assert!(s.doc.dirty);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn long_contact_segments_keep_repeatedly_touched_tiles_and_pressure_samples() {
    let mut s = session();
    let mut p = Pointer {
        phase: PointerPhase::Down,
        x: 12.,
        y: 20.,
        pressure: 0.2,
        time: 0,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    for i in 0..600 {
        p.phase = PointerPhase::Move;
        p.x = 12. + (i % 90) as f32;
        p.y = 20. + (i % 50) as f32;
        p.time = i;
        p.pressure = 0.2 + (i % 70) as f32 / 100.;
        s.pointer(p).unwrap();
    }
    p.phase = PointerPhase::Up;
    s.pointer(p).unwrap();
    let last = s.journal.as_ref().unwrap().entries.len() - 1;
    let expected = codec::png(&mut s.doc).unwrap();
    assert_eq!(
        replay::png(&s.replay_preview(last, false).unwrap()).unwrap(),
        expected
    );
    let events = call(&mut s, "replay.events", json!({}));
    let samples: usize = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["source"]["samples"].as_array())
        .map(Vec::len)
        .sum();
    assert_eq!(samples, 602);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}
#[test]
fn fresh_documents_have_independent_histories_and_public_queries_do_not_add_events() {
    let mut s = session();
    paint(&mut s);
    let old = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    call(&mut s, "canvas.new", json!({"width":24,"height":24}));
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), 2);
    assert_ne!(s.journal.as_ref().unwrap().status()["head_hash"], old);
    let count = s.journal.as_ref().unwrap().entries.len();
    for id in [
        "canvas.state",
        "replay.status",
        "replay.open",
        "plugins.list",
        "app.describe",
    ] {
        call(&mut s, id, json!({}));
    }
    call(
        &mut s,
        "context.read",
        json!({"resources":["replay.recording"]}),
    );
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
}

#[test]
fn recording_fault_blocks_mutations_but_preserves_canvas_and_prefix_recovery() {
    let mut s = session();
    paint(&mut s);
    let expected = codec::png(&mut s.doc).unwrap();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.replay_error = Some("Injected journal write failure".into());
    assert!(s.apply(Action::new("layer.clear", json!({}))).is_err());
    assert_eq!(codec::png(&mut s.doc).unwrap(), expected);
    assert_eq!(call(&mut s, "replay.status", json!({}))["recording"], false);
    let png = output("recovered.png");
    call(&mut s, "replay.recover_png", json!({"path":png}));
    assert_eq!(std::fs::read(png).unwrap(), expected);
    let archive = output("recovered-prefix.mcreplay");
    call(&mut s, "replay.export", json!({"path":archive}));
    assert_eq!(
        Journal::from_path(&archive).unwrap().status()["head_hash"],
        head
    );
}

#[test]
fn atomic_group_failure_restores_pixels_metadata_revision_and_redo_with_one_event() {
    let mut s = session();
    paint(&mut s);
    let painted = codec::png(&mut s.doc).unwrap();
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let pixels = codec::png(&mut s.doc).unwrap();
    let history = s.doc.history_bytes();
    let count = s.journal.as_ref().unwrap().entries.len();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"paint","action":"native.checkerboard","args":{"cell":8}},
            {"id":"brush","action":"brush.set","args":{"color":"#ffcc00"}},
            {"id":"layer","action":"layer.add"},
            {"id":"fail","action":"layer.select","args":{"index":12}}
        ]}),
    );
    assert_eq!(r["ok"], false);
    assert_eq!(r["rolled_back"], true);
    assert_eq!(r["transactional"], true);
    assert_eq!(r["committed"], json!([]));
    assert_eq!(s.state(), before);
    assert_eq!(codec::png(&mut s.doc).unwrap(), pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    let e = call(&mut s, "replay.events", json!({"start":count}));
    assert_eq!(e["events"][0]["before"], e["events"][0]["after"]);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(codec::png(&mut s.doc).unwrap(), painted);
    assert_eq!(s.atomic_depth, 0);
}

#[test]
fn atomic_nested_success_records_one_group_and_output_failure_rolls_it_back() {
    let mut s = session();
    let count = s.journal.as_ref().unwrap().entries.len();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"nested","action":"pipeline.run","args":{"steps":[{"id":"brush","action":"brush.set","args":{"radius":12}}]}},
            {"id":"draw","action":"canvas.stroke","args":{"points":[{"x":12,"y":12},{"x":70,"y":60}]}}
        ]}),
    );
    assert_eq!(r["ok"], true);
    assert_eq!(r["transactional"], true);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    let expected = s.state();
    let png = codec::png(&mut s.doc).unwrap();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"clear","action":"layer.clear","output_schema":{"type":"string"}}
        ]}),
    );
    assert_eq!(r["ok"], false);
    assert_eq!(r["rolled_back"], true);
    assert_eq!(s.state(), expected);
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn atomic_groups_reject_external_effects_without_writing_files() {
    let mut s = session();
    let before = s.state();
    let path = output("forbidden-export.png");
    for dry in [false, true] {
        let error = s
            .apply(Action::new(
                "pipeline.run",
                json!({"atomic":true,"dry_run":dry,"steps":[
                    {"id":"brush","action":"brush.set","args":{"radius":17}},
                    {"id":"external","action":"canvas.export","args":{"path":path}}
                ]}),
            ))
            .unwrap_err();
        assert_eq!(error.code, "non_atomic_action");
    }
    assert!(!path.exists());
    assert_eq!(s.state(), before);
}

#[test]
fn partial_journal_write_rolls_back_action_and_preserves_a_valid_retryable_prefix() {
    let mut s = session();
    paint(&mut s);
    let before = s.state();
    let png = codec::png(&mut s.doc).unwrap();
    let bytes = s.journal.as_ref().unwrap().status()["bytes"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s.apply(Action::new("layer.clear", json!({}))).is_err());
    assert_eq!(s.state(), before);
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
    assert_eq!(s.journal.as_ref().unwrap().status()["bytes"], bytes);
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    assert!(Journal::import(&archive).is_ok());
    s.replay_error = None; // Simulate a recovered storage device; production stays faulted.
    call(&mut s, "brush.set", json!({"radius":9}));
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
    call(&mut s, "history.undo", json!({}));
    assert_ne!(codec::png(&mut s.doc).unwrap(), png);
}

#[test]
fn failed_journal_reservation_never_replaces_an_existing_project() {
    let mut s = session();
    let path = output("atomic-save.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let old = std::fs::read(&path).unwrap();
    paint(&mut s);
    let before = s.state();
    s.journal.as_mut().unwrap().fail_next_write_after(11);
    assert!(s
        .apply(Action::new("document.save", json!({"path":path})))
        .is_err());
    assert_eq!(std::fs::read(path).unwrap(), old);
    assert_eq!(s.state(), before);
}

#[test]
fn unchanged_large_metadata_is_not_repeated_in_every_event() {
    let mut s = session();
    let mut seed = 7u32;
    let text: String = (0..16000)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            format!("{seed:08x}")
        })
        .collect();
    s.ui = json!({"large":text});
    s.replay_boundary().unwrap();
    let start = s.journal.as_ref().unwrap().status()["bytes"]
        .as_u64()
        .unwrap();
    for i in 0..64 {
        call(&mut s, "view.set", json!({"pan_x":i}));
    }
    let growth = s.journal.as_ref().unwrap().status()["bytes"]
        .as_u64()
        .unwrap()
        - start;
    assert!(growth < 40_000, "metadata events grew by {growth} bytes");
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn noisy_background_uses_tile_references_and_sparse_xor_deltas() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":128,"height":64}));
    let mut seed = 1u32;
    for p in &mut s.doc.layers[0].pixels {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        *p = 0xff000000 | seed & 0xffffff;
    }
    s.doc.invalidate();
    s.doc.replay_tiles.extend([(0, 0), (0, 1)]);
    let start = s.journal.as_ref().unwrap().status()["bytes"]
        .as_u64()
        .unwrap();
    s.replay_boundary().unwrap();
    let baseline = s.journal.as_ref().unwrap().status()["bytes"]
        .as_u64()
        .unwrap();
    assert!(
        baseline - start < 40_000,
        "duplicate layer/composite blocks were not shared"
    );
    call(&mut s, "brush.set", json!({"radius":1,"stabilizer":0}));
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":5,"y":5},{"x":9,"y":9}]}),
    );
    let end = s.journal.as_ref().unwrap().status()["bytes"]
        .as_u64()
        .unwrap();
    assert!(
        end - baseline < 3000,
        "sparse edit grew by {} bytes",
        end - baseline
    );
    let last = s.journal.as_ref().unwrap().entries.len() - 1;
    let expected = codec::png(&mut s.doc).unwrap();
    assert_eq!(
        replay::png(&s.replay_preview(last, false).unwrap()).unwrap(),
        expected
    );
    assert_eq!(call(&mut s, "replay.verify", json!({}))["valid"], true);
}

#[test]
fn original_v1_recording_loads_and_upgrades_without_changing_verified_pixels() {
    use sha2::{Digest, Sha256};
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/replay-v1.mcreplay");
    let expected: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/replay-v1.json")).unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..8], b"MCRPLY01");
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let png = replay::png(&journal.state).unwrap();
    let digest: crate::engine::replay::Hash = Sha256::digest(&png).into();
    assert_eq!(crate::engine::replay::hex(&digest), expected["png_sha256"]);
    let upgraded = journal.bytes(None).unwrap();
    assert_eq!(&upgraded[..8], b"MCRPLY02");
    let second = Journal::import(&upgraded).unwrap();
    assert_eq!(second.status()["head_hash"], journal.status()["head_hash"]);
    assert_eq!(replay::png(&second.state).unwrap(), png);
}

#[test]
fn direct_contact_storage_failure_rewinds_all_provisional_segments_and_undo() {
    let mut s = session();
    let before = s.state();
    let history = s.doc.history_bytes();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    let mut p = Pointer {
        phase: PointerPhase::Down,
        x: 12.,
        y: 20.,
        pressure: 0.5,
        time: 0,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    for i in 0..400 {
        p.phase = PointerPhase::Move;
        p.x = 12. + (i % 80) as f32;
        p.y = 20. + (i % 50) as f32;
        p.time = i;
        s.pointer(p).unwrap();
    }
    let pending = call(&mut s, "replay.status", json!({}));
    assert_eq!(pending["transaction_pending"], true);
    assert_eq!(pending["committed_head_hash"], head);
    assert_ne!(pending["head_hash"], head);
    assert!(s
        .apply(Action::new(
            "replay.export",
            json!({"path":output("pending.mcreplay")})
        ))
        .is_err());
    s.journal.as_mut().unwrap().fail_next_write_after(9);
    p.phase = PointerPhase::Up;
    assert!(s.pointer(p).is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().status()["head_hash"], head);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
    assert!(s.pointer_checkpoint.is_none());
    assert!(!s.is_drawing());
}

#[test]
fn atomic_group_journal_failure_rolls_back_every_completed_child() {
    let mut s = session();
    let before = s.state();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(8);
    assert!(s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[
        {"id":"brush","action":"brush.set","args":{"radius":12}},
        {"id":"draw","action":"canvas.stroke","args":{"points":[{"x":12,"y":12},{"x":60,"y":60}]}},
        {"id":"layer","action":"layer.add"}
    ]}))).is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.journal.as_ref().unwrap().status()["head_hash"], head);
    assert_eq!(s.atomic_depth, 0);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}
