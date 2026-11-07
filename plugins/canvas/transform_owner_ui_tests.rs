//! Owner transform drafts preserve exact targets and publish only on Apply.
use super::tests::{click, locate};
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::canvas_geometry::mask_ui_tests::{fixture, Snapshot};
use crate::desktop::tests::frame;
use crate::engine::raster_masks::PaintTarget;
use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
use serde_json::{json, Value};

fn owners() -> Studio {
    let (mut app, _) = fixture(2);
    app.editor.session.runtime = "desktop";
    assert!(
        app.editor
            .core("mask.fill", json!({"mask_id":1,"value":47})),
        "{}",
        app.editor.status
    );
    assert!(app.editor.core("layer.add", json!({})));
    assert!(app.editor.core(
        "selection.shape",
        json!({"kind":"rectangle","x":1,"y":1,"width":3,"height":3})
    ));
    assert!(app
        .editor
        .core("layer.fill_color", json!({"color":"#A46496"})));
    assert!(app.editor.core("selection.deselect", json!({})));
    assert!(app.editor.core(
        "mask.create",
        json!({"owner":{"kind":"layer","id":2},"select":false})
    ));
    assert!(app
        .editor
        .core("mask.fill", json!({"mask_id":3,"value":89})));
    assert!(app
        .editor
        .core("mask.set", json!({"mask_id":3,"linked":false})));
    assert!(app
        .editor
        .core("layer.links.set", json!({"layer_ids":[1,2],"linked":true})));
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1],"active_layer_id":1})
    ));
    assert!(app.editor.core(
        "toolbox.movement.options",
        json!({"target":"layer","step":1,"large_step":10})
    ));
    app.editor.fit = false;
    app.editor.zoom = 8.;
    app.editor.pan = Vec2::ZERO;
    app.editor.push_gui_view();
    app.editor.sync_from_session();
    app
}
fn warm(app: &mut Studio) -> Context {
    let ctx = Context::default();
    for n in 0..4 {
        frame(app, &ctx, vec2(1360., 920.), vec![], n as f64);
    }
    ctx
}
fn assert_open_presentation(app: &Studio, mut before: Value) {
    let revision = before["revision"].as_u64().unwrap();
    before["revision"] = json!(revision + 1);
    before["ui"]["transform.open"] = json!(true);
    assert_eq!(
        app.editor.session.state(),
        before,
        "Opening registered controls may publish only its presentation state and revision"
    );
}
fn raw(app: &Studio, id: u64) -> Vec<u8> {
    let doc = &app.editor.session.doc;
    let plane = doc.mask_plane(id).unwrap();
    (0..doc.width * doc.height)
        .map(|i| plane.sample(i))
        .collect()
}
fn shifted<T: Copy>(input: &[T], dx: i32, dy: i32, fill: T) -> Vec<T> {
    (0..96)
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
fn samples(app: &mut Studio) -> Vec<Value> {
    let journal = app.editor.session.replay_journal(false).unwrap();
    let events = journal.events(0, journal.entries.len()).unwrap();
    let mut all: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    all.extend(
        app.editor
            .session
            .replay_samples
            .iter()
            .map(|p| serde_json::to_value(p).unwrap()),
    );
    all
}
fn pointer(kind: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 5.,
        y: 4.,
        pressure: 0.3,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(kind),
        view_x: Some(50.),
        view_y: Some(40.),
    }
}
fn window(
    app: &mut Studio,
    ctx: &Context,
    size: Vec2,
    events: Vec<Event>,
    time: f64,
) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.editor.transform_window(ctx),
    )
}
fn settle(app: &mut Studio, ctx: &Context) -> FullOutput {
    window(app, ctx, vec2(1360., 920.), vec![], 8.);
    app.editor.transform.as_mut().unwrap().changed = Instant::now() - Duration::from_millis(200);
    window(app, ctx, vec2(1360., 920.), vec![], 8.1)
}

