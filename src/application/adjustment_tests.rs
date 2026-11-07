//! Real non-raster adjustments share composition, public commands and outcomes.
use super::editing_tests::{call, check};
use super::*;
use crate::{
    engine::{
        document::Composition,
        graph::Renderer,
        raster_masks::{MaskOwner, PlaneView},
        replay::Journal,
        selection::{Mask, SelectionState},
        Document,
    },
    plugins::Registry,
};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
use std::{cell::Cell, collections::BTreeMap, path::PathBuf, sync::Arc};

const W: usize = 5;
const H: usize = 3;

fn fixture(count: usize) -> Session {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
    let mut s = Session::new(Registry::load(&root).unwrap()).unwrap();
    call(&mut s, "canvas.new", json!({"width":W,"height":H}));
    for _ in 1..count {
        call(&mut s, "layer.add", json!({}));
    }
    for (layer, raster) in s.doc.layers.iter_mut().enumerate() {
        for (i, pixel) in raster.pixels.iter_mut().enumerate() {
            *pixel = if layer == 0 {
                ([0, 1, 73, 128, 255][i % W] as u32) << 24 | 0x00402010
            } else if i % 2 == 0 {
                0xff1122ee
            } else {
                0x001122ee
            };
        }
        s.doc.replay_tiles.insert((layer, 0));
    }
    s.doc.invalidate();
    s.doc.compose();
    // Direct fixture writes must become a boundary before the next shared edit.
    s.replay_boundary().unwrap();
    s
}
fn pixels(s: &Session) -> BTreeMap<u64, Vec<u32>> {
    s.doc
        .layers
        .iter()
        .enumerate()
        .map(|(i, layer)| (s.doc.layer_id(i), layer.pixels.clone()))
        .collect()
}
fn raw_masks(s: &Session) -> BTreeMap<u64, Vec<u8>> {
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
fn pointers(s: &Session) -> Vec<usize> {
    s.doc
        .layers
        .iter()
        .map(|l| l.pixels.as_ptr() as usize)
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
        masks: raw_masks(s),
        surface: s.doc.composite.clone(),
        active: s.doc.active,
    }
}
fn args() -> Value {
    json!({"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1}})
}
fn create(s: &mut Session, extra: Value) -> Value {
    let mut value = args();
    for (key, value_extra) in extra.as_object().unwrap() {
        value[key] = value_extra.clone();
    }
    call(s, "adjustment.create", value)
}
fn outcome(s: &mut Session) {
    check(s);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut journal = Journal::import(&bytes).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let state = journal.preview(journal.entries.len() - 1).unwrap();
    let mut restored = Document::new(1, 1);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert_eq!(restored.composition(), s.doc.composition());
    assert_eq!(restored.composite, s.doc.composite);
    for (id, samples) in raw_masks(s) {
        let plane = restored.mask_plane(id).unwrap();
        assert_eq!(
            (0..W * H).map(|i| plane.sample(i)).collect::<Vec<_>>(),
            samples
        );
    }
}
fn invert(p: u32) -> u32 {
    if p >> 24 == 0 {
        p
    } else {
        (p & 0xff000000) | ((!p) & 0x00ffffff)
    }
}
// Independent encoded-byte premultiplied interpolation; alpha is not source-over.
fn mix(before: u32, after: u32, opacity: u32, mask: u32) -> u32 {
    let k = (opacity * mask + 127) / 255;
    if k == 0 {
        return before;
    }
    if k == 255 {
        return after;
    }
    let weights = [
        u64::from(before >> 24) * u64::from(255 - k),
        u64::from(after >> 24) * u64::from(k),
    ];
    let total = weights[0] + weights[1];
    let alpha = (total + 127) / 255;
    if alpha == 0 {
        return 0;
    }
    [0, 8, 16]
        .into_iter()
        .fold((alpha as u32) << 24, |p, shift| {
            let sum = u64::from((before >> shift) & 255) * weights[0]
                + u64::from((after >> shift) & 255) * weights[1];
            p | (((sum + total / 2) / total) as u32) << shift
        })
}
fn selection(s: &mut Session) -> Vec<u8> {
    let samples: Vec<_> = (0..W * H).map(|i| [0, 64, 128, 255, 0][i % W]).collect();
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(W, H, samples.clone()).unwrap()),
            previous: Some(
                Mask::new(
                    W,
                    H,
                    (0..W * H)
                        .map(|i| if i % 2 == 0 { 17 } else { 239 })
                        .collect(),
                )
                .unwrap(),
            ),
        })
        .unwrap();
    s.replay_boundary().unwrap();
    samples
}
fn effective_mask(samples: &[u8], density: u32) -> Vec<u8> {
    // Radius 1 is a clamped horizontal/vertical box pair; density follows blur.
    let mut horizontal = vec![0u8; W * H];
    let mut blurred = vec![0u8; W * H];
    for y in 0..H {
        for x in 0..W {
            horizontal[y * W + x] = ((u32::from(samples[y * W + x.saturating_sub(1)])
                + u32::from(samples[y * W + x])
                + u32::from(samples[y * W + (x + 1).min(W - 1)])
                + 1)
                / 3) as u8;
        }
    }
    for y in 0..H {
        for x in 0..W {
            blurred[y * W + x] = ((u32::from(horizontal[y.saturating_sub(1) * W + x])
                + u32::from(horizontal[y * W + x])
                + u32::from(horizontal[(y + 1).min(H - 1) * W + x])
                + 1)
                / 3) as u8;
        }
    }
    blurred
        .into_iter()
        .map(|v| (255 - ((255 - u32::from(v)) * density + 127) / 255) as u8)
        .collect()
}

