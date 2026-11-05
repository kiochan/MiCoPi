use super::panels::{icon_button, parameter};
use super::*;
impl Editor {
    fn layer_menu(&self, ui: &mut Ui, index: usize) -> Option<crate::application::Action> {
        use crate::application::Action;
        let mut action = None;
        let id = self.session.doc.layer_id(index);
        let count = self.session.doc.layers.len();
        for (command, title, args, enabled) in [
            (
                "layer.duplicate",
                "Duplicate Layer",
                serde_json::json!({"layer_id":id}),
                count < 16,
            ),
            (
                "layer.move",
                "Move Layer Up",
                serde_json::json!({"layer_id":id,"to":(index+1).min(count-1)}),
                index + 1 < count,
            ),
            (
                "layer.move",
                "Move Layer Down",
                serde_json::json!({"layer_id":id,"to":index.saturating_sub(1)}),
                index > 0,
            ),
            (
                "layer.delete",
                "Delete Layer",
                serde_json::json!({"layer_id":id}),
                count > 1,
            ),
        ] {
            if ui.add_enabled(enabled, Button::new(tr(title))).clicked() {
                action = Some(Action::new(command, args));
                ui.close();
            }
        }
        ui.separator();
        for (command, title, enabled) in [
            ("layer.via_copy", "Layer via Copy", count < 16),
            (
                "layer.via_cut",
                "Layer via Cut",
                count < 16
                    && self.session.doc.selection().current.is_some()
                    && !self.session.doc.alpha_locked(index),
            ),
        ] {
            if ui
                .add_enabled(
                    enabled && self.session.registry.actions.contains_key(command),
                    Button::new(tr(title)),
                )
                .clicked()
            {
                action = Some(Action::new(
                    "pipeline.run",
                    serde_json::json!({"atomic":true,"steps":[
                        {"id":"select","action":"layer.select","args":{"layer_id":id}},
                        {"id":"transfer","action":command,"args":{}}
                    ]}),
                ));
                ui.close();
            }
        }
        ui.separator();
        for (command, title, args, enabled) in [
            (
                "layer.merge_down",
                "Merge Down",
                serde_json::json!({"layer_id":id}),
                index > 0,
            ),
            (
                "layer.merge_visible",
                "Merge Visible",
                serde_json::json!({}),
                self.session.doc.layers.iter().filter(|l| l.visible).count() > 1,
            ),
            (
                "layer.flatten",
                "Flatten Image",
                serde_json::json!({}),
                true,
            ),
            (
                "layer.stamp_visible",
                "Stamp Visible",
                serde_json::json!({}),
                count < 16,
            ),
        ] {
            if ui
                .add_enabled(
                    enabled && self.session.registry.actions.contains_key(command),
                    Button::new(tr(title)),
                )
                .clicked()
            {
                action = Some(Action::new(command, args));
                ui.close();
            }
        }
        ui.separator();
        for (title, color) in [
            ("Fill with Foreground", &self.session.parameters.color),
            (
                "Fill with Background",
                &self.session.parameters.background_color,
            ),
        ] {
            if ui
                .add_enabled(
                    self.session
                        .registry
                        .actions
                        .contains_key("layer.fill_color"),
                    Button::new(tr(title)),
                )
                .clicked()
            {
                action = Some(Action::new(
                    "pipeline.run",
                    serde_json::json!({"atomic":true,"steps":[
                        {"id":"select","action":"layer.select","args":{"layer_id":id}},
                        {"id":"fill","action":"layer.fill_color","args":{"color":color,"opacity":self.opacity}}
                    ]}),
                ));
                ui.close();
            }
        }
        ui.separator();
        for (command, title) in [
            ("layer.flip_horizontal", "Flip Layer Horizontally"),
            ("layer.flip_vertical", "Flip Layer Vertically"),
            ("layer.rotate_180", "Rotate Layer 180°"),
        ] {
            if ui
                .add_enabled(
                    self.session.registry.actions.contains_key(command),
                    Button::new(tr(title)),
                )
                .clicked()
            {
                action = Some(Action::new(
                    "pipeline.run",
                    serde_json::json!({"atomic":true,"steps":[
                        {"id":"select","action":"layer.select","args":{"layer_id":id}},
                        {"id":"transform","action":command}
                    ]}),
                ));
                ui.close();
            }
        }
        ui.menu_button(tr("Offset Layer"), |ui| {
            let key=Id::new(("layer-offset",self.session.doc.identity,id));
            let mut offset=ui.data_mut(|d|d.get_temp::<(i32,i32)>(key)).unwrap_or_default();
            ui.horizontal(|ui| {
                ui.label("X"); ui.add(DragValue::new(&mut offset.0).range(-4096..=4096));
                ui.label("Y"); ui.add(DragValue::new(&mut offset.1).range(-4096..=4096));
            });
            ui.data_mut(|d|d.insert_temp(key,offset));
            if ui.add_enabled(self.session.registry.actions.contains_key("layer.translate"),Button::new(tr("Apply Offset"))).clicked() {
                action=Some(Action::new("pipeline.run",serde_json::json!({"atomic":true,"steps":[
                    {"id":"select","action":"layer.select","args":{"layer_id":id}},
                    {"id":"translate","action":"layer.translate","args":{"dx":offset.0,"dy":offset.1}}
                ]}))); ui.close();
            }
        });
        action
    }
    pub(super) fn layers_ui(&mut self, ui: &mut Ui) {
        let mut pending = Vec::new();
        ui.horizontal(|ui| {
            if icon_button(ui, self, 42, false).clicked() {
                self.actions.push(Action::NewLayer);
            }
            if icon_button(ui, self, 43, false).clicked() {
                self.actions.push(Action::UiCommand(43));
            }
            ui.menu_button(tr("···"), |ui| {
                if let Some(action) = self.layer_menu(ui, self.session.doc.active) {
                    pending.push(action);
                }
            })
            .response
            .on_hover_text(tr("Layer Actions"));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.weak(tr(format!("{} / 16", self.session.doc.layers.len())));
            });
        });
        let index = self.session.doc.active;
        let id = self.session.doc.layer_id(index);
        let edit_id = Id::new(("layer-name", self.session.doc.identity, id));
        let current = if self.session.doc.custom_layer_name(index).is_empty() {
            localize(format!("Layer {:02}", index + 1))
        } else {
            self.session.doc.layer_name(index)
        };
        let mut draft = ui
            .data_mut(|d| d.get_temp::<String>(edit_id))
            .unwrap_or_else(|| current.clone());
        let response = ui.add(
            TextEdit::singleline(&mut draft)
                .id(edit_id)
                .char_limit(120)
                .desired_width(f32::INFINITY)
                .hint_text(crate::i18n::text("Layer Name")),
        );
        if response.changed() {
            ui.data_mut(|d| d.insert_temp(edit_id, draft.clone()));
        }
        if response.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
            response.surrender_focus();
            ui.data_mut(|d| d.remove::<String>(edit_id));
        } else if response.lost_focus()
            || (response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
        {
            if draft != current && !draft.trim().is_empty() {
                pending.insert(
                    0,
                    crate::application::Action::new(
                        "layer.rename",
                        serde_json::json!({"layer_id":id,"name":draft}),
                    ),
                );
            }
            ui.data_mut(|d| d.remove::<String>(edit_id));
        }
        ui.add_space(self.settings.ui.item_spacing);
        let mut locked = self.session.doc.alpha_locked(index);
        if ui
            .checkbox(&mut locked, tr("Lock Layer Transparency"))
            .changed()
        {
            pending.push(crate::application::Action::new(
                "layer.set",
                serde_json::json!({"layer_id":id,"alpha_locked":locked}),
            ));
        }
        ui.horizontal(|ui| {
            ui.weak(tr("Blend Mode"));
            let mut mode = self.session.doc.layers[self.session.doc.active].mode;
            let before = mode;
            ui.add_enabled_ui(
                self.session.doc.graph.linked || !self.session.doc.graph.enabled,
                |ui| {
                    ComboBox::from_id_salt("layer-blend")
                        .selected_text(tr(mode.title()))
                        .width(110.)
                        .show_ui(ui, |ui| {
                            for item in crate::engine::graph::BlendMode::ALL {
                                ui.selectable_value(&mut mode, item, tr(item.title()));
                            }
                        });
                },
            );
            if mode != before {
                self.core(
                    "layer.set",
                    serde_json::json!({"index":self.session.doc.active,"blend_mode":mode}),
                );
            }
        });
        if ui
            .add(
                Button::new(tr(RichText::new(localize("Node Editor…"))
                    .color(theme::Colors::new(&self.settings.ui).accent_text)))
                .frame(false),
            )
            .clicked()
        {
            self.core("graph.open", serde_json::json!({}));
        }
        if self.session.doc.graph.enabled && !self.session.doc.graph.linked {
            ui.weak(tr(
                "Node Graph Controls Compositing; Set Modes on Blend Nodes",
            ));
            if !self.session.doc.layer_is_rendered(self.session.doc.active) {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Current Layer Is Not Connected to the Output"),
                );
            }
        }
        let mut opacity = self.session.doc.layers[self.session.doc.active].opacity as f32 / 255.;
        let previous = opacity;
        parameter(ui, "Opacity", &mut opacity, 0.0..=1., "%", true, false);
        if (opacity - previous).abs() > 0.0001 {
            self.core(
                "layer.set",
                serde_json::json!({"index":self.session.doc.active,"opacity":opacity}),
            );
        }
        ui.add_space(self.settings.ui.item_spacing);
        let colors = theme::Colors::new(&self.settings.ui);
        let row_height = self.settings.ui.control_height + self.settings.ui.panel_padding * 2.;
        for index in (0..self.session.doc.layers.len()).rev() {
            let (r, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), row_height), Sense::click());
            ui.painter().rect_filled(
                r,
                self.settings.ui.corner_radius,
                if index == self.session.doc.active {
                    colors.tint(if colors.dark { 0.16 } else { 0.13 })
                } else if response.hovered() {
                    colors.raised
                } else {
                    colors.panel
                },
            );
            if index == self.session.doc.active {
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        pos2(r.left() + 1.5, r.center().y),
                        vec2(3., row_height * 0.5),
                    ),
                    1.5,
                    colors.accent,
                );
            }
            let eye =
                Rect::from_min_size(r.min + vec2(4., (row_height - 24.) / 2.), Vec2::splat(24.));
            let toggle = ui.interact(
                eye,
                ui.id().with(("eye", self.session.doc.layer_id(index))),
                Sense::click(),
            );
            let visible = self.session.doc.layers[index].visible;
            ui.painter().circle_stroke(
                eye.center(),
                4.,
                Stroke::new(1., if visible { colors.accent } else { colors.muted }),
            );
            if visible {
                ui.painter().circle_filled(eye.center(), 2., colors.accent);
            }
            if toggle.clicked() {
                self.core(
                    "layer.set",
                    serde_json::json!({"index":index,"visible":!visible}),
                );
            }
            let thumb = Rect::from_center_size(pos2(r.left() + 49., r.center().y), vec2(32., 32.));
            ui.painter()
                .rect_filled(thumb.expand(1.), 5., colors.border);
            ui.painter().rect_filled(thumb, 4., Color32::from_gray(248));
            // A 16×16 sampled mesh keeps thumbnails cheap even on 4096px documents.
            let mut mesh = egui::Mesh::default();
            for y in 0..16 {
                for x in 0..16 {
                    let px = x * self.session.doc.width / 16;
                    let py = y * self.session.doc.height / 16;
                    let p = self.session.doc.layers[index].pixels[py * self.session.doc.width + px];
                    if p >> 24 != 0 {
                        let tile = Rect::from_min_size(
                            thumb.min + vec2(x as f32 * 2., y as f32 * 2.),
                            Vec2::splat(2.),
                        );
                        mesh.add_colored_rect(
                            tile,
                            Color32::from_rgba_unmultiplied(
                                (p >> 16) as u8,
                                (p >> 8) as u8,
                                p as u8,
                                (p >> 24) as u8,
                            ),
                        );
                    }
                }
            }
            ui.painter().add(Shape::mesh(mesh));
            ui.painter().text(
                pos2(r.left() + 76., r.center().y - 9.),
                Align2::LEFT_CENTER,
                if self.session.doc.custom_layer_name(index).is_empty() {
                    localize(format!("Layer {:02}", index + 1))
                } else {
                    self.session.doc.layer_name(index)
                },
                if index == self.session.doc.active {
                    theme::strong(ui.ctx(), self.settings.ui.font_size)
                } else {
                    FontId::proportional(self.settings.ui.font_size)
                },
                colors.text,
            );
            ui.painter().text(
                pos2(r.left() + 76., r.center().y + 11.),
                Align2::LEFT_CENTER,
                format!(
                    "{} · {:.0}%",
                    crate::i18n::text(self.session.doc.layers[index].mode.title()),
                    self.session.doc.layers[index].opacity as f32 / 255. * 100.
                ),
                FontId::proportional(self.settings.ui.small_font_size),
                colors.muted,
            );
            if response.clicked() && !toggle.clicked() {
                self.finish_stroke();
                self.core("layer.select", serde_json::json!({"index":index}));
            }
            response.context_menu(|ui| {
                if let Some(action) = self.layer_menu(ui, index) {
                    pending.push(action);
                }
            });
        }
        for action in pending {
            self.finish_stroke();
            self.core(&action.action, action.args);
        }
    }
}
