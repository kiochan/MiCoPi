//! Real controls share registered adjustment actions and retain painting owners.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::tests::studio;
use crate::engine::{document::Composition, raster_masks::PaintTarget};
use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};

fn app() -> Studio {
    let mut app = studio();
    app.editor.settings.ui.language = crate::i18n::Language::English;
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    app.editor.session.runtime = "desktop";
    app.editor.register_commands();
    assert!(app
        .editor
        .core("canvas.new", json!({"width":12,"height":8})));
    assert!(app
        .editor
        .core("layer.fill_color", json!({"color":"#4879AB"})));
    assert!(app.editor.core("layer.add", json!({})));
    assert!(app.editor.core(
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":5,"height":4})
    ));
    assert!(app
        .editor
        .core("layer.fill_color", json!({"color":"#AB6891"})));
    assert!(app.editor.core("selection.deselect", json!({})));
    app.editor.sync_from_session();
    app
}
fn create(app: &mut Studio, selection: bool) -> u64 {
    let mut args = json!({"node_type":"compositing.filter","parameters":{"filter":"invert","amount":1.},"parent_id":0,"index":1,"name":"Ink adjustment"});
    if selection {
        args["mask_source"] = json!("selection");
    }
    app.editor
        .session
        .apply(crate::application::Action::new("adjustment.create", args))
        .unwrap()["result"]["adjustment_id"]
        .as_u64()
        .unwrap()
}
fn context(app: &Studio) -> Context {
    let ctx = Context::default();
    theme::fonts(&ctx);
    theme::apply(&ctx, &app.editor.settings.ui);
    ctx
}
fn frame(app: &mut Studio, ctx: &Context, size: Vec2, events: Vec<Event>, time: f64) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    let modifiers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .unwrap_or_default();
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            modifiers,
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.layers_ui(ui));
        },
    )
}
fn warm(app: &mut Studio, ctx: &Context, size: Vec2) {
    for time in 0..3 {
        frame(app, ctx, size, vec![], time as f64);
    }
}
fn click(app: &mut Studio, ctx: &Context, size: Vec2, p: Pos2, time: f64) -> FullOutput {
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains(p),
        "Control outside viewport: {p:?}/{size:?}"
    );
    frame(
        app,
        ctx,
        size,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
        ],
        time,
    );
    let mut out = frame(
        app,
        ctx,
        size,
        vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            },
        ],
        time + 0.01,
    );
    // Newly opened popups and inspectors receive egui's ordinary sizing frames
    // before another actual control is hit; mutations still occur only on click.
    for n in 2..4 {
        out = frame(app, ctx, size, vec![], time + n as f64 / 100.);
    }
    out
}
fn label(shape: &Shape, text: &str) -> Option<Pos2> {
    match shape {
        Shape::Text(text_shape) if text_shape.galley.text() == text => {
            Some(text_shape.pos + text_shape.galley.size() * 0.5)
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, text)),
        _ => None,
    }
}
fn name(app: &mut Studio, ctx: &Context, size: Vec2, id: u64, value: &str, time: f64) {
    let key = Id::new(("adjustment-name", app.editor.session.doc.identity, id));
    ctx.memory_mut(|memory| memory.request_focus(key));
    frame(
        app,
        ctx,
        size,
        vec![
            Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            Event::Text(value.into()),
        ],
        time,
    );
}
fn control(ctx: &Context, app: &Studio, kind: &str, id: u64) -> (Rect, bool) {
    ctx.data_mut(|data| {
        data.get_temp::<(Rect, bool)>(Id::new((kind, app.editor.session.doc.identity, id)))
    })
    .unwrap()
}
fn raw(app: &Studio, id: u64) -> Vec<u8> {
    let plane = app.editor.session.doc.mask_plane(id).unwrap();
    (0..96).map(|i| plane.sample(i)).collect()
}
#[derive(Clone)]
struct Content {
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    raw: Vec<(u64, Vec<u8>)>,
    surface: Vec<u32>,
    history: usize,
}
impl Content {
    fn capture(app: &Studio) -> Self {
        Self {
            composition: app.editor.session.doc.composition(),
            pixels: app
                .editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect(),
            raw: app
                .editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .map(|mask| (mask.id, raw(app, mask.id)))
                .collect(),
            surface: app.editor.session.doc.composite.clone(),
            history: app.editor.session.doc.history_bytes(),
        }
    }
    fn unchanged(&self, app: &Studio) {
        self.document(app);
        assert_eq!(app.editor.session.doc.history_bytes(), self.history);
    }
    fn document(&self, app: &Studio) {
        assert_eq!(app.editor.session.doc.composition(), self.composition);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            self.pixels
        );
        assert_eq!(
            app.editor
                .session
                .doc
                .mask_state()
                .items
                .iter()
                .map(|mask| (mask.id, raw(app, mask.id)))
                .collect::<Vec<_>>(),
            self.raw
        );
        assert_eq!(app.editor.session.doc.composite, self.surface);
    }
}
fn samples(app: &mut Studio) -> Vec<Value> {
    let journal = app.editor.session.replay_journal(false).unwrap();
    let events = journal.events(0, journal.entries.len()).unwrap();
    let mut samples: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    samples.extend(
        app.editor
            .session
            .replay_samples
            .iter()
            .map(|sample| serde_json::to_value(sample).unwrap()),
    );
    samples
}
fn pointer(tool: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 5.,
        y: 4.,
        pressure: 0.4,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
        view_x: Some(50.),
        view_y: Some(40.),
    }
}