#[test]
fn owner_transform_linked_preview_is_reversible_and_apply_moves_exact_rgba_mask_pair_once() {
    let mut app = owners();
    let ctx = warm(&mut app);
    let pixels: Vec<_> = app
        .editor
        .session
        .doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect();
    let mask = raw(&app, 1);
    let fixed_mask = raw(&app, 3);
    let group_mask = raw(&app, 2);
    let selection = app.editor.session.doc.selection().clone();
    let selected = app.editor.session.doc.selected_layer_ids();
    let target = app.editor.session.doc.mask_state().target;
    let before = Snapshot::capture(&app.editor);
    let public = app.editor.session.state();
    let history = app.editor.session.doc.history_bytes();
    app.editor.actions.push(Action::UiCommand(10184));
    app.run_actions(&ctx);
    app.consume_ui_effects(&ctx);
    assert_open_presentation(&app, public);
    before.assert_document(&app.editor);
    assert_eq!(app.editor.session.doc.history_bytes(), history);
    let admitted = Snapshot::capture(&app.editor);
    let revision = app.editor.session.revision;
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    let d = app.editor.transform.as_mut().unwrap();
    let DraftScope::Owners(scope) = d.scope else {
        panic!("Linked transform did not capture owners")
    };
    assert_eq!(scope.layer_ids(), [1, 2]);
    assert_eq!(scope.mask_ids(), [1]);
    assert_eq!(d.bounds, [0, 0, 12, 8]);
    d.params.translate_x = 2.;
    d.params.translate_y = -1.;
    d.params.method = Method::Nearest;
    let output = settle(&mut app, &ctx);
    assert!(
        app.editor.transform_texture().is_some(),
        "{}",
        app.editor.transform.as_ref().unwrap().preview_error
    );
    admitted.assert_same(&app.editor);
    let apply = output
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Apply"))
        .unwrap();
    click(&mut app, &ctx, apply, 9.);
    assert!(app.editor.transform.is_none(), "{}", app.editor.status);
    for (index, input) in pixels.iter().enumerate() {
        assert_eq!(
            app.editor.session.doc.layers[index].pixels,
            shifted(input, 2, -1, 0)
        );
    }
    assert_eq!(raw(&app, 1), shifted(&mask, 2, -1, 255));
    assert_eq!(raw(&app, 3), fixed_mask);
    assert_eq!(raw(&app, 2), group_mask);
    assert_eq!(app.editor.session.doc.selection(), &selection);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), selected);
    assert_eq!(app.editor.session.doc.mask_state().target, target);
    assert_eq!(app.editor.session.revision, revision + 1);
    assert_eq!(
        app.editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len(),
        count + 1
    );
    assert!(app.editor.core("history.undo", json!({})));
    before.assert_document(&app.editor);
    assert!(app.editor.core("history.redo", json!({})));
    assert_eq!(raw(&app, 1), shifted(&mask, 2, -1, 255));
}

