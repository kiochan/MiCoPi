//! Menu/shortcut adapters must leave shared geometry admission to Session.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{fixture, Snapshot};
use serde_json::json;

fn contact(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 5.,
        y: 4.,
        pressure: 0.65,
        pen: true,
        eraser: false,
        time,
        tool_override: Some(kind),
        view_x: Some(50.),
        view_y: Some(40.),
    }
}

#[test]
fn studio_geometry_menu_commands_and_cancel_preserve_shared_live_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let (mut app, _) = fixture(1);
        app.editor
            .session
            .pointer(contact(kind, PointerPhase::Down, 0))
            .unwrap();
        app.editor.session.doc.compose();
        let before = Snapshot::capture(&app.editor);
        let ctx = Context::default();
        for id in [10144, 10145, 10173, 10174] {
            app.editor.actions.push(Action::UiCommand(id));
            app.run_actions(&ctx);
            assert!(app.editor.geometry.is_some(), "{kind:?}/{id}");
            before.assert_same(&app.editor);
            assert!(app.editor.session.stroke.active());
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            assert!(app.editor.geometry.is_none());
            before.assert_same(&app.editor);
        }
        for id in 10166..=10172 {
            app.editor.actions.push(Action::UiCommand(id));
            app.run_actions(&ctx);
            before.assert_same(&app.editor);
            assert!(app.editor.session.stroke.active(), "{kind:?}/{id}");
        }
        app.editor
            .session
            .pointer(contact(kind, PointerPhase::Up, 1))
            .unwrap();
        assert!(!app.editor.session.stroke.active());
    }
}

#[test]
fn studio_geometry_registered_plan_form_and_cancel_preserve_shared_live_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let (mut app, _) = fixture(1);
        app.editor.register_commands();
        let command = app
            .editor
            .settings
            .keys
            .commands()
            .find(|command| command.keywords == "canvas.geometry.plan")
            .expect("The public plan must be searchable and bindable")
            .ui;
        app.editor
            .session
            .pointer(contact(kind, PointerPhase::Down, 0))
            .unwrap();
        app.editor.session.doc.compose();
        let before = Snapshot::capture(&app.editor);
        let ctx = Context::default();
        app.editor.actions.push(Action::UiCommand(command));
        app.run_actions(&ctx);
        assert!(app.editor.geometry_contact_modal());
        before.assert_same(&app.editor);
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 900.))),
                time: Some(0.),
                focused: true,
                ..Default::default()
            },
            |ctx| app.editor.plugin_command_window(ctx),
        );
        before.assert_same(&app.editor);
        app.editor.actions.push(Action::Cancel);
        app.run_actions(&ctx);
        assert!(app.editor.command_form.is_none());
        before.assert_same(&app.editor);
        assert!(app.editor.session.stroke.active());
        app.editor
            .session
            .pointer(contact(kind, PointerPhase::Up, 1))
            .unwrap();
        assert!(!app.editor.session.stroke.active());
    }
}

#[test]
fn switching_to_move_tool_keeps_immediate_contact_and_pen_cleanup() {
    let (mut app, _) = fixture(1);
    app.editor
        .session
        .pointer(contact(ToolKind::Brush, PointerPhase::Down, 0))
        .unwrap();
    app.editor.pen_active = true;
    app.editor.actions.push(Action::UiCommand(10175));
    app.run_actions(&Context::default());
    assert!(!app.editor.session.stroke.active());
    assert!(app.editor.session.pointer_checkpoint.is_none());
    assert!(!app.editor.pen_active);
}

#[test]
fn owner_turns_reconcile_local_flags_only_after_shared_contact_completion() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for dynamic in [false, true] {
            for (ui, action) in [
                (10171, "layer.rotate_90_cw"),
                (10172, "layer.rotate_90_ccw"),
            ] {
                for protection in 0..3 {
                    let (mut app, masks) = fixture(1);
                    if protection != 1 {
                        assert!(app.editor.core("mask.delete", json!({"mask_id":masks[0]})));
                    }
                    if protection == 2 {
                        assert!(app
                            .editor
                            .core("layer.set", json!({"position_locked":true})));
                    }
                    app.editor
                        .session
                        .pointer(contact(kind, PointerPhase::Down, 0))
                        .unwrap();
                    app.editor.input_owned = true;
                    app.editor.pen_active = true;
                    app.editor.drawing = app.editor.session.is_drawing();
                    app.editor.panning = (kind == ToolKind::Hand).then_some(Pos2::ZERO);
                    app.editor.session.doc.compose();
                    let before = Snapshot::capture(&app.editor);
                    if dynamic {
                        app.editor.begin_plugin_command(action);
                    } else {
                        app.editor.actions.push(Action::UiCommand(ui));
                        app.run_actions(&Context::default());
                    }
                    if protection == 0 {
                        assert!(!app.editor.session.stroke.active());
                        assert!(app.editor.session.pointer_checkpoint.is_none());
                        assert!(!app.editor.input_owned && !app.editor.pen_active);
                        assert!(!app.editor.drawing && app.editor.panning.is_none());
                    } else {
                        before.assert_same(&app.editor);
                        assert!(app.editor.session.stroke.active());
                        assert!(app.editor.input_owned && app.editor.pen_active);
                        assert_eq!(app.editor.drawing, kind == ToolKind::Brush);
                        assert_eq!(app.editor.panning.is_some(), kind == ToolKind::Hand);
                        app.editor.finish_stroke();
                    }
                }
            }
        }
    }
}
