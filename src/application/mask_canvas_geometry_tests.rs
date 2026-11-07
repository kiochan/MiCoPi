//! Complete document geometry through real native samplers and shared Session contracts.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document::Composition,
    document_info::{Density, DensityUnit, Info},
    graph::{Edge, Filter, Node, NodeKind, Renderer},
    raster_masks::{MaskOwner, PlaneView, SamplePlane},
    selection::{Mask, SelectionState},
    Document,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/mask-canvas-geometry");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn raw(s: &Session) -> Vec<(u64, Vec<u8>)> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|metadata| {
            let data = match s.doc.mask_plane(metadata.id).unwrap() {
                PlaneView::Uniform(value) => vec![value; s.doc.width * s.doc.height],
                PlaneView::Dense(data) => data.to_vec(),
            };
            (metadata.id, data)
        })
        .collect()
}
fn pixels(s: &Session) -> Vec<Vec<u32>> {
    s.doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect()
}
fn fixture() -> Session {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":5,"height":3}));
    call(&mut s, "layer.add", json!({}));
    let group = call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":2}],"name":"Hidden group"}),
    )["group_id"]
        .as_u64()
        .unwrap();
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"visible":false}),
    );
    call(
        &mut s,
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":2}),
    );
    for (layer, data) in s.doc.layers.iter_mut().enumerate() {
        for (index, pixel) in data.pixels.iter_mut().enumerate() {
            let alpha = [0u32, 1, 73, 128, 255][(index + layer) % 5];
            *pixel = alpha << 24 | ((index as u32 * 7919 + layer as u32 * 3571) & 0xffffff);
        }
    }
    // These independent fixture words bypass a native loan. Register both raw
    // tiles before any action checkpoint; display damage alone is not raw damage.
    s.doc.replay_tiles.extend([(0, 0), (1, 0)]);
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let mut graph = s.doc.graph.clone();
    let source = graph
        .nodes
        .iter()
        .find(|node| matches!(node.data, NodeKind::Layer { index: 0 }))
        .unwrap()
        .id;
    let id = graph.nodes.iter().map(|node| node.id).max().unwrap() + 1;
    for edge in &mut graph.edges {
        if edge.from == source {
            edge.from = id;
        }
    }
    graph.nodes.push(Node {
        id,
        node_type: None,
        position: [0., 0.],
        data: NodeKind::Filter {
            filter: Filter::Invert,
            amount: 0.35,
        },
    });
    graph.edges.push(Edge {
        from: source,
        to: id,
        input: 0,
    });
    call(&mut s, "graph.set", json!({"graph":graph}));
    for owner in [
        json!({"kind":"layer","id":1}),
        json!({"kind":"layer","id":2}),
        json!({"kind":"group","id":group}),
    ] {
        call(&mut s, "mask.create", json!({"owner":owner,"select":false}));
    }
    let ids: Vec<_> = s
        .doc
        .mask_state()
        .items
        .iter()
        .map(|mask| mask.id)
        .collect();
    s.doc
        .replace_mask_plane(
            ids[0],
            SamplePlane::Dense((0..15).map(|i| (i * 37 + 11) as u8).collect()),
        )
        .unwrap();
    s.doc
        .replace_mask_plane(
            ids[1],
            SamplePlane::Dense((0..15).map(|i| (i * 19 + 7) as u8).collect()),
        )
        .unwrap();
    s.doc
        .replace_mask_plane(ids[2], SamplePlane::Uniform(73))
        .unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[1],"enabled":false,"linked":false,"density":137,"feather_radius_px":1}),
    );
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":ids[2],"linked":false,"density":203,"feather_radius_px":2}),
    );
    call(
        &mut s,
        "layer.set",
        json!({"layer_id":1,"alpha_locked":true,"pixel_locked":true,"position_locked":true}),
    );
    call(
        &mut s,
        "mask.target",
        json!({"target":{"kind":"mask","id":ids[0]}}),
    );
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(5, 3, (0..15).map(|i| (i * 29 + 5) as u8).collect()).unwrap()),
            previous: Some(
                Mask::new(5, 3, (0..15).map(|i| (i * 13 + 23) as u8).collect()).unwrap(),
            ),
        })
        .unwrap();
    s.doc
        .set_import_info(Info::with_png_density(Density {
            xppu: 1000,
            yppu: 2000,
            unit: DensityUnit::Meter,
        }))
        .unwrap();
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    outcome(&mut s);
    s
}

