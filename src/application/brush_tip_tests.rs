//! Scalar tip geometry shares native drawing, parameters, presets and outcomes.
use super::editing_tests::{call, check};
use super::*;
use crate::{
    configuration::Settings,
    engine::{
        document::Composition,
        raster_masks::PlaneView,
        replay::Journal,
        selection::{Mask, SelectionState},
        Document,
    },
    plugins::Registry,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc};
const W: usize = 7;
const H: usize = 7;
const COLOR: u32 = 0x194f93;

fn fixture() -> Session {
    let mut s = Session::new(
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
    )
    .unwrap();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    s
}
fn configure(s: &mut Session, extra: Value) {
    let mut args = json!({"radius":2.5,"opacity":1,"hardness":1,"flow":1,"opacity_mode":"dab",
        "color":"#194F93","stabilizer":0,"pressure_size":false,"pressure_opacity":false,
        "spacing":0.11,"tip_shape":"square","tip_roundness":0.5,"tip_angle":0});
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    call(s, "brush.set", args);
}
fn dab(s: &mut Session) {
    call(
        s,
        "canvas.stroke",
        json!({"points":[{"x":3.5,"y":3.5,"pressure":1,"pen":true,"time":100}]}),
    );
}
fn raw_mask(s: &Session, id: u64) -> Vec<u8> {
    let p = s.doc.mask_plane(id).unwrap();
    (0..W * H).map(|i| p.sample(i)).collect()
}
#[derive(Debug, PartialEq)]
struct Content {
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    masks: Vec<(u64, Vec<u8>)>,
    surface: Vec<u32>,
    active: usize,
}
fn content(s: &Session) -> Content {
    Content {
        composition: s.doc.composition(),
        pixels: s.doc.layers.iter().map(|l| l.pixels.clone()).collect(),
        masks: s
            .doc
            .mask_state()
            .items
            .iter()
            .map(|m| (m.id, raw_mask(s, m.id)))
            .collect(),
        surface: s.doc.composite.clone(),
        active: s.doc.active,
    }
}
fn square() -> Vec<u32> {
    (0..W * H)
        .map(|i| {
            let (x, y) = (i % W, i / W);
            let a = if (1..=5).contains(&x) {
                match y {
                    2 | 4 => 191,
                    3 => 255,
                    _ => 0,
                }
            } else {
                0
            };
            if a == 0 {
                0
            } else {
                (a << 24) | COLOR
            }
        })
        .collect()
}
fn quarter<T: Copy>(source: &[T]) -> Vec<T> {
    (0..W * H)
        .map(|i| source[(H - 1 - i % W) * W + i / W])
        .collect()
}
fn outcome(s: &mut Session) {
    check(s);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut j = Journal::import(&bytes).unwrap();
    assert_eq!(j.verify(None).unwrap()["valid"], true);
    let state = j.preview(j.entries.len() - 1).unwrap();
    assert_eq!(state.metadata.parameters, s.parameters);
    let mut doc = Document::new(1, 1);
    state.restore_document(&mut doc);
    assert!(doc.renderer.is_none());
    assert_eq!(doc.composition(), s.doc.composition());
    assert_eq!(doc.composite, s.doc.composite);
    for m in &s.doc.mask_state().items {
        let plane = doc.mask_plane(m.id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(),
            raw_mask(s, m.id)
        );
    }
}

#[test]
fn brush_tip_defaults_are_explicit_public_state_but_omitted_from_legacy_wire() {
    let mut s = fixture();
    let legacy = serde_json::to_value(Parameters::default()).unwrap();
    for field in ["tip_shape", "tip_roundness", "tip_angle"] {
        assert!(legacy.get(field).is_none());
    }
    let state = s.parameter_state();
    assert_eq!(state["tip_shape"], "round");
    assert_eq!(state["tip_roundness"], 1.);
    assert_eq!(state["tip_angle"], 0.);
    let resource = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool","plugins.registry"]}),
    );
    let tool = &resource["resources"]["painting.tool"]["value"];
    assert_eq!(tool["capabilities"]["tip_geometry"], true);
    let native = resource["resources"]["plugins.registry"]["value"]["native"]
        .as_array()
        .unwrap();
    assert_eq!(
        native.iter().find(|p| p["id"] == "painting").unwrap()["tool_options"]["tip_geometry"],
        true
    );
    assert_eq!(
        native.iter().find(|p| p["id"] == "compositing").unwrap()["tool_options"]["tip_geometry"],
        false
    );
    for field in ["tip_shape", "tip_roundness", "tip_angle"] {
        assert_eq!(tool["parameters"][field], state[field]);
    }
    let description = call(&mut s, "app.describe", json!({}));
    let schema = &description["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "painting.tool")
        .unwrap()["outputSchema"];
    crate::plugins::validate(schema, tool).unwrap();
    let nodes = s.registry.node_catalog();
    let params = &nodes["action.brush.set"].parameters_schema["properties"];
    assert_eq!(params["tip_shape"]["enum"], json!(["round", "square"]));
    assert_eq!(params["tip_roundness"]["minimum"], 0.25);
    assert_eq!(params["tip_angle"]["minimum"], -180);
    assert_eq!(params["tip_angle"]["maximum"], 180);
    assert_eq!(
        nodes["action.brush.angle.adjust"].parameters_schema["properties"]["delta_degrees"]
            ["maximum"],
        180
    );
    for bad in [
        json!({"tip_shape":"triangle"}),
        json!({"tip_roundness":0.249999999}),
        json!({"tip_roundness":1.00000001}),
        json!({"tip_angle":180.000001}),
        json!({"tip_angle":-180.000001}),
    ] {
        let before = s.parameters.clone();
        assert!(s.apply(Action::new("brush.set", bad)).is_err());
        assert_eq!(s.parameters, before);
    }
    outcome(&mut s);
}