#[test]
fn owner_transform_auto_mask_precedes_links_and_explicit_preview_keeps_exact_singleton_scope() {
    let mut app = owners();
    let ctx = warm(&mut app);
    assert!(app.editor.core(
        "toolbox.movement.options",
        json!({"target":"auto","step":1,"large_step":10})
    ));
    app.consume_ui_effects(&ctx);
    assert!(app
        .editor
        .core("mask.target", json!({"target":{"kind":"mask","id":3}})));
    let before = Snapshot::capture(&app.editor);
    let source = raw(&app, 3);
    app.editor.open_transform();
    let d = app.editor.transform.as_mut().unwrap();
    let DraftScope::Owners(scope) = d.scope else {
        panic!("Expected mask scope")
    };
    assert!(scope.is_mask_only());
    assert_eq!(scope.mask_ids(), [3]);
    assert_eq!(scope.mask_owner_layer_id(), Some(2));
    d.params.translate_x = 1.;
    d.params.method = Method::Nearest;
    let out = settle(&mut app, &ctx);
    before.assert_same(&app.editor);
    let owner_label = format!("Raster mask: {}", app.editor.session.doc.layer_name(1));
    assert!(out
        .shapes
        .iter()
        .any(|shape| locate(&shape.shape, &owner_label).is_some()));
    let apply = out
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Apply"))
        .unwrap();
    click(&mut app, &ctx, apply, 9.);
    assert_eq!(raw(&app, 3), shifted(&source, 1, 0, 255));
    assert_eq!(app.editor.session.doc.active, 0);
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        PaintTarget::Mask { id: 3 }
    );
    assert!(app.editor.core("history.undo", json!({})));
    before.assert_document(&app.editor);
    assert!(
        app.editor.core(
            "canvas.transform.configure",
            json!({"parameters":{"layer_ids":[2],"translate_x":1,"method":"nearest"}})
        ),
        "{}",
        app.editor.status
    );
    app.consume_ui_effects(&ctx);
    let d = app.editor.transform.as_ref().unwrap();
    let DraftScope::Owners(scope) = d.scope else {
        panic!("Expected explicit scope")
    };
    assert_eq!(scope.layer_ids(), [2]);
    assert!(scope.mask_ids().is_empty());
    assert_eq!(d.bounds, [1, 1, 4, 4]);
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    before.assert_document(&app.editor);
}

