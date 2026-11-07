//! Real Arrange controls capture exact rasters and use shared idle admission.
use super::*;
use crate::application::{Pointer, PointerPhase, ToolKind};
use crate::desktop::tests::studio;
use crate::engine::document::Composition;

fn app(masked: bool) -> Studio {
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
        .core("canvas.new", json!({"width":24,"height":16})));
    for (i, (x, y, w, h, color)) in [
        (2, 2, 3, 2, "#A46A91"),
        (7, 6, 4, 3, "#7CA6B1"),
        (18, 11, 2, 2, "#E8CA91"),
    ]
    .into_iter()
    .enumerate()
    {
        if i > 0 {
            assert!(app.editor.core("layer.add", json!({})));
        }
        assert!(app.editor.core(
            "selection.shape",
            json!({"kind":"rectangle","x":x,"y":y,"width":w,"height":h})
        ));
        assert!(app.editor.core("layer.fill_color", json!({"color":color})));
        assert!(app.editor.core("selection.deselect", json!({})));
    }
    assert!(app.editor.core(
        "layer.links.set",
        json!({"layer_ids":[1,2,3],"linked":true})
    ));
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1,2],"active_layer_id":1})
    ));
    if masked {
        for (owner, value) in [(2, 89), (3, 173)] {
            let result = app
                .editor
                .session
                .apply(crate::application::Action::new(
                    "mask.create",
                    json!({"owner":{"kind":"layer","id":owner},"select":false}),
                ))
                .unwrap();
            let mask = result["result"]["mask_id"].as_u64().unwrap();
            assert!(app
                .editor
                .core("mask.fill", json!({"mask_id":mask,"value":value})));
        }
        assert!(app
            .editor
            .core("mask.set", json!({"mask_id":1,"enabled":false})));
        assert!(app
            .editor
            .core("mask.target", json!({"target":{"kind":"mask","id":2}})));
    }
    for x in [0, 1] {
        assert!(app.editor.core(
            "selection.shape",
            json!({"kind":"rectangle","x":x,"y":1,"width":3,"height":2})
        ));
    }
    app.editor.sync_from_session();
    app
}
fn ctx(app: &Studio) -> Context {
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
            time: Some(time),
            events,
            focused: true,
            ..Default::default()
        },
        |ctx| {
            CentralPanel::default().show(ctx, |ui| app.editor.layers_ui(ui));
        },
    )
}
fn warm(app: &mut Studio, ctx: &Context, size: Vec2, time: f64) {
    for n in 0..3 {
        frame(app, ctx, size, vec![], time + n as f64 / 100.);
    }
}
fn click(app: &mut Studio, ctx: &Context, size: Vec2, p: Pos2, time: f64) -> FullOutput {
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains(p),
        "{p:?}/{size:?}"
    );
    for (n, pressed) in [true, false].into_iter().enumerate() {
        frame(
            app,
            ctx,
            size,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
            time + n as f64 / 100.,
        );
    }
    frame(app, ctx, size, vec![], time + 0.02);
    frame(app, ctx, size, vec![], time + 0.03)
}
fn label(shape: &Shape, text: &str) -> Option<Pos2> {
    match shape {
        Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
        Shape::Vec(items) => items.iter().find_map(|shape| label(shape, text)),
        _ => None,
    }
}
fn open(app: &mut Studio, ctx: &Context, size: Vec2, time: f64) -> FullOutput {
    let out = frame(app, ctx, size, vec![], time);
    let title = crate::i18n::text("Arrange");
    let position = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, &title))
        .unwrap();
    click(app, ctx, size, position, time + 0.1)
}
fn control(ctx: &Context, mode: &str) -> (Rect, bool) {
    ctx.data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new(("layer-arrangement-control", mode))))
        .unwrap()
}
fn raw(app: &Studio, id: u64) -> Vec<u8> {
    let plane = app.editor.session.doc.mask_plane(id).unwrap();
    (0..384).map(|i| plane.sample(i)).collect()
}
fn shifted<T: Copy>(source: &[T], dx: i32, dy: i32, fill: T) -> Vec<T> {
    (0..384)
        .map(|i| {
            let x = (i % 24) as i32 - dx;
            let y = (i / 24) as i32 - dy;
            if (0..24).contains(&x) && (0..16).contains(&y) {
                source[y as usize * 24 + x as usize]
            } else {
                fill
            }
        })
        .collect()
}
struct Before {
    public: Value,
    composition: Composition,
    pixels: Vec<Vec<u32>>,
    raw: Vec<(u64, Vec<u8>)>,
    surface: Vec<u32>,
    history: usize,
}
impl Before {
    fn capture(app: &Studio) -> Self {
        Self {
            public: app.editor.session.state(),
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
    fn same(&self, app: &Studio) {
        assert_eq!(app.editor.session.state(), self.public);
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
    let mut values: Vec<_> = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["source"]["samples"].as_array())
        .flatten()
        .cloned()
        .collect();
    values.extend(
        app.editor
            .session
            .replay_samples
            .iter()
            .map(|sample| serde_json::to_value(sample).unwrap()),
    );
    values
}
fn pointer(tool: ToolKind, phase: PointerPhase, time: u32) -> Pointer {
    Pointer {
        phase,
        x: 2.,
        y: 2.,
        pressure: 0.4,
        time,
        pen: true,
        eraser: false,
        tool_override: Some(tool),
        view_x: Some(40.),
        view_y: Some(40.),
    }
}

#[test]
fn arrangement_control_moves_only_captured_selected_rasters_and_linked_masks_once() {
    let mut app = app(true);
    let ctx = ctx(&app);
    let size = vec2(420., 1200.);
    warm(&mut app, &ctx, size, 0.);
    let before = Before::capture(&app);
    let active = app.editor.session.doc.active;
    let selected = app.editor.session.doc.selected_layer_ids();
    let target = app.editor.session.doc.mask_state().target;
    let selections = app.editor.session.doc.selection().clone();
    let recipe = app.editor.session.doc.last_transform();
    open(&mut app, &ctx, size, 1.);
    before.same(&app);
    let (button, enabled) = control(&ctx, "left");
    assert!(enabled);
    let revision = app.editor.session.revision;
    let count = app
        .editor
        .session
        .replay_journal(false)
        .unwrap()
        .entries
        .len();
    click(&mut app, &ctx, size, button.center(), 2.);
    assert_eq!(app.editor.session.doc.layers[0].pixels, before.pixels[0]);
    assert_eq!(
        app.editor.session.doc.layers[1].pixels,
        shifted(&before.pixels[1], -5, 0, 0)
    );
    assert_eq!(app.editor.session.doc.layers[2].pixels, before.pixels[2]);
    assert_eq!(raw(&app, 1), shifted(&before.raw[0].1, -5, 0, 255));
    assert_eq!(raw(&app, 2), before.raw[1].1);
    assert_eq!(app.editor.session.doc.active, active);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), selected);
    assert_eq!(app.editor.session.doc.mask_state().target, target);
    assert_eq!(app.editor.session.doc.selection(), &selections);
    assert_eq!(app.editor.session.doc.last_transform(), recipe);
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
    before.document(&app);
    assert!(app.editor.core("history.redo", json!({})));
    assert_eq!(raw(&app, 1), shifted(&before.raw[0].1, -5, 0, 255));
}

