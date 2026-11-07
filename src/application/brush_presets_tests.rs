//! User brush data stays separate from plugin contributions and document state.
use super::editing_tests::{call, check};
use super::*;
use crate::{
    configuration::Settings,
    engine::{document::Composition, replay::Journal, Document},
    plugins::Registry,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};

fn fixture(tag: &str) -> (Session, PathBuf) {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-presets/{tag}/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    let settings = Settings::decode(
        r#"{"version":1,"custom":{"keep":42},"keybindings":{"file.save":"Ctrl+F12"}}"#,
        path.clone(),
    )
    .unwrap();
    settings.save().unwrap();
    let mut s = Session::new(
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
    )
    .unwrap();
    s.settings_path = Some(path.clone());
    s.load_profile_presets(&settings).unwrap();
    call(&mut s, "canvas.new", json!({"width":7,"height":5}));
    call(
        &mut s,
        "brush.set",
        json!({"color":"#CC3355","background_color":"#77AACC","stabilizer":0,"pressure_size":false}),
    );
    (s, path)
}
fn snapshot(s: &Session) -> Value {
    let mut p = s.parameter_state();
    p.as_object_mut().unwrap().remove("color");
    p.as_object_mut().unwrap().remove("background_color");
    p
}
fn payload(s: &Session, name: &str, patch: Value) -> Value {
    let mut parameters = snapshot(s);
    for (key, value) in patch.as_object().unwrap() {
        parameters[key] = value.clone();
    }
    json!({"name":name,"owner":"painting","tool_id":"brush","parameters":parameters})
}
fn save(s: &mut Session, name: &str, patch: Value) -> u64 {
    let p = payload(s, name, patch);
    call(s, "brush.presets.upsert", p)["preset_id"]
        .as_u64()
        .unwrap()
}
fn query(s: &mut Session) -> Value {
    call(s, "brush.presets.query", json!({}))
}
#[derive(Debug, PartialEq)]
struct DocumentState {
    composition: Composition,
    raw: Vec<Vec<u32>>,
    masks: Vec<(u64, Vec<u8>)>,
    surface: Vec<u32>,
    active: usize,
    history: usize,
}
fn document(s: &Session) -> DocumentState {
    DocumentState {
        composition: s.doc.composition(),
        raw: s.doc.layers.iter().map(|l| l.pixels.clone()).collect(),
        masks: s
            .doc
            .mask_state()
            .items
            .iter()
            .map(|m| {
                let p = s.doc.mask_plane(m.id).unwrap();
                (
                    m.id,
                    (0..s.doc.width * s.doc.height)
                        .map(|i| p.sample(i))
                        .collect(),
                )
            })
            .collect(),
        surface: s.doc.composite.clone(),
        active: s.doc.active,
        history: s.doc.history_bytes(),
    }
}
fn outcome(s: &mut Session) {
    check(s);
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut j = Journal::import(&archive).unwrap();
    assert_eq!(j.verify(None).unwrap()["valid"], true);
    let state = j.preview(j.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    assert_eq!(state.metadata.parameters, s.parameters);
    assert_eq!(state.metadata.selected_tool, s.selected_tool);
    assert!(serde_json::to_value(&state.metadata)
        .unwrap()
        .get("brush_presets")
        .is_none());
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, samples) in document(s).masks {
        let plane = restored.mask_plane(id).unwrap();
        assert_eq!(
            (0..s.doc.width * s.doc.height)
                .map(|i| plane.sample(i))
                .collect::<Vec<_>>(),
            samples
        );
    }
}

