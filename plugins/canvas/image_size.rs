//! Provisional Image Size controls backed by the shared physical-unit plan.
use super::*;
use crate::application::{output_size::Method, units::Unit, Action as CoreAction};
use crate::engine::document_info::Info;
use serde_json::{json, Value};

pub(super) struct Draft {
    revision: u64,
    info: Info,
    unit: Unit,
    width: f64,
    height: f64,
    locked: bool,
    width_axis: bool,
    method: Method,
    error: String,
}
impl Draft {
    pub(super) fn accept_local_view(&mut self, before: u64, after: u64) {
        if self.revision == before {
            self.revision = after;
        }
    }
    fn args(&self) -> Value {
        let number = |value: f64| {
            if self.unit == Unit::Pixels
                && value.is_finite()
                && value.fract() == 0.
                && (1.0..=4096.0).contains(&value)
            {
                json!(value as usize)
            } else {
                json!(value)
            }
        };
        let mut args = json!({"unit":self.unit,"lock_aspect":self.locked,"method":self.method});
        if !self.locked || self.width_axis {
            args["width"] = number(self.width);
        }
        if !self.locked || !self.width_axis {
            args["height"] = number(self.height);
        }
        args
    }
    fn axes(&self) -> Option<[f64; 2]> {
        crate::application::units::pixels_per_inch(self.info)
    }
    fn values(&self, unit: Unit, pixels: [usize; 2]) -> Result<[f64; 2], &'static str> {
        if unit == Unit::Pixels {
            return Ok(pixels.map(|v| v as f64));
        }
        let axes = self
            .axes()
            .ok_or("Physical units require known density. Set Document Resolution first.")?;
        Ok(std::array::from_fn(|axis| {
            unit.from_pixels(pixels[axis], axes[axis])
        }))
    }
    fn switch_unit(&mut self, unit: Unit, pixels: [usize; 2]) -> Result<(), &'static str> {
        let [width, height] = self.values(unit, pixels)?;
        self.unit = unit;
        self.width = width;
        self.height = height;
        Ok(())
    }
    fn sync_derived(&mut self, plan: &Value) {
        if self.locked {
            if let Some(pixels) = dimensions(plan) {
                if let Ok([width, height]) = self.values(self.unit, pixels) {
                    if self.width_axis {
                        self.height = height;
                    } else {
                        self.width = width;
                    }
                }
            }
        }
    }
}
fn dimensions(plan: &Value) -> Option<[usize; 2]> {
    Some([
        plan["width"].as_u64()? as usize,
        plan["height"].as_u64()? as usize,
    ])
}
fn unit_title(unit: Unit) -> &'static str {
    match unit {
        Unit::Pixels => "Pixels",
        Unit::Millimeters => "Millimeters",
        Unit::Inches => "Inches",
    }
}
fn input(value: &mut f64, unit: Unit) -> DragValue<'_> {
    DragValue::new(value)
        .range(if unit == Unit::Pixels {
            1.0..=4096.0
        } else {
            0.000000000001..=1000000000.0
        })
        .speed(if unit == Unit::Pixels { 1. } else { 0.1 })
        .max_decimals(if unit == Unit::Pixels { 0 } else { 12 })
        .suffix(match unit {
            Unit::Pixels => " px",
            Unit::Millimeters => " mm",
            Unit::Inches => " in",
        })
}
impl Editor {
    pub(in crate::desktop) fn open_image_size(&mut self) {
        self.open_canvas_geometry(false);
        self.geometry.as_mut().unwrap().image_size = Some(Draft {
            revision: self.session.revision,
            info: self.session.doc.info(),
            unit: Unit::Pixels,
            width: self.session.doc.width as f64,
            height: self.session.doc.height as f64,
            locked: true,
            width_axis: true,
            method: Method::Bilinear,
            error: String::new(),
        });
    }
    fn plan_image_size(&mut self, d: &Draft) -> Result<Value, String> {
        self.session
            .apply(CoreAction::new("canvas.image_size.plan", d.args()))
            .map(|mut response| response["result"].take())
            .map_err(|error| error.message)
    }
    fn image_size_current(&self, dialog: &CanvasDialog, d: &Draft) -> bool {
        dialog.identity == self.session.doc.identity
            && dialog.source == [self.session.doc.width, self.session.doc.height]
            && d.revision == self.session.revision
            && d.info == self.session.doc.info()
            && !self.session.stroke.active()
            && self.session.pointer_checkpoint.is_none()
    }
    fn apply_image_size(&mut self, dialog: &CanvasDialog, d: &mut Draft) -> bool {
        if !self.geometry_idle() {
            d.error = GEOMETRY_CONTACT_ACTIVE.into();
            return false;
        }
        if !self.image_size_current(dialog, d) {
            d.error = "The canvas changed. Reopen this dialog.".into();
            return false;
        }
        match self.plan_image_size(d) {
            Err(error) => {
                d.error = error;
                return false;
            }
            Ok(plan) if !geometry_sampler_ready(&plan) => {
                d.error = RAW_MASK_SAMPLER_UNAVAILABLE.into();
                return false;
            }
            _ => {}
        }
        if self.core_at_revision("canvas.image_size", d.args(), Some(d.revision)) {
            self.sync_from_session();
            self.fit = true;
            self.pan = Vec2::ZERO;
            true
        } else {
            d.error = self.status.clone();
            false
        }
    }
    pub(super) fn image_size_window(&mut self, ctx: &Context, mut dialog: CanvasDialog) {
        let mut d = dialog.image_size.take().unwrap();
        let stale = !self.image_size_current(&dialog, &d);
        let mut plan = self.plan_image_size(&d);
        let before_args = d.args();
        let a = self.settings.ui.clone();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 18.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 8.),
        );
        let scroll_height =
            (screen.height() - a.control_height * 5. - a.panel_padding * 4. - a.outer_padding * 2.)
                .max(a.control_height * 2.);
        let footer_height = a.control_height + a.panel_padding + a.item_spacing * 3. + 1.;
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Window::new(tr("Image Size"))
            .id(Id::new("image-size"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .frame(frame)
            .min_width(width)
            .max_width(width)
            .default_width(width)
            .min_height(scroll_height + footer_height)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ScrollArea::vertical().id_salt("image-size-controls")
                    .max_height(scroll_height).auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.weak(tr("Scale every layer, raster mask and selection. Apply commits one edit."));
                        ui.label(format!("{}: {} × {} px", localize("Source Size"), dialog.source[0], dialog.source[1]));
                        ui.add_space(a.section_spacing);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr("Units"));
                            let mut unit = d.unit;
                            ComboBox::from_id_salt("image-size-units")
                                .selected_text(tr(unit_title(unit))).show_ui(ui, |ui| {
                                    for choice in [Unit::Pixels, Unit::Millimeters, Unit::Inches] {
                                        ui.add_enabled_ui(choice == Unit::Pixels || d.axes().is_some(), |ui| {
                                            ui.selectable_value(&mut unit, choice, tr(unit_title(choice)));
                                        });
                                    }
                                });
                            if unit != d.unit {
                                if let Some(pixels) = plan.as_ref().ok().and_then(dimensions) {
                                    if let Err(error) = d.switch_unit(unit, pixels) {
                                        d.error = error.into();
                                    }
                                } else {
                                    d.error = "Enter valid dimensions before switching units.".into();
                                }
                            }
                        });
                        ui.add_space(a.item_spacing);
                        Grid::new("image-size-fields").num_columns(2)
                            .spacing(vec2(a.section_spacing, a.item_spacing)).show(ui, |ui| {
                                ui.label(tr("Width"));
                                if ui.add(input(&mut d.width, d.unit)).changed() {
                                    d.width_axis = true;
                                    if let Ok(value) = self.plan_image_size(&d) {d.sync_derived(&value);}
                                }
                                ui.end_row();
                                ui.label(tr("Height"));
                                if ui.add(input(&mut d.height, d.unit)).changed() {
                                    d.width_axis = false;
                                    if let Ok(value) = self.plan_image_size(&d) {d.sync_derived(&value);}
                                }
                                ui.end_row();
                            });
                        if ui.checkbox(&mut d.locked, tr("Lock Aspect Ratio")).changed() && d.locked {
                            d.width_axis = true;
                            if let Ok(value) = self.plan_image_size(&d) {d.sync_derived(&value);}
                        }
                        ui.add_space(a.section_spacing);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr("Resampling"));
                            ComboBox::from_id_salt("image-size-resampling")
                                .selected_text(tr(match d.method {
                                    Method::Bilinear => "Bilinear", Method::Nearest => "Nearest Neighbor", Method::Area => "Area (Downscale)",
                                })).show_ui(ui, |ui| {
                                    for (method, title) in [(Method::Bilinear,"Bilinear"),(Method::Nearest,"Nearest Neighbor"),(Method::Area,"Area (Downscale)")] {
                                        ui.selectable_value(&mut d.method, method, tr(title));
                                    }
                                });
                        });
                        ui.weak(tr("Nearest keeps pixel edges. Bilinear smooths. Area preserves detail when shrinking."));
                        ui.add_space(a.section_spacing);
                        plan = self.plan_image_size(&d);
                        if plan.is_ok() && d.args() != before_args {d.error.clear();}
                        match &plan {
                            Ok(value) => {
                                if let Some([w,h]) = dimensions(value) {
                                    ui.strong(format!("{}: {} × {} px", localize("Pixel Dimensions"), w, h));
                                }
                                let info = &value["document_info"];
                                if let (Some(w),Some(h)) = (info["width_mm"].as_f64(),info["height_mm"].as_f64()) {
                                    ui.label(format!("{}: {:.2} × {:.2} mm", localize("Physical Size"), w,h));
                                }
                                for (field,title) in [
                                    ("estimated_layer_bytes","Layer Buffers"),
                                    ("estimated_mask_bytes","Selection Buffers"),
                                    ("estimated_raster_mask_bytes","Raster Mask Buffers"),
                                    ("estimated_effective_mask_bytes","Effective Mask Cache (estimate)"),
                                    ("estimated_composite_bytes","Composite Buffer"),
                                    ("estimated_staged_bytes","Staged Edit Buffers"),
                                ] {
                                    if matches!(field,"estimated_raster_mask_bytes"|"estimated_effective_mask_bytes") && value["raster_mask_count"].as_u64().unwrap_or(0)==0 {continue;}
                                    if let Some(bytes) = value[field].as_u64() {
                                        ui.weak(format!("{}: {:.1} MiB", localize(title), bytes as f64 / (1024. * 1024.)));
                                    }
                                }
                                if value["raster_mask_count"].as_u64().unwrap_or(0)>0 {ui.weak(tr("Mask feather stays fixed in canvas pixels."));}
                                if !geometry_sampler_ready(value) {ui.colored_label(Color32::LIGHT_RED,localize(RAW_MASK_SAMPLER_UNAVAILABLE));}
                                if value["identity"] == true {ui.weak(tr("Dimensions are unchanged; Apply leaves the document unchanged."));}
                            }
                            Err(error) => {ui.colored_label(Color32::LIGHT_RED, localize(error));}
                        }
                        ui.weak(tr("Output sizes only. History, source pixels and plugin scratch buffers use additional memory."));
                        ui.add_space(a.item_spacing);
                        if let Some([x,y]) = d.axes() {
                            ui.label(format!("{}: {:.2} × {:.2} ppi", localize("Document Resolution"), x, y));
                            ui.weak(tr("Resolution is inherited. Use Document Resolution to change print density."));
                            if (x-y).abs() > f64::EPSILON {
                                ui.weak(tr("Aspect ratio follows pixel dimensions, including images with different horizontal and vertical densities."));
                            }
                        } else {
                            ui.weak(tr("Physical units require known density. Set Document Resolution first."));
                        }
                        if d.unit != Unit::Pixels {ui.weak(tr("Physical dimensions round to whole pixels."));}
                        if !self.geometry_idle() {
                            ui.colored_label(Color32::LIGHT_RED,localize(GEOMETRY_CONTACT_ACTIVE));
                        } else if stale {
                            ui.colored_label(Color32::LIGHT_RED, tr("The canvas changed. Reopen this dialog."));
                        } else if !d.error.is_empty() {
                            ui.colored_label(Color32::LIGHT_RED, localize(&d.error));
                        }
                    });
                ui.add_space(a.panel_padding);
                ui.separator();
                ui.horizontal(|ui| {
                    let response = ui.add_enabled(!stale && plan.as_ref().is_ok_and(geometry_sampler_ready), Button::new(tr("Apply")));
                    #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new("image-size-apply"),(response.rect,response.enabled())));
                    apply = response.clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if apply && self.apply_image_size(&dialog, &mut d) {
            return;
        }
        if open && !cancel {
            dialog.image_size = Some(d);
            self.geometry = Some(dialog);
        }
    }
}

