//! Whole-owner affine edits share exact targets, raw coverage and compound history.
use super::editing_tests::{call, check};
use super::*;
use crate::{
    engine::{
        document::Composition,
        graph::{Edge, Filter, Node, NodeKind, Renderer},
        raster_masks::{PlaneView, SamplePlane},
        replay::Journal,
        selection::{Mask, SelectionState},
        Document,
    },
    plugins::Registry,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

const W: usize = 5;
const H: usize = 3;
fn rasters(s: &Session) -> BTreeMap<u64, Vec<u32>> {
    s.doc
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (s.doc.layer_id(i), l.pixels.clone()))
        .collect()
}
fn masks(s: &Session) -> BTreeMap<u64, Vec<u8>> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            let plane = s.doc.mask_plane(m.id).unwrap();
            (m.id, (0..W * H).map(|i| plane.sample(i)).collect())
        })
        .collect()
}
fn pointers(s: &Session) -> (BTreeMap<u64, usize>, BTreeMap<u64, Option<usize>>) {
    (
        s.doc
            .layers
            .iter()
            .enumerate()
            .map(|(i, l)| (s.doc.layer_id(i), l.pixels.as_ptr() as usize))
            .collect(),
        s.doc
            .mask_state()
            .items
            .iter()
            .map(|m| {
                (
                    m.id,
                    match s.doc.mask_plane(m.id).unwrap() {
                        PlaneView::Dense(p) => Some(p.as_ptr() as usize),
                        PlaneView::Uniform(_) => None,
                    },
                )
            })
            .collect(),
    )
}
#[derive(Debug, PartialEq)]
struct Content {
    composition: Composition,
    rasters: BTreeMap<u64, Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
    active: usize,
}
fn content(s: &Session) -> Content {
    Content {
        composition: s.doc.composition(),
        rasters: rasters(s),
        masks: masks(s),
        active: s.doc.active,
    }
}
fn fixture() -> Session {
    fixture_with_registry(Registry::bundled())
}
fn fixture_with_registry(registry: Registry) -> Session {
    let mut s = Session::new(registry).unwrap();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    call(&mut s, "layer.add", json!({}));
    call(
        &mut s,
        "group.create",
        json!({"name":"Color","members":[{"kind":"layer","id":2}]}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":1,"compositing":"pass_through","opacity":193./255.}),
    );
    call(&mut s, "layer.select", json!({"layer_id":1}));
    call(&mut s, "layer.add", json!({}));
    assert_eq!(
        (0..s.doc.layers.len())
            .map(|i| s.doc.layer_id(i))
            .collect::<Vec<_>>(),
        [1, 3, 2]
    );
    for index in 0..s.doc.layers.len() {
        let id = s.doc.layer_id(index);
        s.doc.layers[index].pixels = (0..W * H)
            .map(|i| {
                ([0, 73, 137, 255][(i + id as usize) % 4] as u32) << 24
                    | ((i as u32 * 7919 + id as u32 * 3571 + 19) & 0xffffff)
            })
            .collect();
        s.doc.replay_tiles.insert((index, 0));
    }
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let mut graph = s.doc.graph.clone();
    let index = s.doc.layer_index(2).unwrap();
    let source = graph
        .nodes
        .iter()
        .find(|n| n.data == (NodeKind::Layer { index }))
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
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.}),
    );
    for owner in [
        json!({"kind":"layer","id":1}),
        json!({"kind":"layer","id":2}),
        json!({"kind":"layer","id":3}),
        json!({"kind":"group","id":1}),
    ] {
        let id = call(&mut s, "mask.create", json!({"owner":owner,"select":false}))["mask_id"]
            .as_u64()
            .unwrap();
        let plane = if id == 2 {
            SamplePlane::Uniform(91)
        } else {
            SamplePlane::Dense(
                (0..W * H)
                    .map(|i| (i * 37 + id as usize * 19) as u8)
                    .collect(),
            )
        };
        s.doc.replace_mask_plane(id, plane).unwrap();
    }
    s.replay_boundary().unwrap();
    call(&mut s, "mask.set", json!({"mask_id":1,"enabled":false}));
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":2,"density":173,"feather_radius_px":1}),
    );
    call(&mut s, "mask.set", json!({"mask_id":3,"linked":false}));
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
                        .map(|i| if i % 2 == 0 { 79 } else { 255 })
                        .collect(),
                )
                .unwrap(),
            ),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[2,3],"active_layer_id":3}),
    );
    call(
        &mut s,
        "layer.links.set",
        json!({"layer_ids":[1,3],"linked":true}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":4}}),
    );
    s.doc.compose();
    s
}
fn mapped(matrix: [f64; 6], i: usize) -> (f64, f64) {
    let [a, b, c, d, e, f] = matrix;
    let determinant = a * e - b * d;
    let (x, y) = ((i % W) as f64 + 0.5 - c, (i / W) as f64 + 0.5 - f);
    ((e * x - b * y) / determinant, (a * y - d * x) / determinant)
}
fn taps(x: f64, y: f64) -> (i64, i64, [u64; 4]) {
    let (x, y) = (x - 0.5, y - 0.5);
    let (a, b) = (
        ((x - x.floor()) * 65536.).round() as u64,
        ((y - y.floor()) * 65536.).round() as u64,
    );
    (
        x.floor() as i64,
        y.floor() as i64,
        [
            (65536 - a) * (65536 - b),
            a * (65536 - b),
            (65536 - a) * b,
            a * b,
        ],
    )
}
fn at<T: Copy>(input: &[T], x: i64, y: i64, fill: T) -> T {
    if x >= 0 && y >= 0 && x < W as i64 && y < H as i64 {
        input[y as usize * W + x as usize]
    } else {
        fill
    }
}
fn rgba_reference(input: &[u32], matrix: [f64; 6], bilinear: bool) -> Vec<u32> {
    (0..W * H)
        .map(|i| {
            let (x, y) = mapped(matrix, i);
            if !bilinear {
                return at(input, x.floor() as i64, y.floor() as i64, 0);
            }
            let (ix, iy, weights) = taps(x, y);
            if weights[0] == 1u64 << 32 {
                return at(input, ix, iy, 0);
            }
            let pixels = [
                at(input, ix, iy, 0),
                at(input, ix + 1, iy, 0),
                at(input, ix, iy + 1, 0),
                at(input, ix + 1, iy + 1, 0),
            ];
            let alpha: Vec<_> = pixels
                .iter()
                .zip(weights)
                .map(|(p, w)| u64::from(p >> 24) * w)
                .collect();
            let total: u64 = alpha.iter().sum();
            let a = (total + (1u64 << 31)) / (1u64 << 32);
            if a == 0 {
                return 0;
            }
            [0, 8, 16]
                .into_iter()
                .fold((a as u32) << 24, |word, shift| {
                    word | (((pixels
                        .iter()
                        .zip(&alpha)
                        .map(|(p, w)| u64::from((p >> shift) & 255) * w)
                        .sum::<u64>()
                        + total / 2)
                        / total) as u32)
                        << shift
                })
        })
        .collect()
}
fn a8_reference(input: &[u8], matrix: [f64; 6], bilinear: bool) -> Vec<u8> {
    (0..W * H)
        .map(|i| {
            let (x, y) = mapped(matrix, i);
            if !bilinear {
                return at(input, x.floor() as i64, y.floor() as i64, 255);
            }
            let (ix, iy, w) = taps(x, y);
            let samples = [
                at(input, ix, iy, 255),
                at(input, ix + 1, iy, 255),
                at(input, ix, iy + 1, 255),
                at(input, ix + 1, iy + 1, 255),
            ];
            ((samples
                .into_iter()
                .zip(w)
                .map(|(sample, weight)| u64::from(sample) * weight)
                .sum::<u64>()
                + (1u64 << 31))
                / (1u64 << 32)) as u8
        })
        .collect()
}
fn outcome(s: &mut Session) {
    check(s);
    let n = s.journal.as_ref().unwrap().entries.len();
    let state = s.replay_preview(n - 1, false).unwrap();
    assert!(state.matches_document(&s.doc));
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, expected) in masks(s) {
        let plane = restored.mask_plane(id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(),
            expected
        );
    }
}
fn unchanged_except_recipe(before: &Content, s: &Session) {
    let mut actual = s.doc.composition();
    actual.last_transform = before.composition.last_transform;
    assert_eq!(actual, before.composition);
    assert_eq!(s.doc.active, before.active);
}

