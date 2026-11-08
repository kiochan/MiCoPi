//! Integrated painting workflows: bound source effects and independent owner geometry.
use super::editing_tests::{call, check, session};
use super::*;
use serde_json::{json, Value};

fn setup() -> (Session, u64) {
    let mut s = session();
    call(&mut s, "native.checkerboard", json!({"cell":2}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#ff884080"}));
    call(&mut s, "layer.add", json!({}));
    call(&mut s, "layer.fill_color", json!({"color":"#8044bb80"}));
    call(
        &mut s,
        "group.create",
        json!({"name":"Ink","members":[{"kind":"layer","id":1}]}),
    );
    let mask = call(
        &mut s,
        "mask.create",
        json!({"owner":{"kind":"layer","id":1},"select":false}),
    )["mask_id"]
        .as_u64()
        .unwrap();
    call(&mut s, "mask.fill", json!({"mask_id":mask,"value":128}));
    let controller=call(&mut s,"adjustment.create",json!({"name":"Shared tone","node_type":"compositing.filter","parameters":{"filter":"invert","amount":0.7},"scope":"targets","target_layer_ids":[1,3],"opacity":0.6}))["adjustment_id"].as_u64().unwrap();
    (s, controller)
}
fn intent(s: &Session) -> Value {
    json!({"document_identity":s.doc.identity,"document_revision":s.revision,"layer_ids":[3,1],"spacing":1,"steps":[[{"x":5.,"y":4.,"dx":1.,"dy":0.5,"radius":4.,"strength":0.8}]]})
}
fn rasters(s: &Session) -> Vec<(u64, Vec<u32>)> {
    s.doc
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| (s.doc.layer_id(i), l.pixels.clone()))
        .collect()
}
fn masks(s: &Session) -> Vec<(u64, Vec<u8>)> {
    s.doc
        .mask_state()
        .items
        .iter()
        .map(|m| {
            (
                m.id,
                (0..s.doc.width * s.doc.height)
                    .map(|i| s.doc.mask_plane(m.id).unwrap().sample(i))
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn shared_tone_survives_multi_owner_liquify_preview_apply_and_one_undo() {
    let (mut s, id) = setup();
    let before = rasters(&s);
    let raw_masks = masks(&s);
    let controller = s.doc.adjustment(id).unwrap().clone();
    let composition = s.doc.composition();
    let untouched = s.doc.layers[s.doc.layer_index(2).unwrap()].pixels.as_ptr();
    let args = intent(&s);
    let preview = call(&mut s, "canvas.liquify.preview", args.clone());
    assert_eq!(rasters(&s), before);
    assert_eq!(masks(&s), raw_masks);
    assert_eq!(s.doc.composition(), composition);
    call(&mut s, "canvas.liquify", args);
    assert_eq!(s.doc.adjustment(id).unwrap(), &controller);
    assert_eq!(
        s.doc.layers[s.doc.layer_index(2).unwrap()].pixels.as_ptr(),
        untouched
    );
    let image = codec::png(&mut s.doc).unwrap();
    assert_eq!(preview["data"], codec::base64(&image));
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(rasters(&s), before);
    assert_eq!(masks(&s), raw_masks);
    assert_eq!(s.doc.composition(), composition);
    check(&mut s);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(codec::png(&mut s.doc).unwrap(), image);
    assert_eq!(s.doc.adjustment(id).unwrap(), &controller);
    check(&mut s);
}
#[test]
fn binding_or_parameter_changes_invalidate_an_existing_liquify_draft() {
    let (mut s, id) = setup();
    let request: crate::plugins::liquify::Request = serde_json::from_value(intent(&s)).unwrap();
    let draft = crate::plugins::liquify::Draft::begin(&s, request).unwrap();
    let before = rasters(&s);
    call(
        &mut s,
        "adjustment.set",
        json!({"adjustment_id":id,"target_layer_ids":[],"parameters":{"amount":0.2}}),
    );
    assert!(draft.preview(&s).is_err());
    assert_eq!(rasters(&s), before);
    assert!(s
        .doc
        .adjustment(id)
        .unwrap()
        .target_layer_ids
        .as_ref()
        .unwrap()
        .is_empty());
    check(&mut s);
}
