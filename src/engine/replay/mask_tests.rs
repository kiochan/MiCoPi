//! Codec and shared-cell checks independent of a renderer or native plugin.
use super::*;
use crate::engine::{
    document::Composition,
    raster_masks::{MaskMetadata, MaskOwner, MaskState, PlaneView},
};

fn metadata(width: usize, height: usize, layers: usize) -> Metadata {
    let mut document = Document::new(width, height);
    for _ in 1..layers {
        document.add_layer();
    }
    Metadata {
        width,
        height,
        active: document.active,
        composition: document.composition(),
        selected_tool: "brush".into(),
        tool: ToolKind::Brush,
        parameters: Parameters::default(),
        view: View::default(),
        revision: 0,
        dirty: false,
        file: None,
        source_file: None,
        ui: Value::Null,
        drawing: false,
        render_error: None,
    }
}
fn state(mut metadata: Metadata, samples: &[(u64, PlaneView<'_>)]) -> State {
    metadata.composition.raster_masks = MaskState {
        version: 1,
        items: samples
            .iter()
            .map(|(id, _)| MaskMetadata::new(*id, MaskOwner::Layer { id: *id }))
            .collect(),
        next_id: samples.last().map_or(0, |(id, _)| id + 1),
        target: Default::default(),
    };
    metadata.validate().unwrap();
    let zero = Cell {
        block: Arc::new(Block { pixels: vec![0] }),
        digest: hash(&0u32.to_le_bytes()),
    };
    let masks = samples
        .iter()
        .map(|(id, plane)| {
            (
                *id,
                (0..metadata.count())
                    .map(|tile| masks::Block::read(*plane, &metadata, tile).cell())
                    .collect(),
            )
        })
        .collect();
    State {
        layers: vec![vec![zero.clone(); metadata.count()]; metadata.composition.layers.len()],
        surface: vec![zero; metadata.count()],
        metadata,
        masks,
    }
}
fn event(previous: Option<&State>, current: &State, replace: bool) -> Event {
    let mut changes = vec![];
    if replace {
        for (layer, grid) in current
            .layers
            .iter()
            .chain(std::iter::once(&current.surface))
            .enumerate()
        {
            for (tile, cell) in grid.iter().enumerate() {
                changes.push(Change {
                    layer,
                    tile,
                    block: Some((*cell.block).clone()),
                    reuse: None,
                    xor: false,
                });
            }
        }
    }
    Event {
        sequence: usize::from(previous.is_some()),
        elapsed_ms: 0,
        source: if previous.is_none() {
            Source::Genesis {
                version: "fixture".into(),
                environment: Value::Null,
            }
        } else {
            Source::Observed {
                reason: "A8 codec fixture".into(),
            }
        },
        success: true,
        error: None,
        before: previous.map(State::digest).unwrap_or([0; 32]),
        after: current.digest(),
        metadata: Some(current.metadata.clone()),
        metadata_delta: None,
        replace,
        changes,
        mask_version: (!current.metadata.composition.raster_masks.is_default()
            || previous.is_some_and(|old| !old.metadata.composition.raster_masks.is_default()))
        .then_some(1),
        mask_changes: current
            .masks
            .iter()
            .flat_map(|(id, grid)| {
                grid.iter()
                    .enumerate()
                    .map(move |(tile, cell)| masks::Change {
                        mask_id: *id,
                        tile,
                        block: Some((*cell.block).clone()),
                        reuse: None,
                        xor: false,
                    })
            })
            .collect(),
    }
}
fn copied_event(event: &Event) -> Event {
    serde_json::from_slice(&serde_json::to_vec(event).unwrap()).unwrap()
}
fn composition_without_masks(metadata: &Metadata) -> Composition {
    let mut composition = metadata.composition.clone();
    composition.raster_masks = Default::default();
    composition
}

#[test]
fn a8_default_wire_and_state_digest_keep_the_legacy_shape() {
    let before = state(metadata(65, 65, 1), &[]);
    let encoded = event(None, &before, true);
    let bytes = serde_json::to_vec(&encoded).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value.get("mask_version").is_none());
    assert!(value.get("mask_changes").is_none());
    assert!(value["metadata"]["composition"]
        .get("raster_masks")
        .is_none());
    let mut expected = Sha256::new();
    expected.update(serde_json::to_vec(&before.metadata).unwrap());
    for grid in before.layers.iter().chain(std::iter::once(&before.surface)) {
        for cell in grid {
            expected.update(cell.digest);
        }
    }
    assert_eq!(before.digest(), Hash::from(expected.finalize()));
    assert_eq!(encoded.apply(None).unwrap().digest(), before.digest());
    assert_eq!(serde_json::to_vec(&copied_event(&encoded)).unwrap(), bytes);

    let bytes = [37; 65 * 65];
    let masked = state(metadata(65, 65, 1), &[(1, PlaneView::Dense(&bytes))]);
    assert_ne!(masked.digest(), before.digest());
    assert_eq!(masked.surface_hash(), before.surface_hash());
    let value = serde_json::to_value(event(None, &masked, true)).unwrap();
    assert_eq!(value["mask_version"], 1);
    assert_eq!(value["mask_changes"].as_array().unwrap().len(), 4);
    let rejection = serde_json::from_value::<LegacyEvent>(value)
        .unwrap_err()
        .to_string();
    assert!(
        rejection.contains("unknown field")
            && (rejection.contains("`mask_version`") || rejection.contains("`mask_changes`")),
        "The old strict reader must reject an A8 extension field: {rejection}"
    );

    let mut allocator_only = state(metadata(65, 65, 1), &[]);
    allocator_only.metadata.composition.raster_masks.next_id = 2;
    let value = serde_json::to_value(event(None, &allocator_only, true)).unwrap();
    assert_eq!(value["mask_version"], 1);
    assert!(value.get("mask_changes").is_none());
    let rejection = serde_json::from_value::<LegacyEvent>(value)
        .unwrap_err()
        .to_string();
    assert!(
        rejection.contains("unknown field `mask_version`"),
        "Allocator-only mask state must still reject on its explicit marker: {rejection}"
    );
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEvent {
    #[serde(rename = "sequence")]
    _sequence: Value,
    #[serde(rename = "elapsed_ms")]
    _elapsed: Value,
    #[serde(rename = "source")]
    _source: Value,
    #[serde(rename = "success")]
    _success: Value,
    #[serde(rename = "error")]
    _error: Value,
    #[serde(rename = "before")]
    _before: Value,
    #[serde(rename = "after")]
    _after: Value,
    #[serde(rename = "metadata", default)]
    _metadata: Option<Value>,
    #[serde(rename = "metadata_delta", default)]
    _delta: Option<Value>,
    #[serde(rename = "replace")]
    _replace: Value,
    #[serde(rename = "changes")]
    _changes: Value,
}

#[test]
fn a8_cells_deduplicate_separately_and_sparse_xor_restores_exact_bytes() {
    let mut uniform = state(
        metadata(128, 64, 2),
        &[(1, PlaneView::Uniform(79)), (2, PlaneView::Uniform(79))],
    );
    let previous = state(metadata(128, 64, 2), &[]);
    let mut encoded = event(None, &uniform, true);
    encoded.compact(&previous, &mut uniform).unwrap();
    assert!(encoded.mask_changes[0].block.is_some());
    assert!(encoded.mask_changes[1..]
        .iter()
        .all(|change| change.reuse == Some(0) && change.block.is_none()));
    let decoded = encoded.apply(None).unwrap();
    assert!(Arc::ptr_eq(
        &decoded.masks[&1][0].block,
        &decoded.masks[&2][1].block
    ));
    assert_eq!(decoded.digest(), uniform.digest());

    let mut raw: Vec<_> = (0..64 * 64).map(|i| (i * 131 % 251 + 1) as u8).collect();
    let previous = state(metadata(64, 64, 1), &[(1, PlaneView::Dense(&raw))]);
    raw[83] ^= 17;
    let mut current = state(metadata(64, 64, 1), &[(1, PlaneView::Dense(&raw))]);
    let mut encoded = event(Some(&previous), &current, false);
    encoded.compact(&previous, &mut current).unwrap();
    assert!(encoded.mask_changes[0].xor);
    assert!(encoded.metadata.is_none());
    assert_eq!(
        encoded.apply(Some(&previous)).unwrap().digest(),
        current.digest()
    );
    let mut output = vec![0; raw.len()];
    masks::fill(&current.masks[&1], &current.metadata, &mut output);
    assert_eq!(output, raw);
}

#[test]
fn a8_grids_survive_raster_count_replacement_without_being_rerecorded() {
    let previous = state(metadata(65, 65, 1), &[(1, PlaneView::Uniform(101))]);
    let current = state(metadata(65, 65, 2), &[(1, PlaneView::Uniform(101))]);
    let mut encoded = event(Some(&previous), &current, true);
    encoded.mask_changes.clear();
    let decoded = encoded.apply(Some(&previous)).unwrap();
    assert_eq!(decoded.digest(), current.digest());
    assert!(Arc::ptr_eq(
        &decoded.masks[&1][0].block,
        &previous.masks[&1][0].block
    ));

    let current = state(metadata(66, 65, 2), &[(1, PlaneView::Uniform(101))]);
    let mut encoded = event(Some(&previous), &current, true);
    encoded.mask_changes.clear();
    assert!(
        encoded.apply(Some(&previous)).is_err(),
        "New geometry needs complete A8 coverage"
    );
}

#[test]
fn a8_removed_masks_are_implicit_and_allocator_only_events_still_require_the_marker() {
    let previous = state(metadata(65, 65, 1), &[(1, PlaneView::Uniform(113))]);
    let mut current = state(metadata(65, 65, 1), &[]);
    current.metadata.composition.raster_masks.next_id = 2;
    let mut encoded = event(Some(&previous), &current, false);
    assert!(encoded.mask_changes.is_empty());
    assert_eq!(encoded.mask_version, Some(1));
    assert_eq!(
        encoded.apply(Some(&previous)).unwrap().digest(),
        current.digest()
    );
    encoded.mask_version = None;
    assert!(encoded.apply(Some(&previous)).is_err());
    let mut bad = event(Some(&previous), &current, false);
    bad.metadata
        .as_mut()
        .unwrap()
        .composition
        .raster_masks
        .next_id = 1;
    assert!(bad
        .apply(Some(&previous))
        .err()
        .unwrap()
        .message
        .contains("allocator"));
    let clean = state(metadata(65, 65, 1), &[]);
    let mut redundant = event(None, &clean, true);
    redundant.mask_version = Some(1);
    assert!(redundant.apply(None).is_err());
}

#[test]
fn a8_rejects_missing_invalid_duplicate_and_cross_namespace_tiles_before_publication() {
    let previous = state(metadata(65, 65, 1), &[]);
    let current = state(metadata(65, 65, 1), &[(1, PlaneView::Uniform(113))]);
    let good = event(Some(&previous), &current, false);
    for mode in 0..12 {
        let mut bad = copied_event(&good);
        match mode {
            0 => bad.mask_version = None,
            1 => bad.mask_version = Some(2),
            2 => {
                bad.mask_changes.pop();
            }
            3 => bad.mask_changes[0].mask_id = 999,
            4 => bad.mask_changes[0].tile = current.metadata.count(),
            5 => bad.mask_changes[1].tile = 0,
            6 => bad.mask_changes[0].block = Some(masks::Block { bytes: vec![1, 2] }),
            7 => {
                bad.mask_changes[0].block = None;
                bad.mask_changes[0].reuse = Some(0);
            }
            8 => bad.mask_changes[0].xor = true,
            9 => {
                bad.mask_changes[0].block = None;
                bad.mask_changes[0].reuse = Some(1);
            }
            10 => bad.mask_changes[0].reuse = Some(0),
            11 => bad.mask_changes.push(masks::Change {
                mask_id: 1,
                tile: 0,
                block: Some(masks::Block { bytes: vec![113] }),
                reuse: None,
                xor: false,
            }),
            _ => unreachable!(),
        }
        let digest = previous.digest();
        assert!(
            bad.apply(Some(&previous)).is_err(),
            "Malformed A8 mode {mode}"
        );
        assert_eq!(previous.digest(), digest);
    }
    let mut cross = event(None, &current, true);
    cross.mask_changes[0].block = None;
    cross.mask_changes[0].reuse = Some(0);
    assert!(!cross.changes.is_empty());
    assert!(
        cross.apply(None).is_err(),
        "RGBA tile zero cannot satisfy an A8 reference"
    );

    let mut bad_owner = serde_json::to_value(good).unwrap();
    bad_owner["metadata"]["composition"]["raster_masks"]["items"][0]["owner"]["id"] = json!(99);
    assert!(serde_json::from_value::<Event>(bad_owner)
        .unwrap()
        .apply(Some(&previous))
        .is_err());
}

#[test]
fn a8_wire_rejects_bad_base64_bounds_and_preserves_edge_tiles() {
    let oversized_list = serde::de::value::SeqDeserializer::<_, serde_json::Error>::new(
        std::iter::repeat(Value::Null).take(32 * 64 * 64 + 1),
    );
    assert!(masks::deserialize_changes(oversized_list)
        .err()
        .unwrap()
        .to_string()
        .contains("Too many A8 tile changes"));
    for invalid in [
        "", "A", "AA=A", "A===", "====", "/z==", "AAB=", "AA==AA==", "****",
    ] {
        assert!(
            serde_json::from_value::<masks::Block>(json!({"bytes":invalid})).is_err(),
            "{invalid}"
        );
    }
    let oversized = crate::application::codec::base64(&vec![1; 4097]);
    assert!(serde_json::from_value::<masks::Block>(json!({"bytes":oversized})).is_err());
    let raw: Vec<_> = (0..65 * 65).map(|i| (i * 37 % 256) as u8).collect();
    let current = state(metadata(65, 65, 1), &[(1, PlaneView::Dense(&raw))]);
    assert_eq!(
        current.masks[&1]
            .iter()
            .map(|cell| cell.block.bytes.len())
            .collect::<Vec<_>>(),
        vec![4096, 64, 64, 1]
    );
    let encoded = event(None, &current, true);
    let decoded = copied_event(&encoded).apply(None).unwrap();
    let mut output = vec![0; raw.len()];
    masks::fill(&decoded.masks[&1], &decoded.metadata, &mut output);
    assert_eq!(output, raw);
    assert!(masks::uniform(&decoded.masks[&1]).is_none());
    assert_eq!(
        composition_without_masks(&decoded.metadata),
        metadata(65, 65, 1).composition
    );
}

#[test]
fn a8_verified_restoration_reuses_dense_storage_and_never_invokes_a_renderer() {
    struct Refuse;
    impl crate::engine::graph::Renderer for Refuse {
        fn render(
            &self,
            _: &[micopi_plugin_api::CompositeNode],
            _: &[micopi_plugin_api::CompositeLayer],
            _: u32,
            _: u32,
            _: &mut micopi_plugin_api::Pixels,
        ) -> Result<(), String> {
            panic!("Verified restoration must not execute a renderer")
        }
    }
    let raw: Vec<_> = (0..65 * 65).map(|i| (i * 137 % 256) as u8).collect();
    let current = state(metadata(65, 65, 1), &[(1, PlaneView::Dense(&raw))]);
    let mut document = Document::new(65, 65);
    document.renderer = Some(Arc::new(Refuse));
    current.restore_document(&mut document);
    assert!(current.matches_document(&document));
    let pointer = match document.mask_plane(1).unwrap() {
        PlaneView::Dense(samples) => samples.as_ptr(),
        _ => panic!("Expected dense samples"),
    };
    let mut future = document.composition();
    future.raster_masks.next_id = 8;
    document.restore_composition(future);
    current.restore_document(&mut document);
    assert_eq!(
        document.mask_state().next_id,
        2,
        "Verified restoration keeps the recorded allocator exactly"
    );
    assert!(current.matches_document(&document));
    assert_eq!(
        pointer,
        match document.mask_plane(1).unwrap() {
            PlaneView::Dense(samples) => samples.as_ptr(),
            _ => panic!("Expected dense samples"),
        }
    );
    assert_eq!(document.replay_mask_tiles.len(), 4);
    document.compose();
    assert!(document.render_error.is_none());

    let mut uniform = state(metadata(65, 65, 1), &[(1, PlaneView::Uniform(213))]);
    uniform.metadata.composition.raster_masks.next_id =
        current.metadata.composition.raster_masks.next_id;
    uniform.restore_document(&mut document);
    assert!(matches!(
        document.mask_plane(1).unwrap(),
        PlaneView::Uniform(213)
    ));
    assert!(uniform.matches_document(&document));
}

fn session() -> crate::application::Session {
    let registry = crate::plugins::Registry::load(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
    )
    .unwrap();
    assert!(
        registry.diagnostics.is_empty(),
        "{:?}",
        registry.diagnostics
    );
    let mut session = crate::application::Session::new(registry).unwrap();
    session
        .apply(crate::application::Action::new(
            "canvas.new",
            json!({"width":65,"height":65}),
        ))
        .unwrap();
    session
}
fn output() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let directory =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-replay");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}.mcp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

#[test]
fn a8_journal_roundtrip_shares_checkpoints_and_recovers_partial_write_without_dll_reexecution() {
    use crate::engine::raster_masks::SamplePlane;
    let mut session = session();
    let id = session
        .doc
        .create_mask(MaskOwner::Layer { id: 1 }, 255)
        .unwrap();
    session.replay_boundary().unwrap();
    let raw: Vec<_> = (0..65 * 65).map(|i| (i * 29 % 256) as u8).collect();
    session
        .doc
        .replace_mask_plane(id, SamplePlane::Dense(raw.clone()))
        .unwrap();
    session.replay_boundary().unwrap();
    let before = session.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut imported = Journal::import(&before).unwrap();
    assert_eq!(imported.verify(None).unwrap()["valid"], true);
    assert!(imported.state.matches_document(&session.doc));
    let saved = session.journal.as_ref().unwrap().state.clone();
    let history = session.doc.checkpoint_history();
    assert!(Arc::ptr_eq(
        &saved.masks[&id][0].block,
        &session.journal.as_ref().unwrap().state.masks[&id][0].block
    ));
    let mut next = raw;
    next[80] ^= 77;
    session
        .doc
        .replace_mask_plane(id, SamplePlane::Dense(next))
        .unwrap();
    let metadata = session.replay_metadata();
    let prepared = session
        .journal
        .as_ref()
        .unwrap()
        .prepare(
            &mut session.doc,
            metadata,
            Source::Observed {
                reason: "Partial A8 append fixture".into(),
            },
            true,
            None,
        )
        .unwrap();
    session.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(session.journal.as_mut().unwrap().commit(prepared).is_err());
    assert_eq!(
        session.journal.as_mut().unwrap().bytes(None).unwrap(),
        before
    );
    saved.restore_document(&mut session.doc);
    session.doc.restore_history(history);
    assert!(saved.matches_document(&session.doc));
    assert_eq!(
        session.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
    let mut restored = Document::new(65, 65);
    imported.state.restore_document(&mut restored);
    assert!(imported.state.matches_document(&restored));
    assert!(restored.renderer.is_none());
}

#[test]
fn a8_save_rescans_all_raw_tiles_and_embedded_outcomes_match_the_project() {
    let mut session = session();
    let id = session
        .doc
        .create_mask(MaskOwner::Layer { id: 1 }, 255)
        .unwrap();
    session.replay_boundary().unwrap();
    let lease = session.doc.begin_mask_stroke(id).unwrap();
    let region = session.doc.mask_region(lease, 64, 1, 1, 1).unwrap();
    unsafe {
        *region.data = 19;
    }
    session.doc.finish();
    // Emulate an engine caller whose raw write was not announced to the journal.
    // Save is explicitly a complete verification boundary for every raw plane.
    session.doc.replay_mask_tiles.clear();
    let path = output();
    session
        .apply(crate::application::Action::new(
            "document.save",
            json!({"path":path}),
        ))
        .unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(b"MICOPI04"));
    let mut document = Document::load(&path).unwrap();
    assert_eq!(document.mask_plane(id).unwrap().sample(65 + 64), 19);
    let mut journal = crate::application::replay::read_embedded(&mut document)
        .unwrap()
        .unwrap();
    assert!(journal.state.matches_document(&document));
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let event = journal.event(journal.entries.len() - 1).unwrap();
    assert_eq!(event.mask_version, Some(1));
    assert!(!event.mask_changes.is_empty());
}
