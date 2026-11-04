//! Canvas framing controls; all commits go through public document actions.
use super::*;
pub(super) struct CanvasDialog {
    identity: u64,
    source: [usize; 2],
    crop: bool,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    anchor: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    fn label(shape: &egui::epaint::Shape, text: &str) -> Option<Pos2> {
        match shape {
            egui::epaint::Shape::Text(t) if t.galley.text() == text => {
                Some(t.pos + t.galley.size() * 0.5)
            }
            egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| label(s, text)),
            _ => None,
        }
    }
    #[test]
    fn framing_dialog_previews_without_mutation_and_apply_resizes_texture() {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor
            .core("canvas.new", serde_json::json!({"width":12,"height":8}));
        assert!(app
            .editor
            .core("layer.fill_color", serde_json::json!({"color":"#33669980"})));
        app.editor.sync_from_session();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        frame(&mut app, &ctx, size, vec![], 0.);
        app.editor.actions.push(Action::UiCommand(10144));
        app.run_actions(&ctx);
        let d = app.editor.geometry.as_mut().unwrap();
        d.width = 17;
        d.height = 11;
        d.anchor = 8;
        for t in 1..4 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        assert!(app.editor.blocked);
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
        let output = frame(&mut app, &ctx, size, vec![], 4.);
        let p = output
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "Apply"))
            .unwrap();
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            5.,
        );
        assert!(app.editor.geometry.is_none());
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (17, 11)
        );
        frame(&mut app, &ctx, size, vec![], 6.);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [17, 11]);
        assert_eq!(
            app.editor.session.doc.layers[0].pixels[3 * 17 + 5],
            0x80336699
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels[0], 0);
    }
    #[test]
    fn crop_dialog_cancel_and_stale_source_never_modify_the_document() {
        let mut app = studio();
        app.editor
            .core("canvas.new", serde_json::json!({"width":12,"height":8}));
        let ctx = Context::default();
        let size = vec2(960., 680.);
        app.editor.open_canvas_geometry(true);
        let d = app.editor.geometry.as_mut().unwrap();
        d.x = 2;
        d.y = 1;
        d.width = 7;
        d.height = 5;
        let state = app.editor.session.state();
        frame(&mut app, &ctx, size, vec![], 0.);
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
            1.,
        );
        assert!(app.editor.geometry.is_none());
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, vec![0; 96]);
        assert_eq!(app.editor.session.state()["layers"], state["layers"]);
        app.editor.open_canvas_geometry(true);
        app.editor
            .core("canvas.new", serde_json::json!({"width":12,"height":8}));
        let output = frame(&mut app, &ctx, size, vec![], 2.);
        assert!(output.shapes.iter().any(|s| label(
            &s.shape,
            "The canvas changed. Reopen this dialog."
        )
        .is_some()));
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
    }
}
const ANCHORS: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];
const LABELS: [&str; 9] = [
    "Top Left",
    "Top",
    "Top Right",
    "Left",
    "Center",
    "Right",
    "Bottom Left",
    "Bottom",
    "Bottom Right",
];
impl Editor {
    pub(super) fn open_canvas_geometry(&mut self, crop: bool) {
        self.finish_stroke();
        self.area_drag = None;
        self.area_target = None;
        self.geometry = Some(CanvasDialog {
            identity: self.session.doc.identity,
            source: [self.session.doc.width, self.session.doc.height],
            crop,
            x: 0,
            y: 0,
            width: self.session.doc.width,
            height: self.session.doc.height,
            anchor: 4,
        });
    }
    pub(super) fn crop_outline(&self, painter: &Painter, color: Color32) {
        if let Some(d) = &self.geometry {
            if d.crop
                && d.identity == self.session.doc.identity
                && d.source == [self.session.doc.width, self.session.doc.height]
            {
                let r = Rect::from_min_size(
                    self.image_rect.min + vec2(d.x as f32, d.y as f32) * self.zoom,
                    vec2(d.width as f32, d.height as f32) * self.zoom,
                );
                painter.rect_stroke(r, 0, Stroke::new(2., color), StrokeKind::Inside);
                for t in [1. / 3., 2. / 3.] {
                    painter.line_segment(
                        [
                            pos2(r.left() + r.width() * t, r.top()),
                            pos2(r.left() + r.width() * t, r.bottom()),
                        ],
                        Stroke::new(1., color.gamma_multiply(0.5)),
                    );
                    painter.line_segment(
                        [
                            pos2(r.left(), r.top() + r.height() * t),
                            pos2(r.right(), r.top() + r.height() * t),
                        ],
                        Stroke::new(1., color.gamma_multiply(0.5)),
                    );
                }
            }
        }
    }
    pub(super) fn canvas_geometry_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.geometry.take() else {
            return;
        };
        let stale = d.identity != self.session.doc.identity
            || d.source != [self.session.doc.width, self.session.doc.height];
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Window::new(tr(if d.crop { "Crop Canvas" } else { "Canvas Size" }))
            .id(Id::new("canvas-geometry"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.)
            .min_width(360.)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.weak(tr(if d.crop {
                    "Crop all layers to a pixel rectangle."
                } else {
                    "Reframe all layers without scaling. New space is transparent."
                }));
                ui.add_space(8.);
                Grid::new("canvas-geometry-fields")
                    .num_columns(2)
                    .show(ui, |ui| {
                        if d.crop {
                            ui.label("X");
                            ui.add(
                                DragValue::new(&mut d.x)
                                    .range(0..=d.source[0] - 1)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                            ui.label("Y");
                            ui.add(
                                DragValue::new(&mut d.y)
                                    .range(0..=d.source[1] - 1)
                                    .suffix(" px"),
                            );
                            ui.end_row();
                            d.width = d.width.min(d.source[0] - d.x);
                            d.height = d.height.min(d.source[1] - d.y);
                        }
                        ui.label(tr("Width"));
                        ui.add(
                            DragValue::new(&mut d.width)
                                .range(1..=if d.crop { d.source[0] - d.x } else { 4096 })
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label(tr("Height"));
                        ui.add(
                            DragValue::new(&mut d.height)
                                .range(1..=if d.crop { d.source[1] - d.y } else { 4096 })
                                .suffix(" px"),
                        );
                        ui.end_row();
                    });
                if !d.crop {
                    ui.add_space(8.);
                    ui.label(tr("Anchor"));
                    Grid::new("canvas-anchor")
                        .spacing(vec2(4., 4.))
                        .show(ui, |ui| {
                            for (i, label) in LABELS.iter().enumerate() {
                                if ui.selectable_label(d.anchor == i, tr(*label)).clicked() {
                                    d.anchor = i;
                                }
                                if i % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                }
                ui.add_space(8.);
                ui.weak(tr(
                    "Pixels outside the new canvas are removed. Undo restores them.",
                ));
                let valid = !stale
                    && d.width * d.height * self.session.doc.layers.len() <= 64 * 1024 * 1024;
                if stale {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        tr("The canvas changed. Reopen this dialog."),
                    );
                } else if !valid {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        tr("Canvas size exceeds document limits"),
                    );
                }
                ui.horizontal(|ui| {
                    apply = ui.add_enabled(valid, Button::new(tr("Apply"))).clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if apply {
            let (action, args) = if d.crop {
                (
                    "canvas.crop",
                    serde_json::json!({"x":d.x,"y":d.y,"width":d.width,"height":d.height}),
                )
            } else {
                (
                    "canvas.resize",
                    serde_json::json!({"width":d.width,"height":d.height,"anchor":ANCHORS[d.anchor]}),
                )
            };
            if self.core(action, args) {
                self.sync_from_session();
                self.fit = true;
                self.pan = Vec2::ZERO;
                return;
            }
        }
        if open && !cancel {
            self.geometry = Some(d);
        }
    }
}
