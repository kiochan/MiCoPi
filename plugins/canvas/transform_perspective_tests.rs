//! Perspective drafts share input routing and never publish intermediate pixel changes.
use super::*;
use crate::desktop::tests::frame;
use serde_json::json;
// Reuse the actual Canvas fixture, button hit lookup and frame helpers.
use super::tests::{app, click, locate};
#[test]
fn baking_affine_geometry_preserves_corners_and_preview_and_keeps_pixels_provisional() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], n as f64);
    }
    app.editor.open_transform();
    let p = Parameters {
        target: Some(MoveTarget::Layer),
        angle: 23.,
        skew_x: 16.,
        skew_y: -12.,
        scale_x: 0.8,
        scale_y: 0.9,
        translate_x: 2.,
        ..Default::default()
    };
    app.editor.transform.as_mut().unwrap().params = p;
    let source = app.editor.session.doc.layers[0].pixels.clone();
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], 3. + n as f64 * 0.1);
    }
    let out = frame(&mut app, &ctx, size, vec![], 3.3);
    let button = out
        .shapes
        .iter()
        .find_map(|s| locate(&s.shape, "Edit Perspective Corners"))
        .unwrap();
    click(&mut app, &ctx, button, 3.4);
    let d = app.editor.transform.as_ref().unwrap();
    let baked = d.params;
    assert!(baked.quad.is_some());
    let a = p.mapping(d.bounds).unwrap().0;
    let b = baked.mapping(d.bounds).unwrap().0;
    assert_eq!(b.denominator, [0., 0., 1.]);
    for (from, to) in a
        .corners(d.bounds)
        .unwrap()
        .into_iter()
        .zip(b.corners(d.bounds).unwrap())
    {
        assert!((from[0] - to[0]).abs() < 1e-8 && (from[1] - to[1]).abs() < 1e-8);
    }
    assert_eq!(
        affine::preview(&app.editor.session, p).unwrap().0,
        affine::preview(&app.editor.session, baked).unwrap().0
    );
    assert_eq!(app.editor.session.doc.layers[0].pixels, source);
    assert_eq!(
        app.editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len(),
        count
    );
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    assert!(app.editor.transform.is_none());
}
#[test]
fn perspective_edges_and_inside_move_only_the_intended_corners_and_reject_crossings() {
    let bounds = [4, 4, 20, 16];
    let q = [[4., 4.], [20., 4.], [18., 16.], [6., 16.]];
    let p = Parameters {
        quad: Some(q),
        ..Default::default()
    };
    for i in 0..4 {
        let n = perspective::drag(
            p,
            bounds,
            Handle::Skew(i),
            pos2(0., 0.),
            pos2(1., 2.),
            false,
        )
        .unwrap()
        .quad
        .unwrap();
        for j in 0..4 {
            let delta = if j == i || j == (i + 1) % 4 {
                [1., 2.]
            } else {
                [0., 0.]
            };
            assert_eq!(n[j], [q[j][0] + delta[0], q[j][1] + delta[1]]);
        }
    }
    let n = perspective::drag(p, bounds, Handle::Move, pos2(0., 0.), pos2(3., 1.), true)
        .unwrap()
        .quad
        .unwrap();
    for i in 0..4 {
        assert_eq!(n[i], [q[i][0] + 3., q[i][1]]);
    }
    assert!(perspective::drag(
        p,
        bounds,
        Handle::Corner(0),
        pos2(4., 4.),
        pos2(30., 20.),
        false
    )
    .is_none());
}
#[test]
fn perspective_mouse_and_pen_corners_preview_cancel_apply_and_undo() {
    use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
    let size = vec2(1360., 920.);
    for pen in [false, true] {
        for i in 0..4 {
            let mut app = app();
            let ctx = Context::default();
            for n in 0..3 {
                frame(&mut app, &ctx, size, vec![], n as f64);
            }
            app.editor.open_transform();
            let original = app.editor.session.doc.layers[0].pixels.clone();
            let q = [[4., 4.], [20., 4.], [18., 16.], [6., 16.]];
            app.editor.transform.as_mut().unwrap().params.quad = Some(q);
            frame(&mut app, &ctx, size, vec![], 3.);
            let count = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            let delta = vec2(
                if i == 0 || i == 3 { 1. } else { -1. },
                if i < 2 { 1. } else { -1. },
            );
            let start =
                app.editor.image_rect.min + vec2(q[i][0] as f32, q[i][1] as f32) * app.editor.zoom;
            let end = start + delta * app.editor.zoom;
            let out = frame(&mut app, &ctx, size, vec![Event::PointerMoved(start)], 3.1);
            assert_eq!(out.platform_output.cursor_icon, CursorIcon::Crosshair);
            for (n, (phase, p)) in [(0, start), (1, end), (2, end)].into_iter().enumerate() {
                let events = if pen {
                    let ppp = ctx.pixels_per_point();
                    app.editor.packets = vec![PenSample {
                        phase: match phase {
                            0 => PenPhase::Down,
                            1 => PenPhase::Move,
                            _ => PenPhase::Up,
                        },
                        x: p.x * ppp,
                        y: p.y * ppp,
                        pressure: 0.5,
                        eraser: false,
                        time: 4000 + n as u32 * 20,
                    }];
                    vec![]
                } else if phase == 1 {
                    vec![Event::PointerMoved(p)]
                } else {
                    vec![Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed: phase == 0,
                        modifiers: Modifiers::NONE,
                    }]
                };
                frame(&mut app, &ctx, size, events, 4. + n as f64 * 0.02);
            }
            let changed = app.editor.transform.as_ref().unwrap().params.quad.unwrap();
            for j in 0..4 {
                let d = if j == i { delta } else { Vec2::ZERO };
                assert!(
                    (changed[j][0] - q[j][0] - d.x as f64).abs() < 1e-5
                        && (changed[j][1] - q[j][1] - d.y as f64).abs() < 1e-5
                );
            }
            assert_eq!(app.editor.session.doc.layers[0].pixels, original);
            assert_eq!(
                app.editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len(),
                count
            );
            app.editor.transform.as_mut().unwrap().changed =
                Instant::now() - Duration::from_millis(200);
            let out = frame(&mut app, &ctx, size, vec![], 4.2);
            assert!(app.editor.transform_texture().is_some());
            if !pen && i == 0 {
                let button = out
                    .shapes
                    .iter()
                    .find_map(|s| locate(&s.shape, "Apply"))
                    .unwrap();
                click(&mut app, &ctx, button, 4.3);
                assert!(app.editor.transform.is_none());
                assert_eq!(
                    app.editor
                        .session
                        .replay_journal(false)
                        .unwrap()
                        .entries
                        .len(),
                    count + 1
                );
                app.editor.core("history.undo", json!({}));
                assert_eq!(app.editor.session.doc.layers[0].pixels, original);
            } else {
                app.editor.actions.push(Action::Cancel);
                app.run_actions(&ctx);
                assert!(app.editor.transform.is_none());
                assert_eq!(app.editor.session.doc.layers[0].pixels, original);
            }
        }
    }
}

