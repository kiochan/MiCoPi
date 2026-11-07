//! Exact raster sets align/distribute by raw ink, independently of painting state.
use super::editing_tests::{call, check};
use super::*;
use crate::{
    engine::{
        document::Composition,
        raster_masks::SamplePlane,
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
const W: usize = 10;
const H: usize = 8;
const BOXES: [[i32; 4]; 4] = [[1, 1, 3, 3], [5, 2, 8, 5], [2, 5, 4, 6], [0, 0, 1, 1]];
const IDS: [u64; 3] = [3, 1, 2];
fn fixture() -> Session {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let mut s = Session::new(Registry::load(&root).unwrap()).unwrap();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    for _ in 1..4 {
        call(&mut s, "layer.add", json!({}));
    }
    for (index, layer) in s.doc.layers.iter_mut().enumerate() {
        let [left, top, right, bottom] = BOXES[index];
        for (i, pixel) in layer.pixels.iter_mut().enumerate() {
            let (x, y) = ((i % W) as i32, (i / W) as i32);
            *pixel = if (left..right).contains(&x) && (top..bottom).contains(&y) {
                0x80335577 + index as u32 * 0x10101
            } else {
                0x00123456
            };
        }
        s.doc.replay_tiles.insert((index, 0));
    }
    s.doc.invalidate();
    s.doc.compose();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}]}),
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":1,"compositing":"pass_through","opacity":193./255.}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":2,"opacity":137./255.}),
    );
    for owner in 1..=3 {
        let id = call(
            &mut s,
            "mask.create",
            json!({"owner":{"kind":"layer","id":owner},"select":false}),
        )["mask_id"]
            .as_u64()
            .unwrap();
        s.doc
            .replace_mask_plane(
                id,
                if owner == 2 {
                    SamplePlane::Uniform(91)
                } else {
                    SamplePlane::Dense(
                        (0..W * H)
                            .map(|i| (i * 37 + owner as usize * 11) as u8)
                            .collect(),
                    )
                },
            )
            .unwrap();
    }
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":1,"density":173,"feather_radius_px":1}),
    );
    call(&mut s, "mask.set", json!({"mask_id":2,"enabled":false}));
    call(&mut s, "mask.set", json!({"mask_id":3,"linked":false}));
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"group","id":1},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":0}));
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":mask}}),
    );
    call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"compositing.filter","parameters":{"filter":"invert","amount":0.25},"parent_id":1}),
    );
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
            previous: Some(Mask::new(W, H, vec![79; W * H]).unwrap()),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,3,4],"active_layer_id":4}),
    );
    call(
        &mut s,
        "layer.links.set",
        json!({"layer_ids":[1,4],"linked":true}),
    );
    call(
        &mut s,
        "canvas.transform",
        json!({"layer_ids":[4],"translate_x":1,"method":"nearest"}),
    );
    s
}
fn pixels(s: &Session) -> BTreeMap<u64, Vec<u32>> {
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
            let p = s.doc.mask_plane(m.id).unwrap();
            (m.id, (0..W * H).map(|i| p.sample(i)).collect())
        })
        .collect()
}
#[derive(Debug, PartialEq)]
struct Content {
    composition: Composition,
    pixels: BTreeMap<u64, Vec<u32>>,
    masks: BTreeMap<u64, Vec<u8>>,
    surface: Vec<u32>,
    active: usize,
}
fn content(s: &Session) -> Content {
    Content {
        composition: s.doc.composition(),
        pixels: pixels(s),
        masks: masks(s),
        surface: s.doc.composite.clone(),
        active: s.doc.active,
    }
}
fn shifted<T: Copy>(source: &[T], dx: i32, dy: i32, fill: T) -> Vec<T> {
    (0..W * H)
        .map(|i| {
            let (x, y) = ((i % W) as i32 - dx, (i / W) as i32 - dy);
            if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
                source[y as usize * W + x as usize]
            } else {
                fill
            }
        })
        .collect()
}
fn rounded(n: i64, d: i64) -> i32 {
    let sign = if n < 0 { -1 } else { 1 };
    (sign * ((n.abs() + d / 2) / d)) as i32
}
fn reference(mode: &str, canvas: bool, ids: &[u64]) -> BTreeMap<u64, (i32, i32)> {
    let bounds = if canvas {
        [0, 0, W as i32, H as i32]
    } else {
        [
            ids.iter()
                .map(|id| BOXES[*id as usize - 1][0])
                .min()
                .unwrap(),
            ids.iter()
                .map(|id| BOXES[*id as usize - 1][1])
                .min()
                .unwrap(),
            ids.iter()
                .map(|id| BOXES[*id as usize - 1][2])
                .max()
                .unwrap(),
            ids.iter()
                .map(|id| BOXES[*id as usize - 1][3])
                .max()
                .unwrap(),
        ]
    };
    let horizontal = mode == "distribute_horizontal_centers";
    let vertical = mode == "distribute_vertical_centers";
    let mut sorted = ids.to_vec();
    let axis = usize::from(vertical);
    if horizontal || vertical {
        sorted.sort_by_key(|id| {
            let b = BOXES[*id as usize - 1];
            (b[axis] + b[axis + 2], *id)
        });
    }
    ids.iter()
        .map(|id| {
            let b = BOXES[*id as usize - 1];
            let (dx, dy) = match mode {
                "left" => (bounds[0] - b[0], 0),
                "right" => (bounds[2] - b[2], 0),
                "horizontal_center" => (
                    rounded(i64::from(bounds[0] + bounds[2] - b[0] - b[2]), 2),
                    0,
                ),
                "top" => (0, bounds[1] - b[1]),
                "bottom" => (0, bounds[3] - b[3]),
                "vertical_center" => (
                    0,
                    rounded(i64::from(bounds[1] + bounds[3] - b[1] - b[3]), 2),
                ),
                _ => {
                    let first = BOXES[sorted[0] as usize - 1];
                    let last = BOXES[*sorted.last().unwrap() as usize - 1];
                    let start = i64::from(first[axis] + first[axis + 2]);
                    let end = i64::from(last[axis] + last[axis + 2]);
                    let rank = sorted.iter().position(|v| v == id).unwrap() as i64;
                    let d = (ids.len() - 1) as i64;
                    let delta = rounded(
                        start * d + (end - start) * rank - i64::from(b[axis] + b[axis + 2]) * d,
                        2 * d,
                    );
                    if horizontal {
                        (delta, 0)
                    } else {
                        (0, delta)
                    }
                }
            };
            (*id, (dx, dy))
        })
        .collect()
}
fn outcome(s: &mut Session) {
    check(s);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut j = Journal::import(&bytes).unwrap();
    assert_eq!(j.verify(None).unwrap()["valid"], true);
    let state = j.preview(j.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, raw) in masks(s) {
        let plane = restored.mask_plane(id).unwrap();
        assert_eq!((0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(), raw);
    }
}

#[test]
fn layer_arrangement_plans_and_raw_outputs_follow_independent_alignment_and_distribution() {
    for mode in [
        "left",
        "horizontal_center",
        "right",
        "top",
        "vertical_center",
        "bottom",
        "distribute_horizontal_centers",
        "distribute_vertical_centers",
    ] {
        let mut s = fixture();
        let before = content(&s);
        let public = s.state();
        let history = s.doc.history_bytes();
        let stable = s.doc.layers[s.doc.layer_index(4).unwrap()].pixels.as_ptr();
        let revision = s.revision;
        let events = s.journal.as_ref().unwrap().entries.len();
        let offsets = reference(mode, false, &IDS);
        let request = json!({"layer_ids":IDS,"mode":mode});
        let plan = call(&mut s, "layer.arrange.plan", request.clone());
        assert_eq!(plan["layer_ids"], json!(IDS));
        assert_eq!(plan["reference"], "targets");
        assert_eq!(plan["reference_bounds"], json!([1, 1, 8, 6]));
        assert_eq!(plan["linked_mask_ids"], json!([1, 2]));
        assert_eq!(plan["staged_bytes"], W * H * (4 * 3 + 2));
        assert_eq!(plan["staging_limit_bytes"], 192 * 1024 * 1024);
        assert_eq!(plan["can_apply"], true);
        assert_eq!(plan["contact_active"], false);
        for (row, id) in plan["targets"].as_array().unwrap().iter().zip(IDS) {
            assert_eq!(row["layer_id"], id);
            assert_eq!(row["bounds"], json!(BOXES[id as usize - 1]));
            assert_eq!(
                (
                    row["dx"].as_i64().unwrap() as i32,
                    row["dy"].as_i64().unwrap() as i32
                ),
                offsets[&id]
            );
        }
        assert_eq!(content(&s), before);
        assert_eq!(s.state(), public);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
        let result = call(&mut s, "layer.arrange", request);
        assert_eq!(result["revision"], revision + 1);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
        assert_eq!(s.doc.composition(), before.composition);
        assert_eq!(s.doc.active, before.active);
        assert_eq!(
            s.doc.layers[s.doc.layer_index(4).unwrap()].pixels.as_ptr(),
            stable
        );
        for id in IDS {
            let (dx, dy) = offsets[&id];
            assert_eq!(pixels(&s)[&id], shifted(&before.pixels[&id], dx, dy, 0));
        }
        for (id, mask) in [(1, 1), (2, 2)] {
            let (dx, dy) = offsets[&id];
            assert_eq!(masks(&s)[&mask], shifted(&before.masks[&mask], dx, dy, 255));
        }
        assert_eq!(masks(&s)[&3], before.masks[&3]);
        assert_eq!(masks(&s)[&4], before.masks[&4]);
        assert_eq!(pixels(&s)[&4], before.pixels[&4]);
        let event = s.journal.as_mut().unwrap().events(events, 1).unwrap()["events"][0].clone();
        assert_eq!(
            event["source"]["context"]["arrangement"],
            json!({"mode":mode,"reference":plan["reference"],
            "reference_bounds":plan["reference_bounds"],"targets":plan["targets"]})
        );
        let after = content(&s);
        if offsets.values().any(|offset| *offset != (0, 0)) {
            call(&mut s, "history.undo", json!({}));
            assert_eq!(content(&s), before);
            call(&mut s, "history.redo", json!({}));
            assert_eq!(content(&s), after);
        } else {
            assert_eq!(s.doc.history_bytes(), history);
            assert_eq!(result["changed"], false);
        }
        outcome(&mut s);
    }
}

#[test]
fn layer_arrangement_canvas_center_rounds_signed_half_ties_and_ignores_raster_links() {
    let mut s = fixture();
    let before = content(&s);
    let plan = call(
        &mut s,
        "layer.arrange.plan",
        json!({"layer_ids":[2.0,1],"mode":"horizontal_center","reference":"canvas"}),
    );
    assert_eq!(plan["reference_bounds"], json!([0, 0, W, H]));
    assert_eq!(plan["targets"][0]["dx"], -2);
    assert_eq!(plan["targets"][1]["dx"], 3);
    call(
        &mut s,
        "layer.arrange",
        json!({"layer_ids":[2,1],"mode":"horizontal_center","reference":"canvas"}),
    );
    assert_eq!(pixels(&s)[&1], shifted(&before.pixels[&1], 3, 0, 0));
    assert_eq!(pixels(&s)[&2], shifted(&before.pixels[&2], -2, 0, 0));
    assert_eq!(pixels(&s)[&3], before.pixels[&3]);
    assert_eq!(pixels(&s)[&4], before.pixels[&4]);
    assert_eq!(s.doc.composition(), before.composition);
    outcome(&mut s);
}

#[test]
fn layer_arrangement_invalid_literals_precede_macro_prefix_and_empty_or_locked_noops_reject() {
    let mut s = fixture();
    for arguments in [
        json!({"layer_ids":[1],"mode":"left"}),
        json!({"layer_ids":[1,1.0],"mode":"left"}),
        json!({"layer_ids":[1,2],"mode":"distribute_horizontal_centers"}),
        json!({"layer_ids":[1,2,3],"mode":"distribute_vertical_centers","reference":"canvas"}),
        json!({"layer_ids":[1,2],"mode":"unknown"}),
        json!({"layer_ids":[],"mode":"left"}),
    ] {
        s.registry.actions.insert("test.bad-arrangement".into(),serde_json::from_value(json!({"id":"test.bad-arrangement","title":"Bad suffix",
            "steps":[{"action":"layer.rename","args":{"name":"Must not run"}},{"action":"layer.arrange","args":arguments}]})).unwrap());
        let before = content(&s);
        let revision = s.revision;
        assert!(s
            .apply(Action::new("test.bad-arrangement", json!({})))
            .is_err());
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
    }
    for property in ["pixel_locked", "position_locked", "alpha_locked"] {
        call(&mut s, "layer.set", json!({"layer_id":4,(property):true}));
        let before = content(&s);
        let revision = s.revision;
        let mode = if property == "alpha_locked" {
            "right"
        } else {
            "top"
        };
        assert!(s
            .apply(Action::new(
                "layer.arrange",
                json!({"layer_ids":[4],"reference":"canvas","mode":mode})
            ))
            .is_err());
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        call(&mut s, "layer.set", json!({"layer_id":4,(property):false}));
    }
    call(&mut s, "layer.set", json!({"layer_id":3,"visible":false}));
    let before = content(&s);
    assert!(s
        .apply(Action::new(
            "layer.arrange",
            json!({"layer_ids":IDS,"mode":"left"})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    call(&mut s, "layer.set", json!({"layer_id":3,"visible":true}));
    let index = s.doc.layer_index(3).unwrap();
    s.doc.layers[index].pixels.fill(0x00123456);
    s.doc.replay_tiles.insert((index, 0));
    s.doc.invalidate();
    s.doc.compose();
    s.replay_boundary().unwrap();
    let before = content(&s);
    assert!(s
        .apply(Action::new(
            "layer.arrange.plan",
            json!({"layer_ids":IDS,"mode":"left"})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    outcome(&mut s);
}

fn sample(tool: ToolKind, phase: PointerPhase, i: u32) -> Pointer {
    Pointer {
        phase,
        x: 1.5 + (i % 2) as f32 * 0.1,
        y: 1.5,
        pressure: 0.3 + (i % 5) as f32 * 0.1,
        time: 100 + i,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
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
fn layer_arrangement_plans_preserve_chunked_brush_hand_and_apply_keeps_idle_guard() {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        let mut s = fixture();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1.2,"hardness":1,"stabilizer":0,"pressure_size":false}),
        );
        s.pointer(sample(tool, PointerPhase::Down, 0)).unwrap();
        for i in 1..270 {
            s.pointer(sample(tool, PointerPhase::Move, i)).unwrap();
        }
        let before = content(&s);
        let revision = s.revision;
        let samples = transcript(&mut s);
        assert_eq!(samples.len(), 270);
        let plan = call(
            &mut s,
            "layer.arrange.plan",
            json!({"layer_ids":IDS,"mode":"left"}),
        );
        assert_eq!(plan["contact_active"], true);
        assert_eq!(plan["can_apply"], false);
        assert_eq!(
            s.apply(Action::new(
                "layer.arrange",
                json!({"layer_ids":IDS,"mode":"left"})
            ))
            .unwrap_err()
            .code,
            "stroke_active"
        );
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
        assert_eq!(transcript(&mut s), samples);
        s.pointer(sample(tool, PointerPhase::Up, 270)).unwrap();
        let samples = transcript(&mut s);
        assert_eq!(samples.len(), 271);
        assert_eq!(samples.last().unwrap()["phase"], "up");
        outcome(&mut s);
    }
}

static CALLS: AtomicUsize = AtomicUsize::new(0);
unsafe extern "C" fn fail_second(
    _: *const abi::Move,
    _: *const abi::Pixels,
    _: *const abi::Coverage,
    out: *mut abi::Pixels,
) -> i32 {
    if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        std::ptr::write_bytes((*out).data, 0, (*out).len);
        abi::OK
    } else {
        abi::ERROR
    }
}
#[test]
fn layer_arrangement_late_native_atomic_and_partial_journal_failures_restore_every_owner() {
    let mut s = fixture();
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let callback = native.replace_movement_pixels_for_test(Some(fail_second));
    assert!(callback.is_some());
    CALLS.store(0, Ordering::SeqCst);
    assert!(s
        .apply(Action::new(
            "layer.arrange",
            json!({"layer_ids":IDS,"mode":"right"})
        ))
        .is_err());
    assert_eq!(CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_movement_pixels_for_test(callback);
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
        {"id":"arrange","action":"layer.arrange","args":{"layer_ids":IDS,"mode":"bottom"}},
        {"id":"fail","action":"layer.select","args":{"index":15}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    let count = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "layer.arrange",
            json!({"layer_ids":IDS,"mode":"left"})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
}

#[test]
fn layer_arrangement_typed_bindings_project_and_dll_free_outcomes_keep_exact_scope() {
    let mut s = fixture();
    let before = content(&s);
    let revision = s.revision;
    let nodes = s.registry.node_catalog();
    assert!(nodes["action.layer.arrange.plan"].read_only);
    assert!(!nodes["action.layer.arrange"].read_only);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.layer.arrange.plan","position":[0,0],"parameters":{"layer_ids":IDS,"mode":"left"}},
        {"id":2,"type":"action.layer.arrange","position":[350,0],"parameters":{"mode":"left"}}],
        "edges":[{"from":1,"output":"layer_ids","to":2,"input":"layer_ids"}]});
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"graph":graph,"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(content(&s), before);
    assert_eq!(s.revision, revision);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"graph":graph,"atomic":true}))["ok"],
        true
    );
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/layer-arrangement/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("arranged.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let expected = content(&s);
    let mut opened = fixture();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(content(&opened), expected);
    outcome(&mut s);
}

