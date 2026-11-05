//! Density interchange shares document metadata, atomic history and recorded outcomes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::replay::Journal;
use image::ImageEncoder;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn output(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output/png-density-units");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn crc(bytes: &[u8]) -> u32 {
    let mut value = !0u32;
    for byte in bytes {
        value ^= *byte as u32;
        for _ in 0..8 {
            value = (value >> 1) ^ (0xedb88320 & (0u32.wrapping_sub(value & 1)));
        }
    }
    !value
}
fn fixture(density: Option<(u32, u32, u8)>) -> Vec<u8> {
    let pixels: Vec<u8> = (0..12)
        .flat_map(|n| [n * 17, 255 - n * 13, n * 9, 255])
        .collect();
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&pixels, 4, 3, image::ExtendedColorType::Rgba8)
        .unwrap();
    if let Some((x, y, unit)) = density {
        let mut chunk = b"pHYs".to_vec();
        chunk.extend_from_slice(&x.to_be_bytes());
        chunk.extend_from_slice(&y.to_be_bytes());
        chunk.push(unit);
        let mut encoded = 9u32.to_be_bytes().to_vec();
        encoded.extend_from_slice(&chunk);
        encoded.extend_from_slice(&crc(&chunk).to_be_bytes());
        // The independent fixture encoder emits IHDR first; place pHYs after it.
        bytes.splice(33..33, encoded);
    }
    bytes
}
fn density(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    let mut at = 8;
    while at + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        if &bytes[at + 4..at + 8] == b"pHYs" {
            assert_eq!(length, 9);
            return Some((
                u32::from_be_bytes(bytes[at + 8..at + 12].try_into().unwrap()),
                u32::from_be_bytes(bytes[at + 12..at + 16].try_into().unwrap()),
                bytes[at + 16],
            ));
        }
        at += length + 12;
    }
    None
}
fn rgba(bytes: &[u8]) -> Vec<u8> {
    image::load_from_memory(bytes)
        .unwrap()
        .to_rgba8()
        .into_raw()
}
fn open(d: Option<(u32, u32, u8)>) -> Session {
    // A non-image extension verifies signature-based format detection.
    let path = output("source.data");
    std::fs::write(&path, fixture(d)).unwrap();
    let mut s = session();
    call(&mut s, "document.open", json!({"path":path}));
    s
}
fn assert_info(info: &Value, d: (u32, u32, u8)) {
    assert_eq!(
        info["png_density"],
        json!({"xppu":d.0,"yppu":d.1,"unit":if d.2==1 {"meter"} else {"unspecified"}})
    );
    assert_eq!(info["resolution_source"], "png");
    let known = d.2 == 1 && d.0 != 0 && d.1 != 0;
    assert_eq!(info["physical_size_known"], known);
    if known {
        assert!((info["width_mm"].as_f64().unwrap() - 4000. / d.0 as f64).abs() < 1e-10);
        assert!((info["height_mm"].as_f64().unwrap() - 3000. / d.1 as f64).abs() < 1e-10);
        assert!((info["resolution_ppi_x"].as_f64().unwrap() - d.0 as f64 * 0.0254).abs() < 1e-8);
        assert!((info["resolution_ppi_y"].as_f64().unwrap() - d.1 as f64 * 0.0254).abs() < 1e-8);
    } else {
        for field in [
            "width_mm",
            "height_mm",
            "resolution_ppi_x",
            "resolution_ppi_y",
        ] {
            assert!(
                info.get(field).is_none(),
                "Unknown physical density must not invent {field}: {info}"
            );
        }
    }
}
#[test]
fn png_density_raw_import_export_preserves_axes_units_and_unknown_physical_size() {
    for d in [
        (3937, 7874, 1),
        (4, 3, 0),
        (0, 7874, 1),
        (3937, 0, 1),
        (0, 0, 0),
        (i32::MAX as u32, 1, 1),
    ] {
        let mut s = open(Some(d));
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let info = call(&mut s, "canvas.info", json!({}));
        assert_info(&info, d);
        assert_eq!(s.state(), before);
        let png = codec::png(&mut s.doc).unwrap();
        assert_eq!(density(&png), Some(d));
        assert_eq!(rgba(&png), rgba(&fixture(Some(d))));
        let path = output("export.png");
        call(&mut s, "canvas.export", json!({"path":path}));
        assert_eq!(std::fs::read(path).unwrap(), png);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        check(&mut s);
    }
    let mut s = session();
    let before = s.state();
    let invalid = output("invalid-density.png");
    std::fs::write(&invalid, fixture(Some((1u32 << 31, 3937, 1)))).unwrap();
    assert!(s
        .apply(Action::new("document.open", json!({"path":invalid})))
        .is_err());
    assert_eq!(s.state(), before);
    for value in [
        json!({"xppu":1u32<<31,"yppu":1,"unit":"meter"}),
        json!({"xppu":1,"yppu":1,"unit":"invalid"}),
    ] {
        assert!(
            serde_json::from_value::<crate::engine::document_info::Info>(
                json!({"png_density":value})
            )
            .is_err()
        );
    }
}
#[test]
fn png_density_resolution_undo_atomic_and_append_failure_preserve_raw_source() {
    let d = (3937, 7874, 1);
    let mut s = open(Some(d));
    let original = codec::png(&mut s.doc).unwrap();
    let original_info = s.doc.info();
    let pixels = s.doc.layers[0].pixels.clone();
    call(&mut s, "canvas.resolution", json!({"resolution_ppi":300}));
    assert!(s.doc.info().png_density.is_none());
    let changed = codec::png(&mut s.doc).unwrap();
    assert_eq!(density(&changed), Some((11811, 11811, 1)));
    assert_eq!(rgba(&changed), rgba(&original));
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.info(), original_info);
    assert_eq!(codec::png(&mut s.doc).unwrap(), original);
    call(&mut s, "history.redo", json!({}));
    assert_eq!(codec::png(&mut s.doc).unwrap(), changed);
    call(&mut s, "history.undo", json!({}));
    let before = s.state();
    let history = s.doc.history_bytes();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[{"id":"resolution","action":"canvas.resolution","args":{"resolution_ppi":600}},{"id":"fail","action":"layer.select","args":{"layer_id":99999}}]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.info(), original_info);
    assert_eq!(s.doc.history_bytes(), history);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new(
            "canvas.resolution",
            json!({"resolution_ppi":600})
        ))
        .is_err());
    assert_eq!(s.state(), before);
    assert_eq!(s.doc.info(), original_info);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}
