use super::*;
use crate::desktop::canvas_geometry::mask_ui_tests::{fixture, frame, label, Snapshot};

#[test]
fn image_size_mask_plan_keeps_selection_and_raster_budgets_separate_and_apply_target_stable() {
    let (mut app, masks) = fixture(1);
    app.editor.open_image_size();
    let mut dialog = app.editor.geometry.take().unwrap();
    let mut draft = dialog.image_size.take().unwrap();
    draft.width = 6.;
    let before = Snapshot::capture(&app.editor);
    let plan = app.editor.plan_image_size(&draft).unwrap();
    assert_eq!(plan["mask_count"], 1);
    assert_eq!(plan["raster_mask_count"], 1);
    assert_eq!(plan["estimated_mask_bytes"], 24);
    assert_eq!(plan["estimated_raster_mask_bytes"], 24);
    assert_eq!(plan["estimated_effective_mask_bytes"], 24);
    assert_eq!(plan["raster_mask_feather_policy"], "fixed_canvas_pixels");
    before.assert_same(&app.editor);
    assert!(
        app.editor.apply_image_size(&dialog, &mut draft),
        "{}",
        draft.error
    );
    assert_eq!(
        (app.editor.session.doc.width, app.editor.session.doc.height),
        (6, 4)
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
}

#[test]
fn image_size_mask_estimates_stay_in_scroll_body_and_footer_is_visible_in_cjk() {
    for language in crate::i18n::Language::ALL {
        let (mut app, _) = fixture(1);
        app.editor.settings.ui.language = language;
        app.editor.open_image_size();
        let before = Snapshot::capture(&app.editor);
        let ctx = Context::default();
        for (index, size) in [vec2(360., 280.), vec2(460., 380.), vec2(1280., 900.)]
            .into_iter()
            .enumerate()
        {
            let mut out = frame(&mut app, &ctx, size, vec![], (index * 4) as f64);
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
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(point),
                    "{language:?}:{source} {point:?}/{size:?}"
                );
                assert!(
                    clip.contains(point),
                    "{language:?}:{source} {point:?}/{clip:?}"
                );
            }
            if size.y > 800. {
                for source in ["Raster Mask Buffers", "Effective Mask Cache (estimate)"] {
                    let text = format!("{}: 0.0 MiB", crate::i18n::text_in(language, source));
                    assert!(
                        out.shapes
                            .iter()
                            .any(|shape| label(&shape.shape, &text).is_some()),
                        "{language:?}:{text}"
                    );
                }
            }
        }
        before.assert_same(&app.editor);
    }
    crate::i18n::set_language(crate::i18n::Language::English);
}
