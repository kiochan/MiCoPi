//! Layer organization is shared public state, transactional metadata and edit policy.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::replay::Journal;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn ids(result: &Value) -> Vec<u64> {
    result["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_u64().unwrap())
        .collect()
}
fn layers() -> Session {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "layer.rename", json!({"name":"Ink 墨 Ä"}));
    call(
        &mut s,
        "layer.set",
        json!({"pixel_locked":true,"color_label":"red"}),
    );
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.rename", json!({"name":"Paint 彩"}));
    call(
        &mut s,
        "layer.set",
        json!({"position_locked":true,"color_label":"blue","visible":false}),
    );
    call(&mut s, "layer.add", json!({}));
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"color_label":"green"}),
    );
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.rename", json!({"name":"ink outline"}));
    call(&mut s, "layer.set", json!({"color_label":"red"}));
    s
}
fn output() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/layer-organization-units");
    std::fs::create_dir_all(&folder).unwrap();
    folder.join(format!(
        "{}-{}.mcp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn layer_organization_query_combines_public_filters_without_editing_or_allocating_layer_pixels() {
    let mut s = layers();
    let all: Vec<_> = (0..4).map(|i| s.doc.layer_id(i)).collect();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    for (args, expected) in [
        (json!({}), all.clone()),
        (json!({"query":""}), all.clone()),
        (json!({"query":"INK"}), vec![all[0], all[3]]),
        (json!({"query":"ä"}), vec![all[0]]),
        (json!({"query":"墨"}), vec![all[0]]),
        (json!({"query":"layer 03"}), vec![all[2]]),
        (
            json!({"query":"ink","color_label":"red","visible":true,"locked":true,"pixel_locked":true,"position_locked":false,"alpha_locked":false}),
            vec![all[0]],
        ),
        (json!({"color_label":"red","locked":false}), vec![all[3]]),
        (
            json!({"visible":false,"position_locked":true}),
            vec![all[1]],
        ),
        (json!({"locked":true}), vec![all[0], all[1], all[2]]),
        (
            json!({"alpha_locked":true,"pixel_locked":false,"position_locked":false}),
            vec![all[2]],
        ),
        (json!({"query":"墨","color_label":"blue"}), vec![]),
    ] {
        let result = call(&mut s, "layer.query", args);
        assert_eq!(result["total"], 4);
        assert_eq!(result["matched"], expected.len());
        assert_eq!(ids(&result), expected);
        assert_eq!(result["revision"], s.revision);
        assert_eq!(result["drawing"], false);
        for row in result["layers"].as_array().unwrap() {
            let index = row["index"].as_u64().unwrap() as usize;
            assert_eq!(row, &state["layers"][index]);
            assert_eq!(row["active"], index == s.doc.active);
        }
    }
    let query = call(&mut s, "layer.query", json!({}));
    let context = call(&mut s, "context.read", json!({"resources":["core.layers"]}));
    assert_eq!(context["resources"]["core.layers"]["ok"], true);
    assert_eq!(context["resources"]["core.layers"]["value"], query);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
}

#[test]
fn layer_organization_invalid_queries_labels_and_conflicting_selectors_preserve_source() {
    let mut s = layers();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    for args in [
        json!({"query":null}),
        json!({"query":3}),
        json!({"query":"line\nbreak"}),
        json!({"query":"tab\t"}),
        json!({"query":"\u{0085}"}),
        json!({"query":"墨".repeat(171)}),
        json!({"query":"🙂".repeat(129)}),
        json!({"color_label":"purple"}),
        json!({"color_label":null}),
        json!({"visible":null}),
        json!({"locked":"true"}),
        json!({"pixel_locked":null}),
        json!({"position_locked":null}),
        json!({"alpha_locked":null}),
        json!({"index":0}),
        json!({"layer_id":1}),
        json!({"extra":true}),
    ] {
        assert!(
            s.apply(Action::new("layer.query", args.clone())).is_err(),
            "{args}"
        );
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
    }
    assert_eq!(
        call(&mut s, "layer.query", json!({"query":"🙂".repeat(128)}))["matched"],
        0
    );
    let other = s.doc.layer_id(1);
    for args in [
        json!({"color_label":"purple"}),
        json!({"color_label":null}),
        json!({"pixel_locked":null}),
        json!({"position_locked":null}),
        json!({"index":0,"layer_id":other,"pixel_locked":false}),
        json!({"layer_id":99999,"color_label":"red"}),
    ] {
        assert!(
            s.apply(Action::new("layer.set", args.clone())).is_err(),
            "{args}"
        );
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
    }
    check(&mut s);
}

#[test]
fn layer_organization_combined_properties_are_one_undo_without_copying_pixels() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let before = s.doc.composition();
    let pointer = s.doc.layers[0].pixels.as_ptr();
    let pixels = s.doc.layers[0].pixels.clone();
    let events = s.journal.as_ref().unwrap().entries.len();
    call(
        &mut s,
        "layer.set",
        json!({"alpha_locked":true,"pixel_locked":true,"position_locked":true,"color_label":"violet","visible":false,"opacity":0.5,"blend_mode":"multiply"}),
    );
    let row = &s.state()["layers"][0];
    assert_eq!(row["alpha_locked"], true);
    assert_eq!(row["pixel_locked"], true);
    assert_eq!(row["position_locked"], true);
    assert_eq!(row["color_label"], "violet");
    assert_eq!(s.doc.layers[0].pixels.as_ptr(), pointer);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    let after = s.doc.composition();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), after);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    check(&mut s);
}

