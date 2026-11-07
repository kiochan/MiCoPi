//! Subtree deletion uses shared admission before UI drafts can end a contact.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{click, fixture, label, Snapshot};
use crate::engine::raster_masks::{PaintTarget, PlaneView};

fn studio(populated: bool) -> Studio {
    let (mut app, _) = fixture(2);
    for color in ["#E5A4C4", "#243044"] {
        assert!(app.editor.core("layer.add", json!({})));
        assert!(app.editor.core("layer.fill_color", json!({"color":color})));
    }
    assert!(app.editor.core(
        "mask.create",
        json!({"owner":{"kind":"layer","id":3},"select":false})
    ));
    if populated {
        assert!(app.editor.core(
            "group.move",
            json!({
                "items":[{"kind":"layer","id":1},{"kind":"layer","id":2}],
                "parent_id":1,"index":0
            })
        ));
        assert!(app.editor.core(
            "group.create",
            json!({
                "members":[{"kind":"layer","id":2}],"parent_id":1,"name":"Nested"
            })
        ));
        assert!(app.editor.core(
            "mask.create",
            json!({"owner":{"kind":"group","id":2},"select":false})
        ));
    }
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1,2,3],"active_layer_id":1})
    ));
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":1}})));
    assert!(app.editor.core(
        "brush.set",
        json!({
            "radius":1.,"color":"#000000","stabilizer":0.55,
            "pressure_size":true,"pressure_opacity":true
        })
    ));
    assert!(app.editor.core(
        "canvas.stroke",
        json!({"tool":"brush","points":[
            {"x":3.,"y":3.,"pressure":0.4,"pen":true,"time":0},
            {"x":8.,"y":5.,"pressure":0.8,"pen":true,"time":20}
        ]})
    ));
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
    app
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
}
fn properties_frame(
    app: &mut Studio,
    ctx: &Context,
    events: Vec<Event>,
    time: f64,
    mut pending: Vec<crate::application::Action>,
) -> FullOutput {
    crate::i18n::set_language(crate::i18n::Language::English);
    let output = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 1000.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                app.editor.group_properties_ui(ui, &mut pending);
            });
        },
    );
    app.editor.dispatch_layer_pending_ui(pending, Some(ctx));
    output
}
fn option(output: &FullOutput, text: &str) -> Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, text))
        .unwrap()
}
fn rendered_labels(output: &FullOutput) -> Vec<String> {
    fn visit(shape: &Shape, labels: &mut Vec<String>) {
        match shape {
            Shape::Text(text) => labels.push(text.galley.text().to_owned()),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, labels);
                }
            }
            _ => {}
        }
    }
    let mut labels = Vec::new();
    for shape in &output.shapes {
        visit(&shape.shape, &mut labels);
    }
    labels
}
fn open_delete(app: &mut Studio, ctx: &Context, base: f64) -> Pos2 {
    let mut output = properties_frame(app, ctx, vec![], base, vec![]);
    for offset in 1..3 {
        output = properties_frame(app, ctx, vec![], base + offset as f64, vec![]);
    }
    properties_frame(
        app,
        ctx,
        click(option(&output, "Group Actions")),
        base + 3.,
        vec![],
    );
    output = properties_frame(app, ctx, vec![], base + 4., vec![]);
    option(&output, "Delete Group and Contents")
}
fn raw(app: &Studio, id: u64) -> Vec<u8> {
    let doc = &app.editor.session.doc;
    match doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => vec![value; doc.width * doc.height],
        PlaneView::Dense(data) => data.to_vec(),
    }
}
fn pointer(kind: ToolKind, phase: PointerPhase, time: u32, pressure: f32) -> Pointer {
    Pointer {
        phase,
        x: (time / 20) as f32,
        y: 4.,
        pressure,
        pen: true,
        eraser: false,
        time,
        tool_override: Some(kind),
        view_x: Some(50.),
        view_y: Some(40.),
    }
}
fn down(app: &mut Studio, kind: ToolKind, local: bool) -> usize {
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
    start
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
fn samples(app: &mut Studio, start: usize) -> Vec<Value> {
    let journal = app.editor.session.replay_journal(false).unwrap();
    let events = journal
        .events(start, journal.entries.len() - start)
        .unwrap();
    let mut result = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["source"]["kind"] == "pointer")
        .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
        .collect::<Vec<_>>();
    result.extend(
        serde_json::to_value(&app.editor.session.replay_samples)
            .unwrap()
            .as_array()
            .unwrap()
            .clone(),
    );
    result
}
fn finish(app: &mut Studio, kind: ToolKind, start: usize) {
    app.editor
        .session
        .pointer(pointer(kind, PointerPhase::Move, 120, 0.9))
        .unwrap();
    app.editor
        .session
        .pointer(pointer(kind, PointerPhase::Up, 140, 0.4))
        .unwrap();
    let actual = samples(app, start);
    assert_eq!(actual.len(), 3);
    for (sample, (time, pressure)) in actual.iter().zip([(100, 0.3), (120, 0.9), (140, 0.4)]) {
        assert_eq!(sample["time"], time);
        assert!((sample["pressure"].as_f64().unwrap() - pressure).abs() < 1e-6);
        assert_eq!(sample["pen"], true);
    }
    assert_eq!(
        app.editor
            .session
            .apply(crate::application::Action::new("replay.verify", json!({})))
            .unwrap()["result"]["valid"],
        true
    );
}
fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}
fn draft_frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(crate::i18n::Language::English);
    app.editor.blocked = app.editor.palette.is_some() || app.editor.command_form.is_some();
    app.editor.contact_draft_allowed = app.editor.contact_draft_modal();
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 1000.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.canvas_ui(ui));
            app.editor.palette_ui(ctx);
            app.editor.plugin_command_window(ctx);
        },
    )
}
fn command(app: &Studio) -> u32 {
    app.editor
        .settings
        .keys
        .commands()
        .find(|command| command.keywords == "group.delete")
        .unwrap()
        .ui
}
fn select_delete(app: &mut Studio) {
    let index = app
        .editor
        .settings
        .keys
        .commands()
        .enumerate()
        .find_map(|(index, command)| (command.keywords == "group.delete").then_some(index))
        .unwrap();
    let id = app.editor.settings.keys.command(index).id.to_owned();
    let editor = &mut app.editor;
    let palette = editor.palette.as_mut().unwrap();
    palette.query(id, &editor.settings.keys);
    palette.selected = palette
        .results
        .iter()
        .position(|candidate| *candidate == index)
        .unwrap();
}