#[test]
fn owner_affine_exact_cross_parent_set_uses_one_pivot_and_compound_history() {
    let mut s = fixture();
    let before = content(&s);
    let storage = pointers(&s);
    let revision = s.revision;
    let count = s.journal.as_ref().unwrap().entries.len();
    let plan = call(
        &mut s,
        "canvas.transform.plan",
        json!({"layer_ids":[2.0,1],"angle":90,"method":"nearest"}),
    );
    assert_eq!(plan["target"], "layers");
    assert_eq!(plan["layer_ids"], json!([2, 1]));
    assert_eq!(plan["mask_ids"], json!([2, 1]));
    assert_eq!(plan["source_bounds"], json!([0, 0, W, H]));
    assert_eq!(plan["matrix"], json!([0., -1., 4., 1., 0., -1.]));
    assert_eq!(plan["staged_bytes"], W * H * 10);
    assert_eq!(s.revision, revision);
    assert_eq!(content(&s), before);
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[2,1],"angle":90,"method":"nearest"}),
    );
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    let matrix = [0., -1., 4., 1., 0., -1.];
    for id in [2, 1] {
        assert_eq!(
            rasters(&s)[&id],
            rgba_reference(&before.rasters[&id], matrix, false)
        );
        assert_eq!(
            masks(&s)[&id],
            a8_reference(&before.masks[&id], matrix, false)
        );
    }
    assert_eq!(rasters(&s)[&3], before.rasters[&3]);
    for id in [3, 4] {
        assert_eq!(masks(&s)[&id], before.masks[&id]);
        assert_eq!(pointers(&s).1[&id], storage.1[&id]);
    }
    assert_eq!(pointers(&s).0[&3], storage.0[&3]);
    unchanged_except_recipe(&before, &s);
    let recipe = call(&mut s, "canvas.transform.state", json!({}));
    assert_eq!(recipe["target"], "owners");
    assert_eq!(
        recipe["owners"],
        json!({"version":1,"layer_ids":[2,1],"mask_ids":[2,1]})
    );
    let event = s.journal.as_mut().unwrap().events(count, 1).unwrap()["events"][0].clone();
    assert_eq!(
        event["source"]["context"]["transform"]["layer_ids"],
        json!([2, 1])
    );
    assert_eq!(
        event["source"]["context"]["transform"]["pivot"],
        json!([2.5, 1.5])
    );
    let after = content(&s);
    outcome(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), before);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(content(&s), after);
    outcome(&mut s);
    call(&mut s, "history.undo", json!({}));
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[2,1],"translate_x":0.5}),
    );
    for id in [2, 1] {
        assert_eq!(
            rasters(&s)[&id],
            rgba_reference(&before.rasters[&id], [1., 0., 0.5, 0., 1., 0.], true)
        );
        assert_eq!(
            masks(&s)[&id],
            a8_reference(&before.masks[&id], [1., 0., 0.5, 0., 1., 0.], true)
        );
    }
    unchanged_except_recipe(&before, &s);
    outcome(&mut s);
}