#[test]
fn adjustment_root_rows_inspect_without_selecting_rasters_and_apply_cancel_are_one_explicit_edit() {
    let mut app = app();
    let id = create(&mut app, false);
    let ctx = context(&app);
    let size = vec2(380., 1120.);
    assert!(!app.editor.session.doc.has_groups());
    assert!(app.editor.session.doc.has_adjustments());
    warm(&mut app, &ctx, size);
    let active = app.editor.session.doc.active;
    let selected = app.editor.session.doc.selected_layer_ids();
    let target = app.editor.session.doc.mask_state().target;
    let before = Content::capture(&app);
    let public = app.editor.session.state();
    let row = ctx
        .data_mut(|data| {
            data.get_temp::<Rect>(Id::new((
                "adjustment-row",
                app.editor.session.doc.identity,
                id,
            )))
        })
        .unwrap();
    click(&mut app, &ctx, size, row.center(), 4.);
    assert_eq!(app.editor.focused_adjustment_ui(&ctx), Some(id));
    assert_eq!(app.editor.session.state(), public);
    before.unchanged(&app);
    name(&mut app, &ctx, size, id, "Soft shadows", 5.);
    before.unchanged(&app);
    let (apply, enabled) = control(&ctx, &app, "adjustment-apply", id);
    assert!(enabled);
    let revision = app.editor.session.revision;
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    click(&mut app, &ctx, size, apply.center(), 6.);
    assert_eq!(
        app.editor.session.doc.adjustment(id).unwrap().name,
        "Soft shadows"
    );
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
    assert_eq!(app.editor.session.doc.active, active);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), selected);
    assert_eq!(app.editor.session.doc.mask_state().target, target);
    assert_eq!(
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        before.pixels
    );
    assert!(app.editor.core("history.undo", json!({})));
    before.document(&app);
    frame(&mut app, &ctx, size, vec![], 7.);
    name(&mut app, &ctx, size, id, "Discard this", 8.);
    let unchanged = Content::capture(&app);
    let public = app.editor.session.state();
    let (cancel, enabled) = control(&ctx, &app, "adjustment-cancel", id);
    assert!(enabled);
    click(&mut app, &ctx, size, cancel.center(), 9.);
    unchanged.unchanged(&app);
    assert_eq!(app.editor.session.state(), public);
    assert!(ctx
        .data_mut(|data| data.get_temp::<Draft>(draft_key(app.editor.session.doc.identity, id)))
        .is_none());
}