#[derive(Clone, Debug, PartialEq)]
struct Content {
    width: usize,
    height: usize,
    active: usize,
    composition: Composition,
    rgba: Vec<Vec<u32>>,
    raw: Vec<(u64, Vec<u8>)>,
}
fn content(s: &Session) -> Content {
    Content {
        width: s.doc.width,
        height: s.doc.height,
        active: s.doc.active,
        composition: s.doc.composition(),
        rgba: pixels(s),
        raw: raw(s),
    }
}
struct Before {
    content: Content,
    public: Value,
    surface: Vec<u32>,
    history: usize,
    revision: u64,
    next_id: u64,
}
impl Before {
    fn capture(s: &Session) -> Self {
        Self {
            content: content(s),
            public: s.state(),
            surface: s.doc.composite.clone(),
            history: s.doc.history_bytes(),
            revision: s.revision,
            next_id: s.doc.mask_state().next_id,
        }
    }
    fn restored(&self, s: &Session) {
        assert_eq!(content(s), self.content);
        assert_eq!(s.state(), self.public);
        assert_eq!(s.doc.composite, self.surface);
        assert_eq!(s.doc.history_bytes(), self.history);
        assert_eq!(s.revision, self.revision);
        assert_eq!(s.doc.mask_state().next_id, self.next_id);
        assert!(!s.doc.history_group_active());
    }
}

fn reframe<T: Copy>(
    source: &[T],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
    dx: i32,
    dy: i32,
    fill: T,
) -> Vec<T> {
    (0..w * h)
        .map(|i| {
            let (x, y) = (
                (i % w) as i64 - i64::from(dx),
                (i / w) as i64 - i64::from(dy),
            );
            if x >= 0 && y >= 0 && x < sw as i64 && y < sh as i64 {
                source[y as usize * sw + x as usize]
            } else {
                fill
            }
        })
        .collect()
}
fn orthogonal<T: Copy>(source: &[T], sw: usize, sh: usize, kind: usize) -> Vec<T> {
    let (w, h) = if kind < 2 { (sh, sw) } else { (sw, sh) };
    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let (sx, sy) = match kind {
                0 => (y, sh - 1 - x),
                1 => (sw - 1 - y, x),
                2 => (sw - 1 - x, sh - 1 - y),
                3 => (sw - 1 - x, y),
                _ => (x, sh - 1 - y),
            };
            source[sy * sw + sx]
        })
        .collect()
}
fn transformed_selection(
    state: &SelectionState,
    w: usize,
    h: usize,
    transform: impl Fn(&[u8]) -> Vec<u8>,
) -> SelectionState {
    let convert = |mask: &Option<Mask>| {
        mask.as_ref()
            .map(|mask| Mask::new(w, h, transform(mask.coverage())).unwrap())
    };
    SelectionState {
        current: convert(&state.current),
        previous: convert(&state.previous),
    }
}
fn expected_frame(before: &Content, w: usize, h: usize, dx: i32, dy: i32) -> Content {
    let mut result = before.clone();
    result.width = w;
    result.height = h;
    result.rgba = before
        .rgba
        .iter()
        .map(|data| reframe(data, before.width, before.height, w, h, dx, dy, 0))
        .collect();
    result.raw = before
        .raw
        .iter()
        .map(|(id, data)| {
            (
                *id,
                reframe(data, before.width, before.height, w, h, dx, dy, 255),
            )
        })
        .collect();
    result.composition.selection =
        transformed_selection(&before.composition.selection, w, h, |data| {
            reframe(data, before.width, before.height, w, h, dx, dy, 0)
        });
    result.composition.last_transform = None;
    result
}
fn expected_rotation(before: &Content, kind: usize) -> Content {
    let mut result = before.clone();
    if kind < 2 {
        (result.width, result.height) = (before.height, before.width);
        result.composition.document_info = before.composition.document_info.rotated_quarter();
    }
    result.rgba = before
        .rgba
        .iter()
        .map(|data| orthogonal(data, before.width, before.height, kind))
        .collect();
    result.raw = before
        .raw
        .iter()
        .map(|(id, data)| (*id, orthogonal(data, before.width, before.height, kind)))
        .collect();
    result.composition.selection = transformed_selection(
        &before.composition.selection,
        result.width,
        result.height,
        |data| orthogonal(data, before.width, before.height, kind),
    );
    result.composition.last_transform = None;
    result
}
fn outcome(s: &mut Session) {
    check(s);
    let last = s.journal.as_ref().unwrap().entries.len() - 1;
    let recorded = s.replay_preview(last, false).unwrap();
    assert!(recorded.matches_document(&s.doc));
    let mut restored = Document::new(1, 1);
    recorded.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, expected) in raw(s) {
        assert_eq!(
            (0..expected.len())
                .map(|i| restored.mask_plane(id).unwrap().sample(i))
                .collect::<Vec<_>>(),
            expected
        );
    }
}
fn one_edit(s: &mut Session, action: &str, args: Value, expected: Content) {
    let before = content(s);
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    call(s, action, args);
    assert_eq!(content(s), expected, "{action}");
    assert_eq!(s.revision, revision + 1);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    outcome(s);
    call(s, "history.undo", json!({}));
    assert_eq!(content(s), before, "One Undo must restore {action}");
    outcome(s);
    call(s, "history.redo", json!({}));
    assert_eq!(content(s), expected, "One Redo must restore {action}");
    outcome(s);
}

