//! Persisted link sets expand only whole-layer Move and are captured once.
use super::*;

fn linked_fixture(masked: bool) -> Studio {
    let mut app = movement_fixture(true, "layer");
    if !masked {
        assert!(app.editor.core("mask.delete", json!({"mask_id":1})));
    }
    assert!(app.editor.core("layer.add", json!({})));
    assert!(app
        .editor
        .core("layer.fill_color", json!({"color":"#B46693"})));
    if masked {
        assert!(app.editor.core(
            "mask.create",
            json!({"owner":{"kind":"layer","id":2},"select":false})
        ));
        assert!(app
            .editor
            .core("mask.fill", json!({"mask_id":2,"value":89})));
        assert!(app
            .editor
            .core("mask.set", json!({"mask_id":2,"linked":false})));
    }
    assert!(app
        .editor
        .core("layer.links.set", json!({"layer_ids":[1,2],"linked":true})));
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1],"active_layer_id":1})
    ));
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
    app
}

#[test]
fn linked_move_outline_captures_all_members_and_commits_pixels_and_linked_masks_once() {
    let mut app = linked_fixture(true);
    selections(&mut app);
    let ctx = context(&app);
    layout(&mut app, &ctx);
    let p = app.editor.image_rect.min + vec2(2., 2.) * app.editor.zoom;
    let end = p + vec2(2., -1.) * app.editor.zoom;
    let pixels: Vec<_> = app
        .editor
        .session
        .doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let mask = raw(&app, 1);
    let unlinked = raw(&app, 2);
    let selection = app.editor.session.doc.selection().clone();
    let target = app.editor.session.doc.mask_state().target;
    let before = Snapshot::capture(&app.editor);
    let revision = app.editor.session.revision;
    let events = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    pen(&mut app, &ctx, PenPhase::Down, p, 3000);
    assert!(
        app.editor.movement_gesture.is_some(),
        "{}",
        app.editor.status
    );
    assert_eq!(
        app.editor
            .movement_gesture
            .as_ref()
            .unwrap()
            .layer_ids
            .as_deref(),
        Some(&[1, 2][..])
    );
    pen(&mut app, &ctx, PenPhase::Move, end, 3100);
    before.assert_same(&app.editor);
    pen(&mut app, &ctx, PenPhase::Up, end, 3200);
    for (index, input) in pixels.iter().enumerate() {
        assert_eq!(
            app.editor.session.doc.layers[index].pixels,
            shifted(input, 2, -1, 0)
        );
    }
    assert_eq!(raw(&app, 1), shifted(&mask, 2, -1, 255));
    assert_eq!(raw(&app, 2), unlinked);
    assert_eq!(app.editor.session.doc.selection(), &selection);
    assert_eq!(app.editor.session.doc.mask_state().target, target);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), [1]);
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
    before.assert_document(&app.editor);
    assert!(app.editor.core("history.redo", json!({})));
    let first = app.editor.session.doc.layers[0].pixels.clone();
    let second = app.editor.session.doc.layers[1].pixels.clone();
    app.editor.actions.push(Action::UiCommand(10177));
    app.run_actions(&ctx);
    assert_eq!(
        app.editor.session.doc.layers[0].pixels,
        shifted(&first, 1, 0, 0)
    );
    assert_eq!(
        app.editor.session.doc.layers[1].pixels,
        shifted(&second, 1, 0, 0)
    );
}

#[test]
fn linked_move_auto_mask_and_selected_pixels_keep_their_single_destination() {
    for masked in [false, true] {
        let mut app = linked_fixture(masked);
        assert!(app.editor.core(
            "toolbox.movement.options",
            json!({"target":"auto","step":1,"large_step":10})
        ));
        selections(&mut app);
        if masked {
            assert!(app
                .editor
                .core("mask.target", json!({"target":{"kind":"mask","id":1}})));
        }
        let second = app.editor.session.doc.layers[1].pixels.clone();
        assert!(
            app.editor.movement_start(pos2(2., 2.)),
            "{}",
            app.editor.status
        );
        assert!(app
            .editor
            .movement_gesture
            .as_ref()
            .unwrap()
            .layer_ids
            .is_none());
        assert!(
            matches!(
                app.editor.movement_gesture.as_ref().unwrap().destination,
                MoveDestination::RasterMask { mask_id: 1, .. }
            ) == masked
        );
        app.editor.movement_move(pos2(3., 2.), false);
        app.editor.movement_release();
        assert_eq!(app.editor.session.doc.layers[1].pixels, second);
        assert_eq!(app.editor.session.doc.layer_link_sets(), vec![vec![1, 2]]);
    }
}

#[test]
fn linked_layers_do_not_expand_native_brush_painting() {
    let mut app = linked_fixture(false);
    app.editor.area_mode = 0;
    assert!(app.editor.core(
        "brush.set",
        json!({"radius":2.,"color":"#000000","pressure_size":false,"pressure_opacity":false})
    ));
    app.editor.sync_from_session();
    let ctx = context(&app);
    layout(&mut app, &ctx);
    let first = app.editor.session.doc.layers[0].pixels.clone();
    let second = app.editor.session.doc.layers[1].pixels.clone();
    let p = app.editor.image_rect.min + vec2(5., 4.) * app.editor.zoom;
    pen(&mut app, &ctx, PenPhase::Down, p, 3000);
    pen(&mut app, &ctx, PenPhase::Up, p, 3100);
    assert_ne!(app.editor.session.doc.layers[0].pixels, first);
    assert_eq!(app.editor.session.doc.layers[1].pixels, second);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), [1]);
    assert_eq!(app.editor.session.doc.layer_link_sets(), vec![vec![1, 2]]);
}

