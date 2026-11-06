//! Owner edits must reach shared idle admission before UI drafts end a contact.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{click, fixture, label, Snapshot};
use crate::engine::raster_masks::PlaneView;

fn studio() -> Studio {
    let (mut app, masks) = fixture(1);
    assert!(app.editor.core("layer.add", json!({})));
    assert!(app.editor.core("layer.select", json!({"layer_id":1})));
    assert!(app.editor.core(
        "mask.target",
        json!({"target":{"kind":"mask","id":masks[0]}})
    ));
    assert!(app.editor.core(
        "brush.set",
        json!({"radius":1.,"color":"#000000","stabilizer":0.55})
    ));
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
    app
}
fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 5.,
        y: 4.,
        pressure: 0.7,
        pen: true,
        eraser: false,
        time,
        tool_override: Some(kind),
        view_x: Some(50.),
        view_y: Some(40.),
    }
}
fn down(app: &mut Studio, kind: ToolKind, local: bool) {
    app.editor
        .session
        .pointer(pointer(kind, PointerPhase::Down, 0))
        .unwrap();
    app.editor.sync_from_session();
    app.editor.input_owned = local;
    app.editor.pen_active = local;
    app.editor.panning = (local && kind == ToolKind::Hand).then_some(Pos2::ZERO);
    app.editor.session.doc.compose();
}
fn assert_contact(app: &Studio, kind: ToolKind, local: bool) {
    assert!(app.editor.session.stroke.active());
    assert!(app.editor.session.pointer_checkpoint.is_some());
    assert_eq!(app.editor.input_owned, local);
    assert_eq!(app.editor.pen_active, local);
    assert_eq!(app.editor.drawing, kind == ToolKind::Brush);
    assert_eq!(
        app.editor.panning.is_some(),
        local && kind == ToolKind::Hand
    );
}
fn form_frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(crate::i18n::Language::English);
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 900.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.editor.plugin_command_window(ctx),
    )
}
fn mask_samples(app: &Studio, id: u64) -> Vec<u8> {
    match app.editor.session.doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => {
            vec![value; app.editor.session.doc.width * app.editor.session.doc.height]
        }
        PlaneView::Dense(samples) => samples.to_vec(),
    }
}

#[test]
fn pending_owner_edits_reject_before_dirty_names_and_preserve_local_or_external_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for action in [
                crate::application::Action::new("layer.duplicate", json!({"layer_id":1})),
                crate::application::Action::new("layer.delete", json!({"layer_id":1})),
                crate::application::Action::new(
                    "layer.batch",
                    json!({"action":"layer.delete","layer_ids":[1]}),
                ),
            ] {
                let mut app = studio();
                down(&mut app, kind, local);
                let before = Snapshot::capture(&app.editor);
                let ctx = Context::default();
                app.editor.dispatch_layer_pending_ui(
                    vec![
                        crate::application::Action::new(
                            "layer.rename",
                            json!({"layer_id":1,"name":"Pending ink name"}),
                        ),
                        action,
                    ],
                    Some(&ctx),
                );
                before.assert_same(&app.editor);
                assert_contact(&app, kind, local);
                assert_eq!(
                    ctx.data_mut(|data| data.get_temp::<String>(Id::new((
                        "layer-name",
                        app.editor.session.doc.identity,
                        1u64
                    )))),
                    Some("Pending ink name".into())
                );
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Move, 10))
                    .unwrap();
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Up, 20))
                    .unwrap();
                assert!(!app.editor.session.stroke.active());
            }
        }
    }
}

#[test]
fn static_duplicate_and_searchable_owner_forms_preserve_live_contacts_through_run_and_cancel() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            app.editor.register_commands();
            down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let ctx = Context::default();
            theme::fonts(&ctx);
            theme::apply(&ctx, &app.editor.settings.ui);
            app.editor.actions.push(Action::UiCommand(10140));
            app.run_actions(&ctx);
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            for (index, action) in ["layer.duplicate", "layer.delete", "layer.batch"]
                .into_iter()
                .enumerate()
            {
                let command = app
                    .editor
                    .settings
                    .keys
                    .commands()
                    .find(|command| command.keywords == action)
                    .expect("Owner commands are searchable and bindable")
                    .ui;
                app.editor.actions.push(Action::UiCommand(command));
                app.run_actions(&ctx);
                assert!(app.editor.command_form.is_some());
                assert!(app.editor.contact_draft_modal());
                app.editor.dispatch_layer_pending_ui(
                    vec![crate::application::Action::new(
                        "layer.rename",
                        json!({"layer_id":1,"name":"Dialog-local name draft"}),
                    )],
                    Some(&ctx),
                );
                before.assert_same(&app.editor);
                let base = (index * 10) as f64;
                let mut output = form_frame(&mut app, &ctx, vec![], base);
                for frame in 1..3 {
                    output = form_frame(&mut app, &ctx, vec![], base + frame as f64);
                }
                let run = output
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, "Run Command"))
                    .unwrap();
                // Batch arguments are an unfinished draft here; valid batch
                // rejection is exercised through the pending dispatcher above.
                if action != "layer.batch" {
                    form_frame(&mut app, &ctx, click(run), base + 3.);
                }
                before.assert_same(&app.editor);
                assert_contact(&app, kind, local);
                app.editor.actions.push(Action::Cancel);
                app.run_actions(&ctx);
                assert!(app.editor.command_form.is_none());
                before.assert_same(&app.editor);
                assert_contact(&app, kind, local);
            }
            app.editor
                .session
                .pointer(pointer(kind, PointerPhase::Up, 20))
                .unwrap();
        }
    }
}

