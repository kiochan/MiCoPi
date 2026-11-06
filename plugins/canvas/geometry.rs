//! Canvas framing controls; all commits go through public document actions.
use super::*;
use serde_json::{json, Value};
#[path = "image_size.rs"]
mod image_size;
#[path = "trim.rs"]
mod trim;
pub(super) struct CanvasDialog {
    identity: u64,
    source: [usize; 2],
    revision: u64,
    error: String,
    crop: bool,
    trim: Option<trim::TrimDraft>,
    image_size: Option<image_size::Draft>,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    anchor: usize,
}

impl CanvasDialog {
    pub(super) fn accept_local_view(&mut self, identity: u64, before: u64, after: u64) {
        if self.identity == identity {
            if self.revision == before {
                self.revision = after;
            }
            if let Some(draft) = self.trim.as_mut() {
                if draft.revision == before {
                    draft.revision = after;
                }
            }
            if let Some(draft) = self.image_size.as_mut() {
                draft.accept_local_view(before, after);
            }
        }
    }
    fn arguments(&self) -> Value {
        if self.crop {
            json!({"x":self.x,"y":self.y,"width":self.width,"height":self.height})
        } else {
            json!({"width":self.width,"height":self.height,"anchor":ANCHORS[self.anchor]})
        }
    }
}