struct FailRender {
    native: Arc<dyn crate::engine::graph::Renderer>,
    armed: Cell<bool>,
    calls: Cell<usize>,
}
impl crate::engine::graph::Renderer for FailRender {
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
    fn supports_blend_mode(&self, m: u32) -> bool {
        self.native.supports_blend_mode(m)
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
            unsafe { o.data.write(0xdeadc0de) };
            self.calls.set(self.calls.get() + 1);
            Err("Injected arranged-surface failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
#[test]
fn layer_arrangement_post_publication_renderer_failure_and_missing_sampler_keep_all_sources() {
    let mut s = fixture();
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let renderer = Arc::new(FailRender {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(true),
        calls: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    assert!(s
        .apply(Action::new(
            "layer.arrange",
            json!({"layer_ids":IDS,"mode":"right"})
        ))
        .is_err());
    assert_eq!(renderer.calls.get(), 1);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    s.doc.renderer = Some(renderer.native.clone());
    let native = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap()).unwrap();
    let sampler = native.replace_a8_sampler_for_test(None);
    assert!(sampler.is_some());
    assert!(s
        .apply(Action::new(
            "layer.arrange.plan",
            json!({"layer_ids":IDS,"mode":"left"})
        ))
        .is_err());
    assert!(s
        .apply(Action::new(
            "layer.arrange",
            json!({"layer_ids":IDS,"mode":"left"})
        ))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.doc.history_bytes(), history);
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_a8_sampler_for_test(sampler);
    outcome(&mut s);
}
