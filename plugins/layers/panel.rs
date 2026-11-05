use super::panels::{icon_button, parameter};
use super::*;
use crate::engine::document::LayerColor;
use serde_json::{json, Value};
use std::{hash::Hasher, sync::Arc};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum LockFilter {
    #[default]
    Any,
    Locked,
    Unlocked,
    Pixels,
    Position,
    Transparency,
}
impl LockFilter {
    const ALL: [Self; 6] = [
        Self::Any,
        Self::Locked,
        Self::Unlocked,
        Self::Pixels,
        Self::Position,
        Self::Transparency,
    ];
    fn title(self) -> &'static str {
        match self {
            Self::Any => "Any Lock",
            Self::Locked => "Locked Layers",
            Self::Unlocked => "Unlocked Layers",
            Self::Pixels => "Pixel Locked",
            Self::Position => "Position Locked",
            Self::Transparency => "Transparency Locked",
        }
    }
}
#[derive(Clone, Default, PartialEq, Eq)]
struct LayerFilter {
    query: String,
    color: Option<LayerColor>,
    visible: Option<bool>,
    selected: Option<bool>,
    lock: LockFilter,
}
impl LayerFilter {
    fn key(identity: u64) -> Id {
        Id::new(("layer-filter", identity))
    }
    fn active(&self) -> bool {
        !self.query.is_empty()
            || self.color.is_some()
            || self.visible.is_some()
            || self.selected.is_some()
            || self.lock != LockFilter::Any
    }
    fn args(&self) -> Value {
        let mut args = json!({});
        if !self.query.is_empty() {
            args["query"] = json!(self.query);
        }
        if let Some(color) = self.color {
            args["color_label"] = json!(color);
        }
        if let Some(visible) = self.visible {
            args["visible"] = json!(visible);
        }
        if let Some(selected) = self.selected {
            args["selected"] = json!(selected);
        }
        match self.lock {
            LockFilter::Any => {}
            LockFilter::Locked => args["locked"] = json!(true),
            LockFilter::Unlocked => args["locked"] = json!(false),
            LockFilter::Pixels => args["pixel_locked"] = json!(true),
            LockFilter::Position => args["position_locked"] = json!(true),
            LockFilter::Transparency => args["alpha_locked"] = json!(true),
        }
        args
    }
}
#[derive(Clone)]
struct LayerQueryCache {
    filter: LayerFilter,
    metadata: u64,
    rows: Result<Arc<[usize]>, String>,
}
fn layer_fingerprint(editor: &Editor) -> u64 {
    let doc = &editor.session.doc;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    hash.write_u64(doc.identity);
    hash.write_usize(doc.active);
    hash.write_usize(doc.layers.len());
    for (i, layer) in doc.layers.iter().enumerate() {
        hash.write_u64(doc.layer_id(i));
        let name = doc.custom_layer_name(i).as_bytes();
        hash.write_usize(name.len());
        hash.write(name);
        for value in [
            layer.visible as u8,
            layer.opacity,
            layer.mode as u8,
            doc.alpha_locked(i) as u8,
            doc.pixel_locked(i) as u8,
            doc.position_locked(i) as u8,
            doc.color_label(i) as u8,
            doc.layer_selected(i) as u8,
        ] {
            hash.write_u8(value);
        }
    }
    hash.finish()
}
#[derive(Clone)]
struct LayerAnchor {
    id: u64,
    active: u64,
    selected: Vec<u64>,
}
fn layer_anchor_key(identity: u64) -> Id {
    Id::new(("layer-selection-anchor", identity))
}
fn batch_action(command: &str, ids: &[u64], mut args: Value) -> crate::application::Action {
    if let Some(args) = args.as_object_mut() {
        args.remove("layer_id");
        args.remove("index");
    }
    crate::application::Action::new(
        "layer.batch",
        json!({"action":command,"layer_ids":ids,"args":args}),
    )
}
fn property_action(ids: &[u64], mut args: Value) -> Option<crate::application::Action> {
    if args.as_object().is_none_or(|args| args.is_empty()) {
        return None;
    }
    Some(if ids.len() > 1 {
        batch_action("layer.set", ids, args)
    } else {
        args["layer_id"] = json!(ids[0]);
        crate::application::Action::new("layer.set", args)
    })
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct LayerProperties {
    alpha: bool,
    pixels: bool,
    position: bool,
    color: LayerColor,
}
impl LayerProperties {
    fn read(editor: &Editor, index: usize) -> Self {
        let doc = &editor.session.doc;
        Self {
            alpha: doc.alpha_locked(index),
            pixels: doc.pixel_locked(index),
            position: doc.position_locked(index),
            color: doc.color_label(index),
        }
    }
    #[cfg(test)]
    fn action(self, before: Self, id: u64) -> Option<crate::application::Action> {
        (self != before).then(|| crate::application::Action::new("layer.set", json!({"layer_id":id,
            "alpha_locked":self.alpha,"pixel_locked":self.pixels,"position_locked":self.position,"color_label":self.color})))
    }
}
fn label_color(color: LayerColor) -> Option<Color32> {
    Some(theme::rgb(match color {
        LayerColor::None => return None,
        LayerColor::Red => 0xe996a0,
        LayerColor::Orange => 0xe5b186,
        LayerColor::Yellow => 0xd9cc85,
        LayerColor::Green => 0x9ac8aa,
        LayerColor::Blue => 0x94b9e6,
        LayerColor::Violet => 0xb6a2dc,
        LayerColor::Gray => 0xa3a0aa,
    }))
}
fn layer_icon_side(a: &crate::configuration::Appearance) -> f32 {
    a.control_height.max(a.icon_size + a.panel_padding * 1.5)
}
fn layer_icon_button(
    a: &crate::configuration::Appearance,
    icon: char,
    selected: bool,
) -> Button<'static> {
    Button::new(theme::icon(icon, a.icon_size))
        .min_size(Vec2::splat(layer_icon_side(a)))
        .selected(selected)
        .frame_when_inactive(selected)
}
fn color_label_text(ui: &Ui, color: LayerColor) -> RichText {
    let text = ui.visuals().text_color();
    let tint = label_color(color)
        .map(|color| {
            if ui.visuals().dark_mode {
                color
            } else {
                color.lerp_to_gamma(text, 0.55)
            }
        })
        .unwrap_or(text);
    RichText::new(localize(color.title())).color(tint)
}
fn layer_text(ui: &Ui, text: String, rect: Rect, font: FontId, color: Color32) {
    if rect.width() <= 0. {
        return;
    }
    let galley = WidgetText::from(RichText::new(text).font(font)).into_galley(
        ui,
        Some(TextWrapMode::Truncate),
        rect.width(),
        TextStyle::Body,
    );
    ui.painter()
        .with_clip_rect(ui.clip_rect().intersect(rect))
        .galley(
            pos2(rect.left(), rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
}
impl Editor {
    fn layer_row_selection(
        &mut self,
        ctx: &Context,
        clicked: u64,
        visible: &[usize],
        modifiers: Modifiers,
    ) -> bool {
        let selected = self.session.doc.selected_layer_ids();
        let active = self.session.doc.layer_id(self.session.doc.active);
        let key = layer_anchor_key(self.session.doc.identity);
        let prior = ctx.data_mut(|data| data.get_temp::<LayerAnchor>(key));
        let visible_ids: Vec<_> = visible
            .iter()
            .map(|&i| self.session.doc.layer_id(i))
            .collect();
        let anchor = prior
            .filter(|anchor| {
                anchor.selected == selected
                    && anchor.active == active
                    && visible_ids.contains(&anchor.id)
            })
            .map(|anchor| anchor.id)
            .unwrap_or_else(|| {
                if visible_ids.contains(&active) {
                    active
                } else {
                    clicked
                }
            });
        let mut ids = selected.clone();
        let mut next_active = clicked;
        let mut next_anchor = clicked;
        if modifiers.shift {
            let Some(start) = visible_ids.iter().position(|id| *id == anchor) else {
                return false;
            };
            let Some(end) = visible_ids.iter().position(|id| *id == clicked) else {
                return false;
            };
            if !modifiers.ctrl {
                ids.clear();
            }
            for &id in &visible_ids[start.min(end)..=start.max(end)] {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            next_anchor = anchor;
        } else if modifiers.ctrl {
            if let Some(position) = ids.iter().position(|id| *id == clicked) {
                if ids.len() == 1 {
                    return true;
                }
                ids.remove(position);
                next_active = if active != clicked {
                    active
                } else {
                    let clicked_index = self.session.doc.layer_index(clicked).unwrap();
                    *ids.iter()
                        .min_by_key(|id| {
                            let i = self.session.doc.layer_index(**id).unwrap();
                            (i.abs_diff(clicked_index), i)
                        })
                        .unwrap()
                };
                next_anchor = next_active;
            } else {
                ids.push(clicked);
            }
        } else {
            ids = vec![clicked];
        }
        if self.input_owned || self.drawing || self.panning.is_some() {
            self.finish_stroke();
        }
        let revision = self.session.revision;
        let success = self.core_at_revision(
            "layer.selection.set",
            json!({"layer_ids":ids,"active_layer_id":next_active}),
            Some(revision),
        );
        if success {
            ctx.data_mut(|data| {
                data.insert_temp(
                    key,
                    LayerAnchor {
                        id: next_anchor,
                        active: self.session.doc.layer_id(self.session.doc.active),
                        selected: self.session.doc.selected_layer_ids(),
                    },
                )
            });
        }
        success
    }
    fn dispatch_layer_pending(&mut self, pending: Vec<crate::application::Action>) {
        for mut action in pending {
            if self.input_owned || self.drawing || self.panning.is_some() {
                self.finish_stroke();
            }
            // A pending active-name commit may precede this explicit batch.
            // Capture its guard at dispatch, after the preceding local action.
            if action.action == "layer.batch" && action.expected_revision.is_none() {
                action.expected_revision = Some(self.session.revision);
            }
            if !self.core_at_revision(&action.action, action.args, action.expected_revision) {
                break;
            }
        }
    }
    fn filtered_layer_indices(&mut self, filter: &LayerFilter) -> Result<Vec<usize>, String> {
        let response = self
            .session
            .apply(crate::application::Action::new(
                "layer.query",
                filter.args(),
            ))
            .map_err(|error| error.message)?;
        let rows = response["result"]["layers"]
            .as_array()
            .ok_or("Invalid layer query result")?;
        rows.iter()
            .map(|row| {
                let index = row["index"].as_u64().ok_or("Invalid layer query result")? as usize;
                if index >= self.session.doc.layers.len()
                    || row["id"].as_u64() != Some(self.session.doc.layer_id(index))
                {
                    return Err("Invalid layer query result".into());
                }
                Ok(index)
            })
            .collect()
    }
    fn cached_layer_indices(
        &mut self,
        ctx: &Context,
        filter: &LayerFilter,
    ) -> Result<Arc<[usize]>, String> {
        let key = Id::new(("layer-query-cache", self.session.doc.identity));
        let metadata = layer_fingerprint(self);
        if let Some(cache) = ctx.data_mut(|d| d.get_temp::<Arc<LayerQueryCache>>(key)) {
            if cache.filter == *filter && cache.metadata == metadata {
                return cache.rows.clone();
            }
        }
        let rows = self.filtered_layer_indices(filter).map(Arc::from);
        ctx.data_mut(|d| {
            d.insert_temp(
                key,
                Arc::new(LayerQueryCache {
                    filter: filter.clone(),
                    metadata,
                    rows: rows.clone(),
                }),
            )
        });
        rows
    }
    fn layer_filters_ui(&mut self, ui: &mut Ui) -> Arc<[usize]> {
        let key = LayerFilter::key(self.session.doc.identity);
        let mut filter = ui
            .data_mut(|d| d.get_temp::<LayerFilter>(key))
            .unwrap_or_default();
        let a = &self.settings.ui;
        ui.horizontal(|ui| {
            let filter_width = layer_icon_side(a);
            let width =
                (ui.available_width() - filter_width - a.item_spacing).max(a.control_height);
            ui.add_sized(
                [width, a.control_height],
                TextEdit::singleline(&mut filter.query)
                    .id(ui.id().with("layer-search"))
                    .char_limit(512)
                    .desired_width(width)
                    .hint_text(crate::i18n::text("Find Layers")),
            );
            egui::containers::menu::MenuButton::from_button(layer_icon_button(
                a,
                '\u{e152}',
                filter.active(),
            ))
            .ui(ui, |ui| {
                ui.label(tr("Filter Layers"));
                ui.add_space(a.item_spacing);
                ComboBox::from_id_salt("layer-filter-color")
                    .selected_text(
                        filter
                            .color
                            .map(|c| localize(c.title()))
                            .unwrap_or_else(|| localize("Any Color")),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut filter.color, None, tr("Any Color"));
                        for color in LayerColor::ALL {
                            ui.selectable_value(&mut filter.color, Some(color), tr(color.title()));
                        }
                    });
                ComboBox::from_id_salt("layer-filter-visibility")
                    .selected_text(tr(match filter.visible {
                        None => "Any Visibility",
                        Some(true) => "Visible Layers",
                        Some(false) => "Hidden Layers",
                    }))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut filter.visible, None, tr("Any Visibility"));
                        ui.selectable_value(&mut filter.visible, Some(true), tr("Visible Layers"));
                        ui.selectable_value(&mut filter.visible, Some(false), tr("Hidden Layers"));
                    });
                ComboBox::from_id_salt("layer-filter-lock")
                    .selected_text(tr(filter.lock.title()))
                    .show_ui(ui, |ui| {
                        for lock in LockFilter::ALL {
                            ui.selectable_value(&mut filter.lock, lock, tr(lock.title()));
                        }
                    });
                if ui
                    .add_enabled(filter.active(), Button::new(tr("Clear Filters")))
                    .clicked()
                {
                    filter = LayerFilter::default();
                    ui.close();
                }
            })
            .0
            .on_hover_text(tr("Filter Layers"));
        });
        let queried_filter = filter.clone();
        let rows = self.cached_layer_indices(ui.ctx(), &filter);
        match &rows {
            Ok(rows) => {
                let selected = (0..self.session.doc.layers.len())
                    .filter(|&i| self.session.doc.layer_selected(i))
                    .count();
                let outside = (0..self.session.doc.layers.len())
                    .filter(|&i| self.session.doc.layer_selected(i) && !rows.contains(&i))
                    .count();
                if selected > 1 {
                    ui.weak(tr(format!("{} selected", selected)));
                }
                if outside > 0 {
                    ui.weak(tr(format!("{} selected outside this filter", outside)));
                }
                if filter.active() {
                    ui.weak(tr(format!(
                        "{} of {} layers",
                        rows.len(),
                        self.session.doc.layers.len()
                    )));
                    if !rows.contains(&self.session.doc.active) {
                        ui.weak(tr("Active layer is outside this filter."));
                    }
                    if ui.small_button(tr("Clear Filters")).clicked() {
                        filter = LayerFilter::default();
                    }
                }
            }
            Err(error) => {
                ui.colored_label(ui.visuals().warn_fg_color, localize(error));
            }
        }
        ui.data_mut(|d| d.insert_temp(key, filter.clone()));
        if filter != queried_filter {
            self.cached_layer_indices(ui.ctx(), &filter)
                .unwrap_or_default()
        } else {
            rows.unwrap_or_default()
        }
    }
    fn layer_menu(&self, ui: &mut Ui, index: usize) -> Option<crate::application::Action> {
        use crate::application::Action;
        let mut action = None;
        let id = self.session.doc.layer_id(index);
        let count = self.session.doc.layers.len();
        let targets = if self.session.doc.layer_selected(index) {
            self.session.doc.selected_layer_ids()
        } else {
            vec![id]
        };
        let indices: Vec<_> = targets
            .iter()
            .map(|id| self.session.doc.layer_index(*id).unwrap())
            .collect();
        let multi = targets.len() > 1;
        let lowest = *indices.iter().min().unwrap();
        let remaining = count - targets.len();
        let pixels = indices.iter().any(|&i| self.session.doc.pixel_locked(i));
        let position = indices.iter().any(|&i| self.session.doc.position_locked(i));
        if multi {
            ui.weak(tr(format!("{} selected", targets.len())));
            for (visible, title) in [
                (true, "Show Selected Layers"),
                (false, "Hide Selected Layers"),
            ] {
                if ui.button(tr(title)).clicked() {
                    action = property_action(&targets, json!({"visible":visible}));
                    ui.close();
                }
            }
            if ui
                .add_enabled(
                    !pixels && indices.iter().all(|&i| !self.session.doc.alpha_locked(i)),
                    Button::new(tr("Clear Selected Layers")),
                )
                .clicked()
            {
                action = Some(batch_action("layer.clear", &targets, json!({})));
                ui.close();
            }
            ui.separator();
        }
        let protected = |i| self.session.doc.alpha_locked(i) || self.session.doc.pixel_locked(i);
        for (command, title, args, enabled) in [
            (
                "layer.duplicate",
                if multi {
                    "Duplicate Selected Layers"
                } else {
                    "Duplicate Layer"
                },
                serde_json::json!({"layer_id":id}),
                count + targets.len() <= 16,
            ),
            (
                "layer.move",
                if multi {
                    "Move Selected Layers Up"
                } else {
                    "Move Layer Up"
                },
                serde_json::json!({"layer_id":id,"to":(lowest+1).min(remaining)}),
                lowest < remaining,
            ),
            (
                "layer.move",
                if multi {
                    "Move Selected Layers Down"
                } else {
                    "Move Layer Down"
                },
                serde_json::json!({"layer_id":id,"to":lowest.saturating_sub(1)}),
                lowest > 0,
            ),
            (
                "layer.delete",
                if multi {
                    "Delete Selected Layers"
                } else {
                    "Delete Layer"
                },
                serde_json::json!({"layer_id":id}),
                count > targets.len(),
            ),
        ] {
            if ui.add_enabled(enabled, Button::new(tr(title))).clicked() {
                action = Some(Action::new(command, args));
                ui.close();
            }
        }
        ui.separator();
        for (command, title, enabled) in [
            ("layer.via_copy", "Layer via Copy", count < 16 && !multi),
            (
                "layer.via_cut",
                "Layer via Cut",
                count < 16
                    && !multi
                    && self.session.doc.selection().current.is_some()
                    && !self.session.doc.alpha_locked(index)
                    && !pixels,
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
                !multi && index > 0 && !protected(index) && !protected(index - 1),
            ),
            (
                "layer.merge_visible",
                "Merge Visible",
                serde_json::json!({}),
                self.session.doc.layers.iter().filter(|l| l.visible).count() > 1
                    && self
                        .session
                        .doc
                        .layers
                        .iter()
                        .enumerate()
                        .all(|(i, l)| !l.visible || !protected(i)),
            ),
            (
                "layer.flatten",
                "Flatten Image",
                serde_json::json!({}),
                (0..count).all(|i| !protected(i)),
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
                        .contains_key("layer.fill_color")
                        && !pixels
                        && !multi,
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
            ("layer.rotate_90_cw", "Rotate Layer 90° Clockwise"),
            ("layer.rotate_90_ccw", "Rotate Layer 90° Counterclockwise"),
        ] {
            if ui
                .add_enabled(
                    self.session.registry.actions.contains_key(command)
                        && !pixels
                        && !position
                        && (!multi
                            || matches!(
                                command,
                                "layer.flip_horizontal"
                                    | "layer.flip_vertical"
                                    | "layer.rotate_180"
                            )),
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
            if ui.add_enabled(self.session.registry.actions.contains_key("layer.translate") && !pixels && !position,Button::new(tr("Apply Offset"))).clicked() {
                action=Some(Action::new("pipeline.run",serde_json::json!({"atomic":true,"steps":[
                    {"id":"select","action":"layer.select","args":{"layer_id":id}},
                    {"id":"translate","action":"layer.translate","args":{"dx":offset.0,"dy":offset.1}}
                ]}))); ui.close();
            }
        });
        ui.separator();
        ui.menu_button(tr("Color Label"), |ui| {
            for color in LayerColor::ALL {
                let text = color_label_text(ui, color);
                if ui
                    .selectable_label(self.session.doc.color_label(index) == color, text)
                    .clicked()
                {
                    action = Some(Action::new(
                        "layer.set",
                        json!({"layer_id":id,"color_label":color}),
                    ));
                    ui.close();
                }
            }
        });
        if multi {
            if let Some(source) = &action {
                let (command, args) = if source.action == "pipeline.run" {
                    let last = source.args["steps"]
                        .as_array()
                        .and_then(|steps| steps.last());
                    last.and_then(|step| {
                        step["action"].as_str().map(|command| {
                            (
                                command,
                                step.get("args").cloned().unwrap_or_else(|| json!({})),
                            )
                        })
                    })
                    .unwrap_or(("", json!({})))
                } else {
                    (source.action.as_str(), source.args.clone())
                };
                if matches!(
                    command,
                    "layer.set"
                        | "layer.duplicate"
                        | "layer.delete"
                        | "layer.move"
                        | "layer.clear"
                        | "layer.flip_horizontal"
                        | "layer.flip_vertical"
                        | "layer.rotate_180"
                        | "layer.translate"
                ) {
                    action = Some(batch_action(command, &targets, args));
                }
            }
        }
        action
    }
    pub(super) fn layers_ui(&mut self, ui: &mut Ui) {
        let mut pending = Vec::new();
        let targets = self.session.doc.selected_layer_ids();
        let target_indices: Vec<_> = targets
            .iter()
            .map(|id| self.session.doc.layer_index(*id).unwrap())
            .collect();
        ui.horizontal_wrapped(|ui| {
            if icon_button(ui, self, 42, false).clicked() {
                self.actions.push(Action::NewLayer);
            }
            if ui
                .add_enabled_ui(
                    target_indices.iter().all(|&i| {
                        !self.session.doc.pixel_locked(i) && !self.session.doc.alpha_locked(i)
                    }),
                    |ui| icon_button(ui, self, 43, false),
                )
                .inner
                .clicked()
            {
                if targets.len() > 1 {
                    pending.push(batch_action("layer.clear", &targets, json!({})));
                } else {
                    self.actions.push(Action::UiCommand(43));
                }
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
        if targets.len() > 1 {
            ui.weak(tr("Active Layer Name"));
        }
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
        let before = LayerProperties::read(self, index);
        let alpha_mixed = target_indices
            .iter()
            .any(|&i| self.session.doc.alpha_locked(i) != before.alpha);
        let pixels_mixed = target_indices
            .iter()
            .any(|&i| self.session.doc.pixel_locked(i) != before.pixels);
        let position_mixed = target_indices
            .iter()
            .any(|&i| self.session.doc.position_locked(i) != before.position);
        let color_mixed = target_indices
            .iter()
            .any(|&i| self.session.doc.color_label(i) != before.color);
        let mut changes = json!({});
        let mut alpha = before.alpha && !alpha_mixed;
        ui.scope(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
            if ui
                .add(
                    Checkbox::new(&mut alpha, tr("Lock Layer Transparency"))
                        .indeterminate(alpha_mixed),
                )
                .changed()
            {
                changes["alpha_locked"] = json!(alpha);
            }
        });
        let a = self.settings.ui.clone();
        ui.horizontal_wrapped(|ui| {
            ui.weak(tr("Layer Locks"));
            let pixel_hint = if pixels_mixed {
                format!("{}\n{}", localize("Lock Pixels"), localize("Mixed Values"))
            } else {
                localize("Lock Pixels").into_owned()
            };
            if ui
                .add(layer_icon_button(
                    &a,
                    '\u{e899}',
                    before.pixels && !pixels_mixed,
                ))
                .on_hover_text(pixel_hint)
                .clicked()
            {
                changes["pixel_locked"] = json!(pixels_mixed || !before.pixels);
            }
            let position_hint = if position_mixed {
                format!(
                    "{}\n{}",
                    localize("Lock Position"),
                    localize("Mixed Values")
                )
            } else {
                localize("Lock Position").into_owned()
            };
            if ui
                .add(layer_icon_button(
                    &a,
                    '\u{e89f}',
                    before.position && !position_mixed,
                ))
                .on_hover_text(position_hint)
                .clicked()
            {
                changes["position_locked"] = json!(position_mixed || !before.position);
            }
            ComboBox::from_id_salt(("layer-color", id))
                .width(a.control_height * 4.)
                .truncate()
                .selected_text(if color_mixed {
                    tr("Mixed Values")
                } else {
                    color_label_text(ui, before.color)
                })
                .show_ui(ui, |ui| {
                    for color in LayerColor::ALL {
                        if ui
                            .selectable_label(
                                !color_mixed && before.color == color,
                                color_label_text(ui, color),
                            )
                            .clicked()
                        {
                            changes["color_label"] = json!(color);
                        }
                    }
                })
                .response
                .on_hover_text(tr("Color Label"));
        });
        let mut mode = self.session.doc.layers[index].mode;
        let mode_mixed = target_indices
            .iter()
            .any(|&i| self.session.doc.layers[i].mode != mode);
        let mut mode_changed = false;
        let narrow = ui.available_width() < a.control_height * 9.;
        let blend = |ui: &mut Ui| {
            ui.weak(tr("Blend Mode"));
            ui.add_enabled_ui(
                self.session.doc.graph.linked || !self.session.doc.graph.enabled,
                |ui| {
                    ComboBox::from_id_salt("layer-blend")
                        .width(if narrow {
                            ui.available_width().min(a.control_height * 6.)
                        } else {
                            a.control_height * 4.
                        })
                        .truncate()
                        .selected_text(tr(if mode_mixed {
                            "Mixed Values"
                        } else {
                            mode.title()
                        }))
                        .show_ui(ui, |ui| {
                            for item in crate::engine::graph::BlendMode::ALL {
                                if ui
                                    .selectable_label(!mode_mixed && mode == item, tr(item.title()))
                                    .clicked()
                                {
                                    mode = item;
                                    mode_changed = true;
                                }
                            }
                        });
                },
            );
        };
        if narrow {
            ui.vertical(blend);
        } else {
            ui.horizontal(blend);
        }
        if mode_changed {
            changes["blend_mode"] = json!(mode);
        }
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
            changes["opacity"] = json!(opacity);
        }
        if target_indices
            .iter()
            .any(|&i| self.session.doc.layers[i].opacity != self.session.doc.layers[index].opacity)
        {
            ui.weak(tr("Mixed opacity; edits apply to selected layers."));
        }
        if let Some(action) = property_action(&targets, changes) {
            pending.push(action);
        }
        ui.add_space(self.settings.ui.item_spacing);
        let indices = self.layer_filters_ui(ui);
        ui.add_space(a.item_spacing);
        let colors = theme::Colors::new(&a);
        let name_height = a.font_size * 1.5;
        let detail_height = a.small_font_size * 1.5;
        let row_height = (a.control_height + a.panel_padding * 2.)
            .max(name_height + detail_height + a.item_spacing * 2.);
        for &index in indices.iter().rev() {
            let width = ui
                .available_width()
                .min((ui.clip_rect().right() - ui.next_widget_position().x).max(0.));
            let (_, r) = ui.allocate_space(vec2(width, row_height));
            let response = ui.interact(
                r,
                ui.id().with((
                    "layer-row",
                    self.session.doc.identity,
                    self.session.doc.layer_id(index),
                )),
                Sense::click(),
            );
            ui.painter().rect_filled(
                r,
                self.settings.ui.corner_radius,
                if index == self.session.doc.active {
                    colors.tint(if colors.dark { 0.16 } else { 0.13 })
                } else if self.session.doc.layer_selected(index) {
                    colors.tint(if colors.dark { 0.08 } else { 0.06 })
                } else if response.hovered() {
                    colors.raised
                } else {
                    colors.panel
                },
            );
            if index == self.session.doc.active {
                ui.painter().rect_stroke(
                    r.shrink(0.5),
                    a.corner_radius,
                    Stroke::new(1., colors.accent.gamma_multiply(0.6)),
                    StrokeKind::Inside,
                );
            }
            let color = self.session.doc.color_label(index);
            if let Some(color) = label_color(color)
                .or_else(|| (index == self.session.doc.active).then_some(colors.accent))
            {
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        pos2(r.left() + 1.5, r.center().y),
                        vec2(3., row_height - a.panel_padding * 2.),
                    ),
                    1.5,
                    color,
                );
            }
            let eye = Rect::from_min_size(
                r.min + vec2(a.item_spacing, (row_height - a.control_height) * 0.5),
                Vec2::splat(a.control_height),
            );
            let toggle = ui.interact(
                eye,
                ui.id().with((
                    "eye",
                    self.session.doc.identity,
                    self.session.doc.layer_id(index),
                )),
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
                    serde_json::json!({"layer_id":self.session.doc.layer_id(index),"visible":!visible}),
                );
            }
            let thumb_size =
                (a.control_height + a.panel_padding * 0.75).min(row_height - a.item_spacing * 2.);
            let thumb = Rect::from_min_size(
                pos2(
                    eye.right() + a.item_spacing,
                    r.center().y - thumb_size * 0.5,
                ),
                Vec2::splat(thumb_size),
            );
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
                            thumb.min + vec2(x as f32, y as f32) * (thumb_size / 16.),
                            Vec2::splat(thumb_size / 16.),
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
            let name = if self.session.doc.custom_layer_name(index).is_empty() {
                localize(format!("Layer {:02}", index + 1))
            } else {
                self.session.doc.layer_name(index)
            };
            let properties = LayerProperties::read(self, index);
            let text_left = thumb.right() + a.panel_padding;
            let text_right = r.right() - a.panel_padding;
            layer_text(
                ui,
                name.clone(),
                Rect::from_min_max(
                    pos2(text_left, r.top() + a.item_spacing),
                    pos2(text_right, r.top() + a.item_spacing + name_height),
                ),
                if index == self.session.doc.active {
                    theme::strong(ui.ctx(), a.font_size)
                } else {
                    FontId::proportional(a.font_size)
                },
                colors.text,
            );
            let badges = [
                (properties.alpha, '\u{e3ec}', "Transparency Locked"),
                (properties.pixels, '\u{e899}', "Pixel Locked"),
                (properties.position, '\u{e89f}', "Position Locked"),
            ];
            let badge_count = badges.iter().filter(|(enabled, _, _)| *enabled).count();
            let badge_width = a.small_font_size + a.item_spacing;
            let badge_left = (text_right - badge_count as f32 * badge_width).max(text_left);
            layer_text(
                ui,
                format!(
                    "{} · {:.0}%",
                    crate::i18n::text(self.session.doc.layers[index].mode.title()),
                    self.session.doc.layers[index].opacity as f32 / 255. * 100.
                ),
                Rect::from_min_max(
                    pos2(text_left, r.bottom() - a.item_spacing - detail_height),
                    pos2(
                        (badge_left - a.item_spacing).max(text_left),
                        r.bottom() - a.item_spacing,
                    ),
                ),
                FontId::proportional(a.small_font_size),
                colors.muted,
            );
            for (n, (_, icon, _)) in badges.iter().filter(|(enabled, _, _)| *enabled).enumerate() {
                let rect = Rect::from_min_size(
                    pos2(
                        badge_left + n as f32 * badge_width,
                        r.bottom() - a.item_spacing - detail_height,
                    ),
                    vec2(badge_width, detail_height),
                );
                ui.painter()
                    .with_clip_rect(ui.clip_rect().intersect(r))
                    .text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *icon,
                        FontId::new(a.small_font_size, FontFamily::Name("Material".into())),
                        colors.muted,
                    );
            }
            let mut hint = format!("{}\n{}", name, localize(properties.color.title()));
            for (_, _, title) in badges.iter().filter(|(enabled, _, _)| *enabled) {
                hint.push('\n');
                hint.push_str(&localize(*title));
            }
            hint.push('\n');
            hint.push_str(&localize(
                "Click selects one layer. Ctrl toggles. Shift selects a visible range.",
            ));
            if index == self.session.doc.active {
                hint.push('\n');
                hint.push_str(&localize("Active painting layer"));
            }
            let response = response.on_hover_text(hint);
            if response.clicked() && !toggle.clicked() {
                self.layer_row_selection(
                    ui.ctx(),
                    self.session.doc.layer_id(index),
                    &indices,
                    ui.input(|input| input.modifiers),
                );
            }
            response.context_menu(|ui| {
                if let Some(action) = self.layer_menu(ui, index) {
                    pending.push(action);
                }
            });
        }
        self.dispatch_layer_pending(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::studio;
    fn panel_frame(
        app: &mut Studio,
        ctx: &Context,
        size: Vec2,
        events: Vec<Event>,
        time: f64,
    ) -> FullOutput {
        crate::i18n::set_language(app.editor.settings.ui.language);
        if !ctx.data_mut(|d| {
            d.get_temp::<bool>(Id::new("layer-test-fonts"))
                .unwrap_or_default()
        }) {
            theme::fonts(ctx);
            theme::apply(ctx, &app.editor.settings.ui);
            ctx.data_mut(|d| d.insert_temp(Id::new("layer-test-fonts"), true));
        }
        ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                time: Some(time),
                focused: true,
                ..Default::default()
            },
            |ctx| {
                CentralPanel::default()
                    .frame(Frame::NONE)
                    .show(ctx, |ui| app.editor.layers_ui(ui));
            },
        )
    }
    fn label(shape: &Shape, text: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => Some(t.pos + t.galley.size() * 0.5),
            Shape::Vec(v) => v.iter().find_map(|s| label(s, text)),
            _ => None,
        }
    }
    fn layers() -> Studio {
        let mut app = studio();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        assert!(app.editor.core("layer.rename", json!({"name":"Ink 墨"})));
        assert!(app.editor.core(
            "layer.set",
            json!({"color_label":"violet","pixel_locked":true})
        ));
        assert!(app.editor.core("layer.add", json!({})));
        assert!(app
            .editor
            .core("layer.rename", json!({"name":"Sketch 下描き"})));
        app
    }
    #[test]
    fn layers_filters_use_the_shared_query_without_selecting_hiding_or_recording() {
        let mut app = layers();
        let filter = LayerFilter {
            query: "iNk".into(),
            color: Some(LayerColor::Violet),
            visible: Some(true),
            selected: None,
            lock: LockFilter::Pixels,
        };
        let ctx = Context::default();
        ctx.data_mut(|d| {
            d.insert_temp(
                LayerFilter::key(app.editor.session.doc.identity),
                filter.clone(),
            )
        });
        let state = app.editor.session.state();
        let history = app.editor.session.doc.history_bytes();
        let pointers: Vec<_> = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|l| l.pixels.as_ptr())
            .collect();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        assert_eq!(app.editor.filtered_layer_indices(&filter).unwrap(), vec![0]);
        let mut output = panel_frame(&mut app, &ctx, vec2(244., 1000.), vec![], 0.);
        for n in 1..3 {
            output = panel_frame(&mut app, &ctx, vec2(244., 1000.), vec![], n as f64);
        }
        assert!(output.shapes.iter().any(|s| label(
            &s.shape,
            "Active layer is outside this filter."
        )
        .is_some()));
        assert!(output
            .shapes
            .iter()
            .any(|s| label(&s.shape, "Clear Filters").is_some()));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.active, 1);
        assert!(app.editor.session.doc.layers.iter().all(|l| l.visible));
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|l| l.pixels.as_ptr())
                .collect::<Vec<_>>(),
            pointers
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
        let p = output
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "Clear Filters"))
            .unwrap();
        panel_frame(
            &mut app,
            &ctx,
            vec2(244., 1000.),
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
            3.,
        );
        assert!(!ctx
            .data_mut(
                |d| d.get_temp::<LayerFilter>(LayerFilter::key(app.editor.session.doc.identity))
            )
            .unwrap()
            .active());
        assert_eq!(
            app.editor
                .filtered_layer_indices(&LayerFilter::default())
                .unwrap(),
            vec![0, 1]
        );
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        app.editor.register_commands();
        for query in [
            "Find Layers",
            "查找图层",
            "レイヤーを検索",
            "레이어 찾기",
            "layer.query",
        ] {
            let keys = &app.editor.settings.keys;
            let mut palette =
                crate::ui::palette::Palette::new(crate::ui::palette::Mode::Commands, keys);
            palette.query(query.into(), keys);
            assert!(
                palette
                    .results
                    .iter()
                    .any(|&i| keys.command(i).keywords == "layer.query"),
                "{query}"
            );
        }
    }
    #[test]
    fn layers_filter_cache_reuses_paint_results_and_invalidates_metadata_and_filter_changes() {
        let mut app = layers();
        let ctx = Context::default();
        let filter = LayerFilter {
            query: "Ink".into(),
            ..Default::default()
        };
        let first = app.editor.cached_layer_indices(&ctx, &filter).unwrap();
        assert_eq!(&*first, &[0]);
        let again = app.editor.cached_layer_indices(&ctx, &filter).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        // Remove the query action temporarily to detect an unwanted execution
        // during a repaint. Plugin contribution changes require restart in use.
        let query = app
            .editor
            .session
            .registry
            .actions
            .remove("layer.query")
            .unwrap();
        app.editor.session.doc.layers[1].pixels[0] = 0xff112233;
        let painted = app.editor.cached_layer_indices(&ctx, &filter).unwrap();
        assert!(Arc::ptr_eq(&first, &painted));
        app.editor
            .session
            .registry
            .actions
            .insert("layer.query".into(), query);
        let id = app.editor.session.doc.layer_id(0);
        assert!(app
            .editor
            .core("layer.rename", json!({"layer_id":id,"name":"Line"})));
        assert!(app
            .editor
            .cached_layer_indices(&ctx, &filter)
            .unwrap()
            .is_empty());
        let filter = LayerFilter {
            query: "Line".into(),
            color: Some(LayerColor::Violet),
            visible: Some(true),
            selected: None,
            lock: LockFilter::Pixels,
        };
        assert_eq!(
            &*app.editor.cached_layer_indices(&ctx, &filter).unwrap(),
            &[0]
        );
        assert!(app.editor.core("layer.move", json!({"layer_id":id,"to":1})));
        assert_eq!(
            &*app.editor.cached_layer_indices(&ctx, &filter).unwrap(),
            &[1]
        );
        assert!(app
            .editor
            .core("layer.set", json!({"layer_id":id,"visible":false})));
        assert!(app
            .editor
            .cached_layer_indices(&ctx, &filter)
            .unwrap()
            .is_empty());
        assert!(app.editor.core(
            "layer.set",
            json!({"layer_id":id,"visible":true,"color_label":"red"})
        ));
        assert!(app
            .editor
            .cached_layer_indices(&ctx, &filter)
            .unwrap()
            .is_empty());
        let mut filter = LayerFilter {
            color: Some(LayerColor::Red),
            ..filter
        };
        assert_eq!(
            &*app.editor.cached_layer_indices(&ctx, &filter).unwrap(),
            &[1]
        );
        assert!(app.editor.core(
            "layer.set",
            json!({"layer_id":id,"pixel_locked":false,"position_locked":true})
        ));
        assert!(app
            .editor
            .cached_layer_indices(&ctx, &filter)
            .unwrap()
            .is_empty());
        filter.lock = LockFilter::Position;
        assert_eq!(
            &*app.editor.cached_layer_indices(&ctx, &filter).unwrap(),
            &[1]
        );
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        assert!(app
            .editor
            .cached_layer_indices(&ctx, &filter)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn layers_combined_properties_commit_one_event_and_restore_with_one_undo() {
        let mut app = layers();
        let index = app.editor.session.doc.active;
        let id = app.editor.session.doc.layer_id(index);
        let before = LayerProperties::read(&app.editor, index);
        let after = LayerProperties {
            alpha: true,
            pixels: true,
            position: true,
            color: LayerColor::Blue,
        };
        let action = after.action(before, id).unwrap();
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let source = app.editor.session.doc.composition();
        let pixels = app.editor.session.doc.layers[index].pixels.clone();
        assert!(app.editor.core(&action.action, action.args.clone()));
        assert!(LayerProperties::read(&app.editor, index) == after);
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        assert_eq!(
            journal.events(events, 1).unwrap()["events"][0]["source"]["args"],
            action.args
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.composition(), source);
        assert!(LayerProperties::read(&app.editor, index) == before);
        assert_eq!(app.editor.session.doc.layers[index].pixels, pixels);
        assert!(app.editor.core("history.redo", json!({})));
        assert!(LayerProperties::read(&app.editor, index) == after);
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
    }
    #[test]
    fn layers_filter_redraw_keeps_pending_rename_attached_to_its_stable_id() {
        let mut app = layers();
        let ctx = Context::default();
        let size = vec2(244., 1000.);
        for n in 0..3 {
            panel_frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let id = app.editor.session.doc.layer_id(1);
        let edit = Id::new(("layer-name", app.editor.session.doc.identity, id));
        ctx.memory_mut(|m| m.request_focus(edit));
        ctx.data_mut(|d| {
            d.insert_temp(edit, "仕上げ Ink".to_string());
            d.insert_temp(
                LayerFilter::key(app.editor.session.doc.identity),
                LayerFilter {
                    query: "Ink 墨".into(),
                    ..Default::default()
                },
            );
        });
        for n in 0..2 {
            panel_frame(&mut app, &ctx, size, vec![], 3. + n as f64);
        }
        assert_eq!(app.editor.session.doc.layer_name(1), "Sketch 下描き");
        assert_eq!(
            ctx.data_mut(|d| d.get_temp::<String>(edit)).as_deref(),
            Some("仕上げ Ink")
        );
        panel_frame(
            &mut app,
            &ctx,
            size,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: Some(Key::Enter),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            5.,
        );
        assert_eq!(app.editor.session.doc.layer_name(1), "仕上げ Ink");
        assert_eq!(app.editor.session.doc.layer_id(1), id);
        assert_eq!(app.editor.session.doc.layer_name(0), "Ink 墨");
        assert!(ctx.data_mut(|d| d.get_temp::<String>(edit)).is_none());
    }
    fn selected(app: &Studio) -> Vec<u64> {
        app.editor.session.doc.selected_layer_ids()
    }
    fn native_multi() -> (Studio, Vec<u64>) {
        let mut app = studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        for i in 0..4 {
            if i > 0 {
                assert!(app.editor.core("layer.add", json!({})));
            }
            assert!(app
                .editor
                .core("layer.rename", json!({"name":format!("L{i}")})));
            assert!(app.editor.core(
                "layer.fill_color",
                json!({"color":format!("#{:06X}",0x224466+i*0x110000)})
            ));
        }
        let ids = (0..4)
            .map(|i| app.editor.session.doc.layer_id(i))
            .collect::<Vec<_>>();
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[ids[1],ids[2]],"active_layer_id":ids[2]})
        ));
        (app, ids)
    }
    fn menu_click(app: &mut Studio, title: &str) -> crate::application::Action {
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let mut chosen = None;
        let mut point = None;
        for n in 0..4 {
            let events = if n == 3 {
                let p = point.unwrap();
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
                ]
            } else {
                vec![]
            };
            let output = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(460., 1400.))),
                    events,
                    time: Some(n as f64),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
                        if let Some(action) =
                            app.editor.layer_menu(ui, app.editor.session.doc.active)
                        {
                            chosen = Some(action);
                        }
                    });
                },
            );
            if n == 2 {
                point = output
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, title));
                assert!(point.is_some(), "missing menu action {title}");
            }
        }
        chosen.unwrap_or_else(|| panic!("menu click did not dispatch {title}"))
    }
    #[test]
    fn layers_ctrl_toggle_and_shift_ranges_keep_active_and_filter_boundaries_explicit() {
        let mut app = layers();
        assert!(app.editor.core("layer.add", json!({})));
        assert!(app.editor.core("layer.add", json!({})));
        let ids = (0..4)
            .map(|i| app.editor.session.doc.layer_id(i))
            .collect::<Vec<_>>();
        let ctx = Context::default();
        let all = [0, 1, 2, 3];
        let pixels = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect::<Vec<_>>();
        let history = app.editor.session.doc.history_bytes();
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[0], &all, Modifiers::NONE));
        assert_eq!(selected(&app), vec![ids[0]]);
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[2], &all, Modifiers::CTRL));
        assert_eq!(selected(&app), vec![ids[0], ids[2]]);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            ids[2]
        );
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[3], &all, Modifiers::SHIFT));
        assert_eq!(selected(&app), vec![ids[2], ids[3]]);
        let both = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::NONE
        };
        assert!(app.editor.layer_row_selection(&ctx, ids[0], &all, both));
        assert_eq!(selected(&app), ids);
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[0], &all, Modifiers::CTRL));
        assert_eq!(app.editor.session.doc.active, 1);
        for id in [ids[2], ids[3]] {
            assert!(app
                .editor
                .layer_row_selection(&ctx, id, &all, Modifiers::CTRL));
        }
        assert_eq!(selected(&app), vec![ids[1]]);
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[1], &all, Modifiers::CTRL));
        assert_eq!(selected(&app), vec![ids[1]]);
        let visible = [0, 2, 3];
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[0], &visible, Modifiers::NONE));
        assert!(app
            .editor
            .layer_row_selection(&ctx, ids[3], &visible, Modifiers::SHIFT));
        assert_eq!(selected(&app), vec![ids[0], ids[2], ids[3]]);
        assert!(app
            .editor
            .session
            .doc
            .layers
            .iter()
            .all(|layer| layer.visible));
        // An external selection invalidates the old gesture anchor. Add-range
        // keeps explicitly selected layers that are outside the current filter.
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[ids[0],ids[1]],"active_layer_id":ids[0]})
        ));
        assert!(app.editor.layer_row_selection(&ctx, ids[3], &visible, both));
        assert_eq!(selected(&app), ids);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
        );
        assert_eq!(app.editor.session.doc.history_bytes(), history);
    }
    #[test]
    fn layers_mixed_property_click_preserves_other_fields_and_commits_pending_active_name_first() {
        let mut app = layers();
        let ids = (0..2)
            .map(|i| app.editor.session.doc.layer_id(i))
            .collect::<Vec<_>>();
        assert!(app.editor.core("layer.set",json!({"layer_id":ids[0],"alpha_locked":true,"position_locked":true,"color_label":"red"})));
        assert!(app
            .editor
            .core("layer.set", json!({"layer_id":ids[1],"color_label":"blue"})));
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":ids,"active_layer_id":ids[0]})
        ));
        let before = ids
            .iter()
            .map(|id| {
                LayerProperties::read(
                    &app.editor,
                    app.editor.session.doc.layer_index(*id).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        for n in 0..3 {
            panel_frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let edit = Id::new(("layer-name", app.editor.session.doc.identity, ids[0]));
        ctx.data_mut(|data| data.insert_temp(edit, "Finished Ink".to_owned()));
        ctx.memory_mut(|memory| memory.request_focus(edit));
        let output = panel_frame(&mut app, &ctx, size, vec![], 3.);
        let p = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, "Lock Layer Transparency"))
            .unwrap();
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        panel_frame(
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
            4.,
        );
        assert_eq!(app.editor.session.doc.layer_name(0), "Finished Ink");
        assert_eq!(app.editor.session.doc.layer_name(1), "Sketch 下描き");
        for (i, previous) in before.iter().enumerate() {
            let after = LayerProperties::read(&app.editor, i);
            assert!(after.alpha);
            assert_eq!(
                (after.pixels, after.position, after.color),
                (previous.pixels, previous.position, previous.color)
            );
        }
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 2);
        let recorded = journal.events(events, 2).unwrap();
        assert_eq!(recorded["events"][0]["source"]["action"], "layer.rename");
        assert_eq!(
            recorded["events"][1]["source"]["args"]["args"],
            json!({"alpha_locked":true})
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.layer_name(0), "Finished Ink");
        assert!(LayerProperties::read(&app.editor, 0) == before[0]);
        assert!(LayerProperties::read(&app.editor, 1) == before[1]);
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.layer_name(0), "Ink 墨");
    }
    #[test]
    fn layers_batch_menu_clear_delete_reorder_and_duplicate_each_have_one_event_and_one_undo() {
        for (title, command) in [
            ("Clear Selected Layers", "layer.clear"),
            ("Delete Selected Layers", "layer.delete"),
            ("Move Selected Layers Up", "layer.move"),
            ("Move Selected Layers Down", "layer.move"),
            ("Duplicate Selected Layers", "layer.duplicate"),
        ] {
            let (mut app, ids) = native_multi();
            let before = app.editor.session.doc.composition();
            let pixels = app
                .editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>();
            let action = menu_click(&mut app, title);
            assert_eq!(action.action, "layer.batch");
            assert_eq!(action.args["action"], command);
            assert_eq!(action.args["layer_ids"], json!([ids[1], ids[2]]));
            app.editor.session.replay_boundary().unwrap();
            let events = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            app.editor.dispatch_layer_pending(vec![action]);
            assert_eq!(
                app.editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len(),
                events + 1,
                "{title}: {}",
                app.editor.status
            );
            let order = (0..app.editor.session.doc.layers.len())
                .map(|i| app.editor.session.doc.layer_id(i))
                .collect::<Vec<_>>();
            match title {
                "Clear Selected Layers" => {
                    for i in [1, 2] {
                        assert!(app.editor.session.doc.layers[i]
                            .pixels
                            .iter()
                            .all(|p| *p == 0));
                    }
                }
                "Delete Selected Layers" => assert_eq!(order, vec![ids[0], ids[3]]),
                "Move Selected Layers Up" => {
                    assert_eq!(order, vec![ids[0], ids[3], ids[1], ids[2]])
                }
                "Move Selected Layers Down" => {
                    assert_eq!(order, vec![ids[1], ids[2], ids[0], ids[3]])
                }
                "Duplicate Selected Layers" => {
                    assert_eq!(order.len(), 6);
                    assert!(selected(&app).iter().all(|id| !ids.contains(id)));
                }
                _ => unreachable!(),
            }
            let allocated = app.editor.session.doc.composition().next_layer_id;
            assert!(app.editor.core("history.undo", json!({})), "{title}");
            let mut restored = app.editor.session.doc.composition();
            if command == "layer.duplicate" {
                // Undo removes the copies without reusing their stable IDs.
                assert_eq!(restored.next_layer_id, allocated);
                assert!(restored.next_layer_id > before.next_layer_id);
                restored.next_layer_id = before.next_layer_id;
            }
            assert_eq!(restored, before, "{title}");
            assert_eq!(
                app.editor
                    .session
                    .doc
                    .layers
                    .iter()
                    .map(|layer| layer.pixels.clone())
                    .collect::<Vec<_>>(),
                pixels,
                "{title}"
            );
        }
    }
    #[test]
    fn layers_pending_dispatch_preserves_explicit_revision_conflicts() {
        let mut app = layers();
        let ids = selected(&app);
        let mut action = batch_action("layer.set", &ids, json!({"visible":false}));
        action.expected_revision = Some(app.editor.session.revision.saturating_sub(1));
        let before = app.editor.session.state();
        app.editor.dispatch_layer_pending(vec![action]);
        assert_eq!(app.editor.session.state(), before);
        assert!(app.editor.status.contains("revision_conflict"));
    }
    #[test]
    fn layers_new_selection_and_batch_controls_keep_external_hand_contacts_intact() {
        let (mut app, ids) = native_multi();
        assert!(app.editor.core("tool.select", json!({"tool":"hand"})));
        assert!(app.editor.core(
            "canvas.pointer",
            json!({"phase":"down","x":2,"y":2,"time":0})
        ));
        let state = app.editor.session.state();
        assert!(!app.editor.layer_row_selection(
            &Context::default(),
            ids[0],
            &[0, 1, 2, 3],
            Modifiers::NONE
        ));
        assert_eq!(app.editor.session.state(), state);
        app.editor.dispatch_layer_pending(vec![batch_action(
            "layer.clear",
            &[ids[1], ids[2]],
            json!({}),
        )]);
        assert_eq!(app.editor.session.state(), state);
        assert!(app.editor.session.stroke.active());
        assert!(app.editor.session.pointer_checkpoint.is_some());
        assert!(app
            .editor
            .core("canvas.pointer", json!({"phase":"up","x":2,"y":2,"time":1})));
    }
    #[test]
    fn layer_icon_controls_keep_square_targets_when_a_font_glyph_is_unavailable() {
        let ctx = Context::default();
        let a = crate::configuration::Appearance::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &a);
        // Exercise a missing-glyph path without depending on the shipped subset.
        let missing = char::from_u32(0x10ffff).unwrap();
        let mut rects = [Rect::NOTHING; 2];
        let mut clicked = false;
        for frame in 0..4 {
            let events = if frame == 3 {
                let p = rects[0].max - vec2(2., 2.);
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
                ]
            } else {
                vec![]
            };
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(176., 200.))),
                    events,
                    time: Some(frame as f64),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    assert!(!ctx.fonts_mut(|fonts| fonts.has_glyph(
                        &FontId::new(a.icon_size, FontFamily::Name("Material".into())),
                        missing
                    )));
                    CentralPanel::default().frame(Frame::NONE).show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let response = ui.add(layer_icon_button(&a, missing, false));
                            rects[0] = response.rect;
                            clicked |= response.clicked();
                            rects[1] = egui::containers::menu::MenuButton::from_button(
                                layer_icon_button(&a, missing, false),
                            )
                            .ui(ui, |ui| {
                                ui.label("Filters");
                            })
                            .0
                            .rect;
                        });
                    });
                },
            );
            for rect in rects {
                assert!(
                    rect.width() >= layer_icon_side(&a) && rect.height() >= layer_icon_side(&a),
                    "{rect:?}"
                );
            }
        }
        assert!(
            clicked,
            "the outer corner of the square target must receive input"
        );
    }
    #[test]
    fn layers_names_filters_and_properties_fit_narrow_cjk_panels() {
        let old_language = crate::i18n::current();
        let name = "原稿下描き 레이어 Inking ".repeat(7);
        for language in crate::i18n::Language::ALL {
            for width in [176., 244., 480.] {
                let mut app = layers();
                app.editor.settings.ui.language = language;
                assert!(app.editor.core("layer.rename", json!({"name":name})));
                assert!(app.editor.core("layer.set",json!({"alpha_locked":true,"pixel_locked":true,"position_locked":true,"color_label":"green"})));
                let ids = (0..app.editor.session.doc.layers.len())
                    .map(|i| app.editor.session.doc.layer_id(i))
                    .collect::<Vec<_>>();
                assert!(app.editor.core("layer.selection.set",json!({"layer_ids":ids,"active_layer_id":app.editor.session.doc.layer_id(app.editor.session.doc.active)})));
                let state = app.editor.session.state();
                let ctx = Context::default();
                let size = vec2(width, 1100.);
                for n in 0..3 {
                    panel_frame(&mut app, &ctx, size, vec![], n as f64);
                }
                let output = panel_frame(&mut app, &ctx, size, vec![], 3.);
                let find = crate::i18n::text_in(language, "Find Layers");
                let p = output
                    .shapes
                    .iter()
                    .find_map(|s| label(&s.shape, &find))
                    .unwrap();
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains(p),
                    "{language:?} search outside {width}"
                );
                for (code, minimum) in [('\u{e3ec}', 1), ('\u{e899}', 2), ('\u{e89f}', 2)] {
                    let count=output.shapes.iter().filter(|shape|matches!(&shape.shape,Shape::Text(text)
                        if text.galley.text()==code.to_string() && text.galley.job.sections.iter().all(|section|section.format.font_id.family==FontFamily::Name("Material".into())))).count();
                    assert!(count >= minimum, "{language:?}: Material badge {code}");
                }
                let shape=output.shapes.iter().find(|s|matches!(&s.shape,Shape::Text(t) if t.galley.text()==name && t.galley.elided)).expect("long row name must be elided");
                let Shape::Text(t) = &shape.shape else {
                    unreachable!()
                };
                let bounds = t.galley.rect.translate(t.pos.to_vec2());
                assert!(
                    shape.clip_rect.contains_rect(bounds),
                    "{language:?}: name clip {bounds:?} vs {:?}",
                    shape.clip_rect
                );
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(bounds),
                    "{language:?}: name overflow at {width}"
                );
                assert_eq!(app.editor.session.state(), state);
            }
        }
        crate::i18n::set_language(old_language);
    }
}
