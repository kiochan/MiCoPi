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
