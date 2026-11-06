//! Hierarchy UI keeps owner masks and native contacts under shared admission.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{click, fixture, label, Snapshot};
use crate::engine::raster_masks::{MaskState, PlaneView};
use crate::ui::{keymap::Kind, palette::Mode};

fn studio() -> Studio {
    let (mut app, _) = fixture(2);
    for color in ["#E5A4C4", "#243044"] {
        assert!(app.editor.core("layer.add", json!({})));
        assert!(app.editor.core("layer.fill_color", json!({"color":color})));
    }
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":2,"opacity":137./255.})));
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":3,"visible":false})));
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":1})
    ));
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":1}})));
    assert!(app.editor.core(
        "mask.set",
        json!({"mask_id":2,"density":177,"feather_radius_px":1,"linked":false})
    ));
    assert!(app
        .editor
        .core("mask.fill", json!({"mask_id":2,"value":190})));
    assert!(app.editor.core(
        "brush.set",
        json!({"radius":1.,"color":"#000000","stabilizer":0.55})
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
fn pointer(kind: ToolKind, phase: PointerPhase, x: f32, pressure: f32, time: u32) -> Pointer {
    Pointer {
        phase,
        x,
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
        .pointer(pointer(kind, PointerPhase::Down, 3., 0.3, 100))
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
fn finish_and_check_samples(app: &mut Studio, kind: ToolKind, start: usize) {
    app.editor
        .session
        .pointer(pointer(kind, PointerPhase::Move, 6., 0.9, 120))
        .unwrap();
    app.editor
        .session
        .pointer(pointer(kind, PointerPhase::Up, 9., 0.4, 140))
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
fn raw_masks(app: &Studio) -> (MaskState, Vec<(u64, Vec<u8>, Option<usize>)>) {
    let doc = &app.editor.session.doc;
    (
        doc.mask_state().clone(),
        doc.mask_state()
            .items
            .iter()
            .map(|mask| {
                let (data, pointer) = match doc.mask_plane(mask.id).unwrap() {
                    PlaneView::Uniform(value) => (vec![value; doc.width * doc.height], None),
                    PlaneView::Dense(data) => (data.to_vec(), Some(data.as_ptr() as usize)),
                };
                (mask.id, data, pointer)
            })
            .collect(),
    )
}
fn raster_pointers(app: &Studio) -> std::collections::BTreeMap<u64, usize> {
    app.editor
        .session
        .doc
        .layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            (
                app.editor.session.doc.layer_id(index),
                layer.pixels.as_ptr() as usize,
            )
        })
        .collect()
}

#[test]
fn pending_group_structure_checks_idle_before_names_and_preserves_every_sample() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for action in [
                crate::application::Action::new(
                    "group.create",
                    json!({"members":[],"parent_id":1}),
                ),
                crate::application::Action::new("group.create_selected", json!({})),
                crate::application::Action::new(
                    "group.move",
                    json!({"items":[{"kind":"layer","id":1}],"parent_id":1,"index":0}),
                ),
            ] {
                let mut app = studio();
                let start = down(&mut app, kind, local);
                let before = Snapshot::capture(&app.editor);
                let expected_samples = samples(&mut app, start);
                let ctx = Context::default();
                app.editor.dispatch_layer_pending_ui(
                    vec![
                        crate::application::Action::new(
                            "layer.rename",
                            json!({"layer_id":1,"name":"Hierarchy-local draft"}),
                        ),
                        action,
                    ],
                    Some(&ctx),
                );
                before.assert_same(&app.editor);
                assert_contact(&app, kind, local);
                assert!(app.editor.status.contains("stroke_active"));
                assert_eq!(samples(&mut app, start), expected_samples);
                assert_eq!(
                    ctx.data_mut(|data| data.get_temp::<String>(Id::new((
                        "layer-name",
                        app.editor.session.doc.identity,
                        1u64
                    )))),
                    Some("Hierarchy-local draft".into())
                );
                finish_and_check_samples(&mut app, kind, start);
            }
        }
    }
}

#[test]
fn ctrl_g_queued_apart_from_a_name_draft_reaches_shared_idle_admission() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            app.editor.settings.keys = crate::ui::keymap::Keymap::default();
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let ctx = context(&app);
            let _ = ctx.run(
                RawInput {
                    events: key(
                        Key::G,
                        Modifiers {
                            ctrl: true,
                            ..Modifiers::NONE
                        },
                    ),
                    focused: true,
                    ..Default::default()
                },
                |ctx| app.editor.shortcuts(ctx),
            );
            assert_eq!(app.editor.actions, vec![Action::UiCommand(10200)]);
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Shortcut-local draft"}),
                )],
                Some(&ctx),
            );
            before.assert_same(&app.editor);
            app.run_actions(&ctx);
            before.assert_same(&app.editor);
            assert_contact(&app, kind, local);
            assert!(app.editor.status.contains("stroke_active"));
            finish_and_check_samples(&mut app, kind, start);
        }
    }
}

