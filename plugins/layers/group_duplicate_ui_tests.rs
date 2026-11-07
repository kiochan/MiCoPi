//! Group copy preserves shared contact admission and explicit mask feedback.
use super::group_delete_ui_tests::{
    assert_contact, context, down, draft_frame, finish, key, option, properties_frame, raw,
    rendered_labels, samples,
};
use super::*;
use crate::application::ToolKind;
use crate::desktop::canvas_geometry::mask_ui_tests::{click, label, Snapshot};
use crate::engine::{
    document::Composition,
    raster_masks::{MaskOwner, PaintTarget, PlaneView},
};
use std::collections::BTreeMap;

fn studio(populated: bool) -> Studio {
    let mut app = super::group_delete_ui_tests::studio(populated);
    assert!(app
        .editor
        .core("layer.set", json!({"layer_id":2,"opacity":137./255.})));
    app.editor.sync_from_session();
    app.editor.session.doc.compose();
    app
}
fn open_duplicate(app: &mut Studio, ctx: &Context, base: f64) -> Pos2 {
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
    option(&output, "Duplicate Group")
}
struct Source {
    composition: Composition,
    active: u64,
    rasters: BTreeMap<u64, (Vec<u32>, usize)>,
    masks: BTreeMap<u64, (Vec<u8>, Option<usize>)>,
    surface: Vec<u32>,
}
impl Source {
    fn capture(app: &Studio) -> Self {
        let doc = &app.editor.session.doc;
        Self {
            composition: doc.composition(),
            active: doc.layer_id(doc.active),
            rasters: doc
                .layers
                .iter()
                .enumerate()
                .map(|(index, layer)| {
                    (
                        doc.layer_id(index),
                        (layer.pixels.clone(), layer.pixels.as_ptr() as usize),
                    )
                })
                .collect(),
            masks: doc
                .mask_state()
                .items
                .iter()
                .map(|mask| {
                    let pointer = match doc.mask_plane(mask.id).unwrap() {
                        PlaneView::Dense(data) => Some(data.as_ptr() as usize),
                        PlaneView::Uniform(_) => None,
                    };
                    (mask.id, (raw(app, mask.id), pointer))
                })
                .collect(),
            surface: doc.composite.clone(),
        }
    }
    fn assert_original_buffers(&self, app: &Studio) {
        let doc = &app.editor.session.doc;
        for (id, (pixels, pointer)) in &self.rasters {
            let layer = &doc.layers[doc.layer_index(*id).unwrap()];
            assert_eq!(&layer.pixels, pixels);
            assert_eq!(layer.pixels.as_ptr() as usize, *pointer);
        }
        for (id, (samples, pointer)) in &self.masks {
            assert_eq!(&raw(app, *id), samples);
            let actual = match doc.mask_plane(*id).unwrap() {
                PlaneView::Dense(data) => Some(data.as_ptr() as usize),
                PlaneView::Uniform(_) => None,
            };
            assert_eq!(&actual, pointer);
        }
    }
    fn assert_undo(&self, app: &Studio) {
        let doc = &app.editor.session.doc;
        let mut expected = self.composition.clone();
        let actual = doc.composition();
        assert!(actual.next_layer_id >= expected.next_layer_id);
        assert!(actual.next_group_id >= expected.next_group_id);
        assert!(actual.raster_masks.next_id >= expected.raster_masks.next_id);
        expected.next_layer_id = actual.next_layer_id;
        expected.next_group_id = actual.next_group_id;
        expected.raster_masks.next_id = actual.raster_masks.next_id;
        assert_eq!(actual, expected);
        assert_eq!(doc.layer_id(doc.active), self.active);
        assert_eq!(doc.composite, self.surface);
        self.assert_original_buffers(app);
    }
}

#[test]
fn actual_group_duplicate_menu_copies_owned_masks_and_retains_original_buffers_through_undo() {
    for (target, active) in [
        (json!({"kind":"layer_pixels"}), 1),
        (json!({"kind":"mask","id":1}), 1),
        (json!({"kind":"mask","id":2}), 1),
        (json!({"kind":"mask","id":3}), 1),
        (json!({"kind":"mask","id":4}), 1),
        (json!({"kind":"mask","id":1}), 3),
    ] {
        let mut app = studio(true);
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[1,2,3],"active_layer_id":active})
        ));
        assert!(app.editor.core("mask.target", json!({"target":target})));
        let ctx = context(&app);
        app.editor.focus_group_ui(&ctx, Some(1));
        let before = Source::capture(&app);
        let revision = app.editor.session.revision;
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let point = open_duplicate(&mut app, &ctx, 0.);
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
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(3));
        assert_eq!(app.editor.session.doc.layers.len(), 5);
        assert_eq!(app.editor.session.doc.selected_layer_ids(), vec![4, 5]);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            if active == 1 { 4 } else { 5 }
        );
        let expected_target = match target["id"].as_u64() {
            Some(1) => PaintTarget::Mask { id: 5 },
            Some(2) => PaintTarget::Mask { id: 6 },
            Some(3) => PaintTarget::Mask { id: 3 },
            Some(4) => PaintTarget::Mask { id: 7 },
            _ => PaintTarget::LayerPixels,
        };
        assert_eq!(app.editor.session.doc.mask_state().target, expected_target);
        before.assert_original_buffers(&app);
        for (source, copied, owner) in [
            (1, 5, MaskOwner::Layer { id: 4 }),
            (2, 6, MaskOwner::Group { id: 3 }),
            (4, 7, MaskOwner::Group { id: 4 }),
        ] {
            let mut metadata = before
                .composition
                .raster_masks
                .items
                .iter()
                .find(|mask| mask.id == source)
                .unwrap()
                .clone();
            metadata.id = copied;
            metadata.owner = owner;
            assert_eq!(
                app.editor
                    .session
                    .doc
                    .mask_state()
                    .items
                    .iter()
                    .find(|mask| mask.id == copied),
                Some(&metadata)
            );
            assert_eq!(raw(&app, copied), raw(&app, source));
        }
        assert_eq!(
            app.editor.session.doc.layers[app.editor.session.doc.layer_index(5).unwrap()].opacity,
            137
        );
        assert!(app
            .editor
            .core("mask.fill", json!({"mask_id":5,"value":17})));
        assert_eq!(raw(&app, 1), before.masks[&1].0);
        assert_eq!(raw(&app, 5), vec![17; 12 * 8]);
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(raw(&app, 5), before.masks[&1].0);
        assert!(app.editor.core("history.undo", json!({})));
        app.editor.session.doc.compose();
        before.assert_undo(&app);
        assert!(app.editor.core("history.redo", json!({})));
        assert_eq!(app.editor.session.doc.mask_state().target, expected_target);
        assert_eq!(raw(&app, 5), before.masks[&1].0);
        before.assert_original_buffers(&app);
    }
}

