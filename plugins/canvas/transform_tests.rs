//! Desktop transform drafts must not publish provisional document outcomes.
use super::*;
use crate::desktop::tests::{frame, studio};
use serde_json::json;
fn app() -> Studio {
    let mut app = studio();
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    app.editor
        .core("canvas.new", json!({"width":32,"height":24}));
    let mut pixels = vec![0; 32 * 24];
    for y in 4..16 {
        for x in 4..20 {
            pixels[y * 32 + x] = 0xffaa77ff;
        }
    }
    app.editor.session.doc.replace_region(0, 0, 32, 24, &pixels);
    app.editor.session.replay_boundary().unwrap();
    app.editor.sync_from_session();
    app
}
fn locate(shape: &Shape, text: &str) -> Option<Pos2> {
    match shape {
        Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
        Shape::Vec(v) => v.iter().find_map(|s| locate(s, text)),
        _ => None,
    }
}
fn click(app: &mut Studio, ctx: &Context, p: Pos2, time: f64) {
    for (n, pressed) in [true, false].into_iter().enumerate() {
        frame(
            app,
            ctx,
            vec2(1360., 920.),
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            time + n as f64 * 0.01,
        );
    }
}
#[test]
fn transform_cached_pixel_preview_cancel_apply_and_undo_leave_one_verified_outcome() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], n as f64);
    }
    app.editor.open_transform();
    let original = app.editor.session.doc.layers[0].pixels.clone();
    let composition = app.editor.session.doc.composition();
    app.editor.transform.as_mut().unwrap().params.translate_x = 3.;
    frame(&mut app, &ctx, size, vec![], 3.);
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    app.editor.transform.as_mut().unwrap().changed = Instant::now() - Duration::from_millis(200);
    frame(&mut app, &ctx, size, vec![], 3.1);
    assert!(app.editor.transform_texture().is_some());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
    assert_eq!(app.editor.session.doc.composition(), composition);
    assert_eq!(
        app.editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len(),
        count
    );
    let id = app.editor.transform_texture().unwrap().id();
    frame(&mut app, &ctx, size, vec![], 3.2);
    assert_eq!(app.editor.transform_texture().unwrap().id(), id);
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    assert!(app.editor.transform.is_none());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
    app.editor.open_transform();
    app.editor.transform.as_mut().unwrap().params.translate_x = 3.;
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], 4. + n as f64 * 0.1);
    }
    let out = frame(&mut app, &ctx, size, vec![], 4.4);
    let p = out
        .shapes
        .iter()
        .find_map(|s| locate(&s.shape, "Apply"))
        .unwrap();
    click(&mut app, &ctx, p, 4.5);
    assert!(app.editor.transform.is_none());
    assert_eq!(
        app.editor.session.doc.layers[0].pixels[6 * 32 + 9],
        0xffaa77ff
    );
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
    assert_eq!(
        app.editor
            .session
            .apply(crate::application::Action::new("replay.verify", json!({})))
            .unwrap()["result"]["valid"],
        true
    );
}
#[test]
fn transform_mouse_drag_does_not_move_pixels_and_escape_discards_the_draft() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], n as f64);
    }
    app.editor.open_transform();
    frame(&mut app, &ctx, size, vec![], 3.);
    let original = app.editor.session.doc.layers[0].pixels.clone();
    let p = app.editor.image_rect.min + vec2(10., 10.) * app.editor.zoom;
    let end = p + vec2(3., 2.) * app.editor.zoom;
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        4.,
    );
    assert!(app.editor.transform.as_ref().unwrap().drag.is_some());
    frame(&mut app, &ctx, size, vec![Event::PointerMoved(end)], 4.1);
    assert_eq!(
        app.editor.transform.as_ref().unwrap().params.translate_x,
        3.
    );
    assert_eq!(
        app.editor.transform.as_ref().unwrap().params.translate_y,
        2.
    );
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
    frame(
        &mut app,
        &ctx,
        size,
        vec![Event::PointerButton {
            pos: end,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        4.2,
    );
    assert!(app.editor.transform.as_ref().unwrap().drag.is_none());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
    frame(
        &mut app,
        &ctx,
        size,
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        4.3,
    );
    assert!(app.editor.transform.is_none());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
}
#[test]
fn transform_source_change_hides_cached_pixels_and_prevents_apply() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    app.editor.open_transform();
    app.editor.transform.as_mut().unwrap().params.translate_x = 3.;
    frame(&mut app, &ctx, size, vec![], 0.);
    app.editor.transform.as_mut().unwrap().changed = Instant::now() - Duration::from_millis(200);
    frame(&mut app, &ctx, size, vec![], 1.);
    assert!(app.editor.transform_texture().is_some());
    app.editor
        .core("layer.fill_color", json!({"color":"#abcdef"}));
    let original = app.editor.session.doc.layers[0].pixels.clone();
    assert!(app.editor.transform_texture().is_none());
    let out = frame(&mut app, &ctx, size, vec![], 2.);
    assert!(out.shapes.iter().any(|s| locate(
        &s.shape,
        "Document changed; reopen Free Transform."
    )
    .is_some()));
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
}
#[test]
fn transform_corner_and_rotation_handles_are_reversible_parameter_gestures() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], n as f64);
    }
    app.editor.open_transform();
    frame(&mut app, &ctx, size, vec![], 3.);
    let original = app.editor.session.doc.layers[0].pixels.clone();
    let d = app.editor.transform.as_ref().unwrap();
    let q = quad(d.params, d.bounds).unwrap();
    let p = app.editor.image_rect.min + q[2].to_vec2() * app.editor.zoom;
    let center =
        app.editor.image_rect.min + (q[0] + (q[2] - q[0]) * 0.5).to_vec2() * app.editor.zoom;
    let end = center + (p - center) * 0.5;
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        4.,
    );
    assert!(matches!(
        app.editor
            .transform
            .as_ref()
            .unwrap()
            .drag
            .as_ref()
            .map(|d| d.handle),
        Some(Handle::Corner(2))
    ));
    frame(&mut app, &ctx, size, vec![Event::PointerMoved(end)], 4.1);
    assert!((app.editor.transform.as_ref().unwrap().params.scale_x - 0.5).abs() < 0.001);
    assert!((app.editor.transform.as_ref().unwrap().params.scale_y - 0.5).abs() < 0.001);
    frame(&mut app, &ctx, size, vec![Event::WindowFocused(false)], 4.2);
    assert_eq!(app.editor.transform.as_ref().unwrap().params.scale_x, 1.);
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerButton {
                pos: end,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
            Event::WindowFocused(true),
        ],
        4.3,
    );
    let r = app
        .editor
        .transform
        .as_ref()
        .unwrap()
        .rotate_point(app.editor.zoom, app.editor.settings.ui.icon_size)
        .unwrap();
    let p = app.editor.image_rect.min + r.to_vec2() * app.editor.zoom;
    let end = center + vec2((p - center).length(), 0.);
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        5.,
    );
    assert!(matches!(
        app.editor
            .transform
            .as_ref()
            .unwrap()
            .drag
            .as_ref()
            .map(|d| d.handle),
        Some(Handle::Rotate)
    ));
    frame(&mut app, &ctx, size, vec![Event::PointerMoved(end)], 5.1);
    assert!((app.editor.transform.as_ref().unwrap().params.angle - 90.).abs() < 0.001);
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
}
#[test]
fn transform_pen_preview_and_cancel_do_not_enter_the_painting_or_event_path() {
    use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    for n in 0..3 {
        frame(&mut app, &ctx, size, vec![], n as f64);
    }
    app.editor.open_transform();
    frame(&mut app, &ctx, size, vec![], 3.);
    let original = app.editor.session.doc.layers[0].pixels.clone();
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    let p = app.editor.image_rect.min + vec2(10., 10.) * app.editor.zoom;
    let end = p + vec2(3., 2.) * app.editor.zoom;
    let ppp = ctx.pixels_per_point();
    for (n, (phase, p)) in [
        (PenPhase::Down, p),
        (PenPhase::Move, end),
        (PenPhase::Up, end),
    ]
    .into_iter()
    .enumerate()
    {
        app.editor.packets = vec![PenSample {
            phase,
            x: p.x * ppp,
            y: p.y * ppp,
            pressure: 0.3,
            eraser: false,
            time: 4000 + n as u32 * 100,
        }];
        frame(&mut app, &ctx, size, vec![], 4. + n as f64 * 0.1);
    }
    assert_eq!(
        app.editor.transform.as_ref().unwrap().params.translate_x,
        3.
    );
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
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    assert!(app.editor.transform.is_none());
    assert_eq!(app.editor.session.doc.layers[0].pixels, original);
}
#[test]
fn registered_preview_configures_an_existing_draft_without_invalidating_its_source() {
    let mut app = app();
    let ctx = Context::default();
    app.editor.open_transform();
    let revision = app.editor.session.revision;
    assert!(app.editor.core(
        "canvas.transform.configure",
        json!({"parameters":{"angle":20,"scale_x":0.8,"scale_y":0.8}})
    ));
    assert_eq!(app.editor.session.revision, revision);
    app.consume_ui_effects(&ctx);
    let d = app.editor.transform.as_ref().unwrap();
    assert!(d.valid(&app.editor.session));
    assert_eq!(d.params.angle, 20.);
    frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
    app.editor.transform.as_mut().unwrap().changed = Instant::now() - Duration::from_millis(200);
    frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.2);
    assert!(app.editor.transform_texture().is_some());
}
#[test]
fn relocating_or_recentering_the_pivot_preserves_the_current_forward_mapping() {
    let bounds = [0, 0, 32, 24];
    let p = Parameters {
        angle: 35.,
        skew_x: 18.,
        skew_y: -22.,
        scale_x: 0.8,
        scale_y: 1.2,
        translate_x: 4.,
        flip_x: true,
        ..Default::default()
    };
    let before = p.matrix(bounds).unwrap().0;
    let next = relocate_pivot(p, bounds, Some([3., 7.])).unwrap();
    let after = next.matrix(bounds).unwrap().0;
    for (a, b) in before.into_iter().zip(after) {
        assert!((a - b).abs() < 1e-10);
    }
    let reset = relocate_pivot(next, bounds, None).unwrap();
    let after = reset.matrix(bounds).unwrap().0;
    assert!(reset.pivot_x.is_none());
    for (a, b) in before.into_iter().zip(after) {
        assert!((a - b).abs() < 1e-10);
    }
    assert!(relocate_pivot(p, bounds, Some([8193., 7.])).is_none());
}
#[test]
fn transform_dialog_stays_inside_narrow_short_and_resized_viewports() {
    let mut app = app();
    let ctx = Context::default();
    app.editor.open_transform();
    for size in [vec2(1360., 920.), vec2(700., 500.), vec2(1700., 1000.)] {
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| app.editor.transform_window(ctx),
            ));
        }
        let output = output.unwrap();
        for label in ["Apply", "Cancel"] {
            let p = output
                .shapes
                .iter()
                .find_map(|s| locate(&s.shape, label))
                .unwrap_or_else(|| panic!("{label} is hidden at {size:?}"));
            assert!(p.x >= 0. && p.y >= 0. && p.x <= size.x && p.y <= size.y);
        }
        let rect = ctx
            .memory(|m| m.area_rect(Id::new("free-transform")))
            .unwrap();
        assert!(
            rect.min.x >= -1.
                && rect.min.y >= -1.
                && rect.max.x <= size.x + 1.
                && rect.max.y <= size.y + 1.,
            "Dialog {rect:?} exceeds {size:?}"
        );
    }
}
#[test]
fn transform_invalid_target_can_switch_back_to_cached_valid_layer_bounds() {
    let mut app = app();
    let ctx = Context::default();
    let size = vec2(1360., 920.);
    app.editor.open_transform();
    frame(&mut app, &ctx, size, vec![], 0.);
    app.editor.transform.as_mut().unwrap().params.target = Some(MoveTarget::SelectionMask);
    frame(&mut app, &ctx, size, vec![], 1.);
    assert!(!app.editor.transform.as_ref().unwrap().bounds_valid);
    app.editor.transform.as_mut().unwrap().params.target = Some(MoveTarget::Layer);
    frame(&mut app, &ctx, size, vec![], 2.);
    assert!(app.editor.transform.as_ref().unwrap().bounds_valid);
    assert!(app.editor.transform.as_ref().unwrap().error.is_empty());
}

