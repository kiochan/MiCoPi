//! Pixel-exact comparison with the previous in-process implementation (test only).
use super::*;
use crate::{
    engine::Document,
    plugins::{tool_reference, Registry},
};
fn fresh() -> Session {
    let registry = Registry::bundled();
    assert!(
        registry.native_tools.contains_key("brush"),
        "compile painting DLL before tests: {:?}",
        registry.diagnostics
    );
    let mut s = Session::new(registry).unwrap();
    s.doc = Document::new(512, 384);
    s
}
fn point(phase: PointerPhase, x: f32, y: f32, pressure: f32, time: u32, pen: bool) -> Pointer {
    Pointer {
        phase,
        x,
        y,
        pressure,
        time,
        pen,
        eraser: false,
        tool_override: None,
        view_x: Some(x * 1.7),
        view_y: Some(y * 1.7),
    }
}
#[test]
fn typed_dll_brush_and_eraser_match_reference_pixels_with_pressure_and_stabilizer() {
    for (index, (pen, stabilizer, opacity, gamma, hardness, cancel)) in [
        (false, 0., false, 1., 1., false),
        (true, 0., true, 0.25, 0., false),
        (true, 0.35, false, 1., 0.85, false),
        (true, 0.9, true, 4., 1., false),
        (true, 1., true, 1.5, 0.2, true),
    ]
    .into_iter()
    .enumerate()
    {
        for kind in [ToolKind::Brush, ToolKind::Eraser] {
            let mut native = fresh();
            let mut reference = fresh();
            let mut backend = tool_reference::StrokeEngine::default();
            for s in [&mut native, &mut reference] {
                s.doc.layers[0].pixels.fill(0x80aa7788);
                s.tool = kind;
                s.selected_tool = if kind == ToolKind::Brush {
                    "brush"
                } else {
                    "eraser"
                }
                .into();
                s.parameters = Parameters {
                    radius: 8.5,
                    hardness,
                    opacity: 0.7,
                    stabilizer,
                    pressure_opacity: opacity,
                    pressure_gamma: gamma,
                    minimum_radius: 0.08,
                    color: "#395FC7".into(),
                    ..Default::default()
                };
            }
            for i in 0..65u32 {
                let phase = if i == 0 {
                    PointerPhase::Down
                } else if i == 64 {
                    if cancel {
                        PointerPhase::Cancel
                    } else {
                        PointerPhase::Up
                    }
                } else {
                    PointerPhase::Move
                };
                let pressure = if i == 0 || i == 64 {
                    0.
                } else {
                    0.05 + ((i * 7) % 27) as f32 / 28.
                };
                let sample = point(
                    phase,
                    30.25 + i as f32 * 5.,
                    100.75 + (i as f32 * 0.37).sin() * 28.,
                    pressure,
                    (u32::MAX - 10).wrapping_add(i * 4),
                    pen,
                );
                native.pointer(sample).unwrap();
                reference.pointer_reference(&mut backend, sample).unwrap();
                if i == 30 {
                    // A contact freezes parameters even if a caller changes defaults.
                    native.parameters.radius = 3.;
                    reference.parameters.radius = 3.;
                }
            }
            assert_eq!(
                native.doc.layers[0].pixels, reference.doc.layers[0].pixels,
                "case {index}, kind {kind:?}"
            );
            assert_eq!(native.doc.history_bytes(), reference.doc.history_bytes());
            native.doc.undo();
            assert!(native.doc.layers[0].pixels.iter().all(|p| *p == 0x80aa7788));
            native.doc.redo();
            assert_eq!(native.doc.layers[0].pixels, reference.doc.layers[0].pixels);
            assert!(!native.is_drawing());
        }
    }
}
#[test]
fn typed_dll_picker_hand_zoom_and_transient_tools_match_reference() {
    for kind in [ToolKind::Picker, ToolKind::Hand, ToolKind::Zoom] {
        let mut native = fresh();
        let mut reference = fresh();
        let mut backend = tool_reference::StrokeEngine::default();
        for s in [&mut native, &mut reference] {
            s.doc.layers[0].pixels.fill(0xff24ac68);
            s.doc.invalidate();
            s.view.zoom = 1.3;
        }
        let samples = [
            point(PointerPhase::Down, 80., 90., 0.5, 1, true),
            point(PointerPhase::Move, 110., 100., 0.8, 9, true),
            point(PointerPhase::Up, 120., 105., 0., 17, true),
        ];
        for mut sample in samples {
            sample.tool_override = Some(kind);
            native.pointer(sample).unwrap();
            reference.pointer_reference(&mut backend, sample).unwrap();
        }
        assert_eq!(native.view, reference.view);
        assert_eq!(native.parameters, reference.parameters);
        assert_eq!(native.tool, ToolKind::Brush);
        assert!(!native.stroke.active());
        assert_eq!(native.doc.history_bytes(), 0);
    }
}
#[test]
fn no_tool_dll_never_falls_back_to_host_rasterization() {
    let mut s = fresh();
    s.registry.native_tools.clear();
    assert!(s
        .pointer(point(PointerPhase::Down, 50., 50., 1., 1, false))
        .is_err());
    assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    assert_eq!(s.doc.history_bytes(), 0);
}
#[test]
#[ignore = "release DLL/reference throughput benchmark; not a CI timing threshold"]
fn dll_brush_benchmark() {
    use std::time::Instant;
    let mut native = fresh();
    let mut reference = fresh();
    let mut backend = tool_reference::StrokeEngine::default();
    native.parameters.radius = 6.;
    reference.parameters.radius = 6.;
    let samples: Vec<_> = (0..80u32)
        .flat_map(|stroke| {
            (0..64u32).map(move |i| {
                let phase = if i == 0 {
                    PointerPhase::Down
                } else if i == 63 {
                    PointerPhase::Up
                } else {
                    PointerPhase::Move
                };
                point(
                    phase,
                    40. + i as f32 * 6.,
                    60. + ((stroke * 17) % 250) as f32 + (i as f32 * 0.2).sin() * 15.,
                    0.1 + ((i * 7) % 29) as f32 / 32.,
                    (stroke * 64 + i) * 4,
                    true,
                )
            })
        })
        .collect();
    // Interleave repeats to reduce warm-cache and scheduling bias.
    let mut native_times = vec![];
    let mut reference_times = vec![];
    for _ in 0..5 {
        let start = Instant::now();
        for p in &samples {
            reference.pointer_reference(&mut backend, *p).unwrap();
        }
        reference_times.push(start.elapsed().as_secs_f64());
        let start = Instant::now();
        for p in &samples {
            native.pointer(*p).unwrap();
        }
        native_times.push(start.elapsed().as_secs_f64());
    }
    assert_eq!(native.doc.layers[0].pixels, reference.doc.layers[0].pixels);
    native_times.sort_by(f64::total_cmp);
    reference_times.sort_by(f64::total_cmp);
    println!(
        "DLL {:.3} ms; reference {:.3} ms; ratio {:.3}; samples per run {}",
        native_times[2] * 1000.,
        reference_times[2] * 1000.,
        native_times[2] / reference_times[2],
        samples.len()
    );
}