#[test]
fn adjustment_is_an_alpha_preserving_stack_item_with_one_undo_and_no_raster_copy() {
    let mut s = fixture(1);
    let original = pixels(&s);
    let source = s.doc.composite.clone();
    let addresses = pointers(&s);
    let active = s.doc.active;
    let selected = s.doc.selected_layer_ids();
    let revision = s.revision;
    let events = s.journal.as_ref().unwrap().entries.len();
    let result = create(&mut s, json!({"opacity":128./255.,"index":1}));
    let id = result["adjustment_id"].as_u64().unwrap();
    assert_eq!(result["revision"], revision + 1);
    assert_eq!(s.revision, revision + 1);
    assert_eq!(result["mask_ids"], json!([]));
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events + 1);
    assert_eq!(s.doc.layers.len(), 1);
    assert_eq!(pixels(&s), original);
    assert_eq!(pointers(&s), addresses);
    assert_eq!(s.doc.active, active);
    assert_eq!(s.doc.selected_layer_ids(), selected);
    assert_eq!(
        s.doc.composite,
        source
            .iter()
            .map(|p| mix(*p, invert(*p), 128, 255))
            .collect::<Vec<_>>()
    );
    assert_eq!(s.doc.composite[3], 0x80808080);
    assert!(s
        .doc
        .composite
        .iter()
        .zip(&source)
        .all(|(a, b)| a >> 24 == b >> 24));
    call(&mut s, "history.undo", json!({}));
    assert!(s.doc.adjustments().is_empty());
    assert_eq!(s.doc.composite, source);
    assert_eq!(pointers(&s), addresses);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(s.doc.adjustments()[0].id, id);
    assert_eq!(s.doc.composite[3], 0x80808080);
    outcome(&mut s);
}

#[test]
fn adjustment_placement_filters_only_below_and_pass_through_fades_backdrop_once() {
    let mut s = fixture(2);
    let source = pixels(&s);
    let result = create(&mut s, json!({"index":1}));
    let id = result["adjustment_id"].as_u64().unwrap();
    let expected: Vec<_> = (0..W * H)
        .map(|i| {
            if i % 2 == 0 {
                0xff1122ee
            } else {
                let p = source[&1][i];
                if p >> 24 == 0 {
                    0
                } else {
                    invert(p)
                }
            }
        })
        .collect();
    assert_eq!(s.doc.composite, expected);
    assert_eq!(pixels(&s), source);
    call(
        &mut s,
        "adjustment.move",
        json!({"adjustment_id":id,"parent_id":0,"index":2}),
    );
    assert_eq!(s.doc.composite[0], invert(0xff1122ee));
    assert_ne!(s.doc.composite, expected);

    let mut s = fixture(1);
    let original = s.doc.composite.clone();
    let group = call(&mut s, "group.create", json!({"members":[]}))["group_id"]
        .as_u64()
        .unwrap();
    create(&mut s, json!({"parent_id":group,"index":0}));
    assert_eq!(
        s.doc.composite, original,
        "isolated adjustment cannot see earlier roots"
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"compositing":"pass_through","opacity":137./255.}),
    );
    assert_eq!(
        s.doc.composite,
        original
            .iter()
            .map(|p| mix(*p, invert(*p), 137, 255))
            .collect::<Vec<_>>()
    );
    call(
        &mut s,
        "group.set",
        json!({"group_id":group,"visible":false}),
    );
    assert_eq!(s.doc.composite, original);
    outcome(&mut s);
}

