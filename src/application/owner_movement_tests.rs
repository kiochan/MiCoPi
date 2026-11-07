//! Integer owner movement keeps raw planes separate from selection and effects.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    graph::{Edge, Filter, Node, NodeKind, Renderer},
    raster_masks::{PaintTarget, PlaneView, SamplePlane},
    replay::Journal,
    selection::{Mask, SelectionState},
    Document,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{cell::Cell, collections::BTreeMap, path::PathBuf, sync::Arc};

const W: usize = 5;
const H: usize = 3;

fn raw(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            let view = s.doc.mask_plane(m.id).unwrap();
            (m.id, (0..W * H).map(|i| view.sample(i)).collect())
        })
        .collect()
}
fn rgba(s: &Session) -> Vec<Vec<u32>> {
    s.doc.layers.iter().map(|l| l.pixels.clone()).collect()
}
fn pointers(s: &Session) -> BTreeMap<u64, Option<usize>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            (
                m.id,
                match s.doc.mask_plane(m.id).unwrap() {
                    PlaneView::Dense(data) => Some(data.as_ptr() as usize),
                    PlaneView::Uniform(_) => None,
                },
            )
        })
        .collect()
}
/// Independent coordinate lookup oracle: the destination is sampled from its
/// inverse integer translation, with a different exterior for A8 and RGBA.
fn shifted<T: Copy>(input: &[T], dx: i32, dy: i32, exterior: T) -> Vec<T> {
    (0..W * H)
        .map(|i| {
            let x = (i % W) as i64 - i64::from(dx);
            let y = (i / W) as i64 - i64::from(dy);
            if (0..W as i64).contains(&x) && (0..H as i64).contains(&y) {
                input[y as usize * W + x as usize]
            } else {
                exterior
            }
        })
        .collect()
}
#[derive(Clone, Debug, PartialEq)]
struct Content {
    composition: Composition,
    active: usize,
    pixels: Vec<Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
}
fn content(s: &Session) -> Content {
    Content {
        composition: s.doc.composition(),
        active: s.doc.active,
        pixels: rgba(s),
        masks: raw(s),
    }
}
struct Before {
    content: Content,
    public: Value,
    surface: Vec<u32>,
    history: usize,
    revision: u64,
    events: usize,
}
impl Before {
    fn capture(s: &Session) -> Self {
        Self {
            content: content(s),
            public: s.state(),
            surface: s.doc.composite.clone(),
            history: s.doc.history_bytes(),
            revision: s.revision,
            events: s.journal.as_ref().unwrap().entries.len(),
        }
    }
    fn restored(&self, s: &Session) {
        assert_eq!(content(s), self.content);
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert_eq!(s.revision, self.revision);
        assert!(!s.doc.history_group_active());
    }
    fn failed_audit(&self, s: &mut Session, action: &str) {
        self.restored(s);
        let journal = s.journal.as_mut().unwrap();
        assert_eq!(journal.entries.len(), self.events + 1);
        let event = journal.events(self.events, 1).unwrap()["events"][0].clone();
        assert_eq!(event["source"]["action"], action);
        assert_eq!(event["success"], false);
        assert_eq!(event["before"], event["after"]);
        assert_eq!(event["changed_tiles"], 0);
        assert_eq!(event["changed_mask_tiles"], 0);
    }
}
fn outcome(s: &mut Session) {
    check(s);
    let n = s.journal.as_ref().unwrap().entries.len();
    let preview = s.replay_preview(n - 1, false).unwrap();
    assert!(preview.matches_document(&s.doc));
    let mut restored = Document::new(1, 1);
    preview.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, expected) in raw(s) {
        let plane = restored.mask_plane(id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(),
            expected
        );
    }
}
struct Fixture {
    s: Session,
    owner: u64,
    paper: u64,
    other: u64,
    group: u64,
    group_id: u64,
}
fn fixture(uniform: bool, linked: bool, enabled: bool) -> Fixture {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.add", json!({}));
    let group_id = call(
        &mut s,
        "group.create",
        json!({"name":"Cels","members":[{"kind":"layer","id":2},{"kind":"layer","id":3}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "group.set",
        json!({"group_id":group_id,"opacity":193./255.}),
    );
    for (layer, raster) in s.doc.layers.iter_mut().enumerate() {
        for (i, pixel) in raster.pixels.iter_mut().enumerate() {
            *pixel = [0u32, 1, 73, 137, 255][(i + layer) % 5] << 24
                | ((i as u32 * 7919 + layer as u32 * 3571 + 19) & 0xffffff);
        }
    }
    // Raw fixture words bypass native loans. Commit their tile damage before
    // the next shared action can capture a rollback checkpoint.
    s.doc.replay_tiles.extend([(0, 0), (1, 0), (2, 0)]);
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|n| n.data == (NodeKind::Layer { index: 1 }))
        .unwrap()
        .id;
    let filter = graph.nodes.iter().map(|n| n.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = filter;
        }
    }
    graph.nodes.push(Node {
        id: filter,
        node_type: None,
        position: [70., 40.],
        data: NodeKind::Filter {
            filter: Filter::Saturation,
            amount: 0.35,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: filter,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    let mut ids = Vec::new();
    for owner in [
        json!({"kind":"layer","id":1}),
        json!({"kind":"layer","id":2}),
        json!({"kind":"layer","id":3}),
        json!({"kind":"group","id":group_id}),
    ] {
        ids.push(
            call(&mut s, "mask.create", json!({"owner":owner,"select":false}))["mask_id"]
                .as_u64()
                .unwrap(),
        );
    }
    for (i, id) in ids.iter().copied().enumerate() {
        let plane = if i == 1 && uniform {
            SamplePlane::Uniform(91)
        } else {
            SamplePlane::Dense((0..W * H).map(|v| (v * 37 + i * 29 + 7) as u8).collect())
        };
        s.doc.replace_mask_plane(id, plane).unwrap();
    }
    s.doc
        .set_selection(SelectionState {
            current: Some(
                Mask::new(
                    W,
                    H,
                    (0..W * H)
                        .map(|i| if i % 3 == 0 { 128 } else { 0 })
                        .collect(),
                )
                .unwrap(),
            ),
            previous: Some(
                Mask::new(
                    W,
                    H,
                    (0..W * H)
                        .map(|i| if i % 4 == 0 { 255 } else { 0 })
                        .collect(),
                )
                .unwrap(),
            ),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[1],"linked":linked,"enabled":enabled,"density":173,"feather_radius_px":1}),
    );
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[3],"linked":false,"density":203,"feather_radius_px":2}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.,"color_label":"violet"}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":2}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[3]}}),
    );
    s.doc.compose();
    Fixture {
        s,
        paper: ids[0],
        owner: ids[1],
        other: ids[2],
        group: ids[3],
        group_id,
    }
}
fn last_context(s: &mut Session) -> Value {
    let journal = s.journal.as_mut().unwrap();
    journal.events(journal.entries.len() - 1, 1).unwrap()["events"][0]["source"]["context"]
        ["movement"]
        .clone()
}