#[test]
fn mask_canvas_all_anchors_crop_and_orthogonal_commands_preserve_every_owner_and_history() {
    for (anchor, dx, dy) in [
        ("top_left", 0, 0),
        ("top", 1, 0),
        ("top_right", 2, 0),
        ("left", 0, 1),
        ("center", 1, 1),
        ("right", 2, 1),
        ("bottom_left", 0, 3),
        ("bottom", 1, 3),
        ("bottom_right", 2, 3),
    ] {
        let mut s = fixture();
        let expected = expected_frame(&content(&s), 7, 6, dx, dy);
        one_edit(
            &mut s,
            "canvas.resize",
            json!({"width":7,"height":6,"anchor":anchor}),
            expected,
        );
    }
    for (action, args, w, h, dx, dy) in [
        (
            "canvas.resize",
            json!({"width":3,"height":2,"anchor":"center"}),
            3,
            2,
            -1,
            -1,
        ),
        (
            "canvas.crop",
            json!({"x":1,"y":1,"width":3,"height":2}),
            3,
            2,
            -1,
            -1,
        ),
    ] {
        let mut s = fixture();
        let expected = expected_frame(&content(&s), w, h, dx, dy);
        one_edit(&mut s, action, args, expected);
    }
    for (kind, action) in [
        "canvas.rotate_90_cw",
        "canvas.rotate_90_ccw",
        "canvas.rotate_180",
        "canvas.flip_horizontal",
        "canvas.flip_vertical",
    ]
    .into_iter()
    .enumerate()
    {
        let mut s = fixture();
        let before = content(&s);
        let expected = expected_rotation(&before, kind);
        if kind < 2 {
            let old = before.composition.document_info.png_density.unwrap();
            let next = expected.composition.document_info.png_density.unwrap();
            assert_eq!((next.xppu, next.yppu), (old.yppu, old.xppu));
        }
        one_edit(&mut s, action, json!({}), expected);
    }
}