#[test]
fn skew_edge_projection_and_corner_inverse_respect_rotations_flips_and_custom_pivots() {
    let bounds = [4, 4, 20, 16];
    let p = Parameters {
        angle: 33.,
        skew_x: 20.,
        skew_y: -18.,
        scale_x: 1.3,
        scale_y: 0.7,
        flip_x: true,
        pivot_x: Some(8.),
        pivot_y: Some(7.),
        ..Default::default()
    };
    let q = quad(p, bounds).unwrap();
    for i in 0..4 {
        let mut wanted = p;
        if i % 2 == 0 {
            wanted.skew_x += 5.;
        } else {
            wanted.skew_y += 7.;
        }
        let delta = edge(quad(wanted, bounds).unwrap(), i) - edge(q, i);
        let result = skew_drag(p, bounds, i, delta, false).unwrap();
        assert!((result.skew_x - wanted.skew_x).abs() < 1e-4, "edge {i}");
        assert!((result.skew_y - wanted.skew_y).abs() < 1e-4, "edge {i}");
        let snap = skew_drag(p, bounds, i, delta, true).unwrap();
        let angle = if i % 2 == 0 { snap.skew_x } else { snap.skew_y };
        assert!((angle / 5. - (angle / 5.).round()).abs() < 1e-10);
    }
    let (m, pivot) = p.matrix(bounds).unwrap();
    for point in [[4., 4.], [20., 16.]] {
        let v = [
            m[0] * (point[0] - pivot[0]) + m[1] * (point[1] - pivot[1]),
            m[3] * (point[0] - pivot[0]) + m[4] * (point[1] - pivot[1]),
        ];
        let local = p.unskew(v);
        assert!((local[0] + p.scale_x * (point[0] - pivot[0])).abs() < 1e-10);
        assert!((local[1] - p.scale_y * (point[1] - pivot[1])).abs() < 1e-10);
    }
}