#[test]
fn owner_affine_mask_only_bilinear_ignores_pixel_locks_and_preserves_selection_and_target() {
    let mut s = fixture();
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"pixel_locked":true,"alpha_locked":true}),
    );
    let before = content(&s);
    let storage = pointers(&s);
    let plan = call(
        &mut s,
        "canvas.transform.plan",
        json!({"mask_id":2.0,"translate_x":0.5}),
    );
    assert_eq!(plan["target"], "raster_mask");
    assert_eq!(
        plan["owner_scope"],
        json!({"version":1,"layer_ids":[],"mask_ids":[2],"mask_owner_layer_id":2})
    );
    call(
        &mut s,
        "canvas.transform",
        json!({"mask_id":2,"translate_x":0.5}),
    );
    assert_eq!(
        masks(&s)[&2],
        a8_reference(&before.masks[&2], [1., 0., 0.5, 0., 1., 0.], true)
    );
    assert_eq!(masks(&s)[&2][0], 173, "half raw91 and half exterior255");
    assert_eq!(rasters(&s), before.rasters);
    unchanged_except_recipe(&before, &s);
    for id in [1, 3, 4] {
        assert_eq!(masks(&s)[&id], before.masks[&id]);
        assert_eq!(pointers(&s).1[&id], storage.1[&id]);
    }
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), before);
    call(&mut s, "history.redo", json!({}));
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":2}}),
    );
    let auto = call(
        &mut s,
        "canvas.transform.plan",
        json!({"target":"auto","translate_y":1}),
    );
    assert_eq!(auto["target"], "raster_mask");
    assert_eq!(auto["mask_ids"], json!([2]));
    outcome(&mut s);
}