#[test]
fn adjustment_selection_mask_is_owned_snapshot_and_preserves_live_selection_and_target() {
    let mut s = fixture(1);
    let existing = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1}}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    let samples = selection(&mut s);
    let selection = s.doc.selection().clone();
    let target = s.doc.mask_state().target;
    let source = s.doc.composite.clone();
    let original = pixels(&s);
    let addresses = pointers(&s);
    let result = create(
        &mut s,
        json!({"opacity":128./255.,"mask_source":"selection"}),
    );
    let id = result["adjustment_id"].as_u64().unwrap();
    let mask = result["mask_ids"][0].as_u64().unwrap();
    assert_ne!(mask, existing);
    assert_eq!(
        s.doc
            .mask_state()
            .items
            .iter()
            .find(|m| m.id == mask)
            .unwrap()
            .owner,
        MaskOwner::Adjustment { id }
    );
    assert_eq!(raw_masks(&s)[&mask], samples);
    let mask_pointer = match s.doc.mask_plane(mask).unwrap() {
        PlaneView::Dense(p) => p.as_ptr(),
        _ => panic!("selection snapshot must own samples"),
    };
    assert_ne!(
        mask_pointer,
        s.doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .as_ptr()
    );
    assert_eq!(s.doc.selection(), &selection);
    assert_eq!(s.doc.mask_state().target, target);
    assert_eq!(
        s.doc.composite,
        source
            .iter()
            .zip(&samples)
            .map(|(p, m)| mix(*p, invert(*p), 128, u32::from(*m)))
            .collect::<Vec<_>>()
    );
    assert_eq!(pixels(&s), original);
    assert_eq!(pointers(&s), addresses);
    let expected = s.doc.composite.clone();
    call(&mut s, "selection.deselect", json!({}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    assert_eq!(raw_masks(&s)[&mask], samples);
    assert_eq!(s.doc.composite, expected);
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":mask,"density":173,"feather_radius_px":1}),
    );
    let effective = effective_mask(&samples, 173);
    assert_eq!(
        s.doc.composite,
        source
            .iter()
            .zip(effective)
            .map(|(p, m)| mix(*p, invert(*p), 128, u32::from(m)))
            .collect::<Vec<_>>()
    );
    call(&mut s, "mask.set", json!({"mask_id":mask,"enabled":false}));
    assert_eq!(
        s.doc.composite,
        source
            .iter()
            .map(|p| mix(*p, invert(*p), 128, 255))
            .collect::<Vec<_>>()
    );
    assert_eq!(raw_masks(&s)[&mask], samples);
    call(&mut s, "adjustment.delete", json!({"adjustment_id":id}));
    assert!(s.doc.mask_plane(mask).is_err());
    assert_eq!(s.doc.mask_state().target, target);
    assert!(s.doc.mask_plane(existing).is_ok());
    call(&mut s, "history.undo", json!({}));
    assert_eq!(raw_masks(&s)[&mask], samples);
    outcome(&mut s);
}