#[test]
fn owner_transform_open_cancel_and_natural_pen_packets_preserve_brush_and_hand_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            for queued in [false, true] {
                let mut app = owners();
                let ctx = warm(&mut app);
                app.editor
                    .session
                    .pointer(pointer(kind, PointerPhase::Down, 100))
                    .unwrap();
                if queued && local && kind == ToolKind::Brush {
                    for time in 101..381 {
                        app.editor
                            .session
                            .pointer(pointer(kind, PointerPhase::Move, time))
                            .unwrap();
                    }
                }
                app.editor.sync_from_session();
                app.editor.input_owned = local;
                app.editor.pen_active = local;
                app.editor.panning = (local && kind == ToolKind::Hand).then_some(Pos2::ZERO);
                app.editor.session.replay_boundary().unwrap();
                app.editor.session.doc.compose();
                let before = Snapshot::capture(&app.editor);
                let public = app.editor.session.state();
                let history = app.editor.session.doc.history_bytes();
                let transcript = samples(&mut app);
                app.editor.radius += 2.;
                app.editor.pan += vec2(7., 3.);
                if queued {
                    assert!(
                        app.editor.core(
                            "canvas.transform.configure",
                            json!({"parameters":{"layer_ids":[1,2],"translate_x":1}})
                        ),
                        "{}",
                        app.editor.status
                    );
                } else {
                    app.editor.actions.push(Action::UiCommand(10184));
                    app.run_actions(&ctx);
                }
                app.consume_ui_effects(&ctx);
                if !queued {
                    assert_open_presentation(&app, public);
                } else {
                    assert_eq!(app.editor.session.state(), public);
                }
                before.assert_document(&app.editor);
                assert_eq!(app.editor.session.doc.history_bytes(), history);
                let admitted = Snapshot::capture(&app.editor);
                assert!(app.editor.guarded_transform_modal());
                if !queued {
                    app.editor.transform.as_mut().unwrap().params.translate_x = 1.;
                }
                // egui measures a newly opened Window before painting its
                // controls. Exercise each sizing frame without losing input.
                let mut out = frame(&mut app, &ctx, vec2(1360., 920.), vec![], 7.);
                for n in 1..3 {
                    out = frame(
                        &mut app,
                        &ctx,
                        vec2(1360., 920.),
                        vec![],
                        7. + n as f64 / 100.,
                    );
                    admitted.assert_same(&app.editor);
                    assert_eq!(samples(&mut app), transcript);
                }
                let apply = out
                    .shapes
                    .iter()
                    .find_map(|shape| locate(&shape.shape, "Apply"))
                    .unwrap();
                click(&mut app, &ctx, apply, 7.1);
                admitted.assert_same(&app.editor);
                assert_eq!(samples(&mut app), transcript);
                assert_eq!(app.editor.input_owned, local);
                assert_eq!(app.editor.pen_active, local);
                assert!(
                    app.editor.session.stroke.active()
                        && app.editor.session.pointer_checkpoint.is_some()
                );
                app.editor.actions.push(Action::Cancel);
                app.run_actions(&ctx);
                assert!(app.editor.transform.is_none());
                admitted.assert_same(&app.editor);
                assert_eq!(app.editor.radius, app.editor.session.parameters.radius);
                assert_eq!(
                    app.editor.pan,
                    vec2(app.editor.session.view.pan_x, app.editor.session.view.pan_y)
                );
                assert_eq!(samples(&mut app), transcript);
                assert_eq!(app.editor.input_owned, local);
                assert_eq!(app.editor.pen_active, local);
                if local {
                    // Open the same guarded modal again: native MOVE/UP belong to the
                    // original contact, rather than transform handles or discarded packets.
                    app.editor.open_transform();
                    let p = app.editor.image_rect.min + vec2(5., 4.) * app.editor.zoom;
                    for (phase, time) in [(PenPhase::Move, 500), (PenPhase::Up, 600)] {
                        app.editor.packets.push(PenSample {
                            phase,
                            x: p.x * ctx.pixels_per_point(),
                            y: p.y * ctx.pixels_per_point(),
                            pressure: 0.6,
                            eraser: false,
                            time,
                        });
                        frame(
                            &mut app,
                            &ctx,
                            vec2(1360., 920.),
                            vec![],
                            time as f64 / 1000. + 8.,
                        );
                    }
                    let all = samples(&mut app);
                    assert_eq!(all.len(), transcript.len() + 3);
                    assert_eq!(&all[..transcript.len()], transcript.as_slice());
                    assert_eq!(all[all.len() - 3]["phase"], "move");
                    assert_eq!(all[all.len() - 3]["time"], 500);
                    assert_eq!(all[all.len() - 2]["phase"], "move");
                    assert_eq!(all[all.len() - 2]["time"], 600);
                    assert_eq!(all.last().unwrap()["phase"], "up");
                    assert_eq!(all.last().unwrap()["time"], 600);
                    assert!(!app.editor.session.stroke.active());
                    assert!(!app.editor.pen_active);
                } else {
                    app.editor
                        .session
                        .pointer(pointer(kind, PointerPhase::Up, 300))
                        .unwrap();
                    assert_eq!(samples(&mut app).len(), transcript.len() + 1);
                }
            }
        }
    }
}

#[test]
fn owner_transform_rebases_only_its_own_idle_navigation_and_rejects_prior_external_changes() {
    let mut app = owners();
    let ctx = warm(&mut app);
    app.editor.open_transform();
    let original = app.editor.transform.as_ref().unwrap().source;
    app.editor.transform.as_mut().unwrap().params.translate_x = 1.;
    app.editor.zoom = 9.;
    app.editor.pan = vec2(4., 3.);
    app.editor.radius += 2.;
    window(&mut app, &ctx, vec2(1360., 920.), vec![], 6.);
    let d = app.editor.transform.as_ref().unwrap();
    assert_eq!(d.source.0, original.0);
    assert_eq!(d.source.1, app.editor.session.revision);
    assert_eq!(d.source.1, original.1 + 1);
    assert_eq!(app.editor.session.view.zoom, 9.);
    assert_ne!(app.editor.radius, app.editor.session.parameters.radius);
    let source = d.source;
    assert!(app
        .editor
        .core("mask.fill", json!({"mask_id":3,"value":90})));
    let revision = app.editor.session.revision;
    let view = app.editor.session.view;
    app.editor.pan += vec2(8., 7.);
    let out = window(&mut app, &ctx, vec2(1360., 920.), vec![], 7.);
    assert_eq!(app.editor.transform.as_ref().unwrap().source, source);
    assert_eq!(app.editor.session.revision, revision);
    assert_eq!(app.editor.session.view, view);
    assert!(out.shapes.iter().any(|shape| locate(
        &shape.shape,
        "Document changed; reopen Free Transform."
    )
    .is_some()));
    let before = Snapshot::capture(&app.editor);
    let apply = out
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Apply"))
        .unwrap();
    click(&mut app, &ctx, apply, 8.);
    before.assert_same(&app.editor);
    assert!(app.editor.transform.is_some());
    app.editor.actions.push(Action::Cancel);
    app.run_actions(&ctx);
    before.assert_same(&app.editor);
}

