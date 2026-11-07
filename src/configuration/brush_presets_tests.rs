//! Storage/codec boundaries independently of plugin loading or UI state.
use super::*;
use crate::configuration::Settings;
use serde_json::json;
use std::path::PathBuf;

fn snapshot() -> PresetParameters {
    PresetParameters::capture(&Parameters::default()).unwrap()
}
fn preset(id: u64) -> UserPreset {
    UserPreset::new(
        id,
        "線画 · 선화".into(),
        "missing-provider".into(),
        "missing-provider.ink".into(),
        snapshot(),
    )
    .unwrap()
}

#[test]
fn preset_snapshot_is_complete_but_preserves_both_artist_swatches() {
    let mut source = Parameters::default();
    source.radius = 19.;
    source.opacity = 0.4;
    source.hardness = 0.6;
    source.stabilizer = 0.2;
    source.pressure_size = false;
    source.pressure_opacity = true;
    source.pressure_gamma = 1.5;
    source.minimum_radius = 0.25;
    source.spacing = 0.01;
    source.minimum_opacity = 0.3;
    source.flow = 0.25;
    source.opacity_mode = OpacityMode::Stroke;
    source.color = "#ABCDEF".into();
    source.background_color = "#010203".into();
    let captured = PresetParameters::capture(&source).unwrap();
    let mut expected = canonical_value(&source).unwrap();
    expected.as_object_mut().unwrap().remove("color");
    expected.as_object_mut().unwrap().remove("background_color");
    assert_eq!(captured.value().unwrap(), expected);
    let mut current = Parameters::default();
    current.color = "#aAbBcC".into();
    current.background_color = "#dDeEfF".into();
    let applied = captured.apply_to(&current).unwrap();
    assert_eq!(applied.color, current.color);
    assert_eq!(applied.background_color, current.background_color);
    assert_eq!(PresetParameters::capture(&applied).unwrap(), captured);
    assert_eq!(source.color, "#ABCDEF");
    assert_eq!(current.radius, 6.);
}

#[test]
fn preset_raw_numbers_validate_before_f32_rounding_and_spacing_boundary_round_trips() {
    let valid = snapshot().value().unwrap();
    for (key, value) in [
        ("radius", json!(200.000001)),
        ("opacity", json!(1.0000000001)),
        ("hardness", json!(-0.00000000001)),
        ("pressure_gamma", json!(0.2499999999)),
        ("spacing", json!(0.009999999999)),
        ("flow", json!(1.0000000001)),
    ] {
        let mut raw = valid.clone();
        raw[key] = value;
        assert!(PresetParameters::decode_value(&raw).is_err(), "{key}");
        assert!(
            serde_json::from_value::<PresetParameters>(raw).is_err(),
            "{key}"
        );
    }
    let mut raw = valid;
    raw["spacing"] = json!(0.01);
    let mut captured = PresetParameters::decode_value(&raw).unwrap();
    for _ in 0..8 {
        let text = encode_canonical(&captured).unwrap();
        assert!(text.contains("\"spacing\": 0.01"));
        captured = serde_json::from_str(&text).unwrap();
        assert_eq!(captured.spacing, 0.01_f32);
        captured.validate().unwrap();
    }
    let mut bad = captured.clone();
    bad.flow = f32::NAN;
    assert!(bad.validate().is_err());
}

#[test]
fn preset_parameter_format_rejects_missing_color_code_and_unknown_fields() {
    let valid = snapshot().value().unwrap();
    for key in PARAMETER_KEYS {
        let mut raw = valid.clone();
        raw.as_object_mut().unwrap().remove(key);
        assert!(PresetParameters::decode_value(&raw).is_err());
    }
    for (key, value) in [
        ("color", json!("#FFFFFF")),
        ("background_color", json!("#000000")),
        ("handler", json!("native.command")),
        ("flow", json!("0.5")),
        ("opacity_mode", json!("future")),
    ] {
        let mut raw = valid.clone();
        raw[key] = value;
        assert!(PresetParameters::decode_value(&raw).is_err());
    }
}