#[test]
fn owner_movement_whole_layer_moves_only_its_linked_raw_plane_and_undo_is_compound() {
    for (uniform, linked, enabled) in [
        (false, true, true),
        (true, true, false),
        (false, false, true),
    ] {
        let Fixture {
            mut s,
            owner,
            paper,
            other,
            group,
            ..
        } = fixture(uniform, linked, enabled);
        let before = Before::capture(&s);
        let storage = pointers(&s);
        let raster_storage = [
            s.doc.layers[0].pixels.as_ptr(),
            s.doc.layers[2].pixels.as_ptr(),
        ];
        let result = call(
            &mut s,
            "canvas.move",
            json!({"dx":2,"dy":-1,"target":"layer"}),
        );
        assert_eq!(result, s.state());
        assert_eq!(s.revision, before.revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events + 1);
        let mut expected = before.content.clone();
        expected.pixels[1] = shifted(&expected.pixels[1], 2, -1, 0);
        if linked {
            expected
                .masks
                .insert(owner, shifted(&expected.masks[&owner], 2, -1, 255));
        }
        assert_eq!(content(&s), expected);
        for id in [paper, other, group] {
            assert_eq!(pointers(&s)[&id], storage[&id]);
        }
        if !linked {
            assert_eq!(pointers(&s)[&owner], storage[&owner]);
        }
        assert_eq!(
            [
                s.doc.layers[0].pixels.as_ptr(),
                s.doc.layers[2].pixels.as_ptr()
            ],
            raster_storage
        );
        assert_eq!(
            last_context(&mut s),
            json!({"target":"layer","layer_id":2,"linked_mask_id":if linked {Some(owner)} else {None},"dx":2,"dy":-1})
        );
        outcome(&mut s);
        let moved = content(&s);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), before.content);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), moved);
        outcome(&mut s);
    }
}