// Direct floating two-dimensional integration is independent of native rational
// axis tables and streaming row caches. The fixtures are small; epsilon only
// corrects floating representation at an exact positive half-byte tie.
fn weights(source: usize, target: usize, index: usize, method: &str) -> Vec<(usize, f64)> {
    if method == "nearest" {
        return vec![(
            (((index as f64 + 0.5) * source as f64 / target as f64).floor() as usize)
                .min(source - 1),
            1.,
        )];
    }
    if method == "area" && source > target {
        let (left, right) = (
            index as f64 * source as f64 / target as f64,
            (index + 1) as f64 * source as f64 / target as f64,
        );
        return (0..source)
            .filter_map(|pixel| {
                let overlap = right.min((pixel + 1) as f64) - left.max(pixel as f64);
                (overlap > 0.).then_some((pixel, overlap / (right - left)))
            })
            .collect();
    }
    let p = (index as f64 + 0.5) * source as f64 / target as f64 - 0.5;
    let a = p.floor() as isize;
    let f = p - p.floor();
    vec![
        (a.clamp(0, source as isize - 1) as usize, 1. - f),
        ((a + 1).clamp(0, source as isize - 1) as usize, f),
    ]
}
fn rounded(value: f64) -> u32 {
    (value + 0.5 + 1e-10).floor() as u32
}
fn resize_a8(source: &[u8], sw: usize, sh: usize, w: usize, h: usize, method: &str) -> Vec<u8> {
    (0..w * h)
        .map(|i| {
            let mut sum = 0.;
            for (y, wy) in weights(sh, h, i / w, method) {
                for (x, wx) in weights(sw, w, i % w, method) {
                    sum += source[y * sw + x] as f64 * wx * wy;
                }
            }
            rounded(sum) as u8
        })
        .collect()
}
fn resize_rgba(source: &[u32], sw: usize, sh: usize, w: usize, h: usize, method: &str) -> Vec<u32> {
    (0..w * h)
        .map(|i| {
            if method == "nearest" {
                return source
                    [weights(sh, h, i / w, method)[0].0 * sw + weights(sw, w, i % w, method)[0].0];
            }
            let mut alpha = 0.;
            let mut rgb = [0.; 3];
            for (y, wy) in weights(sh, h, i / w, method) {
                for (x, wx) in weights(sw, w, i % w, method) {
                    let p = source[y * sw + x];
                    let a = (p >> 24) as f64 * wx * wy;
                    alpha += a;
                    for (c, shift) in [0, 8, 16].into_iter().enumerate() {
                        rgb[c] += ((p >> shift) & 255) as f64 * a;
                    }
                }
            }
            let a = rounded(alpha);
            if a == 0 {
                return 0;
            }
            a << 24
                | rounded(rgb[0] / alpha)
                | rounded(rgb[1] / alpha) << 8
                | rounded(rgb[2] / alpha) << 16
        })
        .collect()
}
#[test]
fn mask_canvas_image_size_samples_raw_and_both_selections_with_each_method() {
    for method in ["nearest", "bilinear", "area"] {
        let mut s = fixture();
        let before = content(&s);
        let mut expected = before.clone();
        expected.width = 3;
        expected.height = 2;
        expected.rgba = before
            .rgba
            .iter()
            .map(|data| resize_rgba(data, 5, 3, 3, 2, method))
            .collect();
        expected.raw = before
            .raw
            .iter()
            .map(|(id, data)| (*id, resize_a8(data, 5, 3, 3, 2, method)))
            .collect();
        expected.composition.selection =
            transformed_selection(&before.composition.selection, 3, 2, |data| {
                resize_a8(data, 5, 3, 3, 2, method)
            });
        one_edit(
            &mut s,
            "canvas.image_size",
            json!({"width":3,"height":2,"lock_aspect":false,"method":method}),
            expected,
        );
    }
}

#[test]
fn mask_canvas_plans_report_all_raw_planes_and_keep_legacy_selection_costs_read_only() {
    let mut s = fixture();
    let before = Before::capture(&s);
    let events = s.journal.as_ref().unwrap().entries.len();
    let plan = call(
        &mut s,
        "canvas.geometry.plan",
        json!({"operation":"resize","width":7,"height":5,"anchor":"bottom_right"}),
    );
    for (field, value) in [
        ("source_width", 5),
        ("source_height", 3),
        ("width", 7),
        ("height", 5),
        ("offset_x", 2),
        ("offset_y", 2),
        ("layer_count", 2),
        ("pixel_selection_count", 2),
        ("raster_mask_count", 3),
        ("estimated_layer_bytes", 280),
        ("estimated_selection_bytes", 70),
        ("estimated_raster_mask_bytes", 105),
        ("estimated_effective_mask_bytes", 70),
        ("estimated_composite_bytes", 140),
        ("estimated_staged_bytes", 455),
    ] {
        assert_eq!(plan[field], value, "{field}");
    }
    assert_eq!(plan["a8_sampler_available"], true);
    assert_eq!(plan["requires_a8_sampler"], true);
    assert_eq!(plan["identity"], false);
    assert_eq!(plan["raster_mask_fill"], 255);
    assert_eq!(plan["raster_mask_feather_policy"], "fixed_canvas_pixels");
    crate::plugins::validate(
        s.registry.actions["canvas.geometry.plan"]
            .output_schema
            .as_ref()
            .unwrap(),
        &plan,
    )
    .unwrap();
    let image = call(
        &mut s,
        "canvas.image_size.plan",
        json!({"width":7,"height":5,"lock_aspect":false}),
    );
    assert_eq!(image["mask_count"], 2);
    assert_eq!(image["estimated_mask_bytes"], 70);
    assert_eq!(image["raster_mask_count"], 3);
    assert_eq!(image["estimated_raster_mask_bytes"], 105);
    assert_eq!(image["estimated_effective_mask_bytes"], 70);
    assert_eq!(image["estimated_staged_bytes"], 455);
    let identity = call(
        &mut s,
        "canvas.geometry.plan",
        json!({"operation":"resize","width":5,"height":3}),
    );
    assert_eq!(identity["identity"], true);
    assert_eq!(identity["requires_a8_sampler"], false);
    assert_eq!(identity["estimated_staged_bytes"], 0);
    before.restored(&s);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
}

