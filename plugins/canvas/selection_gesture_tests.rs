use super::*;
use crate::application::Action as CoreAction;
use crate::desktop::tests::{frame, studio};
use serde_json::json;
fn fresh(mode: u8) -> (Studio, Context) {
    let mut app = studio();
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    app.editor
        .core("canvas.new", json!({"width":24,"height":20}));
    app.editor.sync_from_session();
    app.editor.area_mode = mode;
    let ctx = Context::default();
    for t in 0..3 {
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], t as f64);
    }
    (app, ctx)
}
fn mouse(app: &mut Studio, ctx: &Context, q: Pos2, pressed: bool, mods: Modifiers, t: f64) {
    let pos = app.editor.image_rect.min + q.to_vec2() * app.editor.zoom;
    let events = vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: mods,
        },
    ];
    let _ = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 920.))),
            focused: true,
            modifiers: mods,
            time: Some(t),
            events,
            ..Default::default()
        },
        |ctx| app.frame_ui(ctx),
    );
}
fn verify(app: &mut Studio) {
    let result = app
        .editor
        .session
        .apply(CoreAction::new("replay.verify", json!({})))
        .unwrap();
    assert_eq!(result["result"]["valid"], true);
}
#[test]
fn rectangle_drag_previews_without_mutation_and_commits_one_undoable_action() {
    let (mut app, ctx) = fresh(3);
    let before = app.editor.session.doc.composition();
    let history = app.editor.session.doc.history_bytes();
    mouse(&mut app, &ctx, pos2(4., 3.), true, Modifiers::NONE, 3.);
    let end = app.editor.image_rect.min + vec2(11., 9.) * app.editor.zoom;
    frame(
        &mut app,
        &ctx,
        vec2(1360., 920.),
        vec![Event::PointerMoved(end)],
        3.1,
    );
    assert_eq!(app.editor.session.doc.composition(), before);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
    let events = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    mouse(&mut app, &ctx, pos2(11., 9.), false, Modifiers::NONE, 3.2);
    assert_eq!(
        app.editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len(),
        events + 1
    );
    let mask = app.editor.session.doc.selection().current.as_ref().unwrap();
    assert_eq!(mask.coverage().iter().filter(|v| **v > 0).count(), 42);
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    verify(&mut app);
    app.editor.core("history.undo", json!({}));
    assert_eq!(app.editor.session.doc.composition(), before);
    app.editor.core("history.redo", json!({}));
    assert!(app.editor.session.doc.selection().current.is_some());
}
#[test]
fn alt_subtraction_and_shift_square_use_down_modifiers_without_eyedropper() {
    let (mut app, ctx) = fresh(3);
    app.editor.core("selection.all", json!({}));
    mouse(&mut app, &ctx, pos2(4., 3.), true, Modifiers::ALT, 3.);
    mouse(&mut app, &ctx, pos2(11., 9.), false, Modifiers::ALT, 3.1);
    assert_eq!(
        app.editor
            .session
            .doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v == 0)
            .count(),
        42
    );
    assert_eq!(app.editor.session.selected_tool, "brush");
    app.editor.selection_combine = "replace";
    assert!(app
        .editor
        .selection_down(pos2(4., 3.), Modifiers::NONE, 4000));
    app.editor.selection_move(pos2(9., 3.), true);
    app.editor.selection_release();
    assert_eq!(
        app.editor
            .session
            .doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v > 0)
            .count(),
        25
    );
    verify(&mut app);
}
#[test]
fn cancel_and_stale_document_discard_preview_without_changing_selection() {
    let (mut app, ctx) = fresh(4);
    let before = app.editor.session.doc.composition();
    mouse(&mut app, &ctx, pos2(4., 3.), true, Modifiers::NONE, 3.);
    frame(
        &mut app,
        &ctx,
        vec2(1360., 920.),
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        3.1,
    );
    assert!(app.editor.selection_gesture.is_none());
    mouse(&mut app, &ctx, pos2(11., 9.), false, Modifiers::NONE, 3.2);
    assert_eq!(app.editor.session.doc.composition(), before);
    mouse(&mut app, &ctx, pos2(4., 3.), true, Modifiers::NONE, 4.);
    app.editor
        .core("canvas.new", json!({"width":18,"height":18}));
    mouse(&mut app, &ctx, pos2(11., 9.), false, Modifiers::NONE, 4.1);
    assert!(app.editor.session.doc.selection().current.is_none());
    assert!(app.editor.selection_gesture.is_none());
}
#[test]
fn polygon_waits_across_clicks_and_enter_commits_without_raster_strokes() {
    let (mut app, ctx) = fresh(6);
    for (i, p) in [pos2(4., 3.), pos2(11., 3.), pos2(11., 9.), pos2(4., 9.)]
        .into_iter()
        .enumerate()
    {
        mouse(&mut app, &ctx, p, true, Modifiers::NONE, 3. + i as f64);
        mouse(&mut app, &ctx, p, false, Modifiers::NONE, 3.1 + i as f64);
        assert!(app.editor.polygon_pending());
        assert!(app.editor.session.doc.selection().current.is_none());
    }
    frame(
        &mut app,
        &ctx,
        vec2(1360., 920.),
        vec![Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        7.2,
    );
    assert!(!app.editor.polygon_pending());
    assert_eq!(
        app.editor
            .session
            .doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v > 0)
            .count(),
        42
    );
    assert!(app.editor.session.doc.layers[0]
        .pixels
        .iter()
        .all(|p| *p == 0));
    verify(&mut app);
}
#[test]
fn synthetic_pen_lasso_closes_and_dense_trace_remains_bounded() {
    use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
    let (mut app, ctx) = fresh(5);
    for (i, (phase, q)) in [
        (PenPhase::Down, pos2(4., 3.)),
        (PenPhase::Move, pos2(11., 3.)),
        (PenPhase::Move, pos2(11., 9.)),
        (PenPhase::Move, pos2(4., 9.)),
        (PenPhase::Up, pos2(4., 3.)),
    ]
    .into_iter()
    .enumerate()
    {
        let p =
            (app.editor.image_rect.min + q.to_vec2() * app.editor.zoom) * ctx.pixels_per_point();
        app.editor.packets.push(PenSample {
            phase,
            x: p.x,
            y: p.y,
            pressure: 0.3 + i as f32 * 0.1,
            eraser: false,
            time: 3000 + i as u32 * 20,
        });
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![],
            3. + i as f64 * 0.1,
        );
    }
    assert_eq!(
        app.editor
            .session
            .doc
            .selection()
            .current
            .as_ref()
            .unwrap()
            .coverage()
            .iter()
            .filter(|v| **v > 0)
            .count(),
        42
    );
    verify(&mut app);
    app.editor
        .selection_down(pos2(1., 1.), Modifiers::NONE, 5000);
    for i in 0..20000 {
        app.editor.selection_move(
            if i % 2 == 0 {
                pos2(20., 2.)
            } else {
                pos2(2., 18.)
            },
            false,
        );
    }
    assert!(app.editor.selection_gesture.as_ref().unwrap().points.len() < 4096);
    app.editor.finish_stroke();
}