#[test]
fn skew_mouse_and_pen_edge_handles_only_publish_after_apply() {
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
            frame(&mut app, &ctx, size, vec![], 3.);
            let d = app.editor.transform.as_ref().unwrap();
            let original = app.editor.session.doc.layers[0].pixels.clone();
            let count = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            let start = edge(quad(d.params, d.bounds).unwrap(), i);
            let mut wanted = d.params;
            if i % 2 == 0 {
                wanted.skew_x = 20.;
            } else {
                wanted.skew_y = 20.;
            }
            let end = edge(quad(wanted, d.bounds).unwrap(), i);
            let screen = |p: Pos2| app.editor.image_rect.min + p.to_vec2() * app.editor.zoom;
            let (start, end) = (screen(start), screen(end));
            let out = frame(&mut app, &ctx, size, vec![Event::PointerMoved(start)], 3.1);
            assert_eq!(out.platform_output.cursor_icon, CursorIcon::Move);
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
                if phase == 0 {
                    assert!(
                        matches!(app.editor.transform.as_ref().unwrap().drag.as_ref().map(|d|d.handle),Some(Handle::Skew(n)) if n==i)
                    );
                }
            }
            let params = app.editor.transform.as_ref().unwrap().params;
            assert!((params.skew_x - wanted.skew_x).abs() < 1e-4);
            assert!((params.skew_y - wanted.skew_y).abs() < 1e-4);
            assert_eq!(app.editor.session.doc.layers[0].pixels, original);
            if !pen && i == 1 {
                let d = app.editor.transform.as_ref().unwrap();
                let q = quad(d.params, d.bounds).unwrap();
                let center = q[0] + (q[2] - q[0]) * 0.5;
                let start = app.editor.image_rect.min + q[2].to_vec2() * app.editor.zoom;
                let end = app.editor.image_rect.min
                    + (center + (q[2] - center) * 0.8).to_vec2() * app.editor.zoom;
                for (phase, p) in [(0, start), (1, end), (2, end)] {
                    let event = if phase == 1 {
                        Event::PointerMoved(p)
                    } else {
                        Event::PointerButton {
                            pos: p,
                            button: PointerButton::Primary,
                            pressed: phase == 0,
                            modifiers: Modifiers::NONE,
                        }
                    };
                    frame(&mut app, &ctx, size, vec![event], 4.1 + phase as f64 * 0.02);
                }
                let p = app.editor.transform.as_ref().unwrap().params;
                assert!((p.scale_x - 0.8).abs() < 1e-4 && (p.scale_y - 0.8).abs() < 1e-4);
                assert!((p.skew_y - 20.).abs() < 1e-4);
            }
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
