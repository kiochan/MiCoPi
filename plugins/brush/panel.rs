//! Feature-owned presentation adapter; document changes use shared actions.
use super::panels::{parameter, section};
use super::*;
#[path = "presets.rs"]
mod presets;
pub(super) const OPACITY_MODES: [crate::application::OpacityMode; 2] = [
    crate::application::OpacityMode::Dab,
    crate::application::OpacityMode::Stroke,
];
pub(super) fn opacity_mode_title(mode: crate::application::OpacityMode) -> &'static str {
    match mode {
        crate::application::OpacityMode::Dab => "Build Up",
        crate::application::OpacityMode::Stroke => "Per Stroke",
    }
}
impl Editor {
    pub(super) fn brush_ui(&mut self, ui: &mut Ui) {
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
                    let (r, response) = ui.allocate_exact_size(
                        vec2(width, a.control_height * 2. + a.item_spacing),
                        Sense::click(),
                    );
                    let colors = theme::Colors::new(&a);
                    let selected = self.session.selected_tool == tool.id;
                    ui.painter().rect_filled(
                        r,
                        a.corner_radius,
                        if selected {
                            colors.panel.lerp_to_gamma(colors.accent, 0.18)
                        } else if response.hovered() {
                            colors.border
                        } else {
                            colors.raised
                        },
                    );
                    // A small ribbon mesh suggests pressure taper without allocating
                    // bitmap previews or invoking the painting DLL during UI frames.
                    let mut preview = egui::Mesh::default();
                    let inset = a.item_spacing + 2.;
                    let span = (r.width() - inset * 2.).max(1.);
                    let amplitude = a.control_height * 0.12;
                    let color = if selected { colors.accent } else { colors.text };
                    for n in 0..=24 {
                        let t = n as f32 / 24.;
                        let angle = t * std::f32::consts::TAU;
                        let center = pos2(
                            r.left() + inset + span * t,
                            r.top() + a.control_height * 0.7 + angle.sin() * amplitude,
                        );
                        let normal = vec2(-angle.cos() * amplitude * std::f32::consts::TAU, span)
                            .normalized();
                        let half = ((radius * 0.22).clamp(0.7, 5.)
                            * (t * std::f32::consts::PI).sin().max(0.).sqrt())
                        .max(0.25);
                        // A one-point transparent fringe provides antialiasing for
                        // thin pen previews as well as broad brush silhouettes.
                        preview.colored_vertex(center + normal * (half + 1.), Color32::TRANSPARENT);
                        preview.colored_vertex(center + normal * half, color);
                        preview.colored_vertex(center - normal * half, color);
                        preview.colored_vertex(center - normal * (half + 1.), Color32::TRANSPARENT);
                        if n > 0 {
                            let i = n * 4;
                            for strip in 0..3 {
                                preview.add_triangle(i - 4 + strip, i - 3 + strip, i + strip);
                                preview.add_triangle(i - 3 + strip, i + strip + 1, i + strip);
                            }
                        }
                    }
                    ui.painter().add(Shape::mesh(preview));
                    let title = WidgetText::from(
                        RichText::new(crate::i18n::translated_for(
                            self.session.registry.contribution_owner("tools", &tool.id),
                            name,
                        ))
                        .size(a.small_font_size)
                        .color(colors.muted),
                    )
                    .into_galley(
                        ui,
                        Some(TextWrapMode::Truncate),
                        (r.width() - 4.).max(0.),
                        TextStyle::Small,
                    );
                    ui.painter().with_clip_rect(r.shrink(2.)).galley(
                        pos2(
                            r.center().x - title.size().x * 0.5,
                            r.bottom() - 13. - title.size().y * 0.5,
                        ),
                        title,
                        colors.muted,
                    );
                    let response = response.on_hover_text(tr_for(
                        self.session.registry.contribution_owner("tools", &tool.id),
                        &tool.title,
                    ));
                    if response.clicked() {
                        self.select_tool(&tool.id);
                    }
                }
            });
        }
        self.user_brush_presets_ui(ui);
        section(ui, "Brush Tip", a.section_spacing);
        let mut diameter = self.radius * 2.;
        parameter(ui, "Size", &mut diameter, 1.0..=400., " px", false, true);
        self.radius = diameter / 2.;
        parameter(ui, "Opacity", &mut self.opacity, 0.0..=1., "%", true, false);
        ui.add_enabled_ui(self.area_mode == 0 && matches!(self.tool, Tool::Brush | Tool::Eraser), |ui| {
        let mut flow = self.session.parameters.flow;
        let flow_response = ui.scope(|ui| parameter(ui, "Flow", &mut flow, 0.0..=1., "%", true, false)).response;
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(Id::new("brush-flow-control"), flow_response.rect));
        flow_response.on_hover_text(tr("How quickly paint builds up as you draw."));
        if flow != self.session.parameters.flow {
            self.core("brush.set", serde_json::json!({"flow":flow}));
        }
        let mut limited = self.session.parameters.opacity_mode == crate::application::OpacityMode::Stroke;
        let response = ui.scope(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
            ui.add(Checkbox::new(&mut limited, tr("Limit opacity per stroke")))
        }).inner;
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(Id::new("brush-stroke-opacity-control"), response.rect));
        if response.changed() {
            self.core("brush.set", serde_json::json!({"opacity_mode":if limited {"stroke"} else {"dab"}}));
        }
        ui.add(Label::new(tr(if self.session.parameters.opacity_mode == crate::application::OpacityMode::Stroke {
            "Keep each stroke within Opacity; lift the pen to build another coat."
        } else {
            "Paint builds up as you pass over the same area."
        }).weak()).wrap());
        }).response.on_disabled_hover_text(tr("Flow and stroke opacity apply to Brush and Eraser."));
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
            self.core(
                "brush.set",
                serde_json::json!({"spacing":crate::application::spacing_json(spacing)}),
            );
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
}

#[cfg(test)]
#[path = "opacity_ui_tests.rs"]
mod opacity_ui_tests;
