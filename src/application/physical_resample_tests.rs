//! Physical image sizing shares pixel sampling, transactional history and outcome replay.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document_info::{Density, DensityUnit, Info},
    replay::Journal,
    selection::{Mask, SelectionState},
};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn source() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":2}));
    s.doc.replace_region(
        0,
        0,
        4,
        2,
        &[
            0x00112233, 0x80aabbcc, 0xff102030, 0xff405060, 0xff708090, 0, 0x44114488, 0xffabcdef,
        ],
    );
    call(&mut s, "layer.rename", json!({"name":"Ink 墨"}));
    call(
        &mut s,
        "layer.set",
        json!({"visible":false,"alpha_locked":true,"opacity":0.5}),
    );
    call(&mut s, "layer.add", json!({}));
    s.doc.replace_region(
        0,
        0,
        4,
        2,
        &[
            0xff000000, 0xffffffff, 0xff112233, 0xff445566, 0x803377aa, 0, 0x00aabbcc, 0xff778899,
        ],
    );
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(4, 2, vec![0, 32, 64, 128, 255, 180, 90, 0]).unwrap()),
            previous: Some(Mask::new(4, 2, vec![255, 0, 200, 100, 50, 150, 250, 30]).unwrap()),
        })
        .unwrap();
    s.doc
        .set_import_info(Info::with_png_density(Density {
            xppu: 1000,
            yppu: 2000,
            unit: DensityUnit::Meter,
        }))
        .unwrap();
    s.replay_boundary().unwrap();
    s
}
fn output() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/physical-resample-units");
    std::fs::create_dir_all(&folder).unwrap();
    folder.join(format!(
        "{}-{}.mcp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn physical_resample_plan_uses_exact_axes_pixel_aspect_and_bounded_buffer_estimates() {
    let mut s = source();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    let events = s.journal.as_ref().unwrap().entries.len();
    // Planning remains available without a native sampling provider.
    s.registry.native_plugins.clear();
    let plan = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":10,"height":20,"unit":"mm","lock_aspect":false}),
    );
    assert_eq!(
        (plan["width"].as_u64(), plan["height"].as_u64()),
        (Some(10), Some(40))
    );
    assert_eq!(plan["source_width"], 4);
    assert_eq!(plan["source_height"], 2);
    assert_eq!(plan["method"], "bilinear");
    assert_eq!(plan["identity"], false);
    assert_eq!(plan["layer_count"], 2);
    assert_eq!(plan["mask_count"], 2);
    assert_eq!(plan["estimated_layer_bytes"], 10 * 40 * 2 * 4);
    assert_eq!(plan["estimated_mask_bytes"], 10 * 40 * 2);
    assert_eq!(plan["estimated_composite_bytes"], 10 * 40 * 4);
    assert_eq!(plan["estimated_staged_bytes"], 10 * 40 * 10);
    assert_eq!(plan["document_info"], s.doc.info().state(10, 40));
    assert_eq!(plan["revision"], s.revision);
    assert_eq!(plan["drawing"], false);
    let height = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"height":10,"unit":"mm","method":"area"}),
    );
    // Non-square physical density never replaces the source pixel ratio.
    assert_eq!(height["width"], 40);
    assert_eq!(height["height"], 20);
    let identity = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":4,"height":1,"unit":"mm","lock_aspect":false}),
    );
    assert_eq!(identity["identity"], true);
    assert_eq!(identity["estimated_staged_bytes"], 0);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
}