#[test]
fn adjustment_queries_catalog_and_typed_workflow_bindings_are_inert_until_run() {
    let mut s = fixture(1);
    let id = create(&mut s, json!({"name":"Ink tone"}))["adjustment_id"]
        .as_u64()
        .unwrap();
    let before = content(&s);
    let state = s.state();
    let history = s.doc.history_bytes();
    let count = s.journal.as_ref().unwrap().entries.len();
    let catalog = call(&mut s, "adjustment.catalog", json!({}));
    assert!(catalog["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["node_type"] == "compositing.filter" && i["owner"] == "compositing"));
    let query = call(&mut s, "adjustment.query", json!({}));
    assert_eq!(query["limits"]["count"], 16);
    assert_eq!(query["items"][0]["id"], id);
    assert_eq!(query["items"][0]["provider_available"], true);
    let item = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    assert_eq!(item["parameters"], json!({"filter":"invert","amount":1.0}));
    assert_eq!(item["revision"], s.revision);
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    let nodes = s.registry.node_catalog();
    for action in [
        "adjustment.create",
        "adjustment.set",
        "adjustment.move",
        "adjustment.delete",
    ] {
        let node = &nodes[&format!("action.{action}")];
        assert!(!node.read_only);
        assert!(node.outputs.iter().any(|p| p.id
            == if action == "adjustment.delete" {
                "deleted_adjustment_ids"
            } else {
                "adjustment_id"
            }));
    }
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.adjustment.create","position":[0,0],"parameters":args()},
        {"id":2,"type":"action.adjustment.set","position":[350,0],"parameters":{"name":"Bound ID"}}
    ],"edges":[{"from":1,"output":"adjustment_id","to":2,"input":"adjustment_id"}]});
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"graph":graph,"atomic":true,"dry_run":true})
        )["ok"],
        true
    );
    assert_eq!(content(&s), before);
    assert_eq!(s.revision, state["revision"].as_u64().unwrap());
    assert_eq!(
        call(&mut s, "workflow.run", json!({"graph":graph,"atomic":true}))["ok"],
        true
    );
    assert_eq!(s.doc.adjustments().len(), 2);
    assert_eq!(s.doc.adjustments()[1].name, "Bound ID");
    outcome(&mut s);
}