#[test]
fn layer_organization_query_keeps_pressure_contact_live_and_organization_edits_reject_it() {
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
    let samples = s.replay_samples.len();
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        call(&mut s, "layer.query", json!({"locked":false}))["drawing"],
        true
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.replay_samples.len(), samples);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    for args in [
        json!({"pixel_locked":false}),
        json!({"position_locked":true}),
        json!({"color_label":"red","opacity":0.5}),
    ] {
        assert_eq!(
            s.apply(Action::new("layer.set", args)).unwrap_err().code,
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
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
}

#[test]
fn layer_organization_locks_block_destructive_routes_while_readers_masks_and_new_copies_work() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":2,"y":2,"width":6,"height":4}),
    );
    // Store a valid recipe before locking; Repeat Copy reads its source.
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"layer","translate_x":1,"method":"nearest"}),
    );
    call(&mut s, "layer.rename", json!({"name":"Locked source"}));
    call(
        &mut s,
        "layer.set",
        json!({"pixel_locked":true,"position_locked":true,"color_label":"orange"}),
    );
    let source = s.doc.layer_id(0);
    let pixels = s.doc.layers[0].pixels.clone();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let snapshot = codec::png(&mut s.doc).unwrap();
    for (id, args) in [
        ("layer.clear", json!({})),
        ("native.invert", json!({})),
        ("layer.fill_color", json!({"color":"#FF0000"})),
        ("clipboard.cut", json!({})),
        ("layer.via_cut", json!({})),
        ("canvas.move", json!({"target":"layer","dx":1,"dy":0})),
        (
            "canvas.transform",
            json!({"target":"layer","translate_x":1}),
        ),
        ("layer.flip_horizontal", json!({})),
        (
            "canvas.stroke",
            json!({"points":[{"x":2,"y":2,"pen":true,"pressure":0.2},{"x":10,"y":6,"pen":true,"pressure":1}]}),
        ),
        (
            "canvas.pointer",
            json!({"phase":"down","x":2,"y":2,"pressure":0.5,"pen":true}),
        ),
    ] {
        let error = s.apply(Action::new(id, args)).unwrap_err();
        assert_eq!(error.code, "pixel_locked", "{id}: {error}");
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
        assert!(!s.is_drawing());
    }
    call(&mut s, "tool.select", json!({"tool":"eraser"}));
    assert_eq!(
        s.apply(Action::new(
            "canvas.stroke",
            json!({"points":[{"x":2,"y":2},{"x":8,"y":6}]})
        ))
        .unwrap_err()
        .code,
        "pixel_locked"
    );
    call(&mut s, "tool.select", json!({"tool":"brush"}));
    call(&mut s, "clipboard.copy", json!({}));
    call(&mut s, "clipboard.copy_merged", json!({}));
    assert!(call(&mut s, "clipboard.state", json!({}))["available"]
        .as_bool()
        .unwrap());
    assert_eq!(codec::png(&mut s.doc).unwrap(), snapshot);
    for id in [
        "layer.duplicate",
        "layer.via_copy",
        "clipboard.paste_in_place",
        "layer.stamp_visible",
        "canvas.transform.repeat_copy",
    ] {
        let count = s.doc.layers.len();
        call(&mut s, id, json!({}));
        assert_eq!(s.doc.layers.len(), count + 1);
        let row = s.state()["layers"][s.doc.active].clone();
        if id == "layer.duplicate" {
            assert_eq!(row["pixel_locked"], true);
            assert_eq!(row["position_locked"], true);
            assert_eq!(row["color_label"], "orange");
        } else {
            assert_eq!(row["alpha_locked"], false);
            assert_eq!(row["pixel_locked"], false);
            assert_eq!(row["position_locked"], false);
            assert_eq!(
                row["color_label"],
                if id == "canvas.transform.repeat_copy" {
                    "orange"
                } else {
                    "none"
                }
            );
        }
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layer_id(s.doc.active), source);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    call(
        &mut s,
        "canvas.move",
        json!({"target":"selection_mask","dx":1,"dy":0}),
    );
    call(
        &mut s,
        "canvas.transform",
        json!({"target":"selection_mask","translate_y":1,"method":"nearest"}),
    );
    assert_eq!(s.doc.layers[0].pixels, pixels);
    check(&mut s);
}