#[test]
fn owner_transform_queued_preview_rejects_stale_revision_and_reused_document_ids_before_opening() {
    for replace in [false, true] {
        let mut app = owners();
        let ctx = warm(&mut app);
        assert!(app.editor.core(
            "canvas.transform.configure",
            json!({"parameters":{"layer_ids":[1,2],"translate_x":1}})
        ));
        if replace {
            assert!(app
                .editor
                .core("canvas.new", json!({"width":12,"height":8})));
        } else {
            assert!(app
                .editor
                .core("mask.fill", json!({"mask_id":3,"value":91})));
        }
        let before = Snapshot::capture(&app.editor);
        app.consume_ui_effects(&ctx);
        assert!(app.editor.transform.is_none());
        assert!(app.editor.status.contains("Document changed"));
        before.assert_same(&app.editor);
    }
}

#[test]
fn owner_transform_unsupported_group_mask_and_selected_pixels_preserve_live_contact_on_open_and_cancel(
) {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for target in ["group", "selection"] {
            let mut app = owners();
            let ctx = warm(&mut app);
            if target == "selection" {
                assert!(app.editor.core(
                    "toolbox.movement.options",
                    json!({"target":"selection","step":1,"large_step":10})
                ));
            }
            app.consume_ui_effects(&ctx);
            app.editor
                .session
                .pointer(pointer(kind, PointerPhase::Down, 100))
                .unwrap();
            app.editor.sync_from_session();
            app.editor.input_owned = true;
            app.editor.pen_active = true;
            app.editor.panning = (kind == ToolKind::Hand).then_some(Pos2::ZERO);
            app.editor.session.doc.compose();
            let before = Snapshot::capture(&app.editor);
            let public = app.editor.session.state();
            let history = app.editor.session.doc.history_bytes();
            let transcript = samples(&mut app);
            if target == "group" {
                assert!(!app.editor.core(
                    "canvas.transform.configure",
                    json!({"parameters":{"mask_id":2,"translate_x":1}})
                ));
                assert!(app.editor.transform.is_none());
                before.assert_same(&app.editor);
            } else {
                app.editor.actions.push(Action::UiCommand(10184));
                app.run_actions(&ctx);
                app.consume_ui_effects(&ctx);
                assert_open_presentation(&app, public);
                before.assert_document(&app.editor);
                assert_eq!(app.editor.session.doc.history_bytes(), history);
                let admitted = Snapshot::capture(&app.editor);
                let d = app.editor.transform.as_ref().unwrap();
                assert!(d.guarded_contact);
                assert!(!d.bounds_valid);
                assert!(!d.error.is_empty());
                window(&mut app, &ctx, vec2(1360., 920.), vec![], 7.);
                admitted.assert_same(&app.editor);
                app.editor.actions.push(Action::Cancel);
                app.run_actions(&ctx);
                admitted.assert_same(&app.editor);
            }
            assert_eq!(samples(&mut app), transcript);
            assert!(
                app.editor.input_owned
                    && app.editor.pen_active
                    && app.editor.session.stroke.active()
            );
        }
    }
}

