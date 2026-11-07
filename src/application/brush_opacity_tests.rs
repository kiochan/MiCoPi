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
