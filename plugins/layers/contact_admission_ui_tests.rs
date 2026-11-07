//! UI entry, draft sync and cancellation preserve shared contact admission.
use super::group_delete_ui_tests::{assert_contact, context, down, finish, raw, samples, studio};
use super::*;
use crate::application::ToolKind;
use crate::desktop::canvas_geometry::mask_ui_tests::Snapshot;
use crate::engine::raster_masks::PlaneView;
use std::collections::BTreeMap;

fn command(app: &Studio, id: &str) -> u32 {
    app.editor
        .settings
        .keys
        .commands()
        .find(|command| command.keywords == id)
        .unwrap()
        .ui
}
fn declare(app: &mut Studio, id: &str, steps: Value) {
    let definition = serde_json::from_value(json!({"id":id,"title":id,"steps":steps})).unwrap();
    app.editor
        .session
        .registry
        .actions
        .insert(id.into(), definition);
}
fn pending(ctx: &Context, app: &Studio, id: u64) -> Id {
    let key = Id::new(("layer-name", app.editor.session.doc.identity, id));
    ctx.data_mut(|data| data.insert_temp(key, format!("Pending raster {id}")));
    key
}
struct Before {
    document: Snapshot,
    raw: BTreeMap<u64, (Vec<u8>, Option<usize>)>,
    pending: Vec<(Id, String)>,
    samples: Vec<Value>,
}
impl Before {
    fn capture(app: &mut Studio, ctx: &Context, start: usize) -> Self {
        let names = [1, 2]
            .into_iter()
            .map(|id| {
                let key = pending(ctx, app, id);
                (key, format!("Pending raster {id}"))
            })
            .collect();
        Self {
            document: Snapshot::capture(&app.editor),
            raw: app
                .editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .map(|mask| {
                    let pointer = match app.editor.session.doc.mask_plane(mask.id).unwrap() {
                        PlaneView::Dense(data) => Some(data.as_ptr() as usize),
                        PlaneView::Uniform(_) => None,
                    };
                    (mask.id, (raw(app, mask.id), pointer))
                })
                .collect(),
            pending: names,
            samples: samples(app, start),
        }
    }
    fn unchanged(
        &self,
        app: &mut Studio,
        ctx: &Context,
        start: usize,
        kind: ToolKind,
        local: bool,
    ) {
        self.document.assert_same(&app.editor);
        for (id, (bytes, pointer)) in &self.raw {
            assert_eq!(&raw(app, *id), bytes);
            let current = match app.editor.session.doc.mask_plane(*id).unwrap() {
                PlaneView::Dense(data) => Some(data.as_ptr() as usize),
                PlaneView::Uniform(_) => None,
            };
            assert_eq!(&current, pointer);
        }
        for (id, text) in &self.pending {
            assert_eq!(
                ctx.data_mut(|data| data.get_temp::<String>(*id)).as_ref(),
                Some(text)
            );
        }
        assert_contact(app, kind, local);
        assert_eq!(samples(app, start), self.samples);
    }
}
fn form_frame(app: &mut Studio, ctx: &Context, time: f64) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 1000.))),
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.editor.plugin_command_window(ctx),
    )
}

#[test]
fn contact_admission_dynamic_readonly_entries_preserve_brush_hand_and_external_samples() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for id in ["canvas.state", "mask.snapshot", "native.stats"] {
                let mut app = studio(true);
                let ctx = context(&app);
                app.editor.register_commands();
                let start = down(&mut app, kind, local);
                let before = Before::capture(&mut app, &ctx, start);
                let entry = command(&app, id);
                app.editor.actions.push(Action::UiCommand(entry));
                app.run_actions(&ctx);
                assert!(
                    app.editor.command_form.is_some(),
                    "Dynamic entry must open its argument form: {id}"
                );
                before.unchanged(&mut app, &ctx, start, kind, local);
                finish(&mut app, kind, start);
            }
        }
    }
}

