//! Physical creation normalizes commands to bounded pixels without provisional edits.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::{document_info::Info, replay::Journal};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-output/physical-creation-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
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
#[test]
fn physical_creation_plans_a4_and_equivalent_units_without_allocating_a_document() {
    let mut s = session();
    let state = s.state();
    let identity = s.doc.identity;
    let pointer = s.doc.layers[0].pixels.as_ptr();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let a4 = call(
        &mut s,
        "canvas.new.plan",
        json!({"width":210,"height":297,"unit":"mm","resolution_ppi":300.0,"background":"#FFFFFF"}),
    );
    assert_eq!(a4["width"], 2480);
    assert_eq!(a4["height"], 3508);
    assert_eq!(a4["estimated_initial_pixel_bytes"], 2480u64 * 3508 * 8);
    assert_eq!(a4["document_info"]["resolution_ppi"], 300);
    assert_eq!(a4["document_info"]["initial_background"], "#FFFFFF");
    assert_eq!(a4["revision"], s.revision);
    assert_eq!(a4["drawing"], false);
    let metric = call(
        &mut s,
        "canvas.new.plan",
        json!({"width":25.4,"height":12.7,"unit":"mm","resolution_ppi":100}),
    );
    let inches = call(
        &mut s,
        "canvas.new.plan",
        json!({"width":1,"height":0.5,"unit":"in","resolution_ppi":100}),
    );
    assert_eq!(metric["width"], 100);
    assert_eq!(metric["height"], 50);
    for key in [
        "width",
        "height",
        "document_info",
        "estimated_initial_pixel_bytes",
        "revision",
        "drawing",
    ] {
        assert_eq!(metric[key], inches[key]);
    }
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.identity, identity);
    assert_eq!(s.doc.layers[0].pixels.as_ptr(), pointer);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
}
#[test]
fn physical_creation_rounds_half_up_and_rejects_unrepresentable_edges_without_clamping() {
    let mut s = session();
    let half = call(
        &mut s,
        "canvas.new.plan",
        json!({"width":0.5,"height":1.5,"unit":"in","resolution_ppi":1}),
    );
    assert_eq!(half["width"], 1);
    assert_eq!(half["height"], 2);
    let edge = call(
        &mut s,
        "canvas.new.plan",
        json!({"width":4096.4999,"height":1,"unit":"in","resolution_ppi":1}),
    );
    assert_eq!(edge["width"], 4096);
    assert_eq!(edge["height"], 1);
    for args in [
        json!({"width":0.4999,"height":1,"unit":"in","resolution_ppi":1}),
        json!({"width":4096.5,"height":1,"unit":"in","resolution_ppi":1}),
        json!({"width":0.5,"height":1,"unit":"px","resolution_ppi":1}),
        json!({"width":4096.4999,"height":1,"unit":"px","resolution_ppi":1}),
    ] {
        assert!(s.apply(Action::new("canvas.new.plan", args)).is_err());
    }
}
#[test]
fn physical_creation_invalid_parameters_leave_pixels_metadata_and_history_unchanged() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    let state = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    let composition = s.doc.composition();
    for args in [
        json!({}),
        json!({"width":1}),
        json!({"width":0,"height":1,"unit":"mm"}),
        json!({"width":-1,"height":1,"unit":"mm"}),
        json!({"width":4097,"height":1}),
        json!({"width":1.5,"height":1}),
        json!({"width":1,"height":1,"unit":"cm"}),
        json!({"width":1,"height":1,"resolution_ppi":0}),
        json!({"width":1,"height":1,"resolution_ppi":2401}),
        json!({"width":1,"height":1,"resolution_ppi":72.5}),
        json!({"width":null,"height":1}),
        json!({"width":1,"height":null}),
        json!({"width":1,"height":1,"resolution_ppi":null}),
        json!({"width":1,"height":1,"unit":null}),
        json!({"width":1,"height":1,"background":null}),
        json!({"width":1,"height":1,"background":"#FFFFFFFF"}),
        json!({"width":1,"height":1,"extra":true}),
    ] {
        for id in ["canvas.new.plan", "canvas.new"] {
            assert!(
                s.apply(Action::new(id, args.clone())).is_err(),
                "{id}: {args}"
            );
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.history_bytes(), history);
        }
    }
    for number in ["NaN", "Infinity", "1e400"] {
        let wire =
            format!("{{\"action\":\"canvas.new\",\"args\":{{\"width\":{number},\"height\":1}}}}");
        assert!(serde_json::from_str::<Action>(&wire).is_err());
    }
    check(&mut s);
}
#[test]
fn physical_creation_plan_keeps_contact_live_but_creation_requires_idle() {
    let mut s = session();
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"down","x":2,"y":2,"pressure":0.5,"pen":true,"time":0}),
    );
    let state = s.state();
    let history = s.doc.history_bytes();
    let events = s.journal.as_ref().unwrap().entries.len();
    let samples = s.replay_samples.len();
    let args = json!({"width":25.4,"height":12.7,"unit":"mm","resolution_ppi":100});
    let plan = call(&mut s, "canvas.new.plan", args.clone());
    assert_eq!(plan["drawing"], true);
    assert_eq!(plan["width"], 100);
    assert_eq!(plan["height"], 50);
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), events);
    assert_eq!(s.replay_samples.len(), samples);
    assert_eq!(
        s.apply(Action::new("canvas.new", args)).unwrap_err().code,
        "stroke_active"
    );
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(s.is_drawing());
    call(
        &mut s,
        "canvas.pointer",
        json!({"phase":"up","x":3,"y":3,"pressure":0.5,"pen":true,"time":1}),
    );
    check(&mut s);
}
#[test]
fn physical_creation_explicit_default_resolution_persists_pixels_project_and_outcome_png() {
    let mut s = session();
    let args = json!({"width":1,"height":0.5,"unit":"in","background":"#aBcDeF"});
    let plan = call(&mut s, "canvas.new.plan", args.clone());
    assert_eq!(plan["width"], 72);
    assert_eq!(plan["height"], 36);
    assert_eq!(plan["document_info"]["resolution_explicit"], true);
    call(&mut s, "canvas.new", args);
    assert_eq!((s.doc.width, s.doc.height), (72, 36));
    assert_eq!(s.doc.history_bytes(), 0);
    assert_eq!(s.journal.as_ref().unwrap().entries.len(), 2);
    assert_eq!(s.doc.info().resolution_ppi, 72);
    assert!(s.doc.info().resolution_explicit);
    assert!(s.doc.info().png_density.is_none());
    assert!(s.doc.layers[0].pixels.iter().all(|&p| p == 0xffabcdef));
    let png = codec::png(&mut s.doc).unwrap();
    assert_eq!(density(&png), Some((2835, 2835, 1)));
    assert_eq!(
        s.journal.as_mut().unwrap().events(1, 1).unwrap()["events"][0]["source"]["args"]["unit"],
        "in"
    );
    let path = output("physical.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let info = s.doc.info();
    let pixels = s.doc.layers[0].pixels.clone();
    let archive = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.info(), info);
    assert_eq!(reopened.doc.layers[0].pixels, pixels);
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), png);
    check(&mut reopened);
    let mut journal = Journal::import(&archive).unwrap();
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
    let last = journal.entries.len() - 1;
    assert_eq!(replay::png(&journal.preview(last).unwrap()).unwrap(), png);
    // Legacy pixel commands without a resolution still omit all default metadata.
    call(&mut s, "canvas.new", json!({"width":12.0,"height":8.0}));
    assert_eq!(s.doc.info(), Info::default());
    assert!(serde_json::to_value(s.doc.composition())
        .unwrap()
        .get("document_info")
        .is_none());
    assert_eq!(density(&codec::png(&mut s.doc).unwrap()), None);
    check(&mut s);
}
#[test]
fn physical_creation_partial_append_failure_restores_prior_document_recording_and_history() {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    s.replay_boundary().unwrap();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    let pixels = s.doc.layers[0].pixels.clone();
    let identity = s.doc.identity;
    let count = s.journal.as_ref().unwrap().entries.len();
    let head = s.journal.as_ref().unwrap().status()["head_hash"].clone();
    let prefix: Vec<_> = s
        .journal
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.hash)
        .collect();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.new",
            json!({"width":1,"height":0.5,"unit":"in","resolution_ppi":100,"background":"#ABCDEF"})
        ))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.identity, identity);
    let journal = s.journal.as_mut().unwrap();
    // The failed candidate does not replace the old recording. Its healthy
    // journal retains the rejected request as one unchanged-state audit event.
    assert_eq!(journal.entries.len(), count + 1);
    assert_eq!(
        journal
            .entries
            .iter()
            .take(count)
            .map(|e| e.hash)
            .collect::<Vec<_>>(),
        prefix
    );
    assert_ne!(journal.status()["head_hash"], head);
    let audit = journal.events(count, 1).unwrap();
    let event = &audit["events"][0];
    assert_eq!(event["source"]["action"], "canvas.new");
    assert_eq!(event["source"]["args"]["unit"], "in");
    assert_eq!(event["success"], false);
    assert!(event["error"].as_str().is_some_and(|e| !e.is_empty()));
    assert_eq!(event["before"], event["after"]);
    assert_eq!(event["changed_tiles"], 0);
    assert_eq!(event["visual"], false);
    let restored = journal.preview(count).unwrap();
    assert_eq!(restored.metadata.width, 12);
    assert_eq!(restored.metadata.height, 8);
    assert_eq!(restored.metadata.composition, composition);
    assert_eq!(restored.pixels(), pixels);
    assert_eq!(journal.verify(None).unwrap()["valid"], true);
}
#[test]
fn physical_creation_schema_nodes_and_workflow_share_the_plan_without_implicit_execution() {
    let mut s = session();
    for id in ["canvas.new", "canvas.new.plan"] {
        let schema = s.registry.actions[id].schema();
        assert_eq!(schema["properties"]["width"]["type"], "number");
        for unit in ["px", "mm", "in"] {
            assert!(schema["properties"]["unit"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == unit));
        }
    }
    assert!(s.registry.actions["canvas.new.plan"].is_read_only());
    assert!(call(&mut s, "node.catalog", json!({}))
        .to_string()
        .contains("action.canvas.new.plan"));
    let pixels = s.doc.layers[0].pixels.clone();
    let info = s.doc.info();
    let graph = json!({"version":1,"nodes":[
        {"id":1,"type":"action.canvas.new.plan","position":[0,0],"parameters":{"width":25.4,"height":12.7,"unit":"mm","resolution_ppi":100}},
        {"id":2,"type":"action.canvas.new","position":[400,0],"parameters":{"resolution_ppi":100,"background":"#FFFFFF"}}
    ],"edges":[{"from":1,"output":"width","to":2,"input":"width"},{"from":1,"output":"height","to":2,"input":"height"}]});
    call(&mut s, "workflow.set", json!({"graph":graph}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.info(), info);
    assert_eq!(
        call(
            &mut s,
            "workflow.run",
            json!({"dry_run":true,"atomic":false})
        )["ok"],
        true
    );
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.info(), info);
    assert_eq!(
        call(&mut s, "workflow.run", json!({"atomic":false}))["ok"],
        true
    );
    assert_eq!((s.doc.width, s.doc.height), (100, 50));
    assert_eq!(s.doc.info().resolution_ppi, 100);
    assert!(s.doc.layers[0].pixels.iter().all(|&p| p == 0xffffffff));
    check(&mut s);
}

