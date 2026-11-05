//! Selection actions exercise the staged native geometry/color, clipping and tool DLL.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::json;
fn rectangle(s: &mut Session) {
    call(
        s,
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":3}),
    );
}
fn selected(s: &Session) -> usize {
    s.doc
        .selection()
        .current
        .as_ref()
        .unwrap()
        .coverage()
        .iter()
        .filter(|v| **v > 0)
        .count()
}
#[test]
fn saved_defaults_drive_omitted_arguments_but_explicit_arguments_and_ui_overrides_remain_distinct()
{
    let mut s = session();
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/selection-defaults");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("settings-{}.json", std::process::id()));
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"selection":{"tolerance":0,"contiguous":false,"sample_merged":false}}),
    );
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    call(&mut s, "selection.wand", json!({"x":0,"y":0}));
    assert_eq!(selected(&s), 48);
    call(
        &mut s,
        "selection.wand",
        json!({"x":0,"y":0,"contiguous":true}),
    );
    assert_eq!(selected(&s), 9);
    s.runtime = "desktop";
    call(
        &mut s,
        "toolbox.selection.options",
        json!({"tolerance":255,"contiguous":true,"sample_merged":true}),
    );
    let controls = s.selection_sampling();
    assert_eq!(s.selection_defaults.tolerance, 0);
    call(&mut s, "selection.wand", json!({"x":0,"y":0}));
    assert_eq!(selected(&s), 48);
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"ui":{"panel_padding":9}}),
    );
    assert_eq!(s.selection_sampling(), controls);
    let bytes = std::fs::read(&path).unwrap();
    let defaults = s.selection_defaults;
    assert!(s
        .apply(Action::new(
            "settings.update",
            json!({"path":path,"selection":{"tolerance":256}})
        ))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(s.selection_defaults, defaults);
    s.runtime = "headless";
    let bad_path = dir.join("missing/subdirectory/settings.json");
    assert!(s
        .apply(Action::new(
            "settings.update",
            json!({"path":bad_path,"selection":{"tolerance":255}})
        ))
        .is_err());
    assert_eq!(s.selection_defaults, defaults);
    call(
        &mut s,
        "settings.update",
        json!({"path":path,"selection":{"tolerance":255,"contiguous":true}}),
    );
    assert_eq!(s.selection_sampling(), s.selection_defaults);
    call(&mut s, "selection.wand", json!({"x":0,"y":0}));
    assert_eq!(selected(&s), 96);
    check(&mut s);
}
#[test]
fn color_actions_become_validated_workflow_nodes_without_running_on_edit() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.selection.wand","position":[0,0],"parameters":{"x":0,"y":0,"tolerance":0}},
        {"id":2,"type":"action.selection.grow","position":[400,0],"parameters":{"tolerance":0}},
        {"id":3,"type":"action.selection.similar","position":[800,0],"parameters":{"tolerance":0}}
    ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"},{"from":2,"output":"$exec","to":3,"input":"$exec"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert!(s.doc.selection().current.is_none());
    let before = s.doc.composition();
    call(
        &mut s,
        "workflow.run",
        json!({"dry_run":true,"atomic":true}),
    );
    assert_eq!(s.doc.composition(), before);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(selected(&s), 48);
    check(&mut s);
}
#[test]
fn color_selection_uses_active_or_merged_pixels_and_restores_verified_outcomes() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#ffffff"}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","width":6,"height":8}),
    );
    call(&mut s, "layer.fill_color", json!({"color":"#000000"}));
    call(&mut s, "selection.deselect", json!({}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#0000ff80"}));
    let pixels = s
        .doc
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect::<Vec<_>>();
    let before = s.doc.composition();
    call(&mut s, "selection.wand", json!({"x":1,"y":1,"tolerance":0}));
    assert_eq!(selected(&s), 96);
    call(
        &mut s,
        "selection.wand",
        json!({"x":1,"y":1,"tolerance":0,"sample_merged":true}),
    );
    assert_eq!(selected(&s), 48);
    let expected = s.doc.composition();
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(selected(&s), 96);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), before);
    call(&mut s, "history.redo", json!({}));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.composition(), expected);
    let revision = s.revision;
    let mut stale = Action::new("selection.wand", json!({"x":0,"y":0}));
    stale.expected_revision = Some(revision - 1);
    assert!(s.apply(stale).is_err());
    for (id, args) in [
        ("selection.wand", json!({"x":12,"y":0})),
        ("selection.wand", json!({"x":0,"y":0,"tolerance":256})),
        ("selection.grow", json!({"tolerance":-1})),
        ("selection.similar", json!({"sample_merged":1})),
    ] {
        assert!(s.apply(Action::new(id, args)).is_err());
        assert_eq!(s.doc.composition(), expected);
        assert_eq!(s.revision, revision);
    }
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"color","action":"selection.wand","args":{"x":1,"y":1}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), expected);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
    let folder =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/selection");
    std::fs::create_dir_all(&folder).unwrap();
    let path = folder.join(format!("color-{}.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.composition(), expected);
    check(&mut reopened);
    // Restore recorded masks into a document without a renderer or tool DLL.
    let journal = s.replay_journal(false).unwrap();
    let mut replayed = crate::engine::Document::new(12, 8);
    journal.state.restore_document(&mut replayed);
    assert_eq!(replayed.composition(), expected);
    assert_eq!(
        replayed
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
    check(&mut s);
}
#[test]
fn grow_and_similar_expand_the_original_palette_and_rollback_as_one_group() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":3}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    let before = s.doc.composition();
    call(&mut s, "selection.grow", json!({"tolerance":0}));
    assert_eq!(selected(&s), 9);
    call(&mut s, "selection.similar", json!({"tolerance":0}));
    assert_eq!(selected(&s), 48);
    let expected = s.doc.composition();
    for _ in 0..2 {
        call(&mut s, "history.undo", json!({}));
    }
    assert_eq!(s.doc.composition(), before);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"grow","action":"selection.grow","args":{"tolerance":0}},{"id":"similar","action":"selection.similar","args":{"tolerance":0}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"grow","action":"selection.grow","args":{"tolerance":0}},{"id":"similar","action":"selection.similar","args":{"tolerance":0}}]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(s.doc.composition(), expected);
    check(&mut s);
    // Atomic execution groups replay outcomes, preserving per-action undo.
    for _ in 0..2 {
        call(&mut s, "history.undo", json!({}));
    }
    assert_eq!(s.doc.composition(), before);
    call(&mut s, "selection.deselect", json!({}));
    let before = s.state();
    for id in ["selection.grow", "selection.similar"] {
        assert_eq!(
            s.apply(Action::new(id, json!({}))).unwrap_err().code,
            "selection_empty"
        );
        assert_eq!(s.state(), before);
    }
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":100,"width":2,"height":2}),
    );
    for id in ["selection.grow", "selection.similar"] {
        assert!(s.apply(Action::new(id, json!({}))).is_err());
    }
    check(&mut s);
}
fn outside_unchanged(s: &Session, before: &[u32]) {
    let mask = s.doc.selection().current.as_ref().unwrap().coverage();
    for (i, (&old, &new)) in before.iter().zip(&s.doc.layers[0].pixels).enumerate() {
        if mask[i] == 0 {
            assert_eq!(old, new, "pixel {i} outside selection");
        }
    }
}
#[test]
fn selection_combinations_queries_undo_and_atomic_failure_preserve_verified_outcomes() {
    let mut s = session();
    rectangle(&mut s);
    assert_eq!(
        s.doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v == 255)
            .count(),
        15
    );
    let original = s.doc.composition();
    let revision = s.revision;
    call(&mut s, "selection.read", json!({}));
    assert_eq!(s.revision, revision);
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":7,"y":2,"width":2,"height":3,"mode":"add"}),
    );
    assert_eq!(
        s.doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v == 255)
            .count(),
        18
    );
    call(&mut s, "selection.invert", json!({}));
    assert_eq!(
        s.doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v == 255)
            .count(),
        96 - 18
    );
    call(&mut s, "selection.deselect", json!({}));
    assert!(s.doc.selection().current.is_none());
    call(&mut s, "selection.reselect", json!({}));
    assert!(s.doc.selection().current.is_some());
    let before = s.doc.composition();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"select","action":"selection.all"},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.doc.composition(), before);
    check(&mut s);
    let mut stale = Action::new("selection.all", json!({}));
    stale.expected_revision = Some(0);
    assert!(s.apply(stale).is_err());
    for _ in 0..4 {
        call(&mut s, "history.undo", json!({}));
    }
    assert_eq!(s.doc.composition(), original);
    check(&mut s);
}
#[test]
fn native_fill_gradient_invert_and_clear_obey_selection_and_restore_on_undo() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#334455a0"}));
    rectangle(&mut s);
    for (action, args) in [
        ("layer.fill_color", json!({"color":"#dd2255"})),
        (
            "layer.fill",
            json!({"x":4,"y":3,"color":"#22dd55","contiguous":false}),
        ),
        (
            "layer.gradient",
            json!({"x0":0,"y0":0,"x1":12,"y1":8,"start_color":"#ff0000","end_color":"#0000ff"}),
        ),
        ("native.invert", json!({})),
        ("layer.clear", json!({})),
    ] {
        let before = s.doc.layers[0].pixels.clone();
        call(&mut s, action, args);
        outside_unchanged(&s, &before);
        assert_ne!(s.doc.layers[0].pixels, before, "{action}");
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, before);
        call(&mut s, "history.redo", json!({}));
        check(&mut s);
    }
}
#[test]
fn typed_pressure_stabilization_brush_and_eraser_clip_to_the_frozen_mask() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#334455"}));
    rectangle(&mut s);
    call(
        &mut s,
        "brush.set",
        json!({"radius":3,"stabilizer":0.4,"pressure_size":true,"pressure_opacity":true,"color":"#dd2255"}),
    );
    for tool in ["brush", "eraser"] {
        let before = s.doc.layers[0].pixels.clone();
        call(
            &mut s,
            "canvas.stroke",
            json!({"tool":tool,"points":[{"x":0,"y":3,"pen":true,"pressure":0.3,"time":1},{"x":5,"y":3,"pen":true,"pressure":1.0,"time":17},{"x":11,"y":3,"pen":true,"pressure":0.7,"time":33}]}),
        );
        outside_unchanged(&s, &before);
        assert_ne!(s.doc.layers[0].pixels, before);
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, before);
    }
    // An active empty mask blocks all marks rather than becoming unrestricted.
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":100,"width":3,"height":3}),
    );
    let before = s.doc.layers[0].pixels.clone();
    call(&mut s, "canvas.stroke", json!({"points":[{"x":5,"y":3}]}));
    call(&mut s, "native.invert", json!({}));
    assert_eq!(s.doc.layers[0].pixels, before);
    check(&mut s);
}
#[test]
fn invalid_shapes_fail_without_publishing_changes() {
    let mut s = session();
    let before = s.doc.composition();
    for args in [
        json!({"kind":"rectangle"}),
        json!({"kind":"ellipse","width":0,"height":2}),
        json!({"kind":"polygon","points":[[1,2],[2,3]]}),
    ] {
        assert!(s.apply(Action::new("selection.shape", args)).is_err());
        assert_eq!(s.doc.composition(), before);
    }
    assert!(s
        .apply(Action::new("selection.reselect", json!({})))
        .is_err());
    rectangle(&mut s);
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"ellipse","x":3,"y":2,"width":5,"height":3,"mode":"intersect"}),
    );
    assert!(
        s.doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v > 0)
            .count()
            < 15
    );
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"polygon","points":[[0,0],[12,0],[0,8]],"mode":"subtract"}),
    );
    check(&mut s);
    let mut s = session();
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.selection.shape","position":[0,0],"parameters":{"kind":"rectangle","x":3,"y":2,"width":5,"height":3}},
        {"id":2,"type":"action.layer.fill_color","position":[400,0],"parameters":{"color":"#cc8844"}},
        {"id":3,"type":"action.selection.deselect","position":[800,0],"parameters":{}}
    ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"},{"from":2,"output":"$exec","to":3,"input":"$exec"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert!(s.doc.selection().current.is_none());
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    call(
        &mut s,
        "workflow.run",
        json!({"dry_run":true,"atomic":true}),
    );
    assert!(s.doc.selection().current.is_none());
    let result = call(&mut s, "workflow.run", json!({"atomic":true}));
    assert_eq!(result["ok"], true);
    assert!(s.doc.selection().current.is_none());
    for y in 0..8 {
        for x in 0..12 {
            assert_eq!(
                s.doc.layers[0].pixels[y * 12 + x],
                if (3..8).contains(&x) && (2..5).contains(&y) {
                    0xffcc8844
                } else {
                    0
                }
            );
        }
    }
    check(&mut s);
}