#[test]
fn adjustment_atomic_and_journal_failures_restore_ids_raw_masks_revision_and_history() {
    let mut s = fixture(1);
    selection(&mut s);
    let before = content(&s);
    let public = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"new","action":"adjustment.create","args":{"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1},"mask_source":"selection"}},
            {"id":"set","action":"adjustment.set","args":{"adjustment_id":{"$ref":"new#/result/adjustment_id"},"name":"Transient"}},
            {"id":"fail","action":"adjustment.delete","args":{"adjustment_id":99999}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    assert_eq!(result["completed"], json!(["new", "set"]));
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    let head = s.journal.as_ref().unwrap().checkpoint().head_hash();
    let count = s.journal.as_ref().unwrap().entries.len();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    let mut action = args();
    action["mask_source"] = json!("selection");
    assert!(s.apply(Action::new("adjustment.create", action)).is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.state(), public);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().checkpoint().head_hash(), head);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert!(s.replay_error.is_some());
    s.replay_error = None; // Fixture-only recovery; production remains faulted after I/O failure.
    let result = create(&mut s, json!({"mask_source":"selection"}));
    assert_eq!(result["adjustment_id"], 1);
    assert_eq!(result["mask_ids"], json!([1]));
    call(&mut s, "history.undo", json!({}));
    let fresh = create(&mut s, json!({"mask_source":"selection"}));
    assert_eq!(fresh["adjustment_id"], 2);
    assert_eq!(fresh["mask_ids"], json!([2]));
    outcome(&mut s);
}

fn alias(s: &mut Session) {
    let (_, mut definition) = s.registry.nodes["compositing.filter"].clone();
    definition.id = "test.ink-tone".into();
    definition.binding = crate::plugins::nodes::Binding::Image {
        operation: "filter".into(),
        defaults: json!({"filter":"invert","amount":0.1}),
    };
    definition.parameters_schema = Some(json!({"type":"object","required":["filter","amount"],
        "properties":{"filter":{"type":"string","enum":["invert"]},
        "amount":{"type":"number","minimum":0,"maximum":0.1}},"additionalProperties":false}));
    s.registry
        .nodes
        .insert(definition.id.clone(), ("test".into(), definition));
}
#[test]
fn adjustment_provider_schema_is_authoritative_for_hidden_disabled_and_persisted_items() {
    let mut s = fixture(1);
    alias(&mut s);
    let result = call(
        &mut s,
        "adjustment.create",
        json!({"node_type":"test.ink-tone","visible":false}),
    );
    let id = result["adjustment_id"].as_u64().unwrap();
    let item = call(&mut s, "adjustment.get", json!({"adjustment_id":id}));
    assert_eq!(item["parameters"], json!({"filter":"invert","amount":0.1}));
    let before = content(&s);
    let revision = s.revision;
    for value in [
        json!({"amount":0.100000001}),
        json!({"filter":"brightness"}),
        json!({"extra":true}),
    ] {
        assert!(s
            .apply(Action::new(
                "adjustment.set",
                json!({"adjustment_id":id,"parameters":value})
            ))
            .is_err());
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
    }
    for node_type in ["missing.filter", "compositing.blend", "action.layer.rename"] {
        let mut value = args();
        value["node_type"] = json!(node_type);
        value["visible"] = json!(false);
        assert!(s.apply(Action::new("adjustment.create", value)).is_err());
        assert_eq!(content(&s), before);
    }
    let macro_action:crate::plugins::ActionDefinition=serde_json::from_value(json!({"id":"test.bad-adjustment","title":"Invalid adjustment suffix",
        "steps":[{"action":"layer.rename","args":{"name":"Must not publish"}},
        {"action":"adjustment.create","args":{"node_type":"test.ink-tone","visible":false,"parameters":{"amount":0.2}}}]})).unwrap();
    s.registry
        .actions
        .insert(macro_action.id.clone(), macro_action);
    assert!(s
        .apply(Action::new("test.bad-adjustment", json!({})))
        .is_err());
    assert_eq!(content(&s), before);
    assert_eq!(s.revision, revision);
    let mut graph = s.doc.graph.clone();
    graph.enabled = false;
    call(&mut s, "graph.set", json!({"graph":graph}));
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/adjustments-provider/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("hidden-disabled-alias.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let mut receiver = fixture(1);
    let receiving_document = content(&receiver);
    let receiving_state = receiver.state();
    assert!(receiver
        .apply(Action::new("document.open", json!({"path":path})))
        .is_err());
    assert_eq!(content(&receiver), receiving_document);
    assert_eq!(receiver.state(), receiving_state);
    outcome(&mut receiver);
    let before = content(&s);
    s.registry.nodes.remove("test.ink-tone");
    assert_eq!(
        call(&mut s, "adjustment.query", json!({}))["items"][0]["provider_available"],
        false
    );
    assert!(s
        .apply(Action::new(
            "adjustment.set",
            json!({"adjustment_id":id,"name":"Unavailable provider"})
        ))
        .is_err());
    assert_eq!(
        content(&s),
        before,
        "disabled graphs must not hide unavailable definitions from publication preflight"
    );
    alias(&mut s);
    outcome(&mut s);
}

fn pointer(tool: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 1.5 + (time % 2) as f32 * 0.2,
        y: 1.5,
        pressure: 0.25 + (time % 5) as f32 * 0.1,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
        view_x: Some(time as f32),
        view_y: Some(40.),
    }
}
fn transcript(s: &mut Session) -> Vec<Value> {
    let journal = s.replay_journal(false).unwrap();
    let events = journal.events(0, journal.entries.len()).unwrap();
    let mut result: Vec<Value> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    result.extend(
        s.replay_samples
            .iter()
            .map(|p| serde_json::to_value(p).unwrap()),
    );
    result
}
#[test]
fn adjustment_reads_and_inspection_preserve_chunked_contacts_while_mutations_reject() {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        let mut s = fixture(1);
        let id = create(&mut s, json!({}))["adjustment_id"].as_u64().unwrap();
        call(
            &mut s,
            "brush.set",
            json!({"radius":1.2,"hardness":1,"stabilizer":0,"pressure_size":false}),
        );
        s.pointer(pointer(tool, PointerPhase::Down, 100)).unwrap();
        for i in 1..280 {
            s.pointer(pointer(tool, PointerPhase::Move, 100 + i))
                .unwrap();
        }
        let samples = transcript(&mut s);
        assert_eq!(samples.len(), 280);
        let before = content(&s);
        let revision = s.revision;
        let history = s.doc.history_bytes();
        for action in [
            Action::new("adjustment.create", args()),
            Action::new(
                "adjustment.set",
                json!({"adjustment_id":id,"visible":false}),
            ),
            Action::new(
                "adjustment.move",
                json!({"adjustment_id":id,"parent_id":0,"index":0}),
            ),
            Action::new("adjustment.delete", json!({"adjustment_id":id})),
        ] {
            assert_eq!(s.apply(action).unwrap_err().code, "stroke_active");
            assert_eq!(content(&s), before);
            assert_eq!(s.revision, revision);
            assert_eq!(s.doc.history_bytes(), history);
            assert!(s.stroke.active() && s.pointer_checkpoint.is_some());
            assert_eq!(transcript(&mut s), samples);
        }
        for (action, arguments) in [
            ("adjustment.catalog", json!({})),
            ("adjustment.query", json!({})),
            ("adjustment.get", json!({"adjustment_id":id})),
        ] {
            call(&mut s, action, arguments);
        }
        assert_eq!(
            s.apply(Action::new(
                "adjustment.inspect",
                json!({"adjustment_id":id})
            ))
            .unwrap_err()
            .code,
            "ui_unavailable"
        );
        assert!(s.ui_effects.is_empty());
        s.runtime = "desktop";
        assert_eq!(
            call(&mut s, "adjustment.inspect", json!({"adjustment_id":id})),
            json!({"queued":true,"adjustment_id":id})
        );
        assert_eq!(
            s.ui_effects.pop_front().unwrap(),
            json!({"id":"adjustment.inspect","value":id,"document_identity":s.doc.identity})
        );
        assert_eq!(content(&s), before);
        assert_eq!(s.revision, revision);
        assert_eq!(transcript(&mut s), samples);
        s.pointer(pointer(tool, PointerPhase::Up, 380)).unwrap();
        let samples = transcript(&mut s);
        assert_eq!(samples.len(), 281);
        assert_eq!(samples.last().unwrap()["phase"], "up");
        assert!(!s.stroke.active());
        outcome(&mut s);
    }
}

