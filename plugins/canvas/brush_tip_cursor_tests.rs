//! The actual Canvas outline follows nominal tip geometry and pressure in pixels.
use super::*;
use crate::application::TipShape;
use crate::desktop::tests::studio;
use crate::platform::windows::desktop_bridge::PenSample;
use serde_json::json;

fn fixture() -> Studio {
    let mut app = studio();
    app.editor.settings.ui.language = crate::i18n::Language::English;
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    assert!(app
        .editor
        .core("canvas.new", json!({"width":12,"height":8})));
    app.editor.sync_from_session();
    app.editor.fit = false;
    app.editor.zoom = 8.;
    app.editor.pan = Vec2::ZERO;
    app.editor.push_gui_view();
    app
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
}
fn frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640., 480.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.canvas_ui(ui));
        },
    )
}
fn paths(output: &FullOutput) -> Vec<Vec<Pos2>> {
    fn visit(shape: &Shape, out: &mut Vec<Vec<Pos2>>) {
        match shape {
            Shape::Path(path)
                if path.stroke.width == 2.5
                    && path.stroke.color
                        == egui::epaint::ColorMode::Solid(Color32::from_white_alpha(230)) =>
            {
                out.push(path.points.clone())
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        visit(&shape.shape, &mut out);
    }
    out
}
fn circles(output: &FullOutput) -> Vec<(Pos2, f32)> {
    fn visit(shape: &Shape, out: &mut Vec<(Pos2, f32)>) {
        match shape {
            Shape::Circle(circle)
                if circle.stroke.width == 2.5
                    && circle.stroke.color == Color32::from_white_alpha(230) =>
            {
                out.push((circle.center, circle.radius))
            }
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for shape in &output.shapes {
        visit(&shape.shape, &mut out);
    }
    out
}
fn pen(
    app: &mut Studio,
    ctx: &Context,
    phase: PenPhase,
    center: Pos2,
    pressure: f32,
    time: u32,
) -> FullOutput {
    let ppp = ctx.pixels_per_point();
    app.editor.packets = vec![PenSample {
        phase,
        x: center.x * ppp,
        y: center.y * ppp,
        pressure,
        eraser: false,
        time,
    }];
    frame(app, ctx, vec![], time as f64 / 1000.)
}

#[test]
fn brush_tip_canvas_default_round_cursor_keeps_circle_path_and_clamps_before_zoom() {
    let mut app = fixture();
    let ctx = context(&app);
    assert!(app.editor.core("brush.set",json!({"radius":0.5,"pressure_size":true,"minimum_radius":0,"pressure_gamma":1,"stabilizer":0,"tip_shape":"round","tip_roundness":1,"tip_angle":137})));
    app.editor.sync_from_session();
    app.editor.fit = false;
    app.editor.zoom = 8.;
    for n in 0..3 {
        frame(&mut app, &ctx, vec![], n as f64);
    }
    let center = app.editor.image_rect.min + vec2(5.5, 4.5) * app.editor.zoom;
    let hover = frame(&mut app, &ctx, vec![Event::PointerMoved(center)], 3.);
    assert_eq!(circles(&hover), vec![(center, 4.)]);
    assert!(paths(&hover).is_empty());
    let down = pen(&mut app, &ctx, PenPhase::Down, center, 0.1, 4000);
    // Positive pressure scales0.5 to0.05, then native nominal major radius
    // remains0.5 document pixels; a screen-only clamp would incorrectly yield0.5.
    assert_eq!(circles(&down), vec![(center, 4.)]);
    assert!(paths(&down).is_empty());
    pen(&mut app, &ctx, PenPhase::Up, center, 0.1, 4100);
    assert!(!app.editor.session.stroke.active());
}

#[test]
fn brush_tip_canvas_rotated_rectangle_and_ellipse_follow_pressure_zoom_and_cached_axes() {
    for (shape, angle) in [(TipShape::Square, 90.), (TipShape::Round, -45.)] {
        let mut app = fixture();
        let ctx = context(&app);
        assert!(app.editor.core("brush.set",json!({"radius":4,"pressure_size":true,"minimum_radius":0,"pressure_gamma":1,"stabilizer":0,"tip_shape":shape,"tip_roundness":0.25,"tip_angle":angle})));
        app.editor.sync_from_session();
        app.editor.fit = false;
        app.editor.zoom = 3.;
        for n in 0..3 {
            frame(&mut app, &ctx, vec![], n as f64);
        }
        let center = app.editor.image_rect.min + vec2(5.5, 4.5) * app.editor.zoom;
        let output = pen(&mut app, &ctx, PenPhase::Down, center, 0.5, 4000);
        let outlines = paths(&output);
        assert_eq!(outlines.len(), 1);
        assert!(circles(&output).is_empty());
        let (sin, cos) = (angle as f32).to_radians().sin_cos();
        let radius = 6.;
        for point in &outlines[0] {
            let d = (*point - center) / radius;
            let x = cos * d.x + sin * d.y;
            let y = -sin * d.x + cos * d.y;
            if shape == TipShape::Square {
                assert!((x.abs() - 1.).abs() < 1e-5);
                assert!((y.abs() - 0.25).abs() < 1e-5);
            } else {
                assert!((x * x + (y / 0.25).powi(2) - 1.).abs() < 1e-4);
            }
        }
        if shape == TipShape::Square {
            assert_eq!(outlines[0].len(), 4);
        } else {
            assert!(outlines[0].len() >= 32 && outlines[0].len() <= 256);
        }
        let stationary = frame(&mut app, &ctx, vec![], 4.05);
        assert_eq!(paths(&stationary), outlines);
        pen(&mut app, &ctx, PenPhase::Up, center, 0.5, 4100);
    }
}