const RAW_MASK_SAMPLER_UNAVAILABLE: &str = "Painting DLL does not support raw raster-mask geometry";
const GEOMETRY_CONTACT_ACTIVE: &str = "Finish the current contact before changing canvas geometry";
fn geometry_sampler_ready(plan: &Value) -> bool {
    plan["requires_a8_sampler"] != true || plan["a8_sampler_available"] == true
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
        self.area_drag = None;
        self.area_target = None;
        self.geometry = Some(CanvasDialog {
            identity: self.session.doc.identity,
            source: [self.session.doc.width, self.session.doc.height],
            revision: self.session.revision,
            error: String::new(),
            crop,
            trim: None,
            image_size: None,
            x: 0,
            y: 0,
            width: self.session.doc.width,
            height: self.session.doc.height,
            anchor: 4,
        });
    }
    fn geometry_idle(&self) -> bool {
        !self.session.stroke.active() && self.session.pointer_checkpoint.is_none()
    }
    fn plan_canvas_geometry(&mut self, dialog: &CanvasDialog) -> Result<Value, String> {
        let mut args = dialog.arguments();
        args["operation"] = json!(if dialog.crop { "crop" } else { "resize" });
        self.session
            .apply(crate::application::Action::new(
                "canvas.geometry.plan",
                args,
            ))
            .map(|mut response| response["result"].take())
            .map_err(|error| error.message)
    }
    fn canvas_geometry_current(&self, dialog: &CanvasDialog) -> bool {
        dialog.identity == self.session.doc.identity
            && dialog.source == [self.session.doc.width, self.session.doc.height]
            && dialog.revision == self.session.revision
    }
    fn apply_canvas_geometry(&mut self, dialog: &mut CanvasDialog) -> bool {
        if !self.geometry_idle() {
            dialog.error = GEOMETRY_CONTACT_ACTIVE.into();
            return false;
        }
        if !self.canvas_geometry_current(dialog) {
            dialog.error = "The canvas changed. Reopen this dialog.".into();
            return false;
        }
        match self.plan_canvas_geometry(dialog) {
            Err(error) => {
                dialog.error = error;
                return false;
            }
            Ok(plan) if !geometry_sampler_ready(&plan) => {
                dialog.error = RAW_MASK_SAMPLER_UNAVAILABLE.into();
                return false;
            }
            _ => {}
        }
        if self.core_at_revision(
            if dialog.crop {
                "canvas.crop"
            } else {
                "canvas.resize"
            },
            dialog.arguments(),
            Some(dialog.revision),
        ) {
            self.sync_from_session();
            self.fit = true;
            self.pan = Vec2::ZERO;
            true
        } else {
            dialog.error = self.status.clone();
            false
        }
    }
    pub(super) fn crop_outline(&self, painter: &Painter, color: Color32) {
        if let Some(d) = &self.geometry {
            if d.crop
                && d.trim.as_ref().is_none_or(|t| {
                    t.revision == self.session.revision
                        && !self.session.is_drawing()
                        && t.has_preview()
                })
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
        if d.trim.is_some() {
            self.trim_canvas_window(ctx, d);
            return;
        }
        if d.image_size.is_some() {
            self.image_size_window(ctx, d);
            return;
        }
        let stale = !self.canvas_geometry_current(&d);
        let idle = self.geometry_idle();
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        let a = self.settings.ui.clone();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 14.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 8.),
        );
        let scroll_height =
            (screen.height() - a.control_height * 5. - a.panel_padding * 4. - a.outer_padding * 2.)
                .max(a.control_height * 2.);
        let footer_height = a.control_height + a.panel_padding + a.item_spacing * 3. + 1.;
        let before_args = d.arguments();
        Window::new(tr(if d.crop { "Crop Canvas" } else { "Canvas Size" }))
            .id(Id::new("canvas-geometry"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .frame(frame)
            .default_width(width)
            .min_width(width)
            .max_width(width)
            .min_height(scroll_height + footer_height)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let mut valid = false;
                ScrollArea::vertical()
                    .id_salt("canvas-geometry-controls")
                    .max_height(scroll_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.weak(tr(if d.crop {
                            "Crop layers, raster masks and selections to a pixel rectangle."
                        } else {
                            "Reframe layers, raster masks and selections without scaling."
                        }));
                        if !d.crop {
                            ui.weak(tr(
                                "New layer space is transparent; new mask space reveals pixels.",
                            ));
                        }
                        ui.add_space(a.panel_padding);
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
                            ui.add_space(a.panel_padding);
                            ui.label(tr("Anchor"));
                            Grid::new("canvas-anchor")
                                .spacing(vec2(a.item_spacing, a.item_spacing))
                                .show(ui, |ui| {
                                    for (i, label) in LABELS.iter().enumerate() {
                                        if ui.selectable_label(d.anchor == i, tr(*label)).clicked()
                                        {
                                            d.anchor = i;
                                        }
                                        if i % 3 == 2 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        }
                        ui.add_space(a.panel_padding);
                        ui.weak(tr(
                            "Pixels outside the new canvas are removed. Undo restores them.",
                        ));
                        if d.arguments() != before_args {
                            d.error.clear();
                        }
                        let plan = self.plan_canvas_geometry(&d);
                        valid = !stale && idle && plan.as_ref().is_ok_and(geometry_sampler_ready);
                        match &plan {
                            Ok(plan) => {
                                if plan["raster_mask_count"].as_u64().unwrap_or(0) > 0 {
                                    for (field, title) in [
                                        ("estimated_raster_mask_bytes", "Raster Mask Buffers"),
                                        (
                                            "estimated_effective_mask_bytes",
                                            "Effective Mask Cache (estimate)",
                                        ),
                                    ] {
                                        if let Some(bytes) = plan[field].as_u64() {
                                            ui.weak(format!(
                                                "{}: {:.1} MiB",
                                                localize(title),
                                                bytes as f64 / 1048576.
                                            ));
                                        }
                                    }
                                }
                                if !geometry_sampler_ready(plan) {
                                    ui.colored_label(
                                        Color32::LIGHT_RED,
                                        localize(RAW_MASK_SAMPLER_UNAVAILABLE),
                                    );
                                }
                            }
                            Err(error) => {
                                ui.colored_label(Color32::LIGHT_RED, localize(error));
                            }
                        }
                        if !idle {
                            ui.colored_label(Color32::LIGHT_RED, localize(GEOMETRY_CONTACT_ACTIVE));
                        } else if stale {
                            ui.colored_label(
                                Color32::LIGHT_RED,
                                tr("The canvas changed. Reopen this dialog."),
                            );
                        } else if !d.error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, localize(&d.error));
                        }
                    });
                ui.add_space(a.panel_padding);
                ui.separator();
                ui.horizontal(|ui| {
                    let response = ui.add_enabled(valid, Button::new(tr("Apply")));
                    #[cfg(test)]
                    ui.data_mut(|data| {
                        data.insert_temp(
                            Id::new("canvas-geometry-apply"),
                            (response.rect, response.enabled()),
                        )
                    });
                    apply = response.clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if apply && self.apply_canvas_geometry(&mut d) {
            return;
        }
        if open && !cancel {
            self.geometry = Some(d);
        }
    }
}

#[cfg(test)]
#[path = "geometry_mask_ui_tests.rs"]
pub(super) mod mask_ui_tests;