#[cfg(test)]
#[path = "image_size_mask_ui_tests.rs"]
mod mask_ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    use crate::engine::document_info::{Density, DensityUnit};
    fn label(shape: &Shape, text: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
            Shape::Vec(v) => v.iter().find_map(|s| label(s, text)),
            _ => None,
        }
    }
    fn native_studio() -> Studio {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app
    }
    fn density(xppu: u32, yppu: u32, unit: DensityUnit) -> Info {
        Info::with_png_density(Density { xppu, yppu, unit })
    }
    #[test]
    fn image_size_physical_units_use_exact_separate_axes_and_switch_without_pixel_drift() {
        let mut app = studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":400,"height":200})));
        let info = density(10000, 20000, DensityUnit::Meter);
        app.editor.session.doc.set_import_info(info).unwrap();
        app.editor.open_image_size();
        let mut dialog = app.editor.geometry.take().unwrap();
        let mut d = dialog.image_size.take().unwrap();
        let state = app.editor.session.state();
        let pointer = app.editor.session.doc.layers[0].pixels.as_ptr();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        d.switch_unit(Unit::Millimeters, [400, 200]).unwrap();
        assert_eq!((d.width, d.height), (40., 10.));
        d.width = 20.;
        let plan = app.editor.plan_image_size(&d).unwrap();
        assert_eq!(dimensions(&plan), Some([200, 100]));
        d.sync_derived(&plan);
        assert_eq!(d.height, 5.);
        for _ in 0..8 {
            for unit in [Unit::Inches, Unit::Pixels, Unit::Millimeters] {
                let pixels = dimensions(&app.editor.plan_image_size(&d).unwrap()).unwrap();
                d.switch_unit(unit, pixels).unwrap();
                assert_eq!(
                    dimensions(&app.editor.plan_image_size(&d).unwrap()),
                    Some([200, 100])
                );
            }
        }
        d.width_axis = false;
        d.height = 10.;
        let plan = app.editor.plan_image_size(&d).unwrap();
        assert_eq!(dimensions(&plan), Some([400, 200]));
        d.sync_derived(&plan);
        assert_eq!(d.width, 40.);
        d.locked = false;
        d.width = 10.;
        d.height = 10.;
        let plan = app.editor.plan_image_size(&d).unwrap();
        assert_eq!(dimensions(&plan), Some([100, 200]));
        assert_eq!(plan["estimated_layer_bytes"], 100 * 200 * 4);
        assert_eq!(plan["estimated_staged_bytes"], 100 * 200 * 4);
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.layers[0].pixels.as_ptr(), pointer);
        assert_eq!(app.editor.session.doc.info(), info);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        for unknown in [
            density(10000, 20000, DensityUnit::Unspecified),
            density(0, 20000, DensityUnit::Meter),
        ] {
            d.info = unknown;
            d.unit = Unit::Pixels;
            assert!(d.switch_unit(Unit::Millimeters, [100, 200]).is_err());
            assert_eq!(d.unit, Unit::Pixels);
        }
        app.editor.register_commands();
        for query in [
            "Plan Image Size",
            "规划图像大小",
            "画像サイズを計画",
            "이미지 크기 계획",
            "canvas.image_size.plan",
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette =
                crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(query.into(), keys);
            assert!(
                palette.results.iter().any(|&i| {
                    let command = keys.command(i);
                    command.keywords == "canvas.image_size.plan" && command.owner == Some("core")
                }),
                "{query}"
            );
        }
    }
    #[test]
    fn image_size_physical_apply_is_one_shared_edit_and_undo_restores_source_and_density() {
        let mut app = native_studio();
        assert!(app.editor.core(
            "canvas.new",
            json!({"width":12,"height":8,"resolution_ppi":100})
        ));
        assert!(app.editor.core("native.checkerboard", json!({"cell":2})));
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        frame(&mut app, &ctx, size, vec![], 0.);
        app.editor.actions.push(Action::UiCommand(10173));
        app.run_actions(&ctx);
        let d = app
            .editor
            .geometry
            .as_mut()
            .unwrap()
            .image_size
            .as_mut()
            .unwrap();
        d.switch_unit(Unit::Millimeters, [6, 4]).unwrap();
        d.method = Method::Area;
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let info = app.editor.session.doc.info();
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], 1. + n as f64);
        }
        assert!(app.editor.blocked);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        // Resizing the host changes only its fitted view. The matching-before
        // view acknowledgement must retain this document edit's validity.
        for (n, resized) in [vec2(460., 380.), vec2(1700., 1000.), size]
            .into_iter()
            .enumerate()
        {
            frame(&mut app, &ctx, resized, vec![], 4. + n as f64 * 0.1);
        }
        let output = frame(&mut app, &ctx, size, vec![], 5.);
        let p = output
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "Apply"))
            .unwrap();
        // Flush UI/window/renderer metadata accumulated by the resize frames,
        // independently of the document edit whose event boundary is checked.
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let expected = app
            .editor
            .geometry
            .as_ref()
            .unwrap()
            .image_size
            .as_ref()
            .unwrap()
            .args();
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
            6.,
        );
        assert!(app.editor.geometry.is_none(), "{}", app.editor.status);
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (6, 4)
        );
        assert_eq!(app.editor.session.doc.info(), info);
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        assert_eq!(
            journal.events(events, 1).unwrap()["events"][0]["source"]["args"],
            expected
        );
        assert_eq!(
            app.editor
                .session
                .apply(CoreAction::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
        frame(&mut app, &ctx, size, vec![], 7.);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [6, 4]);
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.info(), info);
        // Outside frame rendering, the same UI commit helper publishes exactly
        // one document event. A fitted-view update is a separate operation.
        app.editor.open_image_size();
        let mut dialog = app.editor.geometry.take().unwrap();
        let mut d = dialog.image_size.take().unwrap();
        d.switch_unit(Unit::Millimeters, [6, 4]).unwrap();
        let expected = d.args();
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        assert!(app.editor.apply_image_size(&dialog, &mut d), "{}", d.error);
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        assert_eq!(
            journal.events(events, 1).unwrap()["events"][0]["source"]["args"],
            expected
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.info(), info);
    }
    #[test]
    fn image_size_tiny_physical_values_at_maximum_png_density_survive_widget_frames() {
        let mut app = studio();
        assert!(app.editor.core("canvas.new", json!({"width":1,"height":2})));
        let info = density(i32::MAX as u32, i32::MAX as u32 - 1, DensityUnit::Meter);
        app.editor.session.doc.set_import_info(info).unwrap();
        app.editor.open_image_size();
        let pointer = app.editor.session.doc.layers[0].pixels.as_ptr();
        let ctx = Context::default();
        for (index, unit) in [Unit::Inches, Unit::Millimeters, Unit::Pixels]
            .into_iter()
            .enumerate()
        {
            let d = app
                .editor
                .geometry
                .as_mut()
                .unwrap()
                .image_size
                .as_mut()
                .unwrap();
            d.switch_unit(unit, [1, 2]).unwrap();
            let values = [d.width, d.height];
            for t in 0..4 {
                frame(
                    &mut app,
                    &ctx,
                    vec2(960., 680.),
                    vec![],
                    (index * 4 + t) as f64,
                );
            }
            let mut dialog = app.editor.geometry.take().unwrap();
            let d = dialog.image_size.as_ref().unwrap();
            assert_eq!([d.width, d.height], values);
            assert_eq!(
                dimensions(&app.editor.plan_image_size(d).unwrap()),
                Some([1, 2])
            );
            dialog.image_size.as_mut().unwrap().error.clear();
            app.editor.geometry = Some(dialog);
        }
        assert_eq!(app.editor.session.doc.info(), info);
        assert_eq!(app.editor.session.doc.layers[0].pixels.as_ptr(), pointer);
    }
    #[test]
    fn image_size_cancel_and_external_density_or_revision_changes_never_apply_a_stale_draft() {
        let mut app = studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        let ctx = Context::default();
        let size = vec2(960., 680.);
        app.editor.open_image_size();
        let before = app.editor.session.state();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let d = app
            .editor
            .geometry
            .as_mut()
            .unwrap()
            .image_size
            .as_mut()
            .unwrap();
        d.switch_unit(Unit::Inches, [6, 4]).unwrap();
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
        assert_eq!(app.editor.session.state()["layers"], before["layers"]);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        app.editor.open_image_size();
        let mut dialog = app.editor.geometry.take().unwrap();
        let mut d = dialog.image_size.take().unwrap();
        d.width = 6.;
        let revision = d.revision;
        app.editor
            .session
            .doc
            .set_import_info(density(10000, 20000, DensityUnit::Meter))
            .unwrap();
        assert_eq!(app.editor.session.revision, revision);
        let state = app.editor.session.state();
        assert!(!app.editor.apply_image_size(&dialog, &mut d));
        assert_eq!(d.error, "The canvas changed. Reopen this dialog.");
        assert_eq!(app.editor.session.state(), state);
        app.editor.open_image_size();
        let mut dialog = app.editor.geometry.take().unwrap();
        let mut d = dialog.image_size.take().unwrap();
        assert!(app
            .editor
            .core("canvas.resolution", json!({"resolution_ppi":300})));
        let revision = d.revision;
        d.accept_local_view(app.editor.session.revision, app.editor.session.revision + 1);
        assert_eq!(d.revision, revision);
        let state = app.editor.session.state();
        assert!(!app.editor.apply_image_size(&dialog, &mut d));
        assert_eq!(app.editor.session.state(), state);
    }
    #[test]
    fn image_size_apply_rejects_an_active_hand_contact_even_at_the_current_revision() {
        let mut app = native_studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        assert!(app.editor.core("tool.select", json!({"tool":"hand"})));
        app.editor.open_image_size();
        let mut dialog = app.editor.geometry.take().unwrap();
        let mut d = dialog.image_size.take().unwrap();
        d.width = 6.;
        assert!(app.editor.core(
            "canvas.pointer",
            json!({"phase":"down","x":2,"y":2,"time":0})
        ));
        assert!(!app.editor.session.is_drawing());
        assert!(app.editor.session.stroke.active());
        // Isolate the contact guard from ordinary revision-conflict rejection.
        d.revision = app.editor.session.revision;
        let state = app.editor.session.state();
        let plan = app.editor.plan_image_size(&d).unwrap();
        assert_eq!(plan["drawing"], false);
        assert_eq!(app.editor.session.state(), state);
        assert!(!app.editor.image_size_current(&dialog, &d));
        assert!(!app.editor.apply_image_size(&dialog, &mut d));
        assert_eq!(app.editor.session.state(), state);
        assert!(app.editor.session.stroke.active());
        assert!(app.editor.session.pointer_checkpoint.is_some());
        assert!(app
            .editor
            .core("canvas.pointer", json!({"phase":"up","x":2,"y":2,"time":1})));
    }
    #[test]
    fn image_size_footer_fits_all_languages_at_narrow_wide_and_resized_viewports() {
        let language_before = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            let mut app = studio();
            app.editor.settings.ui.language = language;
            app.editor.open_image_size();
            let pixels = app.editor.session.doc.layers[0].pixels.clone();
            let d = app
                .editor
                .geometry
                .as_mut()
                .unwrap()
                .image_size
                .as_mut()
                .unwrap();
            d.switch_unit(Unit::Millimeters, [600, 450]).unwrap();
            let ctx = Context::default();
            for (n, size) in [
                vec2(1360., 920.),
                vec2(460., 380.),
                vec2(1700., 1150.),
                vec2(700., 500.),
            ]
            .into_iter()
            .enumerate()
            {
                for t in 0..3 {
                    frame(&mut app, &ctx, size, vec![], (n * 4 + t) as f64);
                }
                let output = frame(&mut app, &ctx, size, vec![], (n * 4 + 3) as f64);
                for source in ["Apply", "Cancel"] {
                    let text = crate::i18n::text_in(language, source);
                    let (clip, p) = output
                        .shapes
                        .iter()
                        .find_map(|s| label(&s.shape, &text).map(|p| (s.clip_rect, p)))
                        .unwrap_or_else(|| panic!("{language:?}: {source} missing at {size:?}"));
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains(p),
                        "{language:?}: {source} outside {size:?}: {p:?}"
                    );
                    assert!(
                        clip.contains(p),
                        "{language:?}: {source} clipped at {size:?}: {p:?}, {clip:?}"
                    );
                }
            }
            assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        }
        crate::i18n::set_language(language_before);
    }
}