#[test]
fn brush_tip_hard_square_matches_independent_rectangle_stencil_and_quarter_turn() {
    for angle in [0, 90, -90] {
        let mut s = fixture();
        configure(&mut s, json!({"tip_angle":angle}));
        let before = content(&s);
        let history = s.doc.history_bytes();
        dab(&mut s);
        let expected = if angle == 0 {
            square()
        } else {
            quarter(&square())
        };
        assert_eq!(s.doc.layers[0].pixels, expected);
        assert_eq!(s.doc.composite, expected);
        assert!(s.doc.history_bytes() > history);
        let painted = content(&s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), before);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), painted);
        outcome(&mut s);
    }
    // Circle rotation is immaterial; retain the exact legacy radial pixels.
    let mut a = fixture();
    configure(&mut a, json!({"tip_shape":"round","tip_roundness":1}));
    dab(&mut a);
    let mut b = fixture();
    configure(
        &mut b,
        json!({"tip_shape":"round","tip_roundness":1,"tip_angle":137}),
    );
    dab(&mut b);
    assert_eq!(a.doc.layers[0].pixels, b.doc.layers[0].pixels);
}

#[test]
fn brush_tip_square_eraser_and_alpha_lock_preserve_existing_channel_contracts() {
    for erase in [false, true] {
        let mut s = fixture();
        let original = vec![0x80335988; W * H];
        s.doc.layers[0].pixels = original.clone();
        s.doc.replay_tiles.insert((0, 0));
        s.doc.invalidate();
        s.replay_boundary().unwrap();
        if erase {
            call(&mut s, "tool.select", json!({"tool":"eraser"}));
        } else {
            call(&mut s, "layer.set", json!({"alpha_locked":true}));
        }
        configure(&mut s, json!({}));
        let before = content(&s);
        dab(&mut s);
        let expected: Vec<_> = square()
            .iter()
            .map(|ink| {
                let amount = ink >> 24;
                if erase {
                    (original[0] & 0xffffff) | (((128 * (255 - amount) + 127) / 255) << 24)
                } else {
                    [0, 8, 16].into_iter().fold(0x80000000, |out, shift| {
                        out | (((((COLOR >> shift) & 255) * amount
                            + ((original[0] >> shift) & 255) * (255 - amount)
                            + 127)
                            / 255)
                            << shift)
                    })
                }
            })
            .collect();
        assert_eq!(s.doc.layers[0].pixels, expected);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), before);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, expected);
        outcome(&mut s);
    }
}

#[test]
fn brush_tip_raw_a8_uses_q16_influence_and_selection_once_preserving_effect_metadata() {
    let mut s = fixture();
    let id = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":id,"density":173,"feather_radius_px":1,"linked":false}),
    );
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(W, H, vec![128; W * H]).unwrap()),
            previous: Some(Mask::new(W, H, vec![79; W * H]).unwrap()),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    configure(
        &mut s,
        json!({"color":"#000000","flow":0.25,"opacity_mode":"stroke","tip_angle":90}),
    );
    let metadata = s.doc.mask_state().clone();
    let selection = s.doc.selection().clone();
    let pixels = s.doc.layers[0].pixels.clone();
    dab(&mut s);
    let expected: Vec<_> = quarter(&square())
        .iter()
        .map(|p| match p >> 24 {
            255 => 223,
            191 => 231,
            _ => 255,
        })
        .collect();
    assert_eq!(raw_mask(&s, id), expected);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.mask_state(), &metadata);
    assert_eq!(s.doc.selection(), &selection);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw_mask(&s, id), vec![255; W * H]);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(raw_mask(&s, id), expected);
    outcome(&mut s);
}

