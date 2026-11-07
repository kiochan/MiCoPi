//! Actual painting-DLL contacts, independent integer influence references and outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    raster_masks::{PlaneView, SamplePlane},
    replay::Journal,
    selection::{Mask, SelectionState},
    Document,
};
use crate::plugins::Registry;
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

const W: usize = 7;
const H: usize = 5;
const COLOR: u32 = 0x194f93;

fn fresh(flow: f32) -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    assert!(
        s.registry.native_plugins["painting"].supports_stroke_opacity(),
        "stage matching painting DLL first"
    );
    call(
        &mut s,
        "brush.set",
        json!({"radius":20,"hardness":1,"opacity":0.4,"flow":flow,
        "opacity_mode":"stroke","color":"#194F93","spacing":0.01,"stabilizer":0,
        "pressure_size":false,"pressure_opacity":false}),
    );
    s
}
fn point(phase: PointerPhase, step: u32, pressure: f32) -> Pointer {
    Pointer {
        phase,
        x: 2. + (step % 2) as f32 * 0.5,
        y: 2.,
        pressure,
        time: 100 + step,
        pen: true,
        eraser: false,
        tool_override: None,
        view_x: None,
        view_y: None,
    }
}
fn contact(s: &mut Session, pressures: &[f32], cancel: bool) {
    for (i, &pressure) in pressures.iter().enumerate() {
        s.pointer(point(
            if i == 0 {
                PointerPhase::Down
            } else {
                PointerPhase::Move
            },
            i as u32,
            pressure,
        ))
        .unwrap();
    }
    let step = pressures.len() as u32 - 1;
    s.pointer(point(
        if cancel {
            PointerPhase::Cancel
        } else {
            PointerPhase::Up
        },
        step,
        *pressures.last().unwrap(),
    ))
    .unwrap();
    assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
}
/// Rational fixed-point reference; the chosen full-interior dabs have shape 1.
fn influence(caps: &[f64], flow: f64, selection: u8) -> u32 {
    let amount = (65535. * flow).round() as u64;
    let mut weight = 0u64;
    for &cap in caps {
        let cap = (65535. * cap).round() as u64;
        weight += (cap.saturating_sub(weight) * amount + 32767) / 65535;
    }
    ((weight * u64::from(selection) + 32767) / 65535) as u32
}
fn over(dst: u32, color: u32, alpha: u32) -> u32 {
    if alpha == 0 {
        return dst;
    }
    let da = dst >> 24;
    let out = alpha + (da * (255 - alpha) + 127) / 255;
    let mut result = out << 24;
    for shift in [0, 8, 16] {
        let src = (color >> shift) & 255;
        let before = (dst >> shift) & 255;
        let value = (src * alpha * 255 + before * da * (255 - alpha) + out * 127) / (out * 255);
        result |= value.min(255) << shift;
    }
    result
}
fn atop(dst: u32, color: u32, alpha: u32) -> u32 {
    if dst >> 24 == 0 {
        return dst;
    }
    [0, 8, 16].into_iter().fold(dst & 0xff000000, |out, shift| {
        out | ((((color >> shift) & 255) * alpha + ((dst >> shift) & 255) * (255 - alpha) + 127)
            / 255)
            << shift
    })
}
fn seed(s: &mut Session, pixels: Vec<u32>) {
    s.doc.layers[0].pixels = pixels;
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
}
fn samples(s: &Session, id: u64) -> Vec<u8> {
    let view = s.doc.mask_plane(id).unwrap();
    (0..W * H).map(|i| view.sample(i)).collect()
}
fn stream(s: &mut Session) -> Vec<Value> {
    let events = s.journal.as_mut().unwrap().events(0, 256).unwrap();
    let mut samples: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    samples.extend(
        s.replay_samples
            .iter()
            .map(|p| serde_json::to_value(p).unwrap()),
    );
    samples
}
fn outcome(s: &mut Session) {
    check(s);
    let index = s.journal.as_ref().unwrap().entries.len() - 1;
    let mut journal = Journal::import(&s.journal.as_mut().unwrap().bytes(None).unwrap()).unwrap();
    let preview = journal.preview(index).unwrap();
    assert!(preview.matches_document(&s.doc));
    assert_eq!(preview.metadata.parameters, s.parameters);
    let mut restored = Document::new(1, 1);
    preview.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for mask in &s.doc.mask_state().items {
        let actual = restored.mask_plane(mask.id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| actual.sample(i)).collect::<Vec<_>>(),
            samples(s, mask.id)
        );
    }
}