#[test]
fn owner_transform_perspective_is_disabled_and_preview_failure_does_not_disable_apply() {
    let mut app = owners();
    let ctx = warm(&mut app);
    app.editor.open_transform();
    app.editor.transform.as_mut().unwrap().params.translate_x = 1.;
    let mut out = window(&mut app, &ctx, vec2(1360., 920.), vec![], 6.);
    for n in 1..3 {
        out = window(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![],
            6. + n as f64 / 100.,
        );
    }
    let perspective = out
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Edit Perspective Corners"))
        .unwrap();
    click(&mut app, &ctx, perspective, 7.);
    assert!(app.editor.transform.as_ref().unwrap().params.quad.is_none());
    // Preview failure is presentation state. The already validated source can
    // still use the shared mutation when preview is disabled.
    let d = app.editor.transform.as_mut().unwrap();
    d.preview = false;
    d.preview_error = "Preview failed".into();
    let out = window(&mut app, &ctx, vec2(1360., 920.), vec![], 8.);
    let apply = out
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Apply"))
        .unwrap();
    click(&mut app, &ctx, apply, 9.);
    assert!(app.editor.transform.is_none());
    assert_eq!(
        app.editor.session.doc.last_transform().unwrap().target,
        crate::engine::transform::Target::Owners
    );
    // The ordinary singleton route retains its original perspective controls.
    let mut legacy = super::tests::app();
    let legacy_ctx = warm(&mut legacy);
    legacy.editor.open_transform();
    assert!(!legacy.editor.guarded_transform_modal());
    let mut out = window(&mut legacy, &legacy_ctx, vec2(1360., 920.), vec![], 6.);
    for n in 1..3 {
        out = window(
            &mut legacy,
            &legacy_ctx,
            vec2(1360., 920.),
            vec![],
            6. + n as f64 / 100.,
        );
    }
    let p = out
        .shapes
        .iter()
        .find_map(|shape| locate(&shape.shape, "Edit Perspective Corners"))
        .unwrap();
    click(&mut legacy, &legacy_ctx, p, 7.);
    assert!(legacy
        .editor
        .transform
        .as_ref()
        .unwrap()
        .params
        .quad
        .is_some());
}

#[test]
fn owner_transform_four_language_narrow_wide_controls_keep_scope_feedback_and_footer_visible() {
    use crate::i18n::Language;
    crate::i18n::load_test_catalogs();
    for language in [
        Language::English,
        Language::Chinese,
        Language::Japanese,
        Language::Korean,
    ] {
        let mut app = owners();
        app.editor.settings.ui.language = language;
        let ctx = warm(&mut app);
        app.editor.open_transform();
        for (index, size) in [vec2(460., 420.), vec2(1360., 920.)]
            .into_iter()
            .enumerate()
        {
            let mut out = window(&mut app, &ctx, size, vec![], 10. + index as f64 * 4.);
            for n in 1..3 {
                out = window(
                    &mut app,
                    &ctx,
                    size,
                    vec![],
                    10. + index as f64 * 4. + n as f64,
                );
            }
            let source = "Transforming {count} layers and their linked masks.";
            let label = crate::i18n::text_in(language, source).replace("{count}", "2");
            assert!(
                out.shapes
                    .iter()
                    .any(|shape| locate(&shape.shape, &label).is_some()),
                "{language:?} {size:?}: missing scope feedback"
            );
            for source in ["Apply", "Cancel"] {
                let label = crate::i18n::text_in(language, source);
                let (clip, p) = out
                    .shapes
                    .iter()
                    .find_map(|shape| locate(&shape.shape, &label).map(|p| (shape.clip_rect, p)))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(p) && clip.contains(p),
                    "{language:?} {source} {p:?}/{clip:?}/{size:?}"
                );
            }
        }
    }
    crate::i18n::set_language(Language::English);
}
