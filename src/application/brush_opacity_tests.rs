//! New brush options preserve legacy wire defaults and share public validation.
use super::*;
use crate::{
    configuration::{PaintingPreferences, Settings},
    plugins::Registry,
};
use serde_json::{json, Value};
use std::path::PathBuf;

#[test]
fn brush_opacity_legacy_parameter_bytes_decode_without_new_options_and_roundtrip_exactly() {
    let legacy = r##"{"radius":6.0,"opacity":1.0,"hardness":0.85,"color":"#333743","background_color":"#FFFFFF","stabilizer":0.35,"pressure_size":true,"pressure_opacity":false,"pressure_gamma":1.0,"minimum_radius":0.08}"##;
    let parameters: Parameters = serde_json::from_str(legacy).unwrap();
    assert_eq!(parameters, Parameters::default());
    assert_eq!(parameters.flow, 1.);
    assert_eq!(parameters.opacity_mode, OpacityMode::Dab);
    assert_eq!(serde_json::to_string(&parameters).unwrap(), legacy);
    let changed = parameters
        .patched(&json!({"flow":0.25,"opacity_mode":"stroke"}))
        .unwrap();
    let encoded = serde_json::to_value(&changed).unwrap();
    assert_eq!(encoded["flow"], 0.25);
    assert_eq!(encoded["opacity_mode"], "stroke");
    assert_eq!(
        serde_json::from_value::<Parameters>(encoded).unwrap(),
        changed
    );
    let restored = changed
        .patched(&json!({"flow":1,"opacity_mode":"dab"}))
        .unwrap();
    assert_eq!(serde_json::to_string(&restored).unwrap(), legacy);
}

#[test]
fn brush_opacity_options_validate_independently_and_are_explicit_in_public_state() {
    let base = Parameters::default();
    for patch in [
        json!({"flow":-0.01}),
        json!({"flow":1.01}),
        json!({"flow":null}),
        json!({"flow":"0.5"}),
        json!({"opacity_mode":"legacy"}),
        json!({"opacity_mode":null}),
        json!({"opacity_mode":true}),
    ] {
        assert!(base.patched(&patch).is_err(), "{patch}");
    }
    for flow in [0., 0.25, 1.] {
        for mode in ["dab", "stroke"] {
            let changed = base
                .patched(&json!({"flow":flow,"opacity_mode":mode}))
                .unwrap();
            assert_eq!(changed.flow, flow);
            assert_eq!(changed.opacity, base.opacity);
            assert_eq!(changed.pressure_opacity, base.pressure_opacity);
        }
    }
    let mut session = Session::new(Registry::bundled()).unwrap();
    assert_eq!(session.state()["parameters"]["flow"], 1.);
    assert_eq!(session.state()["parameters"]["opacity_mode"], "dab");
    session
        .apply(Action::new(
            "brush.set",
            json!({"flow":0.25,"opacity_mode":"stroke","opacity":0.4}),
        ))
        .unwrap();
    session
        .apply(Action::new("brush.set", json!({"radius":3.})))
        .unwrap();
    assert_eq!(session.state()["parameters"]["flow"], 0.25);
    assert_eq!(session.state()["parameters"]["opacity_mode"], "stroke");
    assert_eq!(session.parameters.opacity, 0.4);
}