#[test]
fn stroke_opacity_repeated_dabs_cap_one_contact_but_two_contacts_accumulate() {
    let mut s = fresh(1.);
    let before = s.doc.layers[0].pixels.clone();
    let rev = s.revision;
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":2,"y":2},{"x":2.5,"y":2},{"x":2,"y":2}]}),
    );
    // Existing typed sample increments plus the enclosing Stroke action bump.
    assert_eq!(s.revision, rev + 5);
    let first = over(0, COLOR, 102);
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == first));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, before);
    call(&mut s, "history.redo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == first));
    contact(&mut s, &[1., 1., 1.], false);
    let second = over(first, COLOR, 102);
    assert_eq!(second >> 24, 163);
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == second));
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == first));
    outcome(&mut s);
}

#[test]
fn stroke_opacity_flow_pressure_floor_and_zero_use_independent_fixed_point_reference() {
    for (flow, pressures, pressure_opacity, floor, caps) in [
        (0., vec![1., 1., 1.], false, 0., vec![0.4; 3]),
        (0.25, vec![1., 1., 1.], false, 0., vec![0.4; 3]),
        (1., vec![0.5, 1., 0.1], true, 0., vec![0.2, 0.4, 0.04]),
        (0.25, vec![0., 0.2, 0.2], true, 0.25, vec![0., 0.16, 0.16]),
    ] {
        let mut s = fresh(flow);
        call(
            &mut s,
            "brush.set",
            json!({"pressure_opacity":pressure_opacity,"minimum_opacity":floor}),
        );
        contact(&mut s, &pressures, false);
        let alpha = influence(&caps, f64::from(flow), 255);
        assert!(
            s.doc.layers[0]
                .pixels
                .iter()
                .all(|p| *p == over(0, COLOR, alpha)),
            "flow={flow}, caps={caps:?}"
        );
        outcome(&mut s);
    }
}

#[test]
fn stroke_opacity_selection_is_applied_once_after_accumulation_and_survives_undo() {
    let mut s = fresh(0.25);
    let selected: Vec<u8> = (0..W * H).map(|i| [0, 64, 128, 255][i % 4]).collect();
    let selection = SelectionState {
        current: Some(Mask::new(W, H, selected.clone()).unwrap()),
        previous: None,
    };
    s.doc.set_selection(selection.clone()).unwrap();
    s.replay_boundary().unwrap();
    contact(&mut s, &[1., 1., 1.], false);
    let expected: Vec<_> = selected
        .iter()
        .map(|&m| over(0, COLOR, influence(&[0.4; 3], 0.25, m)))
        .collect();
    assert_eq!(s.doc.layers[0].pixels, expected);
    assert_eq!(*s.doc.selection(), selection);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    assert_eq!(*s.doc.selection(), selection);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, expected);
    outcome(&mut s);
}