#[test]
fn perspective_invalid_draft_can_reset_and_buttons_remain_visible_in_short_viewports() {
    let mut app = app();
    let ctx = Context::default();
    app.editor.open_transform();
    let original = app.editor.session.doc.layers[0].pixels.clone();
    app.editor.transform.as_mut().unwrap().params.quad =
        Some([[4., 4.], [20., 4.], [18., 16.], [6., 16.]]);
    for size in [vec2(1360., 920.), vec2(700., 500.), vec2(1700., 1000.)] {
        let mut out = None;
        for _ in 0..3 {
            out = Some(ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.editor.transform_window(ctx),
            ));
        }
        let out = out.unwrap();
        for label in ["Apply", "Cancel"] {
            let p = out
                .shapes
                .iter()
                .find_map(|s| locate(&s.shape, label))
                .unwrap_or_else(|| panic!("{label} clipped at {size:?}"));
            assert!(p.x >= 0. && p.y >= 0. && p.x <= size.x && p.y <= size.y);
        }
    }
    app.editor.transform.as_mut().unwrap().params.quad =
        Some([[0., 0.], [20., 20.], [20., 0.], [0., 20.]]);
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], 10. + n as f64 * 0.1);
    }
    let out = frame(&mut app, &ctx, size, vec![], 10.3);
    let reset = out
        .shapes
        .iter()
        .find_map(|s| locate(&s.shape, "Reset Transform"))
        .unwrap();
    click(&mut app, &ctx, reset, 10.4);
    assert!(app.editor.transform.as_ref().unwrap().params.quad.is_none());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
}
