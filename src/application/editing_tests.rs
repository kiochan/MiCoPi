//! Editing contracts across real DLLs, atomic history, projects and replay.
use super::*;
use crate::{
    engine::{graph::NodeKind, replay::Journal, Document},
    plugins::Registry,
};
use serde_json::{json, Value};
use std::path::PathBuf;
pub(super) fn session() -> Session {
    let registry =
        Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap();
    let mut s = Session::new(registry).unwrap();
    call(&mut s, "canvas.new", json!({"width":12,"height":8}));
    s
}
pub(super) fn call(s: &mut Session, id: &str, args: Value) -> Value {
    s.apply(Action::new(id, args)).unwrap()["result"].clone()
}
pub(super) fn check(s: &mut Session) {
    assert_eq!(call(s, "replay.verify", json!({}))["valid"], true);
    let last = s.replay_journal(false).unwrap().entries.len() - 1;
    let pixels = codec::png(&mut s.doc).unwrap();
    assert_eq!(
        replay::png(&s.replay_preview(last, false).unwrap()).unwrap(),
        pixels
    );
}
#[test]
fn layer_ids_survive_reorder_and_names_and_pixels_survive_undo_redo() {
    let mut s = session();
    let id = s.doc.layer_id(0);
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let pixels = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "layer.rename",
        json!({"layer_id":id,"name":"Ink 墨"}),
    );
    call(&mut s, "layer.duplicate", json!({"layer_id":id}));
    let copied = s.doc.layer_id(1);
    assert_ne!(copied, id);
    assert_eq!(s.doc.layers[1].pixels, pixels);
    call(&mut s, "layer.move", json!({"layer_id":id,"to":1}));
    assert_eq!(s.doc.layer_index(id), Some(1));
    call(&mut s, "layer.select", json!({"layer_id":id}));
    assert_eq!(s.doc.active, 1);
    call(&mut s, "layer.delete", json!({"layer_id":copied}));
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.layer_name(0), "Ink 墨");
    check(&mut s);
    for _ in 0..4 {
        call(&mut s, "history.undo", json!({}));
        check(&mut s);
    }
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(s.doc.custom_layer_name(0), "");
    assert_eq!(s.doc.layers[0].pixels, pixels);
    for _ in 0..4 {
        call(&mut s, "history.redo", json!({}));
        check(&mut s);
    }
    assert_eq!(s.doc.layer_id(0), id);
    assert_eq!(s.doc.layer_name(0), "Ink 墨");
    assert_eq!(s.doc.layers[0].pixels, pixels);
}
#[test]
fn layer_ids_are_not_reused_after_undo_and_failed_groups_restore_the_allocator() {
    let mut s = session();
    call(&mut s, "layer.add", json!({}));
    let old = s.doc.layer_id(1);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "layer.add", json!({}));
    assert!(s.doc.layer_id(1) > old);
    let before = s.state();
    let metadata = s.doc.composition();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"copy","action":"layer.duplicate"},
            {"id":"rename","action":"layer.rename","args":{"name":"temporary"}},
            {"id":"fail","action":"layer.move","args":{"to":12}}
        ]}),
    );
    assert_eq!(r["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), metadata);
    check(&mut s);
}
#[test]
fn graph_sources_follow_layer_identity_and_referenced_layers_cannot_be_deleted() {
    let mut s = session();
    let source = s.doc.layer_id(0);
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    call(&mut s, "layer.add", json!({}));
    call(
        &mut s,
        "graph.set",
        json!({"graph":{"enabled":true,"nodes":[
        {"id":0,"kind":"layer","index":0,"position":[0,0]},
        {"id":1,"kind":"output","position":[200,0]}
    ],"edges":[{"from":0,"to":1,"input":0}]}}),
    );
    let expected = codec::png(&mut s.doc).unwrap();
    call(&mut s, "layer.move", json!({"layer_id":source,"to":1}));
    assert!(matches!(
        s.doc.graph.nodes[0].data,
        NodeKind::Layer { index: 1 }
    ));
    assert_eq!(codec::png(&mut s.doc).unwrap(), expected);
    call(&mut s, "layer.duplicate", json!({"index":0}));
    assert!(matches!(
        s.doc.graph.nodes[0].data,
        NodeKind::Layer { index: 2 }
    ));
    let before = s.state();
    assert!(s
        .apply(Action::new("layer.delete", json!({"layer_id":source})))
        .is_err());
    assert_eq!(s.state(), before);
    call(&mut s, "layer.delete", json!({"index":0}));
    assert!(matches!(
        s.doc.graph.nodes[0].data,
        NodeKind::Layer { index: 1 }
    ));
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    check(&mut s);
    assert!(matches!(
        s.doc.graph.nodes[0].data,
        NodeKind::Layer { index: 2 }
    ));
}
#[test]
fn invalid_layer_targets_names_and_last_layer_deletion_preserve_state() {
    let mut s = session();
    let before = s.state();
    for (id, args) in [
        ("layer.select", json!({"layer_id":999})),
        ("layer.rename", json!({"name":"\n"})),
        ("layer.rename", json!({"name":"x".repeat(513)})),
        ("layer.move", json!({"to":7})),
        ("layer.delete", json!({})),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.state(), before);
    }
    call(&mut s, "layer.add", json!({}));
    let before = s.state();
    let id = s.doc.layer_id(1);
    assert!(s
        .apply(Action::new(
            "layer.select",
            json!({"index":0,"layer_id":id})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    check(&mut s);
}
#[test]
fn structural_moves_do_not_clone_layer_pixel_allocations() {
    let mut d = Document::new(8, 8);
    d.add_layer();
    let id = d.layer_id(0);
    let allocation = d.layers[0].pixels.as_ptr();
    d.move_layer(0, 1).unwrap();
    assert_eq!(d.layer_id(1), id);
    assert_eq!(d.layers[1].pixels.as_ptr(), allocation);
    d.undo();
    assert_eq!(d.layers[0].pixels.as_ptr(), allocation);
    d.delete_layer(0).unwrap();
    d.undo();
    assert_eq!(d.layers[0].pixels.as_ptr(), allocation);
}
#[test]
fn new_layer_metadata_persists_and_legacy_journal_projects_upgrade_without_hash_changes() {
    let mut s = session();
    call(&mut s, "layer.rename", json!({"name":"Paint 画"}));
    call(&mut s, "layer.duplicate", json!({}));
    let expected = s.doc.composition();
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/editing");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(format!("layers-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), expected);
    check(&mut reopened);
    let bytes = include_bytes!("../../tests/fixtures/replay/v1.mcreplay");
    let mut journal = Journal::import(bytes).unwrap();
    let head = journal.status()["head_hash"].clone();
    let mut legacy = Document::new(journal.state.metadata.width, journal.state.metadata.height);
    journal.state.restore_document(&mut legacy);
    let path = folder.join(format!("legacy-{}.mcp", std::process::id()));
    legacy
        .save_with_replay(&path, Some(&journal.bytes(None).unwrap()))
        .unwrap();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert!(reopened.doc.layer_id(0) > 0);
    assert_eq!(journal.verify(None).unwrap()["head_hash"], head);
    check(&mut reopened);
}
#[test]
fn native_spatial_edits_are_exact_undoable_atomic_and_replayable() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let original = s.doc.layers[0].pixels.clone();
    call(&mut s, "layer.flip_horizontal", json!({}));
    for (expected, row) in original.chunks(12).zip(s.doc.layers[0].pixels.chunks(12)) {
        assert_eq!(row, expected.iter().rev().copied().collect::<Vec<_>>());
    }
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    call(&mut s, "layer.translate", json!({"dx":2,"dy":-1}));
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if x >= 2 && y < 7 {
                    original[(y + 1) * 12 + x - 2]
                } else {
                    0
                }
            );
        }
    }
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let r = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"flip","action":"layer.flip_vertical"},
            {"id":"rotate","action":"layer.rotate_180"},
            {"id":"fail","action":"layer.select","args":{"layer_id":999}}
        ]}),
    );
    assert_eq!(r["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    check(&mut s);
}

#[test]
fn malformed_id_metadata_and_hidden_pixel_edits_are_rejected() {
    let mut s = session();
    call(&mut s, "layer.add", json!({}));
    let valid = s.doc.composition();
    for defect in 0..5 {
        let mut m = valid.clone();
        match defect {
            0 => m.layer_metadata[1].id = m.layer_metadata[0].id,
            1 => m.layer_metadata[0].id = 0,
            2 => m.next_layer_id = 1,
            3 => {
                m.layer_metadata.pop();
            }
            _ => m.layer_metadata[0].name = "bad\nname".into(),
        }
        assert!(m.validate_layer_metadata().is_err());
    }
    call(&mut s, "layer.set", json!({"visible":false}));
    let before = s.doc.composition();
    let pixels = s.doc.layers[1].pixels.clone();
    assert!(s
        .apply(Action::new("layer.flip_horizontal", json!({})))
        .is_err());
    assert_eq!(s.doc.composition(), before);
    assert_eq!(s.doc.layers[1].pixels, pixels);
    check(&mut s);
}

#[test]
fn fill_and_gradient_are_undoable_atomic_and_replayable() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let original = s.doc.layers[0].pixels.clone();
    call(&mut s, "layer.fill", json!({"x":0,"y":0,"color":"#ff0000"}));
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if x < 3 && y < 3 {
                    0xffff0000
                } else {
                    original[y * 12 + x]
                }
            );
        }
    }
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    call(
        &mut s,
        "layer.gradient",
        json!({"x0":0.5,"y0":0.5,"x1":11.5,"y1":0.5,"start_color":"#ff0000","end_color":"#0000ff"}),
    );
    assert_eq!(s.doc.layers[0].pixels[0], 0xffff0000);
    assert_eq!(s.doc.layers[0].pixels[11], 0xff0000ff);
    check(&mut s);
    let pixels = s.doc.layers[0].pixels.clone();
    let before = s.state();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"fill","action":"layer.fill","args":{"x":0,"y":0,"color":"#00ff00"}},
            {"id":"invalid","action":"layer.gradient","args":{"x0":1,"y0":1,"x1":1,"y1":1,"start_color":"#ff0000","end_color":"#ffffff"}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    for args in [
        json!({"x":100,"y":0,"color":"#ffffff"}),
        json!({"x":0,"y":0,"color":"#bad"}),
    ] {
        assert!(s.apply(Action::new("layer.fill", args)).is_err());
        assert_eq!(s.state(), before);
    }
    check(&mut s);
}