#[test]
fn adjustment_create_menu_snapshots_selection_into_owned_mask_and_keeps_live_selection_independent()
{
    let mut app = app();
    assert!(app.editor.core(
        "selection.shape",
        json!({"kind":"rectangle","x":2,"y":1,"width":5,"height":4})
    ));
    let ctx = context(&app);
    let size = vec2(380., 1120.);
    warm(&mut app, &ctx, size);
    let public = app.editor.session.state();
    let pixel_selection = app.editor.session.doc.selection().clone();
    let out = frame(&mut app, &ctx, size, vec![], 4.);
    let menu = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "New Adjustment"))
        .unwrap();
    let out = click(&mut app, &ctx, size, menu, 5.);
    let checkbox = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Create Mask from Selection"))
        .unwrap();
    let out = click(&mut app, &ctx, size, checkbox, 6.);
    let effect = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Invert"))
        .unwrap();
    click(&mut app, &ctx, size, effect, 7.);
    let item = app.editor.session.doc.adjustments().first().unwrap();
    let id = item.id;
    assert_eq!(app.editor.focused_adjustment_ui(&ctx), Some(id));
    assert_eq!(app.editor.session.doc.layers.len(), 2);
    assert_eq!(
        app.editor.session.doc.active,
        public["active_layer"].as_u64().unwrap() as usize
    );
    assert_eq!(app.editor.session.doc.selection(), &pixel_selection);
    let mask = app
        .editor
        .session
        .doc
        .mask_state()
        .items
        .iter()
        .find(|mask| mask.owner == MaskOwner::Adjustment { id })
        .unwrap()
        .id;
    let expected = (0..96)
        .map(|index| {
            if (2..7).contains(&(index % 12)) && (1..5).contains(&(index / 12)) {
                255
            } else {
                0
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(raw(&app, mask), expected);
    let composite = app.editor.session.doc.composite.clone();
    assert!(app.editor.core("selection.all", json!({})));
    assert_eq!(raw(&app, mask), expected);
    assert_eq!(app.editor.session.doc.composite, composite);
    assert!(app.editor.core("selection.deselect", json!({})));
    assert_eq!(raw(&app, mask), expected);
}

#[test]
fn adjustment_mask_thumbnail_routes_real_pen_input_to_a8_without_touching_the_painting_raster() {
    let mut app = app();
    assert!(app.editor.core(
        "selection.shape",
        json!({"kind":"rectangle","x":2,"y":1,"width":7,"height":6})
    ));
    let id = create(&mut app, true);
    assert!(app.editor.core("selection.deselect", json!({})));
    let mask = app
        .editor
        .session
        .doc
        .mask_state()
        .items
        .iter()
        .find(|mask| mask.owner == MaskOwner::Adjustment { id })
        .unwrap()
        .id;
    let ctx = context(&app);
    let size = vec2(380., 1120.);
    warm(&mut app, &ctx, size);
    let thumbnail = ctx
        .data_mut(|data| {
            data.get_temp::<Rect>(Id::new((
                "mask-thumbnail-control",
                app.editor.session.doc.identity,
                mask,
            )))
        })
        .unwrap();
    click(&mut app, &ctx, size, thumbnail.center(), 4.);
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        PaintTarget::Mask { id: mask }
    );
    assert_eq!(app.editor.focused_adjustment_ui(&ctx), Some(id));
    assert!(app.editor.core("layer.set", json!({"pixel_locked":true})));
    assert!(app.editor.core("brush.set",json!({"color":"#000000","radius":1.,"stabilizer":0.,"pressure_size":true,"pressure_opacity":true})));
    app.editor.sync_from_session();
    app.editor.fit = false;
    app.editor.zoom = 8.;
    app.editor.pan = Vec2::ZERO;
    app.editor.push_gui_view();
    let pixels = app
        .editor
        .session
        .doc
        .layers
        .iter()
        .map(|layer| layer.pixels.clone())
        .collect::<Vec<_>>();
    let before = raw(&app, mask);
    let active = app.editor.session.doc.active;
    let canvas = |app: &mut Studio, time: f64| {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640., 480.))),
                time: Some(time),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default().show(ctx, |ui| app.editor.canvas_ui(ui));
            },
        );
    };
    for n in 0..3 {
        canvas(&mut app, 5. + n as f64 / 100.);
    }
    // A radius below one pixel still covers this pixel center at low pressure.
    let p = app.editor.image_rect.min + vec2(5.5, 4.5) * app.editor.zoom;
    for (phase, time, pressure) in [
        (PenPhase::Down, 6000, 0.4),
        (PenPhase::Move, 6100, 0.7),
        (PenPhase::Up, 6200, 0.7),
    ] {
        app.editor.packets.push(PenSample {
            phase,
            x: p.x * ctx.pixels_per_point(),
            y: p.y * ctx.pixels_per_point(),
            pressure,
            eraser: false,
            time,
        });
        canvas(&mut app, time as f64 / 1000.);
        if matches!(phase, PenPhase::Down) {
            assert!(
                app.editor.session.stroke.active() && app.editor.pen_active,
                "{}; canvas={:?}, point={p:?}",
                app.editor.status,
                app.editor.canvas
            );
            assert_ne!(
                raw(&app, mask),
                before,
                "A centered pressure dab must change the actual A8 plane"
            );
        }
    }
    assert_ne!(raw(&app, mask), before);
    assert_eq!(
        app.editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>(),
        pixels
    );
    assert_eq!(app.editor.session.doc.active, active);
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        PaintTarget::Mask { id: mask }
    );
    assert!(!app.editor.session.stroke.active());
    assert!(app.editor.core("history.undo", json!({})));
    assert_eq!(raw(&app, mask), before);
    assert!(app.editor.core(
        "adjustment.set",
        json!({"adjustment_id":id,"visible":false})
    ));
    let before = Content::capture(&app);
    let error = app
        .editor
        .session
        .pointer(pointer(ToolKind::Brush, PointerPhase::Down, 7000))
        .unwrap_err();
    assert!(error.message.contains("hidden"));
    before.unchanged(&app);
    assert!(!app.editor.session.stroke.active());
}

