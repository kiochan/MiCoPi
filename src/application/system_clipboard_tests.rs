//! External-resource failures use an isolated in-memory adapter; no user clipboard access.
use super::editing_tests::{call, check, session};
use super::*;
use crate::platform::clipboard::{
    decode_png, encode_png, Image, ReadImage, SystemClipboard, View as ImageView,
};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
#[derive(Default)]
struct Memory {
    png: Vec<u8>,
    metadata: Vec<u8>,
    reads: usize,
    writes: usize,
    fail_write: bool,
}
struct Backend(Arc<Mutex<Memory>>);
impl SystemClipboard for Backend {
    fn status(&self) -> Result<Value, String> {
        Ok(
            json!({"available":!self.0.lock().unwrap().png.is_empty(),"png":true,"dibv5":false,"dib":false,"scope":"windows"}),
        )
    }
    fn read(&self) -> Result<ReadImage, String> {
        let mut m = self.0.lock().unwrap();
        m.reads += 1;
        Ok(ReadImage {
            image: decode_png(&m.png)?,
            metadata: Some(m.metadata.clone()),
            format: "png",
        })
    }
    fn write(&self, image: &ImageView<'_>, metadata: &[u8]) -> Result<Value, String> {
        let mut m = self.0.lock().unwrap();
        m.writes += 1;
        if m.fail_write {
            return Err("Simulated busy clipboard".into());
        }
        m.png = encode_png(image)?;
        m.metadata = metadata.to_vec();
        Ok(json!({"scope":"windows","png":true,"dibv5":false,"origin_metadata":true,"warnings":[]}))
    }
}
fn desktop() -> (Session, Arc<Mutex<Memory>>) {
    let mut s = session();
    s.runtime = "desktop";
    let memory = Arc::new(Mutex::new(Memory::default()));
    s.system_clipboard = Some(Box::new(Backend(memory.clone())));
    (s, memory)
}
fn selected(s: &mut Session) {
    call(
        s,
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":3}),
    );
}
#[test]
fn system_copy_cut_paste_preserve_alpha_undo_and_verified_origin() {
    let (mut s, m) = desktop();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    selected(&mut s);
    let pixels = s.doc.layers[0].pixels.clone();
    let mask = s.doc.selection().clone();
    call(&mut s, "clipboard.system_cut", json!({}));
    assert_eq!(m.lock().unwrap().writes, 1);
    assert_eq!(s.doc.layers[0].pixels[27], 0);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.selection(), &mask);
    let result = call(&mut s, "clipboard.system_paste_in_place", json!({}));
    assert_eq!(result["x"], 3);
    assert_eq!(result["y"], 2);
    assert_eq!(s.doc.layers[1].pixels[27], pixels[27]);
    let n = s.replay_journal(false).unwrap().entries.len() - 1;
    let event = s.replay_journal(false).unwrap().events(n, 1).unwrap();
    assert_eq!(
        event["events"][0]["source"]["context"]["system"]["origin_verified"],
        true
    );
    s.system_clipboard = None;
    s.clipboard = None;
    check(&mut s);
    assert_eq!(m.lock().unwrap().reads, 1);
}
#[test]
fn external_write_failure_never_clears_pixels_or_replaces_internal_clipboard() {
    let (mut s, m) = desktop();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "clipboard.copy", json!({}));
    selected(&mut s);
    let p = s.clipboard.clone().unwrap();
    let before = s.state();
    let composition = s.doc.composition();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    m.lock().unwrap().fail_write = true;
    assert!(s
        .apply(Action::new("clipboard.system_cut", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(Arc::ptr_eq(s.clipboard.as_ref().unwrap(), &p));
    check(&mut s);
}
#[test]
fn system_effects_reject_atomic_groups_and_headless_before_access() {
    let (mut s, m) = desktop();
    for id in [
        "clipboard.system_copy",
        "clipboard.system_copy_merged",
        "clipboard.system_cut",
        "clipboard.system_paste",
        "clipboard.system_paste_in_place",
    ] {
        assert!(s
            .apply(Action::new(
                "pipeline.run",
                json!({"atomic":true,"steps":[{"id":"external","action":id,"args":{}}]})
            ))
            .is_err());
    }
    assert_eq!(m.lock().unwrap().writes, 0);
    assert_eq!(m.lock().unwrap().reads, 0);
    s.runtime = "headless";
    for id in [
        "clipboard.system_copy",
        "clipboard.system_cut",
        "clipboard.system_paste",
        "clipboard.system_state",
    ] {
        assert_eq!(
            s.apply(Action::new(id, json!({}))).unwrap_err().code,
            "ui_unavailable"
        );
    }
    assert_eq!(m.lock().unwrap().writes, 0);
    assert_eq!(m.lock().unwrap().reads, 0);
}
#[test]
fn corrupted_images_and_untrusted_origin_hints_do_not_publish_partial_results() {
    let (mut s, m) = desktop();
    call(&mut s, "clipboard.copy", json!({}));
    let p = s.clipboard.clone().unwrap();
    let before = s.state();
    assert!(s
        .apply(Action::new("clipboard.system_paste", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert!(Arc::ptr_eq(s.clipboard.as_ref().unwrap(), &p));
    let image = Image {
        width: 2,
        height: 1,
        pixels: vec![0x00112233, 0x80402010],
    };
    {
        let mut m = m.lock().unwrap();
        m.png = encode_png(&image.view()).unwrap();
        m.metadata = serde_json::to_vec(
            &json!({"version":1,"origin":[4,3],"source_size":[12,8],"sha256":"0".repeat(64)}),
        )
        .unwrap();
    }
    let result = call(&mut s, "clipboard.system_paste_in_place", json!({}));
    assert_eq!(result["x"], 0);
    assert_eq!(result["y"], 0);
    assert_eq!(&s.doc.layers[1].pixels[..2], &image.pixels);
    check(&mut s);
}
#[test]
fn journal_failure_rolls_back_local_cut_but_external_copy_remains_available() {
    let (mut s, m) = desktop();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    selected(&mut s);
    let before = s.state();
    let pixels = s.doc.layers[0].pixels.clone();
    let history = s.doc.history_bytes();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("clipboard.system_cut", json!({})))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(s.clipboard.is_none());
    assert!(!m.lock().unwrap().png.is_empty());
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

#[test]
fn failed_system_paste_journal_restores_local_payload_and_layers() {
    let (mut s, m) = desktop();
    call(&mut s, "clipboard.copy", json!({}));
    let previous = s.clipboard.clone().unwrap();
    m.lock().unwrap().png = encode_png(
        &Image {
            width: 2,
            height: 1,
            pixels: vec![0x80402010, 0x00112233],
        }
        .view(),
    )
    .unwrap();
    let state = s.state();
    let composition = s.doc.composition();
    let history = s.doc.history_bytes();
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("clipboard.system_paste", json!({})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(s.doc.composition(), composition);
    assert_eq!(s.doc.history_bytes(), history);
    assert!(Arc::ptr_eq(s.clipboard.as_ref().unwrap(), &previous));
    assert_eq!(m.lock().unwrap().writes, 0);
}
#[test]
fn system_state_query_and_schema_validation_are_read_only_and_discoverable() {
    let (mut s, m) = desktop();
    let rev = s.revision;
    let state = call(&mut s, "clipboard.system_state", json!({}));
    assert_eq!(state["scope"], "windows");
    assert_eq!(s.revision, rev);
    for id in ["clipboard.system_copy", "clipboard.system_paste"] {
        assert!(s.apply(Action::new(id, json!({"unexpected":1}))).is_err());
    }
    assert_eq!(m.lock().unwrap().writes, 0);
    assert_eq!(m.lock().unwrap().reads, 0);
    let catalog = call(&mut s, "node.catalog", json!({}));
    for id in [
        "action.clipboard.system_copy",
        "action.clipboard.system_paste",
        "state.core.system_clipboard",
    ] {
        assert!(catalog.to_string().contains(id));
    }
}

#[test]
fn unavailable_paste_capacity_rejects_before_reading_external_pixels() {
    let (mut s, m) = desktop();
    for _ in 1..crate::engine::MAX_LAYERS {
        call(&mut s, "layer.add", json!({}));
    }
    let before = s.state();
    assert!(s
        .apply(Action::new("clipboard.system_paste", json!({})))
        .is_err());
    assert_eq!(m.lock().unwrap().reads, 0);
    assert_eq!(s.state(), before);
}