#[test]
fn owner_movement_auto_mask_beats_pixel_selection_and_explicit_id_beats_stored_target() {
    for explicit in [false, true] {
        let Fixture {
            mut s,
            owner,
            paper,
            group,
            ..
        } = fixture(false, false, false);
        call(&mut s, "layer.select", json!({"layer_id":1}));
        call(
            &mut s,
            "layer.set",
            json!({"layer_id":2,"pixel_locked":true,"alpha_locked":true}),
        );
        call(
            &mut s,
            "mask.target",
            json!({"target":{"kind":"mask","id":if explicit {group} else {owner}}}),
        );
        let before = Before::capture(&s);
        let storage = pointers(&s);
        let args = if explicit {
            json!({"mask_id":owner as f64,"dx":-1,"dy":1})
        } else {
            json!({"dx":-1,"dy":1,"target":"auto"})
        };
        call(&mut s, "canvas.move", args);
        let mut expected = before.content.clone();
        expected
            .masks
            .insert(owner, shifted(&expected.masks[&owner], -1, 1, 255));
        assert_eq!(content(&s), expected);
        assert_eq!(s.doc.active, 0);
        assert_eq!(pointers(&s)[&paper], storage[&paper]);
        assert_eq!(pointers(&s)[&group], storage[&group]);
        assert_eq!(
            last_context(&mut s),
            json!({"target":"raster_mask","mask_id":owner,"owner_layer_id":2,"dx":-1,"dy":1})
        );
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), before.content);
        outcome(&mut s);
    }
}

#[test]
fn owner_movement_nudge_preferences_explicit_override_and_integral_ids_are_shared() {
    let Fixture {
        mut s,
        owner,
        group,
        ..
    } = fixture(false, true, true);
    s.apply_movement_defaults(MovementPreferences {
        target: MoveTarget::SelectionMask,
        step: 2,
        large_step: 3,
    });
    let before = content(&s);
    call(
        &mut s,
        "canvas.nudge",
        json!({"mask_id":2.0,"direction":"right"}),
    );
    let mut expected = before.clone();
    expected
        .masks
        .insert(owner, shifted(&before.masks[&owner], 2, 0, 255));
    assert_eq!(content(&s), expected);
    assert_eq!(last_context(&mut s)["mask_id"], owner);
    assert_eq!(s.doc.mask_state().target, PaintTarget::Mask { id: group });
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.nudge",
        json!({"target":"layer","direction":"up","large":true}),
    );
    let mut expected = before.clone();
    expected.pixels[1] = vec![0; W * H];
    expected.masks.insert(owner, vec![255; W * H]);
    assert_eq!(content(&s), expected);
    outcome(&mut s);
}