#[test]
fn layer_organization_position_lock_allows_paint_clear_and_document_geometry_but_blocks_layer_geometry(
) {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(
        &mut s,
        "layer.set",
        json!({"position_locked":true,"color_label":"yellow"}),
    );
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    for (id, args) in [
        ("canvas.move", json!({"target":"layer","dx":1,"dy":0})),
        (
            "canvas.transform",
            json!({"target":"layer","translate_x":1}),
        ),
        ("layer.flip_horizontal", json!({})),
        ("layer.translate", json!({"dx":1,"dy":0})),
        ("layer.rotate_90_cw", json!({})),
    ] {
        assert_eq!(
            s.apply(Action::new(id, args)).unwrap_err().code,
            "position_locked",
            "{id}"
        );
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    call(
        &mut s,
        "brush.set",
        json!({"radius":2,"stabilizer":0.35,"pressure_size":true,"pressure_opacity":true}),
    );
    call(
        &mut s,
        "canvas.stroke",
        json!({"points":[{"x":2,"y":2,"pen":true,"pressure":0.2},{"x":10,"y":6,"pen":true,"pressure":1}]}),
    );
    assert_ne!(s.doc.layers[0].pixels, pixels);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "clipboard.cut", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "layer.clear", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":24,"method":"nearest"}),
    );
    assert_eq!((s.doc.width, s.doc.height), (24, 16));
    assert_eq!(s.state()["layers"][0]["position_locked"], true);
    assert_eq!(s.state()["layers"][0]["color_label"], "yellow");
    call(&mut s, "canvas.resolution", json!({"resolution_ppi":300}));
    call(&mut s, "layer.add", json!({}));
    let source = s.doc.layer_id(0);
    call(&mut s, "layer.move", json!({"layer_id":source,"to":1}));
    call(&mut s, "layer.delete", json!({"layer_id":source}));
    assert_eq!(s.doc.layers.len(), 1);
    check(&mut s);
}

#[test]
fn layer_organization_double_locks_allow_exact_document_geometry_and_undo() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":3}));
    let pixels = [
        0xff110001, 0xff220002, 0xff330003, 0xff440004, 0xff550005, 0xff660006,
    ];
    s.doc.replace_region(0, 0, 2, 3, &pixels);
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "layer.set",
        json!({"pixel_locked":true,"position_locked":true,"color_label":"blue"}),
    );
    let composition = s.doc.composition();
    call(&mut s, "canvas.rotate_90_cw", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (3, 2));
    assert_eq!(
        s.doc.layers[0].pixels,
        [0xff550005, 0xff330003, 0xff110001, 0xff660006, 0xff440004, 0xff220002]
    );
    let row = s.state()["layers"][0].clone();
    assert_eq!(row["pixel_locked"], true);
    assert_eq!(row["position_locked"], true);
    assert_eq!(row["color_label"], "blue");
    call(&mut s, "history.undo", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (2, 3));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.composition(), composition);
    call(
        &mut s,
        "canvas.image_size",
        json!({"width":4,"method":"nearest"}),
    );
    assert_eq!((s.doc.width, s.doc.height), (4, 6));
    for y in 0..6 {
        for x in 0..4 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 4 + x],
                pixels[(y / 2) * 2 + x / 2]
            );
        }
    }
    assert_eq!(
        s.doc.composition().layer_metadata,
        composition.layer_metadata
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.composition(), composition);
    check(&mut s);
}

