use super::*;
use crate::desktop::tests::studio;
use crate::engine::{document::Composition, raster_masks::PlaneView};

pub(in crate::desktop) fn fixture(count: usize) -> (Studio, Vec<u64>) {
    let mut app = studio();
    app.editor.session = crate::application::Session::new(
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap(),
    )
    .unwrap();
    assert!(app
        .editor
        .core("canvas.new", json!({"width":12,"height":8})));
    let mut owners = vec![json!({"kind":"layer","id":1})];
    for _ in 1..count {
        let response = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "group.create",
                json!({"members":[]}),
            ))
            .unwrap();
        owners.push(json!({"kind":"group","id":response["result"]["group_id"]}));
    }
    assert!(app.editor.core(
        "selection.shape",
        json!({"kind":"rectangle","x":3,"y":2,"width":6,"height":4})
    ));
    assert!(app
        .editor
        .core("layer.fill_color", json!({"color":"#5477CC"})));
    assert!(app.editor.core("selection.deselect", json!({})));
    let mut masks = Vec::new();
    for owner in owners {
        masks.push(
            app.editor
                .session
                .apply(crate::application::Action::new(
                    "mask.create",
                    json!({"owner":owner,"select":false}),
                ))
                .unwrap()["result"]["mask_id"]
                .as_u64()
                .unwrap(),
        );
    }
    assert!(app.editor.core(
        "mask.set",
        json!({"mask_id":masks[0],"density":203,"feather_radius_px":2})
    ));
    assert!(app.editor.core(
        "mask.target",
        json!({"target":{"kind":"mask","id":masks[0]}})
    ));
    app.editor.sync_from_session();
    (app, masks)
}
fn raw(editor: &Editor) -> Vec<(u64, Vec<u8>)> {
    editor
        .session
        .doc
        .mask_state()
        .items
        .iter()
        .map(|mask| {
            let samples = match editor.session.doc.mask_plane(mask.id).unwrap() {
                PlaneView::Uniform(value) => {
                    vec![value; editor.session.doc.width * editor.session.doc.height]
                }
                PlaneView::Dense(samples) => samples.to_vec(),
            };
            (mask.id, samples)
        })
        .collect()
}
pub(in crate::desktop) struct Snapshot {
    public: Value,
    composition: Composition,
    raw: Vec<(u64, Vec<u8>)>,
    pixels: Vec<Vec<u32>>,
    surface: Vec<u32>,
    history: usize,
}
impl Snapshot {
    pub(in crate::desktop) fn capture(editor: &Editor) -> Self {
        Self {
            public: editor.session.state(),
            composition: editor.session.doc.composition(),
            raw: raw(editor),
            pixels: editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect(),
            surface: editor.session.doc.composite.clone(),
            history: editor.session.doc.history_bytes(),
        }
    }
    pub(in crate::desktop) fn assert_same(&self, editor: &Editor) {
        assert_eq!(editor.session.state(), self.public);
        self.assert_document(editor);
        assert_eq!(editor.session.doc.history_bytes(), self.history);
    }
    pub(in crate::desktop) fn assert_document(&self, editor: &Editor) {
        assert_eq!(editor.session.doc.composition(), self.composition);
        assert_eq!(raw(editor), self.raw);
        assert_eq!(
            editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            self.pixels
        );
        assert_eq!(editor.session.doc.composite, self.surface);
    }
}
pub(in crate::desktop) fn label(shape: &Shape, text: &str) -> Option<Pos2> {
    match shape {
        Shape::Text(value) if value.galley.text() == text => {
            Some(value.pos + value.galley.size() * 0.5)
        }
        Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, text)),
        _ => None,
    }
}
pub(in crate::desktop) fn click(position: Pos2) -> Vec<Event> {
    vec![
        Event::PointerMoved(position),
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        },
    ]
}
pub(in crate::desktop) fn frame(
    app: &mut Studio,
    ctx: &Context,
    size: Vec2,
    events: Vec<Event>,
    time: f64,
) -> FullOutput {
    crate::i18n::set_language(app.editor.settings.ui.language);
    if !ctx.data_mut(|data| {
        data.get_temp::<bool>(Id::new("mask-geometry-fonts"))
            .unwrap_or(false)
    }) {
        theme::fonts(ctx);
        theme::apply(ctx, &app.editor.settings.ui);
        ctx.data_mut(|data| data.insert_temp(Id::new("mask-geometry-fonts"), true));
    }
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            time: Some(time),
            focused: true,
            ..Default::default()
        },
        |ctx| app.editor.canvas_geometry_window(ctx),
    )
}