#[test]
fn adjustment_dynamic_inspect_and_rejected_apply_preserve_local_external_brush_hand_samples_and_drafts(
) {
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = app();
            let id = create(&mut app, false);
            let ctx = context(&app);
            let size = vec2(380., 1120.);
            warm(&mut app, &ctx, size);
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Down, 100))
                .unwrap();
            app.editor.sync_from_session();
            app.editor.input_owned = local;
            app.editor.pen_active = local;
            app.editor.panning = (local && tool == ToolKind::Hand).then_some(Pos2::ZERO);
            let before = Content::capture(&app);
            let public = app.editor.session.state();
            let transcript = samples(&mut app);
            let command = app
                .editor
                .settings
                .keys
                .commands()
                .find(|command| command.keywords == "adjustment.inspect")
                .unwrap()
                .ui;
            app.editor.actions.push(Action::UiCommand(command));
            app.run_actions(&ctx);
            let mut out = FullOutput::default();
            let command_size = vec2(1360., 920.);
            for time in 4..7 {
                out = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, command_size)),
                        time: Some(time as f64),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| app.editor.plugin_command_window(ctx),
                );
            }
            let run = out
                .shapes
                .iter()
                .find_map(|shape| label(&shape.shape, "Run"))
                .unwrap();
            for (n, pressed) in [true, false].into_iter().enumerate() {
                ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, command_size)),
                        time: Some(7. + n as f64 / 100.),
                        focused: true,
                        events: vec![
                            Event::PointerMoved(run),
                            Event::PointerButton {
                                pos: run,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            },
                        ],
                        ..Default::default()
                    },
                    |ctx| app.editor.plugin_command_window(ctx),
                );
            }
            app.consume_ui_effects(&ctx);
            assert_eq!(app.editor.focused_adjustment_ui(&ctx), Some(id));
            assert_eq!(app.editor.session.state(), public);
            before.unchanged(&app);
            frame(&mut app, &ctx, size, vec![], 8.);
            name(&mut app, &ctx, size, id, "Keep this draft", 9.);
            let (apply, enabled) = control(&ctx, &app, "adjustment-apply", id);
            assert!(enabled);
            click(&mut app, &ctx, size, apply.center(), 10.);
            assert!(
                app.editor.status.contains("stroke_active"),
                "{}",
                app.editor.status
            );
            before.unchanged(&app);
            assert_eq!(app.editor.session.state(), public);
            assert_eq!(samples(&mut app), transcript);
            assert_eq!(app.editor.input_owned, local);
            assert_eq!(app.editor.pen_active, local);
            assert!(app.editor.session.stroke.active());
            let draft = ctx
                .data_mut(|data| {
                    data.get_temp::<Draft>(draft_key(app.editor.session.doc.identity, id))
                })
                .unwrap();
            assert_eq!(draft.value.name, "Keep this draft");
            let (cancel, enabled) = control(&ctx, &app, "adjustment-cancel", id);
            assert!(enabled);
            click(&mut app, &ctx, size, cancel.center(), 11.);
            before.unchanged(&app);
            assert_eq!(samples(&mut app), transcript);
            assert_eq!(app.editor.input_owned, local);
            assert_eq!(app.editor.pen_active, local);
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Up, 200))
                .unwrap();
            assert_eq!(samples(&mut app).len(), transcript.len() + 1);
        }
    }
}