#[test]
fn physical_resample_nearest_preserves_all_layer_words_masks_and_one_undoable_outcome() {
    let mut physical = source();
    let mut pixel = source();
    let original = physical.doc.composition();
    let raw: Vec<_> = physical
        .doc
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect();
    let masks = physical.doc.selection().clone();
    let events = physical.journal.as_ref().unwrap().entries.len();
    call(
        &mut physical,
        "canvas.image_size",
        json!({"width":8,"unit":"mm","method":"nearest"}),
    );
    call(
        &mut pixel,
        "canvas.image_size",
        json!({"width":8,"method":"nearest"}),
    );
    assert_eq!((physical.doc.width, physical.doc.height), (8, 4));
    for (layer, original) in physical.doc.layers.iter().zip(&raw) {
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(layer.pixels[y * 8 + x], original[(y / 2) * 4 + x / 2]);
            }
        }
    }
    for (scaled, original) in [
        (
            physical.doc.selection().current.as_ref().unwrap(),
            masks.current.as_ref().unwrap(),
        ),
        (
            physical.doc.selection().previous.as_ref().unwrap(),
            masks.previous.as_ref().unwrap(),
        ),
    ] {
        for y in 0..4 {
            for x in 0..8 {
                assert_eq!(
                    scaled.coverage()[y * 8 + x],
                    original.coverage()[(y / 2) * 4 + x / 2]
                );
            }
        }
    }
    assert_eq!(physical.doc.composition(), pixel.doc.composition());
    assert_eq!(
        codec::png(&mut physical.doc).unwrap(),
        codec::png(&mut pixel.doc).unwrap()
    );
    assert_eq!(
        physical.doc.info(),
        Info::with_png_density(Density {
            xppu: 1000,
            yppu: 2000,
            unit: DensityUnit::Meter
        })
    );
    assert_eq!(
        physical.doc.composition().layer_metadata,
        original.layer_metadata
    );
    assert_eq!(physical.doc.composition().graph, original.graph);
    assert!(!physical.doc.layers[0].visible);
    assert!(physical.doc.alpha_locked(0));
    assert_eq!(physical.doc.layers[0].opacity, 128);
    assert_eq!(physical.journal.as_ref().unwrap().entries.len(), events + 1);
    check(&mut physical);
    call(&mut physical, "history.undo", json!({}));
    assert_eq!(physical.doc.composition(), original);
    for (layer, original) in physical.doc.layers.iter().zip(&raw) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(physical.doc.selection(), &masks);
    call(&mut physical, "history.redo", json!({}));
    check(&mut physical);
    // Both other samplers use the same normalized geometry and inherited metadata.
    for method in ["bilinear", "area"] {
        let mut physical = source();
        let mut pixel = source();
        call(
            &mut physical,
            "canvas.image_size",
            json!({"height":1.5,"unit":"mm","method":method}),
        );
        call(
            &mut pixel,
            "canvas.image_size",
            json!({"height":3,"method":method}),
        );
        assert_eq!(physical.doc.composition(), pixel.doc.composition());
        for (a, b) in physical.doc.layers.iter().zip(&pixel.doc.layers) {
            assert_eq!(a.pixels, b.pixels);
        }
        assert_eq!(physical.doc.selection(), pixel.doc.selection());
    }
}

#[test]
fn physical_resample_unknown_density_requires_explicit_resolution_but_pixels_remain_available() {
    for density in [
        Density {
            xppu: 4,
            yppu: 3,
            unit: DensityUnit::Unspecified,
        },
        Density {
            xppu: 0,
            yppu: 2000,
            unit: DensityUnit::Meter,
        },
        Density {
            xppu: 1000,
            yppu: 0,
            unit: DensityUnit::Meter,
        },
    ] {
        let mut s = source();
        let info = Info::with_png_density(density);
        s.doc.set_import_info(info).unwrap();
        s.replay_boundary().unwrap();
        let state = s.state();
        let history = s.doc.history_bytes();
        let composition = s.doc.composition();
        for id in ["canvas.image_size.plan", "canvas.image_size"] {
            assert!(s
                .apply(Action::new(id, json!({"width":8,"unit":"mm"})))
                .is_err());
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.history_bytes(), history);
        }
        let plan = call(&mut s, "canvas.image_size.plan", json!({"width":8}));
        assert_eq!(plan["width"], 8);
        assert_eq!(plan["height"], 4);
        assert_eq!(plan["document_info"]["physical_size_known"], false);
        call(&mut s, "canvas.resolution", json!({"resolution_ppi":100}));
        let plan = call(
            &mut s,
            "canvas.image_size.plan",
            json!({"width":2.54,"unit":"mm"}),
        );
        assert_eq!(plan["width"], 10);
        assert_eq!(plan["height"], 5);
        call(
            &mut s,
            "canvas.image_size",
            json!({"width":2.54,"unit":"mm","method":"nearest"}),
        );
        assert_eq!(s.doc.info().resolution_ppi, 100);
        assert!(s.doc.info().resolution_explicit);
        assert_eq!(s.doc.info().png_density, None);
        call(&mut s, "history.undo", json!({}));
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.info(), info);
        assert_eq!((s.doc.width, s.doc.height), (4, 2));
        check(&mut s);
    }
    let mut s = session();
    let plan = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":1,"unit":"in"}),
    );
    assert_eq!(plan["width"], 72);
    assert_eq!(plan["height"], 48);
    assert_eq!(plan["document_info"].get("resolution_explicit"), None);
    assert_eq!(s.doc.info(), Info::default());
}