#[test]
fn hierarchy_forms_keep_local_and_external_contacts_during_drafts_run_and_cancel() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            app.editor.register_commands();
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let ctx = context(&app);
            for (index, action) in ["group.create", "group.create_selected", "group.move"]
                .into_iter()
                .enumerate()
            {
                let command = app
                    .editor
                    .settings
                    .keys
                    .commands()
                    .find(|command| command.keywords == action)
                    .unwrap()
                    .ui;
                app.editor.actions.push(Action::UiCommand(command));
                app.run_actions(&ctx);
                assert!(app.editor.guarded_command_form());
                app.editor.dispatch_layer_pending_ui(
                    vec![crate::application::Action::new(
                        "layer.rename",
                        json!({"layer_id":1,"name":"Form-local draft"}),
                    )],
                    Some(&ctx),
                );
                let base = (index * 10) as f64;
                let mut output = draft_frame(&mut app, &ctx, vec![], base);
                for frame in 1..3 {
                    output = draft_frame(&mut app, &ctx, vec![], base + frame as f64);
                }
                if action != "group.move" {
                    let run = output
                        .shapes
                        .iter()
                        .find_map(|shape| label(&shape.shape, "Run Command"))
                        .unwrap();
                    draft_frame(&mut app, &ctx, click(run), base + 3.);
                    assert!(
                        app.editor.status.contains("stroke_active")
                            || app.editor.command_form.is_some()
                    );
                }
                // Move's required item list remains a draft here. Its valid
                // request is exercised through the pending dispatcher above.
                before.assert_same(&app.editor);
                assert_contact(&app, kind, local);
                app.editor.actions.push(Action::Cancel);
                app.run_actions(&ctx);
                assert!(app.editor.command_form.is_none());
                before.assert_same(&app.editor);
            }
            finish_and_check_samples(&mut app, kind, start);
        }
    }
}

fn toolbar_frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(crate::i18n::Language::English);
    let mut pending = Vec::new();
    let output = ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1100., 1000.))),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.group_toolbar_ui(ui, &mut pending));
        },
    );
    app.editor.dispatch_layer_pending_ui(pending, Some(ctx));
    output
}
#[test]
fn actual_group_toolbar_preserves_masks_through_create_wrap_and_selected_move() {
    for title in [
        "Group Selected Layers",
        "New Empty Group",
        "Wrap in New Group",
        "Move Selected Layers Here",
    ] {
        let mut app = studio();
        let ctx = context(&app);
        if title != "Group Selected Layers" {
            app.editor.focus_group_ui(&ctx, Some(1));
        }
        let masks = raw_masks(&app);
        let pointers = raster_pointers(&app);
        let target = app.editor.session.doc.mask_state().target;
        let active = app
            .editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active);
        let selected = app.editor.session.doc.selected_layer_ids();
        let groups = app.editor.session.doc.groups_state();
        let mut output = toolbar_frame(&mut app, &ctx, vec![], 0.);
        for time in 1..3 {
            output = toolbar_frame(&mut app, &ctx, vec![], time as f64);
        }
        let button = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, "\u{e53b}"))
            .unwrap();
        toolbar_frame(&mut app, &ctx, click(button), 3.);
        output = toolbar_frame(&mut app, &ctx, vec![], 4.);
        let option = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, title))
            .unwrap();
        toolbar_frame(&mut app, &ctx, click(option), 5.);
        assert_eq!(raw_masks(&app), masks);
        assert_eq!(raster_pointers(&app), pointers);
        assert_eq!(app.editor.session.doc.mask_state().target, target);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            active
        );
        assert_eq!(app.editor.session.doc.selected_layer_ids(), selected);
        assert_ne!(app.editor.session.doc.groups_state(), groups);
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.groups_state(), groups);
        assert_eq!(raw_masks(&app), masks);
    }
}

