use super::*;
use crate::engine::{
    replay::{Journal, Source},
    Document,
};
use serde_json::json;

fn sample(w: usize, h: usize) -> Mask {
    Mask::new(w, h, (0..w * h).map(|i| (i * 17 % 256) as u8).collect()).unwrap()
}

#[test]
fn mask_roundtrip_is_cached_shared_and_distinguishes_empty_from_absent() {
    let mask = sample(11, 9);
    let cloned = mask.clone();
    assert_eq!(mask.coverage().as_ptr(), cloned.coverage().as_ptr());
    let encoded = serde_json::to_string(&mask).unwrap();
    let decoded: Mask = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, mask);
    assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    let mut d = Document::new(11, 9);
    assert!(serde_json::to_value(d.composition())
        .unwrap()
        .get("selection")
        .is_none());
    let old = d.composition();
    d.set_selection(SelectionState {
        current: Some(Mask::new(11, 9, vec![0; 99]).unwrap()),
        previous: None,
    })
    .unwrap();
    assert_ne!(d.composition(), old);
    assert!(d.selection().current.is_some());
    d.undo();
    assert_eq!(d.composition(), old);
    d.redo();
    assert!(d.selection().current.is_some());
}

#[test]
fn masks_reject_malformed_trailing_truncated_and_oversized_streams() {
    let value = serde_json::to_value(sample(9, 7)).unwrap();
    for changed in [
        json!({"width":0,"height":7,"deflate_hex":"0300"}),
        json!({"width":4097,"height":7,"deflate_hex":"0300"}),
        json!({"width":9,"height":8,"deflate_hex":value["deflate_hex"]}),
        json!({"width":9,"height":7,"deflate_hex":"gg"}),
        json!({"width":9,"height":7,"deflate_hex":"0"}),
        json!({"width":9,"height":7,"deflate_hex":format!("{}00",value["deflate_hex"].as_str().unwrap())}),
        json!({"width":9,"height":7,"deflate_hex":"00".repeat(MAX_PACKED+1)}),
    ] {
        assert!(serde_json::from_value::<Mask>(changed).is_err());
    }
    let stream = value["deflate_hex"].as_str().unwrap();
    for length in (0..stream.len()).step_by(2) {
        assert!(serde_json::from_value::<Mask>(
            json!({"width":9,"height":7,"deflate_hex":&stream[..length]})
        )
        .is_err());
    }
    assert!(Mask::new(2, 2, vec![0; 3]).is_err());
    let full = Mask::new(4096, 4096, vec![255; 4096 * 4096]).unwrap();
    let wire = serde_json::to_value(full).unwrap();
    assert!(serde_json::from_value::<Mask>(
        json!({"width":1,"height":1,"deflate_hex":wire["deflate_hex"]})
    )
    .is_err());
    // Incompressible coverage is bounded instead of creating huge metadata events.
    let mut seed = 1234567u32;
    let noise = (0..1024 * 1024)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as u8
        })
        .collect();
    assert!(Mask::new(1024, 1024, noise).is_err());
}

#[test]
fn selection_reframes_with_pixels_and_restores_both_masks_and_dimensions() {
    let mut d = Document::new(11, 9);
    let state = SelectionState {
        current: Some(sample(11, 9)),
        previous: Some(Mask::new(11, 9, vec![144; 99]).unwrap()),
    };
    let pointer = state.current.as_ref().unwrap().coverage().as_ptr();
    d.set_selection(state.clone()).unwrap();
    let before = d.composition();
    d.reframe(8, 7, -2, -1).unwrap();
    let cropped = d.selection().clone();
    for y in 0..7 {
        assert_eq!(
            &cropped.current.as_ref().unwrap().coverage()[y * 8..y * 8 + 8],
            &state.current.as_ref().unwrap().coverage()[(y + 1) * 11 + 2..(y + 1) * 11 + 10]
        );
    }
    assert_eq!(cropped.previous.as_ref().unwrap().coverage(), &[144; 56]);
    d.undo();
    assert_eq!(d.composition(), before);
    assert_eq!(
        d.selection().current.as_ref().unwrap().coverage().as_ptr(),
        pointer
    );
    d.redo();
    assert_eq!(d.selection(), &cropped);
    d.reframe(10, 10, 2, 3).unwrap();
    let current = d.selection().current.as_ref().unwrap().coverage();
    assert!(current[..30].iter().all(|v| *v == 0));
    assert_eq!(
        &current[32..40],
        &cropped.current.as_ref().unwrap().coverage()[..8]
    );
    let composition = d.composition();
    assert!(d.set_selection(state).is_err());
    assert_eq!(d.composition(), composition);
}