#[test]
fn separately_queued_owner_command_defers_auto_name_until_shared_admission() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let ctx = Context::default();
            app.editor.actions.push(Action::UiCommand(10140));
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Menu-local name draft"}),
                )],
                Some(&ctx),
            );
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            assert_eq!(app.editor.actions, vec![Action::UiCommand(10140)]);
            assert_eq!(
                ctx.data_mut(|data| data.get_temp::<String>(Id::new((
                    "layer-name",
                    app.editor.session.doc.identity,
                    1u64,
                )))),
                Some("Menu-local name draft".into())
            );
            app.run_actions(&ctx);
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            app.editor
                .session
                .pointer(pointer(kind, PointerPhase::Up, 20))
                .unwrap();
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Committed after contact"}),
                )],
                Some(&ctx),
            );
            assert_eq!(
                app.editor.session.doc.layer_name(0),
                "Committed after contact"
            );
        }
    }
}

#[test]
fn unrelated_queued_commands_keep_normal_name_commit_behavior() {
    let mut app = studio();
    down(&mut app, ToolKind::Brush, true);
    app.editor.actions.push(Action::TogglePanels);
    app.editor
        .dispatch_layer_pending(vec![crate::application::Action::new(
            "layer.rename",
            json!({"layer_id":1,"name":"Explicit ordinary name"}),
        )]);
    assert_eq!(
        app.editor.session.doc.layer_name(0),
        "Explicit ordinary name"
    );
    assert!(!app.editor.session.stroke.active());
    assert_eq!(app.editor.actions, vec![Action::TogglePanels]);
}

#[test]
fn pending_unmasked_batches_preserve_the_existing_shared_idle_guard() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            assert!(app.editor.core("mask.delete", json!({"mask_id":1})));
            down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            app.editor
                .dispatch_layer_pending(vec![crate::application::Action::new(
                    "layer.batch",
                    json!({"action":"layer.set","layer_ids":[1,2],"args":{"color_label":"violet"}}),
                )]);
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            app.editor
                .session
                .pointer(pointer(kind, PointerPhase::Up, 20))
                .unwrap();
        }
    }
}

#[test]
fn actual_singleton_layer_menu_duplicates_or_deletes_owned_masks_and_undo_restores_them() {
    for title in ["Duplicate Layer", "Delete Layer"] {
        let mut app = studio();
        let original = mask_samples(&app, 1);
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let frame = |app: &mut Studio, events, time| {
            crate::i18n::set_language(crate::i18n::Language::English);
            let mut pending = Vec::new();
            let output = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 1200.))),
                    events,
                    time: Some(time),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default()
                        .show(ctx, |ui| {
                            if let Some(action) = app.editor.layer_menu(ui, 0) {
                                pending.push(action);
                            }
                        })
                        .inner
                },
            );
            app.editor.dispatch_layer_pending_ui(pending, Some(&ctx));
            output
        };
        let mut output = frame(&mut app, vec![], 0.);
        for time in 1..3 {
            output = frame(&mut app, vec![], time as f64);
        }
        let point = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, title))
            .unwrap();
        frame(&mut app, click(point), 3.);
        assert_eq!(
            app.editor.session.doc.layers.len(),
            if title == "Duplicate Layer" { 3 } else { 1 }
        );
        assert_eq!(
            app.editor.session.doc.mask_state().items.len(),
            if title == "Duplicate Layer" { 2 } else { 0 }
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.layers.len(), 2);
        assert_eq!(app.editor.session.doc.mask_state().items.len(), 1);
        assert_eq!(mask_samples(&app, 1), original);
    }
}