#[test]
fn arrangement_reference_enables_single_layer_canvas_alignment_and_distribution_requires_three_targets(
) {
    let mut app = app(false);
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[2],"active_layer_id":2})
    ));
    let ctx = ctx(&app);
    let size = vec2(420., 1000.);
    warm(&mut app, &ctx, size, 0.);
    open(&mut app, &ctx, size, 1.);
    assert!(!control(&ctx, "left").1);
    assert!(!control(&ctx, "distribute_horizontal_centers").1);
    let reference = ctx
        .data_mut(|data| data.get_temp::<Rect>(Id::new("layer-arrangement-reference")))
        .unwrap();
    let out = click(&mut app, &ctx, size, reference.center(), 2.);
    let canvas = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Canvas"))
        .unwrap();
    click(&mut app, &ctx, size, canvas, 3.);
    assert!(control(&ctx, "left").1);
    assert!(!control(&ctx, "distribute_horizontal_centers").1);
    let source = app.editor.session.doc.layers[1].pixels.clone();
    click(&mut app, &ctx, size, control(&ctx, "left").0.center(), 4.);
    assert_eq!(
        app.editor.session.doc.layers[1].pixels,
        shifted(&source, -7, 0, 0)
    );
    assert!(app.editor.core("history.undo", json!({})));
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[1,2,3],"active_layer_id":2})
    ));
    open(&mut app, &ctx, size, 5.);
    assert!(control(&ctx, "distribute_horizontal_centers").1);
    let first = app.editor.session.doc.layers[0].pixels.clone();
    let last = app.editor.session.doc.layers[2].pixels.clone();
    let middle = app.editor.session.doc.layers[1].pixels.clone();
    click(
        &mut app,
        &ctx,
        size,
        control(&ctx, "distribute_horizontal_centers").0.center(),
        6.,
    );
    assert_eq!(app.editor.session.doc.layers[0].pixels, first);
    assert_eq!(app.editor.session.doc.layers[2].pixels, last);
    assert_eq!(
        app.editor.session.doc.layers[1].pixels,
        shifted(&middle, 2, 0, 0)
    );
}