#[test]
fn actual_group_delete_menu_removes_only_subtree_owners_and_undo_restores_them() {
    for target in [
        json!({"kind":"layer_pixels"}),
        json!({"kind":"mask","id":1}),
        json!({"kind":"mask","id":2}),
        json!({"kind":"mask","id":3}),
        json!({"kind":"mask","id":4}),
    ] {
        let mut app = studio(true);
        assert!(app.editor.core("mask.target", json!({"target":target})));
        let ctx = context(&app);
        app.editor.focus_group_ui(&ctx, Some(1));
        let before = Snapshot::capture(&app.editor);
        let retained = raw(&app, 3);
        let retained_index = app.editor.session.doc.layer_index(3).unwrap();
        let retained_pointer = app.editor.session.doc.layers[retained_index]
            .pixels
            .as_ptr();
        let revision = app.editor.session.revision;
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let point = open_delete(&mut app, &ctx, 0.);
        properties_frame(&mut app, &ctx, click(point), 5., vec![]);
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
        assert_eq!(app.editor.session.doc.layers.len(), 1);
        assert_eq!(app.editor.session.doc.layer_id(0), 3);
        assert_eq!(app.editor.session.doc.selected_layer_ids(), vec![3]);
        assert_eq!(
            app.editor.session.doc.layers[0].pixels.as_ptr(),
            retained_pointer
        );
        assert_eq!(
            app.editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(raw(&app, 3), retained);
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            if target["id"] == 3 {
                PaintTarget::Mask { id: 3 }
            } else {
                PaintTarget::LayerPixels
            }
        );
        assert_eq!(app.editor.focused_group_ui(&ctx), None);
        assert!(app.editor.core("history.undo", json!({})));
        app.editor.session.doc.compose();
        before.assert_document(&app.editor);
        assert!(app.editor.core("history.redo", json!({})));
        assert_eq!(app.editor.session.doc.layers.len(), 1);
        assert_eq!(raw(&app, 3), retained);
    }
}

#[test]
fn actual_empty_masked_group_delete_keeps_rasters_and_restores_the_selected_mask_on_undo() {
    let mut app = studio(false);
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":2}})));
    let ctx = context(&app);
    app.editor.focus_group_ui(&ctx, Some(1));
    let before = Snapshot::capture(&app.editor);
    let pointers = app
        .editor
        .session
        .doc
        .layers
        .iter()
        .map(|layer| layer.pixels.as_ptr())
        .collect::<Vec<_>>();
    let point = open_delete(&mut app, &ctx, 0.);
    properties_frame(&mut app, &ctx, click(point), 5., vec![]);
    assert_eq!(
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.as_ptr())
            .collect::<Vec<_>>(),
        pointers
    );
    assert_eq!(app.editor.session.doc.selected_layer_ids(), vec![1, 2, 3]);
    assert_eq!(
        app.editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active),
        1
    );
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        PaintTarget::LayerPixels
    );
    assert_eq!(
        app.editor
            .session
            .doc
            .mask_state()
            .items
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    assert!(app.editor.core("history.undo", json!({})));
    app.editor.session.doc.compose();
    before.assert_document(&app.editor);
}