#[test]
fn actual_group_destination_menu_reparents_a_masked_group_with_its_existing_owner() {
    let mut app = studio();
    assert!(app.editor.core(
        "group.move",
        json!({"items":[{"kind":"layer","id":1}],"parent_id":1,"index":0})
    ));
    assert!(app.editor.core(
        "group.create",
        json!({"members":[],"parent_id":0,"name":"Destination"})
    ));
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":2}})));
    let ctx = context(&app);
    app.editor.focus_group_ui(&ctx, Some(1));
    let masks = raw_masks(&app);
    let pointers = raster_pointers(&app);
    let groups = app.editor.session.doc.groups_state();
    let frame = |app: &mut Studio, events, time| {
        crate::i18n::set_language(crate::i18n::Language::English);
        let mut pending = Vec::new();
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
        app.editor.dispatch_layer_pending_ui(pending, Some(&ctx));
        output
    };
    let mut output = frame(&mut app, vec![], 0.);
    for time in 1..3 {
        output = frame(&mut app, vec![], time as f64);
    }
    let button = output
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Group Actions"))
        .unwrap();
    frame(&mut app, click(button), 3.);
    output = frame(&mut app, vec![], 4.);
    let submenu = output
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Move to Group"))
        .unwrap();
    frame(&mut app, click(submenu), 5.);
    output = frame(&mut app, vec![], 6.);
    let destination = output
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Destination"))
        .unwrap();
    frame(&mut app, click(destination), 7.);
    let updated = app.editor.session.doc.groups_state();
    assert_eq!(
        updated["groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["id"] == 1)
            .unwrap()["parent_id"],
        2
    );
    assert_eq!(raw_masks(&app), masks);
    assert_eq!(raster_pointers(&app), pointers);
    assert!(app.editor.core("history.undo", json!({})));
    assert_eq!(app.editor.session.doc.groups_state(), groups);
    assert_eq!(raw_masks(&app), masks);
}

#[test]
fn guarded_ungroup_shortcut_remains_unavailable_without_ending_a_contact() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        let mut app = studio();
        let start = down(&mut app, kind, true);
        let before = Snapshot::capture(&app.editor);
        app.editor.actions.push(Action::UiCommand(10201));
        app.run_actions(&Context::default());
        before.assert_same(&app.editor);
        assert_contact(&app, kind, true);
        assert!(app.editor.status.contains("mask_editing_pending"));
        finish_and_check_samples(&mut app, kind, start);
    }
}

fn palette_select(app: &mut Studio, action: &str) {
    let index = app
        .editor
        .settings
        .keys
        .commands()
        .enumerate()
        .find_map(|(index, command)| (command.keywords == action).then_some(index))
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
fn real_command_palette_preserves_contacts_on_cancel_and_guarded_selection_only() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio();
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let history = app.editor.session.doc.history_bytes();
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
            assert!(app.editor.command_palette_open());
            before.assert_document(&app.editor);
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
            assert!(app.editor.command_palette_open());
            assert_contact(&app, kind, local);
            draft_frame(&mut app, &ctx, key(Key::Escape, Modifiers::NONE), 2.);
            assert!(app.editor.palette.is_none());
            assert_contact(&app, kind, local);
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
            draft_frame(&mut app, &ctx, vec![], 3.);
            palette_select(&mut app, "group.move");
            draft_frame(&mut app, &ctx, key(Key::Enter, Modifiers::NONE), 4.);
            assert!(app.editor.palette.is_none());
            app.run_actions(&ctx);
            assert!(app.editor.guarded_command_form());
            draft_frame(&mut app, &ctx, vec![], 5.);
            before.assert_document(&app.editor);
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_contact(&app, kind, local);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            assert!(app.editor.command_form.is_none());
            finish_and_check_samples(&mut app, kind, start);

            down(&mut app, kind, local);
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
            draft_frame(&mut app, &ctx, vec![], 6.);
            let index = app
                .editor
                .settings
                .keys
                .commands()
                .enumerate()
                .find_map(|(index, command)| {
                    matches!(command.kind, Kind::Press(Action::Fit)).then_some(index)
                })
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
            draft_frame(&mut app, &ctx, key(Key::Enter, Modifiers::NONE), 7.);
            app.run_actions(&ctx);
            assert!(!app.editor.session.stroke.active());
            assert!(app.editor.session.pointer_checkpoint.is_none());
            assert!(!app.editor.input_owned && !app.editor.pen_active);
        }
    }
}

#[test]
fn command_palette_explicit_cancel_is_inert_and_binding_mode_keeps_old_cleanup() {
    let mut app = studio();
    let ctx = context(&app);
    down(&mut app, ToolKind::Brush, true);
    app.editor.actions.push(Action::OpenPalette);
    app.run_actions(&ctx);
    let before = Snapshot::capture(&app.editor);
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    before.assert_same(&app.editor);
    assert_contact(&app, ToolKind::Brush, true);
    app.editor.actions.push(Action::OpenPalette);
    app.run_actions(&ctx);
    draft_frame(&mut app, &ctx, vec![], 0.);
    draft_frame(&mut app, &ctx, key(Key::Tab, Modifiers::NONE), 1.);
    assert!(app.editor.palette.as_ref().unwrap().mode == Mode::Bindings);
    assert!(!app.editor.session.stroke.active());
    assert!(!app.editor.input_owned && !app.editor.pen_active);
}