#[test]
fn brush_presets_user_records_keep_registry_ownership_and_queries_are_cached_read_only() {
    let (mut s, path) = fixture("catalog");
    let tools = serde_json::to_value(&s.registry.tools).unwrap();
    let before = document(&s);
    let parameters = s.parameters.clone();
    let initial = query(&mut s);
    let mut record = payload(
        &s,
        "Warm marker",
        json!({"radius":13,"flow":0.25,"opacity_mode":"stroke","spacing":0.01}),
    );
    record["expected_library_token"] = initial["library_token"].clone();
    let id = call(&mut s, "brush.presets.upsert", record)["preset_id"]
        .as_u64()
        .unwrap();
    assert_eq!(id, 1);
    assert_eq!(document(&s), before);
    assert_eq!(s.parameters, parameters);
    assert_eq!(serde_json::to_value(&s.registry.tools).unwrap(), tools);
    let state = s.state();
    let count = s.journal.as_ref().unwrap().entries.len();
    let q = query(&mut s);
    assert_eq!(q["version"], 1);
    assert_eq!(q["user"][0]["preset"]["name"], "Warm marker");
    assert_eq!(q["user"][0]["available"], true);
    assert_eq!(q["user"][0]["matching"], false);
    assert!(q["matching_preset_ids"].as_array().unwrap().is_empty());
    assert!(q["builtin"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["tool_id"] == "glaze_marker" && p["owner"] == "painting"));
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["custom"], json!({"keep":42}));
    assert!(saved["keybindings"].to_string().contains("Ctrl+F12"));
    let library_token = q["library_token"].clone();
    assert!(!library_token.is_null());
    // Cache reads must survive an unavailable profile path without filesystem I/O.
    let mut bad = path.clone();
    bad.set_file_name("directory-is-absent/settings.json");
    s.settings_path = Some(bad);
    let mut q2 = query(&mut s);
    q2["profile_path"] = q["profile_path"].clone();
    assert_eq!(q2, q);
    assert_eq!(s.state(), state);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    s.settings_path = Some(path);
    outcome(&mut s);
}