#[test]
fn actual_delete_menu_preserves_dirty_group_and_layer_names_during_every_contact_kind() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            let ctx = context(&app);
            app.editor.focus_group_ui(&ctx, Some(1));
            properties_frame(&mut app, &ctx, vec![], 0., vec![]);
            let name = Id::new(("group-name", app.editor.session.doc.identity, 1u64));
            ctx.memory_mut(|memory| memory.request_focus(name));
            properties_frame(&mut app, &ctx, vec![], 1., vec![]);
            let mut edit = key(
                Key::A,
                Modifiers {
                    ctrl: true,
                    command: true,
                    ..Modifiers::NONE
                },
            );
            edit.push(Event::Text("Pending subtree name".into()));
            let output = properties_frame(&mut app, &ctx, edit, 2., vec![]);
            assert!(output
                .shapes
                .iter()
                .any(|shape| label(&shape.shape, "Pending subtree name").is_some()));
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let expected_samples = samples(&mut app, start);
            let point = open_delete(&mut app, &ctx, 3.);
            properties_frame(
                &mut app,
                &ctx,
                click(point),
                8.,
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Pending raster name"}),
                )],
            );
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            assert!(app.editor.status.contains("stroke_active"));
            assert_eq!(samples(&mut app, start), expected_samples);
            assert_eq!(app.editor.focused_group_ui(&ctx), Some(1));
            assert_eq!(
                ctx.data_mut(|data| data.get_temp::<String>(Id::new((
                    "layer-name",
                    app.editor.session.doc.identity,
                    1u64
                )))),
                Some("Pending raster name".into())
            );
            let output = properties_frame(&mut app, &ctx, vec![], 9., vec![]);
            assert!(output
                .shapes
                .iter()
                .any(|shape| label(&shape.shape, "Pending subtree name").is_some()));
            finish(&mut app, kind, start);
            ctx.memory_mut(|memory| memory.request_focus(name));
            properties_frame(&mut app, &ctx, vec![], 10., vec![]);
            properties_frame(
                &mut app,
                &ctx,
                key(Key::Enter, Modifiers::NONE),
                11.,
                vec![],
            );
            assert_eq!(
                app.editor.session.doc.groups_state()["groups"][0]["name"],
                "Pending subtree name"
            );
        }
    }
}

#[test]
fn real_delete_palette_form_keeps_contact_and_sample_chunks_through_run_and_cancel() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let history = app.editor.session.doc.history_bytes();
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
            assert!(app.editor.command_palette_open());
            assert_contact(&app, kind, local);
            draft_frame(&mut app, &ctx, vec![], 0.);
            draft_frame(
                &mut app,
                &ctx,
                vec![Event::PointerButton {
                    pos: pos2(20., 900.),
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                1.,
            );
            assert_contact(&app, kind, local);
            select_delete(&mut app);
            draft_frame(&mut app, &ctx, key(Key::Enter, Modifiers::NONE), 2.);
            app.run_actions(&ctx);
            assert!(app.editor.guarded_command_form());
            assert!(app.editor.contact_draft_modal());
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Form-local raster name"}),
                )],
                Some(&ctx),
            );
            let mut output = draft_frame(&mut app, &ctx, vec![], 3.);
            for time in 4..6 {
                output = draft_frame(&mut app, &ctx, vec![], time as f64);
            }
            // The required selector starts at its schema minimum, group 1.
            assert!(output
                .shapes
                .iter()
                .any(|shape| label(&shape.shape, "1").is_some()));
            draft_frame(&mut app, &ctx, click(option(&output, "Run Command")), 6.);
            let output = draft_frame(&mut app, &ctx, vec![], 7.);
            assert!(
                rendered_labels(&output).iter().any(|text| text.starts_with(
                    "stroke_active: Finish the current contact before editing groups"
                )),
                "Missing shared contact rejection; rendered form labels: {:#?}",
                rendered_labels(&output)
            );
            before.assert_document(&app.editor);
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_contact(&app, kind, local);
            let after_run = samples(&mut app, start);
            assert_eq!(after_run.len(), 1);
            draft_frame(&mut app, &ctx, key(Key::Escape, Modifiers::NONE), 8.);
            assert!(app.editor.command_form.is_none());
            before.assert_document(&app.editor);
            assert_contact(&app, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn separately_queued_delete_form_defers_a_lost_focus_layer_name_before_opening() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            app.editor.register_commands();
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let command = command(&app);
            app.editor.actions.push(Action::UiCommand(command));
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Queued delete draft"}),
                )],
                Some(&ctx),
            );
            before.assert_same(&app.editor);
            app.run_actions(&ctx);
            assert!(app.editor.guarded_command_form());
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            before.assert_same(&app.editor);
            assert!(app.editor.command_form.is_none());
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn group_copy_ungroup_and_layer_batches_stay_guarded_after_subtree_delete_is_enabled() {
    let mut app = studio(true);
    let before = Snapshot::capture(&app.editor);
    for action in [
        crate::application::Action::new("group.duplicate", json!({"group_id":1})),
        crate::application::Action::new("group.ungroup", json!({"group_id":1})),
        crate::application::Action::new(
            "layer.batch",
            json!({"action":"layer.delete","layer_ids":[1,2]}),
        ),
    ] {
        app.editor.dispatch_layer_pending(vec![action]);
        assert!(app.editor.status.contains("mask_editing_pending"));
        before.assert_same(&app.editor);
    }
}