struct ControlledRenderer {
    native: Arc<dyn Renderer>,
    mix: bool,
    armed: Cell<bool>,
    calls: Cell<usize>,
}
impl ControlledRenderer {
    fn fail(&self, out: &mut abi::Pixels) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if self.armed.replace(false) {
            unsafe { out.data.write(0xdeadc0de) };
            Err("Injected adjustment publication failure".into())
        } else {
            Ok(())
        }
    }
}
impl Renderer for ControlledRenderer {
    fn supports_masks(&self) -> bool {
        self.native.supports_masks()
    }
    fn supports_mask_effects(&self) -> bool {
        self.native.supports_mask_effects()
    }
    fn supports_mix(&self) -> bool {
        self.mix && self.native.supports_mix()
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
        self.fail(o)?;
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
        self.fail(o)?;
        self.native.render_masked(n, l, m, w, h, o)
    }
}
#[test]
fn adjustment_hidden_candidates_require_mix_and_late_renderer_failure_restores_publication() {
    for capability in [false, true] {
        let mut s = fixture(1);
        selection(&mut s);
        let before = content(&s);
        let history = s.doc.history_bytes();
        let renderer = Arc::new(ControlledRenderer {
            native: s.doc.renderer.as_ref().unwrap().clone(),
            mix: capability,
            armed: Cell::new(true),
            calls: Cell::new(0),
        });
        s.doc.renderer = Some(renderer.clone());
        let public = s.state();
        let mut value = args();
        value["visible"] = json!(capability);
        value["mask_source"] = json!("selection");
        assert!(s.apply(Action::new("adjustment.create", value)).is_err());
        assert_eq!(renderer.calls.get(), usize::from(capability));
        assert_eq!(content(&s), before);
        assert_eq!(s.state(), public);
        assert_eq!(s.doc.history_bytes(), history);
        s.doc.renderer = Some(renderer.native.clone());
        let result = create(&mut s, json!({"mask_source":"selection"}));
        assert_eq!(result["adjustment_id"], 1);
        assert_eq!(result["mask_ids"], json!([1]));
        outcome(&mut s);
    }
}

