//! Actual Move input keeps masked owner admission and outline publication separate.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{fixture, Snapshot};
use crate::engine::raster_masks::PaintTarget;
use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
use serde_json::{json, Value};

fn raw(app: &Studio, id: u64) -> Vec<u8> {
    let doc = &app.editor.session.doc;
    let plane = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|i| plane.sample(i))
        .collect()
}
fn shifted<T: Copy>(input: &[T], dx: i32, dy: i32, fill: T) -> Vec<T> {
    (0..12 * 8)
        .map(|i| {
            let x = (i % 12) as i32 - dx;
            let y = (i / 12) as i32 - dy;
            if (0..12).contains(&x) && (0..8).contains(&y) {
                input[y as usize * 12 + x as usize]
            } else {
                fill
            }
        })
        .collect()
}
fn movement_fixture(linked: bool, target: &str) -> Studio {
    let (mut app, _) = fixture(1);
    app.editor.session.runtime = "desktop";
    assert!(app
        .editor
        .core("mask.fill", json!({"mask_id":1,"value":47})));
    assert!(app
        .editor
        .core("mask.set", json!({"mask_id":1,"linked":linked})));
    assert!(
        app.editor.core(
            "toolbox.movement.options",
            json!({"target":target,"step":1,"large_step":10})
        ),
        "{}",
        app.editor.status
    );
    app.editor.area_mode = 8;
    app.editor.fit = false;
    app.editor.zoom = 4.;
    app.editor.pan = Vec2::ZERO;
    app.editor.push_gui_view();
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
    app
}
fn selections(app: &mut Studio) {
    for x in [1, 5] {
        assert!(app.editor.core(
            "selection.shape",
            json!({
                "kind":"rectangle","x":x,"y":2,"width":3,"height":3
            })
        ));
    }
    assert!(app.editor.session.doc.selection().current.is_some());
    assert!(app.editor.session.doc.selection().previous.is_some());
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
}
fn canvas_frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) {
    let _ = ctx.run(
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
    );
}
fn layout(app: &mut Studio, ctx: &Context) {
    for t in 0..3 {
        canvas_frame(app, ctx, vec![], t as f64);
    }
}
fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}
fn pen(app: &mut Studio, ctx: &Context, phase: PenPhase, pos: Pos2, time: u32) {
    let ppp = ctx.pixels_per_point();
    app.editor.packets = vec![PenSample {
        phase,
        x: pos.x * ppp,
        y: pos.y * ppp,
        pressure: 0.7,
        eraser: false,
        time,
    }];
    canvas_frame(app, ctx, vec![], time as f64 / 1000.);
}
fn pointer(kind: ToolKind, phase: PointerPhase, time: u32, pressure: f32) -> Pointer {
    Pointer {
        phase,
        x: (time / 20) as f32,
        y: 4.,
        pressure,
        time,
        pen: true,
        eraser: false,
        view_x: Some(50.),
        view_y: Some(40.),
        tool_override: Some(kind),
    }
}
fn samples(app: &mut Studio, start: usize) -> Vec<Value> {
    let journal = app.editor.session.replay_journal(false).unwrap();
    let events = journal
        .events(start, journal.entries.len() - start)
        .unwrap();
    let mut result: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["source"]["kind"] == "pointer")
        .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
        .collect();
    result.extend(
        serde_json::to_value(&app.editor.session.replay_samples)
            .unwrap()
            .as_array()
            .unwrap()
            .clone(),
    );
    result
}

#[test]
fn masked_move_layer_gesture_publishes_linked_pair_once_and_retains_pixel_selections() {
    for linked in [false, true] {
        let mut app = movement_fixture(linked, "layer");
        selections(&mut app);
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let mask = raw(&app, 1);
        let selection = app.editor.session.doc.selection().clone();
        let metadata = app.editor.session.doc.mask_state().clone();
        let before = Snapshot::capture(&app.editor);
        let revision = app.editor.session.revision;
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        assert!(app.editor.movement_start(pos2(2., 3.)));
        assert!(
            matches!(app.editor.movement_gesture.as_ref().unwrap().destination,
            MoveDestination::Layer {layer_id:1, linked_mask_id} if linked_mask_id == linked.then_some(1))
        );
        app.editor.movement_move(pos2(5., 1.), false);
        before.assert_same(&app.editor);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        app.editor.movement_release();
        assert_eq!(
            app.editor.session.doc.layers[0].pixels,
            shifted(&pixels, 3, -2, 0)
        );
        assert_eq!(
            raw(&app, 1),
            if linked {
                shifted(&mask, 3, -2, 255)
            } else {
                mask.clone()
            }
        );
        assert_eq!(app.editor.session.doc.selection(), &selection);
        assert_eq!(app.editor.session.doc.mask_state(), &metadata);
        assert_eq!(app.editor.session.revision, revision + 1);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events + 1
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(raw(&app, 1), mask);
        assert_eq!(app.editor.session.doc.selection(), &selection);
        assert!(app.editor.core("history.redo", json!({})));
        assert_eq!(
            raw(&app, 1),
            if linked {
                shifted(&mask, 3, -2, 255)
            } else {
                mask
            }
        );
    }
}

