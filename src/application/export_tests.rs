//! Scaled output uses shared native sampling without resizing the document.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{
    document_info::{Density, DensityUnit, Info},
    graph::{BlendMode, Edge, Filter, Graph, Node, NodeKind},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/export-units");
    std::fs::create_dir_all(&root).unwrap();
    root.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn bytes(value: &Value) -> Vec<u8> {
    let text = value["data"].as_str().unwrap();
    let digit = |b: u8| -> u32 {
        match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => 0,
            _ => panic!("Invalid output base64"),
        }
    };
    let mut result = Vec::new();
    for block in text.as_bytes().chunks_exact(4) {
        let n = (digit(block[0]) << 18)
            | (digit(block[1]) << 12)
            | (digit(block[2]) << 6)
            | digit(block[3]);
        result.push((n >> 16) as u8);
        if block[2] != b'=' {
            result.push((n >> 8) as u8);
        }
        if block[3] != b'=' {
            result.push(n as u8);
        }
    }
    result
}
fn image(value: &Value) -> (u32, u32, Vec<u8>) {
    let decoded = image::load_from_memory(&bytes(value)).unwrap().to_rgba8();
    (decoded.width(), decoded.height(), decoded.into_raw())
}
fn pixel_words(value: &Value) -> Vec<u32> {
    image(value)
        .2
        .chunks_exact(4)
        .map(|p| u32::from_be_bytes([p[3], p[0], p[1], p[2]]))
        .collect()
}
fn density(data: &[u8]) -> Option<(u32, u32, u8)> {
    let mut at = 8;
    while at + 12 <= data.len() {
        let length = u32::from_be_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        if &data[at + 4..at + 8] == b"pHYs" {
            return Some((
                u32::from_be_bytes(data[at + 8..at + 12].try_into().unwrap()),
                u32::from_be_bytes(data[at + 12..at + 16].try_into().unwrap()),
                data[at + 16],
            ));
        }
        at += length + 12;
    }
    None
}
fn filtered_layers() -> (Session, Vec<u32>) {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":4,"height":2}));
    let source = [
        0xff112233, 0xff445566, 0xff778899, 0xffaabbcc, 0xff102030, 0xff405060, 0xff708090,
        0xffabcdef,
    ];
    s.doc.replace_region(0, 0, 4, 2, &source);
    call(&mut s, "layer.add", json!({}));
    let overlay = [0, 0xfffedcba, 0, 0, 0, 0, 0xff010203, 0];
    s.doc.replace_region(0, 0, 4, 2, &overlay);
    let node = |id, data| Node {
        id,
        position: [0., 0.],
        node_type: None,
        data,
    };
    let graph = Graph {
        enabled: true,
        linked: false,
        nodes: vec![
            node(1, NodeKind::Layer { index: 0 }),
            node(2, NodeKind::Layer { index: 1 }),
            node(
                3,
                NodeKind::Blend {
                    mode: BlendMode::Normal,
                    opacity: 1.,
                },
            ),
            node(
                4,
                NodeKind::Filter {
                    filter: Filter::Invert,
                    amount: 1.,
                },
            ),
            node(5, NodeKind::Output),
        ],
        edges: vec![
            Edge {
                from: 1,
                to: 3,
                input: 0,
            },
            Edge {
                from: 2,
                to: 3,
                input: 1,
            },
            Edge {
                from: 3,
                to: 4,
                input: 0,
            },
            Edge {
                from: 4,
                to: 5,
                input: 0,
            },
        ],
    };
    call(&mut s, "graph.set", json!({"graph":graph}));
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":0,"y":0,"width":1,"height":1}),
    );
    let expected = source
        .iter()
        .zip(overlay)
        .map(|(&a, b)| if b == 0 { a ^ 0xffffff } else { b ^ 0xffffff })
        .collect();
    s.replay_boundary().unwrap();
    (s, expected)
}
#[test]
fn export_preview_samples_filtered_merged_frame_and_leaves_document_history_untouched() {
    let (mut s, expected) = filtered_layers();
    let before = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pointers: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.as_ptr()).collect();
    let count = s.journal.as_ref().unwrap().entries.len();
    let identity = call(&mut s, "canvas.export.preview", json!({}));
    assert_eq!(pixel_words(&identity), expected);
    assert_eq!(bytes(&identity), codec::png(&mut s.doc).unwrap());
    let plan = call(
        &mut s,
        "canvas.export.plan",
        json!({"width":8,"method":"nearest"}),
    );
    assert_eq!(
        (plan["width"].as_u64(), plan["height"].as_u64()),
        (Some(8), Some(4))
    );
    assert_eq!(plan["identity"], false);
    let preview = call(
        &mut s,
        "canvas.export.preview",
        json!({"width":8,"method":"nearest"}),
    );
    let scaled = pixel_words(&preview);
    for y in 0..4 {
        for x in 0..8 {
            assert_eq!(scaled[y * 8 + x], expected[(y / 2) * 4 + x / 2]);
        }
    }
    let snapshot = call(
        &mut s,
        "canvas.snapshot",
        json!({"width":8,"method":"nearest"}),
    );
    assert_eq!(bytes(&snapshot), bytes(&preview));
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    let path = output("scaled.png");
    let exported = call(
        &mut s,
        "canvas.export",
        json!({"path":path,"width":8,"method":"nearest"}),
    );
    let file = std::fs::read(&path).unwrap();
    assert_eq!(file, bytes(&preview));
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(&file));
    assert_eq!(exported["sha256"], digest);
    let event = s.journal.as_mut().unwrap().events(count, 1).unwrap();
    let artifact = &event["events"][0]["source"]["context"]["export"];
    assert_eq!(artifact["sha256"], digest);
    assert_eq!(artifact["path"], json!(path));
    assert_eq!(artifact["width"], 8);
    assert_eq!(artifact["height"], 4);
    assert_eq!(artifact["method"], "nearest");
    assert_eq!(artifact["document_info"], exported["document_info"]);
    let mut after = before.clone();
    after["revision"] = json!(s.revision);
    assert_eq!(s.revision, before["revision"].as_u64().unwrap() + 1);
    assert_eq!(s.state(), after);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), count + 1);
    check(&mut s);
    std::fs::remove_file(&path).unwrap();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut recorded = crate::engine::replay::Journal::import(&archive).unwrap();
    assert_eq!(recorded.verify(None).unwrap()["valid"], true);
    assert_eq!(
        recorded.events(count, 1).unwrap()["events"][0]["source"]["context"]["export"]["sha256"],
        digest
    );
    let last = recorded.entries.len() - 1;
    assert_eq!(
        replay::png(&recorded.preview(last).unwrap()).unwrap(),
        bytes(&identity)
    );
    assert!(
        !path.exists(),
        "Outcome playback must not repeat an external export"
    );
}
#[test]
fn export_default_area_retains_thin_line_coverage_without_selection_cropping() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":8,"height":4}));
    let mut source = vec![0; 32];
    for y in 0..4 {
        source[y * 8 + 3] = 0xffff0000;
    }
    s.doc.replace_region(0, 0, 8, 4, &source);
    call(
        &mut s,
        "selection.shape",
        json!({"kind":"rectangle","x":7,"y":0,"width":1,"height":4}),
    );
    let composition = s.doc.composition();
    let preview = call(&mut s, "canvas.export.preview", json!({"width":2}));
    assert_eq!(image(&preview), (2, 1, vec![255, 0, 0, 64, 0, 0, 0, 0]));
    assert_eq!(preview["method"], "area");
    assert_eq!((s.doc.width, s.doc.height), (8, 4));
    assert_eq!(s.doc.layers[0].pixels, source);
    assert_eq!(s.doc.composition(), composition);
}
#[test]
fn export_physical_dimensions_use_raw_axes_or_explicit_override_without_editing_density() {
    let mut s = session();
    call(
        &mut s,
        "canvas.new",
        json!({"width":8,"height":4,"background":"#FFFFFF"}),
    );
    let raw = Density {
        xppu: 3937,
        yppu: 7874,
        unit: DensityUnit::Meter,
    };
    s.doc.set_import_info(Info::with_png_density(raw)).unwrap();
    s.replay_boundary().unwrap();
    let before = s.state();
    let plan = call(
        &mut s,
        "canvas.export.plan",
        json!({"width":25.4,"height":25.4,"unit":"mm","lock_aspect":false}),
    );
    assert_eq!(plan["width"], 100);
    assert_eq!(plan["height"], 200);
    assert_eq!(
        plan["document_info"]["png_density"],
        json!({"xppu":3937,"yppu":7874,"unit":"meter"})
    );
    let overridden = call(
        &mut s,
        "canvas.export.preview",
        json!({"width":1,"height":1,"unit":"in","lock_aspect":false,"resolution_ppi":72}),
    );
    assert_eq!(
        (overridden["width"].as_u64(), overridden["height"].as_u64()),
        (Some(72), Some(72))
    );
    assert_eq!(density(&bytes(&overridden)), Some((2835, 2835, 1)));
    assert_eq!(s.state(), before);
    let unknown = Density {
        xppu: 4,
        yppu: 3,
        unit: DensityUnit::Unspecified,
    };
    s.doc
        .set_import_info(Info::with_png_density(unknown))
        .unwrap();
    s.replay_boundary().unwrap();
    let before = s.state();
    assert!(s
        .apply(Action::new(
            "canvas.export.plan",
            json!({"width":25.4,"unit":"mm"})
        ))
        .is_err());
    let resolved = call(
        &mut s,
        "canvas.export.plan",
        json!({"width":25.4,"unit":"mm","resolution_ppi":300}),
    );
    assert_eq!(resolved["width"], 300);
    assert_eq!(resolved["height"], 150);
    let percent = call(&mut s, "canvas.export.preview", json!({"scale":50}));
    assert_eq!(
        (percent["width"].as_u64(), percent["height"].as_u64()),
        (Some(4), Some(2))
    );
    assert_eq!(density(&bytes(&percent)), Some((4, 3, 0)));
    assert_eq!(s.state(), before);
}
#[test]
fn export_rejects_invalid_sizes_stale_revisions_and_atomic_file_effects_before_writing() {
    let mut s = session();
    let path = output("keep.png");
    std::fs::write(&path, b"previous destination").unwrap();
    let before = s.state();
    let history = s.doc.history_bytes();
    for args in [
        json!({"width":0}),
        json!({"width":2.5}),
        json!({"width":4097}),
        json!({"width":5,"height":5}),
        json!({"width":5,"lock_aspect":false}),
        json!({"scale":0}),
        json!({"scale":50,"width":4}),
        json!({"width":5,"unit":"bad"}),
        json!({"width":5,"method":"bad"}),
        json!({"resolution_ppi":0}),
    ] {
        let mut file = args.clone();
        file["path"] = json!(path);
        assert!(
            s.apply(Action::new("canvas.export", file)).is_err(),
            "{args}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"previous destination");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
    }
    let mut stale = Action::new("canvas.export", json!({"path":path,"width":6}));
    stale.expected_revision = Some(s.revision + 1);
    assert!(s.apply(stale).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous destination");
    let result=s.apply(Action::new("pipeline.run",json!({"atomic":true,"steps":[{"id":"export","action":"canvas.export","args":{"path":path}}]})));
    assert!(result.is_err() || result.unwrap()["result"]["ok"] == false);
    assert_eq!(std::fs::read(&path).unwrap(), b"previous destination");
    assert_eq!(s.state(), before);
}
#[test]
fn export_identity_works_without_sampling_dll_and_failures_preserve_destination() {
    let mut s = session();
    call(&mut s, "layer.fill_color", json!({"color":"#123456"}));
    s.registry.native_plugins.clear();
    let before = s.state();
    let history = s.doc.history_bytes();
    let identity = call(
        &mut s,
        "canvas.export.preview",
        json!({"resolution_ppi":72}),
    );
    assert_eq!(density(&bytes(&identity)), Some((2835, 2835, 1)));
    let plan = call(&mut s, "canvas.export.plan", json!({"width":24}));
    assert_eq!(plan["width"], 24);
    let path = output("keep.png");
    std::fs::write(&path, b"old").unwrap();
    assert!(s
        .apply(Action::new(
            "canvas.export",
            json!({"path":path,"width":24})
        ))
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    let project = output("renamed.data");
    call(&mut s, "document.save", json!({"path":project}));
    let saved = std::fs::read(&project).unwrap();
    assert!(s
        .apply(Action::new("canvas.export", json!({"path":project})))
        .is_err());
    assert_eq!(std::fs::read(&project).unwrap(), saved);
    let blocked = output("destination.mcp");
    assert!(s
        .apply(Action::new("canvas.export", json!({"path":blocked})))
        .is_err());
    assert!(!blocked.exists());
    let missing = output("missing-parent").join("out.png");
    let state = s.state();
    assert!(s
        .apply(Action::new("canvas.export", json!({"path":missing})))
        .is_err());
    assert_eq!(s.state(), state);
}
#[test]
fn export_preview_and_plan_keep_active_contact_live_while_file_export_rejects_it() {
    let mut s = session();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.5,"pen":true,"time":0}),
    );
    let before = s.state();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    assert_eq!(
        call(&mut s, "canvas.export.plan", json!({"width":6}))["drawing"],
        true
    );
    assert_eq!(
        call(&mut s, "canvas.export.preview", json!({"width":6}))["drawing"],
        true
    );
    let path = output("active.png");
    assert_eq!(
        s.apply(Action::new("canvas.export", json!({"path":path})))
            .unwrap_err()
            .code,
        "stroke_active"
    );
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(!path.exists());
    // Failed export may append its diagnostic, but queries never complete a contact.
    assert!(s.journal.as_ref().unwrap().entries.len() >= events);
    assert!(s.is_drawing());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":3,"y":3,"pressure":0.5,"pen":true,"time":1}),
    );
    check(&mut s);
}
#[test]
fn export_nodes_are_discoverable_and_editing_the_workflow_does_not_export() {
    let mut s = session();
    let catalog = call(&mut s, "node.catalog", json!({})).to_string();
    assert!(catalog.contains("action.canvas.export.plan"));
    assert!(catalog.contains("action.canvas.export.preview"));
    let pixels = s.doc.layers[0].pixels.clone();
    let info = s.doc.info();
    let graph = json!({"version":1,"nodes":[{"id":1,"type":"action.canvas.export.plan","position":[0,0],"parameters":{"width":6}},{"id":2,"type":"action.canvas.export.preview","position":[400,0],"parameters":{"method":"nearest"}}],"edges":[{"from":1,"output":"width","to":2,"input":"width"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.info(), info);
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
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.info(), info);
    assert_eq!((s.doc.width, s.doc.height), (12, 8));
    check(&mut s);
}

#[test]
fn export_compositor_failure_never_replaces_an_existing_destination() {
    let (mut s, _) = filtered_layers();
    s.doc.renderer = None;
    s.doc.invalidate();
    s.doc.compose();
    assert!(s.doc.render_error.is_some());
    let history = s.doc.history_bytes();
    let composition = s.doc.composition();
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let path = output("compositor.png");
    std::fs::write(&path, b"keep original").unwrap();
    assert!(s
        .apply(Action::new("canvas.export", json!({"path":path,"width":8})))
        .is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"keep original");
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
}

#[test]
fn export_partial_recording_failure_reports_published_file_and_preserves_committed_journal() {
    use sha2::Digest;
    let (mut s, _) = filtered_layers();
    let preview = call(
        &mut s,
        "canvas.export.preview",
        json!({"width":8,"method":"nearest"}),
    );
    let expected = bytes(&preview);
    let digest = format!("{:x}", sha2::Sha256::digest(&expected));
    s.replay_boundary().unwrap();
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
    let path = output("published-before-recording-fault.png");
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    let error = s
        .apply(Action::new(
            "canvas.export",
            json!({"path":path,"width":8,"method":"nearest"}),
        ))
        .unwrap_err();
    assert_eq!(error.code, "export_recording");
    assert!(
        error.message.contains(&path.to_string_lossy().to_string()),
        "{error}"
    );
    assert!(error.message.contains(&digest), "{error}");
    assert_eq!(std::fs::read(path).unwrap(), expected);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        s.doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
    let journal = s.journal.as_mut().unwrap();
    assert_eq!(journal.entries.len(), count);
    assert_eq!(journal.status()["head_hash"], head);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
}
#[test]
#[ignore = "Explicit Release cold-output measurement; excludes source preparation and recording"]
fn export_preview_cost() {
    use std::time::Instant;
    let side = 2048usize;
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":side,"height":side}));
    let source_word = |index: usize| {
        let value = (index as u32).wrapping_mul(0x9e3779b9).rotate_left(13);
        0xff000000 | ((value ^ (value >> 11)) & 0xffffff)
    };
    for (index, pixel) in s.doc.layers[0].pixels.iter_mut().enumerate() {
        *pixel = source_word(index);
    }
    s.doc.invalidate();
    // Populate normal composition and the source recording before all timers.
    s.doc.compose();
    s.replay_boundary().unwrap();
    let before = s.state();
    let history = s.doc.history_bytes();
    let count = s.journal.as_ref().unwrap().entries.len();
    eprintln!("Export preview {side}x{side} deterministic opaque source, cached Session/DLLs and warm composite. Timers span Session.apply dispatch, cached composition, native resampling when needed, PNG encoding and Base64/JSON result construction. Source preparation, source recording, reference decoding/assertions, filesystem I/O and pen latency are excluded.");
    for (label, args) in [
        ("identity", json!({})),
        ("area", json!({"width":1024,"method":"area"})),
        ("nearest", json!({"width":1024,"method":"nearest"})),
    ] {
        let request = Action::new("canvas.export.preview", args);
        let started = Instant::now();
        let result = s.apply(request).unwrap();
        let elapsed = started.elapsed();
        let value = &result["result"];
        let encoded = bytes(value);
        let decoded = image::load_from_memory(&encoded).unwrap().to_rgba8();
        let target = if label == "identity" { side } else { side / 2 };
        assert_eq!(decoded.dimensions(), (target as u32, target as u32));
        assert_eq!(value["width"], target);
        assert_eq!(value["height"], target);
        assert_eq!(value["identity"], label == "identity");
        assert_eq!(density(&encoded), None);
        for (index, pixel) in decoded.chunks_exact(4).enumerate() {
            let x = index % target;
            let y = index / target;
            let expected = match label {
                "identity" => source_word(index),
                "nearest" => source_word((y * 2 + 1) * side + x * 2 + 1),
                _ => {
                    let block = [
                        source_word(y * 2 * side + x * 2),
                        source_word(y * 2 * side + x * 2 + 1),
                        source_word((y * 2 + 1) * side + x * 2),
                        source_word((y * 2 + 1) * side + x * 2 + 1),
                    ];
                    let mut word = 0xff000000;
                    for shift in [0, 8, 16] {
                        let sum: u32 = block.iter().map(|p| (p >> shift) & 255).sum();
                        word |= ((sum + 2) / 4) << shift;
                    }
                    word
                }
            };
            assert_eq!(
                u32::from_be_bytes([pixel[3], pixel[0], pixel[1], pixel[2]]),
                expected,
                "{label} at ({x},{y})"
            );
        }
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.journal.as_ref().unwrap().entries.len(), count);
        eprintln!("  {label}: {target}x{target}, Session.apply={elapsed:?}, PNG={} bytes, Base64={} bytes; source layer={} bytes, existing warm composite={} bytes, required sampled ARGB={} bytes, encoding RGBA={} bytes. Encoded PNG/Base64/JSON capacities, DLL axis/row plans, retained recording state and post-timer reference decoder allocations are additional; this is not a process peak-memory measurement.",encoded.len(),value["data"].as_str().unwrap().len(),side*side*4,side*side*4,if label=="identity" {0} else {target*target*4},target*target*4);
    }
}

#[test]
#[cfg(windows)]
fn export_missing_active_project_cannot_be_replaced_through_win32_suffix_alias() {
    let mut s = session();
    let project = output("missing-active.data");
    s.file = Some(project.clone());
    s.replay_boundary().unwrap();
    let before = s.state();
    for suffix in [".", " "] {
        let alias = PathBuf::from(format!("{}{}", project.display(), suffix));
        assert!(s
            .apply(Action::new("canvas.export", json!({"path":alias})))
            .is_err());
        assert!(!project.exists());
        assert_eq!(s.state(), before);
    }
}
#[test]
fn export_recording_failure_receipt_survives_non_atomic_pipeline_and_workflow_reports() {
    use sha2::Digest;
    for workflow in [false, true] {
        let (mut s, _) = filtered_layers();
        let path = output(if workflow {
            "workflow-recording-fault.png"
        } else {
            "pipeline-recording-fault.png"
        });
        std::fs::write(&path, b"previous external artifact").unwrap();
        if workflow {
            call(
                &mut s,
                "workflow.set",
                json!({"graph":{"version":1,"nodes":[
                {"id":1,"type":"action.canvas.export","position":[0,0],"parameters":{"path":path,"width":8,"method":"nearest"}},
                {"id":2,"type":"action.layer.add","position":[400,0],"parameters":{}}
            ],"edges":[{"from":1,"output":"$exec","to":2,"input":"$exec"}]}}),
            );
        }
        let preview = call(
            &mut s,
            "canvas.export.preview",
            json!({"width":8,"method":"nearest"}),
        );
        let expected = bytes(&preview);
        let digest = format!("{:x}", sha2::Sha256::digest(&expected));
        s.replay_boundary().unwrap();
        let count = s.journal.as_ref().unwrap().entries.len();
        let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
        let composition = s.doc.composition();
        let history = s.doc.history_bytes();
        let pixels: Vec<_> = s.doc.layers.iter().map(|l| l.pixels.clone()).collect();
        s.journal.as_mut().unwrap().fail_next_write_after(7);
        let result = if workflow {
            call(&mut s, "workflow.run", json!({"atomic":false}))
        } else {
            call(
                &mut s,
                "pipeline.run",
                json!({"atomic":false,"steps":[
                    {"id":"export","action":"canvas.export","args":{"path":path,"width":8,"method":"nearest"}},
                    {"id":"never","action":"layer.add"}
                ]}),
            )
        };
        assert_eq!(result["ok"], false);
        let report = if workflow {
            assert_eq!(result["failed_node"], 1);
            assert_eq!(result["completed_nodes"], json!([]));
            &result["execution"]
        } else {
            assert_eq!(result["failed_step"], "export");
            &result
        };
        assert_eq!(report["transactional"], false);
        assert_eq!(report["completed"], json!([]));
        assert_eq!(report["error"]["code"], "export_recording");
        let message = report["error"]["message"].as_str().unwrap();
        assert!(
            message.contains(&path.to_string_lossy().to_string()),
            "{message}"
        );
        assert!(message.contains(&digest), "{message}");
        assert_eq!(std::fs::read(path).unwrap(), expected);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(
            s.doc
                .layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
        );
        let journal = s.journal.as_mut().unwrap();
        assert_eq!(journal.entries.len(), count);
        assert_eq!(journal.status()["head_hash"], head);
        assert_eq!(journal.verify(None).unwrap()["valid"], true);
    }
}