fn sample(phase: PointerPhase, i: u32) -> Pointer {
    Pointer {
        phase,
        x: 1.5 + (i % 4) as f32 * 0.6,
        y: 2.5 + (i % 3) as f32 * 0.2,
        pressure: 0.2 + (i % 5) as f32 * 0.2,
        time: 100 + i * 3,
        pen: true,
        eraser: false,
        tool_override: Some(ToolKind::Brush),
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
fn brush_tip_pressure_stabilization_and_chunked_typed_input_match_shared_stroke_execution() {
    let parameters = json!({"tip_shape":"round","tip_roundness":0.5,"tip_angle":30,"radius":3,"stabilizer":0.65,
        "pressure_size":true,"pressure_opacity":true,"pressure_gamma":1.5,"minimum_radius":0.25,"flow":0.25,"opacity_mode":"stroke"});
    let mut typed = fixture();
    configure(&mut typed, parameters.clone());
    let before = content(&typed);
    typed.pointer(sample(PointerPhase::Down, 0)).unwrap();
    for i in 1..270 {
        typed.pointer(sample(PointerPhase::Move, i)).unwrap();
        if i % 31 == 0 {
            call(&mut typed, "canvas.state", json!({}));
            typed.replay_boundary().unwrap();
        }
    }
    let samples = transcript(&mut typed);
    assert_eq!(samples.len(), 270);
    assert!(typed.stroke.active());
    typed.pointer(sample(PointerPhase::Up, 269)).unwrap();
    assert_eq!(transcript(&mut typed).len(), 271);
    let mut shared = fixture();
    configure(&mut shared, parameters);
    let points: Vec<_> = (0..270)
        .map(|i| {
            let mut p = serde_json::to_value(sample(PointerPhase::Move, i)).unwrap();
            p.as_object_mut().unwrap().remove("phase");
            p
        })
        .collect();
    call(&mut shared, "canvas.stroke", json!({"points":points}));
    assert_eq!(typed.doc.layers[0].pixels, shared.doc.layers[0].pixels);
    assert!(typed.doc.layers[0].pixels.iter().any(|p| p >> 24 != 0));
    call(&mut typed, "history.undo", json!({}));
    assert_eq!(content(&typed), before);
    call(&mut typed, "history.redo", json!({}));
    outcome(&mut typed);
    outcome(&mut shared);
}

#[test]
fn brush_tip_missing_capability_rejects_before_down_and_angle_adjust_preserves_live_contact() {
    for masked in [false, true] {
        let mut s = fixture();
        let id = if masked {
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
        let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
        let features = native.replace_tool_options_features_for_test(0).unwrap();
        native.replace_tool_options_features_for_test(features & !abi::TOOL_OPTION_TIP_GEOMETRY);
        configure(&mut s, json!({}));
        let before = content(&s);
        let revision = s.revision;
        let history = s.doc.history_bytes();
        assert_eq!(
            s.pointer(sample(PointerPhase::Down, 0)).unwrap_err().code,
            "tool_options_unsupported"
        );
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(s.pointer_checkpoint.is_none() && !s.stroke.active());
        if let Some(id) = id {
            assert!(matches!(
                s.doc.mask_plane(id).unwrap(),
                PlaneView::Uniform(255)
            ));
        }
        configure(
            &mut s,
            json!({"tip_shape":"round","tip_roundness":1,"tip_angle":0}),
        );
        s.pointer(sample(PointerPhase::Down, 0)).unwrap();
        s.doc.compose();
        let before = content(&s);
        let parameters = s.parameters.clone();
        let samples = transcript(&mut s);
        assert_eq!(
            s.apply(Action::new(
                "brush.angle.adjust",
                json!({"delta_degrees":15})
            ))
            .unwrap_err()
            .code,
            "tool_options_unsupported"
        );
        assert_eq!(content(&s), before);
        assert_eq!(s.parameters, parameters);
        assert!(s.stroke.active());
        assert_eq!(transcript(&mut s), samples);
        s.pointer(sample(PointerPhase::Up, 0)).unwrap();
        s.tool_instances.clear();
        Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .replace_tool_options_features_for_test(features);
        outcome(&mut s);
    }
}

#[test]
fn brush_tip_angle_actions_wrap_and_atomic_journal_failures_restore_parameters_without_pixel_history(
) {
    let mut s = fixture();
    configure(&mut s, json!({"tip_angle":179}));
    let doc = content(&s);
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let result = call(&mut s, "brush.angle.adjust", json!({"delta_degrees":1}));
    assert_eq!(result["parameters"]["tip_angle"], -180.);
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(s.parameters.tip_angle, -180.);
    for (action, expected) in [
        ("brush.angle.counterclockwise", 179.),
        ("brush.angle.clockwise_coarse", -166.),
        ("brush.angle.counterclockwise_coarse", 179.),
    ] {
        call(&mut s, action, json!({}));
        assert_eq!(s.parameters.tip_angle, expected);
    }
    assert_eq!(content(&s), doc);
    assert_eq!(s.doc.history_bytes(), history);
    let params = s.parameters.clone();
    let public = s.state();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"angle","action":"brush.angle.adjust","args":{"delta_degrees":15}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.parameters, params);
    assert_eq!(s.state(), public);
    assert_eq!(content(&s), doc);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "brush.angle.adjust",
            json!({"delta_degrees":15})
        ))
        .is_err());
    assert_eq!(s.parameters, params);
    assert_eq!(s.state(), public);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    s.replay_error = None;
    outcome(&mut s);
}

