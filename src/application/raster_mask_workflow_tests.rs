//! Public mask actions, schema-derived workflows and long typed contacts.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::raster_masks::{PaintTarget, PlaneView};
use serde_json::json;

fn samples(s: &Session, id: u64) -> Vec<u8> {
    match s.doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(v) => vec![v; s.doc.width * s.doc.height],
        PlaneView::Dense(data) => data.to_vec(),
    }
}
fn create(s: &mut Session, source: &str) -> u64 {
    let value = call(
        s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"source":source}),
    );
    crate::plugins::validate(
        s.registry.actions["mask.create"]
            .output_schema
            .as_ref()
            .unwrap(),
        &value,
    )
    .unwrap();
    value["mask_id"].as_u64().unwrap()
}

#[test]
fn mask_workflow_selection_creation_has_one_undo_and_bounded_read_only_samples() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#5477CC"}));
    let rgba = s.doc.layers[0].pixels.clone();
    let source: Vec<_> = (0..s.doc.width * s.doc.height)
        .map(|i| (i * 37 % 256) as u8)
        .collect();
    s.doc
        .set_selection(crate::engine::selection::SelectionState {
            current: Some(
                crate::engine::selection::Mask::new(s.doc.width, s.doc.height, source.clone())
                    .unwrap(),
            ),
            previous: None,
        })
        .unwrap();
    s.replay_boundary().unwrap();
    let id = create(&mut s, "selection");
    assert_eq!(samples(&s, id), source);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id });
    assert_eq!(s.doc.layers[0].pixels, rgba);
    let before = s.state();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let snapshot = call(
        &mut s,
        "mask.snapshot",
        json!({"mask_id":id,"width":256,"height":256}),
    );
    assert_eq!(snapshot["samples"], json!(source));
    assert_eq!(snapshot["width"], s.doc.width);
    assert_eq!(snapshot["height"], s.doc.height);
    let public = call(
        &mut s,
        "context.read",
        json!({"resources":["layers.masks"]}),
    );
    let public = &public["resources"]["layers.masks"]["value"];
    crate::plugins::validate(&s.registry.states["layers.masks"].output_schema, public).unwrap();
    assert_eq!(public["items"][0]["id"], id);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert!(s
        .apply(Action::new("mask.snapshot", json!({"mask_id":id,"x":4095})))
        .is_err());
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.mask_state().items.is_empty());
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(samples(&s, id), source);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id });
    assert_eq!(s.doc.layers[0].pixels, rgba);
    check(&mut s);
}