#[test]
fn selection_tools_and_combination_controls_use_registered_ui_commands() {
    let (mut app, ctx) = fresh(0);
    app.editor.core("toolbox.mode.selection_ellipse", json!({}));
    frame(&mut app, &ctx, vec2(1360., 920.), vec![], 3.);
    assert_eq!(app.editor.area_mode, 4);
    app.editor.core("toolbox.selection.intersect", json!({}));
    frame(&mut app, &ctx, vec2(1360., 920.), vec![], 3.1);
    assert_eq!(app.editor.selection_combine, "intersect");
    for (i, pressed) in [true, false].into_iter().enumerate() {
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![Event::Key {
                key: Key::M,
                physical_key: Some(Key::M),
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            4. + i as f64 * 0.1,
        );
    }
    assert_eq!(app.editor.area_mode, 3);
    let mut headless =
        crate::application::Session::new(crate::plugins::Registry::bundled()).unwrap();
    assert_eq!(
        headless
            .apply(CoreAction::new(
                "toolbox.mode.selection_rectangle",
                json!({})
            ))
            .unwrap_err()
            .code,
        "ui_unavailable"
    );
    assert!(headless
        .apply(CoreAction::new(
            "selection.shape",
            json!({"kind":"rectangle","width":4,"height":4})
        ))
        .is_ok());
}

#[test]
fn ellipse_release_and_polygon_closure_match_document_geometry() {
    let (mut app, ctx) = fresh(4);
    mouse(&mut app, &ctx, pos2(4., 3.), true, Modifiers::NONE, 3.);
    mouse(&mut app, &ctx, pos2(11., 9.), false, Modifiers::NONE, 3.1);
    let mask = app
        .editor
        .session
        .doc
        .selection()
        .current
        .as_ref()
        .unwrap()
        .coverage();
    for y in 0..20 {
        for x in 0..24 {
            let (dx, dy) = ((x as f64 + 0.5 - 7.5) / 3.5, (y as f64 + 0.5 - 6.) / 3.);
            assert_eq!(
                mask[y * 24 + x],
                if dx * dx + dy * dy <= 1. { 255 } else { 0 }
            );
        }
    }
    for close_first in [true, false] {
        let (mut app, ctx) = fresh(6);
        for (i, p) in [pos2(4., 3.), pos2(11., 3.), pos2(11., 9.), pos2(4., 9.)]
            .into_iter()
            .enumerate()
        {
            mouse(&mut app, &ctx, p, true, Modifiers::NONE, 3. + i as f64);
            mouse(&mut app, &ctx, p, false, Modifiers::NONE, 3.1 + i as f64);
        }
        let q = if close_first {
            pos2(4., 3.)
        } else {
            pos2(4., 9.)
        };
        mouse(&mut app, &ctx, q, true, Modifiers::NONE, 6.2);
        mouse(&mut app, &ctx, q, false, Modifiers::NONE, 6.3);
        assert!(!app.editor.polygon_pending());
        assert_eq!(
            app.editor
                .session
                .doc
                .selection()
                .current
                .as_ref()
                .unwrap()
                .coverage()
                .iter()
                .filter(|v| **v > 0)
                .count(),
            42
        );
        verify(&mut app);
    }
}
