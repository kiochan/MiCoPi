//! Feature-owned presentation adapter; document changes use shared actions.
use super::panels::{parameter, section};
use super::*;
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
}