#[test]
fn masked_move_auto_uses_selected_mask_owner_before_active_layer_and_pixel_selection() {
    let mut app = movement_fixture(true, "auto");
    assert!(app.editor.core("layer.add", json!({})));
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":1}})));
    selections(&mut app);
    let pixels: Vec<_> = app
        .editor
        .session
        .doc
        .layers
        .iter()
        .map(|l| l.pixels.clone())
        .collect();
    let selection = app.editor.session.doc.selection().clone();
    let mask = raw(&app, 1);
    assert!(app.editor.movement_start(pos2(2., 2.)));
    assert!(matches!(
        app.editor.movement_gesture.as_ref().unwrap().destination,
        MoveDestination::RasterMask {
            mask_id: 1,
            owner_layer_id: 1
        }
    ));
    app.editor.movement_move(pos2(4., 3.), false);
    app.editor.movement_release();
    assert_eq!(raw(&app, 1), shifted(&mask, 2, 1, 255));
    assert_eq!(
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
    assert_eq!(app.editor.session.doc.selection(), &selection);
    assert_eq!(
        app.editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active),
        2
    );
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        PaintTarget::Mask { id: 1 }
    );
    let current = raw(&app, 1);
    app.editor.actions.push(Action::UiCommand(10177));
    let ctx = context(&app);
    app.run_actions(&ctx);
    assert_eq!(raw(&app, 1), shifted(&current, 1, 0, 255));
    assert_eq!(app.editor.session.doc.selection(), &selection);
}

#[test]
fn masked_move_mouse_pen_and_static_nudge_preserve_existing_brush_hand_admission() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for route in ["mouse", "pen", "nudge"] {
                let mut app = movement_fixture(true, "auto");
                let ctx = context(&app);
                layout(&mut app, &ctx);
                let p = app.editor.image_rect.center();
                let start = app
                    .editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len();
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Down, 100, 0.3))
                    .unwrap();
                app.editor.sync_from_session();
                app.editor.input_owned = local;
                app.editor.pen_active = local;
                app.editor.pen_eraser = true;
                app.editor.pressure = 0.3;
                app.editor.panning = (local && kind == ToolKind::Hand).then_some(Pos2::ZERO);
                app.editor.session.doc.compose();
                let before = Snapshot::capture(&app.editor);
                let transcript = samples(&mut app, start);
                // These drafts must not be published before native admission.
                app.editor.radius += 2.;
                app.editor.pan += vec2(3., 2.);
                match route {
                    "mouse" => {
                        // A native contact suppresses mouse events by design;
                        // direct shared gesture admission still covers local pen ownership.
                        if local {
                            assert!(!app.editor.movement_start(pos2(4., 4.)));
                        } else {
                            canvas_frame(
                                &mut app,
                                &ctx,
                                vec![Event::PointerMoved(p), button(p, true)],
                                3.,
                            );
                        }
                    }
                    "pen" => pen(&mut app, &ctx, PenPhase::Down, p, 3000),
                    _ => {
                        app.editor.actions.push(Action::UiCommand(10177));
                        app.run_actions(&ctx);
                    }
                }
                assert!(app.editor.movement_gesture.is_none(), "{route}");
                assert!(
                    app.editor.status.contains("stroke_active"),
                    "{}: {}",
                    route,
                    app.editor.status
                );
                before.assert_same(&app.editor);
                assert!(app.editor.session.stroke.active());
                assert!(app.editor.session.pointer_checkpoint.is_some());
                assert_eq!(app.editor.input_owned, local);
                assert_eq!(app.editor.pen_active, local);
                assert_eq!(
                    app.editor.panning.is_some(),
                    local && kind == ToolKind::Hand
                );
                assert_eq!(app.editor.drawing, kind == ToolKind::Brush);
                if route != "nudge" {
                    assert!(app.editor.pen_eraser);
                }
                assert_eq!(samples(&mut app, start), transcript);
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Move, 120, 0.9))
                    .unwrap();
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Up, 140, 0.4))
                    .unwrap();
                let actual = samples(&mut app, start);
                assert_eq!(actual.len(), 3);
                for (sample, (time, pressure)) in
                    actual.iter().zip([(100, 0.3), (120, 0.9), (140, 0.4)])
                {
                    assert_eq!(sample["time"], time);
                    assert!((sample["pressure"].as_f64().unwrap() - pressure).abs() < 1e-6);
                    assert_eq!(sample["pen"], true);
                }
            }
        }
    }
}