#[test]
fn adjustment_inspector_stale_drafts_and_four_language_narrow_controls_keep_apply_cancel_visible() {
    use crate::i18n::Language;
    crate::i18n::load_test_catalogs();
    for language in [
        Language::English,
        Language::Chinese,
        Language::Japanese,
        Language::Korean,
    ] {
        let mut app = app();
        let id = create(&mut app, false);
        app.editor.settings.ui.language = language;
        let ctx = context(&app);
        app.editor.focus_adjustment_ui(&ctx, Some(id));
        for (i, size) in [vec2(280., 740.), vec2(700., 1000.)]
            .into_iter()
            .enumerate()
        {
            for n in 0..3 {
                frame(&mut app, &ctx, size, vec![], i as f64 * 10. + n as f64);
            }
            name(
                &mut app,
                &ctx,
                size,
                id,
                "Drafted shadows",
                4. + i as f64 * 10.,
            );
            let mut out = frame(&mut app, &ctx, size, vec![], 5. + i as f64 * 10.);
            for n in 0..2 {
                out = frame(&mut app, &ctx, size, vec![], 6. + i as f64 * 10. + n as f64);
            }
            for source in ["Apply", "Cancel"] {
                let title = crate::i18n::text_in(language, source);
                let (clip, p) = out
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        label(&shape.shape, &title).map(|point| (shape.clip_rect, point))
                    })
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(p) && clip.contains(p),
                    "{language:?} {source}: {p:?}/{clip:?}/{size:?}"
                );
            }
            assert!(app
                .editor
                .core("layer.rename", json!({"name":"External paint layer"})));
            let before = Content::capture(&app);
            frame(&mut app, &ctx, size, vec![], 8. + i as f64 * 10.);
            let (_, enabled) = control(&ctx, &app, "adjustment-apply", id);
            assert!(!enabled);
            let draft = ctx
                .data_mut(|data| {
                    data.get_temp::<Draft>(draft_key(app.editor.session.doc.identity, id))
                })
                .unwrap();
            assert_eq!(draft.value.name, "Drafted shadows");
            before.unchanged(&app);
            let (cancel, _) = control(&ctx, &app, "adjustment-cancel", id);
            click(&mut app, &ctx, size, cancel.center(), 9. + i as f64 * 10.);
        }
    }
    crate::i18n::set_language(Language::English);
}