#[test]
fn owner_movement_outline_moves_only_current_selection_and_pending_targets_stay_guarded() {
    let Fixture {
        mut s,
        owner,
        group,
        ..
    } = fixture(false, true, true);
    let before = content(&s);
    call(
        &mut s,
        "canvas.move",
        json!({"target":"selection_mask","dx":1,"dy":0}),
    );
    let mut expected = before.clone();
    expected.composition.selection.current = Some(
        Mask::new(
            W,
            H,
            shifted(
                before
                    .composition
                    .selection
                    .current
                    .as_ref()
                    .unwrap()
                    .coverage(),
                1,
                0,
                0,
            ),
        )
        .unwrap(),
    );
    assert_eq!(content(&s), expected);
    assert_eq!(
        last_context(&mut s),
        json!({"target":"selection_mask","dx":1,"dy":0})
    );
    call(&mut s, "history.undo", json!({}));
    for (action, args) in [
        ("canvas.move", json!({"target":"selection","dx":1,"dy":0})),
        ("canvas.move", json!({"mask_id":group,"dx":1,"dy":0})),
        ("canvas.move", json!({"dx":1,"dy":0})),
        ("canvas.transform", json!({"translate_x":1})),
    ] {
        let before = Before::capture(&s);
        let error = s.apply(Action::new(action, args)).unwrap_err();
        assert_eq!(error.code, "mask_editing_pending", "{action}");
        before.restored(&s);
    }
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"layer_pixels"}}),
    );
    let before = Before::capture(&s);
    assert_eq!(
        s.apply(Action::new("canvas.move", json!({"dx":1,"dy":0})))
            .unwrap_err()
            .code,
        "mask_editing_pending"
    );
    before.restored(&s);
    assert_eq!(raw(&s)[&owner], before.content.masks[&owner]);
    outcome(&mut s);
}

#[test]
fn owner_movement_literal_conflicts_reject_before_prefix_but_new_mask_ids_resolve_at_execution() {
    let Fixture { mut s, owner, .. } = fixture(false, true, true);
    for (action, args) in [
        (
            "canvas.move",
            json!({"dx":1,"dy":0,"mask_id":owner,"target":"layer"}),
        ),
        (
            "canvas.move",
            json!({"dx":1,"dy":0,"mask_id":owner,"target":"selection"}),
        ),
        (
            "canvas.nudge",
            json!({"direction":"right","mask_id":owner,"target":"selection_mask"}),
        ),
        ("canvas.move", json!({"dx":1,"dy":0,"mask_id":0})),
        ("canvas.move", json!({"dx":1,"dy":0,"mask_id":2.5})),
        (
            "canvas.move",
            json!({"dx":1,"dy":0,"mask_id":4294967296u64}),
        ),
    ] {
        let before = Before::capture(&s);
        let error = s.apply(Action::new("pipeline.run", json!({"steps":[
            {"id":"prefix","action":"layer.rename","args":{"layer_id":1,"name":"Must not execute"}},
            {"id":"invalid","action":action,"args":args}
        ]}))).unwrap_err();
        assert!(!error.message.is_empty());
        before.failed_audit(&mut s, "pipeline.run");
    }
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"new","action":"mask.create","args":{"owner":{"kind":"layer","id":1},"source":"hide_all","select":false}},
            {"id":"move","action":"canvas.move","args":{"mask_id":{"$ref":"new#/result/mask_id"},"dx":1,"dy":0}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(raw(&s)[&1], shifted(&[0u8; W * H], 1, 0, 255));
    assert_eq!(s.doc.mask_state().target, PaintTarget::LayerPixels);
    let schema = s.registry.actions["canvas.move"].schema();
    assert_eq!(schema["properties"]["mask_id"]["type"], "integer");
    let nodes = call(&mut s, "node.catalog", json!({}));
    assert!(nodes.to_string().contains("action.canvas.move"));
    outcome(&mut s);
}