#[test]
fn contact_admission_guarded_macro_entries_reach_the_backend_before_ui_cleanup() {
    for nested in [false, true] {
        for kind in [ToolKind::Brush, ToolKind::Hand] {
            for local in [false, true] {
                let mut app = studio(true);
                let ctx = context(&app);
                declare(
                    &mut app,
                    "test.contact_guard",
                    json!([{"action":"group.duplicate","args":{"group_id":1}}]),
                );
                let id = if nested {
                    declare(
                        &mut app,
                        "test.contact_outer",
                        json!([{"action":"test.contact_guard","args":{}}]),
                    );
                    "test.contact_outer"
                } else {
                    "test.contact_guard"
                };
                app.editor.register_commands();
                let start = down(&mut app, kind, local);
                let before = Before::capture(&mut app, &ctx, start);
                let error = app
                    .editor
                    .session
                    .apply(crate::application::Action::new(id, json!({})))
                    .unwrap_err();
                assert_eq!(error.code, "stroke_active");
                before.unchanged(&mut app, &ctx, start, kind, local);
                let entry = command(&app, id);
                app.editor.actions.push(Action::UiCommand(entry));
                app.run_actions(&ctx);
                assert!(app.editor.status.contains("stroke_active"));
                before.unchanged(&mut app, &ctx, start, kind, local);
                finish(&mut app, kind, start);
            }
        }
    }
}

#[test]
fn contact_admission_readonly_form_frames_do_not_publish_pending_parameters_or_view() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            app.editor.begin_plugin_command("canvas.state");
            assert!(app.editor.command_form.is_some());
            let before = Before::capture(&mut app, &ctx, start);
            app.editor.radius += 2.;
            app.editor.pan += vec2(3., 2.);
            app.editor.fit = false;
            for time in [0., 1., 2.] {
                form_frame(&mut app, &ctx, time);
            }
            before.unchanged(&mut app, &ctx, start, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn contact_admission_readonly_form_cancel_preserves_native_contact_and_name_drafts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            app.editor.begin_plugin_command("mask.snapshot");
            let before = Before::capture(&mut app, &ctx, start);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            assert!(app.editor.command_form.is_none());
            before.unchanged(&mut app, &ctx, start, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

fn alias(app: &mut Studio, id: &str, operation: &str, args: Value) {
    let properties: serde_json::Map<String, Value> = args
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            let kind = match value {
                Value::Array(_) => "array",
                Value::Object(_) => "object",
                Value::Bool(_) => "boolean",
                Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
                Value::Number(_) => "number",
                _ => "string",
            };
            (key.clone(), json!({"type":kind,"default":value}))
        })
        .collect();
    let definition=serde_json::from_value(json!({"id":id,"title":id,"operation":operation,"input_schema":{"type":"object","properties":properties,"required":args.as_object().unwrap().keys().collect::<Vec<_>>(),"additionalProperties":false}})).unwrap();
    app.editor
        .session
        .registry
        .actions
        .insert(id.into(), definition);
}
fn enter(app: &mut Studio, ctx: &Context, id: &str) {
    app.editor.register_commands();
    let id = command(app, id);
    app.editor.actions.push(Action::UiCommand(id));
    app.run_actions(ctx);
}
fn form_events(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1280., 1000.))),
            time: Some(time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| app.editor.plugin_command_window(ctx),
    )
}
fn run_form(app: &mut Studio, ctx: &Context) -> FullOutput {
    use super::group_delete_ui_tests::option;
    use crate::desktop::canvas_geometry::mask_ui_tests::click;
    let mut output = form_events(app, ctx, vec![], 0.);
    for time in [1., 2.] {
        output = form_events(app, ctx, vec![], time);
    }
    form_events(app, ctx, click(option(&output, "Run Command")), 3.);
    form_events(app, ctx, vec![], 4.)
}
fn labels(output: &FullOutput) -> String {
    super::group_delete_ui_tests::rendered_labels(output).join("\n")
}
fn ended(app: &mut Studio, start: usize) {
    assert!(!app.editor.session.stroke.active());
    assert!(app.editor.session.pointer_checkpoint.is_none());
    assert!(!app.editor.input_owned);
    assert!(!app.editor.pen_active);
    assert!(!app.editor.drawing);
    assert!(app.editor.panning.is_none());
    let actual = samples(app, start);
    assert_eq!(actual.len(), 2);
    assert_eq!(actual[0]["phase"], "down");
    assert_eq!(actual[0]["time"], 100);
    assert_eq!(actual[0]["pen"], true);
    assert!((actual[0]["pressure"].as_f64().unwrap() - 0.3).abs() < 1e-6);
    // Existing Session::finish_stroke synthesizes UP from the last sample;
    // reconciliation after the backend is inactive must not add another one.
    let mut up = actual[0].clone();
    up["phase"] = json!("up");
    assert_eq!(actual[1], up);
    assert_eq!(
        app.editor
            .session
            .apply(crate::application::Action::new("replay.verify", json!({})))
            .unwrap()["result"]["valid"],
        true
    );
}
fn remove_masks(app: &mut Studio) {
    let ids: Vec<_> = app
        .editor
        .session
        .doc
        .mask_state()
        .items
        .iter()
        .map(|mask| mask.id)
        .collect();
    for id in ids {
        assert!(app.editor.core("mask.delete", json!({"mask_id":id})));
    }
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
}