#[test]
fn mask_canvas_trim_uses_masked_visible_bounds_but_reframes_hidden_and_disabled_raw_planes() {
    let mut s = fixture();
    s.doc.layers[0].pixels.fill(0x00112233);
    for x in 1..4 {
        s.doc.layers[0].pixels[5 + x] = 0xff336699;
    }
    s.doc.replay_tiles.insert((0, 0));
    s.doc.invalidate();
    s.replay_boundary().unwrap();
    let before = Before::capture(&s);
    let bounds = call(&mut s, "canvas.trim_bounds", json!({}));
    assert_eq!(
        (
            bounds["x"].as_u64(),
            bounds["y"].as_u64(),
            bounds["width"].as_u64(),
            bounds["height"].as_u64()
        ),
        (Some(1), Some(1), Some(3), Some(1))
    );
    before.restored(&s);
    let expected = expected_frame(&content(&s), 3, 1, -1, -1);
    one_edit(&mut s, "canvas.trim", json!({}), expected);
}

#[test]
fn mask_canvas_mixed_geometry_roundtrips_project_and_recorded_raw_outcomes() {
    let mut s = fixture();
    for (action, args) in [
        ("canvas.rotate_90_cw", json!({})),
        (
            "canvas.resize",
            json!({"width":6,"height":7,"anchor":"bottom_right"}),
        ),
        (
            "canvas.image_size",
            json!({"width":4,"height":3,"lock_aspect":false,"method":"area"}),
        ),
        ("canvas.flip_horizontal", json!({})),
        ("canvas.crop", json!({"x":1,"y":0,"width":3,"height":2})),
    ] {
        call(&mut s, action, args);
        outcome(&mut s);
    }
    let expected = content(&s);
    let path = output("geometry.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    outcome(&mut s);
    assert_eq!(&std::fs::read(&path).unwrap()[..8], b"MICOPI04");
    let mut opened = session();
    call(&mut opened, "document.open", json!({"path":path}));
    assert_eq!(content(&opened), expected);
    outcome(&mut opened);
}

fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 1. + (time % 3) as f32,
        y: 1. + (time % 2) as f32 * 0.5,
        pressure: 0.3 + (time % 5) as f32 * 0.1,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(kind),
        view_x: None,
        view_y: None,
    }
}
#[test]
fn mask_canvas_live_brush_and_hand_rejections_keep_contacts_raw_storage_and_every_sample() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let mut s = fixture();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1,"color":"#000000","stabilizer":0.35,"pressure_size":true}),
        );
        s.pointer(pointer(kind, PointerPhase::Down, 0)).unwrap();
        let id = s.doc.mask_state().items[0].id;
        for (i, (action, args)) in [
            ("canvas.resize", json!({"width":7,"height":5})),
            ("canvas.crop", json!({"x":1,"y":0,"width":4,"height":3})),
            ("canvas.rotate_90_cw", json!({})),
            ("canvas.rotate_90_ccw", json!({})),
            ("canvas.rotate_180", json!({})),
            ("canvas.flip_horizontal", json!({})),
            ("canvas.flip_vertical", json!({})),
            ("canvas.image_size", json!({"width":7})),
            ("canvas.trim", json!({})),
        ]
        .into_iter()
        .enumerate()
        {
            let before = Before::capture(&s);
            let last = s.stroke.last_sample().unwrap().time;
            let checkpoint = s.pointer_checkpoint.is_some();
            let storage = match s.doc.mask_plane(id).unwrap() {
                PlaneView::Dense(data) => Some(data.as_ptr()),
                _ => None,
            };
            let plan = call(
                &mut s,
                "canvas.geometry.plan",
                json!({"operation":"resize","width":7,"height":5}),
            );
            assert_eq!(plan["drawing"], s.is_drawing());
            assert_eq!(plan["contact_active"], true);
            assert_eq!(
                call(&mut s, "canvas.image_size.plan", json!({"width":7}))["contact_active"],
                true
            );
            let error = s.apply(Action::new(action, args)).unwrap_err();
            assert_eq!(error.code, "stroke_active", "{action}");
            before.restored(&s);
            assert!(s.stroke.active());
            assert_eq!(s.stroke.last_sample().unwrap().time, last);
            assert_eq!(s.pointer_checkpoint.is_some(), checkpoint);
            if let Some(storage) = storage {
                assert!(
                    matches!(s.doc.mask_plane(id).unwrap(),PlaneView::Dense(data)if data.as_ptr()==storage)
                );
            }
            s.pointer(pointer(kind, PointerPhase::Move, i as u32 + 1))
                .unwrap();
        }
        s.pointer(pointer(kind, PointerPhase::Up, 10)).unwrap();
        assert!(!s.stroke.active());
        outcome(&mut s);
        let events = call(&mut s, "replay.events", json!({"limit":256}));
        let count: usize = events["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|event| event["source"]["samples"].as_array())
            .map(Vec::len)
            .sum();
        assert_eq!(count, 11);
    }
}