#[test]
fn owner_movement_mask_visibility_and_position_guards_use_the_owner_not_active_raster() {
    for guard in ["position", "layer_hidden", "group_hidden"] {
        let Fixture {
            mut s,
            owner,
            group_id,
            ..
        } = fixture(false, true, true);
        call(&mut s, "layer.select", json!({"layer_id":1}));
        match guard {
            "position" => {
                call(
                    &mut s,
                    "layer.set",
                    json!({"layer_id":2,"position_locked":true}),
                );
            }
            "layer_hidden" => {
                call(&mut s, "layer.set", json!({"layer_id":2,"visible":false}));
            }
            _ => {
                call(
                    &mut s,
                    "group.set",
                    json!({"group_id":group_id,"visible":false}),
                );
            }
        }
        let before = Before::capture(&s);
        assert!(
            s.apply(Action::new(
                "canvas.move",
                json!({"mask_id":owner,"dx":1,"dy":0})
            ))
            .is_err(),
            "{guard}"
        );
        before.restored(&s);
        outcome(&mut s);
    }
    for flag in ["pixel_locked", "alpha_locked"] {
        let Fixture { mut s, .. } = fixture(false, true, true);
        let mut args = json!({"layer_id":2});
        args[flag] = json!(true);
        call(&mut s, "layer.set", args);
        let before = Before::capture(&s);
        assert!(
            s.apply(Action::new(
                "canvas.move",
                json!({"target":"layer","dx":1,"dy":0})
            ))
            .is_err(),
            "{flag}"
        );
        before.restored(&s);
        outcome(&mut s);
    }
}

#[test]
fn owner_movement_identity_and_equal_uniform_results_retain_legacy_revision_rules() {
    let Fixture { mut s, owner, .. } = fixture(true, true, true);
    call(&mut s, "mask.fill", json!({"mask_id":owner,"value":255}));
    for args in [
        json!({"mask_id":owner,"dx":0,"dy":0}),
        json!({"mask_id":owner,"dx":2,"dy":-1}),
    ] {
        let before = Before::capture(&s);
        let result = call(&mut s, "canvas.move", args);
        assert_eq!(result, s.state());
        assert_eq!(content(&s), before.content);
        assert_eq!(s.doc.history_bytes(), before.history);
        assert_eq!(s.revision, before.revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events + 1);
        let event = s
            .journal
            .as_mut()
            .unwrap()
            .events(before.events, 1)
            .unwrap()["events"][0]
            .clone();
        assert_eq!(event["success"], true);
        assert_eq!(event["changed_tiles"], 0);
        assert_eq!(event["changed_mask_tiles"], 0);
    }
    outcome(&mut s);
}

fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 1. + (time - 100) as f32 * 0.02,
        y: 1.,
        pressure: 0.3 + (time - 100) as f32 * 0.005,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(kind),
        view_x: Some(40. + time as f32),
        view_y: Some(40.),
    }
}
fn sample_stream(s: &mut Session) -> Vec<Value> {
    let events = s.journal.as_mut().unwrap().events(0, 256).unwrap();
    let mut samples: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
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

#[test]
fn owner_movement_live_brush_and_hand_reject_without_ending_or_duplicating_samples() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let Fixture { mut s, owner, .. } = fixture(false, true, true);
        call(
            &mut s,
            "brush.set",
            json!({"radius":0.5,"stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 100)).unwrap();
        s.replay_boundary().unwrap(); // A recorded chunk does not end the native contact.
        s.pointer(pointer(kind, PointerPhase::Move, 120)).unwrap();
        s.doc.compose();
        for (action, args) in [
            ("canvas.move", json!({"target":"layer","dx":1,"dy":0})),
            ("canvas.move", json!({"mask_id":owner,"dx":0,"dy":0})),
            ("canvas.nudge", json!({"mask_id":owner,"direction":"right"})),
            (
                "canvas.move",
                json!({"target":"selection_mask","dx":1,"dy":0}),
            ),
        ] {
            let before = Before::capture(&s);
            let samples = sample_stream(&mut s);
            let storage = pointers(&s);
            let error = s.apply(Action::new(action, args)).unwrap_err();
            assert_eq!(error.code, "stroke_active", "{action}");
            before.restored(&s);
            assert_eq!(pointers(&s), storage);
            assert_eq!(sample_stream(&mut s), samples);
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            assert_eq!(s.stroke.last_sample().unwrap().time, 120);
        }
        s.pointer(pointer(kind, PointerPhase::Move, 140)).unwrap();
        s.pointer(pointer(kind, PointerPhase::Up, 160)).unwrap();
        assert_eq!(
            sample_stream(&mut s),
            [
                pointer(kind, PointerPhase::Down, 100),
                pointer(kind, PointerPhase::Move, 120),
                pointer(kind, PointerPhase::Move, 140),
                pointer(kind, PointerPhase::Up, 160),
            ]
            .iter()
            .map(|p| serde_json::to_value(p).unwrap())
            .collect::<Vec<_>>()
        );
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
        outcome(&mut s);
    }
}

#[test]
fn owner_movement_atomic_failure_and_pinned_history_keep_both_original_planes() {
    for mask_only in [false, true] {
        let Fixture { mut s, owner, .. } = fixture(false, true, true);
        let initial = content(&s);
        let args = if mask_only {
            json!({"mask_id":owner,"dx":1,"dy":0})
        } else {
            json!({"target":"layer","dx":1,"dy":0})
        };
        let before = Before::capture(&s);
        let failed = call(
            &mut s,
            "pipeline.run",
            json!({"atomic":true,"steps":[
                {"id":"move","action":"canvas.move","args":args},
                {"id":"fail","action":"layer.select","args":{"layer_id":4294967295u64}}
            ]}),
        );
        assert_eq!(failed["rolled_back"], true);
        before.failed_audit(&mut s, "pipeline.run");
        call(&mut s, "canvas.move", args);
        let moved = content(&s);
        let before = Before::capture(&s);
        // The movement history predates the outer checkpoint. Undo resurrects
        // its original data; mutating that data must not corrupt pinned history.
        let failed = call(
            &mut s,
            "pipeline.run",
            json!({"atomic":true,"steps":[
                {"id":"undo","action":"history.undo","args":{}},
                {"id":"mask","action":"mask.fill","args":{"mask_id":owner,"value":19}},
                {"id":"target","action":"mask.target","args":{"target":{"kind":"layer_pixels"}}},
                {"id":"pixels","action":"layer.fill_color","args":{"color":"#12AB34"}},
                {"id":"fail","action":"layer.select","args":{"layer_id":4294967295u64}}
            ]}),
        );
        assert_eq!(failed["rolled_back"], true);
        assert_eq!(
            failed["completed"],
            json!(["undo", "mask", "target", "pixels"])
        );
        assert_eq!(failed["failed_step"], "fail");
        before.failed_audit(&mut s, "pipeline.run");
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), initial);
        call(&mut s, "history.redo", json!({}));
        assert_eq!(content(&s), moved);
        outcome(&mut s);
    }
}