#[test]
fn stroke_options_change_dabs_and_persist_without_changing_legacy_metadata() {
    let mut s = session();
    let defaults = serde_json::to_value(&s.parameters).unwrap();
    assert!(defaults.get("spacing").is_none());
    assert!(defaults.get("minimum_opacity").is_none());
    assert_eq!(
        s.state()["parameters"]["spacing"].as_f64().unwrap() as f32,
        0.11
    );
    assert_eq!(
        s.state()["parameters"]["minimum_opacity"].as_f64().unwrap() as f32,
        0.
    );
    call(&mut s, "canvas.new", json!({"width":128,"height":32}));
    call(
        &mut s,
        "brush.set",
        json!({"radius":3,"hardness":1,"opacity":1,"pressure_size":false,"stabilizer":0,"spacing":1}),
    );
    let points = json!({"points":[{"x":10,"y":16},{"x":110,"y":16}]});
    call(&mut s, "canvas.stroke", points.clone());
    let sparse = s.doc.layers[0]
        .pixels
        .iter()
        .filter(|p| **p >> 24 > 0)
        .count();
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    call(&mut s, "brush.set", json!({"spacing":0.11}));
    call(&mut s, "canvas.stroke", points);
    let dense = s.doc.layers[0]
        .pixels
        .iter()
        .filter(|p| **p >> 24 > 0)
        .count();
    assert!(dense > sparse);
    call(
        &mut s,
        "brush.set",
        json!({"spacing":0.7,"minimum_opacity":0.4}),
    );
    for (key, value) in [
        ("spacing", json!(0)),
        ("spacing", json!(2.1)),
        ("minimum_opacity", json!(-0.1)),
    ] {
        assert!(s
            .apply(Action::new("brush.set", json!({key:value})))
            .is_err());
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/brush-options-{}.mcp",
        std::process::id()
    ));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.parameters.spacing, 0.7);
    assert_eq!(reopened.parameters.minimum_opacity, 0.4);
    check(&mut reopened);
    let path = path.with_extension("json");
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"document":{"painting":{"spacing":0.7,"minimum_opacity":0.4}}}),
    );
    let settings = call(&mut s, "settings.read", json!({"path":path}));
    assert_eq!(
        settings["painting"]["spacing"].as_f64().unwrap() as f32,
        0.7
    );
    assert_eq!(
        settings["painting"]["minimum_opacity"].as_f64().unwrap() as f32,
        0.4
    );
}