#[test]
fn owner_affine_union_includes_disabled_raw_support_and_empty_fallback_keeps_identity_noop() {
    let mut s = fixture();
    for i in 0..s.doc.layers.len() {
        s.doc.layers[i].pixels.fill(0);
        s.doc.replay_tiles.insert((i, 0));
    }
    let slot = s.doc.layer_index(1).unwrap();
    s.doc.layers[slot].pixels[W + 1] = 0xff112233;
    let mut raw = vec![255; W * H];
    raw[2 * W + 4] = 254;
    s.doc
        .replace_mask_plane(1, SamplePlane::Dense(raw))
        .unwrap();
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let plan = call(
        &mut s,
        "canvas.transform.plan",
        json!({"layer_ids":[1],"angle":90}),
    );
    assert_eq!(plan["source_bounds"], json!([1, 1, 5, 3]));
    assert_eq!(plan["pivot_x"], 3.);
    assert_eq!(plan["pivot_y"], 2.);
    s.doc.layers[slot].pixels.fill(0);
    s.doc.replay_tiles.insert((slot, 0));
    s.doc
        .replace_mask_plane(1, SamplePlane::Uniform(255))
        .unwrap();
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let before = content(&s);
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let plan = call(&mut s, "canvas.transform.plan", json!({"layer_ids":[1]}));
    assert_eq!(plan["source_bounds"], json!([0, 0, W, H]));
    call(&mut s, "canvas.transform", json!({"layer_ids":[1]}));
    assert_eq!(content(&s), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.revision, revision + 1);
    let count = s.journal.as_ref().unwrap().entries.len();
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[1],"translate_x":1,"method":"nearest"}),
    );
    assert_eq!(rasters(&s), before.rasters);
    assert_eq!(masks(&s), before.masks);
    assert!(s.doc.last_transform().is_some());
    assert!(s.doc.history_bytes() > history);
    let event = s.journal.as_mut().unwrap().events(count, 1).unwrap()["events"][0].clone();
    assert_eq!(event["changed_tiles"], 0);
    assert_eq!(event["changed_mask_tiles"], 0);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), before);
    outcome(&mut s);
}

#[test]
fn owner_affine_repeat_uses_captured_ids_after_link_and_active_changes_and_rejects_stale_pairs() {
    let mut s = fixture();
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[2,1],"translate_x":1,"method":"nearest"}),
    );
    let recipe = call(&mut s, "canvas.transform.state", json!({}));
    call(
        &mut s,
        "layer.links.set",
        json!({"layer_ids":[2,3],"linked":true}),
    );
    call(&mut s, "layer.select", json!({"layer_id":3}));
    let before = content(&s);
    call(&mut s, "canvas.transform.repeat", json!({}));
    assert_eq!(call(&mut s, "canvas.transform.state", json!({})), recipe);
    for id in [2, 1] {
        assert_eq!(
            rasters(&s)[&id],
            rgba_reference(&before.rasters[&id], [1., 0., 1., 0., 1., 0.], false)
        );
        assert_eq!(
            masks(&s)[&id],
            a8_reference(&before.masks[&id], [1., 0., 1., 0., 1., 0.], false)
        );
    }
    assert_eq!(rasters(&s)[&3], before.rasters[&3]);
    assert_eq!(s.doc.active, before.active);
    let before = content(&s);
    assert!(s
        .apply(Action::new("canvas.transform.repeat_copy", json!({})))
        .is_err());
    assert_eq!(content(&s), before);
    call(&mut s, "mask.set", json!({"mask_id":1,"linked":false}));
    let before = content(&s);
    let revision = s.revision;
    assert!(s
        .apply(Action::new("canvas.transform.repeat", json!({})))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.revision, revision);
    outcome(&mut s);
}