#[test]
fn brush_presets_apply_uses_real_registered_tool_preserves_swatches_and_publishes_once() {
    let (mut s, _) = fixture("apply");
    call(&mut s, "layer.add", json!({}));
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":2}),
    );
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":1,"y":1,"width":3,"height":2}),
    );
    call(&mut s, "layer.fill_color", json!({"color":"#5488CC"}));
    call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"source":"selection","select":true}),
    );
    assert_eq!(s.doc.layer_id(s.doc.active), 2);
    assert_eq!(s.doc.selected_layer_ids(), vec![1, 2]);
    assert_eq!(
        serde_json::to_value(s.doc.mask_state().target).unwrap(),
        json!({"kind":"mask","id":1})
    );
    assert!(s.doc.selection().current.is_some());
    assert!(s.doc.layers[1].pixels.iter().any(|p| p >> 24 != 0));
    let id = save(
        &mut s,
        "Glaze",
        json!({"radius":11,"opacity":0.4,"flow":0.25,"opacity_mode":"stroke"}),
    );
    let doc = document(&s);
    let fg = s.parameters.color.clone();
    let bg = s.parameters.background_color.clone();
    let revision = s.revision;
    let count = s.journal.as_ref().unwrap().entries.len();
    let result = call(&mut s, "brush.presets.apply", json!({"preset_id":id}));
    assert_eq!(result["preset_id"], id);
    assert_eq!(result["selected_tool"], "brush");
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.selected_tool, "brush");
    assert_eq!(s.parameters.radius, 11.);
    assert_eq!(s.parameters.flow, 0.25);
    assert_eq!(s.parameters.opacity_mode, OpacityMode::Stroke);
    assert_eq!(s.parameters.color, fg);
    assert_eq!(s.parameters.background_color, bg);
    assert_eq!(document(&s), doc);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    assert_eq!(query(&mut s)["matching_preset_ids"], json!([id]));
    call(&mut s, "brush.set", json!({"radius":12}));
    assert!(query(&mut s)["matching_preset_ids"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut record = payload(&s, "Ink alias", json!({"radius":8}));
    record["tool_id"] = json!("ink");
    let alias = call(&mut s, "brush.presets.upsert", record)["preset_id"]
        .as_u64()
        .unwrap();
    let mapping = s.registry.native_tools.remove("ink");
    call(&mut s, "brush.presets.apply", json!({"preset_id":alias}));
    assert_eq!(s.selected_tool, "ink");
    assert_eq!(s.parameters.radius, 8.);
    if let Some(mapping) = mapping {
        s.registry.native_tools.insert("ink".into(), mapping);
    }
    outcome(&mut s);
}

#[test]
fn brush_presets_full_record_validation_missing_owners_and_stale_tokens_never_publish_partially() {
    let (mut s, path) = fixture("validation");
    let id = save(&mut s, "Original", json!({}));
    let q = query(&mut s);
    let bytes = std::fs::read(&path).unwrap();
    let state = s.state();
    for key in ["radius", "flow", "opacity_mode", "stabilizer"] {
        let mut value = payload(&s, "Invalid", json!({}));
        value["parameters"].as_object_mut().unwrap().remove(key);
        assert!(s.apply(Action::new("brush.presets.upsert", value)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(query(&mut s), q);
        assert_eq!(s.state(), state);
    }
    for patch in [
        json!({"color":"#112233"}),
        json!({"background_color":"#112233"}),
        json!({"spacing":0.009999999}),
        json!({"flow":1.01}),
    ] {
        let value = payload(&s, "Invalid", patch);
        assert!(s.apply(Action::new("brush.presets.upsert", value)).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(query(&mut s), q);
    }
    let mut renamed = payload(&s, "Renamed", json!({}));
    renamed["preset_id"] = json!(id);
    renamed["expected_library_token"] = q["library_token"].clone();
    call(&mut s, "brush.presets.upsert", renamed.clone());
    let fresh = query(&mut s);
    let bytes = std::fs::read(&path).unwrap();
    assert!(s
        .apply(Action::new("brush.presets.upsert", renamed))
        .is_err());
    assert_eq!(query(&mut s), fresh);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    for owner in ["missing.package", "other-owner"] {
        let mut record = payload(&s, "Unavailable", json!({}));
        record["owner"] = json!(owner);
        let id = call(&mut s, "brush.presets.upsert", record)["preset_id"]
            .as_u64()
            .unwrap();
        let before = s.state();
        assert!(s
            .apply(Action::new("brush.presets.apply", json!({"preset_id":id})))
            .is_err());
        assert_eq!(s.state(), before);
        let catalog = query(&mut s);
        assert_eq!(
            catalog["user"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["preset"]["id"] == id)
                .unwrap()["available"],
            false
        );
    }
    outcome(&mut s);
}

#[test]
fn brush_presets_profile_failure_and_atomic_file_effect_rejection_preserve_cache_and_document() {
    let (mut s, path) = fixture("file-failure");
    let block = path.parent().unwrap().join("parent-file");
    std::fs::write(&block, b"not a directory").unwrap();
    s.settings_path = Some(block.join("settings.json"));
    let q = query(&mut s);
    let state = s.state();
    let before = document(&s);
    let bytes = std::fs::read(&path).unwrap();
    let record = payload(&s, "Cannot persist", json!({}));
    assert_eq!(
        s.apply(Action::new("brush.presets.upsert", record))
            .unwrap_err()
            .code,
        "preset_io"
    );
    assert_eq!(query(&mut s), q);
    assert_eq!(s.state(), state);
    assert_eq!(document(&s), before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    s.settings_path = Some(path.clone());
    let before = s.state();
    let record = payload(&s, "Atomic file", json!({}));
    assert!(s
        .apply(Action::new(
            "pipeline.run",
            json!({"atomic":true,"steps":[
        {"id":"rename","action":"layer.rename","args":{"name":"Must not publish"}},
        {"id":"file","action":"brush.presets.upsert","args":record}]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    outcome(&mut s);
}

fn sample(tool: ToolKind, phase: PointerPhase, i: u32) -> Pointer {
    Pointer {
        phase,
        x: 1.5 + (i % 2) as f32 * 0.1,
        y: 1.5,
        pressure: 0.3 + (i % 5) as f32 * 0.1,
        time: 100 + i,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
        view_x: Some(i as f32),
        view_y: Some(40.),
    }
}
fn transcript(s: &mut Session) -> Vec<Value> {
    let j = s.replay_journal(false).unwrap();
    let events = j.events(0, j.entries.len()).unwrap();
    let mut p: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    p.extend(
        s.replay_samples
            .iter()
            .map(|p| serde_json::to_value(p).unwrap()),
    );
    p
}
#[test]
fn brush_presets_library_file_edits_reject_chunked_contacts_and_valid_apply_finishes_exactly_once()
{
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        let (mut s, path) = fixture(if tool == ToolKind::Brush {
            "contact-brush"
        } else {
            "contact-hand"
        });
        let id = save(
            &mut s,
            "Glaze",
            json!({"radius":3,"flow":0.25,"opacity_mode":"stroke"}),
        );
        let export = path.parent().unwrap().join("contact.micopi-brushes.json");
        call(
            &mut s,
            "brush.presets.export",
            json!({"path":export,"preset_ids":[id]}),
        );
        let profile_bytes = std::fs::read(&path).unwrap();
        let export_bytes = std::fs::read(&export).unwrap();
        let catalog = query(&mut s);
        s.pointer(sample(tool, PointerPhase::Down, 0)).unwrap();
        for i in 1..270 {
            s.pointer(sample(tool, PointerPhase::Move, i)).unwrap();
        }
        let samples = transcript(&mut s);
        assert_eq!(samples.len(), 270);
        // A rejected action can materialize the same dirty surface for its
        // replay boundary. Compare composed pixels, not an older cache frame.
        s.doc.compose();
        let before = document(&s);
        let parameters = s.parameters.clone();
        let selected = s.selected_tool.clone();
        assert_eq!(
            before.raw.iter().flatten().any(|p| p >> 24 != 0),
            tool == ToolKind::Brush
        );
        for action in [
            Action::new(
                "brush.presets.upsert",
                payload(&s, "Rejected during contact", json!({"radius":2})),
            ),
            Action::new("brush.presets.delete", json!({"preset_id":id})),
            Action::new("brush.presets.import", json!({"path":export})),
            Action::new(
                "brush.presets.export",
                json!({"path":export,"preset_ids":[id]}),
            ),
        ] {
            assert_eq!(s.apply(action).unwrap_err().code, "stroke_active");
            assert_eq!(query(&mut s), catalog);
            assert_eq!(std::fs::read(&path).unwrap(), profile_bytes);
            assert_eq!(std::fs::read(&export).unwrap(), export_bytes);
            assert_eq!(document(&s), before);
            assert_eq!(s.parameters, parameters);
            assert_eq!(transcript(&mut s), samples);
            assert!(s.stroke.active());
        }
        query(&mut s);
        assert_eq!(document(&s), before);
        assert_eq!(s.parameters, parameters);
        assert_eq!(s.selected_tool, selected);
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        assert_eq!(transcript(&mut s), samples);
        assert!(s
            .apply(Action::new(
                "brush.presets.apply",
                json!({"preset_id":999999})
            ))
            .is_err());
        assert_eq!(document(&s), before);
        assert_eq!(s.parameters, parameters);
        assert_eq!(transcript(&mut s), samples);
        assert!(s.stroke.active());
        call(&mut s, "brush.presets.apply", json!({"preset_id":id}));
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
        let ended = transcript(&mut s);
        assert_eq!(ended.len(), samples.len() + 1);
        assert_eq!(ended[..samples.len()], samples);
        assert_eq!(ended.last().unwrap()["phase"], "up");
        assert_eq!(ended.last().unwrap()["time"], 369);
        assert_eq!(s.parameters.radius, 3.);
        assert_eq!(s.parameters.flow, 0.25);
        assert_eq!(s.parameters.color, parameters.color);
        assert_eq!(s.parameters.background_color, parameters.background_color);
        outcome(&mut s);
    }
}

#[test]
fn brush_presets_unsupported_native_options_reject_before_ending_existing_contact() {
    let (mut s, _) = fixture("old-capabilities");
    let id = save(
        &mut s,
        "New options",
        json!({"flow":0.25,"opacity_mode":"stroke","spacing":0.01}),
    );
    let features = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_tool_options_features_for_test(0)
        .unwrap();
    let catalog = query(&mut s);
    assert_eq!(catalog["user"][0]["available"], false);
    s.pointer(sample(ToolKind::Brush, PointerPhase::Down, 0))
        .unwrap();
    s.doc.compose();
    let before = document(&s);
    let state = s.state();
    let samples = transcript(&mut s);
    assert!(s
        .apply(Action::new("brush.presets.apply", json!({"preset_id":id})))
        .is_err());
    assert_eq!(document(&s), before);
    assert_eq!(s.state(), state);
    assert_eq!(transcript(&mut s), samples);
    assert!(s.stroke.active());
    s.pointer(sample(ToolKind::Brush, PointerPhase::Up, 1))
        .unwrap();
    s.tool_instances.clear();
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_tool_options_features_for_test(features);
    call(&mut s, "brush.presets.apply", json!({"preset_id":id}));
    outcome(&mut s);
}

#[test]
fn brush_presets_import_is_whole_bundle_validated_and_assigns_fresh_ids_in_file_order() {
    let (mut s, path) = fixture("import-export");
    let a = save(&mut s, "First", json!({"spacing":0.01}));
    let b = save(&mut s, "Second", json!({"radius":17}));
    let export = path.parent().unwrap().join("export.json");
    let result = call(
        &mut s,
        "brush.presets.export",
        json!({"path":export,"preset_ids":[b,a]}),
    );
    let bytes = std::fs::read(&export).unwrap();
    assert_eq!(result["bytes"], bytes.len());
    assert_eq!(result["sha256"].as_str().unwrap().len(), 64);
    let bundle: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bundle["format"], "micopi.brush-presets");
    assert_eq!(bundle["version"], 1);
    assert_eq!(bundle["items"][0]["id"], b);
    assert_eq!(bundle["items"][1]["id"], a);
    let imported = call(&mut s, "brush.presets.import", json!({"path":export}));
    assert_eq!(imported["imported_ids"], json!([3, 4]));
    assert_eq!(
        imported["id_map"],
        json!([{"source_id":b,"preset_id":3},{"source_id":a,"preset_id":4}])
    );
    let catalog = query(&mut s);
    assert_eq!(catalog["user"].as_array().unwrap().len(), 4);
    let settings_bytes = std::fs::read(&path).unwrap();
    let invalid = path.parent().unwrap().join("invalid.json");
    for mode in ["future", "duplicate", "incomplete", "unknown"] {
        let mut value = bundle.clone();
        match mode {
            "future" => value["version"] = json!(2),
            "duplicate" => value["items"][1]["id"] = value["items"][0]["id"].clone(),
            "incomplete" => {
                value["items"][1]["parameters"]
                    .as_object_mut()
                    .unwrap()
                    .remove("flow");
            }
            _ => value["items"][1]["native"] = json!("untrusted.dll"),
        }
        std::fs::write(&invalid, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(s
            .apply(Action::new("brush.presets.import", json!({"path":invalid})))
            .is_err());
        assert_eq!(query(&mut s), catalog);
        assert_eq!(std::fs::read(&path).unwrap(), settings_bytes);
    }
    std::fs::write(&invalid, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(s
        .apply(Action::new("brush.presets.import", json!({"path":invalid})))
        .is_err());
    assert_eq!(query(&mut s), catalog);
    assert_eq!(std::fs::read(&path).unwrap(), settings_bytes);
    assert!(s
        .apply(Action::new(
            "brush.presets.export",
            json!({"path":export,"preset_ids":[a,a]})
        ))
        .is_err());
    assert_eq!(std::fs::read(&export).unwrap(), bytes);
    let alias = path.parent().unwrap().join("export-alias");
    std::fs::create_dir_all(&alias).unwrap();
    let alias = alias.join("..").join("settings.json");
    assert!(s
        .apply(Action::new(
            "brush.presets.export",
            json!({"path":alias,"preset_ids":[a]})
        ))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), settings_bytes);
    assert_eq!(query(&mut s), catalog);
    // The cached library remains usable after an external removal, but an
    // export must never recreate the active profile as a preset bundle.
    std::fs::remove_file(&path).unwrap();
    let missing_alias =
        path.parent()
            .unwrap()
            .join("export-alias")
            .join("..")
            .join(if cfg!(windows) {
                "SETTINGS.JSON"
            } else {
                "settings.json"
            });
    let rejected = s.apply(Action::new(
        "brush.presets.export",
        json!({"path":missing_alias,"preset_ids":[a]}),
    ));
    let created = path.exists() || missing_alias.exists();
    std::fs::write(&path, &settings_bytes).unwrap();
    assert!(rejected.is_err());
    assert!(!created);
    assert_eq!(query(&mut s), catalog);
    assert_eq!(std::fs::read(&path).unwrap(), settings_bytes);
    outcome(&mut s);
}

#[test]
fn brush_presets_cache_reload_preserves_live_parameters_and_apply_rollback_preserves_library() {
    let (mut s, path) = fixture("reload-rollback");
    let id = save(
        &mut s,
        "Glaze",
        json!({"radius":13,"flow":0.25,"opacity_mode":"stroke"}),
    );
    let mut parameters = s.parameters.clone();
    parameters.radius = 21.;
    parameters.flow = 0.6;
    s.parameters = parameters.clone();
    let cached = query(&mut s);
    let mut edited = Settings::load(path.clone()).unwrap();
    edited.brush_presets.items[0].name = "External profile edit".into();
    edited.save().unwrap();
    assert_eq!(
        query(&mut s),
        cached,
        "queries must not reload profile files"
    );
    call(
        &mut s,
        "brush.presets.reload",
        json!({"expected_library_token":cached["library_token"]}),
    );
    assert_eq!(s.parameters, parameters);
    let refreshed = query(&mut s);
    assert_eq!(
        refreshed["user"][0]["preset"]["name"],
        "External profile edit"
    );
    assert!(
        refreshed["library_token"].as_u64().unwrap() > cached["library_token"].as_u64().unwrap()
    );
    let committed_bytes = std::fs::read(&path).unwrap();
    let committed_state = s.state();
    let mut reset = Settings::load(path.clone()).unwrap();
    reset.brush_presets.items.clear();
    reset.brush_presets.next_id = 1;
    reset.save().unwrap();
    let rejected = s.apply(Action::new(
        "brush.presets.reload",
        json!({"expected_library_token":refreshed["library_token"]}),
    ));
    std::fs::write(&path, &committed_bytes).unwrap();
    assert_eq!(rejected.unwrap_err().code, "preset_library_changed");
    assert_eq!(query(&mut s), refreshed);
    assert_eq!(s.state(), committed_state);
    assert_eq!(s.parameters, parameters);
    let replacement: Value = serde_json::from_str(&reset.encode().unwrap()).unwrap();
    let rejected = s.apply(Action::new(
        "settings.update",
        json!({"path":path,"document":replacement}),
    ));
    let after_update = std::fs::read(&path).unwrap();
    std::fs::write(&path, &committed_bytes).unwrap();
    assert_eq!(rejected.unwrap_err().code, "preset_library_changed");
    assert_eq!(after_update, committed_bytes);
    assert_eq!(query(&mut s), refreshed);
    assert_eq!(s.state(), committed_state);
    assert_eq!(s.parameters, parameters);
    let mut replacement_session = Session::new(Registry::bundled()).unwrap();
    replacement_session.load_profile_presets(&reset).unwrap();
    assert!(replacement_session.preset_library().items.is_empty());
    assert_eq!(replacement_session.preset_library().next_id, 1);
    let q = query(&mut s);
    let bytes = std::fs::read(&path).unwrap();
    let before = s.state();
    let doc = document(&s);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"preset","action":"brush.presets.apply","args":{"preset_id":id}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(document(&s), doc);
    assert_eq!(query(&mut s), q);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    let count = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("brush.presets.apply", json!({"preset_id":id})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(document(&s), doc);
    assert_eq!(query(&mut s), q);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    s.replay_error = None; // Fixture-only recovered storage; production remains faulted.
    call(&mut s, "brush.presets.apply", json!({"preset_id":id}));
    let nodes = s.registry.node_catalog();
    assert!(nodes.contains_key("state.painting.presets"));
    assert!(!nodes["action.brush.presets.apply"].read_only);
    let state = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.presets"]}),
    );
    crate::plugins::validate(
        &s.registry.states["painting.presets"].output_schema,
        &state["resources"]["painting.presets"]["value"],
    )
    .unwrap();
    let mut fresh = Session::new(Registry::bundled()).unwrap();
    let settings = Settings::load(path).unwrap();
    fresh.load_profile_presets(&settings).unwrap();
    assert_eq!(query(&mut fresh)["library"], query(&mut s)["library"]);
    outcome(&mut s);
}

#[test]
fn brush_presets_capacity_and_settings_reload_are_bounded_and_do_not_reset_the_live_brush() {
    let (mut s, path) = fixture("capacity");
    let parameters = s.parameters.clone();
    let record = payload(&s, "Template", json!({"spacing":0.01}));
    let mut items = Vec::new();
    for id in 1..=128 {
        let mut item = record.clone();
        item["id"] = json!(id);
        item["name"] = json!(format!("Preset {id}"));
        items.push(item);
    }
    let settings = Settings::load(path.clone()).unwrap();
    let mut value: Value = serde_json::from_str(&settings.encode().unwrap()).unwrap();
    value["brush_presets"] = json!({"version":1,"next_id":129,"items":items});
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"document":value}),
    );
    assert_eq!(s.parameters, parameters);
    let catalog = query(&mut s);
    assert_eq!(catalog["user"].as_array().unwrap().len(), 128);
    let bytes = std::fs::read(&path).unwrap();
    let next = payload(&s, "Over capacity", json!({}));
    assert!(s.apply(Action::new("brush.presets.upsert", next)).is_err());
    assert_eq!(query(&mut s), catalog);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let bundle = path.parent().unwrap().join("one-too-many.json");
    std::fs::write(&bundle,serde_json::to_vec(&json!({"format":"micopi.brush-presets","version":1,"items":[value["brush_presets"]["items"][0]]})).unwrap()).unwrap();
    assert!(s
        .apply(Action::new("brush.presets.import", json!({"path":bundle})))
        .is_err());
    assert_eq!(query(&mut s), catalog);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut replacement = payload(&s, "Last edited", json!({}));
    replacement["preset_id"] = json!(128);
    assert_eq!(
        call(&mut s, "brush.presets.upsert", replacement)["preset_id"],
        128
    );
    assert_eq!(query(&mut s)["user"].as_array().unwrap().len(), 128);
    assert_eq!(s.parameters, parameters);
    outcome(&mut s);

    // This is a bounded text/file publication check, never a large-image case.
    let (mut near, near_path) = fixture("profile-size-ceiling");
    let raw = json!({"version":1,"custom":{"keep":42,"padding":"x".repeat(1024*1024-32768)}});
    let settings = Settings::decode(&raw.to_string(), near_path.clone()).unwrap();
    settings.save().unwrap();
    near.load_profile_presets(&settings).unwrap();
    let file_before = std::fs::read(&near_path).unwrap();
    assert!(file_before.len() < 1024 * 1024);
    let catalog_before = query(&mut near);
    let state_before = near.state();
    let doc_before = document(&near);
    let mut full = value["brush_presets"]["items"].clone();
    for record in full.as_array_mut().unwrap() {
        record["name"] = json!("n".repeat(512));
    }
    let bundle = near_path
        .parent()
        .unwrap()
        .join("bounded-full-library.json");
    let text =
        serde_json::to_vec(&json!({"format":"micopi.brush-presets","version":1,"items":full}))
            .unwrap();
    assert!(text.len() < 1024 * 1024);
    std::fs::write(&bundle, text).unwrap();
    let error = near
        .apply(Action::new("brush.presets.import", json!({"path":bundle})))
        .unwrap_err();
    assert_eq!(error.code, "preset_io");
    assert!(error.message.contains("settings.json exceeds"));
    assert_eq!(std::fs::read(&near_path).unwrap(), file_before);
    assert_eq!(query(&mut near), catalog_before);
    assert_eq!(near.state(), state_before);
    assert_eq!(document(&near), doc_before);
    outcome(&mut near);
}
