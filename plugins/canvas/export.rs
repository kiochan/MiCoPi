//! Read-only export sizing drafts; the registered backend owns sampling and output.
use super::*;
use crate::application::{output_size, Action as CoreAction};
use serde_json::{json, Value};

pub(super) struct Draft {
    identity: u64,
    revision: u64,
    source: [usize; 2],
    info: crate::engine::document_info::Info,
    percentage: bool,
    scale: f64,
    unit: usize,
    width: f64,
    height: f64,
    locked: bool,
    width_axis: bool,
    method: usize,
    override_resolution: bool,
    resolution: u32,
    texture: Option<TextureHandle>,
    preview_args: Option<Value>,
    error: String,
}
impl Draft {
    pub(super) fn accept_local_view(&mut self, identity: u64, before: u64, after: u64) {
        if self.identity == identity && self.revision == before {
            self.revision = after;
        }
    }
    fn args(&self) -> Value {
        let mut value = json!({"method":(["area","bilinear","nearest"][self.method])});
        if self.percentage {
            value["scale"] = json!(self.scale);
        } else {
            value["unit"] = json!((["px", "mm", "in"][self.unit]));
            value["lock_aspect"] = json!(self.locked);
            if !self.locked || self.width_axis {
                value["width"] = json!(self.width);
            }
            if !self.locked || !self.width_axis {
                value["height"] = json!(self.height);
            }
        }
        if self.override_resolution {
            value["resolution_ppi"] = json!(self.resolution);
        }
        value
    }
    fn plan(&self) -> Result<output_size::Plan, crate::application::ActionError> {
        output_size::plan(self.source, self.info, &self.args())
    }
    fn current(&self, editor: &Editor) -> bool {
        self.identity == editor.session.doc.identity
            && self.revision == editor.session.revision
            && !editor.session.is_drawing()
    }
    fn display_axes(&self) -> [f64; 2] {
        if self.override_resolution {
            return [self.resolution as f64; 2];
        }
        let state = self.info.state(self.source[0], self.source[1]);
        [
            state["resolution_ppi_x"]
                .as_f64()
                .unwrap_or(self.info.resolution_ppi as f64),
            state["resolution_ppi_y"]
                .as_f64()
                .unwrap_or(self.info.resolution_ppi as f64),
        ]
    }
    fn in_unit(&self, pixels: [usize; 2]) -> [f64; 2] {
        let ppi = self.display_axes();
        std::array::from_fn(|i| match self.unit {
            1 => pixels[i] as f64 * 25.4 / ppi[i],
            2 => pixels[i] as f64 / ppi[i],
            _ => pixels[i] as f64,
        })
    }
    fn set_dimensions(&mut self, pixels: [usize; 2]) {
        let values = self.in_unit(pixels);
        self.width = values[0];
        self.height = values[1];
    }
    fn sync_derived(&mut self) {
        if !self.percentage && self.locked {
            if let Ok(plan) = self.plan() {
                let values = self.in_unit([plan.width, plan.height]);
                if self.width_axis {
                    self.height = values[1];
                } else {
                    self.width = values[0];
                }
            }
        }
    }
}
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    if input.len() > 96 * 1024 * 1024 || input.len() % 4 != 0 {
        return Err("Invalid export preview data".into());
    }
    let digit = |b: u8| match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    for (i, q) in input.as_bytes().chunks_exact(4).enumerate() {
        let a = digit(q[0]).ok_or("Invalid export preview data")?;
        let b = digit(q[1]).ok_or("Invalid export preview data")?;
        let last = (i + 1) * 4 == input.len();
        if q[2] == b'=' {
            if !last || q[3] != b'=' || b & 15 != 0 {
                return Err("Invalid export preview data".into());
            }
            out.push(a << 2 | b >> 4);
            continue;
        }
        let c = digit(q[2]).ok_or("Invalid export preview data")?;
        out.push(a << 2 | b >> 4);
        out.push(b << 4 | c >> 2);
        if q[3] == b'=' {
            if !last || c & 3 != 0 {
                return Err("Invalid export preview data".into());
            }
        } else {
            out.push(c << 6 | digit(q[3]).ok_or("Invalid export preview data")?);
        }
    }
    Ok(out)
}
impl Editor {
    pub(super) fn open_export_options(&mut self, parameters: Value) {
        let source = [self.session.doc.width, self.session.doc.height];
        if let Err(error) = output_size::plan(source, self.session.doc.info(), &parameters) {
            self.status = error.to_string();
            return;
        }
        let unit = match parameters["unit"].as_str() {
            Some("mm") => 1,
            Some("in") => 2,
            _ => 0,
        };
        let mut d = Draft {
            identity: self.session.doc.identity,
            revision: self.session.revision,
            source,
            info: self.session.doc.info(),
            percentage: parameters.get("scale").is_some(),
            scale: parameters["scale"].as_f64().unwrap_or(100.),
            unit,
            width: source[0] as f64,
            height: source[1] as f64,
            locked: parameters["lock_aspect"].as_bool().unwrap_or(true),
            width_axis: parameters.get("height").is_none() || parameters.get("width").is_some(),
            method: match parameters["method"].as_str() {
                Some("nearest") => 2,
                Some("bilinear") => 1,
                _ => 0,
            },
            override_resolution: parameters.get("resolution_ppi").is_some(),
            resolution: parameters["resolution_ppi"]
                .as_f64()
                .unwrap_or(self.session.doc.info().resolution_ppi as f64)
                as u32,
            texture: None,
            preview_args: None,
            error: String::new(),
        };
        d.set_dimensions(source);
        if let Some(v) = parameters["width"].as_f64() {
            d.width = v;
        }
        if let Some(v) = parameters["height"].as_f64() {
            d.height = v;
        }
        d.sync_derived();
        self.export_draft = Some(d);
    }
    fn refresh_export_preview(&mut self, ctx: &Context, d: &mut Draft) -> Result<(), String> {
        if !d.current(self) {
            return Err("The canvas changed. Reopen this dialog.".into());
        }
        let args = d.args();
        let mut request = CoreAction::new("canvas.export.preview", args.clone());
        request.expected_revision = Some(d.revision);
        let mut response = self.session.apply(request).map_err(|e| e.to_string())?;
        let encoded = response["result"]["data"].take();
        let bytes = decode_base64(encoded.as_str().ok_or("Invalid export preview data")?)?;
        drop(encoded);
        drop(response);
        let mut reader =
            image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
        let image = ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        if let Some(texture) = d.texture.as_mut() {
            texture.set(image, TextureOptions::LINEAR);
        } else {
            d.texture = Some(ctx.load_texture("export-preview", image, TextureOptions::LINEAR));
        }
        d.preview_args = Some(args);
        d.error.clear();
        Ok(())
    }
    fn export_to_path(&mut self, d: &mut Draft, path: PathBuf) -> bool {
        if !d.current(self) {
            d.error = "The canvas changed. Reopen this dialog.".into();
            return false;
        }
        let mut args = d.args();
        args["path"] = json!(path);
        if self.core_at_revision("canvas.export", args, Some(d.revision)) {
            self.status = "PNG Exported".into();
            true
        } else {
            d.error = self.status.clone();
            false
        }
    }
    pub(super) fn export_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.export_draft.take() else {
            return;
        };
        d.sync_derived();
        let a = self.settings.ui.clone();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 20.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 8.),
        );
        let height =
            (screen.height() - a.control_height * 5. - a.panel_padding * 4. - a.outer_padding * 2.)
                .max(a.control_height * 2.);
        let stale = !d.current(self);
        let mut open = true;
        let mut export = false;
        let mut cancel = false;
        let mut refresh = false;
        Window::new(tr("Export PNG")).id(Id::new("canvas-export-options")).open(&mut open)
            .collapsible(false).resizable(false).frame(frame).min_width(width).max_width(width).default_width(width)
            .anchor(Align2::CENTER_CENTER,Vec2::ZERO).show(ctx,|ui| {
                ScrollArea::vertical().id_salt("export-sizing-controls").max_height(height).auto_shrink([false,true]).show(ui,|ui| {
                    ui.weak(tr("Create a sized copy of the visible artwork. The document stays unchanged."));
                    ui.add_space(a.section_spacing);
                    let prior_pixels=d.plan().map(|p|[p.width,p.height]).unwrap_or(d.source);
                    ui.horizontal_wrapped(|ui|{
                        let old=d.percentage;
                        ui.selectable_value(&mut d.percentage,false,tr("Dimensions"));
                        ui.selectable_value(&mut d.percentage,true,tr("Scale Percentage"));
                        if old!=d.percentage {
                            if d.percentage {d.scale=prior_pixels[0] as f64/d.source[0] as f64*100.;}
                            else {d.set_dimensions(prior_pixels);}
                        }
                    });
                    ui.add_space(a.item_spacing);
                    if d.percentage {
                        ui.horizontal(|ui|{ui.label(tr("Scale"));ui.add(DragValue::new(&mut d.scale).range(0.01..=409600.).speed(1.).suffix(" %"));});
                    } else {
                        let before=d.plan().map(|p|[p.width,p.height]).unwrap_or(d.source);
                        let old=d.unit;
                        ComboBox::from_id_salt("export-size-unit").selected_text(tr(["Pixels","Millimeters","Inches"][d.unit])).show_ui(ui,|ui|{
                            for (i,title) in ["Pixels","Millimeters","Inches"].iter().enumerate(){ui.selectable_value(&mut d.unit,i,tr(*title));}
                        });
                        if old!=d.unit {d.set_dimensions(before);}
                        ui.checkbox(&mut d.locked,tr("Lock Aspect Ratio"));
                        Grid::new("export-dimensions").num_columns(2).spacing(vec2(a.section_spacing,a.item_spacing)).show(ui,|ui| {
                            if d.locked {ui.radio_value(&mut d.width_axis,true,tr("Width"));}else{ui.label(tr("Width"));}
                            ui.add_enabled(!d.locked || d.width_axis,DragValue::new(&mut d.width).range(0.000001..=10000000.).speed(if d.unit==0 {1.}else{0.1}).max_decimals(if d.unit==0 {0}else{4}).suffix([" px"," mm"," in"][d.unit]));ui.end_row();
                            if d.locked {ui.radio_value(&mut d.width_axis,false,tr("Height"));}else{ui.label(tr("Height"));}
                            ui.add_enabled(!d.locked || !d.width_axis,DragValue::new(&mut d.height).range(0.000001..=10000000.).speed(if d.unit==0 {1.}else{0.1}).max_decimals(if d.unit==0 {0}else{4}).suffix([" px"," mm"," in"][d.unit]));ui.end_row();
                        });
                    }
                    ui.add_space(a.item_spacing);
                    if ui.button(tr("Reset Size")).clicked(){d.percentage=false;d.unit=0;d.locked=true;d.width_axis=true;d.set_dimensions(d.source);}
                    ui.add_space(a.section_spacing);
                    ui.label(tr("Sampling"));
                    ComboBox::from_id_salt("export-sampling").selected_text(tr(["Area Average","Bilinear","Nearest Neighbor"][d.method])).show_ui(ui,|ui|{
                        for (i,title) in ["Area Average","Bilinear","Nearest Neighbor"].iter().enumerate(){ui.selectable_value(&mut d.method,i,tr(*title));}
                    });
                    ui.add_space(a.section_spacing);
                    ui.checkbox(&mut d.override_resolution,tr("Override Output Resolution"));
                    if d.override_resolution {ui.horizontal(|ui|{ui.label(tr("Resolution"));ui.add(DragValue::new(&mut d.resolution).range(1..=2400).suffix(" ppi"));});}
                    else {ui.weak(tr("Preserve the document's exact density and units."));}
                    ui.add_space(a.section_spacing);
                    match d.plan() {
                        Ok(plan)=>{
                            ui.strong(format!("{}: {} × {} px",localize("Output Size"),plan.width,plan.height));
                            let info=plan.info.state(plan.width,plan.height);
                            if let (Some(w),Some(h))=(info["width_mm"].as_f64(),info["height_mm"].as_f64()) {ui.label(format!("{}: {:.2} × {:.2} mm",localize("Physical Size"),w,h));}
                            else {ui.weak(tr("Physical size is unknown. Set an output resolution to use millimeters or inches."));}
                            ui.weak(format!("{}: {:.1} MiB",localize("Output Pixel Buffer"),plan.width as f64*plan.height as f64*4./(1024.*1024.)));
                            ui.weak(tr("PNG encoding and preview textures use additional memory."));
                        }
                        Err(error)=>{ui.colored_label(Color32::LIGHT_RED,localize(&error.message));}
                    }
                    ui.add_space(a.section_spacing);
                    refresh=ui.add_enabled(!stale && d.plan().is_ok(),Button::new(tr("Refresh Preview"))).clicked();
                    ui.weak(tr("Preview refresh is explicit. Adjusting controls does not resample the artwork."));
                    if let Some(texture)=d.texture.as_ref() {
                        if d.preview_args.as_ref()!=Some(&d.args()){ui.weak(tr("Preview is out of date. Refresh to update."));}
                        let size=texture.size_vec2();let max=vec2(ui.available_width(),a.control_height*7.);
                        let factor=(max.x/size.x).min(max.y/size.y);
                        let (rect,_)=ui.allocate_exact_size(size*factor,Sense::hover());
                        let tile=a.panel_padding.max(8.);
                        for y in 0..(rect.height()/tile).ceil() as usize {for x in 0..(rect.width()/tile).ceil() as usize {
                            let r=Rect::from_min_size(rect.min+vec2(x as f32*tile,y as f32*tile),vec2(tile,tile)).intersect(rect);
                            ui.painter().rect_filled(r,0.,if (x+y)%2==0{Color32::from_gray(225)}else{Color32::from_gray(245)});
                        }}
                        ui.painter().image(texture.id(),rect,Rect::from_min_max(Pos2::ZERO,pos2(1.,1.)),Color32::WHITE);
                        ui.weak(tr("Preview is fitted to this window."));
                    }
                });
                ui.add_space(a.panel_padding);
                if !d.error.is_empty() {
                    ScrollArea::vertical().id_salt("export-error").max_height(a.control_height*2.).show(ui,|ui| {
                        ui.colored_label(Color32::LIGHT_RED,localize(&d.error));
                    });
                } else if stale {ui.colored_label(Color32::LIGHT_RED,tr("The canvas changed. Reopen this dialog."));}
                ui.separator();
                ui.horizontal(|ui|{
                    export=ui.add_enabled(!stale && d.plan().is_ok(),Button::new(tr("Export PNG…"))).clicked();
                    cancel=ui.button(tr("Cancel")).clicked();
                });
            });
        if refresh {
            if let Err(error) = self.refresh_export_preview(ctx, &mut d) {
                d.error = error;
            }
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if export {
            if let Some(path) = self.native.as_ref().and_then(|n| n.dialog(true, true)) {
                if self.export_to_path(&mut d, path) {
                    open = false;
                }
            }
        }
        if open && !cancel {
            self.export_draft = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    fn app() -> (Studio, Context) {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        assert!(app.editor.core(
            "canvas.new",
            json!({"width":12,"height":8,"background":"#AABBCC"})
        ));
        app.editor.sync_from_session();
        let ctx = Context::default();
        for n in 0..3 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], n as f64);
        }
        (app, ctx)
    }
    fn text(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            Shape::Vec(v) => v.iter().find_map(|s| text(s, label)),
            _ => None,
        }
    }
    #[test]
    fn export_options_use_shared_physical_plans_and_cancel_does_not_publish_artwork() {
        let (mut app, ctx) = app();
        app.editor.actions.push(Action::Export);
        app.run_actions(&ctx);
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 3.);
        assert!(app.editor.export_draft.is_some());
        let state = app.editor.session.state();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let history = app.editor.session.doc.history_bytes();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let d = app.editor.export_draft.as_mut().unwrap();
        d.unit = 1;
        d.width = 25.4;
        d.override_resolution = true;
        d.resolution = 72;
        assert_eq!(
            (d.plan().unwrap().width, d.plan().unwrap().height),
            (72, 48)
        );
        d.sync_derived();
        assert!((d.height - 25.4 * 2. / 3.).abs() < 1e-10);
        for n in 4..7 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], n as f64);
        }
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            7.,
        );
        assert!(app.editor.export_draft.is_none());
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
    }
    #[test]
    fn export_preview_is_explicit_cached_and_source_changes_reject_stale_output() {
        let (mut app, ctx) = app();
        app.editor
            .open_export_options(json!({"scale":50,"method":"nearest"}));
        let mut d = app.editor.export_draft.take().unwrap();
        let state = app.editor.session.state();
        let history = app.editor.session.doc.history_bytes();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        app.editor.refresh_export_preview(&ctx, &mut d).unwrap();
        assert_eq!(d.texture.as_ref().unwrap().size(), [6, 4]);
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        let old = d.preview_args.clone();
        d.scale = 25.;
        app.editor.export_draft = Some(d);
        for n in 3..6 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], n as f64);
        }
        assert_eq!(app.editor.export_draft.as_ref().unwrap().preview_args, old);
        assert_eq!(
            app.editor
                .export_draft
                .as_ref()
                .unwrap()
                .texture
                .as_ref()
                .unwrap()
                .size(),
            [6, 4]
        );
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        assert!(app.editor.core("layer.add", json!({})));
        let mut d = app.editor.export_draft.take().unwrap();
        let state = app.editor.session.state();
        assert!(app.editor.refresh_export_preview(&ctx, &mut d).is_err());
        assert!(!app.editor.export_to_path(
            &mut d,
            PathBuf::from("target/test-output/unexpected-stale-export.png")
        ));
        assert_eq!(app.editor.session.state(), state);
    }
    #[test]
    fn export_actual_file_is_a_sized_copy_with_source_history_intact() {
        let (mut app, ctx) = app();
        app.editor
            .open_export_options(json!({"width":6,"method":"area","resolution_ppi":300}));
        // Local fit changes during resize do not make an otherwise current draft stale.
        for (n, size) in [vec2(700., 500.), vec2(1360., 920.)]
            .into_iter()
            .enumerate()
        {
            frame(&mut app, &ctx, size, vec![], 3. + n as f64);
        }
        let mut d = app.editor.export_draft.take().unwrap();
        assert!(d.current(&app.editor));
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let composition = app.editor.session.doc.composition();
        let history = app.editor.session.doc.history_bytes();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/export-options-{}.png",
            std::process::id()
        ));
        assert!(
            app.editor.export_to_path(&mut d, path.clone()),
            "{}",
            d.error
        );
        let image = image::open(&path).unwrap();
        assert_eq!((image.width(), image.height()), (6, 4));
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (12, 8)
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.composition(), composition);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
    }
    #[test]
    fn export_footer_stays_visible_in_narrow_short_and_wide_windows() {
        for size in [vec2(460., 380.), vec2(700., 500.), vec2(1700., 1000.)] {
            let (mut app, ctx) = app();
            app.editor.open_export_options(json!({"scale":50}));
            for n in 3..6 {
                frame(&mut app, &ctx, size, vec![], n as f64);
            }
            let output = frame(&mut app, &ctx, size, vec![], 6.);
            for label in ["Export PNG…", "Cancel"] {
                let (clip, p) = output
                    .shapes
                    .iter()
                    .find_map(|s| text(&s.shape, label).map(|p| (s.clip_rect, p)))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(p),
                    "{label} outside {size:?}"
                );
                assert!(clip.contains(p), "{label} clipped at {size:?}");
            }
        }
    }
    #[test]
    fn export_written_receipt_remains_visible_when_recording_failure_stales_the_draft() {
        let (mut app, ctx) = app();
        app.editor.open_export_options(json!({"scale":50}));
        let mut d = app.editor.export_draft.take().unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/export-receipt-{}.png",
            std::process::id()
        ));
        app.editor.session.replay_boundary().unwrap();
        app.editor
            .session
            .journal
            .as_mut()
            .unwrap()
            .fail_next_write_after(7);
        assert!(!app.editor.export_to_path(&mut d, path.clone()));
        assert!(d.error.contains("PNG was written to"), "{}", d.error);
        assert!(path.is_file());
        assert!(!d.current(&app.editor));
        app.editor.export_draft = Some(d);
        let size = vec2(460., 380.);
        for n in 3..6 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let output = frame(&mut app, &ctx, size, vec![], 6.);
        assert!(output.shapes.iter().any(|s| match &s.shape {
            Shape::Text(t) =>
                t.galley.text().contains("PNG was written to")
                    && s.clip_rect.contains(t.pos + vec2(1., 1.)),
            _ => false,
        }));
        for label in ["Export PNG…", "Cancel"] {
            let (clip, p) = output
                .shapes
                .iter()
                .find_map(|s| text(&s.shape, label).map(|p| (s.clip_rect, p)))
                .unwrap();
            assert!(Rect::from_min_size(Pos2::ZERO, size).contains(p));
            assert!(clip.contains(p));
        }
    }
    #[test]
    fn export_commands_and_state_are_discoverable_in_four_languages_without_editing_artwork() {
        crate::i18n::load_test_catalogs();
        let (mut app, _) = app();
        app.editor.register_commands();
        let before = app.editor.session.state();
        let composition = app.editor.session.doc.composition();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let history = app.editor.session.doc.history_bytes();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        for (action, queries) in [
            (
                "canvas.export.plan",
                [
                    "Plan PNG Export",
                    "规划 PNG 导出",
                    "PNG 書き出しを計画",
                    "PNG 내보내기 계획",
                ],
            ),
            (
                "canvas.export.preview",
                [
                    "Preview PNG Export",
                    "预览 PNG 导出",
                    "PNG 書き出しをプレビュー",
                    "PNG 내보내기 미리보기",
                ],
            ),
            (
                "canvas.export.open",
                [
                    "PNG Export Options",
                    "PNG 导出选项",
                    "PNG 書き出しオプション",
                    "PNG 내보내기 옵션",
                ],
            ),
        ] {
            let keys = &app.editor.settings.keys;
            for query in queries.into_iter().chain(std::iter::once(action)) {
                let mut palette =
                    crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, keys);
                palette.query(query.into(), keys);
                assert!(
                    palette.results.iter().any(|&i| {
                        let command = keys.command(i);
                        command.keywords == action && command.owner == Some("core")
                    }),
                    "{action}: {query}"
                );
            }
            app.editor.begin_plugin_command(action);
            assert!(app.editor.command_form.is_some());
            assert_eq!(app.editor.session.state(), before);
            app.editor.command_form = None;
        }
        let state = &app.editor.session.registry.states["core.export"];
        assert_eq!(state.action, "canvas.export.plan");
        let owner = app
            .editor
            .session
            .registry
            .contribution_owner("states", "core.export");
        assert_eq!(owner, Some("core"));
        let terms = crate::i18n::search_text_for(owner, &state.title).to_lowercase();
        for query in [
            "Inspect Default PNG Export",
            "查看默认 PNG 导出",
            "既定の PNG 書き出しを確認",
            "기본 PNG 내보내기 확인",
        ] {
            assert!(terms.contains(&query.to_lowercase()), "{query}");
        }
        assert_eq!(app.editor.session.state(), before);
        assert_eq!(app.editor.session.doc.composition(), composition);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        assert!(app.editor.export_draft.is_none());
    }
    #[test]
    fn export_preview_base64_parser_bounds_and_padding() {
        for bytes in [
            b"".as_slice(),
            b"a",
            b"ab",
            b"abc",
            b"abcd",
            &[0, 127, 128, 255],
        ] {
            assert_eq!(
                decode_base64(&crate::application::codec::base64(bytes)).unwrap(),
                bytes
            );
        }
        for invalid in ["x", "A===", "AA=A", "AA==AAAA", "AB==", "AAF=", "AA!!"] {
            assert!(decode_base64(invalid).is_err(), "{invalid}");
        }
    }
}