#[test]
fn masked_move_native_outline_cancel_release_and_stale_revision_are_pixel_atomic() {
    let mut app = movement_fixture(true, "auto");
    let ctx = context(&app);
    layout(&mut app, &ctx);
    let p = app.editor.image_rect.min + vec2(4., 3.) * app.editor.zoom;
    let end = p + vec2(2., 1.) * app.editor.zoom;
    let before = Snapshot::capture(&app.editor);
    pen(&mut app, &ctx, PenPhase::Down, p, 3000);
    pen(&mut app, &ctx, PenPhase::Move, end, 3100);
    assert!(app.editor.movement_gesture.is_some());
    before.assert_same(&app.editor);
    pen(&mut app, &ctx, PenPhase::Cancel, end, 3200);
    assert!(app.editor.movement_gesture.is_none());
    before.assert_same(&app.editor);
    pen(&mut app, &ctx, PenPhase::Down, p, 3300);
    pen(&mut app, &ctx, PenPhase::Move, end, 3400);
    assert!(app
        .editor
        .core("layer.rename", json!({"layer_id":1,"name":"Ink"})));
    let stale = Snapshot::capture(&app.editor);
    pen(&mut app, &ctx, PenPhase::Up, end, 3500);
    assert_eq!(app.editor.status, "Document changed; movement cancelled");
    stale.assert_same(&app.editor);
    let mask = raw(&app, 1);
    let revision = app.editor.session.revision;
    pen(&mut app, &ctx, PenPhase::Down, p, 3600);
    pen(&mut app, &ctx, PenPhase::Move, end, 3700);
    pen(&mut app, &ctx, PenPhase::Up, end, 3800);
    assert_eq!(raw(&app, 1), shifted(&mask, 2, 1, 255));
    assert_eq!(app.editor.session.revision, revision + 1);
    assert!(!app.editor.pen_active);
}

#[test]
fn masked_move_outline_cancel_and_release_never_end_a_later_external_native_contact() {
    for route in ["cancel", "pen_up", "mouse_up", "focus"] {
        let mut app = movement_fixture(true, "auto");
        let ctx = context(&app);
        if route == "focus" {
            // Settle real docking/window publication and pending UI effects
            // before the document/contact snapshot, as in an open Studio.
            for time in [-3., -2., -1.] {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 920.))),
                        time: Some(time),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| app.frame_ui(ctx),
                );
            }
        }
        layout(&mut app, &ctx);
        let p = app.editor.image_rect.center();
        pen(&mut app, &ctx, PenPhase::Down, p, 3000);
        pen(&mut app, &ctx, PenPhase::Move, p + vec2(8., 4.), 3100);
        assert!(app.editor.masked_movement_pending());
        let start = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        app.editor
            .session
            .pointer(pointer(ToolKind::Brush, PointerPhase::Down, 100, 0.3))
            .unwrap();
        app.editor.sync_from_session();
        app.editor.session.doc.compose();
        let before = Snapshot::capture(&app.editor);
        let transcript = samples(&mut app, start);
        match route {
            "cancel" => pen(&mut app, &ctx, PenPhase::Cancel, p, 3200),
            "pen_up" => pen(&mut app, &ctx, PenPhase::Up, p, 3200),
            "mouse_up" => {
                app.editor.pen_active = false;
                canvas_frame(&mut app, &ctx, vec![button(p, false)], 3.2);
            }
            _ => {
                // Focus cancellation belongs to the root/native viewport
                // lifecycle, outside Canvas's mouse-event gate during pen input.
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 920.))),
                        events: vec![Event::WindowFocused(false)],
                        time: Some(3.2),
                        focused: false,
                        ..Default::default()
                    },
                    |ctx| app.frame_ui(ctx),
                );
            }
        }
        assert!(app.editor.movement_gesture.is_none(), "{route}");
        assert!(!app.editor.input_owned, "{route}");
        assert!(app.editor.session.stroke.active(), "{route}");
        assert!(app.editor.session.pointer_checkpoint.is_some(), "{route}");
        assert!(app.editor.drawing, "{route}");
        before.assert_same(&app.editor);
        assert_eq!(samples(&mut app, start), transcript, "{route}");
        app.editor
            .session
            .pointer(pointer(ToolKind::Brush, PointerPhase::Move, 120, 0.9))
            .unwrap();
        app.editor
            .session
            .pointer(pointer(ToolKind::Brush, PointerPhase::Up, 140, 0.4))
            .unwrap();
        let actual = samples(&mut app, start);
        assert_eq!(actual.len(), 3, "{route}");
        assert_eq!(
            actual
                .iter()
                .map(|sample| sample["time"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [100, 120, 140]
        );
    }
}

