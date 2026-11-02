use super::*;
use egui_dock::TabViewer;
pub(super) struct Viewer<'a> {
    pub editor: &'a mut Editor,
}
impl TabViewer for Viewer<'_> {
    type Tab = Panel;
    fn title(&mut self, p: &mut Panel) -> WidgetText {
        if let Panel::Plugin(key) = p {
            return self
                .editor
                .session
                .registry
                .panels
                .values()
                .find(|d| crate::plugins::panel_key(&d.id) == *key)
                .map(|d| localize(d.title.clone()))
                .unwrap_or_else(|| "Plugin Not Loaded".into())
                .into();
        }
        if *p == Panel::Canvas {
            let name = self
                .editor
                .session
                .file
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| crate::i18n::text("Untitled").into());
            format!(
                "{}{}",
                if self.editor.session.doc.dirty {
                    "●  "
                } else {
                    ""
                },
                name
            )
            .into()
        } else {
            let builtin = match p {
                Panel::Color => "color",
                Panel::Brush => "brush",
                Panel::Navigator => "navigator",
                Panel::Layers => "layers",
                Panel::Nodes => "nodes",
                _ => "",
            };
            self.editor
                .session
                .registry
                .panels
                .values()
                .find(|d| d.builtin.as_deref() == Some(builtin))
                .map(|d| localize(d.title.clone()))
                .unwrap_or_else(|| p.title().into())
                .into()
        }
    }
    fn id(&mut self, p: &mut Panel) -> Id {
        Id::new(("panel", *p))
    }
    fn is_closeable(&self, p: &Panel) -> bool {
        *p != Panel::Canvas
    }
    fn allowed_in_windows(&self, p: &mut Panel) -> bool {
        *p != Panel::Canvas
    }
    fn scroll_bars(&self, _: &Panel) -> [bool; 2] {
        [false, false]
    }
    fn on_tab_button(&mut self, p: &mut Panel, r: &Response) {
        if *p != Panel::Canvas {
            r.clone().on_hover_text(tr(
                "Drag Tabs to Split, Group or Float Panels; Right-click for Actions",
            ));
        }
    }
    fn tab_style_override(
        &self,
        p: &Panel,
        global: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        let mut style = global.clone();
        if *p == Panel::Canvas {
            style.tab_body.inner_margin = Margin::ZERO;
        }
        Some(style)
    }
    fn ui(&mut self, ui: &mut Ui, p: &mut Panel) {
        if *p == Panel::Canvas {
            self.editor.canvas_ui(ui);
            return;
        }
        if *p == Panel::Nodes {
            self.editor.nodes_ui(ui);
            return;
        }
        ui.set_min_width(176.);
        ui.spacing_mut().slider_width = (ui.available_width() * 0.35).clamp(52., 120.);
        if *p == Panel::Navigator {
            self.editor.navigator_ui(ui);
            return;
        }
        ScrollArea::vertical()
            .id_salt(*p)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .auto_shrink([false, false])
            .show(ui, |ui| match p {
                Panel::Color => self.editor.color_ui(ui),
                Panel::Brush => self.editor.brush_ui(ui),
                Panel::Navigator => self.editor.navigator_ui(ui),
                Panel::Layers => self.editor.layers_ui(ui),
                Panel::Plugin(key) => self.editor.plugin_panel(ui, *key),
                _ => {}
            });
    }
}
fn section(ui: &mut Ui, title: &str, gap: f32) {
    ui.add_space(gap * 0.5);
    ui.separator();
    ui.add_space(gap * 0.25);
    ui.label(tr(RichText::new(localize(title)).small().strong()));
}
pub(super) fn icon_button(ui: &mut Ui, editor: &Editor, id: u32, selected: bool) -> Response {
    let a = &editor.settings.ui;
    let response = ui.add_sized(
        [a.control_height + 4., a.control_height],
        Button::new(tr(theme::icon(theme::action_icon(id), a.icon_size))).selected(selected),
    );
    let hint = editor.hint(id);
    let keys = editor.label(id);
    response.on_hover_ui(|ui| {
        ui.label(tr(hint.split("  ").next().unwrap_or(&hint)));
        theme::keycaps(ui, &keys);
    })
}
impl Editor {
    fn plugin_panel(&mut self, ui: &mut Ui, key: u32) {
        let definition = self
            .session
            .registry
            .panels
            .values()
            .find(|d| crate::plugins::panel_key(&d.id) == key)
            .cloned();
        if let Some(panel) = definition {
            for button in panel.buttons {
                if ui
                    .add_sized(
                        [ui.available_width(), self.settings.ui.control_height],
                        Button::new(tr(&button.title)),
                    )
                    .clicked()
                {
                    match self.session.apply(button.action) {
                        Ok(_) => self.sync_from_session(),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            }
        } else {
            ui.weak(tr("Plugin Disabled or Unavailable"));
        }
    }
    fn color_ui(&mut self, ui: &mut Ui) {
        let gap = self.settings.ui.section_spacing;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(38., 30.), Sense::hover());
            ui.painter().rect_filled(
                r.translate(vec2(7., 4.)),
                4.,
                Color32::from_rgb(
                    self.background_ink[0],
                    self.background_ink[1],
                    self.background_ink[2],
                ),
            );
            ui.painter().rect_stroke(
                r.translate(vec2(7., 4.)),
                4.,
                Stroke::new(1., ui.visuals().widgets.noninteractive.bg_stroke.color),
                StrokeKind::Inside,
            );
            ui.painter().rect_filled(
                r,
                4.,
                Color32::from_rgb(self.ink[0], self.ink[1], self.ink[2]),
            );
            ui.add_space(8.);
            ui.label(tr(RichText::new(localize(format!(
                "#{:02X}{:02X}{:02X}",
                self.ink[0], self.ink[1], self.ink[2]
            )))
            .monospace()
            .small()));
        });
        ui.add_space(gap * 0.5);
        self.color_wheel(ui);
        section(ui, "Swatches", gap);
        let colors = [
            0x252830, 0xffffff, 0x929aa8, 0xefe3cf, 0xe66b72, 0xed9b62, 0xe8ca76, 0xb1c978,
            0x75b89b, 0x71bcc5, 0x7699d4, 0x9990d3, 0xc68cc2, 0x8c5854, 0x6d7b62, 0x465e77,
        ];
        let side = ((ui.available_width() - 7. * 4.) / 8.).max(14.);
        ui.spacing_mut().item_spacing = vec2(4., 4.);
        for row in colors.chunks(8) {
            ui.horizontal(|ui| {
                for &c in row {
                    let (r, response) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
                    ui.painter().rect_filled(r, 3., theme::rgb(c));
                    if response.hovered() {
                        ui.painter().rect_stroke(
                            r,
                            3.,
                            Stroke::new(1.5, ui.visuals().text_color()),
                            StrokeKind::Inside,
                        );
                    }
                    if response.clicked() {
                        self.ink = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
                    }
                    response.on_hover_text(tr(format!("#{c:06X}")));
                }
            });
        }
        CollapsingHeader::new(tr("RGB Values"))
            .id_salt("rgb-channels")
            .show(ui, |ui| {
                for (i, name) in ["R", "G", "B"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(tr(name));
                        ui.add(Slider::new(&mut self.ink[i], 0..=255));
                    });
                }
            });
    }
    fn color_wheel(&mut self, ui: &mut Ui) {
        use crate::engine::color::{hsv, square, to_hsv, wheel};
        let color = u32::from_be_bytes([0, self.ink[0], self.ink[1], self.ink[2]]);
        let (h, mut saturation, mut value) = to_hsv(color);
        if self.color_last != self.ink && saturation > 0.001 {
            self.color_hue = h;
        }
        let size = ui.available_width().min(160.).max(128.).round() as usize;
        let (slot, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), size as f32), Sense::hover());
        let r = Rect::from_center_size(slot.center(), Vec2::splat(size as f32));
        let response = ui.interact(r, ui.id().with("hsv-wheel"), Sense::click_and_drag());
        let (offset, side) = square(size as i32);
        if let Some(p) = response
            .interact_pointer_pos()
            .filter(|_| response.is_pointer_button_down_on() || response.clicked())
        {
            let d = p - r.center();
            let ring = *self
                .wheel_drag
                .get_or_insert(d.length() > size as f32 * 0.5 - 20.);
            if ring {
                self.color_hue = (d.y.atan2(d.x) / std::f32::consts::TAU).rem_euclid(1.);
            } else {
                saturation = ((p.x - r.left() - offset as f32) / (side - 1) as f32).clamp(0., 1.);
                value = 1. - ((p.y - r.top() - offset as f32) / (side - 1) as f32).clamp(0., 1.);
            }
            let rgb = hsv(self.color_hue, saturation, value);
            self.ink = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8];
        }
        if !response.is_pointer_button_down_on() {
            self.wheel_drag = None;
        }
        self.color_last = self.ink;
        let bg = theme::Colors::new(&self.settings.ui).panel;
        let background = u32::from_be_bytes([0, bg.r(), bg.g(), bg.b()]);
        let key = (size, (self.color_hue * 65535.) as u32, background);
        if self.wheel_key != Some(key) {
            let mut pixels = vec![];
            wheel(&mut pixels, size, self.color_hue, background);
            let image = ColorImage::new(
                [size, size],
                pixels.into_iter().map(|p| theme::rgb(p)).collect(),
            );
            if let Some(texture) = &mut self.wheel_texture {
                texture.set(image, TextureOptions::LINEAR);
            } else {
                self.wheel_texture = Some(ui.ctx().load_texture(
                    "color-wheel",
                    image,
                    TextureOptions::LINEAR,
                ));
            }
            self.wheel_key = Some(key);
        }
        ui.painter().image(
            self.wheel_texture.as_ref().unwrap().id(),
            r,
            Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
            Color32::WHITE,
        );
        let angle = self.color_hue * std::f32::consts::TAU;
        let hue_point = r.center() + vec2(angle.cos(), angle.sin()) * (size as f32 * 0.5 - 9.);
        let sv_point = r.min
            + vec2(
                offset as f32 + saturation * (side - 1) as f32,
                offset as f32 + (1. - value) * (side - 1) as f32,
            );
        for p in [hue_point, sv_point] {
            ui.painter()
                .circle_stroke(p, 4., Stroke::new(2., Color32::WHITE));
            ui.painter()
                .circle_stroke(p, 5., Stroke::new(1., Color32::BLACK));
        }
    }
    fn brush_ui(&mut self, ui: &mut Ui) {
        self.area_ui(ui);
        let a = self.settings.ui.clone();
        let presets: Vec<_> = self
            .session
            .registry
            .tools
            .values()
            .filter(|t| t.preset)
            .cloned()
            .collect();
        let columns = if ui.available_width() >= (a.font_size * 3. + 8.) * 3. + a.item_spacing * 2.
        {
            3
        } else {
            2
        };
        let width = ((ui.available_width() - a.item_spacing * (columns - 1) as f32)
            / columns as f32)
            .max(40.);
        for row in presets.chunks(columns) {
            ui.horizontal(|ui| {
                for tool in row {
                    let name = tool.title.strip_suffix(" Preset").unwrap_or(&tool.title);
                    let radius = tool.parameters["radius"].as_f64().unwrap_or(6.) as f32;
                    let (r, response) = ui.allocate_exact_size(vec2(width, 56.), Sense::click());
                    let colors = theme::Colors::new(&a);
                    ui.painter().rect_filled(
                        r,
                        a.corner_radius,
                        if response.hovered() {
                            colors.border
                        } else {
                            colors.raised
                        },
                    );
                    let points = (0..20)
                        .map(|n| {
                            let t = n as f32 / 19.;
                            pos2(
                                r.left() + 8. + (r.width() - 16.) * t,
                                r.top() + 17. + (t * std::f32::consts::TAU).sin() * 2.5,
                            )
                        })
                        .collect();
                    ui.painter().add(egui::Shape::line(
                        points,
                        Stroke::new((radius * 0.2_f32).clamp(1., 5.), colors.text),
                    ));
                    ui.painter().with_clip_rect(r.shrink(2.)).text(
                        pos2(r.center().x, r.bottom() - 13.),
                        Align2::CENTER_CENTER,
                        localize(name),
                        FontId::proportional(a.small_font_size),
                        colors.muted,
                    );
                    let response = response.on_hover_text(tr(&tool.title));
                    if response.clicked() {
                        self.select_tool(&tool.id);
                    }
                }
            });
        }
        section(ui, "Brush Tip", a.section_spacing);
        let mut diameter = self.radius * 2.;
        parameter(ui, "Size", &mut diameter, 1.0..=400., " px", false, true);
        self.radius = diameter / 2.;
        parameter(ui, "Opacity", &mut self.opacity, 0.0..=1., "%", true, false);
        parameter(
            ui,
            "Hardness",
            &mut self.hardness,
            0.0..=1.,
            "%",
            true,
            false,
        );
        section(ui, "Stroke", a.section_spacing);
        let mut spacing = self.session.parameters.spacing;
        parameter(ui, "Dab Spacing", &mut spacing, 0.01..=2., "%", true, false);
        if spacing != self.session.parameters.spacing {
            self.core("brush.set", serde_json::json!({"spacing":spacing}));
        }
        ui.collapsing(tr("Pressure Dynamics"), |ui| {
            ui.checkbox(&mut self.workspace.dynamics.size, tr("Pressure Size"));
            ui.checkbox(&mut self.workspace.dynamics.opacity, tr("Pressure Opacity"));
            let mut minimum = self.session.parameters.minimum_opacity;
            parameter(
                ui,
                "Minimum Opacity",
                &mut minimum,
                0.0..=1.,
                "%",
                true,
                false,
            );
            if minimum != self.session.parameters.minimum_opacity {
                self.core("brush.set", serde_json::json!({"minimum_opacity":minimum}));
            }
            parameter(
                ui,
                "Minimum Size",
                &mut self.workspace.dynamics.minimum,
                0.0..=1.,
                "%",
                true,
                false,
            );
            parameter(
                ui,
                "Pressure Curve",
                &mut self.workspace.dynamics.gamma,
                0.25..=4.,
                "",
                false,
                false,
            );
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.), Sense::hover());
            let colors = theme::Colors::new(&self.settings.ui);
            let points = (0..=32)
                .map(|i| {
                    let x = i as f32 / 32.;
                    let y = x.powf(self.workspace.dynamics.gamma);
                    pos2(
                        rect.left() + x * rect.width(),
                        rect.bottom() - y * rect.height(),
                    )
                })
                .collect();
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_top()],
                Stroke::new(1., colors.muted),
            );
            ui.painter()
                .add(Shape::line(points, Stroke::new(2., colors.accent)));
        });
        parameter(
            ui,
            "Stabilizer",
            &mut self.workspace.stabilizer,
            0.0..=1.,
            "%",
            true,
            false,
        );
        if ui.button(tr("Pen and Stabilizer Settings…")).clicked() {
            self.core(
                "ui.dispatch",
                serde_json::json!({"id":"settings.search","value":"painting"}),
            );
        }
        section(ui, "Quick Sizes", a.section_spacing);
        ui.horizontal_wrapped(|ui| {
            for n in [1., 2., 3., 5., 8., 12., 20., 35., 60., 100.] {
                if ui
                    .add_sized(
                        [32., a.control_height],
                        Button::new(tr(format!("{n:.0}")))
                            .selected((self.radius * 2. - n).abs() < 0.1),
                    )
                    .clicked()
                {
                    self.radius = n / 2.;
                }
            }
        });
    }
    fn navigator_ui(&mut self, ui: &mut Ui) {
        self.refresh_texture(ui.ctx());
        let width = ui.available_width();
        let reserve = self.settings.ui.control_height
            + ui.text_style_height(&TextStyle::Body)
            + self.settings.ui.item_spacing * 4.;
        let height = (width * self.session.doc.height as f32 / self.session.doc.width as f32)
            .min(150.)
            .min((ui.available_height() - reserve).max(40.));
        let (r, response) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
        let inner = Rect::from_center_size(
            r.center(),
            vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * ((r.width() - 4.) / self.session.doc.width as f32)
                .min((r.height() - 4.) / self.session.doc.height as f32),
        );
        ui.painter()
            .rect_filled(r, 4., theme::Colors::new(&self.settings.ui).background);
        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                inner,
                Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                Color32::WHITE,
            );
        }
        let scale = inner.width() / self.session.doc.width as f32;
        if self.canvas.is_positive() {
            let min = self.document_position(self.canvas.min);
            let max = self.document_position(self.canvas.max);
            let view = Rect::from_min_max(
                inner.min + min.to_vec2() * scale,
                inner.min + max.to_vec2() * scale,
            )
            .intersect(inner);
            ui.painter().rect_stroke(
                view,
                0.,
                Stroke::new(1., theme::Colors::new(&self.settings.ui).accent),
                StrokeKind::Inside,
            );
        }
        if let Some(p) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked() || response.dragged())
        {
            let q = (p - inner.min) / scale;
            self.fit = false;
            self.pan = (vec2(
                self.session.doc.width as f32,
                self.session.doc.height as f32,
            ) * 0.5
                - q)
                * self.zoom;
        }
        ui.add_space(self.settings.ui.item_spacing);
        ui.horizontal(|ui| {
            for id in [86, 40, 85] {
                if icon_button(ui, self, id, false).clicked() {
                    self.actions.push(match id {
                        86 => Action::ZoomOut,
                        40 => Action::Fit,
                        _ => Action::ZoomIn,
                    });
                }
            }
            ui.label(tr(RichText::new(localize(format!(
                "{:.1}%",
                self.zoom * 100.
            )))
            .small()));
        });
        ui.weak(tr(format!(
            "{} × {} px",
            self.session.doc.width, self.session.doc.height
        )));
    }
}