#[test]
fn selection_projects_and_verified_journals_roundtrip_without_dlls() {
    let mut s = crate::application::Session::new(crate::plugins::Registry::bundled()).unwrap();
    s.doc = Document::new(11, 9);
    let meta = s.replay_metadata();
    let mut journal = Journal::new(&mut s.doc, meta, json!({})).unwrap();
    let old = s.doc.composition();
    s.doc
        .set_selection(SelectionState {
            current: Some(sample(11, 9)),
            previous: Some(Mask::new(11, 9, vec![255; 99]).unwrap()),
        })
        .unwrap();
    let selected = s.doc.composition();
    let meta = s.replay_metadata();
    let p = journal
        .prepare(
            &mut s.doc,
            meta,
            Source::Observed {
                reason: "Selection storage fixture".into(),
            },
            true,
            None,
        )
        .unwrap();
    journal.commit(p).unwrap();
    let bytes = journal.bytes(None).unwrap();
    assert!(bytes.len() < 32 * 1024);
    let mut imported = Journal::import(&bytes).unwrap();
    assert_eq!(imported.verify(None).unwrap()["valid"], true);
    let state = imported.preview(1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert_eq!(restored.composition(), selected);
    assert!(restored.renderer.is_none());
    assert_eq!(imported.preview(0).unwrap().metadata.composition, old);
    let output = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/selection-storage");
    std::fs::create_dir_all(&output).unwrap();
    let path = output.join("selection.mcp");
    s.doc.save(&path).unwrap();
    assert_eq!(Document::load(&path).unwrap().composition(), selected);
    let saved_file = std::fs::read(&path).unwrap();
    // A mask can decode successfully yet belong to a different canvas shape.
    let base = 24 + (2 + 11 * 9 * 4);
    let length = u32::from_le_bytes(saved_file[base..base + 4].try_into().unwrap()) as usize;
    let mut bad: serde_json::Value =
        serde_json::from_slice(&saved_file[base + 4..base + 4 + length]).unwrap();
    bad["selection"]["current"]["width"] = json!(9);
    bad["selection"]["current"]["height"] = json!(11);
    let encoded = serde_json::to_vec(&bad).unwrap();
    let mut malformed = saved_file[..base].to_vec();
    malformed.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    malformed.extend_from_slice(&encoded);
    let bad_path = output.join("mismatched-selection.mcp");
    std::fs::write(&bad_path, malformed).unwrap();
    assert!(Document::load(&bad_path).is_err());
    let mut metadata = s.replay_metadata();
    metadata.width += 1;
    assert!(metadata.validate().is_err());
    // A checkpoint swaps shared masks back without recomputation.
    state.restore_document(&mut s.doc);
    let saved = s.doc.checkpoint_history();
    s.doc.set_selection(SelectionState::default()).unwrap();
    state.restore_document(&mut s.doc);
    s.doc.restore_history(saved);
    assert_eq!(s.doc.composition(), selected);
    s.doc.undo();
    assert_eq!(s.doc.composition(), old);
    // Defend the save boundary even if an internal caller supplies oversized metadata.
    s.doc.workflow.nodes.push(crate::engine::workflow::Node {
        id: 1,
        node_type: "fixture".into(),
        position: [0., 0.],
        parameters: json!({"payload":"x".repeat(1024*1024)}),
    });
    assert!(s.doc.save(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), saved_file);
}