#[test]
fn brush_tip_presets_round_trip_optional_geometry_and_legacy_twelve_value_records_reset_it() {
    let mut s = fixture();
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-tips/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("settings.json");
    let settings = Settings::defaults(path.clone());
    settings.save().unwrap();
    s.settings_path = Some(path.clone());
    s.load_profile_presets(&settings).unwrap();
    configure(&mut s, json!({"tip_angle":30}));
    let mut parameters = s.parameter_state();
    parameters.as_object_mut().unwrap().remove("color");
    parameters
        .as_object_mut()
        .unwrap()
        .remove("background_color");
    let id = call(
        &mut s,
        "brush.presets.upsert",
        json!({"name":"Flat nib","owner":"painting","tool_id":"brush","parameters":parameters}),
    )["preset_id"]
        .as_u64()
        .unwrap();
    let bundle = directory.join("tip-bundle.json");
    call(
        &mut s,
        "brush.presets.export",
        json!({"path":bundle,"preset_ids":[id]}),
    );
    let wire: Value = serde_json::from_slice(&std::fs::read(&bundle).unwrap()).unwrap();
    assert_eq!(wire["items"][0]["parameters"]["tip_shape"], "square");
    assert_eq!(wire["items"][0]["parameters"]["tip_roundness"], 0.5);
    assert_eq!(wire["items"][0]["parameters"]["tip_angle"], 30.);
    let imported = call(&mut s, "brush.presets.import", json!({"path":bundle}))["imported_ids"][0]
        .as_u64()
        .unwrap();
    for key in ["tip_shape", "tip_roundness", "tip_angle"] {
        parameters.as_object_mut().unwrap().remove(key);
    }
    assert_eq!(parameters.as_object().unwrap().len(), 12);
    let legacy = call(
        &mut s,
        "brush.presets.upsert",
        json!({"name":"Legacy round","owner":"painting","tool_id":"brush","parameters":parameters}),
    )["preset_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "brush.presets.apply", json!({"preset_id":legacy}));
    assert_eq!(s.parameters.tip_shape, TipShape::Round);
    assert_eq!(s.parameters.tip_roundness, 1.);
    assert_eq!(s.parameters.tip_angle, 0.);
    call(&mut s, "brush.presets.apply", json!({"preset_id":imported}));
    assert_eq!(s.parameters.tip_shape, TipShape::Square);
    assert_eq!(s.parameters.tip_angle, 30.);
    let fg = s.parameters.color.clone();
    let bg = s.parameters.background_color.clone();
    call(&mut s, "tool.select", json!({"tool":"brush"}));
    assert_eq!(s.parameters.tip_shape, TipShape::Square);
    call(&mut s, "tool.select", json!({"tool":"ink"}));
    assert_eq!(s.parameters.tip_shape, TipShape::Round);
    assert_eq!(s.parameters.tip_roundness, 1.);
    assert_eq!(s.parameters.tip_angle, 0.);
    assert_eq!(s.parameters.color, fg);
    assert_eq!(s.parameters.background_color, bg);
    outcome(&mut s);
}

#[test]
fn brush_tip_project_and_dll_free_replay_keep_nondefault_geometry_and_exact_painted_outcomes() {
    let mut s = fixture();
    configure(&mut s, json!({"tip_angle":90}));
    dab(&mut s);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-tips-project/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("flat-tip.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(content(&opened), content(&s));
    assert_eq!(opened.parameters.tip_shape, TipShape::Square);
    assert_eq!(opened.parameters.tip_roundness, 0.5);
    assert_eq!(opened.parameters.tip_angle, 90.);
    outcome(&mut s);
    outcome(&mut opened);
}