fn old_painting() -> Option<Arc<crate::plugins::native::NativePlugin>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let baseline = std::env::var_os("MICOPI_A8_BASELINE_DLL")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/a8-sampler-baseline/micopi_painting_tools.dll"));
    if !baseline.is_file() {
        eprintln!(
            "Preserved sampler baseline absent; old-DLL geometry check skipped: {}",
            baseline.display()
        );
        return None;
    }
    let directory = output("old-plugins");
    std::fs::create_dir_all(&directory).unwrap();
    for entry in std::fs::read_dir(root.join("plugins")).unwrap() {
        let folder = entry.unwrap().path();
        let manifest = folder.join("manifest.json");
        if !manifest.is_file() {
            continue;
        }
        let destination = directory.join(folder.file_name().unwrap());
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::copy(&manifest, destination.join("manifest.json")).unwrap();
        let metadata: Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        if let Some(library) = metadata["library"].as_str() {
            std::fs::copy(
                if metadata["id"] == "painting" {
                    baseline.clone()
                } else {
                    folder.join(library)
                },
                destination.join(library),
            )
            .unwrap();
        }
    }
    let mut registry = crate::plugins::Registry::load(&directory).unwrap();
    let plugin = registry
        .native_plugins
        .remove("painting")
        .expect("The old DLL must load its existing A8 tools");
    assert!(!plugin.supports_a8_sampler());
    Some(plugin)
}
#[test]
fn mask_canvas_old_a8_tool_dll_rejects_sampling_but_retains_identity_noops() {
    let Some(old) = old_painting() else {
        return;
    };
    let mut s = fixture();
    s.registry.native_plugins.insert("painting".into(), old);
    let before = Before::capture(&s);
    let plan = call(
        &mut s,
        "canvas.geometry.plan",
        json!({"operation":"resize","width":7,"height":5}),
    );
    assert_eq!(plan["a8_sampler_available"], false);
    assert_eq!(plan["requires_a8_sampler"], true);
    for (action, args) in [
        ("canvas.resize", json!({"width":7,"height":5})),
        ("canvas.crop", json!({"x":1,"y":0,"width":4,"height":3})),
        ("canvas.rotate_90_cw", json!({})),
        ("canvas.image_size", json!({"width":7})),
    ] {
        assert!(s.apply(Action::new(action, args)).is_err());
        before.restored(&s);
    }
    let data = content(&s);
    let history = s.doc.history_bytes();
    for (action, args) in [
        ("canvas.resize", json!({"width":5,"height":3})),
        ("canvas.crop", json!({"x":0,"y":0,"width":5,"height":3})),
        ("canvas.image_size", json!({"width":5})),
    ] {
        call(&mut s, action, args);
        assert_eq!(content(&s), data);
        assert_eq!(s.doc.history_bytes(), history);
    }
    outcome(&mut s);
}

