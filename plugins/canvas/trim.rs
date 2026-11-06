//! Cached read-only trim preview; Apply dispatches the public atomic operation.
use super::*;
use serde_json::{json, Value};
const MODES: [(&str, &str); 3] = [
    ("transparent", "Transparent Pixels"),
    ("top_left", "Top Left Pixel Color"),
    ("bottom_right", "Bottom Right Pixel Color"),
];
pub(super) struct TrimDraft {
    pub(super) revision: u64,
    mode: usize,
    tolerance: u32,
    sides: [bool; 4],
    last_args: Option<Value>,
    preview: Option<Value>,
    geometry_plan: Option<Value>,
    error: String,
}
impl TrimDraft {
    fn args(&self) -> Value {
        json!({"mode":MODES[self.mode].0,"tolerance":self.tolerance,"top":self.sides[0],"right":self.sides[1],"bottom":self.sides[2],"left":self.sides[3]})
    }
    pub(super) fn has_preview(&self) -> bool {
        self.preview
            .as_ref()
            .is_some_and(|p| p["would_change"] == true)
    }
}
impl Editor {
    pub(in crate::desktop) fn open_trim_canvas(&mut self) {
        self.open_canvas_geometry(true);
        self.geometry.as_mut().unwrap().trim = Some(TrimDraft {
            revision: self.session.revision,
            mode: 0,
            tolerance: 0,
            sides: [true; 4],
            last_args: None,
            preview: None,
            geometry_plan: None,
            error: String::new(),
        });
    }
    pub(super) fn trim_canvas_window(&mut self, ctx: &Context, mut d: CanvasDialog) {
        let mut t = d.trim.take().unwrap();
        let stale = d.identity != self.session.doc.identity
            || d.source != [self.session.doc.width, self.session.doc.height]
            || t.revision != self.session.revision;
        let a = self.settings.ui.clone();
        let idle = self.geometry_idle();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 16.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 8.),
        );
        let scroll_height =
            (screen.height() - a.control_height * 5. - a.panel_padding * 4. - a.outer_padding * 2.)
                .max(a.control_height * 2.);
        let footer_height = a.control_height + a.panel_padding + a.item_spacing * 3. + 1.;
        let outer_width = width + frame.total_margin().sum().x;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Window::new(tr("Trim Canvas"))
            .id(Id::new("canvas-trim"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .frame(frame)
            .default_width(width)
            .min_width(width)
            .max_width(width)
            .min_height(scroll_height + footer_height)
            // Keep the central drawing visible; the dialog remains draggable.
            .default_pos(pos2(
                ctx.content_rect().right() - outer_width - a.outer_padding,
                ctx.content_rect().center().y - a.control_height * 8.,
            ))
            .show(ctx, |ui| {
                ScrollArea::vertical().id_salt("trim-canvas-controls").max_height(scroll_height).auto_shrink([false,true]).show(ui,|ui| {
                ui.weak(tr(
                    "Preview uses visible artwork. Layers and raster masks are cropped; undo restores them.",
                ));
                ui.add_space(a.panel_padding);
                ui.horizontal(|ui| {
                    ui.label(tr("Based On"));
                    ComboBox::from_id_salt("trim-mode")
                        .selected_text(tr(MODES[t.mode].1))
                        .show_ui(ui, |ui| {
                            for (i, (_, title)) in MODES.iter().enumerate() {
                                ui.selectable_value(&mut t.mode, i, tr(*title));
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label(tr("Tolerance"));
                    ui.add(DragValue::new(&mut t.tolerance).range(0..=255));
                });
                ui.weak(tr(if t.mode == 0 {
                    "Remove pixels with alpha at or below this value."
                } else {
                    "Match the corner color within this RGBA difference."
                }));
                ui.add_space(a.panel_padding);
                ui.label(tr("Trim Sides"));
                Grid::new("trim-sides")
                    .num_columns(2)
                    .spacing(vec2(a.item_spacing * 3., a.item_spacing))
                    .show(ui, |ui| {
                        for (i, title) in ["Top", "Right", "Bottom", "Left"].into_iter().enumerate()
                        {
                            ui.checkbox(&mut t.sides[i], tr(title));
                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    });
                let args = t.args();
                if !stale && t.last_args.as_ref() != Some(&args) {
                    t.last_args = Some(args.clone());
                    t.preview = None;
                    t.geometry_plan = None;
                    t.error.clear();
                    let mut action = crate::application::Action::new("canvas.trim_bounds", args);
                    action.expected_revision = Some(t.revision);
                    if !t.sides.iter().any(|v| *v) {
                        t.error = "Choose at least one side to trim".into();
                    } else {
                        match self.session.apply(action) {
                            Ok(result) => {
                                let p = result["result"].clone();
                                d.x = p["x"].as_u64().unwrap() as usize;
                                d.y = p["y"].as_u64().unwrap() as usize;
                                d.width = p["width"].as_u64().unwrap() as usize;
                                d.height = p["height"].as_u64().unwrap() as usize;
                                match self.plan_canvas_geometry(&d) {
                                    Ok(plan) => t.geometry_plan = Some(plan),
                                    Err(error) => t.error = error,
                                }
                                t.preview = Some(p);
                            }
                            Err(error) => t.error = error.to_string(),
                        }
                    }
                    ctx.request_repaint();
                }
                ui.add_space(a.panel_padding);
                if !idle {ui.colored_label(Color32::LIGHT_RED,localize(GEOMETRY_CONTACT_ACTIVE));}
                else if stale {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        tr("The canvas changed. Reopen this dialog."),
                    );
                } else if !t.error.is_empty() {
                    ui.colored_label(Color32::LIGHT_RED, localize(&t.error));
                } else if let Some(p) = &t.preview {
                    ui.label(localize(&format!("Result: {} × {} px", d.width, d.height)));
                    if p["empty"] == true {
                        ui.weak(tr("All pixels match. The canvas stays unchanged."));
                    } else if p["would_change"] == false {
                        ui.weak(tr("No removable border."));
                    }
                }
                if t.geometry_plan.as_ref().is_some_and(|plan|!geometry_sampler_ready(plan)) {
                    ui.colored_label(Color32::LIGHT_RED,localize(RAW_MASK_SAMPLER_UNAVAILABLE));
                }
                ui.weak(tr("The selection does not limit trimming."));
                });
                ui.add_space(a.panel_padding);
                ui.separator();
                ui.horizontal(|ui| {
                    let response = ui.add_enabled(!stale && idle && t.error.is_empty() && t.has_preview() && t.geometry_plan.as_ref().is_some_and(geometry_sampler_ready),Button::new(tr("Apply")));
                    #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new("canvas-trim-apply"),(response.rect,response.enabled())));
                    apply = response.clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if apply {
            let mut action = crate::application::Action::new("canvas.trim", t.args());
            action.expected_revision = Some(t.revision);
            match self.session.apply(action) {
                Ok(_) => {
                    self.sync_from_session();
                    self.fit = true;
                    self.pan = Vec2::ZERO;
                    return;
                }
                Err(error) => t.error = error.to_string(),
            }
        }
        if open && !cancel {
            d.trim = Some(t);
            self.geometry = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    fn label(shape: &Shape, text: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
            Shape::Vec(v) => v.iter().find_map(|s| label(s, text)),
            _ => None,
        }
    }
    #[test]
    fn trim_preview_cancel_apply_side_drafts_and_stale_revisions_are_safe() {
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
        let mut pixels = vec![0x00112233; 96];
        for y in 2..6 {
            for x in 3..9 {
                pixels[y * 12 + x] = 0xff336699;
            }
        }
        app.editor.session.doc.replace_region(0, 0, 12, 8, &pixels);
        app.editor.session.replay_boundary().unwrap();
        app.editor.sync_from_session();
        let ctx = Context::default();
        let size = vec2(960., 680.);
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        app.editor.actions.push(Action::UiCommand(10174));
        app.run_actions(&ctx);
        for t in 3..6 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        let d = app.editor.geometry.as_ref().unwrap();
        assert_eq!((d.x, d.y, d.width, d.height), (3, 2, 6, 4));
        assert!(d.trim.as_ref().unwrap().has_preview());
        let window = ctx.memory(|m| m.area_rect(Id::new("canvas-trim"))).unwrap();
        assert!(
            window.right() <= size.x - app.editor.settings.ui.outer_padding + 1.,
            "{window:?}"
        );
        assert!(!window.contains(app.editor.image_rect.center()));
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
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
            6.,
        );
        assert!(app.editor.geometry.is_none());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        app.editor.open_trim_canvas();
        app.editor
            .geometry
            .as_mut()
            .unwrap()
            .trim
            .as_mut()
            .unwrap()
            .sides[3] = false;
        frame(&mut app, &ctx, size, vec![], 7.);
        let d = app.editor.geometry.as_ref().unwrap();
        assert_eq!((d.x, d.width), (0, 9));
        app.editor
            .geometry
            .as_mut()
            .unwrap()
            .trim
            .as_mut()
            .unwrap()
            .sides[3] = true;
        frame(&mut app, &ctx, size, vec![], 8.);
        let out = frame(&mut app, &ctx, size, vec![], 9.);
        let point = out
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "Apply"))
            .unwrap();
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            10.,
        );
        assert!(app.editor.geometry.is_none());
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (6, 4)
        );
        assert_eq!(
            app.editor.session.doc.layers[0].pixels,
            vec![0xff336699; 24]
        );
        frame(&mut app, &ctx, size, vec![], 11.);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [6, 4]);
        app.editor.actions.push(Action::Undo);
        app.run_actions(&ctx);
        frame(&mut app, &ctx, size, vec![], 12.);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        app.editor.open_trim_canvas();
        frame(&mut app, &ctx, size, vec![], 13.);
        app.editor
            .core("layer.fill_color", json!({"color":"#ffffff"}));
        let out = frame(&mut app, &ctx, size, vec![], 14.);
        assert!(out.shapes.iter().any(|s| label(
            &s.shape,
            "The canvas changed. Reopen this dialog."
        )
        .is_some()));
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
    }
    #[test]
    fn empty_or_unselected_sides_never_publish_a_crop() {
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
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        app.editor.open_trim_canvas();
        frame(&mut app, &ctx, size, vec![], 3.);
        let out = frame(&mut app, &ctx, size, vec![], 3.1);
        assert!(out.shapes.iter().any(|s| label(
            &s.shape,
            "All pixels match. The canvas stays unchanged."
        )
        .is_some()));
        assert!(!app
            .editor
            .geometry
            .as_ref()
            .unwrap()
            .trim
            .as_ref()
            .unwrap()
            .has_preview());
        app.editor
            .geometry
            .as_mut()
            .unwrap()
            .trim
            .as_mut()
            .unwrap()
            .sides = [false; 4];
        let out = frame(&mut app, &ctx, size, vec![], 4.);
        assert!(out
            .shapes
            .iter()
            .any(|s| label(&s.shape, "Choose at least one side to trim").is_some()));
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
    }
}
