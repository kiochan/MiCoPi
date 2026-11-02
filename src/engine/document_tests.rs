use super::*;
fn brush() -> Brush {
    Brush {
        radius: 4.,
        opacity: 1.,
        hardness: 1.,
        color: 0x123456,
        erase: false,
    }
}
#[test]
fn alpha_composition() {
    assert_eq!(over(0, 0x80123456), 0x80123456);
    assert_eq!(over(0xffffffff, 0xff123456), 0xff123456);
    assert_eq!(over(0xffffffff, 0x80000000), 0xff7f7f7f);
}
#[test]
fn stroke_undo_redo_crosses_tiles() {
    let mut d = Document::new(150, 70);
    d.begin(2., 30., brush());
    d.line(145., 30., brush());
    d.finish();
    let painted = d.layers[0].pixels.clone();
    assert_ne!(painted, vec![0; 150 * 70]);
    for x in 2..145 {
        assert_ne!(painted[30 * 150 + x], 0);
    }
    d.undo();
    assert!(d.layers[0].pixels.iter().all(|p| *p == 0));
    d.redo();
    assert_eq!(painted, d.layers[0].pixels);
}
#[test]
fn eraser_and_layers() {
    let mut d = Document::new(20, 20);
    d.begin(10., 10., brush());
    d.finish();
    d.add_layer();
    d.begin(
        10.,
        10.,
        Brush {
            color: 0xff0000,
            ..brush()
        },
    );
    d.finish();
    d.compose();
    assert_eq!(d.composite[210], 0xffff0000);
    d.begin(
        10.,
        10.,
        Brush {
            erase: true,
            ..brush()
        },
    );
    d.finish();
    d.compose();
    assert_eq!(d.composite[210], 0xff123456);
    d.undo();
    d.compose();
    assert_eq!(d.composite[210], 0xffff0000);
}
#[test]
fn off_canvas_and_hidden_layer() {
    let mut d = Document::new(10, 10);
    d.begin(-100., -100., brush());
    d.finish();
    assert!(!d.dirty);
    d.layers[0].visible = false;
    d.begin(5., 5., brush());
    d.finish();
    assert!(!d.dirty);
}
#[test]
fn project_roundtrip() {
    let root = std::env::current_dir().unwrap().join("target/test-output");
    std::fs::create_dir_all(&root).unwrap();
    let p = root.join(format!("micopi-test-{}.mcp", std::process::id()));
    let mut d = Document::new(17, 13);
    d.add_layer();
    d.layers[0].opacity = 123;
    d.begin(5., 5., brush());
    d.finish();
    d.save(&p).unwrap();
    let e = Document::load(&p).unwrap();
    assert_eq!(e.layers.len(), 2);
    assert_eq!(e.active, 1);
    assert_eq!(e.layers[0].opacity, 123);
    assert_eq!(e.layers[1].pixels, d.layers[1].pixels);
    std::fs::remove_file(p).unwrap();
}
#[test]
fn display_survives_export_composition() {
    let mut d = Document::new(20, 20);
    let mut pixels = vec![];
    d.display_pixels(&mut pixels);
    let blank = pixels.clone();
    d.begin(10., 10., brush());
    d.finish();
    d.compose();
    d.display_pixels(&mut pixels);
    assert_ne!(pixels, blank);
    assert_eq!(pixels[210], 0xff123456);
    d.undo();
    d.display_pixels(&mut pixels);
    assert_eq!(pixels, blank);
}
#[test]
fn pressure_interpolates_width_and_zero_contact_is_empty() {
    use crate::engine::input::Dynamics;
    let mut d = Document::new(200, 100);
    let b = Brush {
        radius: 15.,
        ..brush()
    };
    let dynamics = Dynamics::default();
    d.begin(10., 50., dynamics.apply(b, 0.));
    d.finish();
    assert!(!d.dirty);
    d.begin(10., 50., dynamics.apply(b, 0.1));
    d.line(180., 50., dynamics.apply(b, 1.));
    d.finish();
    assert_eq!(d.layers[0].pixels[40 * 200 + 25], 0);
    assert_ne!(d.layers[0].pixels[40 * 200 + 160], 0);
    for x in 10..180 {
        assert_ne!(d.layers[0].pixels[50 * 200 + x], 0);
    }
    d.undo();
    assert!(d.layers[0].pixels.iter().all(|p| *p == 0));
}
#[test]
fn rapid_pressure_drop_never_extrapolates_backwards() {
    let mut d = Document::new(200, 120);
    let b = Brush {
        radius: 30.,
        ..brush()
    };
    d.begin(100., 60., b);
    d.line(105., 60., b);
    d.line(
        110.,
        60.,
        Brush {
            radius: 1.,
            color: 0xff0000,
            ..b
        },
    );
    d.finish();
    assert_eq!(d.layers[0].pixels[60 * 200 + 50], 0);
}
#[test]
#[ignore]
fn brush_benchmark_with_pressure_and_stabilizer() {
    use crate::engine::input::{Dynamics, Sample, Stabilizer};
    let mut d = Document::new(2048, 2048);
    let mut display = vec![];
    d.display_pixels(&mut display);
    let mut filter = Stabilizer::default();
    let dynamics = Dynamics::default();
    let first = filter.reset(Sample {
        x: 100.,
        y: 500.,
        pressure: 0.2,
        time: 0,
    });
    d.begin(first.x, first.y, dynamics.apply(brush(), first.pressure));
    let start = std::time::Instant::now();
    for i in 1..=2000 {
        let p = (i as f32 * 0.01).sin() * 0.4 + 0.6;
        let s = filter.push(
            Sample {
                x: 100. + i as f32 * 0.8,
                y: 500. + (i as f32 * 0.03).sin() * 180.,
                pressure: p,
                time: i * 4,
            },
            0.7,
        );
        d.line(s.x, s.y, dynamics.apply(brush(), s.pressure));
        d.display_pixels(&mut display);
    }
    let s = filter.finish().unwrap();
    d.line(s.x, s.y, dynamics.apply(brush(), s.pressure));
    d.finish();
    println!(
        "2048x2048 / 2000 pressure+stabilizer+compose updates: {:?}; history: {} bytes",
        start.elapsed(),
        d.history_bytes()
    );
    assert!(d.history_bytes() < 4 * 2048 * 2048);
}
#[test]
#[ignore]
fn brush_benchmark() {
    let mut d = Document::new(2048, 2048);
    let mut display = vec![];
    d.display_pixels(&mut display);
    let start = std::time::Instant::now();
    d.begin(100., 500., brush());
    for i in 1..=2000 {
        let x = 100. + i as f32 * 0.8;
        let y = 500. + (i as f32 * 0.03).sin() * 180.;
        d.line(x, y, brush());
        d.display_pixels(&mut display);
    }
    d.finish();
    println!(
        "2048x2048 / 2000 brush+compose updates: {:?}; history: {} bytes",
        start.elapsed(),
        d.history_bytes()
    );
    assert!(d.history_bytes() < 4 * 2048 * 2048);
}