#[test]
fn stroke_opacity_recomposes_erase_and_alpha_locked_color_against_contact_origin() {
    for erase in [false, true] {
        let mut s = fresh(0.25);
        let origin: Vec<_> = (0..W * H)
            .map(|i| [0x00123456, 0x80335988, 0xff112233][i % 3])
            .collect();
        seed(&mut s, origin.clone());
        if erase {
            call(&mut s, "tool.select", json!({"tool":"eraser"}));
        } else {
            call(&mut s, "layer.set", json!({"alpha_locked":true}));
        }
        // Restate the tested options after tool selection, independent of presets.
        call(
            &mut s,
            "brush.set",
            json!({"radius":20,"hardness":1,"opacity":0.4,"flow":0.25,
            "opacity_mode":"stroke","spacing":0.01,"stabilizer":0,"pressure_size":false,"color":"#194F93"}),
        );
        contact(&mut s, &[1., 1., 1.], false);
        let a = influence(&[0.4; 3], 0.25, 255);
        let expected: Vec<_> = origin
            .iter()
            .map(|&p| {
                if erase {
                    (p & 0xffffff) | ((((p >> 24) * (255 - a) + 127) / 255) << 24)
                } else {
                    atop(p, COLOR, a)
                }
            })
            .collect();
        assert_eq!(s.doc.layers[0].pixels, expected);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, origin);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, expected);
        outcome(&mut s);
    }
}

#[test]
fn stroke_opacity_raw_a8_preserves_effects_and_rgba_for_brush_and_reveal_eraser() {
    for erase in [false, true] {
        let mut s = fresh(0.25);
        seed(&mut s, vec![0xffe5c798; W * H]);
        let id = call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":"layer","id":1},"source":"reveal_all","select":true}),
        )["mask_id"]
            .as_u64()
            .unwrap();
        let origin: Vec<u8> = (0..W * H).map(|i| (i * 7) as u8).collect();
        s.doc
            .replace_mask_plane(id, SamplePlane::Dense(origin.clone()))
            .unwrap();
        s.replay_boundary().unwrap();
        call(
            &mut s,
            "mask.set",
            json!({"mask_id":id,"density":173,"feather_radius_px":1,"linked":false}),
        );
        let composition = s.doc.composition();
        if erase {
            call(&mut s, "tool.select", json!({"tool":"eraser"}));
        }
        call(
            &mut s,
            "brush.set",
            json!({"radius":20,"hardness":1,"opacity":0.4,"flow":0.25,"opacity_mode":"stroke", "spacing":0.01,"stabilizer":0,"pressure_size":false,"color":"#000000"}),
        );
        contact(&mut s, &[1., 1., 1.], false);
        let a = influence(&[0.4; 3], 0.25, 255);
        let value = if erase { 255 } else { 0 };
        let expected: Vec<_> = origin
            .iter()
            .map(|&p| ((u32::from(p) * (255 - a) + value * a + 127) / 255) as u8)
            .collect();
        assert_eq!(samples(&s, id), expected);
        assert_eq!(s.doc.composition(), composition);
        assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0xffe5c798));
        assert!(matches!(s.doc.mask_plane(id).unwrap(), PlaneView::Dense(_)));
        call(&mut s, "history.undo", json!({}));
        assert_eq!(samples(&s, id), origin);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(samples(&s, id), expected);
        outcome(&mut s);
    }
}

#[test]
fn stroke_opacity_chunked_observation_keeps_native_contact_pressure_and_one_undo() {
    let mut s = fresh(0.25);
    let mut expected = Vec::new();
    for i in 0..300u32 {
        let sample = point(
            if i == 0 {
                PointerPhase::Down
            } else {
                PointerPhase::Move
            },
            i,
            0.2 + (i % 7) as f32 * 0.1,
        );
        s.pointer(sample).unwrap();
        expected.push(serde_json::to_value(sample).unwrap());
        if i == 120 || i == 270 {
            call(&mut s, "canvas.state", json!({}));
            call(
                &mut s,
                "pipeline.run",
                json!({"steps":[{"id":"observe","action":"canvas.state","args":{}}]}),
            );
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            assert_eq!(stream(&mut s), expected);
        }
    }
    let up = point(PointerPhase::Up, 299, 0.);
    s.pointer(up).unwrap();
    expected.push(serde_json::to_value(up).unwrap());
    assert_eq!(stream(&mut s), expected);
    assert_eq!(
        s.doc.layers[0].pixels,
        vec![over(0, COLOR, influence(&vec![0.4; 300], 0.25, 255)); W * H]
    );
    let painted = s.doc.layers[0].pixels.clone();
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, painted);
    outcome(&mut s);
}