#[test]
fn preset_library_ids_versions_and_allocators_are_strict() {
    let valid = BrushPresetLibrary {
        version: 1,
        next_id: 3,
        items: vec![preset(1), preset(2)],
    };
    valid.validate().unwrap();
    for change in 0..7 {
        let mut invalid = valid.clone();
        match change {
            0 => invalid.version = 2,
            1 => invalid.next_id = 0,
            2 => invalid.next_id = u32::MAX as u64 + 2,
            3 => invalid.items.swap(0, 1),
            4 => invalid.items[1].id = 1,
            5 => invalid.items[0].id = 0,
            _ => invalid.next_id = 2,
        }
        assert!(invalid.validate().is_err());
        assert!(BrushPresetLibrary::decode_value(&canonical_value(&invalid).unwrap()).is_err());
    }
    let mut raw = canonical_value(&valid).unwrap();
    raw["future"] = json!(true);
    assert!(BrushPresetLibrary::decode_value(&raw).is_err());
    let mut raw = canonical_value(&valid).unwrap();
    raw["items"][0]["code"] = json!("ignored?");
    assert!(BrushPresetLibrary::decode_value(&raw).is_err());
}

#[test]
fn preset_library_deletion_never_recycles_ids_and_limits_are_bounded() {
    let mut library = BrushPresetLibrary::default();
    for id in 1..=MAX_PRESETS {
        assert_eq!(
            library
                .insert(
                    format!("Tip {id}"),
                    "painting".into(),
                    "brush".into(),
                    snapshot()
                )
                .unwrap(),
            id as u64
        );
    }
    let full = library.clone();
    assert!(library
        .insert(
            "Overflow".into(),
            "painting".into(),
            "brush".into(),
            snapshot()
        )
        .is_err());
    assert_eq!(library, full);
    assert!(library.remove(17).unwrap());
    assert!(!library.remove(999).unwrap());
    assert_eq!(library.next_id, 129);
    assert_eq!(
        library
            .insert(
                "Fresh".into(),
                "painting".into(),
                "eraser".into(),
                snapshot()
            )
            .unwrap(),
        129
    );
    let mut exhausted = BrushPresetLibrary {
        version: 1,
        next_id: u32::MAX as u64,
        items: vec![],
    };
    assert_eq!(
        exhausted
            .insert("Last".into(), "painting".into(), "brush".into(), snapshot())
            .unwrap(),
        u32::MAX as u64
    );
    exhausted.remove(u32::MAX as u64).unwrap();
    assert!(!exhausted.is_default());
    let before = exhausted.clone();
    assert!(exhausted
        .insert(
            "No reuse".into(),
            "painting".into(),
            "brush".into(),
            snapshot()
        )
        .is_err());
    assert_eq!(exhausted, before);
}

#[test]
fn preset_missing_providers_are_storable_but_identifiers_and_utf8_names_remain_bounded() {
    let source = preset(7);
    source.validate().unwrap();
    assert_eq!(
        UserPreset::decode_value(&canonical_value(&source).unwrap()).unwrap(),
        source
    );
    for change in 0..6 {
        let mut invalid = source.clone();
        match change {
            0 => invalid.name = " ".into(),
            1 => invalid.name = "bad\nname".into(),
            2 => invalid.name = "界".repeat(171),
            3 => invalid.owner = "UpperCase".into(),
            4 => invalid.tool_id = "../not/a/tool".into(),
            _ => invalid.tool_id = "x".repeat(97),
        }
        assert!(invalid.validate().is_err());
    }
    let mut boundary = source;
    boundary.name = "a".repeat(512);
    boundary.validate().unwrap();
}

#[test]
fn preset_bundle_is_data_only_preserves_file_order_and_rejects_duplicates_future_and_extra_code() {
    let bundle = BrushPresetBundle::from_items(vec![preset(9), preset(2)]).unwrap();
    let text = bundle.encode().unwrap();
    let decoded = BrushPresetBundle::decode(&text).unwrap();
    assert_eq!(decoded, bundle);
    assert_eq!(
        decoded.items.iter().map(|p| p.id).collect::<Vec<_>>(),
        [9, 2]
    );
    let valid = canonical_value(&bundle).unwrap();
    for change in 0..6 {
        let mut raw = valid.clone();
        match change {
            0 => raw["format"] = json!("other.brushes"),
            1 => raw["version"] = json!(2),
            2 => raw["items"][1]["id"] = json!(9),
            3 => raw["actions"] = json!([]),
            4 => raw["items"][0]["assets"] = json!(["image.png"]),
            _ => {
                raw.as_object_mut().unwrap().remove("version");
            }
        }
        assert!(BrushPresetBundle::decode(&raw.to_string()).is_err());
    }
    assert!(BrushPresetBundle::decode(&(text + &" ".repeat(MAX_BUNDLE_BYTES))).is_err());
    let oversized =
        json!({"format":BUNDLE_FORMAT,"version":1,"items":vec![Value::Null;MAX_PRESETS+1]});
    assert!(BrushPresetBundle::decode_value(&oversized)
        .unwrap_err()
        .contains("128"));
}