#[test]
fn actual_empty_masked_group_copy_preserves_raster_selection_and_remaps_only_its_target() {
    for target in [
        json!({"kind":"mask","id":2}),
        json!({"kind":"mask","id":1}),
        json!({"kind":"layer_pixels"}),
    ] {
        let mut app = studio(false);
        assert!(app.editor.core("mask.target", json!({"target":target})));
        let ctx = context(&app);
        app.editor.focus_group_ui(&ctx, Some(1));
        let before = Source::capture(&app);
        let selected = app.editor.session.doc.selected_layer_ids();
        let point = open_duplicate(&mut app, &ctx, 0.);
        properties_frame(&mut app, &ctx, click(point), 5., vec![]);
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(2));
        assert_eq!(app.editor.session.doc.layers.len(), 3);
        assert_eq!(app.editor.session.doc.selected_layer_ids(), selected);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            before.active
        );
        assert_eq!(
            app.editor.session.doc.mask_state().target,
            match target["id"].as_u64() {
                Some(2) => PaintTarget::Mask { id: 4 },
                Some(1) => PaintTarget::Mask { id: 1 },
                _ => PaintTarget::LayerPixels,
            }
        );
        assert_eq!(raw(&app, 4), before.masks[&2].0);
        before.assert_original_buffers(&app);
        assert!(app.editor.core("history.undo", json!({})));
        app.editor.session.doc.compose();
        before.assert_undo(&app);
    }
}

#[test]
fn actual_group_duplicate_menu_preserves_dirty_names_and_every_native_contact_kind() {
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
            edit.push(Event::Text("Pending copied folder".into()));
            properties_frame(&mut app, &ctx, edit, 2., vec![]);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let expected_samples = samples(&mut app, start);
            let point = open_duplicate(&mut app, &ctx, 3.);
            properties_frame(
                &mut app,
                &ctx,
                click(point),
                8.,
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Pending copied cel"}),
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
                Some("Pending copied cel".into())
            );
            let output = properties_frame(&mut app, &ctx, vec![], 9., vec![]);
            assert!(rendered_labels(&output)
                .iter()
                .any(|text| text == "Pending copied folder"));
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
                "Pending copied folder"
            );
        }
    }
}

fn command(app: &Studio) -> u32 {
    app.editor
        .settings
        .keys
        .commands()
        .find(|command| command.keywords == "group.duplicate")
        .unwrap()
        .ui
}
fn select_duplicate(app: &mut Studio) {
    let index = app
        .editor
        .settings
        .keys
        .commands()
        .enumerate()
        .find_map(|(index, command)| (command.keywords == "group.duplicate").then_some(index))
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
fn real_group_duplicate_palette_form_preserves_contacts_through_run_and_cancel() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(true);
            let ctx = context(&app);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let history = app.editor.session.doc.history_bytes();
            app.editor.actions.push(Action::OpenPalette);
            app.run_actions(&ctx);
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
            select_duplicate(&mut app);
            draft_frame(&mut app, &ctx, key(Key::Enter, Modifiers::NONE), 2.);
            app.run_actions(&ctx);
            assert!(app.editor.guarded_command_form());
            assert!(app.editor.contact_draft_modal());
            app.editor.dispatch_layer_pending_ui(
                vec![crate::application::Action::new(
                    "layer.rename",
                    json!({"layer_id":1,"name":"Copy-form local name"}),
                )],
                Some(&ctx),
            );
            let mut output = draft_frame(&mut app, &ctx, vec![], 3.);
            for time in 4..6 {
                output = draft_frame(&mut app, &ctx, vec![], time as f64);
            }
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
                "Missing shared rejection: {:#?}",
                rendered_labels(&output)
            );
            before.assert_document(&app.editor);
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_contact(&app, kind, local);
            assert_eq!(samples(&mut app, start).len(), 1);
            draft_frame(&mut app, &ctx, key(Key::Escape, Modifiers::NONE), 8.);
            assert!(app.editor.command_form.is_none());
            before.assert_document(&app.editor);
            assert_contact(&app, kind, local);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn queued_group_duplicate_form_defers_auto_name_before_shared_admission() {
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
                    json!({"layer_id":1,"name":"Queued copy draft"}),
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
            assert!(app.editor.command_form.is_none());
            before.assert_same(&app.editor);
            finish(&mut app, kind, start);
        }
    }
}