#[test]
fn brush_opacity_builtin_presets_reset_modes_but_partial_third_party_presets_inherit() {
    let mut session = Session::new(Registry::bundled()).unwrap();
    session
        .apply(Action::new("tool.select", json!({"tool":"glaze_marker"})))
        .unwrap();
    assert_eq!(session.parameters.opacity_mode, OpacityMode::Stroke);
    assert_eq!(session.parameters.flow, 0.25);
    assert_eq!(session.parameters.opacity, 0.4);
    for preset in ["pencil", "ink", "paint", "soft", "marker", "erase"] {
        session
            .apply(Action::new(
                "brush.set",
                json!({"flow":0.2,"opacity_mode":"stroke"}),
            ))
            .unwrap();
        session
            .apply(Action::new("tool.select", json!({"tool":preset})))
            .unwrap();
        assert_eq!(session.parameters.flow, 1., "{preset}");
        assert_eq!(
            session.parameters.opacity_mode,
            OpacityMode::Dab,
            "{preset}"
        );
    }
    let mut custom = session.registry.tools["brush"].clone();
    custom.id = "test.partial_opacity".into();
    custom.parameters = json!({"radius":9.});
    session.registry.tools.insert(custom.id.clone(), custom);
    session
        .apply(Action::new(
            "brush.set",
            json!({"flow":0.2,"opacity_mode":"stroke"}),
        ))
        .unwrap();
    session
        .apply(Action::new(
            "tool.select",
            json!({"tool":"test.partial_opacity"}),
        ))
        .unwrap();
    assert_eq!(session.parameters.flow, 0.2);
    assert_eq!(session.parameters.opacity_mode, OpacityMode::Stroke);
    assert_eq!(session.parameters.radius, 9.);
}

#[test]
fn brush_opacity_settings_validate_roundtrip_unknown_root_fields_and_apply_without_changing_color()
{
    let old = Settings::decode(
        r#"{"version":1,"extension":{"keep":7},"painting":{"stabilizer":0.5}}"#,
        PathBuf::new(),
    )
    .unwrap();
    assert_eq!(old.painting.flow, 1.);
    assert_eq!(old.painting.opacity_mode, OpacityMode::Dab);
    for painting in [
        json!({"flow":-1}),
        json!({"flow":1.01}),
        json!({"opacity_mode":"invalid"}),
        json!({"flow":null}),
    ] {
        assert!(
            Settings::decode(&json!({"painting":painting}).to_string(), PathBuf::new()).is_err()
        );
    }
    let mut settings = old;
    settings.painting.flow = 0.25;
    settings.painting.opacity_mode = OpacityMode::Stroke;
    let saved = settings.encode().unwrap();
    let restored = Settings::decode(&saved, PathBuf::new()).unwrap();
    assert_eq!(restored.painting, settings.painting);
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["extension"]["keep"],
        7
    );
    let mut parameters = Parameters::default()
        .patched(&json!({"color":"#BCDDEF","opacity":0.6}))
        .unwrap();
    restored.painting.apply(&mut parameters);
    assert_eq!(parameters.color, "#BCDDEF");
    assert_eq!(parameters.opacity, 0.6);
    assert_eq!(PaintingPreferences::from(&parameters), restored.painting);
    let mut invalid = restored.painting;
    invalid.flow = f32::NAN;
    assert!(invalid.validate().is_err());
}

#[test]
fn brush_minimum_spacing_keeps_canonical_legacy_bytes_and_strict_external_bounds() {
    let minimum = Parameters::default()
        .patched(&json!({"spacing":0.01}))
        .unwrap();
    assert_eq!(minimum.spacing, 0.01_f32);
    let legacy_minimum = r##"{"radius":6.0,"opacity":1.0,"hardness":0.85,"color":"#333743","background_color":"#FFFFFF","stabilizer":0.35,"pressure_size":true,"pressure_opacity":false,"pressure_gamma":1.0,"minimum_radius":0.08,"spacing":0.01}"##;
    assert_eq!(serde_json::to_string(&minimum).unwrap(), legacy_minimum);
    let full_patch = serde_json::to_value(&minimum).unwrap();
    assert_eq!(full_patch["spacing"], json!(0.01));
    assert_eq!(minimum.patched(&full_patch).unwrap(), minimum);
    assert_eq!(
        minimum.patched(&json!({"radius":9})).unwrap().spacing,
        0.01_f32
    );
    PaintingPreferences::from(&minimum).validate().unwrap();
    // These literals are below the public 1% floor, including one that rounds
    // back to the accepted f32 value after deserialization.
    for spacing in [0.009, 0.0099999999, 0.01_f32 as f64] {
        assert!(
            Parameters::default()
                .patched(&json!({"spacing":spacing}))
                .is_err(),
            "{spacing:.17}"
        );
        assert!(
            Settings::decode(
                &json!({"painting":{"spacing":spacing}}).to_string(),
                PathBuf::new()
            )
            .is_err(),
            "{spacing:.17}"
        );
    }
    let mut invalid = PaintingPreferences::from(&minimum);
    invalid.spacing = f32::from_bits(0.01_f32.to_bits() - 1);
    assert!(invalid.validate().is_err());
    // Other values keep the established f32-to-Value representation.
    let other = Parameters::default()
        .patched(&json!({"spacing":0.7}))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&other).unwrap()["spacing"],
        json!(0.7_f32)
    );
}