#[test]
fn adjustment_project_and_dll_free_outcomes_preserve_metadata_masks_and_saved_undo_bytes() {
    let mut s = fixture(1);
    selection(&mut s);
    let result = create(
        &mut s,
        json!({"mask_source":"selection","opacity":128./255.}),
    );
    let id = result["adjustment_id"].as_u64().unwrap();
    let mask = result["mask_ids"][0].as_u64().unwrap();
    let original = content(&s);
    call(&mut s, "adjustment.delete", json!({"adjustment_id":id}));
    let deleted = content(&s);
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"undo","action":"history.undo"},
            {"id":"mask","action":"mask.fill","args":{"mask_id":mask,"value":9}},
            {"id":"fail","action":"adjustment.delete","args":{"adjustment_id":99999}}
        ]}),
    );
    assert_eq!(result["ok"], false);
    assert_eq!(result["rolled_back"], true);
    assert_eq!(content(&s), deleted);
    assert_eq!(s.doc.history_bytes(), history);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), original);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(content(&s), deleted);
    call(&mut s, "history.undo", json!({}));
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "target/test-output/adjustments/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("selection-adjustment.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let expected = content(&s);
    let mut loaded = fixture(1);
    call(&mut loaded, "document.open", json!({"path":path}));
    assert_eq!(content(&loaded), expected);
    outcome(&mut s);
}

#[test]
fn adjustment_graph_disable_bypasses_filters_and_subtree_history_keeps_real_raster_projection() {
    let mut s = fixture(1);
    call(
        &mut s,
        "graph.set",
        json!({"graph":{"enabled":true,"linked":true,"nodes":[
        {"id":10,"kind":"layer","index":0,"position":[0,0]},
        {"id":11,"kind":"filter","filter":"grayscale","amount":1,"position":[100,0]},
        {"id":12,"kind":"output","position":[200,0]}],"edges":[
        {"from":10,"to":11,"input":0},{"from":11,"to":12,"input":0}]}}),
    );
    let source = s.doc.graph.clone();
    let raw = pixels(&s);
    let addresses = pointers(&s);
    let group = call(
        &mut s,
        "group.create",
        json!({"members":[{"kind":"layer","id":1}]}),
    )["group_id"]
        .as_u64()
        .unwrap();
    selection(&mut s);
    let result = create(&mut s, json!({"parent_id":group,"mask_source":"selection"}));
    let id = result["adjustment_id"].as_u64().unwrap();
    let mask = result["mask_ids"][0].as_u64().unwrap();
    call(
        &mut s,
        "mask.set",
        json!({"mask_id":mask,"enabled":false,"linked":false}),
    );
    assert_eq!(s.doc.composite[3], 0x80d9d9d9);
    let mut disabled = s.doc.graph.clone();
    disabled.enabled = false;
    call(&mut s, "graph.set", json!({"graph":disabled}));
    assert_eq!(s.doc.composite[3], 0x80402010);
    assert_eq!(pixels(&s), raw);
    assert_eq!(pointers(&s), addresses);
    assert_eq!(s.doc.adjustments()[0].id, id);
    let copy = call(&mut s, "group.duplicate", json!({"group_id":group}))["group_id"]
        .as_u64()
        .unwrap();
    assert_ne!(copy, group);
    assert_eq!(
        s.doc.layers.len(),
        2,
        "subtree copy adds only the actual raster"
    );
    assert_eq!(s.doc.adjustments().len(), 2);
    let copied = s.doc.adjustments()[1].id;
    assert_ne!(copied, id);
    let copied_mask = s
        .doc
        .mask_state()
        .items
        .iter()
        .find(|m| m.owner == (MaskOwner::Adjustment { id: copied }))
        .unwrap()
        .id;
    assert_ne!(copied_mask, mask);
    assert_eq!(raw_masks(&s)[&copied_mask], raw_masks(&s)[&mask]);
    let original = content(&s);
    let deleted = call(&mut s, "group.delete", json!({"group_id":group}));
    assert_eq!(deleted["deleted_adjustment_ids"], json!([id]));
    assert!(s.doc.adjustment(id).is_err());
    assert!(s.doc.mask_plane(mask).is_err());
    assert!(s.doc.adjustment(copied).is_ok());
    assert!(s.doc.mask_plane(copied_mask).is_ok());
    call(&mut s, "history.undo", json!({}));
    assert_eq!(content(&s), original);
    let mut source_now = s.doc.graph.clone();
    source_now.enabled = source.enabled;
    // Raster source identities/filter parameters survive structural compilation;
    // legacy stack synchronization may arrange their presentation positions.
    assert!(source_now
        .nodes
        .iter()
        .any(|n| n.id == 11 && n.data == source.nodes[1].data));
    outcome(&mut s);
}