#[test]
fn physical_resample_rounding_invalid_literals_and_stale_requests_are_non_mutating() {
    let mut s = session();
    call(
        &mut s,
        "canvas.new",
        json!({"width":2,"height":1,"resolution_ppi":1}),
    );
    let half = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":0.5,"unit":"in"}),
    );
    assert_eq!(half["width"], 1);
    assert_eq!(half["height"], 1);
    let edge = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":4096.4999,"unit":"in"}),
    );
    assert_eq!(edge["width"], 4096);
    assert_eq!(edge["height"], 2048);
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    for args in [
        json!({}),
        json!({"width":2,"height":1}),
        json!({"width":2,"lock_aspect":false}),
        json!({"width":0}),
        json!({"width":-1}),
        json!({"width":2.5}),
        json!({"width":0.4999,"unit":"in"}),
        json!({"width":4096.5,"unit":"in"}),
        json!({"width":null}),
        json!({"width":2,"height":null}),
        json!({"width":2,"unit":null}),
        json!({"width":2,"unit":"cm"}),
        json!({"width":2,"lock_aspect":null}),
        json!({"width":2,"method":null}),
        json!({"width":2,"method":"unknown"}),
        json!({"width":2,"resolution_ppi":100}),
        json!({"width":2,"scale_percent":100}),
        json!({"width":2,"extra":true}),
    ] {
        for id in ["canvas.image_size.plan", "canvas.image_size"] {
            assert!(
                s.apply(Action::new(id, args.clone())).is_err(),
                "{id}: {args}"
            );
            assert_eq!(s.state(), before);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.history_bytes(), history);
        }
    }
    for args in [
        json!({"width":2.5}),
        json!({"width":2,"height":1}),
        json!({"width":2,"unit":"cm"}),
    ] {
        let result = s.apply(Action::new(
            "pipeline.run",
            json!({"atomic":false,"steps":[
                {"id":"rename","action":"layer.rename","args":{"name":"Must not run"}},
                {"id":"size","action":"canvas.image_size","args":args}
            ]}),
        ));
        assert!(result.is_err(), "{result:?}");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let mut stale = Action::new("canvas.image_size", json!({"width":4,"unit":"in"}));
    stale.expected_revision = Some(s.revision + 1);
    assert!(s.apply(stale).is_err());
    assert_eq!(s.state(), before);
    for number in ["NaN", "Infinity", "1e400"] {
        let wire = format!("{{\"action\":\"canvas.image_size\",\"args\":{{\"width\":{number}}}}}");
        assert!(serde_json::from_str::<Action>(&wire).is_err());
    }
    check(&mut s);
}

