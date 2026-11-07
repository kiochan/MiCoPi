//! Link buttons share metadata admission, independent from painting selection.
use super::super::group_delete_ui_tests::{assert_contact, context, down, finish, samples, studio};
use super::*;
use crate::application::ToolKind;
use crate::desktop::canvas_geometry::mask_ui_tests::{click, Snapshot};

fn frame(app: &mut Studio, ctx: &Context, width: f32, events: Vec<Event>, time: f64) -> FullOutput {
    ctx.run(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 1100.))),
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
fn control(app: &Studio, ctx: &Context, linked: bool) -> Rect {
    ctx.data_mut(|data| {
        data.get_temp(Id::new((
            "layer-links-control",
            app.editor.session.doc.identity,
            linked,
        )))
    })
    .unwrap()
}
fn select(app: &mut Studio, ids: &[u64], active: u64) {
    assert!(app.editor.core(
        "layer.selection.set",
        json!({"layer_ids":ids,"active_layer_id":active})
    ));
}

#[test]
fn layer_links_buttons_link_and_unlink_explicit_selection_with_one_undo_and_no_paint_target_change()
{
    let mut app = studio(false);
    select(&mut app, &[1, 3], 1);
    let ctx = context(&app);
    for t in 0..3 {
        frame(&mut app, &ctx, 360., vec![], t as f64);
    }
    let before = Snapshot::capture(&app.editor);
    let revision = app.editor.session.revision;
    let target = app.editor.session.doc.mask_state().target;
    let linked = control(&app, &ctx, true).center();
    frame(&mut app, &ctx, 360., click(linked), 3.);
    assert_eq!(app.editor.session.doc.layer_link_sets(), vec![vec![1, 3]]);
    assert_eq!(app.editor.session.revision, revision + 1);
    assert_eq!(app.editor.session.doc.selected_layer_ids(), [1, 3]);
    assert_eq!(
        app.editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active),
        1
    );
    assert_eq!(app.editor.session.doc.mask_state().target, target);
    assert!(app.editor.core("history.undo", json!({})));
    before.assert_document(&app.editor);
    assert!(app.editor.session.doc.layer_link_sets().is_empty());
    assert!(app.editor.core("history.redo", json!({})));
    select(&mut app, &[3], 3);
    frame(&mut app, &ctx, 360., vec![], 4.);
    let unlinked = control(&app, &ctx, false).center();
    frame(&mut app, &ctx, 360., click(unlinked), 5.);
    assert!(app.editor.session.doc.layer_link_sets().is_empty());
    assert_eq!(app.editor.session.doc.selected_layer_ids(), [3]);
    assert_eq!(
        app.editor
            .session
            .doc
            .layer_id(app.editor.session.doc.active),
        3
    );
}

#[test]
fn layer_links_button_preserves_pending_name_and_owned_or_external_brush_hand_contacts() {
    for kind in [ToolKind::Brush, ToolKind::Hand] {
        for local in [false, true] {
            let mut app = studio(false);
            select(&mut app, &[1, 2], 1);
            let ctx = context(&app);
            for t in 0..3 {
                frame(&mut app, &ctx, 420., vec![], t as f64);
            }
            let id = Id::new(("layer-name", app.editor.session.doc.identity, 1u64));
            ctx.data_mut(|data| data.insert_temp(id, "Pending ink name".to_owned()));
            ctx.memory_mut(|memory| memory.request_focus(id));
            frame(&mut app, &ctx, 420., vec![], 3.);
            let start = down(&mut app, kind, local);
            let before = Snapshot::capture(&app.editor);
            let transcript = samples(&mut app, start);
            let position = control(&app, &ctx, true).center();
            frame(&mut app, &ctx, 420., click(position), 4.);
            assert!(
                app.editor.status.contains("stroke_active"),
                "{}",
                app.editor.status
            );
            before.assert_same(&app.editor);
            assert_eq!(
                ctx.data_mut(|data| data.get_temp::<String>(id)).as_deref(),
                Some("Pending ink name")
            );
            assert_contact(&app, kind, local);
            assert_eq!(samples(&mut app, start), transcript);
            finish(&mut app, kind, start);
        }
    }
}

#[test]
fn layer_links_controls_wrap_translated_feedback_without_changing_document_state() {
    crate::i18n::load_test_catalogs();
    let mut app = studio(false);
    select(&mut app, &[1, 2], 1);
    assert!(app
        .editor
        .core("layer.links.set", json!({"layer_ids":[1,2],"linked":true})));
    let ctx = context(&app);
    fn collect(shape: &Shape, texts: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(text) => texts.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
            )),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, texts);
                }
            }
            _ => {}
        }
    }
    for language in crate::i18n::Language::ALL {
        crate::i18n::set_language(language);
        for width in [176., 260., 680.] {
            let before = Snapshot::capture(&app.editor);
            let mut output = frame(&mut app, &ctx, width, vec![], 0.);
            for t in 1..3 {
                output = frame(&mut app, &ctx, width, vec![], t as f64);
            }
            let mut texts = Vec::new();
            for shape in &output.shapes {
                collect(&shape.shape, &mut texts);
            }
            for title in ["Link", "Unlink", "Painting stays on the active layer."] {
                let label = localize(title);
                let (_, bounds) = texts
                    .iter()
                    .find(|(text, _)| text.as_str() == label.as_ref())
                    .unwrap_or_else(|| panic!("{language:?} {title}: {texts:?}"));
                assert!(
                    bounds.right() <= width + 0.5,
                    "{language:?} {width}: {bounds:?}"
                );
            }
            for linked in [true, false] {
                assert!(control(&app, &ctx, linked).right() <= width + 0.5);
            }
            before.assert_same(&app.editor);
        }
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