fn temporary_hand_takeover(native: bool, outline: bool) {
    let mut app = movement_fixture(true, "auto");
    assert!(app.editor.core(
        "brush.set",
        json!({
            "radius":2.,"color":"#000000","opacity":1.,"stabilizer":0.,
            "pressure_size":false,"pressure_opacity":false
        })
    ));
    app.editor.sync_from_session();
    let ctx = context(&app);
    layout(&mut app, &ctx);
    let p = app.editor.image_rect.center();
    if outline {
        if native {
            pen(&mut app, &ctx, PenPhase::Down, p, 3000);
        } else {
            canvas_frame(
                &mut app,
                &ctx,
                vec![Event::PointerMoved(p), button(p, true)],
                3.,
            );
        }
    }
    assert_eq!(app.editor.masked_movement_pending(), outline);
    let original = raw(&app, 1);
    let start = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    app.editor
        .session
        .pointer(pointer(ToolKind::Brush, PointerPhase::Down, 100, 0.3))
        .unwrap();
    let painted = raw(&app, 1);
    assert_ne!(
        painted, original,
        "The external contact must have provisional raw paint"
    );
    app.editor.sync_from_session();
    let external = samples(&mut app, start);
    assert_eq!(external.len(), 1);
    let space = |pressed| Event::Key {
        key: Key::Space,
        physical_key: Some(Key::Space),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    if native {
        let ppp = ctx.pixels_per_point();
        app.editor.packets = vec![PenSample {
            phase: PenPhase::Down,
            x: p.x * ppp,
            y: p.y * ppp,
            pressure: 0.7,
            eraser: false,
            time: 4000,
        }];
        canvas_frame(&mut app, &ctx, vec![space(true)], 4.);
    } else {
        canvas_frame(
            &mut app,
            &ctx,
            vec![space(true), Event::PointerMoved(p), button(p, true)],
            4.,
        );
    }
    assert_eq!(
        raw(&app, 1),
        painted,
        "Temporary Hand must publish, not roll back, external paint"
    );
    assert!(app.editor.movement_gesture.is_none());
    assert!(app.editor.session.is_panning(), "{}", app.editor.status);
    assert!(app.editor.session.stroke.active());
    assert!(app.editor.session.pointer_checkpoint.is_some());
    assert!(app.editor.input_owned);
    assert!(app.editor.panning.is_some());
    assert!(!app.editor.drawing);
    assert_eq!(app.editor.pen_active, native);
    let mut old_up = external[0].clone();
    old_up["phase"] = json!("up");
    let takeover = samples(&mut app, start);
    assert_eq!(takeover.len(), 3, "{takeover:?}");
    assert_eq!(&takeover[..2], &[external[0].clone(), old_up]);
    assert_eq!(takeover[2]["phase"], "down");
    assert_eq!(takeover[2]["time"], 4000);
    assert_eq!(takeover[2]["tool_override"], "hand");
    let end = p + vec2(8., 4.);
    if native {
        pen(&mut app, &ctx, PenPhase::Move, end, 4100);
        pen(&mut app, &ctx, PenPhase::Up, end, 4200);
    } else {
        canvas_frame(&mut app, &ctx, vec![Event::PointerMoved(end)], 4.1);
        canvas_frame(&mut app, &ctx, vec![button(end, false), space(false)], 4.2);
    }
    assert!(!app.editor.session.stroke.active());
    assert!(app.editor.session.pointer_checkpoint.is_none());
    assert!(!app.editor.input_owned);
    assert!(app.editor.panning.is_none());
    assert_eq!(raw(&app, 1), painted);
    let completed = samples(&mut app, start);
    assert_eq!(&completed[..3], takeover.as_slice());
    assert_eq!(completed.last().unwrap()["phase"], "up");
    assert!(
        app.editor
            .session
            .apply(crate::application::Action::new("replay.verify", json!({})))
            .unwrap()["result"]["valid"]
            == true
    );
}

#[test]
fn masked_move_mouse_temporary_hand_takeover_commits_external_paint_before_new_input() {
    for outline in [true, false] {
        temporary_hand_takeover(false, outline);
    }
}

#[test]
fn masked_move_native_temporary_hand_takeover_commits_external_paint_before_new_input() {
    for outline in [true, false] {
        temporary_hand_takeover(true, outline);
    }
}

#[test]
fn masked_move_unsupported_group_selection_and_owner_lock_fail_without_a_gesture() {
    let (mut app, ids) = fixture(2);
    app.editor.session.runtime = "desktop";
    app.editor.area_mode = 8;
    for target in [
        json!({"kind":"mask","id":ids[1]}),
        json!({"kind":"mask","id":ids[0]}),
    ] {
        assert!(app.editor.core("mask.target", json!({"target":target})));
        if target["id"] == ids[0] {
            assert!(app
                .editor
                .core("layer.set", json!({"layer_id":1,"position_locked":true})));
        }
        let before = Snapshot::capture(&app.editor);
        assert!(!app.editor.movement_start(pos2(2., 2.)));
        assert!(app.editor.movement_gesture.is_none());
        before.assert_same(&app.editor);
    }
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":1,"position_locked":false})));
    selections(&mut app);
    assert!(
        app.editor.core(
            "toolbox.movement.options",
            json!({"target":"selection","step":1,"large_step":10})
        ),
        "{}",
        app.editor.status
    );
    let before = Snapshot::capture(&app.editor);
    assert!(!app.editor.movement_start(pos2(2., 2.)));
    assert!(app.editor.movement_gesture.is_none());
    before.assert_same(&app.editor);
}

#[test]
fn masked_move_options_show_resolved_owner_and_link_policy_in_four_languages_and_widths() {
    crate::i18n::load_test_catalogs();
    let mut app = movement_fixture(true, "auto");
    assert!(app
        .editor
        .core("layer.rename", json!({"layer_id":1,"name":"Ink / 墨"})));
    let ctx = context(&app);
    fn labels(shape: &Shape, result: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(text) => result.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
            )),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    labels(shape, result);
                }
            }
            _ => {}
        }
    }
    for language in crate::i18n::Language::ALL {
        crate::i18n::set_language(language);
        for linked in [false, true] {
            assert!(app
                .editor
                .core("mask.set", json!({"mask_id":1,"linked":linked})));
            for target in ["auto", "layer"] {
                assert!(
                    app.editor.core(
                        "toolbox.movement.options",
                        json!({"target":target,"step":1,"large_step":10})
                    ),
                    "{}",
                    app.editor.status
                );
                for width in [244., 520.] {
                    let before = Snapshot::capture(&app.editor);
                    let mut actual = Vec::new();
                    for _ in 0..3 {
                        let out = ctx.run(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(width, 900.),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                CentralPanel::default()
                                    .show(ctx, |ui| app.editor.movement_options_ui(ui));
                            },
                        );
                        actual.clear();
                        for shape in out.shapes {
                            labels(&shape.shape, &mut actual);
                        }
                    }
                    let owner = if target == "auto" {
                        "Raster mask"
                    } else if linked {
                        "Layer and linked mask"
                    } else {
                        "Layer pixels"
                    };
                    assert!(
                        actual
                            .iter()
                            .any(|(text, _)| text == &format!("{}: Ink / 墨", localize(owner))),
                        "{} {target} {width}: {actual:?}",
                        language.code()
                    );
                    let note = if target == "auto" {
                        "Mask only; layer pixels stay fixed."
                    } else if linked {
                        "Exposed mask areas reveal the layer."
                    } else {
                        "Unlinked mask stays fixed."
                    };
                    assert!(
                        actual
                            .iter()
                            .any(|(text, _)| text.as_str() == localize(note).as_ref()),
                        "{}: {actual:?}",
                        language.code()
                    );
                    for (text, rect) in &actual {
                        if text.starts_with(localize(owner).as_ref())
                            || text.as_str() == localize(note).as_ref()
                        {
                            assert!(
                                rect.right() <= width + 0.5,
                                "{} {width}: {text} {rect:?}",
                                language.code()
                            );
                        }
                    }
                    before.assert_same(&app.editor);
                }
            }
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