#[test]
fn fractional_selection_preserves_unselected_storage_and_scales_alpha() {
    use crate::engine::selection::{Mask, SelectionState};
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#11223380"}));
    let mut mask = vec![255; 96];
    mask[0] = 0;
    mask[1] = 128;
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(12, 8, mask).unwrap()),
            previous: None,
        })
        .unwrap();
    call(&mut s, "layer.clear", json!({}));
    assert_eq!(s.doc.layers[0].pixels[0], 0x80112233);
    assert_eq!(s.doc.layers[0].pixels[1], 0x40112233);
    assert_eq!(s.doc.layers[0].pixels[2], 0);
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0x80112233));
    check(&mut s);
}

#[test]
fn fill_connectivity_stays_in_seed_selection_component_and_replays() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#112233"}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":1,"y":1,"width":3,"height":5}),
    );
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":5,"y":1,"width":4,"height":5,"mode":"add"}),
    );
    for (expand, feather) in [(0, 0), (2, 2), (32, 32)] {
        let before = s.doc.layers[0].pixels.clone();
        call(
            &mut s,
            "layer.fill",
            json!({"x":2,"y":2,"color":"#ee5500","expand":expand,"feather":feather}),
        );
        assert_ne!(s.doc.layers[0].pixels[2 * 12 + 2], before[2 * 12 + 2]);
        for y in 0..8 {
            for x in 4..12 {
                assert_eq!(s.doc.layers[0].pixels[y * 12 + x], before[y * 12 + x]);
            }
        }
        outside_unchanged(&s, &before);
        check(&mut s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(s.doc.layers[0].pixels, before);
    }
    call(
        &mut s,
        "layer.fill",
        json!({"x":2,"y":2,"color":"#55ee00","contiguous":false}),
    );
    assert_eq!(s.doc.layers[0].pixels[2 * 12 + 2], 0xff55ee00);
    assert_eq!(s.doc.layers[0].pixels[2 * 12 + 6], 0xff55ee00);
    let before = s.doc.layers[0].pixels.clone();
    call(
        &mut s,
        "layer.fill",
        json!({"x":0,"y":0,"color":"#ff00ff","contiguous":false,"expand":32,"feather":32}),
    );
    assert_eq!(s.doc.layers[0].pixels, before);
    check(&mut s);
    // Positive fractional coverage is traversable, then weighted only once by
    // the host compositor. Applying it in both places would square the alpha.
    let mut s = session();
    let mut coverage = vec![0; 96];
    coverage[12 + 1] = 32;
    coverage[12 + 2] = 128;
    coverage[12 + 3] = 255;
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(crate::engine::selection::Mask::new(12, 8, coverage).unwrap()),
            previous: None,
        })
        .unwrap();
    call(&mut s, "layer.fill", json!({"x":1,"y":1,"color":"#ee5500"}));
    assert_eq!(s.doc.layers[0].pixels[13], 0x20ee5500);
    assert_eq!(s.doc.layers[0].pixels[14], 0x80ee5500);
    assert_eq!(s.doc.layers[0].pixels[15], 0xffee5500);
    check(&mut s);
}