#[test]
fn stroke_opacity_unsupported_options_reject_before_down_but_old_defaults_still_paint() {
    let mut registry = Registry::bundled();
    let plugin = Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let previous = plugin.replace_tool_options_features_for_test(0).unwrap();
    plugin.replace_tool_options_features_for_test(previous & !abi::TOOL_OPTION_STROKE_OPACITY);
    let mut s = Session::new(registry).unwrap();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    call(
        &mut s,
        "brush.set",
        json!({"radius":2,"stabilizer":0,"pressure_size":false}),
    );
    contact(&mut s, &[1.], false);
    for patch in [
        json!({"flow":0.25,"opacity_mode":"dab"}),
        json!({"flow":1,"opacity_mode":"stroke"}),
    ] {
        call(&mut s, "brush.set", patch);
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        let error = s
            .apply(Action::new(
                "canvas.stroke",
                json!({"points":[{"x":2,"y":2}]}),
            ))
            .unwrap_err();
        assert_eq!(error.code, "tool_options_unsupported");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
    }
    outcome(&mut s);
}

#[test]
fn stroke_opacity_atomic_and_journal_failures_restore_history_pixels_parameters_and_archive() {
    let mut s = fresh(0.25);
    contact(&mut s, &[1., 1., 1.], false);
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"options","action":"brush.set","args":{"flow":0.5}},
            {"id":"paint","action":"canvas.stroke","args":{"points":[{"x":2,"y":2},{"x":2.5,"y":2}]}},
            {"id":"fail","action":"layer.select","args":{"index":15}}
        ]}),
    );
    assert_eq!(r["ok"], false);
    assert_eq!(r["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let events = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.stroke",
            json!({"points":[{"x":2,"y":2},{"x":2.5,"y":2}]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(s.journal.as_mut().unwrap().bytes(None).unwrap(), bytes);
    assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
    s.replay_error = None; // Fixture-only recovered storage; production remains faulted.
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    outcome(&mut s);
}

#[test]
fn stroke_opacity_cancel_finishes_and_project_archive_retains_nondefault_parameters() {
    let mut s = fresh(0.25);
    contact(&mut s, &[1., 1., 1.], true);
    let expected = s.doc.layers[0].pixels.clone();
    assert_eq!(
        expected,
        vec![over(0, COLOR, influence(&[0.4; 3], 0.25, 255)); W * H]
    );
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/stroke-opacity");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!(
        "{}-{}.mcp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    call(&mut s, "document.save", json!({"path":path}));
    let mut opened = fresh(1.);
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(opened.doc.layers[0].pixels, expected);
    assert_eq!(opened.parameters, s.parameters);
    let replay = opened.replay_journal(false).unwrap();
    let index = replay.entries.len() - 1;
    let preview = replay.preview(index).unwrap();
    assert_eq!(preview.metadata.parameters.flow, 0.25);
    assert_eq!(
        preview.metadata.parameters.opacity_mode,
        OpacityMode::Stroke
    );
    outcome(&mut s);
}

#[test]
fn stroke_opacity_schema_nodes_and_atomic_workflow_use_the_shared_brush_contract() {
    let mut s = fresh(1.);
    let catalog = call(&mut s, "node.catalog", json!({}));
    let node = catalog["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "action.brush.set")
        .unwrap();
    assert_eq!(
        node["parameters_schema"]["properties"]["flow"]["maximum"],
        1
    );
    assert_eq!(
        node["parameters_schema"]["properties"]["opacity_mode"]["enum"],
        json!(["dab", "stroke"])
    );
    let revision = s.revision;
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.brush.set","position":[0,0],"parameters":{"flow":0.25,"opacity_mode":"stroke"}},
        {"id":2,"type":"action.canvas.stroke","position":[400,0],"parameters":{"points":[{"x":2,"y":2},{"x":2.5,"y":2},{"x":2,"y":2}]}}
    ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"}]});
    let report = call(&mut s, "workflow.run", json!({"graph":graph,"atomic":true}));
    assert_eq!(report["ok"], true);
    assert!(s.revision > revision);
    assert_eq!(s.parameters.flow, 0.25);
    assert_eq!(s.parameters.opacity_mode, OpacityMode::Stroke);
    assert_eq!(
        s.doc.layers[0].pixels,
        vec![over(0, COLOR, influence(&[0.4; 3], 0.25, 255)); W * H]
    );
    outcome(&mut s);
}

#[test]
fn stroke_opacity_public_tool_parameters_match_strict_resource_and_node_schemas_during_contact() {
    for patch in [json!({}), json!({"flow":0.25,"opacity_mode":"stroke"})] {
        let mut s = session();
        call(&mut s, "brush.set", patch);
        let wire = serde_json::to_value(&s.parameters).unwrap();
        // Cold persistence keeps the old omitted defaults; public resources are complete.
        assert!(wire.get("spacing").is_none() && wire.get("minimum_opacity").is_none());
        if s.parameters.opacity_mode == OpacityMode::Dab {
            assert!(wire.get("flow").is_none() && wire.get("opacity_mode").is_none());
        }
        s.pointer(point(PointerPhase::Down, 0, 0.6)).unwrap();
        let revision = s.revision;
        let before = stream(&mut s);
        let description = call(&mut s, "app.describe", json!({}));
        let resource = description["resources"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "painting.tool")
            .unwrap();
        let schema = &resource["outputSchema"];
        let catalog = call(&mut s, "node.catalog", json!({}));
        let node = catalog["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "state.painting.tool")
            .unwrap();
        assert_eq!(node["output_schema"], *schema);
        let parameters_port = node["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "parameters")
            .unwrap();
        assert_eq!(parameters_port["data_type"], "object");
        assert_eq!(
            parameters_port["schema"],
            schema["properties"]["parameters"]
        );
        let observed = call(
            &mut s,
            "context.read",
            json!({"resources":["painting.tool"]}),
        );
        assert_eq!(observed["resources"]["painting.tool"]["ok"], true);
        let tool = &observed["resources"]["painting.tool"]["value"];
        crate::plugins::validate(schema, tool).unwrap();
        assert_eq!(tool["parameters"], s.state()["parameters"]);
        assert_eq!(tool["parameters"], s.parameter_state());
        assert_eq!(tool["parameters"]["flow"], s.parameters.flow);
        assert_eq!(
            tool["parameters"]["opacity_mode"],
            json!(s.parameters.opacity_mode)
        );
        assert_eq!(tool["parameters"]["spacing"], json!(s.parameters.spacing));
        assert_eq!(tool["parameters"]["minimum_opacity"], 0.);
        assert_eq!(
            tool["capabilities"],
            json!({"flow":true,"stroke_opacity":true,"tip_geometry":true})
        );
        for field in ["flow", "opacity_mode", "spacing", "minimum_opacity"] {
            let mut incomplete = tool.clone();
            incomplete["parameters"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                crate::plugins::validate(schema, &incomplete).is_err(),
                "missing {field}"
            );
        }
        let mut unknown = tool.clone();
        unknown["parameters"]["unregistered_option"] = json!(0.5);
        assert!(crate::plugins::validate(schema, &unknown).is_err());
        assert_eq!(s.revision, revision);
        assert_eq!(stream(&mut s), before);
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        s.pointer(point(PointerPhase::Up, 0, 0.6)).unwrap();
        outcome(&mut s);
    }
}

#[test]
fn stroke_opacity_discovery_resolves_fallback_presets_and_distinguishes_tool_from_provider() {
    let mut s = fresh(0.25);
    let mut preset = s.registry.tools["brush"].clone();
    preset.id = "test.capped_fallback".into();
    preset.parameters = json!({"flow":0.25,"opacity_mode":"stroke"});
    s.registry.tools.insert(preset.id.clone(), preset);
    assert!(!s.registry.native_tools.contains_key("test.capped_fallback"));
    call(
        &mut s,
        "tool.select",
        json!({"tool":"test.capped_fallback"}),
    );
    let observed = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool","plugins.registry"]}),
    );
    assert_eq!(
        observed["resources"]["painting.tool"]["value"]["capabilities"],
        json!({"flow":true,"stroke_opacity":true,"tip_geometry":true})
    );
    let native = observed["resources"]["plugins.registry"]["value"]["native"]
        .as_array()
        .unwrap();
    assert_eq!(
        native.iter().find(|p| p["id"] == "painting").unwrap()["tool_options"],
        json!({"flow":true,"stroke_opacity":true,"tip_geometry":true})
    );
    assert_eq!(
        native.iter().find(|p| p["id"] == "compositing").unwrap()["tool_options"],
        json!({"flow":false,"stroke_opacity":false,"tip_geometry":false})
    );
    // Exercise the real fallback, rather than accepting a discovery-only helper answer.
    contact(&mut s, &[1., 1., 1.], false);
    assert_eq!(
        s.doc.layers[0].pixels,
        vec![over(0, COLOR, influence(&[0.4; 3], 0.25, 255)); W * H]
    );
    for tool in ["picker", "hand", "zoom"] {
        call(&mut s, "tool.select", json!({"tool":tool}));
        let observed = call(
            &mut s,
            "context.read",
            json!({"resources":["painting.tool","plugins.registry"]}),
        );
        assert_eq!(
            observed["resources"]["painting.tool"]["value"]["capabilities"],
            json!({"flow":false,"stroke_opacity":false,"tip_geometry":false}),
            "{tool}"
        );
        let native = observed["resources"]["plugins.registry"]["value"]["native"]
            .as_array()
            .unwrap();
        assert_eq!(
            native.iter().find(|p| p["id"] == "painting").unwrap()["tool_options"]
                ["stroke_opacity"],
            true
        );
    }
    call(&mut s, "tool.select", json!({"tool":"eraser"}));
    let observed = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool"]}),
    );
    assert_eq!(
        observed["resources"]["painting.tool"]["value"]["capabilities"]["stroke_opacity"],
        true
    );
    outcome(&mut s);
}

#[test]
fn stroke_opacity_missing_feature_discovery_is_false_without_hiding_complete_parameter_state() {
    let mut registry = Registry::bundled();
    let plugin = Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let features = plugin.replace_tool_options_features_for_test(0).unwrap();
    plugin.replace_tool_options_features_for_test(features & !abi::TOOL_OPTION_STROKE_OPACITY);
    let mut s = Session::new(registry).unwrap();
    let revision = s.revision;
    let observed = call(
        &mut s,
        "context.read",
        json!({"resources":["painting.tool","plugins.registry"]}),
    );
    let tool = &observed["resources"]["painting.tool"]["value"];
    assert_eq!(
        tool["capabilities"],
        json!({"flow":false,"stroke_opacity":false,"tip_geometry":true})
    );
    assert_eq!(tool["parameters"]["flow"], 1.);
    assert_eq!(tool["parameters"]["opacity_mode"], "dab");
    crate::plugins::validate(&pipeline::resource_schema("painting.tool"), tool).unwrap();
    let native = observed["resources"]["plugins.registry"]["value"]["native"]
        .as_array()
        .unwrap();
    assert_eq!(
        native.iter().find(|p| p["id"] == "painting").unwrap()["tool_options"],
        json!({"flow":false,"stroke_opacity":false,"tip_geometry":true})
    );
    assert_eq!(s.revision, revision);
    assert!(!s.stroke.active());
}