pub(super) fn parameter(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    suffix: &str,
    percent: bool,
    logarithmic: bool,
) {
    let width = ui.available_width();
    let gap = ui.spacing().item_spacing.x;
    let label_width = ui
        .painter()
        .layout_no_wrap(
            crate::i18n::translated(label).into_owned(),
            TextStyle::Small.resolve(ui.style()),
            ui.visuals().text_color(),
        )
        .size()
        .x
        .max(ui.text_style_height(&TextStyle::Body) * 3.)
        + 4.;
    let number_width = if percent { 48. } else { 58. };
    let narrow = width < label_width + number_width + gap * 2. + 60.;
    let mut display = if percent { *value * 100. } else { *value };
    let display_range = if percent {
        *range.start() * 100. ..=*range.end() * 100.
    } else {
        range.clone()
    };
    ui.horizontal(|ui| {
        ui.add_sized(
            [
                if narrow {
                    width - number_width - gap
                } else {
                    label_width
                },
                ui.spacing().interact_size.y,
            ],
            Label::new(tr(RichText::new(localize(label)).small())).truncate(),
        );
        if !narrow {
            ui.spacing_mut().slider_width =
                (width - label_width - number_width - gap * 2.).max(24.);
            let changed = ui
                .add(
                    Slider::new(value, range.clone())
                        .show_value(false)
                        .logarithmic(logarithmic),
                )
                .changed();
            if changed {
                display = if percent { *value * 100. } else { *value };
            }
        }
        if ui
            .add_sized(
                [number_width, ui.spacing().interact_size.y],
                DragValue::new(&mut display)
                    .range(display_range)
                    .speed(if percent { 1. } else { 0.5 })
                    .max_decimals(1)
                    .suffix(suffix),
            )
            .changed()
        {
            *value = if percent { display / 100. } else { display };
        }
    });
    if narrow {
        ui.scope(|ui| {
            ui.spacing_mut().slider_width = width;
            ui.add(
                Slider::new(value, range)
                    .show_value(false)
                    .logarithmic(logarithmic),
            );
        });
    }
}