#[test]
fn mask_workflow_dynamic_ids_are_inert_until_run_and_failures_restore_ids_samples_and_history() {
    let mut s = session();
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.mask.create","position":[0,0],"parameters":{"owner":{"kind":"layer","id":1},"select":false}},
        {"id":2,"type":"action.mask.fill","position":[350,0],"parameters":{"value":64}}
    ],"edges":[{"from":1,"output":"mask_id","to":2,"input":"mask_id"}]});
    let before = s.doc.composition();
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert!(s.doc.mask_state().is_default());
    let revision = s.revision;
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(s.revision, revision);
    assert!(s.doc.mask_state().is_default());
    assert_eq!(s.doc.layers[0].pixels, vec![0; 96]);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    let id = s.doc.mask_state().items[0].id;
    assert!(samples(&s, id).iter().all(|v| *v == 64));
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    assert_ne!(s.doc.composition(), before);
    call(&mut s, "mask.invert", json!({"mask_id":id}));
    assert!(samples(&s, id).iter().all(|v| *v == 191));
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"replace","action":"mask.fill","args":{"mask_id":id,"value":0}},
            {"id":"bad","action":"mask.delete","args":{"mask_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(samples(&s, id).iter().all(|v| *v == 191));
    assert!(s.apply(Action::new("pipeline.run",json!({"steps":[
        {"id":"must-not-run","action":"mask.fill","args":{"mask_id":id,"value":0}},
        {"id":"bad-literal","action":"mask.target","args":{"target":{"kind":"layer_pixels","id":id}}}
    ]}))).is_err());
    assert_eq!(s.state(), state);
    check(&mut s);
    s.journal.as_mut().unwrap().fail_next_write_after(13);
    assert!(s
        .apply(Action::new("mask.fill", json!({"mask_id":id,"value":255})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(samples(&s, id).iter().all(|v| *v == 191));
    assert!(s.replay_error.is_some());
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

#[test]
fn mask_workflow_long_pressure_contact_keeps_dense_lease_through_recording_boundaries_and_undo() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":80,"height":48}));
    call(&mut s, "layer.fill_color", json!({"color":"#5F7F9F"}));
    let id = create(&mut s, "reveal_all");
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":id,"density":203,"feather_radius_px":3}),
    );
    call(
        &mut s,
        "brush.set",
        json!({"radius":3,"color":"#000000","stabilizer":0.55,"pressure_size":true}),
    );
    let rgba = s.doc.layers[0].pixels.clone();
    let mut p = Pointer {
        phase: PointerPhase::Down,
        x: 6.,
        y: 7.,
        pressure: 0.3,
        time: 0,
        pen: true,
        eraser: false,
        tool_override: Some(ToolKind::Brush),
        view_x: None,
        view_y: None,
    };
    s.pointer(p).unwrap();
    let pointer = match s.doc.mask_plane(id).unwrap() {
        PlaneView::Dense(data) => data.as_ptr(),
        _ => panic!("materialized contact"),
    };
    for i in 1..=320 {
        p.phase = PointerPhase::Move;
        p.x = 6. + (i % 65) as f32;
        p.y = 7. + (i % 33) as f32;
        p.pressure = 0.2 + (i % 70) as f32 / 100.;
        p.time = i as u32;
        s.pointer(p).unwrap();
        if i == 128 || i == 256 {
            let revision = s.revision;
            let value = call(&mut s, "mask.query", json!({"mask_id":id}));
            assert_eq!(value["target"], json!({"kind":"mask","id":id}));
            assert_eq!(s.revision, revision);
            assert!(s.is_drawing());
            assert_eq!(
                match s.doc.mask_plane(id).unwrap() {
                    PlaneView::Dense(data) => data.as_ptr(),
                    _ => panic!("dense lease"),
                },
                pointer
            );
            assert!(s
                .apply(Action::new("mask.set", json!({"mask_id":id,"density":0})))
                .is_err());
            assert!(s.is_drawing());
        }
    }
    p.phase = PointerPhase::Up;
    p.time += 1;
    s.pointer(p).unwrap();
    assert!(!s.is_drawing());
    assert!(samples(&s, id).iter().any(|v| *v < 255));
    assert_eq!(s.doc.layers[0].pixels, rgba);
    let events = call(&mut s, "replay.events", json!({}));
    let count: usize = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
        .map(Vec::len)
        .sum();
    assert_eq!(count, 322);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert!(samples(&s, id).iter().all(|v| *v == 255));
    assert_eq!(s.doc.layers[0].pixels, rgba);
    call(&mut s, "history.redo", json!({}));
    assert!(samples(&s, id).iter().any(|v| *v < 255));
    check(&mut s);
}

#[test]
fn mask_workflow_apply_retains_filters_and_properties_with_explicit_raw_alpha_quantization() {
    use crate::engine::graph::{Edge, Filter, Node, NodeKind};
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    call(&mut s, "layer.fill_color", json!({"color":"#336699"}));
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":1,"opacity":137./255.}),
    );
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|node| matches!(node.data, NodeKind::Layer { index: 0 }))
        .unwrap()
        .id;
    let filter = graph.nodes.iter().map(|node| node.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = filter;
        }
    }
    graph.nodes.push(Node {
        id: filter,
        node_type: None,
        position: [100., 0.],
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 1.,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: filter,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    let graph = s.doc.graph.clone();
    let id = create(&mut s, "reveal_all");
    call(&mut s, "mask.fill", json!({"mask_id":id,"value":128}));
    s.doc.compose();
    assert_eq!(s.doc.composite, vec![0x45cc9966]);
    let original = s.doc.composition();
    let revision = s.revision;
    let result = call(&mut s, "mask.apply", json!({"mask_id":id}));
    assert_eq!(result["layer_id"], 1);
    assert_eq!(result["bake_mode"], "raw_alpha");
    crate::plugins::validate(
        s.registry.actions["mask.apply"]
            .output_schema
            .as_ref()
            .unwrap(),
        &result,
    )
    .unwrap();
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.doc.layers[0].pixels, vec![0x80336699]);
    assert_eq!(s.doc.layers[0].opacity, 137);
    assert_eq!(s.doc.graph, graph);
    assert_eq!(s.doc.composite, vec![0x44cc9966]);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    s.doc.compose();
    assert_eq!(s.doc.composition(), original);
    assert_eq!(s.doc.layers[0].pixels, vec![0xff336699]);
    assert_eq!(s.doc.composite, vec![0x45cc9966]);
    call(&mut s, "history.redo", json!({}));
    s.doc.compose();
    assert_eq!(s.doc.layers[0].pixels, vec![0x80336699]);
    assert_eq!(s.doc.composite, vec![0x44cc9966]);
    check(&mut s);
}

#[test]
fn mask_workflow_stroke_action_uses_the_same_a8_target_and_locks_as_typed_pointer_input() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#336699"}));
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":1,"pixel_locked":true,"alpha_locked":true}),
    );
    let rgba = s.doc.layers[0].pixels.clone();
    let id = create(&mut s, "reveal_all");
    call(
        &mut s,
        "brush.set",
        json!({"radius":4,"color":"#000000","stabilizer":0.4,"pressure_size":true}),
    );
    let points = json!([{"x":3,"y":3,"pressure":0.4,"pen":true,"time":0},
        {"x":8,"y":5,"pressure":0.9,"pen":true,"time":16}]);
    call(
        &mut s,
        "canvas.stroke",
        json!({"tool":"brush","points":points}),
    );
    let painted = samples(&s, id);
    assert!(painted.iter().any(|v| *v < 255));
    assert_eq!(s.doc.layers[0].pixels, rgba);
    call(&mut s, "history.undo", json!({}));
    assert!(samples(&s, id).iter().all(|v| *v == 255));
    call(&mut s, "history.redo", json!({}));
    assert_eq!(samples(&s, id), painted);
    call(
        &mut s,
        "canvas.stroke",
        json!({"tool":"eraser","points":points}),
    );
    assert!(samples(&s, id)
        .iter()
        .zip(&painted)
        .any(|(new, old)| new > old));
    assert_eq!(s.doc.layers[0].pixels, rgba);
    check(&mut s);
}