#[test]
fn adjustment_registered_filter_alias_keeps_its_owned_schema_and_defaults_through_the_create_menu()
{
    let mut app = app();
    let definition=serde_json::from_value(json!({"id":"test.manga-tone","title":"Manga Tone","category":"Image",
        "binding":{"kind":"image","operation":"filter","defaults":{"filter":"brightness","amount":0.3}},
        "parameters_schema":{"type":"object","properties":{"filter":{"type":"string","enum":["brightness"]},"amount":{"type":"number","minimum":0.2,"maximum":0.4}},"required":["filter","amount"],"additionalProperties":false}})).unwrap();
    app.editor
        .session
        .registry
        .nodes
        .insert("test.manga-tone".into(), ("test".into(), definition));
    app.editor.plugin_nodes = app.editor.session.registry.node_catalog();
    let ctx = context(&app);
    let size = vec2(380., 1120.);
    warm(&mut app, &ctx, size);
    let out = frame(&mut app, &ctx, size, vec![], 4.);
    let p = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "New Adjustment"))
        .unwrap();
    let out = click(&mut app, &ctx, size, p, 5.);
    let p = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Manga Tone"))
        .unwrap();
    let out = click(&mut app, &ctx, size, p, 6.);
    let p = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Brightness"))
        .unwrap();
    click(&mut app, &ctx, size, p, 7.);
    let item = app.editor.session.doc.adjustments().first().unwrap();
    assert_eq!(item.node_type, "test.manga-tone");
    assert_eq!(item.filter, Filter::Brightness);
    assert!((item.amount - 0.3).abs() < 1e-6);
    assert_eq!(app.editor.session.doc.layers.len(), 2);
}

#[test]
fn adjustment_row_inspection_preserves_pending_raster_name_and_native_contact_instead_of_auto_committing(
) {
    let mut app = app();
    let id = create(&mut app, false);
    let ctx = context(&app);
    let size = vec2(380., 1120.);
    warm(&mut app, &ctx, size);
    let active = app
        .editor
        .session
        .doc
        .layer_id(app.editor.session.doc.active);
    let key = Id::new(("layer-name", app.editor.session.doc.identity, active));
    ctx.memory_mut(|memory| memory.request_focus(key));
    frame(
        &mut app,
        &ctx,
        size,
        vec![
            Event::Key {
                key: Key::A,
                physical_key: Some(Key::A),
                pressed: true,
                repeat: false,
                modifiers: Modifiers {
                    ctrl: true,
                    command: true,
                    ..Default::default()
                },
            },
            Event::Text("Kept raster draft".into()),
        ],
        4.,
    );
    app.editor
        .session
        .pointer(pointer(ToolKind::Brush, PointerPhase::Down, 100))
        .unwrap();
    app.editor.sync_from_session();
    app.editor.input_owned = true;
    app.editor.pen_active = true;
    let before = Content::capture(&app);
    let public = app.editor.session.state();
    let transcript = samples(&mut app);
    let row = ctx
        .data_mut(|data| {
            data.get_temp::<Rect>(Id::new((
                "adjustment-row",
                app.editor.session.doc.identity,
                id,
            )))
        })
        .unwrap();
    click(&mut app, &ctx, size, row.center(), 5.);
    frame(&mut app, &ctx, size, vec![], 6.);
    before.unchanged(&app);
    assert_eq!(app.editor.session.state(), public);
    assert_eq!(samples(&mut app), transcript);
    assert_eq!(app.editor.focused_adjustment_ui(&ctx), Some(id));
    assert_eq!(
        app.editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active),
        active
    );
    assert_eq!(
        ctx.data_mut(|data| data.get_temp::<String>(key)).as_deref(),
        Some("Kept raster draft")
    );
    assert!(app.editor.session.stroke.active() && app.editor.input_owned && app.editor.pen_active);
}

