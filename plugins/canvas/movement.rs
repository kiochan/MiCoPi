//! Bounded outline-only movement gestures. No pixel, mask or JSON work on MOVE.
use super::*;
use crate::application::MoveTarget;
use crate::plugins::movement::MoveDestination;
pub(super) fn target_title(t: MoveTarget) -> &'static str {
    match t {
        MoveTarget::Auto => "Auto: Selection or Layer",
        MoveTarget::Layer => "Active Layer",
        MoveTarget::Selection => "Selected Pixels",
        MoveTarget::SelectionMask => "Selection Outline",
    }
}
pub(super) const TARGETS: [MoveTarget; 4] = [
    MoveTarget::Auto,
    MoveTarget::Layer,
    MoveTarget::Selection,
    MoveTarget::SelectionMask,
];
fn move_target_title(target: MoveTarget) -> &'static str {
    if target == MoveTarget::Auto {
        "Auto: Mask, Selection or Layer"
    } else {
        target_title(target)
    }
}
pub(super) struct Gesture {
    source: (u64, u64),
    start: Pos2,
    delta: [i32; 2],
    bounds: [usize; 4],
    destination: MoveDestination,
    layer_ids: Option<Vec<u64>>,
    guarded_contact: bool,
}
impl Editor {
    pub(super) fn guarded_movement_pending(&self) -> bool {
        self.movement_gesture
            .as_ref()
            .is_some_and(|gesture| gesture.guarded_contact)
    }
    pub(super) fn linked_movement_ids(
        &self,
    ) -> Result<Option<Vec<u64>>, crate::application::ActionError> {
        match crate::plugins::movement::destination(
            &self.session,
            self.session.movement_options().target,
            None,
        )? {
            MoveDestination::Layer { layer_id, .. } => {
                let ids = self
                    .session
                    .doc
                    .linked_layer_ids(layer_id)
                    .map_err(|error| crate::application::ActionError::new("layer_links", error))?;
                Ok((ids.len() > 1).then_some(ids))
            }
            _ => Ok(None),
        }
    }
    pub(super) fn movement_contact_guarded(&self) -> bool {
        self.session.doc.has_raster_masks()
            || self.linked_movement_ids().map_or(true, |ids| ids.is_some())
    }
    pub(super) fn guarded_movement_input(
        &self,
        held: crate::ui::keymap::InputState,
        middle: bool,
    ) -> bool {
        self.area_mode == 8
            && self.movement_contact_guarded()
            && !middle
            && !held.pan
            && !held.zoom
            && !held.zoom_out
    }
    pub(super) fn movement_down(&mut self, q: Pos2) -> bool {
        if self.area_mode != 8 {
            return false;
        }
        self.movement_start(q);
        true
    }
    pub(super) fn movement_start(&mut self, q: Pos2) -> bool {
        let layer_ids = match self.linked_movement_ids() {
            Ok(ids) => ids,
            Err(error) => {
                self.status = error.to_string();
                return false;
            }
        };
        let guarded_contact = self.session.doc.has_raster_masks() || layer_ids.is_some();
        // Linked and masked edits reach shared admission with the original
        // Brush/Hand stream intact. Retain ordinary single-layer cleanup.
        if !guarded_contact {
            self.finish_stroke();
        }
        if !q.is_finite() {
            self.status = "Invalid movement position".into();
            return false;
        }
        if let Some(ids) = &layer_ids {
            if let Err(error) = crate::plugins::movement::preview_linked(&self.session, ids) {
                self.status = error.to_string();
                return false;
            }
        }
        match crate::plugins::movement::preview(
            &self.session,
            self.session.movement_options().target,
            None,
        ) {
            Ok(preview) => {
                self.clear_local_contact();
                self.movement_gesture = Some(Gesture {
                    source: (self.session.doc.identity, self.session.revision),
                    start: q,
                    delta: [0, 0],
                    bounds: preview.bounds,
                    destination: preview.destination,
                    layer_ids,
                    guarded_contact,
                });
                self.input_owned = true;
                true
            }
            Err(error) => {
                self.status = error.to_string();
                false
            }
        }
    }
    pub(super) fn movement_move(&mut self, q: Pos2, constrain: bool) -> bool {
        let Some(g) = self.movement_gesture.as_mut() else {
            return false;
        };
        if !q.is_finite() {
            return true;
        }
        let mut delta = q - g.start;
        if constrain {
            if delta.x.abs() >= delta.y.abs() {
                delta.y = 0.;
            } else {
                delta.x = 0.;
            }
        }
        g.delta = [
            delta.x.round().clamp(-4096., 4096.) as i32,
            delta.y.round().clamp(-4096., 4096.) as i32,
        ];
        true
    }
    pub(super) fn movement_release(&mut self) {
        let Some(g) = self.movement_gesture.take() else {
            return;
        };
        self.input_owned = false;
        if g.source != (self.session.doc.identity, self.session.revision) {
            self.status = "Document changed; movement cancelled".into();
            return;
        }
        if g.delta == [0, 0] {
            return;
        }
        let args = if let Some(ids) = g.layer_ids {
            serde_json::json!({"target":"layer","layer_ids":ids,"dx":g.delta[0],"dy":g.delta[1]})
        } else {
            g.destination.action_args(g.delta[0], g.delta[1])
        };
        let mut action = crate::application::Action::new("canvas.move", args);
        action.expected_revision = Some(g.source.1);
        match self.session.apply(action) {
            Ok(_) => self.sync_from_session(),
            Err(error) => self.status = error.to_string(),
        }
    }
    pub(super) fn movement_preview(&self, painter: &Painter, color: Color32) {
        let Some(g) = &self.movement_gesture else {
            return;
        };
        let r = self.document_screen_rect(Rect::from_min_max(
            pos2(
                g.bounds[0] as f32 + g.delta[0] as f32,
                g.bounds[1] as f32 + g.delta[1] as f32,
            ),
            pos2(
                g.bounds[2] as f32 + g.delta[0] as f32,
                g.bounds[3] as f32 + g.delta[1] as f32,
            ),
        ));
        painter.rect_stroke(r, 0., Stroke::new(1.5, color), StrokeKind::Inside);
        painter.text(
            r.left_top() + vec2(6., 6.),
            Align2::LEFT_TOP,
            format!("{:+}, {:+} px", g.delta[0], g.delta[1]),
            FontId::proportional(self.settings.ui.small_font_size),
            color,
        );
    }
    pub(super) fn movement_options_ui(&mut self, ui: &mut Ui) {
        let width = ui.max_rect().width();
        ui.allocate_ui_with_layout(vec2(width, 0.), Layout::top_down(Align::Min), |ui| {
            ui.set_width(width);
            let mut p = self.session.movement_options();
            let before = p;
            ui.horizontal_wrapped(|ui| {
                ui.weak(tr("Target"));
                ComboBox::from_id_salt("move-target")
                    .width(
                        (width - self.settings.ui.item_spacing * 2.)
                            .min(self.settings.ui.font_size * 16.),
                    )
                    .wrap_mode(egui::TextWrapMode::Truncate)
                    .selected_text(tr(move_target_title(p.target)))
                    .show_ui(ui, |ui| {
                        for target in TARGETS {
                            ui.selectable_value(&mut p.target, target, tr(move_target_title(target)));
                        }
                    });
            });
            if p != before {
                self.core("toolbox.movement.options", serde_json::to_value(p).unwrap());
            }
            match crate::plugins::movement::destination(&self.session, p.target, None) {
                Ok(destination) => {
                    let layer_name = |id| self.session.doc.layer_index(id)
                        .map(|index| self.session.doc.layer_name(index));
                    let (title, name) = match destination {
                        MoveDestination::Layer { layer_id, linked_mask_id } => (
                            if linked_mask_id.is_some() { "Layer and linked mask" } else { "Layer pixels" },
                            layer_name(layer_id),
                        ),
                        MoveDestination::RasterMask { owner_layer_id, .. } => ("Raster mask", layer_name(owner_layer_id)),
                        MoveDestination::SelectedPixels { layer_id } => ("Selected Pixels", layer_name(layer_id)),
                        MoveDestination::SelectionOutline => ("Selection Outline", None),
                    };
                    let feedback = name.map_or_else(|| localize(title).into_owned(),
                        |name| format!("{}: {}", localize(title), name));
                    ui.add(Label::new(RichText::new(feedback).weak()).wrap());
                    if let MoveDestination::Layer { layer_id, .. } = destination {
                        if let Ok(ids) = self.session.doc.linked_layer_ids(layer_id) {
                            if ids.len() > 1 {
                                ui.add(Label::new(tr(format!("{} linked layers move together.", ids.len())).weak()).wrap());
                            }
                        }
                    }
                    if matches!(destination, MoveDestination::RasterMask { .. }) {
                        ui.add(Label::new(tr("Mask only; layer pixels stay fixed.").weak()).wrap());
                    } else if let MoveDestination::Layer { layer_id, linked_mask_id: None } = destination {
                        if self.session.doc.mask_state().items.iter().any(|mask|
                            mask.owner == (crate::engine::raster_masks::MaskOwner::Layer { id: layer_id }) && !mask.linked)
                        {
                            ui.add(Label::new(tr("Unlinked mask stays fixed.").weak()).wrap());
                        }
                    }
                    if matches!(destination, MoveDestination::RasterMask { .. } | MoveDestination::Layer { linked_mask_id: Some(_), .. }) {
                        ui.add(Label::new(tr("Exposed mask areas reveal the layer.").weak()).wrap());
                    }
                }
                Err(error) => { ui.add(Label::new(tr(error.message).weak()).wrap()); }
            }
            let drag = tr("Drag the position outline; release to move. Shift constrains the axis. Esc cancels.");
            ui.add(Label::new(drag.weak()).wrap());
            let clipping = tr("Pixels beyond the canvas are clipped. Undo restores them.");
            ui.add(Label::new(clipping.weak()).wrap());
        });
    }
}

