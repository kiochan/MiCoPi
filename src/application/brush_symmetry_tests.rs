//! Shared symmetry intent, native outcome ownership and legacy compatibility.
use super::{
    editing_tests::{call, check},
    *,
};
use crate::{
    configuration::{brush_presets::PresetParameters, Settings},
    engine::{raster_masks::PlaneView, replay::Journal, Document},
    plugins::Registry,
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
fn fixture() -> Session {
    let mut s = Session::new(Registry::bundled()).unwrap();
    call(&mut s, "canvas.new", json!({"width":12,"height":10}));
    s
}
fn configure(s: &mut Session, mode: &str) {
    call(
        s,
        "brush.set",
        json!({"radius":0.5,"opacity":0.5,"hardness":1.,"color":"#000000","stabilizer":0,"pressure_size":false,"pressure_opacity":false,"symmetry":mode,"symmetry_x":0.5,"symmetry_y":0.5}),
    );
}
fn stroke(s: &mut Session) {
    call(
        s,
        "canvas.stroke",
        json!({"points":[{"x":2.5,"y":3.5,"pressure":1,"pen":true,"time":100}]}),
    );
}
fn replay(s: &mut Session) {
    check(s);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    assert_eq!(state.metadata.parameters, s.parameters);
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
}
#[test]
fn symmetry_defaults_legacy_wire_public_schemas_and_numeric_admission_agree() {
    let mut s = fixture();
    let legacy = serde_json::to_value(Parameters::default()).unwrap();
    for key in ["symmetry", "symmetry_x", "symmetry_y"] {
        assert!(legacy.get(key).is_none());
    }
    let roundtrip: Parameters = serde_json::from_value(legacy).unwrap();
    assert_eq!(roundtrip.symmetry, SymmetryMode::Off);
    let observed = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool","plugins.registry"]}),
    );
    let tool = &observed["resources"]["painting.tool"]["value"];
    assert_eq!(tool["parameters"]["symmetry"], "off");
    assert_eq!(tool["parameters"]["symmetry_x"], 0.5);
    assert_eq!(tool["parameters"]["symmetry_y"], 0.5);
    assert_eq!(tool["capabilities"]["symmetry"], true);
    crate::plugins::validate(&pipeline::resource_schema("painting.tool"), tool).unwrap();
    for key in ["symmetry", "symmetry_x", "symmetry_y"] {
        let mut incomplete = tool.clone();
        incomplete["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            crate::plugins::validate(&pipeline::resource_schema("painting.tool"), &incomplete)
                .is_err()
        );
    }
    let schema = Operation::BrushPresetUpsert.schema();
    let parameters_schema = &schema["properties"]["parameters"];
    assert_eq!(parameters_schema["required"].as_array().unwrap().len(), 12);
    let legacy = PresetParameters::capture(&Parameters::default())
        .unwrap()
        .value()
        .unwrap();
    crate::plugins::validate(parameters_schema, &legacy).unwrap();
    for key in ["symmetry", "symmetry_x", "symmetry_y"] {
        assert!(!parameters_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!(key)));
        assert!(parameters_schema["properties"].get(key).is_some());
    }
    for args in [
        json!({"symmetry":"radial"}),
        json!({"symmetry_x":-0.00000001}),
        json!({"symmetry_y":1.00000001}),
        json!({"symmetry_x":null}),
    ] {
        let before = s.parameters.clone();
        assert!(s.apply(Action::new("brush.set", args)).is_err());
        assert_eq!(s.parameters, before);
    }
    for mode in ["off", "vertical", "horizontal", "both"] {
        call(&mut s, &format!("brush.symmetry.{mode}"), json!({}));
        assert_eq!(s.parameter_state()["symmetry"], mode);
    }
    replay(&mut s);
}
#[test]
fn symmetry_marks_share_one_undo_and_verified_outcome_on_rgba_and_raw_a8() {
    for (mode, points) in [
        ("off", vec![(2, 3)]),
        ("vertical", vec![(2, 3), (9, 3)]),
        ("horizontal", vec![(2, 3), (2, 6)]),
        ("both", vec![(2, 3), (9, 3), (2, 6), (9, 6)]),
    ] {
        for a8 in [false, true] {
            let mut s = fixture();
            let mask = if a8 {
                Some(
                    call(
                        &mut s,
                        "mask.create",
                        json!({"owner":{"kind":"layer","id":1}}),
                    )["mask_id"]
                        .as_u64()
                        .unwrap(),
                )
            } else {
                None
            };
            configure(&mut s, mode);
            let composition = s.doc.composition();
            let pixels = s.doc.layers[0].pixels.clone();
            let history = s.doc.history_bytes();
            let count = s.journal.as_ref().unwrap().entries.len();
            stroke(&mut s);
            assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
            assert!(s.doc.history_bytes() > history);
            for y in 0..10 {
                for x in 0..12 {
                    let painted = points.contains(&(x, y));
                    if let Some(mask) = mask {
                        assert_eq!(
                            s.doc.mask_plane(mask).unwrap().sample(y * 12 + x),
                            if painted { 127 } else { 255 }
                        );
                    } else {
                        assert_eq!(
                            s.doc.layers[0].pixels[y * 12 + x],
                            if painted { 0x80000000 } else { 0 }
                        );
                    }
                }
            }
            if a8 {
                assert_eq!(s.doc.layers[0].pixels, pixels);
            }
            let after = s.doc.composite.clone();
            call(&mut s, "history.undo", json!({}));
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            if let Some(mask) = mask {
                assert!((0..120).all(|i| s.doc.mask_plane(mask).unwrap().sample(i) == 255));
            }
            call(&mut s, "history.redo", json!({}));
            assert_eq!(s.doc.composite, after);
            replay(&mut s);
        }
    }
}
#[test]
fn unsupported_symmetry_rejects_before_pixels_or_uniform_masks_materialize() {
    for a8 in [false, true] {
        let mut registry = Registry::bundled();
        let plugin = Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap()).unwrap();
        let flags = plugin.replace_tool_options_features_for_test(0).unwrap();
        plugin.replace_tool_options_features_for_test(
            flags & !micopi_plugin_api::TOOL_OPTION_SYMMETRY,
        );
        let mut s = Session::new(registry).unwrap();
        call(&mut s, "canvas.new", json!({"width":12,"height":10}));
        let mask = if a8 {
            Some(
                call(
                    &mut s,
                    "mask.create",
                    json!({"owner":{"kind":"layer","id":1}}),
                )["mask_id"]
                    .as_u64()
                    .unwrap(),
            )
        } else {
            None
        };
        configure(&mut s, "both");
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        let p = Pointer {
            phase: PointerPhase::Down,
            x: 2.5,
            y: 3.5,
            pressure: 1.,
            time: 100,
            pen: true,
            eraser: false,
            tool_override: None,
            view_x: None,
            view_y: None,
        };
        assert_eq!(s.pointer(p).unwrap_err().code, "tool_options_unsupported");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
        if let Some(mask) = mask {
            assert!(matches!(
                s.doc.mask_plane(mask).unwrap(),
                PlaneView::Uniform(255)
            ));
        }
        configure(&mut s, "off");
        s.pointer(p).unwrap();
        s.pointer(Pointer {
            phase: PointerPhase::Up,
            ..p
        })
        .unwrap();
        replay(&mut s);
    }
}
#[test]
fn symmetry_settings_presets_projects_and_atomic_rollback_keep_exact_intent() {
    let mut s = fixture();
    let profile_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-symmetry-profile/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&profile_dir).unwrap();
    let profile = Settings::defaults(profile_dir.join("settings.json"));
    profile.save().unwrap();
    s.settings_path = Some(profile.path.clone());
    s.load_profile_presets(&profile).unwrap();
    configure(&mut s, "both");
    call(
        &mut s,
        "brush.set",
        json!({"symmetry_x":0.25,"symmetry_y":0.75}),
    );
    let preferences = crate::configuration::PaintingPreferences::from(&s.parameters);
    let mut settings = Settings::defaults(PathBuf::new());
    settings.painting = preferences.clone();
    let raw = settings.encode().unwrap();
    let decoded = Settings::decode(&raw, PathBuf::new()).unwrap();
    assert_eq!(decoded.painting, preferences);
    let captured = PresetParameters::capture(&s.parameters).unwrap();
    let id=call(&mut s,"brush.presets.upsert",json!({"name":"Symmetric ink","owner":"painting","tool_id":"brush","parameters":captured.value().unwrap()}))["preset_id"].as_u64().unwrap();
    configure(&mut s, "off");
    call(&mut s, "brush.presets.apply", json!({"preset_id":id}));
    assert_eq!(s.parameters.symmetry, SymmetryMode::Both);
    assert_eq!(
        (s.parameters.symmetry_x, s.parameters.symmetry_y),
        (0.25, 0.75)
    );
    stroke(&mut s);
    let before = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let parameters = s.parameters.clone();
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"mode","action":"brush.set","args":{"symmetry":"horizontal"}},{"id":"draw","action":"canvas.stroke","args":{"points":[{"x":4.5,"y":5.5,"pressure":1,"pen":true}]}},{"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.parameters, parameters);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.revision, revision);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-symmetry/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("symmetry.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.parameters, parameters);
    assert_eq!(opened.doc.layers[0].pixels, pixels);
    replay(&mut s);
    replay(&mut opened);
    let mut patched = parameters.clone();
    crate::configuration::PaintingPreferences::default().apply(&mut patched);
    assert_eq!(patched.symmetry, SymmetryMode::Off);
    assert_eq!((patched.symmetry_x, patched.symmetry_y), (0.5, 0.5));
    let old = PresetParameters::capture(&Parameters::default()).unwrap();
    assert_eq!(
        old.apply_to(&parameters).unwrap().symmetry,
        SymmetryMode::Off
    );
}