#[test]
fn physical_resample_plan_checks_layer_budget_without_allocating_and_identity_needs_no_sampler() {
    let mut s = source();
    for _ in 0..2 {
        call(&mut s, "layer.add", json!({}));
    }
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    let state = s.state();
    let plan = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":4096,"height":4096,"lock_aspect":false}),
    );
    assert_eq!(plan["estimated_layer_bytes"], 268435456u64);
    assert_eq!(plan["estimated_mask_bytes"], 33554432u64);
    assert_eq!(plan["estimated_composite_bytes"], 67108864u64);
    assert_eq!(plan["estimated_staged_bytes"], 301989888u64);
    assert_eq!(s.state(), state);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    call(&mut s, "layer.add", json!({}));
    let state = s.state();
    let history = s.doc.history_bytes();
    for id in ["canvas.image_size.plan", "canvas.image_size"] {
        assert!(s
            .apply(Action::new(
                id,
                json!({"width":4096,"height":4096,"lock_aspect":false})
            ))
            .is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let mut s = source();
    let composition = s.doc.composition();
    let raw: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let history = s.doc.history_bytes();
    let png = codec::png(&mut s.doc).unwrap();
    s.registry.native_plugins.clear();
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":4,"unit":"mm","method":"area"}),
    );
    assert_eq!(s.doc.composition(), composition);
    for (layer, original) in s.doc.layers.iter().zip(&raw) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
    assert_eq!(
        call(
            &mut s,
            "canvas.image_size.plan",
            json!({"width":8,"unit":"mm"})
        )["width"],
        8
    );
    assert!(s
        .apply(Action::new(
            "canvas.image_size",
            json!({"width":8,"unit":"mm"})
        ))
        .is_err());
    assert_eq!(s.doc.composition(), composition);
    for (layer, original) in s.doc.layers.iter().zip(&raw) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(s.doc.history_bytes(), history);
    check(&mut s);
}

#[test]
fn physical_resample_contact_plan_is_readonly_and_rejection_keeps_pressure_stroke_continuous() {
    let mut s = session();
    call(
        &mut s,
        "brush.set",
        json!({"radius":2,"stabilizer":0.35,"pressure_size":true,"pressure_opacity":true}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.2,"pen":true,"time":0}),
    );
    let state = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let samples = s.replay_samples.len();
    let plan = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":25.4,"unit":"mm"}),
    );
    assert_eq!(plan["drawing"], true);
    assert_eq!(plan["width"], 72);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(s.replay_samples.len(), samples);
    for args in [json!({"width":24}), json!({"width":25.4,"unit":"mm"})] {
        assert_eq!(
            s.apply(Action::new("canvas.image_size", args))
                .unwrap_err()
                .code,
            "stroke_active"
        );
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(s.is_drawing());
    }
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"move","x":6,"y":3,"pressure":1,"pen":true,"time":30}),
    );
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":10,"y":6,"pressure":0.4,"pen":true,"time":60}),
    );
    assert!(!s.is_drawing());
    assert_ne!(s.doc.layers[0].pixels, pixels);
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
}

#[test]
fn physical_resample_atomic_and_partial_append_failures_restore_raw_metadata_masks_and_history() {
    let mut s = source();
    let state = s.state();
    let composition = s.doc.composition();
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"size","action":"canvas.image_size","args":{"width":8,"unit":"mm","method":"nearest"}},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    for (layer, original) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.image_size",
            json!({"width":8,"unit":"mm","method":"nearest"})
        ))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    for (layer, original) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
}