#[test]
fn owner_affine_literal_conflicts_precede_macro_prefix_and_every_owner_lock_is_enforced() {
    let mut s = fixture();
    for args in [
        json!({"layer_ids":[1,1],"translate_x":1}),
        json!({"layer_ids":[1],"mask_id":1,"translate_x":1}),
        json!({"layer_ids":[1],"target":"selection","translate_x":1}),
        json!({"mask_id":1,"target":"layer","translate_x":1}),
        json!({"layer_ids":[1],"quad":[[0,0],[5,0],[5,3],[0,3]]}),
    ] {
        s.registry.actions.insert("test.invalid_owner_affine".into(),serde_json::from_value(json!({"id":"test.invalid_owner_affine","title":"Invalid affine",
            "steps":[{"action":"layer.rename","args":{"name":"must not run"}},{"action":"canvas.transform","args":args}]})).unwrap());
        let before = content(&s);
        assert_eq!(
            s.apply(Action::new("test.invalid_owner_affine", json!({})))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(content(&s), before);
    }
    for property in ["pixel_locked", "position_locked", "alpha_locked"] {
        call(&mut s, "layer.set", json!({"layer_id":1,(property):true}));
        let before = content(&s);
        let revision = s.revision;
        assert!(s
            .apply(Action::new(
                "canvas.transform",
                json!({"layer_ids":[2,1],"translate_x":1,"method":"nearest"})
            ))
            .is_err());
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        call(&mut s, "layer.set", json!({"layer_id":1,(property):false}));
    }
    for args in [
        json!({"target":"selection","translate_x":1}),
        json!({"mask_id":4,"translate_x":1}),
        json!({"layer_ids":[1,99],"translate_x":1}),
    ] {
        let before = content(&s);
        assert!(s.apply(Action::new("canvas.transform", args)).is_err());
        assert_eq!(content(&s), before);
    }
    outcome(&mut s);
}

#[test]
fn owner_affine_atomic_and_partial_journal_failures_preserve_raw_recipe_and_saved_history() {
    let mut s = fixture();
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[2,1],"translate_x":1,"method":"nearest"}),
    );
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"transform","action":"canvas.transform","args":{"layer_ids":[1,2],"translate_y":1,"method":"nearest"}},
            {"id":"fail","action":"layer.select","args":{"index":15}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    assert_eq!(result["completed"], json!(["transform"]));
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    let count = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"mask_id":2,"translate_y":1,"method":"nearest"})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    s.replay_error = None; // Fixture-only recovered storage; production remains faulted.
    call(&mut s, "history.undo", json!({}));
    let restored = content(&s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(content(&s), before);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"undo","action":"history.undo"},{"id":"edit","action":"mask.fill","args":{"mask_id":2,"value":17}},
            {"id":"fail","action":"layer.select","args":{"index":15}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    assert_eq!(content(&s), before);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), restored);
    outcome(&mut s);
}