#[test]
fn geometry_dialog_uses_shared_mask_budget_and_preserves_target_on_rejection_and_cancel() {
    let (mut app, masks) = fixture(5);
    let before = Snapshot::capture(&app.editor);
    app.editor.open_canvas_geometry(false);
    let mut dialog = app.editor.geometry.take().unwrap();
    dialog.width = 4096;
    dialog.height = 4096;
    assert!(4096usize * 4096 * app.editor.session.doc.layers.len() <= 64 * 1024 * 1024);
    assert!(app
        .editor
        .plan_canvas_geometry(&dialog)
        .unwrap_err()
        .contains("budget"));
    assert!(!app.editor.apply_canvas_geometry(&mut dialog));
    before.assert_same(&app.editor);
    app.editor.geometry = Some(dialog);
    let ctx = Context::default();
    for time in 0..3 {
        let _ = frame(&mut app, &ctx, vec2(460., 380.), vec![], time as f64);
    }
    let enabled = ctx
        .data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new("canvas-geometry-apply")))
        .unwrap()
        .1;
    assert!(!enabled);
    let _ = frame(
        &mut app,
        &ctx,
        vec2(460., 380.),
        vec![Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        3.,
    );
    assert!(app.editor.geometry.is_none());
    before.assert_same(&app.editor);
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        crate::engine::raster_masks::PaintTarget::Mask { id: masks[0] }
    );
}

#[test]
fn geometry_dialog_plan_apply_undo_and_stale_metadata_keep_mask_ownership_and_target() {
    let (mut app, masks) = fixture(1);
    let before = Snapshot::capture(&app.editor);
    app.editor.open_canvas_geometry(false);
    let mut dialog = app.editor.geometry.take().unwrap();
    dialog.width = 17;
    dialog.height = 11;
    dialog.anchor = 8;
    let plan = app.editor.plan_canvas_geometry(&dialog).unwrap();
    assert_eq!(plan["raster_mask_count"], 1);
    assert_eq!(plan["pixel_selection_count"], 1);
    assert_eq!(plan["estimated_raster_mask_bytes"], 17 * 11);
    assert_eq!(plan["requires_a8_sampler"], true);
    assert_eq!(plan["a8_sampler_available"], true);
    before.assert_same(&app.editor);
    assert!(
        app.editor.apply_canvas_geometry(&mut dialog),
        "{}",
        dialog.error
    );
    assert_eq!(
        (app.editor.session.doc.width, app.editor.session.doc.height),
        (17, 11)
    );
    assert_eq!(
        app.editor.session.doc.mask_state().target,
        crate::engine::raster_masks::PaintTarget::Mask { id: masks[0] }
    );
    assert_eq!(
        app.editor.session.doc.mask_state().items[0].feather_radius_px,
        2
    );
    assert!(app.editor.core("history.undo", json!({})));
    before.assert_document(&app.editor);
    app.editor.open_canvas_geometry(true);
    let mut dialog = app.editor.geometry.take().unwrap();
    dialog.width = 6;
    dialog.height = 4;
    assert!(app
        .editor
        .core("mask.set", json!({"mask_id":masks[0],"density":173})));
    let changed = Snapshot::capture(&app.editor);
    assert!(!app.editor.apply_canvas_geometry(&mut dialog));
    assert_eq!(dialog.error, "The canvas changed. Reopen this dialog.");
    changed.assert_same(&app.editor);
    assert_eq!(
        app.editor
            .session
            .apply(crate::application::Action::new("replay.verify", json!({})))
            .unwrap()["result"]["valid"],
        true
    );
}

#[test]
fn geometry_and_trim_fixed_footers_fit_short_narrow_english_and_cjk() {
    for language in crate::i18n::Language::ALL {
        let (mut app, _) = fixture(1);
        app.editor.settings.ui.language = language;
        for trim in [false, true] {
            if trim {
                app.editor.open_trim_canvas();
            } else {
                app.editor.open_canvas_geometry(false);
            }
            let before = Snapshot::capture(&app.editor);
            let ctx = Context::default();
            for (index, size) in [vec2(360., 280.), vec2(700., 500.)].into_iter().enumerate() {
                let mut out = frame(&mut app, &ctx, size, vec![], index as f64 * 4.);
                for time in 1..3 {
                    out = frame(&mut app, &ctx, size, vec![], (index * 4 + time) as f64);
                }
                for source in ["Apply", "Cancel"] {
                    let text = crate::i18n::text_in(language, source);
                    let (clip, point) = out
                        .shapes
                        .iter()
                        .find_map(|shape| {
                            label(&shape.shape, &text).map(|point| (shape.clip_rect, point))
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "{language:?},trim={trim}: {source}; viewport={:?}, apply={:?}",
                                ctx.content_rect(),
                                ctx.data_mut(|data| data.get_temp::<(Rect, bool)>(Id::new(
                                    if trim {
                                        "canvas-trim-apply"
                                    } else {
                                        "canvas-geometry-apply"
                                    }
                                )))
                            )
                        });
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains(point),
                        "{language:?},trim={trim}: {point:?}/{size:?}"
                    );
                    assert!(
                        clip.contains(point),
                        "{language:?},trim={trim}: {point:?}/{clip:?}"
                    );
                }
            }
            before.assert_same(&app.editor);
            app.editor.geometry = None;
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