#[test]
fn physical_resample_schema_workflow_projects_and_replay_share_normalized_outcomes() {
    let mut s = source();
    for id in ["canvas.image_size", "canvas.image_size.plan"] {
        let schema = s.registry.actions[id].schema();
        assert_eq!(schema["properties"]["width"]["type"], "number");
        for unit in ["px", "mm", "in"] {
            assert!(schema["properties"]["unit"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == unit));
        }
        assert!(schema["properties"].get("resolution_ppi").is_none());
        assert!(schema["properties"].get("scale_percent").is_none());
    }
    assert!(s.registry.actions["canvas.image_size.plan"].is_read_only());
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.canvas.image_size.plan"));
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let info = s.doc.info();
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.canvas.image_size.plan","position":[0,0],"parameters":{"width":8,"unit":"mm","method":"nearest"}},
        {"id":2,"type":"action.canvas.image_size","position":[400,0],"parameters":{"lock_aspect":false,"method":"nearest"}}
    ],"edges":[{"from":1,"output":"width","to":2,"input":"width"},{"from":1,"output":"height","to":2,"input":"height"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!((s.doc.width, s.doc.height), (4, 2));
    for (layer, original) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(s.doc.info(), info);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (4, 2));
    let before = s.journal.as_ref().unwrap().entries.len() - 1;
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (8, 4));
    assert_eq!(s.doc.info(), info);
    let path = output();
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let png = codec::png(&mut s.doc).unwrap();
    let raw: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    for (layer, original) in reopened.doc.layers.iter().zip(&raw) {
        assert_eq!(&layer.pixels, original);
    }
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), png);
    check(&mut reopened);
    // Playback interprets stored raster outcomes, independently of Session or DLL dispatch.
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let old = journal.preview(before).unwrap();
    assert_eq!((old.metadata.width, old.metadata.height), (4, 2));
    let last = journal.entries.len() - 1;
    assert_eq!(replay::png(&journal.preview(last).unwrap()).unwrap(), png);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn physical_resample_non_drawing_contact_is_preserved_when_mutation_is_rejected() {
    let mut s = source();
    let composition = s.doc.composition();
    let info = s.doc.info();
    let history = s.doc.history_bytes();
    let pixels: Vec<_> = s
        .doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    call(
        &mut s,
        "canvas.pointer",
        json!({
            "phase":"down","x":1,"y":1,"view_x":10,"view_y":20,
            "pressure":0.5,"pen":true,"time":0,"tool_override":"hand"
        }),
    );
    assert!(!s.is_drawing());
    assert!(s.is_panning());
    assert!(s.stroke.active());
    assert!(s.pointer_checkpoint.is_some());
    let state = s.state();
    let events = s.journal.as_ref().unwrap().entries.len();
    let samples = s.replay_samples.len();
    let plan = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":8,"unit":"mm"}),
    );
    assert_eq!(plan["drawing"], false);
    assert_eq!(
        (plan["width"].as_u64(), plan["height"].as_u64()),
        (Some(8), Some(4))
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(s.replay_samples.len(), samples);
    for args in [
        json!({"width":8,"unit":"mm"}),
        json!({"width":4,"unit":"mm"}),
    ] {
        assert_eq!(
            s.apply(Action::new("canvas.image_size", args))
                .unwrap_err()
                .code,
            "stroke_active"
        );
        assert_eq!(s.state(), state);
        assert!(s.stroke.active());
        assert!(s.pointer_checkpoint.is_some());
        assert_eq!(s.stroke.last_sample().unwrap().phase, PointerPhase::Down);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.info(), info);
        assert_eq!(s.doc.history_bytes(), history);
        for (layer, original) in s.doc.layers.iter().zip(&pixels) {
            assert_eq!(&layer.pixels, original);
        }
    }
    let before_move = s.view;
    call(
        &mut s,
        "canvas.pointer",
        json!({
            "phase":"move","x":2,"y":1,"view_x":40,"view_y":55,
            "pressure":0.7,"pen":true,"time":30
        }),
    );
    assert!(s.is_panning());
    assert_ne!(s.view, before_move);
    assert_eq!(s.stroke.last_sample().unwrap().phase, PointerPhase::Move);
    call(
        &mut s,
        "canvas.pointer",
        json!({
            "phase":"up","x":3,"y":1,"view_x":50,"view_y":60,
            "pressure":0,"pen":true,"time":60
        }),
    );
    assert!(!s.stroke.active());
    assert!(!s.is_drawing());
    assert!(!s.is_panning());
    assert!(s.pointer_checkpoint.is_none());
    assert_eq!((s.doc.width, s.doc.height), (4, 2));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.info(), info);
    assert_eq!(s.doc.history_bytes(), history);
    for (layer, original) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, original);
    }
    check(&mut s);
}