#[test]
fn global_fill_and_gradient_controls_preserve_undo_atomicity_and_replay() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":8,"height":4}));
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "layer.fill",
        json!({"x":0,"y":0,"color":"#ff0000","contiguous":false}),
    );
    for (pixel, old) in s.doc.layers[0].pixels.iter().zip(&original) {
        assert_eq!(
            *pixel,
            if *old == original[0] {
                0xffff0000
            } else {
                *old
            }
        );
    }
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    for (spread, reverse, expected) in [
        (
            "repeat",
            false,
            vec![0xff800080, 0xffff0000, 0xff800080, 0xffff0000, 0xff800080],
        ),
        (
            "reflect",
            false,
            vec![0xff800080, 0xffff0000, 0xff800080, 0xff0000ff, 0xff800080],
        ),
        (
            "reflect",
            true,
            vec![0xff800080, 0xff0000ff, 0xff800080, 0xffff0000, 0xff800080],
        ),
    ] {
        call(&mut s, "canvas.new", json!({"width":5,"height":1}));
        call(
            &mut s,
            "layer.gradient",
            json!({"x0":1.5,"y0":0.5,"x1":3.5,"y1":0.5,"start_color":"#ff0000","end_color":"#0000ff","spread":spread,"reverse":reverse}),
        );
        assert_eq!(s.doc.layers[0].pixels, expected);
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, vec![0; 5]);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, expected);
    }
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"fill","action":"layer.fill","args":{"x":0,"y":0,"color":"#00ff00","contiguous":false}},
            {"id":"fail","action":"layer.gradient","args":{"x0":1,"y0":0,"x1":1,"y1":0,"start_color":"#ff0000","end_color":"#0000ff","spread":"repeat"}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    for (id, args) in [
        (
            "layer.fill",
            json!({"x":0,"y":0,"color":"#ff0000","contiguous":"false"}),
        ),
        (
            "layer.gradient",
            json!({"x0":0,"y0":0,"x1":1,"y1":0,"start_color":"#ff0000","end_color":"#0000ff","spread":"invalid"}),
        ),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.state(), before);
    }
    check(&mut s);
}