#[test]
fn linked_move_unmasked_mouse_pen_and_nudge_preserve_live_brush_hand_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for route in ["mouse", "pen", "nudge"] {
                let mut app = linked_fixture(false);
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
                app.editor.panning = (local && kind == ToolKind::Hand).then_some(Pos2::ZERO);
                app.editor.session.doc.compose();
                let before = Snapshot::capture(&app.editor);
                let transcript = samples(&mut app, start);
                app.editor.radius += 2.;
                app.editor.pan += vec2(3., 2.);
                match route {
                    "mouse" if local => {
                        assert!(!app.editor.movement_start(pos2(2., 2.)));
                    }
                    "mouse" => canvas_frame(
                        &mut app,
                        &ctx,
                        vec![Event::PointerMoved(p), button(p, true)],
                        3.,
                    ),
                    "pen" => pen(&mut app, &ctx, PenPhase::Down, p, 3000),
                    _ => {
                        app.editor.actions.push(Action::UiCommand(10177));
                        app.run_actions(&ctx);
                    }
                }
                assert!(
                    app.editor.status.contains("stroke_active"),
                    "{route}: {}",
                    app.editor.status
                );
                assert!(app.editor.movement_gesture.is_none());
                before.assert_same(&app.editor);
                assert_eq!(app.editor.input_owned, local);
                assert_eq!(app.editor.pen_active, local);
                assert_eq!(
                    app.editor.panning.is_some(),
                    local && kind == ToolKind::Hand
                );
                assert_eq!(samples(&mut app, start), transcript);
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Move, 120, 0.9))
                    .unwrap();
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Up, 140, 0.4))
                    .unwrap();
                assert_eq!(samples(&mut app, start).len(), 3);
            }
        }
    }
}

#[test]
fn linked_move_rejects_any_member_lock_and_stale_membership_without_partial_pixels() {
    let mut app = linked_fixture(false);
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":2,"position_locked":true})));
    let locked = Snapshot::capture(&app.editor);
    assert!(!app.editor.movement_start(pos2(2., 2.)));
    locked.assert_same(&app.editor);
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":2,"position_locked":false})));
    assert!(app.editor.movement_start(pos2(2., 2.)));
    app.editor.movement_move(pos2(4., 2.), false);
    assert!(app
        .editor
        .core("layer.links.set", json!({"layer_ids":[2],"linked":false})));
    let stale = Snapshot::capture(&app.editor);
    app.editor.movement_release();
    assert_eq!(app.editor.status, "Document changed; movement cancelled");
    stale.assert_same(&app.editor);
}

#[test]
fn linked_move_outline_cancel_preserves_later_external_contact_and_raw_samples() {
    for cancel in [true, false] {
        let mut app = linked_fixture(false);
        let ctx = context(&app);
        layout(&mut app, &ctx);
        let p = app.editor.image_rect.center();
        pen(&mut app, &ctx, PenPhase::Down, p, 3000);
        assert!(app.editor.guarded_movement_pending());
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
        pen(
            &mut app,
            &ctx,
            if cancel {
                PenPhase::Cancel
            } else {
                PenPhase::Up
            },
            p,
            3200,
        );
        assert!(app.editor.movement_gesture.is_none());
        assert!(!app.editor.input_owned);
        assert!(app.editor.session.stroke.active());
        before.assert_same(&app.editor);
        assert_eq!(samples(&mut app, start), transcript);
        app.editor
            .session
            .pointer(pointer(ToolKind::Brush, PointerPhase::Up, 140, 0.4))
            .unwrap();
        assert_eq!(samples(&mut app, start).len(), 2);
    }
}

#[test]
fn linked_move_resolved_count_wraps_in_four_languages_without_publishing_a_draft() {
    crate::i18n::load_test_catalogs();
    let mut app = linked_fixture(false);
    let ctx = context(&app);
    fn text_bounds(shape: &Shape, label: &str) -> Option<Rect> {
        match shape {
            Shape::Text(text) if text.galley.text() == label => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_bounds(shape, label)),
            _ => None,
        }
    }
    for language in crate::i18n::Language::ALL {
        crate::i18n::set_language(language);
        for width in [244., 680.] {
            let before = Snapshot::capture(&app.editor);
            let mut bounds = None;
            for _ in 0..3 {
                let output = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 600.))),
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| app.editor.movement_options_ui(ui));
                    },
                );
                let label = localize("2 linked layers move together.".to_owned());
                bounds = output
                    .shapes
                    .iter()
                    .find_map(|shape| text_bounds(&shape.shape, &label));
            }
            let bounds =
                bounds.unwrap_or_else(|| panic!("{language:?}: missing linked Move feedback"));
            assert!(
                bounds.right() <= width + 0.5,
                "{language:?} {width}: {bounds:?}"
            );
            before.assert_same(&app.editor);
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