fn pointer(tool: ToolKind, phase: PointerPhase, i: u32) -> Pointer {
    Pointer {
        phase,
        x: 1.5 + (i % 2) as f32 * 0.1,
        y: 1.5,
        pressure: 0.2 + (i % 7) as f32 * 0.1,
        time: 100 + i,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
        view_x: Some(40. + i as f32),
        view_y: Some(40.),
    }
}
fn stream(s: &mut Session) -> Vec<Value> {
    let events = s.journal.as_mut().unwrap().events(0, 256).unwrap();
    let mut all: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    all.extend(
        s.replay_samples
            .iter()
            .map(|p| serde_json::to_value(p).unwrap()),
    );
    all
}
#[test]
fn owner_affine_plans_observe_and_mutations_reject_chunked_brush_and_hand_without_sample_loss() {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        let mut s = fixture();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1.5,"hardness":1,"stabilizer":0.55,"pressure_size":true,"pressure_opacity":true}),
        );
        let original_masks = masks(&s);
        let mut transcript = Vec::new();
        for i in 0..280 {
            let p = pointer(
                tool,
                if i == 0 {
                    PointerPhase::Down
                } else {
                    PointerPhase::Move
                },
                i,
            );
            s.pointer(p).unwrap();
            transcript.push(serde_json::to_value(p).unwrap());
        }
        s.replay_boundary().unwrap();
        if tool == ToolKind::Brush {
            assert_ne!(
                masks(&s)[&4],
                original_masks[&4],
                "the active A8 contact must actually paint"
            );
        }
        let before = content(&s);
        let public = s.state();
        let history = s.doc.history_bytes();
        let plan = call(
            &mut s,
            "canvas.transform.plan",
            json!({"layer_ids":[2,1],"angle":10}),
        );
        assert_eq!(plan["contact_active"], true);
        assert_eq!(content(&s), before);
        assert_eq!(s.state(), public);
        for args in [
            json!({"layer_ids":[2,1],"translate_x":1}),
            json!({"mask_id":2,"translate_x":0}),
        ] {
            assert_eq!(
                s.apply(Action::new("canvas.transform", args))
                    .unwrap_err()
                    .code,
                "stroke_active"
            );
            assert_eq!(content(&s), before);
            assert_eq!(s.state(), public);
            assert_eq!(s.doc.history_bytes(), history);
            assert_eq!(stream(&mut s), transcript);
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        }
        let p = pointer(tool, PointerPhase::Up, 280);
        s.pointer(p).unwrap();
        transcript.push(serde_json::to_value(p).unwrap());
        assert_eq!(stream(&mut s), transcript);
        assert!(!s.stroke.active());
        if tool == ToolKind::Brush {
            call(&mut s, "history.undo", json!({}));
        }
        assert_eq!(
            masks(&s),
            original_masks,
            "one Brush Undo or an unchanged Hand restores all raw planes"
        );
        outcome(&mut s);
    }
}

#[test]
fn owner_affine_workflow_binds_explicit_link_state_and_projects_restore_without_dlls() {
    let mut s = fixture();
    let before = content(&s);
    let catalog = call(&mut s, "node.catalog", json!({}));
    for id in ["action.canvas.transform", "action.canvas.transform.plan"] {
        let node = catalog["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap();
        assert_eq!(
            node["parameters_schema"]["properties"]["layer_ids"]["maxItems"],
            16
        );
        assert_eq!(
            node["parameters_schema"]["properties"]["mask_id"]["minimum"],
            1
        );
    }
    let graph = json!({"version":1,"nodes":[{"id":1,"type":"state.layers.links","position":[0,0]},
        {"id":2,"type":"action.canvas.transform","position":[400,0],"parameters":{"translate_x":1,"method":"nearest"}}],
        "edges":[{"from":1,"output":"active_linked_layer_ids","to":2,"input":"layer_ids"}]});
    let history = s.doc.history_bytes();
    let revision = s.revision;
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"graph":graph,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(content(&s), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.revision, revision);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"graph":graph}))["ok"],
        true
    );
    for id in [1, 3] {
        assert_eq!(
            rasters(&s)[&id],
            rgba_reference(&before.rasters[&id], [1., 0., 1., 0., 1., 0.], false)
        );
    }
    assert_eq!(rasters(&s)[&2], before.rasters[&2]);
    assert_eq!(masks(&s)[&3], before.masks[&3]);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/owner-affine/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let project = directory.join("owners.mcp");
    call(&mut s, "document.save", json!({"path":project}));
    let expected = content(&s);
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":project}));
    assert_eq!(content(&opened), expected);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    outcome(&mut s);
}