#[test]
fn contact_admission_readonly_query_forms_execute_through_the_canvas_modal_route() {
    for (id, evidence) in [
        ("canvas.state", "\"revision\":"),
        ("mask.snapshot", "\"encoding\": \"a8\""),
        ("native.stats", "\"calls\":"),
    ] {
        for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            let before = Before::capture(&mut app, &ctx, start);
            enter(&mut app, &ctx, id);
            super::group_delete_ui_tests::draft_frame(&mut app, &ctx, vec![], 0.);
            let output = run_form(&mut app, &ctx);
            assert!(labels(&output).contains(evidence), "{}", labels(&output));
            before.unchanged(&mut app, &ctx, start, kind, local);
            app.editor.actions.push(Action::Cancel);
            app.run_actions(&ctx);
            before.unchanged(&mut app, &ctx, start, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn contact_admission_readonly_prefix_and_allreadonly_macros_keep_pending_names_and_contacts() {
    for readonly in [false, true] {
        for kind in [ToolKind::Brush, ToolKind::Hand] {
            for local in [false, true] {
                let mut app = studio(true);
                let ctx = context(&app);
                declare(
                    &mut app,
                    "test.observed_prefix",
                    if readonly {
                        json!([{"action":"canvas.state"},{"action":"native.stats"}])
                    } else {
                        json!([{"action":"canvas.state"},{"action":"group.duplicate","args":{"group_id":1}}])
                    },
                );
                app.editor.register_commands();
                let start = down(&mut app, kind, local);
                let mut before = Before::capture(&mut app, &ctx, start);
                let entry = command(&app, "test.observed_prefix");
                app.editor.actions.push(Action::UiCommand(entry));
                app.editor.dispatch_layer_pending_ui(
                    vec![crate::application::Action::new(
                        "layer.rename",
                        json!({"layer_id":1,"name":"Deferred macro name"}),
                    )],
                    Some(&ctx),
                );
                before.pending[0].1 = "Deferred macro name".into();
                app.run_actions(&ctx);
                if !readonly {
                    assert!(app.editor.status.contains("stroke_active"));
                }
                before.unchanged(&mut app, &ctx, start, kind, local);
                finish(&mut app, kind, start);
            }
        }
    }
}

#[test]
fn contact_admission_invalid_declared_macros_reject_without_canceling_input() {
    for case in 0..4 {
        for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
            let mut app = studio(true);
            let ctx = context(&app);
            let steps = match case {
                0 => json!([{"action":"not.registered"}]),
                1 => json!([{"action":"group.duplicate","args":{"group_id":0}}]),
                2 => {
                    declare(
                        &mut app,
                        "test.contact_cycle_b",
                        json!([{"action":"test.contact_invalid"}]),
                    );
                    json!([{"action":"test.contact_cycle_b"}])
                }
                _ => Value::Array((0..257).map(|_| json!({"action":"canvas.state"})).collect()),
            };
            declare(&mut app, "test.contact_invalid", steps);
            app.editor.register_commands();
            let start = down(&mut app, kind, local);
            let before = Before::capture(&mut app, &ctx, start);
            let baseline = app
                .editor
                .session
                .apply(crate::application::Action::new(
                    "test.contact_invalid",
                    json!({}),
                ))
                .unwrap_err();
            assert!(matches!(
                baseline.code.as_str(),
                "unknown_action" | "invalid_arguments" | "action_limit"
            ));
            before.unchanged(&mut app, &ctx, start, kind, local);
            enter(&mut app, &ctx, "test.contact_invalid");
            assert!(app.editor.status.contains(&baseline.code));
            before.unchanged(&mut app, &ctx, start, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn contact_admission_legacy_tool_and_parameter_prefixes_still_end_the_contact_intentionally() {
    for parameter in [false, true] {
        for failure in [false, true] {
            for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
                let mut app = studio(true);
                let ctx = context(&app);
                let first = if parameter {
                    json!({"action":"brush.set","args":{"radius":2.}})
                } else {
                    json!({"action":"tool.select","args":{"tool":"picker"}})
                };
                declare(
                    &mut app,
                    "test.legacy_prefix",
                    json!([first,{"action":"group.duplicate","args":{"group_id":if failure{999999}else{1}}}]),
                );
                app.editor.register_commands();
                let start = down(&mut app, kind, local);
                let layers = app.editor.session.doc.layers.len();
                enter(&mut app, &ctx, "test.legacy_prefix");
                ended(&mut app, start);
                assert_eq!(
                    app.editor.session.doc.layers.len(),
                    layers + if failure { 0 } else { 2 }
                );
                if parameter {
                    assert_eq!(app.editor.session.parameters.radius, 2.);
                } else {
                    assert_eq!(app.editor.session.selected_tool, "picker");
                }
                assert_eq!(app.editor.tool, app.editor.session.tool);
                assert_eq!(app.editor.radius, app.editor.session.parameters.radius);
                if failure {
                    assert!(app.editor.status.contains("group_edit"));
                }
            }
        }
    }
}

#[test]
fn contact_admission_direct_shared_and_conditional_guards_run_before_cleanup() {
    for (operation, args, code) in [
        (
            "group_properties",
            json!({"group_id":1,"opacity":0.5}),
            "stroke_active",
        ),
        (
            "mask_fill",
            json!({"mask_id":1,"value":17}),
            "stroke_active",
        ),
        (
            "layer_properties",
            json!({"layer_id":1,"pixel_locked":true}),
            "stroke_active",
        ),
        (
            "move_pixels",
            json!({"dx":1,"dy":0,"target":"layer"}),
            "stroke_active",
        ),
    ] {
        for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
            let mut app = studio(true);
            let ctx = context(&app);
            alias(&mut app, "test.shared_guard", operation, args.clone());
            app.editor.register_commands();
            let start = down(&mut app, kind, local);
            let before = Before::capture(&mut app, &ctx, start);
            let error = app
                .editor
                .session
                .apply(crate::application::Action::new(
                    "test.shared_guard",
                    args.clone(),
                ))
                .unwrap_err();
            assert_eq!(error.code, code);
            before.unchanged(&mut app, &ctx, start, kind, local);
            enter(&mut app, &ctx, "test.shared_guard");
            let output = run_form(&mut app, &ctx);
            assert!(labels(&output).contains(code), "{}", labels(&output));
            before.unchanged(&mut app, &ctx, start, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn contact_admission_conditional_layer_success_and_clipboard_error_use_actual_backend_transition() {
    for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
        let mut app = studio(true);
        let ctx = context(&app);
        alias(
            &mut app,
            "test.layer_opacity",
            "layer_properties",
            json!({"layer_id":1,"opacity":0.5}),
        );
        let start = down(&mut app, kind, local);
        enter(&mut app, &ctx, "test.layer_opacity");
        run_form(&mut app, &ctx);
        ended(&mut app, start);
        assert_eq!(
            app.editor.session.doc.layers[app.editor.session.doc.layer_index(1).unwrap()].opacity,
            128
        );
        let mut app = studio(true);
        remove_masks(&mut app);
        let ctx = context(&app);
        app.editor.register_commands();
        let start = down(&mut app, kind, local);
        let before = Before::capture(&mut app, &ctx, start);
        enter(&mut app, &ctx, "clipboard.paste");
        let output = run_form(&mut app, &ctx);
        assert!(labels(&output).contains("Session clipboard is empty"));
        before.unchanged(&mut app, &ctx, start, kind, local);
        finish(&mut app, kind, start);
    }
}

#[test]
fn contact_admission_runtime_pipeline_and_workflow_forms_delegate_contact_checks() {
    for workflow in [false, true] {
        for case in 0..4 {
            for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
                let mut app = studio(true);
                let ctx = context(&app);
                let guarded = case == 1;
                let atomic = case >= 2;
                let dry = case == 3;
                let args = if workflow {
                    json!({"atomic":atomic,"dry_run":dry,"graph":{"version":1,"nodes":[{"id":1,"type":if guarded{"action.group.duplicate"}else{"action.canvas.state"},"position":[0,0],"parameters":if guarded{json!({"group_id":1})}else{json!({})}}],"edges":[]}})
                } else {
                    json!({"atomic":atomic,"dry_run":dry,"steps":[{"id":"leaf","action":if guarded{"group.duplicate"}else{"canvas.state"},"args":if guarded{json!({"group_id":1})}else{json!({})}}]})
                };
                alias(
                    &mut app,
                    "test.runtime_admission",
                    if workflow {
                        "workflow_run"
                    } else {
                        "pipeline_run"
                    },
                    args.clone(),
                );
                app.editor.register_commands();
                let start = down(&mut app, kind, local);
                let before = Before::capture(&mut app, &ctx, start);
                // Runtime containers are not classified by guessing their graph: the
                // existing backend rejects atomic admission, including dry runs.
                enter(&mut app, &ctx, "test.runtime_admission");
                let output = run_form(&mut app, &ctx);
                if atomic || guarded {
                    assert!(
                        labels(&output).contains("stroke_active"),
                        "{}",
                        labels(&output)
                    );
                } else {
                    assert!(labels(&output).contains("\"ok\": true"));
                }
                before.unchanged(&mut app, &ctx, start, kind, local);
                finish(&mut app, kind, start);
            }
        }
    }
}

#[test]
fn contact_admission_runtime_legacy_prefix_reconciles_flags_even_after_partial_failure() {
    for parameter in [false, true] {
        for failure in [false, true] {
            for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
                let mut app = studio(true);
                let ctx = context(&app);
                let prefix = if parameter {
                    json!({"id":"prefix","action":"brush.set","args":{"radius":2.}})
                } else {
                    json!({"id":"prefix","action":"tool.select","args":{"tool":"picker"}})
                };
                alias(
                    &mut app,
                    "test.runtime_prefix",
                    "pipeline_run",
                    json!({"steps":[prefix,{"id":"last","action":"group.duplicate","args":{"group_id":if failure{999999}else{1}}}]}),
                );
                let start = down(&mut app, kind, local);
                let layers = app.editor.session.doc.layers.len();
                enter(&mut app, &ctx, "test.runtime_prefix");
                let output = run_form(&mut app, &ctx);
                ended(&mut app, start);
                assert_eq!(
                    app.editor.session.doc.layers.len(),
                    layers + if failure { 0 } else { 2 }
                );
                if failure {
                    assert!(labels(&output).contains("group_edit"));
                } else {
                    assert!(labels(&output).contains("\"ok\": true"));
                }
                if parameter {
                    assert_eq!(app.editor.radius, 2.);
                } else {
                    assert_eq!(app.editor.session.selected_tool, "picker");
                }
            }
        }
    }
}

#[test]
fn contact_admission_readonly_run_accepts_a_genuine_external_move_and_close_discards_only_local_sync(
) {
    use crate::application::{Pointer, PointerPhase};
    for (kind, local) in [(ToolKind::Brush, true), (ToolKind::Hand, false)] {
        let mut app = studio(true);
        let ctx = context(&app);
        let start = down(&mut app, kind, local);
        app.editor.begin_plugin_command("canvas.state");
        let before_move = app.editor.session.revision;
        app.editor
            .session
            .pointer(Pointer {
                phase: PointerPhase::Move,
                x: 5.5,
                y: 4.,
                pressure: 0.6,
                time: 110,
                pen: true,
                eraser: false,
                tool_override: Some(kind),
                view_x: Some(55.),
                view_y: Some(40.),
            })
            .unwrap();
        app.editor.sync_from_session();
        assert!(app.editor.session.revision > before_move);
        let before = Before::capture(&mut app, &ctx, start);
        let current = app.editor.session.revision;
        app.editor.radius += 3.;
        app.editor.pan += vec2(4., 2.);
        app.editor.fit = false;
        let output = run_form(&mut app, &ctx);
        assert!(
            labels(&output).contains(&format!("\"revision\": {current}")),
            "{}",
            labels(&output)
        );
        before.unchanged(&mut app, &ctx, start, kind, local);
        // Read-only execution syncs visible parameters from Session. Make a new
        // local draft to prove closing cannot defer an automatic mutation.
        app.editor.radius += 2.;
        app.editor.pan += vec2(3., 1.);
        app.editor.fit = false;
        app.editor.actions.push(Action::Cancel);
        app.run_actions(&ctx);
        app.editor.push_gui_parameters();
        app.editor.push_gui_view();
        before.unchanged(&mut app, &ctx, start, kind, local);
        app.editor
            .session
            .pointer(Pointer {
                phase: PointerPhase::Up,
                x: 5.5,
                y: 4.,
                pressure: 0.4,
                time: 140,
                pen: true,
                eraser: false,
                tool_override: Some(kind),
                view_x: Some(55.),
                view_y: Some(40.),
            })
            .unwrap();
        assert_eq!(
            samples(&mut app, start)
                .iter()
                .map(|sample| sample["time"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![100, 110, 140]
        );
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
    }
}

#[test]
fn contact_admission_observation_does_not_cancel_idle_backend_local_gesture_previews() {
    for selection in [false, true] {
        let mut app = studio(true);
        remove_masks(&mut app);
        let ctx = context(&app);
        if selection {
            app.editor.area_mode = 4;
            assert!(app
                .editor
                .selection_down(pos2(2., 2.), Modifiers::NONE, 100));
            assert!(app.editor.selection_move(pos2(5., 5.), false));
        } else {
            app.editor.area_mode = 8;
            assert!(app.editor.movement_down(pos2(5., 4.)));
            assert!(app.editor.movement_gesture.is_some());
            assert!(app.editor.movement_move(pos2(6., 5.), false));
        }
        assert!(app.editor.input_owned);
        assert!(!app.editor.session.stroke.active());
        let before = Snapshot::capture(&app.editor);
        enter(&mut app, &ctx, "canvas.state");
        run_form(&mut app, &ctx);
        app.editor.actions.push(Action::Cancel);
        app.run_actions(&ctx);
        before.assert_same(&app.editor);
        assert!(app.editor.input_owned);
        assert!(!app.editor.session.stroke.active());
        if selection {
            assert!(app.editor.selection_gesture.is_some());
            assert!(app.editor.selection_move(pos2(6., 5.), false));
        } else {
            assert!(app.editor.movement_gesture.is_some());
            assert!(app.editor.movement_move(pos2(7., 5.), false));
        }
        app.editor.finish_stroke();
    }
}

#[test]
fn contact_admission_runtime_end_restart_retains_external_contact_but_releases_old_local_ownership()
{
    use crate::application::{Pointer, PointerPhase};
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for parameters in [false, true] {
            let prefix = if parameters {
                json!({"id":"prefix","action":"brush.set","args":{"radius":2.}})
            } else {
                json!({"id":"prefix","action":"tool.select","args":{"tool":"eraser"}})
            };
            let args = json!({"steps":[prefix,{"id":"external","action":"canvas.pointer","args":{"phase":"down","x":9.,"y":6.,"pressure":0.7,"time":200,"pen":true,"tool_override":"brush","view_x":70.,"view_y":50.}}]});
            let observe = json!({"steps":[{"id":"read","action":"canvas.state"}]});
            let mut reference = studio(true);
            let reference_start = down(&mut reference, kind, true);
            assert_eq!(
                reference
                    .editor
                    .session
                    .apply(crate::application::Action::new(
                        "pipeline.run",
                        observe.clone()
                    ))
                    .unwrap()["result"]["ok"],
                true
            );
            assert_eq!(
                reference
                    .editor
                    .session
                    .apply(crate::application::Action::new(
                        "pipeline.run",
                        args.clone()
                    ))
                    .unwrap()["result"]["ok"],
                true
            );
            let expected_samples = samples(&mut reference, reference_start);
            let expected_raw: BTreeMap<_, _> = reference
                .editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .map(|m| (m.id, raw(&reference, m.id)))
                .collect();
            let expected_tool = reference.editor.session.selected_tool.clone();
            let expected_parameters = reference.editor.session.parameters.clone();
            let mut app = studio(true);
            let ctx = context(&app);
            alias(&mut app, "test.external_restart", "pipeline_run", args);
            let start = down(&mut app, kind, true);
            // A recording boundary can flush a chunk without replacing the local
            // native contact. Live journal length is not a contact identity token.
            let before_flush = samples(&mut app, start);
            assert!(app.editor.core("pipeline.run", observe.clone()));
            assert_contact(&app, kind, true);
            assert_eq!(samples(&mut app, start), before_flush);
            enter(&mut app, &ctx, "test.external_restart");
            let output = run_form(&mut app, &ctx);
            // Long result labels may be clipped by the form. The authoritative
            // proof is the new external DOWN and its unchanged raw transcript.
            let diagnostic = format!(
                "last={:?}; samples={}; rendered={}",
                app.editor.session.stroke.last_sample(),
                serde_json::to_string(&samples(&mut app, start)).unwrap(),
                labels(&output).chars().take(4000).collect::<String>()
            );
            assert_eq!(
                app.editor
                    .session
                    .stroke
                    .last_sample()
                    .map(|sample| sample.time),
                Some(200),
                "{diagnostic}"
            );
            assert!(app.editor.session.stroke.active());
            assert!(app.editor.session.pointer_checkpoint.is_some());
            assert!(app.editor.session.is_drawing());
            assert!(
                app.editor.drawing,
                "External drawing remains visible independently of local ownership"
            );
            assert_eq!(app.editor.session.selected_tool, expected_tool);
            assert_eq!(app.editor.session.parameters, expected_parameters);
            assert_eq!(samples(&mut app, start), expected_samples);
            for (id, bytes) in expected_raw {
                assert_eq!(raw(&app, id), bytes);
            }
            assert_eq!(
                expected_samples
                    .iter()
                    .map(|s| s["phase"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                vec!["down", "up", "down"]
            );
            assert_eq!(
                expected_samples
                    .iter()
                    .map(|s| s["time"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                vec![100, 100, 200]
            );
            assert!(!app.editor.input_owned, "{diagnostic}");
            assert!(!app.editor.pen_active);
            assert!(app.editor.panning.is_none());
            assert!(app.editor.core("pipeline.run", observe));
            assert!(app.editor.session.stroke.active());
            assert_eq!(samples(&mut app, start), expected_samples);
            for (phase, time, pressure, x) in [
                (PointerPhase::Move, 220, 0.9, 10.),
                (PointerPhase::Up, 240, 0.4, 10.5),
            ] {
                app.editor
                    .session
                    .pointer(Pointer {
                        phase,
                        x,
                        y: 6.,
                        pressure,
                        time,
                        pen: true,
                        eraser: false,
                        tool_override: Some(ToolKind::Brush),
                        view_x: Some(72.),
                        view_y: Some(50.),
                    })
                    .unwrap();
            }
            let continued = samples(&mut app, start);
            assert_eq!(&continued[..3], expected_samples.as_slice());
            assert_eq!(
                continued
                    .iter()
                    .map(|s| s["time"].as_u64().unwrap())
                    .collect::<Vec<_>>(),
                vec![100, 100, 200, 220, 240]
            );
            assert_eq!(continued[3]["phase"], "move");
            assert_eq!(continued[4]["phase"], "up");
            assert!(!app.editor.session.stroke.active());
            assert!(app.editor.session.pointer_checkpoint.is_none());
            assert!((continued[2]["pressure"].as_f64().unwrap() - 0.7).abs() < 1e-6);
            assert!((continued[3]["pressure"].as_f64().unwrap() - 0.9).abs() < 1e-6);
            assert!((continued[4]["pressure"].as_f64().unwrap() - 0.4).abs() < 1e-6);
            assert_eq!(
                app.editor
                    .session
                    .apply(crate::application::Action::new("replay.verify", json!({})))
                    .unwrap()["result"]["valid"],
                true
            );
            reference.editor.session.finish_stroke();
        }
    }
}

#[test]
fn contact_admission_real_palette_legacy_and_bindings_transitions_discard_suspended_sync() {
    use super::group_delete_ui_tests::{draft_frame, key};
    for bindings in [false, true] {
        for kind in [ToolKind::Brush, ToolKind::Hand] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, true);
            let names = [pending(&ctx, &app, 1), pending(&ctx, &app, 2)];
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
            assert!(app.editor.command_palette_open());
            let parameters = app.editor.session.parameters.clone();
            let view = app.editor.session.view;
            let revision = app.editor.session.revision;
            app.editor.radius += 5.;
            app.editor.pan += vec2(4., 3.);
            app.editor.fit = false;
            draft_frame(&mut app, &ctx, vec![], 0.);
            if bindings {
                draft_frame(&mut app, &ctx, key(Key::Tab, Modifiers::NONE), 1.);
                assert!(
                    app.editor.palette.as_ref().unwrap().mode == crate::ui::palette::Mode::Bindings
                );
                app.editor.push_gui_parameters();
                app.editor.push_gui_view();
            } else {
                // Select the real registered command and let palette_ui queue its
                // UiCommand; direct begin_plugin_command replacement misses this edge.
                let index = app
                    .editor
                    .settings
                    .keys
                    .commands()
                    .enumerate()
                    .find_map(|(i, c)| (c.keywords == "brush.set").then_some(i))
                    .unwrap();
                let id = app.editor.settings.keys.command(index).id.to_owned();
                let editor = &mut app.editor;
                let palette = editor.palette.as_mut().unwrap();
                palette.query(id, &editor.settings.keys);
                palette.selected = palette.results.iter().position(|i| *i == index).unwrap();
                draft_frame(&mut app, &ctx, key(Key::Enter, Modifiers::NONE), 1.);
                app.run_actions(&ctx);
                assert!(app.editor.command_form.is_some());
                for time in [2., 3., 4.] {
                    form_frame(&mut app, &ctx, time);
                }
            }
            assert_eq!(app.editor.session.parameters, parameters);
            assert_eq!(app.editor.session.view, view);
            assert_eq!(
                app.editor.session.revision,
                revision + 1,
                "Only the intentional synthetic contact UP publishes here"
            );
            for (id, text) in names
                .into_iter()
                .zip(["Pending raster 1", "Pending raster 2"])
            {
                assert_eq!(
                    ctx.data_mut(|data| data.get_temp::<String>(id)),
                    Some(text.into())
                );
            }
            ended(&mut app, start);
        }
    }
}