static SAMPLER_CALLS: AtomicUsize = AtomicUsize::new(0);
static SAMPLER_ORIGINAL: Mutex<Option<abi::A8ReframeFn>> = Mutex::new(None);
unsafe extern "C" fn fail_second_mask(
    request: *const abi::A8Reframe,
    source: *const abi::CompositeMask,
    width: u32,
    height: u32,
    output: *mut abi::A8Pixels,
) -> i32 {
    if SAMPLER_CALLS.fetch_add(1, Ordering::SeqCst) == 1 {
        (*output).data.write(201);
        return abi::ERROR;
    }
    let original = SAMPLER_ORIGINAL.lock().unwrap().unwrap();
    original(request, source, width, height, output)
}
#[test]
fn mask_canvas_second_native_plane_failure_discards_staged_work_and_preserves_redo() {
    let mut s = fixture();
    let id = s.doc.mask_state().items[0].id;
    call(&mut s, "mask.fill", json!({"mask_id":id,"value":91}));
    call(&mut s, "history.undo", json!({}));
    let plugin = Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .expect("A fresh fixture has no cached painting instance");
    let original = plugin.replace_a8_sampler_for_test(None).unwrap();
    *SAMPLER_ORIGINAL.lock().unwrap() = original.reframe;
    SAMPLER_CALLS.store(0, Ordering::SeqCst);
    let mut fault = original;
    fault.reframe = Some(fail_second_mask);
    plugin.replace_a8_sampler_for_test(Some(fault));
    let before = Before::capture(&s);
    let error = s
        .apply(Action::new("canvas.resize", json!({"width":7,"height":5})))
        .unwrap_err();
    assert!(error.message.contains("Native A8 reframe"));
    assert_eq!(SAMPLER_CALLS.load(Ordering::SeqCst), 2);
    before.restored(&s);
    assert!(s.replay_error.is_none());
    Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
        .unwrap()
        .replace_a8_sampler_for_test(Some(original));
    *SAMPLER_ORIGINAL.lock().unwrap() = None;
    outcome(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert!(raw(&s)[0].1.iter().all(|value| *value == 91));
    outcome(&mut s);
}

struct FailRender {
    native: Arc<dyn Renderer>,
    armed: Cell<bool>,
    size: Cell<[u32; 2]>,
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
            unsafe { o.data.write(0xdeadc0de) };
            self.size.set([w, h]);
            self.failures.set(self.failures.get() + 1);
            Err("Injected post-publication geometry renderer failure".into())
        } else {
            self.native.render_masked(n, l, m, w, h, o)
        }
    }
}
#[test]
fn mask_canvas_post_publication_render_and_partial_journal_failures_restore_complete_state() {
    let mut s = fixture();
    let renderer = Arc::new(FailRender {
        native: s.doc.renderer.as_ref().unwrap().clone(),
        armed: Cell::new(false),
        size: Cell::new([0, 0]),
        failures: Cell::new(0),
    });
    s.doc.renderer = Some(renderer.clone());
    let before = Before::capture(&s);
    renderer.armed.set(true);
    let error = s
        .apply(Action::new("canvas.resize", json!({"width":7,"height":5})))
        .unwrap_err();
    assert_eq!(error.code, "render_error");
    assert!(error.message.contains("Injected post-publication"));
    assert_eq!(renderer.size.get(), [7, 5]);
    assert_eq!(renderer.failures.get(), 1);
    before.restored(&s);
    assert!(s.replay_error.is_none());
    outcome(&mut s);
    let before = Before::capture(&s);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    let events = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.image_size",
            json!({"width":3,"height":2,"lock_aspect":false,"method":"area"})
        ))
        .is_err());
    before.restored(&s);
    assert!(s.replay_error.is_some());
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