static AFFINE_CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn fail_second_affine(
    _: *const abi::Affine,
    _: *const abi::Pixels,
    _: *const abi::Coverage,
    output: *mut abi::Pixels,
) -> i32 {
    let count = AFFINE_CALLS.fetch_add(1, Ordering::SeqCst);
    std::ptr::write_bytes((*output).data, 0, (*output).len);
    if count == 0 {
        abi::OK
    } else {
        abi::ERROR
    }
}
#[test]
fn owner_affine_missing_capabilities_and_late_native_failure_publish_no_partial_owner_set() {
    let mut s = fixture();
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let original_pixels = native.replace_affine_pixels_for_test(Some(fail_second_affine));
    let original_a8 = native.replace_a8_affine_for_test(None);
    AFFINE_CALLS.store(0, Ordering::SeqCst);
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"layer_ids":[2,1],"translate_x":1})
        ))
        .is_err());
    assert_eq!(AFFINE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    native.replace_a8_affine_for_test(original_a8);
    let original_bounds = native.replace_a8_affine_bounds_for_test(None);
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"layer_ids":[2,1],"translate_x":1})
        ))
        .is_err());
    assert_eq!(AFFINE_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(content(&s), before);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_a8_affine_bounds_for_test(original_bounds);
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"layer_ids":[2,1],"translate_x":1})
        ))
        .is_err());
    assert_eq!(AFFINE_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_affine_pixels_for_test(original_pixels);
    outcome(&mut s);
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
    fn reserve_mask_effects(&self, width: u32, height: u32, radius: u32) -> Result<(), String> {
        self.native.reserve_mask_effects(width, height, radius)
    }
    fn mask_effect_region(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        width: u32,
        height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        self.native
            .mask_effect_region(request, source, width, height, output)
    }
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.native.render(nodes, layers, width, height, output)
    }
    fn render_masked(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        if self.armed.replace(false) {
            unsafe { output.data.write(0xdeadc0de) };
            self.failures.set(self.failures.get() + 1);
            return Err("Injected post-publication affine renderer failure".into());
        }
        self.native
            .render_masked(nodes, layers, masks, width, height, output)
    }
}
#[test]
fn owner_affine_post_publication_renderer_failure_restores_recipe_raw_planes_and_history() {
    let mut s = fixture();
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let surface = s.doc.composite.clone();
    let renderer = Arc::new(FailRender {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(true),
        failures: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    assert!(s
        .apply(Action::new(
            "canvas.transform",
            json!({"layer_ids":[2,1],"translate_x":0.5})
        ))
        .is_err());
    assert_eq!(renderer.failures.get(), 1);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composite, surface);
    assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
    outcome(&mut s);
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[2,1],"translate_x":0.5}),
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), before);
    outcome(&mut s);
}

#[test]
fn owner_affine_pipeline_resolves_new_owner_ids_after_prior_structural_steps() {
    // The fill action is contributed by the optional native-paint package.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let mut s = fixture_with_registry(Registry::load(&root).unwrap());
    call(&mut s, "selection.deselect", json!({}));
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"new","action":"layer.add"},
            {"id":"fill","action":"layer.fill_color","args":{"color":"#225599"}},
            {"id":"transform","action":"canvas.transform","args":{"layer_ids":[{"$ref":"new#/result/layer_id"}],"translate_x":1,"method":"nearest"}}
        ]}),
    );
    assert_eq!(result["ok"], true);
    assert_eq!(result["completed"], json!(["new", "fill", "transform"]));
    let id = result["results"]["new"]["result"]["layer_id"]
        .as_u64()
        .unwrap();
    assert_eq!(
        rasters(&s)[&id],
        rgba_reference(&vec![0xff225599; W * H], [1., 0., 1., 0., 1., 0.], false)
    );
    let recipe = call(&mut s, "canvas.transform.state", json!({}));
    assert_eq!(
        recipe["owners"],
        json!({"version":1,"layer_ids":[id],"mask_ids":[]})
    );
    outcome(&mut s);
}