#[test]
fn arrangement_stale_popup_keeps_captured_ids_and_does_not_retarget_after_selection_changes() {
    let mut app = app(false);
    let ctx = ctx(&app);
    let size = vec2(420., 1000.);
    warm(&mut app, &ctx, size, 0.);
    open(&mut app, &ctx, size, 1.);
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":[2,3],"active_layer_id":2})
    ));
    let before = Before::capture(&app);
    frame(&mut app, &ctx, size, vec![], 2.);
    let captured = ctx
        .data_mut(|data| data.get_temp::<Draft>(Id::new(DRAFT)))
        .unwrap();
    assert_eq!(captured.ids, [1, 2]);
    let (button, enabled) = control(&ctx, "left");
    assert!(!enabled);
    click(&mut app, &ctx, size, button.center(), 3.);
    before.same(&app);
    // Closing and opening again deliberately captures the new selection.
    let out = frame(&mut app, &ctx, size, vec![], 4.);
    let arrange = out
        .shapes
        .iter()
        .find_map(|shape| label(&shape.shape, "Arrange"))
        .unwrap();
    click(&mut app, &ctx, size, arrange, 5.);
    open(&mut app, &ctx, size, 6.);
    assert_eq!(
        ctx.data_mut(|data| data.get_temp::<Draft>(Id::new(DRAFT)))
            .unwrap()
            .ids,
        [2, 3]
    );
}

#[test]
fn arrangement_plan_menu_and_rejection_preserve_local_external_brush_hand_contacts_and_name_drafts()
{
    for tool in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = app(true);
            let ctx = ctx(&app);
            let size = vec2(420., 1200.);
            warm(&mut app, &ctx, size, 0.);
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Down, 100))
                .unwrap();
            if local && tool == ToolKind::Brush {
                for time in 101..381 {
                    app.editor
                        .session
                        .pointer(pointer(tool, PointerPhase::Move, time))
                        .unwrap();
                }
            }
            app.editor.sync_from_session();
            app.editor.input_owned = local;
            app.editor.pen_active = local;
            app.editor.panning = (local && tool == ToolKind::Hand).then_some(Pos2::ZERO);
            app.editor.session.doc.compose();
            let before = Before::capture(&app);
            let transcript = samples(&mut app);
            let result = app
                .editor
                .session
                .apply(crate::application::Action::new(
                    "layer.arrange.plan",
                    json!({"layer_ids":[1,2],"mode":"left"}),
                ))
                .unwrap();
            assert_eq!(result["result"]["contact_active"], true);
            before.same(&app);
            let key = Id::new(("layer-name", app.editor.session.doc.identity, 1));
            ctx.data_mut(|data| data.insert_temp(key, "Kept name draft".to_owned()));
            open(&mut app, &ctx, size, 1.);
            before.same(&app);
            assert_eq!(samples(&mut app), transcript);
            click(&mut app, &ctx, size, control(&ctx, "left").0.center(), 2.);
            assert!(
                app.editor.status.contains("stroke_active"),
                "{}",
                app.editor.status
            );
            before.same(&app);
            assert_eq!(samples(&mut app), transcript);
            assert_eq!(
                ctx.data_mut(|data| data.get_temp::<String>(key)).as_deref(),
                Some("Kept name draft")
            );
            assert_eq!(app.editor.input_owned, local);
            assert_eq!(app.editor.pen_active, local);
            assert!(app.editor.session.stroke.active());
            app.editor
                .session
                .pointer(pointer(tool, PointerPhase::Up, 500))
                .unwrap();
            assert_eq!(samples(&mut app).len(), transcript.len() + 1);
        }
    }
}

#[test]
fn arrangement_four_language_narrow_wide_controls_and_search_stay_within_the_viewport() {
    use crate::i18n::Language;
    crate::i18n::load_test_catalogs();
    for language in [
        Language::English,
        Language::Chinese,
        Language::Japanese,
        Language::Korean,
    ] {
        for size in [vec2(280., 900.), vec2(700., 1100.)] {
            let mut app = app(false);
            app.editor.settings.ui.language = language;
            assert!(app.editor.core(
                "layer.selection.set",
                json!({"layer_ids":[1,2,3],"active_layer_id":2})
            ));
            let ctx = ctx(&app);
            warm(&mut app, &ctx, size, 0.);
            let before = Before::capture(&app);
            open(&mut app, &ctx, size, 1.);
            for (mode, _) in MODES {
                let (rect, enabled) = control(&ctx, mode);
                assert!(enabled);
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                    "{language:?} {mode}: {rect:?}/{size:?}"
                );
            }
            let out = frame(&mut app, &ctx, size, vec![], 2.);
            for source in ["Align To", "Align", "Distribute"] {
                let title = crate::i18n::text_in(language, source);
                let (clip, p) = out
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, &title).map(|p| (shape.clip_rect, p)))
                    .unwrap();
                assert!(
                    clip.contains(p) && Rect::from_min_size(Pos2::ZERO, size).contains(p),
                    "{language:?}/{source}: {p:?}/{clip:?}"
                );
            }
            for (id, title) in [
                (
                    "layer.arrange.plan",
                    "Plan Layer Alignment and Distribution",
                ),
                ("layer.arrange", "Align or Distribute Layers"),
            ] {
                let mut palette = Palette::new(
                    crate::ui::palette::Mode::Commands,
                    &app.editor.settings.keys,
                );
                palette.query(
                    crate::i18n::text_in(language, title).into_owned(),
                    &app.editor.settings.keys,
                );
                assert!(palette.results.iter().any(|&i| app
                    .editor
                    .settings
                    .keys
                    .command(i)
                    .keywords
                    == id));
            }
            before.same(&app);
        }
    }
    crate::i18n::set_language(Language::English);
}