#[test]
fn mask_canvas_scalar_plans_reject_uniform_plane_budget_without_large_allocations() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":1,"height":1}));
    let mut member = json!({"kind":"layer","id":1});
    let mut groups = vec![];
    for _ in 0..4 {
        let id = call(&mut s, "group.create", json!({"members":[member]}))["group_id"]
            .as_u64()
            .unwrap();
        groups.push(id);
        member = json!({"kind":"group","id":id});
    }
    s.doc.create_mask(MaskOwner::Layer { id: 1 }, 255).unwrap();
    for id in groups {
        s.doc.create_mask(MaskOwner::Group { id }, 255).unwrap();
    }
    s.replay_boundary().unwrap();
    let before = Before::capture(&s);
    for (action, args) in [
        (
            "canvas.geometry.plan",
            json!({"operation":"resize","width":4096,"height":4096}),
        ),
        (
            "canvas.image_size.plan",
            json!({"width":4096,"height":4096,"lock_aspect":false}),
        ),
    ] {
        assert!(s.apply(Action::new(action, args)).is_err());
        before.restored(&s);
    }
    assert!(s
        .doc
        .mask_state()
        .items
        .iter()
        .all(|m| matches!(s.doc.mask_plane(m.id).unwrap(), PlaneView::Uniform(255))));
}

#[test]
fn mask_canvas_square_raw_only_turn_recomputes_fixed_pixel_feather_and_preserves_other_guards() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":2,"height":2}));
    call(&mut s, "layer.fill_color", json!({"color":"#FFFFFF"}));
    let id = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    s.doc
        .replace_mask_plane(id, SamplePlane::Dense(vec![165, 4, 135, 79]))
        .unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":id,"feather_radius_px":1}),
    );
    s.replay_boundary().unwrap();
    let before = content(&s);
    let expected = expected_rotation(&before, 0);
    one_edit(&mut s, "canvas.rotate_90_cw", json!({}), expected);
    let effective = s.doc.effective_mask_plane(id).unwrap();
    assert!(matches!(effective,PlaneView::Dense(data)if data==[115,113,84,71]));
    let before = Before::capture(&s);
    for (action, args) in [(
        "canvas.transform",
        json!({"target":"selection","translate_x":1}),
    )] {
        let error = s.apply(Action::new(action, args)).unwrap_err();
        assert_eq!(error.code, "mask_editing_pending", "{action}: {error}");
        before.restored(&s);
    }
}

#[test]
fn mask_canvas_integer_json_floats_and_plan_result_bindings_share_the_staged_frame() {
    let mut s = fixture();
    let before = content(&s);
    let plan = call(
        &mut s,
        "canvas.geometry.plan",
        json!({
            "operation":"resize","width":7.0,"height":5.0
        }),
    );
    assert_eq!(plan["width"], 7);
    assert_eq!(plan["height"], 5);
    assert_eq!(content(&s), before);
    one_edit(
        &mut s,
        "canvas.resize",
        json!({"width":7.0,"height":5.0}),
        expected_frame(&before, 7, 5, 1, 1),
    );

    let mut s = fixture();
    let before = content(&s);
    one_edit(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"plan","action":"canvas.geometry.plan","args":{"operation":"resize","width":7,"height":5}},
            {"id":"resize","action":"canvas.resize","args":{
                "width":{"$ref":"plan#/result/width"},"height":{"$ref":"plan#/result/height"}
            }}
        ]}),
        expected_frame(&before, 7, 5, 1, 1),
    );
    let before = Before::capture(&s);
    assert!(s.apply(Action::new("pipeline.run", json!({"steps":[
        {"id":"must-not-run","action":"mask.fill","args":{"mask_id":1,"value":0}},
        {"id":"bad-literal","action":"canvas.geometry.plan","args":{"operation":"rotate_90_cw","width":7}}
    ]}))).is_err());
    before.restored(&s);
}

#[test]
fn mask_canvas_geometry_workflow_editing_and_dry_run_are_inert_before_bound_execution() {
    let mut s = fixture();
    let before = content(&s);
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.canvas.geometry.plan","position":[0,0],"parameters":{"operation":"resize","width":7,"height":5}},
        {"id":2,"type":"action.canvas.resize","position":[360,0],"parameters":{"anchor":"center"}}
    ],"edges":[
        {"from":1,"output":"width","to":2,"input":"width"},
        {"from":1,"output":"height","to":2,"input":"height"}
    ]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(pixels(&s), before.rgba);
    assert_eq!(raw(&s), before.raw);
    assert_eq!((s.doc.width, s.doc.height), (5, 3));
    let idle = Before::capture(&s);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    idle.restored(&s);
    let before = content(&s);
    one_edit(
        &mut s,
        "workflow.run",
        json!({"atomic":true}),
        expected_frame(&before, 7, 5, 1, 1),
    );
}