#[test]
fn fill_edges_preserve_undo_projects_atomicity_and_verified_pixels() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":9,"height":9}));
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let original = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "layer.fill",
        json!({"x":1,"y":1,"color":"#ff0000","expand":1}),
    );
    for y in 0..9 {
        for x in 0..9 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 9 + x],
                if x < 4 && y < 4 {
                    0xffff0000
                } else {
                    original[y * 9 + x]
                }
            );
        }
    }
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, original);
    call(&mut s, "canvas.new", json!({"width":9,"height":9}));
    call(
        &mut s,
        "layer.fill",
        json!({"x":4,"y":4,"color":"#ff0000","expand":-1,"feather":1,"contiguous":false}),
    );
    assert_eq!(s.doc.layers[0].pixels[0], 0x40ff0000);
    assert_eq!(s.doc.layers[0].pixels[10], 0xbfff0000);
    assert_eq!(s.doc.layers[0].pixels[40], 0xffff0000);
    check(&mut s);
    let pixels = s.doc.layers[0].pixels.clone();
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, vec![0; 81]);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    let before = s.state();
    for (field, value) in [
        ("expand", json!(33)),
        ("expand", json!(-33)),
        ("expand", json!(0.5)),
        ("feather", json!(-1)),
        ("feather", json!(33)),
    ] {
        let mut args = json!({"x":4,"y":4,"color":"#00ff00"});
        args[field] = value;
        assert!(s.apply(Action::new("layer.fill", args)).is_err());
        assert_eq!(s.state(), before);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"edges","action":"layer.fill","args":{"x":4,"y":4,"color":"#00ff00","expand":1,"feather":2}},
            {"id":"fail","action":"layer.select","args":{"index":15}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/fill-edges-{}.mcp",
        std::process::id()
    ));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    check(&mut reopened);
}