#[cfg(test)]
#[path = "movement_mask_ui_tests.rs"]
mod owner_movement_ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    use serde_json::json;
    #[test]
    fn narrow_and_wide_options_wrap_help_inside_the_panel() {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.area_mode = 8;
        let ctx = Context::default();
        fn check(shape: &Shape, width: f32, count: &mut usize) {
            match shape {
                Shape::Text(t)
                    if t.galley.text().starts_with("Drag the position outline")
                        || t.galley.text().starts_with("Pixels beyond the canvas") =>
                {
                    assert!(
                        t.pos.x + t.galley.size().x <= width + 0.5,
                        "Help extends beyond {width}: {:?}",
                        t.galley.size()
                    );
                    *count += 1;
                }
                Shape::Vec(items) => {
                    for s in items {
                        check(s, width, count);
                    }
                }
                _ => {}
            }
        }
        for width in [280., 1000.] {
            let mut count = 0;
            for _ in 0..3 {
                let out = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 300.))),
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| app.editor.tool_options_ui(ui));
                    },
                );
                count = 0;
                for s in out.shapes {
                    check(&s.shape, width, &mut count);
                }
            }
            assert_eq!(count, 2);
        }
    }
    #[test]
    fn mouse_move_preview_cancel_release_and_arrow_shortcuts_use_shared_actions() {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        app.editor
            .core("canvas.new", json!({"width":32,"height":24}));
        let mut pixels = vec![0; 32 * 24];
        pixels[6 * 32 + 6] = 0xff0077ff;
        app.editor.session.doc.replace_region(0, 0, 32, 24, &pixels);
        app.editor.session.replay_boundary().unwrap();
        app.editor.sync_from_session();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        assert!(app.editor.core("toolbox.mode.move", json!({})));
        app.consume_ui_effects(&ctx);
        assert_eq!(app.editor.area_mode, 8);
        frame(&mut app, &ctx, size, vec![], 3.);
        let p = app.editor.image_rect.min + vec2(6.5, 6.5) * app.editor.zoom;
        let end = p + vec2(3., -2.) * app.editor.zoom;
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(
            &mut app,
            &ctx,
            size,
            vec![Event::PointerMoved(p), button(p, true)],
            4.,
        );
        assert!(app.editor.movement_gesture.is_some());
        let count = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        frame(&mut app, &ctx, size, vec![Event::PointerMoved(end)], 4.05);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            count
        );
        frame(&mut app, &ctx, size, vec![button(end, false)], 4.1);
        assert!(app.editor.movement_gesture.is_none());
        assert_eq!(
            app.editor.session.doc.layers[0].pixels[4 * 32 + 9],
            0xff0077ff
        );
        assert_eq!(
            app.editor.session.doc.layers[0]
                .pixels
                .iter()
                .filter(|p| **p != 0)
                .count(),
            1
        );
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            count + 1
        );
        app.editor.actions.push(Action::Undo);
        app.run_actions(&ctx);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        frame(
            &mut app,
            &ctx,
            size,
            vec![Event::PointerMoved(p), button(p, true)],
            5.,
        );
        frame(&mut app, &ctx, size, vec![Event::PointerMoved(end)], 5.05);
        frame(
            &mut app,
            &ctx,
            size,
            vec![Event::Key {
                key: Key::Escape,
                physical_key: Some(Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            5.1,
        );
        assert!(app.editor.movement_gesture.is_none());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        frame(&mut app, &ctx, size, vec![button(end, false)], 5.2);
        frame(
            &mut app,
            &ctx,
            size,
            vec![Event::Key {
                key: Key::ArrowRight,
                physical_key: Some(Key::ArrowRight),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            6.,
        );
        assert_eq!(
            app.editor.session.doc.layers[0].pixels[6 * 32 + 7],
            0xff0077ff
        );
        frame(
            &mut app,
            &ctx,
            size,
            vec![Event::Key {
                key: Key::ArrowDown,
                physical_key: Some(Key::ArrowDown),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::SHIFT,
            }],
            6.1,
        );
        assert_eq!(
            app.editor.session.doc.layers[0].pixels[16 * 32 + 7],
            0xff0077ff
        );
    }
    #[test]
    fn gesture_axis_constraint_and_stale_source_never_publish_pixels() {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor
            .core("canvas.new", json!({"width":12,"height":8}));
        app.editor.area_mode = 8;
        assert!(app.editor.movement_down(pos2(1., 1.)));
        assert!(app.editor.movement_move(pos2(5., 3.), true));
        assert_eq!(app.editor.movement_gesture.as_ref().unwrap().delta, [4, 0]);
        app.editor
            .core("layer.fill_color", json!({"color":"#ffffff"}));
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        app.editor.movement_release();
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.status, "Document changed; movement cancelled");
    }
    #[test]
    fn pen_cancel_and_release_share_the_same_single_movement_commit() {
        use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        app.editor
            .core("canvas.new", json!({"width":32,"height":24}));
        let mut pixels = vec![0; 32 * 24];
        pixels[6 * 32 + 6] = 0xffaabbcc;
        app.editor.session.doc.replace_region(0, 0, 32, 24, &pixels);
        app.editor.session.replay_boundary().unwrap();
        app.editor.sync_from_session();
        app.editor.area_mode = 8;
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for i in 0..3 {
            frame(&mut app, &ctx, size, vec![], i as f64);
        }
        let p = app.editor.image_rect.min + vec2(6.5, 6.5) * app.editor.zoom;
        let end = p + vec2(2., 1.) * app.editor.zoom;
        let ppp = ctx.pixels_per_point();
        let mut time = 3.;
        let mut send = |app: &mut Studio, phase: PenPhase, pos: Pos2| {
            time += 0.1;
            app.editor.packets = vec![PenSample {
                phase,
                x: pos.x * ppp,
                y: pos.y * ppp,
                pressure: 0.3,
                eraser: false,
                time: (time * 1000.) as u32,
            }];
            frame(app, &ctx, size, vec![], time);
        };
        send(&mut app, PenPhase::Down, p);
        send(&mut app, PenPhase::Move, end);
        assert!(app.editor.movement_gesture.is_some());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        send(&mut app, PenPhase::Cancel, end);
        assert!(app.editor.movement_gesture.is_none());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        let count = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        send(&mut app, PenPhase::Down, p);
        send(&mut app, PenPhase::Move, end);
        send(&mut app, PenPhase::Up, end);
        assert_eq!(
            app.editor.session.doc.layers[0].pixels[7 * 32 + 8],
            0xffaabbcc
        );
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            count + 1
        );
        assert!(!app.editor.pen_active);
        send(&mut app, PenPhase::Down, p);
        app.open_settings(None);
        frame(&mut app, &ctx, size, vec![], 5.);
        assert!(app.editor.movement_gesture.is_none());
    }
}
