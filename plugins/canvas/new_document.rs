//! Provisional document setup; creation uses the revision-guarded shared action.
use super::*;
use crate::application::{units::Unit, Action as CoreAction};
use crate::configuration::CanvasPreset;
use serde_json::json;
use serde_json::Value;

pub(super) struct NewCanvasDraft {
    identity: u64,
    revision: u64,
    preset: CanvasPreset,
    unit: Unit,
    width: f64,
    height: f64,
    color: [u8; 3],
    background_mode: usize,
    error: String,
}
impl NewCanvasDraft {
    pub(super) fn accept_local_view(&mut self, identity: u64, before: u64, after: u64) {
        if self.identity == identity && self.revision == before {
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
        let mut args = json!({"width":number(self.width),"height":number(self.height),
            "resolution_ppi":self.preset.resolution_ppi,"background":self.preset.background});
        if self.unit != Unit::Pixels {
            args["unit"] = json!(self.unit);
        }
        args
    }
    fn switch_unit(&mut self, unit: Unit, pixels: [usize; 2]) {
        self.unit = unit;
        let ppi = self.preset.resolution_ppi as f64;
        self.width = unit.from_pixels(pixels[0], ppi);
        self.height = unit.from_pixels(pixels[1], ppi);
    }
    fn matches(&self, preset: &CanvasPreset, pixels: Option<[usize; 2]>) -> bool {
        self.preset.id == preset.id
            && self.preset.name == preset.name
            && self.preset.resolution_ppi == preset.resolution_ppi
            && self.preset.background == preset.background
            && pixels == Some([preset.width, preset.height])
    }
    fn choose(&mut self, preset: &CanvasPreset) {
        self.preset = preset.clone();
        self.unit = Unit::Pixels;
        self.width = preset.width as f64;
        self.height = preset.height as f64;
        self.background_mode = if preset.background == "transparent" {
            0
        } else if preset.background.eq_ignore_ascii_case("#FFFFFF") {
            1
        } else {
            2
        };
        if let Some(rgb) = preset.background.strip_prefix('#') {
            if let Ok(n) = u32::from_str_radix(rgb, 16) {
                self.color = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            }
        }
    }
}
fn planned_pixels(value: &Value) -> Option<[usize; 2]> {
    Some([
        value["width"].as_u64()? as usize,
        value["height"].as_u64()? as usize,
    ])
}
fn unit_label(unit: Unit) -> &'static str {
    match unit {
        Unit::Pixels => "Pixels",
        Unit::Millimeters => "Millimeters",
        Unit::Inches => "Inches",
    }
}
fn dimension_drag(value: &mut f64, unit: Unit) -> DragValue<'_> {
    let pixels = unit == Unit::Pixels;
    DragValue::new(value)
        .range(if pixels {
            1.0..=4096.0
        } else {
            0.000001..=1000000000.0
        })
        .speed(if pixels { 1. } else { 0.1 })
        .max_decimals(if pixels { 0 } else { 4 })
        .suffix(match unit {
            Unit::Pixels => " px",
            Unit::Millimeters => " mm",
            Unit::Inches => " in",
        })
}
pub(in crate::desktop) fn preset_title(p: &CanvasPreset) -> String {
    // User names are document-like text, independent of display language.
    let built_in = matches!(
        (p.id.as_str(), p.name.as_str()),
        ("illustration", "Illustration") | ("square", "Square") | ("a4", "A4") | ("b5", "B5")
    );
    if built_in {
        localize(&p.name)
    } else {
        p.name.clone()
    }
}
impl Editor {
    pub(super) fn open_new_canvas(&mut self) {
        self.finish_stroke();
        self.push_gui_parameters();
        self.push_gui_view();
        let p = &self.settings.new_canvas;
        let preset = p
            .presets
            .iter()
            .find(|x| x.id == p.default_preset)
            .or_else(|| p.presets.first())
            .expect("validated canvas presets")
            .clone();
        let mut draft = NewCanvasDraft {
            identity: self.session.doc.identity,
            revision: self.session.revision,
            preset: preset.clone(),
            unit: Unit::Pixels,
            width: preset.width as f64,
            height: preset.height as f64,
            color: [255; 3],
            background_mode: 0,
            error: String::new(),
        };
        draft.choose(&preset);
        self.new_canvas = Some(draft);
        self.new_open = true;
    }
    fn plan_new_canvas(&mut self, d: &NewCanvasDraft) -> Result<Value, String> {
        self.session
            .apply(CoreAction::new("canvas.new.plan", d.args()))
            .map(|mut response| response["result"].take())
            .map_err(|error| error.message)
    }
    fn create_new_canvas(&mut self, d: &mut NewCanvasDraft) -> bool {
        if d.identity != self.session.doc.identity
            || d.revision != self.session.revision
            || self.session.is_drawing()
        {
            d.error = "The canvas changed. Reopen this dialog.".into();
            return false;
        }
        if let Err(error) = self.plan_new_canvas(d) {
            d.error = error;
            return false;
        }
        if self.core_at_revision("canvas.new", d.args(), Some(d.revision)) {
            self.sync_from_session();
            self.fit = true;
            self.pan = Vec2::ZERO;
            true
        } else {
            d.error = self.status.clone();
            false
        }
    }
}
impl Studio {
    pub(super) fn new_document_window(&mut self, ctx: &Context) {
        if !self.editor.new_open {
            self.editor.new_canvas = None;
            return;
        }
        if self.editor.new_canvas.is_none() {
            self.editor.open_new_canvas();
        }
        let mut d = self.editor.new_canvas.take().unwrap();
        let mut plan = self.editor.plan_new_canvas(&d);
        let before_args = d.args();
        let a = self.editor.settings.ui.clone();
        let screen = ctx.content_rect();
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 18.).min(
            (screen.width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 8.),
        );
        let scroll_height =
            (screen.height() - a.control_height * 5. - a.panel_padding * 4. - a.outer_padding * 2.)
                .max(a.control_height * 2.);
        // Reserve the current scroll budget and footer before egui applies its
        // cached window clip, including immediately after a viewport resize.
        let footer_height = a.control_height + a.panel_padding + a.item_spacing * 3. + 1.;
        let stale = d.identity != self.editor.session.doc.identity
            || d.revision != self.editor.session.revision
            || self.editor.session.is_drawing();
        let mut open = true;
        let mut create = false;
        let mut cancel = false;
        Window::new(tr("New Canvas"))
            .id(Id::new("new-canvas"))
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
                ScrollArea::vertical()
                    .id_salt("new-canvas-controls")
                    .max_height(scroll_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.weak(tr("Start with a preset, then make it yours."));
                        ui.add_space(a.section_spacing);
                        ui.label(tr("Canvas Preset"));
                        let mut selected = None;
                        ComboBox::from_id_salt("new-canvas-preset")
                            .width((width - a.panel_padding * 2.).max(a.control_height * 4.))
                            .selected_text(
                                if self.editor.settings.new_canvas.presets.iter().any(|p| {
                                    d.matches(p, plan.as_ref().ok().and_then(planned_pixels))
                                }) {
                                    preset_title(&d.preset)
                                } else {
                                    localize("Custom Canvas").into_owned()
                                },
                            )
                            .show_ui(ui, |ui| {
                                for p in &self.editor.settings.new_canvas.presets {
                                    if ui
                                        .selectable_label(
                                            d.matches(
                                                p,
                                                plan.as_ref().ok().and_then(planned_pixels),
                                            ),
                                            preset_title(p),
                                        )
                                        .clicked()
                                    {
                                        selected = Some(p.clone());
                                    }
                                }
                            });
                        if let Some(p) = selected {
                            d.choose(&p);
                            plan = self.editor.plan_new_canvas(&d);
                        }
                        ui.add_space(a.section_spacing);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr("Units"));
                            let old = d.unit;
                            ComboBox::from_id_salt("new-canvas-units")
                                .selected_text(tr(unit_label(d.unit)))
                                .show_ui(ui, |ui| {
                                    for unit in [Unit::Pixels, Unit::Millimeters, Unit::Inches] {
                                        ui.selectable_value(
                                            &mut d.unit,
                                            unit,
                                            tr(unit_label(unit)),
                                        );
                                    }
                                });
                            if old != d.unit {
                                if let Some(pixels) = plan.as_ref().ok().and_then(planned_pixels) {
                                    d.switch_unit(d.unit, pixels);
                                } else {
                                    d.unit = old;
                                    d.error =
                                        "Enter valid dimensions before switching units.".into();
                                }
                            }
                        });
                        ui.add_space(a.section_spacing);
                        Grid::new("new-canvas-size")
                            .num_columns(2)
                            .spacing(vec2(a.section_spacing, a.item_spacing))
                            .show(ui, |ui| {
                                ui.label(tr("Width"));
                                ui.add(dimension_drag(&mut d.width, d.unit));
                                ui.end_row();
                                ui.label(tr("Height"));
                                ui.add(dimension_drag(&mut d.height, d.unit));
                                ui.end_row();
                                ui.label(tr("Resolution"));
                                ui.add(
                                    DragValue::new(&mut d.preset.resolution_ppi)
                                        .range(1..=2400)
                                        .suffix(" ppi"),
                                );
                                ui.end_row();
                            });
                        ui.add_space(a.item_spacing);
                        if ui.button(tr("Swap Width and Height")).clicked() {
                            std::mem::swap(&mut d.width, &mut d.height);
                        }
                        ui.add_space(a.section_spacing);
                        ui.label(tr("Initial Background"));
                        let mut mode = d.background_mode;
                        let old_mode = mode;
                        ComboBox::from_id_salt("new-canvas-background")
                            .selected_text(tr(["Transparent", "White", "Custom Color"][mode]))
                            .show_ui(ui, |ui| {
                                for (i, title) in
                                    ["Transparent", "White", "Custom Color"].iter().enumerate()
                                {
                                    ui.selectable_value(&mut mode, i, tr(*title));
                                }
                            });
                        d.background_mode = mode;
                        if old_mode != mode {
                            d.preset.background = match mode {
                                0 => "transparent".into(),
                                1 => "#FFFFFF".into(),
                                _ => format!(
                                    "#{:02X}{:02X}{:02X}",
                                    d.color[0], d.color[1], d.color[2]
                                ),
                            };
                        }
                        if mode == 2 {
                            ui.horizontal(|ui| {
                                if ui.color_edit_button_srgb(&mut d.color).changed() {
                                    d.preset.background = format!(
                                        "#{:02X}{:02X}{:02X}",
                                        d.color[0], d.color[1], d.color[2]
                                    );
                                }
                                ui.monospace(&d.preset.background);
                            });
                        }
                        ui.add_space(a.section_spacing);
                        plan = self.editor.plan_new_canvas(&d);
                        if plan.is_ok() && d.args() != before_args {
                            d.error.clear();
                        }
                        match &plan {
                            Ok(result) => {
                                if let Some([w, h]) = planned_pixels(result) {
                                    ui.strong(format!(
                                        "{}: {} × {} px",
                                        localize("Pixel Dimensions"),
                                        w,
                                        h
                                    ));
                                }
                                let info = &result["document_info"];
                                if let (Some(w), Some(h)) =
                                    (info["width_mm"].as_f64(), info["height_mm"].as_f64())
                                {
                                    ui.label(format!(
                                        "{}: {:.2} × {:.2} mm",
                                        localize("Physical Size"),
                                        w,
                                        h
                                    ));
                                }
                                if let Some(bytes) =
                                    result["estimated_initial_pixel_bytes"].as_u64()
                                {
                                    ui.weak(format!(
                                        "{}: {:.1} MiB",
                                        localize("Initial Pixel Buffers"),
                                        bytes as f64 / (1024. * 1024.)
                                    ));
                                }
                            }
                            Err(error) => {
                                ui.colored_label(Color32::LIGHT_RED, localize(error));
                            }
                        }
                        ui.weak(tr(
                            "Layer and composite only; history and previews use additional memory.",
                        ));
                        ui.weak(tr(if d.unit == Unit::Pixels {
                            "Resolution changes physical size, not pixel count."
                        } else {
                            "In physical units, resolution determines the pixel count."
                        }));
                        if d.unit != Unit::Pixels {
                            ui.weak(tr("Physical dimensions round to whole pixels."));
                        }
                        ui.weak(tr("Background fills the first layer and remains editable."));
                        ui.add_space(a.item_spacing);
                        ui.weak(tr(
                            "Up to 4096 × 4096 pixels. Presets are configurable in Settings.",
                        ));
                        if stale {
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
                    create = ui
                        .add_enabled(!stale && plan.is_ok(), Button::new(tr("Create Canvas")))
                        .clicked();
                    cancel = ui.button(tr("Cancel")).clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            cancel = true;
        }
        if create && self.editor.create_new_canvas(&mut d) {
            open = false;
        }
        self.editor.new_open = open && !cancel;
        if self.editor.new_open {
            self.editor.new_canvas = Some(d);
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
    fn new_canvas_draft_uses_configured_preset_and_cancel_keeps_document_and_preferences() {
        let mut app = studio();
        let ctx = Context::default();
        for n in 0..3 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], n as f64);
        }
        app.editor.settings.new_canvas.default_preset = "square".into();
        app.editor.settings.new_canvas.presets[1].name = "私の原稿".into();
        assert_eq!(
            preset_title(&app.editor.settings.new_canvas.presets[1]),
            "私の原稿"
        );
        app.editor.open_new_canvas();
        let d = app.editor.new_canvas.as_mut().unwrap();
        assert_eq!(d.preset.width, 2048);
        assert_eq!(d.preset.resolution_ppi, 300);
        d.width = 350.;
        d.preset.background = "#AABBCC".into();
        let state = app.editor.session.state();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let history = app.editor.session.doc.history_bytes();
        let preferences = app.editor.settings.encode().unwrap();
        for n in 0..3 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], (n + 3) as f64);
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
        assert!(!app.editor.new_open);
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(app.editor.settings.encode().unwrap(), preferences);
    }
    #[test]
    fn new_canvas_creation_is_revision_guarded_and_one_shared_outcome() {
        let mut app = studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        app.editor.open_new_canvas();
        let mut d = app.editor.new_canvas.take().unwrap();
        d.width = 7.;
        d.height = 5.;
        d.preset.resolution_ppi = 300;
        d.preset.background = "#AABBCC".into();
        assert_eq!(
            d.args(),
            json!({"width":7,"height":5,"resolution_ppi":300,"background":"#AABBCC"})
        );
        app.editor.new_canvas = Some(d);
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        // A fitted stage changes zoom when the host window is resized. That
        // local view update must not invalidate a document-creation draft.
        for (n, resized) in [vec2(700., 500.), size].into_iter().enumerate() {
            frame(&mut app, &ctx, resized, vec![], 2.2 + n as f64 * 0.1);
        }
        assert_eq!(
            app.editor.new_canvas.as_ref().unwrap().revision,
            app.editor.session.revision
        );
        let output = frame(&mut app, &ctx, size, vec![], 3.);
        let p = output
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "Create Canvas"))
            .unwrap_or_else(|| {
                panic!(
                    "Create Canvas missing; labels: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            Shape::Text(t) => Some(t.galley.text()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                )
            });
        for (n, pressed) in [true, false].into_iter().enumerate() {
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                4. + n as f64 * 0.1,
            );
        }
        assert!(!app.editor.new_open, "{}", app.editor.status);
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (7, 5)
        );
        assert!(app.editor.session.doc.layers[0]
            .pixels
            .iter()
            .all(|p| *p == 0xffaabbcc));
        assert_eq!(app.editor.session.doc.history_bytes(), 0);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            2
        );
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
        app.editor.open_new_canvas();
        let mut stale = app.editor.new_canvas.take().unwrap();
        assert!(app.editor.core("layer.add", json!({})));
        let state = app.editor.session.state();
        assert!(!app.editor.create_new_canvas(&mut stale));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(stale.error, "The canvas changed. Reopen this dialog.");
    }
    #[test]
    fn new_canvas_unit_roundtrips_use_resolved_pixels_and_preset_choice_resets_units() {
        let mut app = studio();
        app.editor.open_new_canvas();
        app.editor.register_commands();
        for query in [
            "Plan New Canvas",
            "规划新建画布",
            "新規キャンバスを計画",
            "새 캔버스 계획",
            "canvas.new.plan",
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette =
                crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(query.into(), keys);
            assert!(
                palette.results.iter().any(|&i| {
                    let command = keys.command(i);
                    command.keywords == "canvas.new.plan" && command.owner == Some("core")
                }),
                "{query}"
            );
        }
        let mut d = app.editor.new_canvas.take().unwrap();
        let state = app.editor.session.state();
        let pixel_pointer = app.editor.session.doc.layers[0].pixels.as_ptr();
        for size in [[1, 4096], [17, 31], [2480, 3508]] {
            for ppi in [1, 72, 300, 2400] {
                d.unit = Unit::Pixels;
                d.width = size[0] as f64;
                d.height = size[1] as f64;
                d.preset.resolution_ppi = ppi;
                for _ in 0..4 {
                    for unit in [Unit::Millimeters, Unit::Inches, Unit::Pixels] {
                        let before = app.editor.plan_new_canvas(&d).unwrap();
                        assert_eq!(planned_pixels(&before), Some(size));
                        d.switch_unit(unit, planned_pixels(&before).unwrap());
                        assert_eq!(
                            planned_pixels(&app.editor.plan_new_canvas(&d).unwrap()),
                            Some(size)
                        );
                    }
                }
            }
        }
        let preset = app.editor.settings.new_canvas.presets[2].clone();
        d.choose(&preset);
        assert_eq!(d.unit, Unit::Pixels);
        let pixels = planned_pixels(&app.editor.plan_new_canvas(&d).unwrap());
        assert!(d.matches(&preset, pixels));
        d.switch_unit(Unit::Millimeters, pixels.unwrap());
        assert!(d.matches(
            &preset,
            planned_pixels(&app.editor.plan_new_canvas(&d).unwrap())
        ));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(
            app.editor.session.doc.layers[0].pixels.as_ptr(),
            pixel_pointer
        );
    }
    #[test]
    fn new_canvas_physical_ppi_changes_pixels_and_create_records_the_same_unit_arguments() {
        let mut app = studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        app.editor.open_new_canvas();
        let mut d = app.editor.new_canvas.take().unwrap();
        d.unit = Unit::Millimeters;
        d.width = 25.4;
        d.height = 12.7;
        d.preset.resolution_ppi = 100;
        d.preset.background = "#AABBCC".into();
        assert_eq!(
            planned_pixels(&app.editor.plan_new_canvas(&d).unwrap()),
            Some([100, 50])
        );
        d.preset.resolution_ppi = 200;
        let plan = app.editor.plan_new_canvas(&d).unwrap();
        assert_eq!(planned_pixels(&plan), Some([200, 100]));
        assert_eq!((d.width, d.height), (25.4, 12.7));
        assert_eq!(plan["estimated_initial_pixel_bytes"], 200 * 100 * 8);
        let args = d.args();
        assert!(app.editor.create_new_canvas(&mut d), "{}", d.error);
        assert_eq!(
            (app.editor.session.doc.width, app.editor.session.doc.height),
            (200, 100)
        );
        assert_eq!(app.editor.session.doc.info().resolution_ppi, 200);
        assert!(app.editor.session.doc.layers[0]
            .pixels
            .iter()
            .all(|p| *p == 0xffaabbcc));
        assert_eq!(app.editor.session.doc.history_bytes(), 0);
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), 2);
        let events = journal.events(1, 1).unwrap();
        assert_eq!(events["events"][0]["source"]["args"], args);
        assert_eq!(
            app.editor
                .session
                .apply(CoreAction::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
        app.editor.open_new_canvas();
        let mut stale = app.editor.new_canvas.take().unwrap();
        stale.unit = Unit::Inches;
        stale.width = 1.;
        stale.height = 0.5;
        assert!(app
            .editor
            .core("canvas.resolution", json!({"resolution_ppi":300})));
        let state = app.editor.session.state();
        assert!(!app.editor.create_new_canvas(&mut stale));
        assert_eq!(app.editor.session.state(), state);
    }
    #[test]
    fn new_canvas_physical_draft_footer_is_visible_in_cjk_narrow_and_wide_layouts() {
        let language_before = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            for size in [vec2(460., 380.), vec2(1360., 920.)] {
                let mut app = studio();
                app.editor.settings.ui.language = language;
                app.editor.open_new_canvas();
                let d = app.editor.new_canvas.as_mut().unwrap();
                d.unit = Unit::Millimeters;
                d.width = 210.;
                d.height = 297.;
                d.preset.resolution_ppi = 300;
                let ctx = Context::default();
                for n in 0..3 {
                    frame(&mut app, &ctx, size, vec![], n as f64);
                }
                let output = frame(&mut app, &ctx, size, vec![], 4.);
                for source in ["Create Canvas", "Cancel"] {
                    let text = crate::i18n::text_in(language, source);
                    let (clip, p) = output
                        .shapes
                        .iter()
                        .find_map(|s| label(&s.shape, &text).map(|p| (s.clip_rect, p)))
                        .unwrap();
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains(p),
                        "{language:?}: {source}"
                    );
                    assert!(
                        clip.contains(p),
                        "{language:?}: {source}: {size:?}, point {p:?}, clip {clip:?}"
                    );
                }
                assert_eq!(
                    (app.editor.session.doc.width, app.editor.session.doc.height),
                    (1200, 900)
                );
            }
        }
        crate::i18n::set_language(language_before);
    }
    #[test]
    fn new_canvas_footer_remains_visible_at_narrow_short_and_wide_sizes() {
        for size in [vec2(460., 380.), vec2(700., 500.), vec2(1700., 1000.)] {
            let mut app = studio();
            app.editor.open_new_canvas();
            let ctx = Context::default();
            for n in 0..3 {
                frame(&mut app, &ctx, size, vec![], n as f64);
            }
            let output = frame(&mut app, &ctx, size, vec![], 4.);
            for text in ["Create Canvas", "Cancel"] {
                let clipped = output
                    .shapes
                    .iter()
                    .find_map(|s| label(&s.shape, text).map(|p| (s.clip_rect, p)))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(clipped.1),
                    "{text} outside {size:?}"
                );
                assert!(clipped.0.contains(clipped.1), "{text} clipped at {size:?}");
            }
        }
    }
}