#[test]
fn adjustment_commands_are_searchable_and_bindable_in_all_four_languages_without_executing() {
    use crate::i18n::Language;
    crate::i18n::load_test_catalogs();
    let app = app();
    let public = app.editor.session.state();
    let before = Content::capture(&app);
    for (id, title) in [
        ("adjustment.catalog", "Adjustment Filter Catalog"),
        ("adjustment.query", "Query Adjustment Layers"),
        ("adjustment.get", "Read Adjustment Layer"),
        ("adjustment.create", "Create Adjustment Layer"),
        ("adjustment.set", "Set Adjustment Layer Properties"),
        ("adjustment.move", "Move Adjustment Layer"),
        ("adjustment.delete", "Delete Adjustment Layer"),
        ("adjustment.inspect", "Inspect Adjustment Layer"),
    ] {
        let command = app
            .editor
            .settings
            .keys
            .commands()
            .find(|command| command.keywords == id)
            .unwrap();
        assert!(app
            .editor
            .settings
            .keys
            .plugin_action(command.ui)
            .is_some_and(|action| action == id));
        for language in [
            Language::English,
            Language::Chinese,
            Language::Japanese,
            Language::Korean,
        ] {
            let query = crate::i18n::text_in(language, title).into_owned();
            let mut palette = Palette::new(
                crate::ui::palette::Mode::Commands,
                &app.editor.settings.keys,
            );
            palette.query(query.clone(), &app.editor.settings.keys);
            assert!(
                palette.results.iter().any(|&index| app
                    .editor
                    .settings
                    .keys
                    .command(index)
                    .keywords
                    == id),
                "{language:?}: {id}/{query}"
            );
        }
    }
    assert_eq!(app.editor.session.state(), public);
    before.unchanged(&app);
}

#[test]
fn adjustment_named_folder_query_shows_adjustment_context_and_selected_predicate_excludes_it() {
    let mut app = app();
    let group = app
        .editor
        .session
        .apply(crate::application::Action::new(
            "group.create",
            json!({"members":[{"kind":"layer","id":1}],"name":"Atmosphere"}),
        ))
        .unwrap()["result"]["group_id"]
        .as_u64()
        .unwrap();
    let adjustment=app.editor.session.apply(crate::application::Action::new("adjustment.create",json!({"parent_id":group,"index":1,"name":"Warm shadows","node_type":"compositing.filter","parameters":{"filter":"brightness","amount":0.2}}))).unwrap()["result"]["adjustment_id"].as_u64().unwrap();
    let public = app.editor.session.state();
    let before = Content::capture(&app);
    let result = app
        .editor
        .session
        .apply(crate::application::Action::new(
            "group.query",
            json!({"query":"Atmosphere"}),
        ))
        .unwrap()["result"]
        .clone();
    let rows = result["rows"].as_array().unwrap();
    assert!(rows
        .iter()
        .any(|row| row["kind"] == "group" && row["id"] == group && row["matched"] == true));
    let context = rows
        .iter()
        .find(|row| row["kind"] == "adjustment" && row["id"] == adjustment)
        .unwrap();
    assert_eq!(context["context"], true);
    assert_eq!(context["matched"], false);
    assert!(result["matching_adjustment_ids"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(app.editor.session.state(), public);
    before.unchanged(&app);
    for selected in [true, false] {
        let result = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "group.query",
                json!({"selected":selected}),
            ))
            .unwrap()["result"]
            .clone();
        assert!(!result["rows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["kind"] == "adjustment"));
        assert_eq!(app.editor.session.state(), public);
        before.unchanged(&app);
    }
    assert!(app
        .editor
        .core("group.set", json!({"group_id":group,"visible":false})));
    let public = app.editor.session.state();
    let before = Content::capture(&app);
    for (args, included) in [
        (json!({"visible":true}), true),
        (json!({"effective_visible":true}), false),
        (json!({"visible":true,"effective_visible":false}), true),
    ] {
        let result = app
            .editor
            .session
            .apply(crate::application::Action::new("group.query", args))
            .unwrap()["result"]
            .clone();
        assert_eq!(
            result["rows"]
                .as_array()
                .unwrap()
                .iter()
                .any(|row| row["kind"] == "adjustment" && row["id"] == adjustment),
            included
        );
        assert_eq!(app.editor.session.state(), public);
        before.unchanged(&app);
    }
}