#[test]
fn physical_creation_literal_pipeline_errors_reject_before_earlier_mutations() {
    let mut s = session();
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    for invalid in [
        json!({"width":0,"height":1}),
        json!({"width":1.5,"height":1}),
        json!({"width":0.49,"height":1,"unit":"in","resolution_ppi":1}),
        json!({"width":4096.5,"height":1,"unit":"in","resolution_ppi":1}),
        json!({"width":1,"height":1,"unit":"mm","background":"invalid"}),
    ] {
        let result = s.apply(Action::new(
            "pipeline.run",
            json!({"steps":[
                {"id":"paint","action":"native.checkerboard","args":{"cell":2}},
                {"id":"new","action":"canvas.new","args":invalid}
            ]}),
        ));
        assert!(result.is_err(), "{result:?}");
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
    }
    let args = json!({"width":25.4,"height":12.7,"unit":"mm","resolution_ppi":100});
    let plan = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"plan","action":"canvas.new.plan","args":args}
        ]}),
    );
    assert_eq!(plan["ok"], true);
    assert_eq!(s.state(), before);
    assert!(s
        .apply(Action::new(
            "pipeline.run",
            json!({"atomic":true,"steps":[
                {"id":"new","action":"canvas.new","args":args}
            ]})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    check(&mut s);
}