#[test]
fn layer_organization_destructive_merges_check_consumed_layers_and_preserve_retained_label() {
    for lock in ["pixel_locked", "alpha_locked"] {
        let mut s = session();
        call(&mut s, "native.checkerboard", json!({"cell":2}));
        let mut properties = json!({"color_label":"red"});
        properties[lock] = json!(true);
        call(&mut s, "layer.set", properties);
        call(&mut s, "layer.add", json!({}));
        call(&mut s, "layer.fill_color", json!({"color":"#22448880"}));
        let composition = s.doc.composition();
        let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
        for id in ["layer.merge_down", "layer.merge_visible", "layer.flatten"] {
            assert_eq!(
                s.apply(Action::new(id, json!({}))).unwrap_err().code,
                lock,
                "{id}"
            );
            assert_eq!(s.doc.composition(), composition);
            for (layer, expected) in s.doc.layers.iter().zip(&pixels) {
                assert_eq!(&layer.pixels, expected);
            }
        }
        // Hidden, unconsumed locks do not block merging other visible layers.
        call(&mut s, "layer.set", json!({"index":0,"visible":false}));
        call(&mut s, "layer.set", json!({"color_label":"blue"}));
        call(&mut s, "layer.add", json!({}));
        call(&mut s, "layer.set", json!({"color_label":"green"}));
        call(&mut s, "layer.merge_down", json!({}));
        assert_eq!(s.state()["layers"][s.doc.active]["color_label"], "blue");
        assert_eq!(s.state()["layers"][0][lock], true);
        check(&mut s);
    }
    // Position lock prevents moving existing pixels; a merge may consume the
    // layer and retain the base layer's organization without moving its bounds.
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(
        &mut s,
        "layer.set",
        json!({"position_locked":true,"color_label":"blue"}),
    );
    let base = s.doc.layer_id(0);
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#22448880"}));
    let composition = s.doc.composition();
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let png = codec::png(&mut s.doc).unwrap();
    call(&mut s, "layer.merge_down", json!({}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layer_id(0), base);
    let row = s.state()["layers"][0].clone();
    assert_eq!(row["position_locked"], true);
    assert_eq!(row["pixel_locked"], false);
    assert_eq!(row["color_label"], "blue");
    assert_eq!(codec::png(&mut s.doc).unwrap(), png);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    for (layer, expected) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, expected);
    }
    check(&mut s);
}

#[test]
fn layer_organization_atomic_and_journal_failures_restore_properties_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let before = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"organize","action":"layer.set","args":{"pixel_locked":true,"position_locked":true,"color_label":"gray","opacity":0.5}},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "layer.set",
            json!({"pixel_locked":true,"position_locked":true,"color_label":"gray"})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
}

#[test]
fn layer_organization_schema_state_workflow_projects_and_legacy_recordings_remain_discoverable() {
    let mut s = layers();
    let schema = s.registry.actions["layer.set"].schema();
    assert_eq!(schema["properties"]["pixel_locked"]["type"], "boolean");
    assert_eq!(schema["properties"]["position_locked"]["type"], "boolean");
    assert!(schema["properties"]["color_label"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "violet"));
    assert!(s.registry.actions["layer.query"].is_read_only());
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(catalog.contains("action.layer.query"));
    assert!(catalog.contains("state.core.layers"));
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let graph = json!({"version":1,"nodes":[{"id":1,"type":"action.layer.query","position":[0,0],"parameters":{"query":"墨","pixel_locked":true}}],"edges":[]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    let composition = s.doc.composition();
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"dry_run":true,"atomic":true})
        )["ok"],
        true
    );
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.composition(), composition);
    for (layer, expected) in s.doc.layers.iter().zip(&pixels) {
        assert_eq!(&layer.pixels, expected);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"find","action":"layer.query","args":{"query":"墨","pixel_locked":true}},
            {"id":"set","action":"layer.set","args":{"layer_id":{"$ref":"find#/result/layers/0/id"},"color_label":"violet"}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(s.state()["layers"][0]["color_label"], "violet");
    let path = output();
    call(&mut s, "document.save", json!({"path":path}));
    let composition = s.doc.composition();
    let png = codec::png(&mut s.doc).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), composition);
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), png);
    check(&mut reopened);
    let mut recording = Journal::import(&archive).unwrap();
    assert_eq!(recording.verify(None).unwrap()["valid"], true);
    let last = recording.entries.len() - 1;
    assert_eq!(
        recording.preview(last).unwrap().metadata.composition,
        composition
    );
    assert_eq!(replay::png(&recording.preview(last).unwrap()).unwrap(), png);
    // Existing archives retain their signed metadata representation and hashes.
    let mut legacy =
        Journal::import(include_bytes!("../../tests/fixtures/replay/v1.mcreplay")).unwrap();
    let head = legacy.status()["head_hash"].clone();
    assert_eq!(legacy.verify(None).unwrap()["head_hash"], head);
    let mut plain = session();
    let defaults = serde_json::to_value(plain.doc.composition()).unwrap();
    for row in defaults["layer_metadata"].as_array().unwrap() {
        assert!(row.get("pixel_locked").is_none());
        assert!(row.get("position_locked").is_none());
        assert!(row.get("color_label").is_none());
    }
    assert_eq!(
        call(&mut plain, "layer.query", json!({}))["layers"][0]["color_label"],
        "none"
    );
    std::fs::remove_file(path).unwrap();
}