#[test]
fn png_density_projects_and_recordings_restore_event_specific_density() {
    let d = (4, 3, 0);
    let mut s = open(Some(d));
    let original = codec::png(&mut s.doc).unwrap();
    let original_event = s.journal.as_ref().unwrap().entries.len() - 1;
    call(&mut s, "canvas.resolution", json!({"resolution_ppi":300}));
    let changed = codec::png(&mut s.doc).unwrap();
    let changed_event = s.journal.as_ref().unwrap().entries.len() - 1;
    let path = output("density.mcp");
    call(&mut s, "document.save", json!({"path":path}));
    let expected_info = s.doc.info();
    let mut reopened = session();
    call(&mut reopened, "document.open", json!({"path":path}));
    assert_eq!(reopened.doc.info(), expected_info);
    assert_eq!(codec::png(&mut reopened.doc).unwrap(), changed);
    check(&mut reopened);
    let bytes = s.journal.as_mut().unwrap().bytes(None).unwrap();
    let mut recorded = Journal::import(&bytes).unwrap();
    assert_eq!(recorded.verify(None).unwrap()["valid"], true);
    let old_state = recorded.preview(original_event).unwrap();
    let changed_state = recorded.preview(changed_event).unwrap();
    assert_eq!(density(&replay::png(&old_state).unwrap()), Some(d));
    assert_eq!(replay::png(&old_state).unwrap(), original);
    assert_eq!(replay::png(&changed_state).unwrap(), changed);
    assert!(changed_state
        .metadata
        .composition
        .document_info
        .png_density
        .is_none());
    assert!(old_state
        .metadata
        .composition
        .document_info
        .png_density
        .is_some());
}
#[test]
fn png_density_implicit_legacy_bytes_and_explicit_72_are_distinct_without_resampling() {
    let mut s = session();
    call(
        &mut s,
        "canvas.new",
        json!({"width":4,"height":3,"background":"#AABBCC"}),
    );
    let implicit = codec::png(&mut s.doc).unwrap();
    let rgba = rgba(&implicit);
    let mut legacy = Vec::new();
    image::codecs::png::PngEncoder::new(&mut legacy)
        .write_image(&rgba, 4, 3, image::ExtendedColorType::Rgba8)
        .unwrap();
    assert_eq!(implicit, legacy);
    assert_eq!(density(&implicit), None);
    call(&mut s, "canvas.resolution", json!({"resolution_ppi":72}));
    let explicit = codec::png(&mut s.doc).unwrap();
    assert_eq!(density(&explicit), Some((2835, 2835, 1)));
    assert_eq!(
        rgba,
        image::load_from_memory(&explicit)
            .unwrap()
            .to_rgba8()
            .into_raw()
    );
    call(&mut s, "history.undo", json!({}));
    assert_eq!(codec::png(&mut s.doc).unwrap(), implicit);
    call(
        &mut s,
        "canvas.new",
        json!({"width":4,"height":3,"resolution_ppi":72,"background":"#AABBCC"}),
    );
    assert_eq!(
        density(&codec::png(&mut s.doc).unwrap()),
        Some((2835, 2835, 1))
    );
    check(&mut s);
}
