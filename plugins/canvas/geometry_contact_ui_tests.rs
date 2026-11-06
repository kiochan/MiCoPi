//! Geometry modals own drafts, not an existing local or external native contact.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{click, fixture, label, Snapshot};
use serde_json::json;

fn open(editor: &mut Editor, kind: usize) {
    match kind {
        0 => editor.open_canvas_geometry(false),
        1 => editor.open_canvas_geometry(true),
        2 => editor.open_image_size(),
        _ => editor.open_trim_canvas(),
    }
    editor.blocked = true;
    editor.contact_draft_allowed = true;
}
fn frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(crate::i18n::Language::English);
    if !ctx.data_mut(|data| {
        data.get_temp::<bool>(Id::new("geometry-contact-fonts"))
            .unwrap_or(false)
    }) {
        theme::fonts(ctx);
        theme::apply(ctx, &app.editor.settings.ui);
        ctx.data_mut(|data| data.insert_temp(Id::new("geometry-contact-fonts"), true));
    }
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 900.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.canvas_ui(ui));
            app.editor.canvas_geometry_window(ctx);
        },
    )
}
fn up(position: Pos2) -> Vec<Event> {
    vec![Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]
}
fn pointer(kind: ToolKind, phase: PointerPhase, x: f32, time: u32) -> Pointer {
    Pointer {
        phase,
        x,
        y: 4.,
        pressure: 0.65,
        time,
        pen: false,
        eraser: false,
        view_x: Some(x),
        view_y: Some(4.),
        tool_override: Some(kind),
    }
}

#[test]
fn local_brush_and_hand_continue_move_and_real_up_through_each_read_only_geometry_modal() {
    for tool in ["brush", "hand"] {
        for dialog in 0..4 {
            let (mut app, masks) = fixture(1);
            assert!(app.editor.core("tool.select", json!({ "tool": tool })));
            assert!(app.editor.core(
                "brush.set",
                json!({"radius":2.,"stabilizer":0.35,"color":"#000000"})
            ));
            app.editor.sync_from_session();
            let ctx = Context::default();
            let _ = frame(&mut app, &ctx, vec![], 0.);
            let start = app.editor.image_rect.min + vec2(4., 4.) * app.editor.zoom;
            app.editor.pointer_down(
                start,
                0.65,
                1,
                false,
                crate::ui::keymap::InputState::default(),
                false,
                Modifiers::NONE,
            );
            assert!(app.editor.input_owned && app.editor.session.stroke.active());
            app.editor.session.doc.compose();
            let before = Snapshot::capture(&app.editor);
            open(&mut app.editor, dialog);
            before.assert_same(&app.editor);
            for time in 1..4 {
                let _ = frame(&mut app, &ctx, vec![], time as f64);
            }
            before.assert_same(&app.editor);
            let key = match dialog {
                2 => "image-size-apply",
                3 => "canvas-trim-apply",
                _ => "canvas-geometry-apply",
            };
            assert!(
                !ctx.data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new(key)))
                    .unwrap()
                    .1
            );
            let _ = frame(
                &mut app,
                &ctx,
                vec![Event::PointerButton {
                    pos: start,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                }],
                4.,
            );
            before.assert_same(&app.editor);
            let moved = app.editor.image_rect.min + vec2(6., 4.) * app.editor.zoom;
            let _ = frame(&mut app, &ctx, vec![Event::PointerMoved(moved)], 5.);
            assert!(app.editor.session.stroke.active(), "{tool}/{dialog}");
            assert!(app.editor.input_owned);
            assert_eq!(
                app.editor.session.last_pointer.unwrap().phase,
                PointerPhase::Move
            );
            let _ = frame(&mut app, &ctx, up(moved), 6.);
            assert!(!app.editor.session.stroke.active(), "{tool}/{dialog}");
            assert!(app.editor.session.pointer_checkpoint.is_none());
            assert!(!app.editor.input_owned);
            assert_eq!(
                app.editor.session.last_pointer.unwrap().phase,
                PointerPhase::Up
            );
            assert_eq!(
                app.editor.session.doc.mask_state().target,
                crate::engine::raster_masks::PaintTarget::Mask { id: masks[0] }
            );
        }
    }
}

#[test]
fn external_contacts_survive_dialog_open_read_only_plans_cancel_and_unrelated_mouse_up() {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        for dialog in 0..5 {
            let (mut app, _) = fixture(1);
            let ctx = Context::default();
            let _ = frame(&mut app, &ctx, vec![], 0.);
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Down, 4., 1))
                .unwrap();
            app.editor.sync_from_session();
            app.editor.session.doc.compose();
            assert!(!app.editor.input_owned);
            let before = Snapshot::capture(&app.editor);
            if dialog < 4 {
                open(&mut app.editor, dialog);
            } else {
                // A mouse release elsewhere in the UI does not belong to MCP's
                // native contact, whether or not a geometry modal is present.
                app.editor.blocked = true;
                app.editor.contact_draft_allowed = false;
            }
            before.assert_same(&app.editor);
            let mut output = frame(&mut app, &ctx, vec![], 1.);
            for time in 2..4 {
                output = frame(&mut app, &ctx, vec![], time as f64);
            }
            before.assert_same(&app.editor);
            if dialog < 4 {
                let cancel = output
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, "Cancel"))
                    .unwrap();
                let _ = frame(&mut app, &ctx, click(cancel), 4.);
                assert!(app.editor.geometry.is_none());
            } else {
                let _ = frame(&mut app, &ctx, up(pos2(20., 20.)), 4.);
            }
            before.assert_same(&app.editor);
            assert!(app.editor.session.stroke.active(), "{tool:?}/{dialog}");
            assert!(app.editor.session.pointer_checkpoint.is_some());
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Up, 4., 5))
                .unwrap();
            assert!(!app.editor.session.stroke.active());
        }
    }
}