struct FailRender {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    failures: Cell<usize>,
}
impl Renderer for FailRender {
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.native.supports_mix()
    }
    fn supports_transparent(&self) -> bool {
        self.native.supports_transparent()
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        self.native.supports_blend_mode(mode)
    }
    fn reserve_mask_effects(&self, w: u32, h: u32, r: u32) -> Result<(), String> {
        self.native.reserve_mask_effects(w, h, r)
    }
    fn mask_effect_region(
        &self,
        r: &abi::MaskEffectRequest,
        s: &abi::CompositeMask,
        w: u32,
        h: u32,
        o: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.native.mask_effect_region(r, s, w, h, o)
    }
    fn render(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.native.render(n, l, w, h, o)
    }
    fn render_masked(
        &self,
        n: &[abi::CompositeNode],
        l: &[abi::CompositeLayer],
        m: &[abi::CompositeMask],
        w: u32,
        h: u32,
        o: &mut abi::Pixels,
    ) -> Result<(), String> {
        if self.armed.replace(false) {
            assert!(!o.data.is_null() && o.len > 0);
            unsafe {
                o.data.write(0xdeadc0de);
            }
            self.failures.set(self.failures.get() + 1);
            Err("Injected owner movement render failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}

#[test]
fn owner_movement_renderer_and_partial_journal_failure_restore_raw_history_and_valid_head() {
    for mask_only in [false, true] {
        let Fixture { mut s, owner, .. } = fixture(false, true, true);
        let args = if mask_only {
            json!({"mask_id":owner,"dx":-1,"dy":1})
        } else {
            json!({"target":"layer","dx":-1,"dy":1})
        };
        let renderer = Arc::new(FailRender {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            armed: Cell::new(false),
            failures: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        let before = Before::capture(&s);
        renderer.armed.set(true);
        assert_eq!(
            s.apply(Action::new("canvas.move", args.clone()))
                .unwrap_err()
                .code,
            "render_error"
        );
        assert_eq!(renderer.failures.get(), 1);
        before.failed_audit(&mut s, "canvas.move");
        outcome(&mut s);
        let before = Before::capture(&s);
        let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        assert!(s.apply(Action::new("canvas.move", args.clone())).is_err());
        before.restored(&s);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), before.events);
        assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
        assert!(s.replay_error.is_some());
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
        s.replay_error = None; // Fixture-only simulation of a recovered storage device.
        call(&mut s, "canvas.move", args);
        call(&mut s, "history.undo", json!({}));
        assert_eq!(content(&s), before.content);
        outcome(&mut s);
    }
}

unsafe extern "C" fn reject_a8_reframe(
    _r: *const abi::A8Reframe,
    _s: *const abi::CompositeMask,
    _w: u32,
    _h: u32,
    o: *mut abi::A8Pixels,
) -> i32 {
    if !o.is_null() && !(*o).data.is_null() && (*o).len > 0 {
        (*o).data.write(17);
    }
    abi::ERROR
}

#[test]
fn owner_movement_missing_renderer_sampler_and_failed_native_output_leave_no_half_move() {
    for mode in ["renderer", "sampler", "native"] {
        let Fixture { mut s, owner, .. } = fixture(false, true, true);
        let renderer = s.doc.renderer.clone();
        let mut sampler = None;
        if mode == "renderer" {
            s.doc.renderer = None;
        } else {
            let plugin =
                Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
            let old = plugin.replace_a8_sampler_for_test(None).unwrap();
            if mode == "native" {
                let mut fault = old;
                fault.reframe = Some(reject_a8_reframe);
                plugin.replace_a8_sampler_for_test(Some(fault));
            }
            sampler = Some(old);
        }
        let before = Before::capture(&s);
        assert!(
            s.apply(Action::new(
                "canvas.move",
                json!({"target":"layer","dx":1,"dy":0})
            ))
            .is_err(),
            "{mode}"
        );
        before.restored(&s);
        assert_eq!(raw(&s)[&owner], before.content.masks[&owner]);
        assert!(s.replay_error.is_none());
        s.doc.renderer = renderer;
        if let Some(original) = sampler {
            Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
                .unwrap()
                .replace_a8_sampler_for_test(Some(original));
        }
        outcome(&mut s);
    }
}

#[test]
fn owner_movement_workflow_editing_is_inert_and_project_archive_replays_without_dlls() {
    let Fixture { mut s, owner, .. } = fixture(false, true, true);
    let before = content(&s);
    call(
        &mut s,
        "workflow.set",
        json!({"graph":{"version":1,"nodes":[
        {"id":1,"type":"action.canvas.move","position":[0.,0.],"parameters":{"mask_id":owner,"dx":1,"dy":-1}}
    ],"edges":[]}}),
    );
    assert_eq!(rgba(&s), before.pixels);
    assert_eq!(raw(&s), before.masks);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":true}))["ok"],
        true
    );
    assert_eq!(raw(&s)[&owner], shifted(&before.masks[&owner], 1, -1, 255));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/owner-movement");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{}-workflow.mcp", std::process::id()));
    call(&mut s, "document.save", json!({"path":path}));
    let final_content = content(&s);
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(content(&reopened), final_content);
    outcome(&mut reopened);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut archive = Journal::import(&bytes).unwrap();
    let recorded = archive.preview(archive.entries.len() - 1).unwrap();
    let mut bare = Document::new(1, 1);
    recorded.restore_document(&mut bare);
    assert!(bare.renderer.is_none());
    assert_eq!(bare.composition(), s.doc.composition());
    assert_eq!(bare.composite, s.doc.composite);
    assert_eq!(
        bare.layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        rgba(&s)
    );
    for (id, expected) in raw(&s) {
        let plane = bare.mask_plane(id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(),
            expected
        );
    }
    std::fs::remove_file(path).unwrap();
}