#[test]
fn brush_minimum_spacing_survives_actions_recording_public_schema_settings_projects_and_replay() {
    use super::editing_tests::{call, check, session};
    use crate::engine::{replay::Journal, Document};
    let mut s = session();
    call(
        &mut s,
        "brush.set",
        json!({"spacing":0.01,"opacity":0.4,"flow":0.25,"opacity_mode":"stroke",
        "radius":2,"hardness":1,"stabilizer":0,"pressure_size":false}),
    );
    assert!(
        s.replay_error.is_none(),
        "accepting 1% spacing must leave recording usable"
    );
    let revision = s.revision;
    let entries = s.journal.as_ref().unwrap().entries.len();
    call(&mut s, "brush.set", json!({"radius":3}));
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), entries + 1);
    let full_patch = serde_json::to_value(&s.parameters).unwrap();
    assert_eq!(full_patch["spacing"], json!(0.01));
    call(&mut s, "brush.set", full_patch);
    assert_eq!(s.parameters.spacing, 0.01_f32);
    let context = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool"]}),
    );
    let tool = &context["resources"]["painting.tool"]["value"];
    assert_eq!(tool["parameters"]["spacing"], json!(0.01));
    assert_eq!(s.state()["parameters"]["spacing"], json!(0.01));
    crate::plugins::validate(&pipeline::resource_schema("painting.tool"), tool).unwrap();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-spacing/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let settings_path = directory.join("settings.json");
    call(
        &mut s,
        "settings.update",
        json!({"path":settings_path,
        "document":{"version":1,"extension":{"keep":7},"painting":{"spacing":0.01,"flow":0.25,"opacity_mode":"stroke"}}}),
    );
    let settings = Settings::load(settings_path.clone()).unwrap();
    assert_eq!(settings.painting.spacing, 0.01_f32);
    assert_eq!(settings.painting.flow, 0.25);
    let encoded = settings.encode().unwrap();
    let saved: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(saved["painting"]["spacing"], json!(0.01));
    assert_eq!(saved["extension"]["keep"], 7);
    let reread = call(&mut s, "settings.read", json!({"path":settings_path}));
    assert_eq!(reread["painting"]["spacing"], json!(0.01));
    assert_eq!(
        Settings::decode(&encoded, PathBuf::new()).unwrap().painting,
        settings.painting
    );
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":3,"y":3},{"x":4,"y":3}]}),
    );
    let painted = s.doc.layers[0].pixels.clone();
    assert!(painted.iter().any(|p| *p != 0));
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, painted);
    check(&mut s);
    let path = directory.join("minimum.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.parameters, s.parameters);
    assert_eq!(opened.doc.layers[0].pixels, painted);
    check(&mut opened);
    let mut journal = Journal::import(&s.journal.as_mut().unwrap().bytes(None).unwrap()).unwrap();
    let last = journal.entries.len() - 1;
    let state = journal.preview(last).unwrap();
    assert_eq!(state.metadata.parameters, s.parameters);
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.layers[0].pixels, painted);
    assert_eq!(restored.composition(), s.doc.composition());
}
