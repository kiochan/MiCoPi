//! Physical density follows document axes, independently of visible pixel changes.
use super::editing_tests::{call, check, session};
use super::*;
use crate::engine::document_info::{Density, DensityUnit, Info};
use serde_json::json;

fn imported_density(s: &mut Session, density: Density) {
    let info = Info {
        png_density: Some(density),
        ..Info::default()
    };
    let _ = s.doc.set_import_info(info);
    assert_eq!(s.doc.info(), info);
    s.replay_boundary().unwrap();
}
fn density(s: &Session) -> Density {
    s.doc.info().png_density.unwrap()
}
#[test]
fn png_density_quarter_turn_swaps_physical_axes_with_pixels_in_one_history_entry() {
    let mut s = session();
    call(&mut s, "canvas.new", json!({"width":3,"height":2}));
    s.doc.replace_region(0, 0, 3, 2, &[1, 2, 3, 4, 5, 6]);
    let original = Density {
        xppu: 3000,
        yppu: 6000,
        unit: DensityUnit::Meter,
    };
    imported_density(&mut s, original);
    let composition = s.doc.composition();
    let count = s.replay_journal(false).unwrap().entries.len();
    call(&mut s, "canvas.rotate_90_cw", json!({}));
    assert_eq!((s.doc.width, s.doc.height), (2, 3));
    assert_eq!((density(&s).xppu, density(&s).yppu), (6000, 3000));
    assert_eq!(s.doc.layers[0].pixels, [4, 1, 5, 2, 6, 3]);
    assert_eq!(s.replay_journal(false).unwrap().entries.len(), count + 1);
    check(&mut s);
    call(&mut s, "history.undo", json!({}));
    assert_eq!(s.doc.composition(), composition);
    assert_eq!((s.doc.width, s.doc.height), (3, 2));
    assert_eq!(s.doc.layers[0].pixels, [1, 2, 3, 4, 5, 6]);
    call(&mut s, "history.redo", json!({}));
    call(&mut s, "canvas.rotate_90_ccw", json!({}));
    assert_eq!(density(&s), original);
    check(&mut s);
}
#[test]
fn png_density_uniform_square_turn_is_not_an_early_pixel_noop() {
    for original in [
        Density {
            xppu: 100,
            yppu: 200,
            unit: DensityUnit::Unspecified,
        },
        Density {
            xppu: 0,
            yppu: 3000,
            unit: DensityUnit::Meter,
        },
    ] {
        for command in ["canvas.rotate_90_cw", "canvas.rotate_90_ccw"] {
            let mut s = session();
            call(&mut s, "canvas.new", json!({"width":2,"height":2}));
            imported_density(&mut s, original);
            let pixels = s.doc.layers[0].pixels.clone();
            call(&mut s, command, json!({}));
            assert_eq!(
                (density(&s).xppu, density(&s).yppu),
                (original.yppu, original.xppu)
            );
            assert_eq!(density(&s).unit, original.unit);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            call(&mut s, "history.undo", json!({}));
            assert_eq!(density(&s), original);
            call(&mut s, "history.redo", json!({}));
            assert_eq!(
                (density(&s).xppu, density(&s).yppu),
                (original.yppu, original.xppu)
            );
            check(&mut s);
        }
    }
}
#[test]
fn png_density_layer_turns_flips_crop_and_resampling_keep_document_density() {
    let original = Density {
        xppu: 100,
        yppu: 200,
        unit: DensityUnit::Unspecified,
    };
    for (command, args) in [
        ("layer.rotate_90_cw", json!({})),
        ("layer.rotate_90_ccw", json!({})),
        ("canvas.rotate_180", json!({})),
        ("canvas.flip_horizontal", json!({})),
        ("canvas.flip_vertical", json!({})),
        ("canvas.crop", json!({"x":0,"y":0,"width":2,"height":2})),
        ("canvas.resize", json!({"width":5,"height":4})),
        (
            "canvas.image_size",
            json!({"width":8,"height":6,"lock_aspect":false}),
        ),
    ] {
        let mut s = session();
        call(&mut s, "canvas.new", json!({"width":4,"height":3}));
        imported_density(&mut s, original);
        call(&mut s, command, args);
        assert_eq!(density(&s), original, "{command}");
        check(&mut s);
    }
}
#[test]
fn png_density_quarter_turn_atomic_and_partial_append_failures_restore_axes_and_history() {
    let mut s = session();
    let original = Density {
        xppu: 3000,
        yppu: 6000,
        unit: DensityUnit::Meter,
    };
    imported_density(&mut s, original);
    let state = s.state();
    let history = s.doc.history_bytes();
    let pixels = s.doc.layers[0].pixels.clone();
    let result = call(
        &mut s,
        "pipeline.run",
        json!({"atomic":true,"steps":[
            {"id":"turn","action":"canvas.rotate_90_cw"},
            {"id":"fail","action":"layer.select","args":{"layer_id":99999}}
        ]}),
    );
    assert_eq!(result["rolled_back"], true);
    assert_eq!(s.state(), state);
    assert_eq!(density(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .apply(Action::new("canvas.rotate_90_cw", json!({})))
        .is_err());
    assert_eq!(s.state(), state);
    assert_eq!(density(&s), original);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.doc.layers[0].pixels, pixels);
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}