#[test]
fn preset_settings_omit_legacy_default_and_preserve_explicit_empty_and_allocator_after_delete() {
    let mut settings=Settings::decode(r##"{"custom":{"keep":42},"painting":{"flow":0.25,"opacity_mode":"stroke","spacing":0.01},"keybindings":{"file.save":"Ctrl+F12"}}"##,PathBuf::new()).unwrap();
    assert!(settings.brush_presets.is_default());
    assert!(serde_json::from_str::<Value>(&settings.encode().unwrap())
        .unwrap()
        .get("brush_presets")
        .is_none());
    let id = settings
        .brush_presets
        .insert(
            "名前 · 이름".into(),
            "unavailable".into(),
            "unavailable.ink".into(),
            snapshot(),
        )
        .unwrap();
    let copy = Settings::decode(&settings.encode().unwrap(), PathBuf::new()).unwrap();
    assert_eq!(copy.brush_presets, settings.brush_presets);
    let value: Value = serde_json::from_str(&copy.encode().unwrap()).unwrap();
    assert_eq!(value["custom"], json!({"keep":42}));
    assert_eq!(value["keybindings"]["file.save"], json!(["Ctrl+F12"]));
    assert_eq!(copy.painting, settings.painting);
    settings.brush_presets.remove(id).unwrap();
    let text = settings.encode().unwrap();
    let reloaded = Settings::decode(&text, PathBuf::new()).unwrap();
    assert!(reloaded.brush_presets.items.is_empty());
    assert_eq!(reloaded.brush_presets.next_id, 2);
    let explicit = Settings::decode(
        r#"{"brush_presets":{"version":1,"next_id":1,"items":[]}}"#,
        PathBuf::new(),
    )
    .unwrap();
    assert!(serde_json::from_str::<Value>(&explicit.encode().unwrap())
        .unwrap()
        .get("brush_presets")
        .is_some());
}

#[test]
fn preset_settings_reject_original_out_of_range_numeric_literals_before_any_conversion() {
    let library = BrushPresetLibrary {
        version: 1,
        next_id: 2,
        items: vec![preset(1)],
    };
    let mut value = json!({"brush_presets":canonical_value(&library).unwrap()});
    value["brush_presets"]["items"][0]["parameters"]["spacing"] = json!(0.009999999999);
    assert!(Settings::decode(&value.to_string(), PathBuf::new()).is_err());
    value["brush_presets"]["items"][0]["parameters"]["spacing"] = json!(0.01);
    Settings::decode(&value.to_string(), PathBuf::new()).unwrap();
}

#[test]
fn preset_profile_encode_cannot_publish_a_file_its_decoder_would_reject_as_oversize() {
    let raw = json!({"artist_notes":"x".repeat(1024*1024-64)}).to_string();
    let settings = Settings::decode(&raw, PathBuf::new()).unwrap();
    assert!(settings.encode().unwrap_err().contains("1 MB"));
    assert_eq!(
        settings.editor_document().unwrap()["artist_notes"],
        "x".repeat(1024 * 1024 - 64)
    );
}

#[test]
fn preset_profile_rebase_adopts_latest_unknown_roots_without_replacing_typed_preferences() {
    let mut current=Settings::decode(r#"{"ui":{"theme":"warm"},"painting":{"flow":0.25},"keybindings":{"file.save":"Ctrl+F12"},"custom":{"old":1}}"#,PathBuf::from("current.json")).unwrap();
    current
        .brush_presets
        .insert(
            "Keep local typed cache".into(),
            "painting".into(),
            "brush".into(),
            snapshot(),
        )
        .unwrap();
    let latest=Settings::decode(r#"{"ui":{"theme":"light"},"painting":{"flow":0.75},"custom":{"latest":42},"external":{"keep":true}}"#,PathBuf::from("latest.json")).unwrap();
    let presets = current.brush_presets.clone();
    current.rebase_original(&latest);
    let value: Value = serde_json::from_str(&current.encode().unwrap()).unwrap();
    assert_eq!(value["custom"], json!({"latest":42}));
    assert_eq!(value["external"], json!({"keep":true}));
    assert_eq!(value["ui"]["theme"], "warm");
    assert_eq!(value["painting"]["flow"], 0.25);
    assert_eq!(value["keybindings"]["file.save"], json!(["Ctrl+F12"]));
    assert_eq!(current.brush_presets, presets);
    assert_eq!(current.path, PathBuf::from("current.json"));
}
