//! Group hierarchy presentation; document changes use registered group actions.
use super::*;
use crate::engine::groups::GroupPolicy;
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub(super) struct Entry {
    pub(super) kind: String,
    pub(super) id: u64,
}
#[derive(Clone, Deserialize)]
pub(super) struct Group {
    pub(super) id: u64,
    pub(super) name: String,
    pub(super) parent_id: u64,
    pub(super) visible: bool,
    pub(super) opacity: f32,
    pub(super) blend_mode: crate::engine::graph::BlendMode,
    #[serde(default)]
    pub(super) compositing: GroupPolicy,
    #[serde(default)]
    pub(super) mergeable: bool,
    #[serde(default)]
    pub(super) merge_error: Option<String>,
    #[serde(default)]
    pub(super) children: Vec<Entry>,
    #[serde(default)]
    pub(super) layer_ids: Vec<u64>,
}
#[derive(Clone, Deserialize)]
pub(super) struct Row {
    pub(super) kind: String,
    pub(super) id: u64,
    pub(super) depth: usize,
    pub(super) name: String,
    pub(super) visible: bool,
    pub(super) effective_visible: bool,
    #[serde(default)]
    pub(super) layer_ids: Vec<u64>,
}
#[derive(Deserialize)]
struct Query {
    #[serde(default)]
    pass_through_available: bool,
    #[serde(default)]
    roots: Vec<Entry>,
    groups: Vec<Group>,
    rows: Vec<Row>,
    #[serde(default)]
    matching_layer_ids: Vec<u64>,
    #[serde(default)]
    matching_group_ids: Vec<u64>,
}
pub(super) struct Data {
    pub(super) pass_through_available: bool,
    pub(super) roots: Vec<Entry>,
    pub(super) groups: Vec<Group>,
    pub(super) rows: Vec<Row>,
    pub(super) layers: Arc<[usize]>,
    pub(super) matched_layers: usize,
    pub(super) matched_groups: usize,
}
#[derive(Clone)]
struct Cache {
    metadata: u64,
    generation: u64,
    filter: LayerFilter,
    result: Result<Arc<Data>, String>,
}
#[derive(Clone, Default)]
struct Workspace {
    focused: Option<u64>,
    collapsed: std::collections::BTreeSet<u64>,
}
#[derive(Clone)]
struct NameDraft {
    text: String,
    generation: u64,
}
#[derive(Clone)]
struct OpacityDraft {
    value: f32,
    revision: u64,
    generation: u64,
    dragging: bool,
    submitted: bool,
}
struct OpacityCommit {
    value: f32,
    revision: u64,
}
fn opacity_key(identity: u64, id: u64) -> Id {
    Id::new(("group-opacity", identity, id))
}
fn workspace_key(identity: u64) -> Id {
    Id::new(("layer-groups", identity))
}
fn name_key(identity: u64, id: u64) -> Id {
    Id::new(("group-name", identity, id))
}
#[derive(Clone, Copy)]
pub(super) enum DisplayEntry {
    Group(usize),
    Layer {
        index: usize,
        depth: usize,
        effective_visible: bool,
    },
}
pub(super) struct Listing {
    pub(super) data: Arc<Data>,
    pub(super) entries: Vec<DisplayEntry>,
    pub(super) layers: Arc<[usize]>,
}
impl Editor {
    pub(super) fn group_query_ui(
        &mut self,
        ctx: &Context,
        filter: &LayerFilter,
    ) -> Result<Arc<Data>, String> {
        let key = Id::new(("group-query", self.session.doc.identity, filter.active()));
        let metadata = layer_fingerprint(self);
        let generation = self.session.doc.groups_revision();
        if let Some(cache) = ctx.data_mut(|data| data.get_temp::<Arc<Cache>>(key)) {
            if cache.metadata == metadata
                && cache.generation == generation
                && cache.filter == *filter
            {
                return cache.result.clone();
            }
        }
        let result = self
            .session
            .apply(crate::application::Action::new(
                "group.query",
                filter.args(),
            ))
            .map_err(|error| error.message)
            .and_then(|response| {
                serde_json::from_value::<Query>(response["result"].clone())
                    .map_err(|error| error.to_string())
            })
            .and_then(|query| {
                let layers = query
                    .rows
                    .iter()
                    .filter(|row| row.kind == "layer")
                    .map(|row| {
                        self.session
                            .doc
                            .layer_index(row.id)
                            .ok_or_else(|| "Invalid group query result".to_owned())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Arc::new(Data {
                    pass_through_available: query.pass_through_available,
                    roots: query.roots,
                    groups: query.groups,
                    rows: query.rows,
                    layers: Arc::from(layers),
                    matched_layers: query.matching_layer_ids.len(),
                    matched_groups: query.matching_group_ids.len(),
                }))
            });
        ctx.data_mut(|data| {
            data.insert_temp(
                key,
                Arc::new(Cache {
                    metadata,
                    generation,
                    filter: filter.clone(),
                    result: result.clone(),
                }),
            )
        });
        result
    }
    pub(super) fn panel_filtered_indices(
        &mut self,
        ctx: &Context,
        filter: &LayerFilter,
    ) -> Result<Arc<[usize]>, String> {
        if self.session.doc.has_groups() {
            self.group_query_ui(ctx, filter)
                .map(|data| data.layers.clone())
        } else {
            self.cached_layer_indices(ctx, filter)
        }
    }
    pub(super) fn group_sibling_move_ui(&self, ids: &[u64], up: bool) -> Option<Value> {
        let state = self.session.doc.groups_state();
        let rows = state["rows"].as_array()?;
        let mut parent = None;
        for id in ids {
            let row = rows
                .iter()
                .find(|row| row["kind"] == "layer" && row["id"].as_u64() == Some(*id))?;
            let id = row["parent_id"].as_u64()?;
            if parent.is_some_and(|parent| parent != id) {
                return None;
            }
            parent = Some(id);
        }
        let parent = parent?;
        let children = if parent == 0 {
            state["roots"].as_array()?
        } else {
            state["groups"]
                .as_array()?
                .iter()
                .find(|group| group["id"].as_u64() == Some(parent))?["children"]
                .as_array()?
        };
        let positions = children
            .iter()
            .enumerate()
            .filter(|(_, child)| {
                child["kind"] == "layer" && child["id"].as_u64().is_some_and(|id| ids.contains(&id))
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if positions.len() != ids.len() {
            return None;
        }
        let lowest = *positions.iter().min()?;
        let remaining = children.len() - ids.len();
        if up && lowest >= remaining || !up && lowest == 0 {
            return None;
        }
        let index = if up {
            (lowest + 1).min(remaining)
        } else {
            lowest - 1
        };
        Some(
            json!({"items":ids.iter().map(|id|json!({"kind":"layer","id":id})).collect::<Vec<_>>(),"parent_id":parent,"index":index}),
        )
    }
    pub(super) fn focused_group_ui(&self, ctx: &Context) -> Option<u64> {
        ctx.data_mut(|data| data.get_temp::<Workspace>(workspace_key(self.session.doc.identity)))
            .and_then(|workspace| workspace.focused)
    }
    pub(in crate::desktop) fn focus_group_ui(&self, ctx: &Context, id: Option<u64>) {
        let key = workspace_key(self.session.doc.identity);
        let mut workspace = ctx
            .data_mut(|data| data.get_temp::<Workspace>(key))
            .unwrap_or_default();
        workspace.focused = id;
        ctx.data_mut(|data| data.insert_temp(key, workspace));
    }
    pub(super) fn group_listing_ui(
        &mut self,
        ctx: &Context,
        filter: &LayerFilter,
    ) -> Result<Listing, String> {
        let data = self.group_query_ui(ctx, filter)?;
        let workspace = ctx
            .data_mut(|data| data.get_temp::<Workspace>(workspace_key(self.session.doc.identity)))
            .unwrap_or_default();
        let mut hidden = None;
        let mut entries = Vec::new();
        let mut layers = Vec::new();
        for (i, row) in data.rows.iter().enumerate() {
            if hidden.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            hidden = None;
            if row.kind == "group" {
                entries.push(DisplayEntry::Group(i));
                if !filter.active() && workspace.collapsed.contains(&row.id) {
                    hidden = Some(row.depth);
                }
            } else if let Some(index) = self.session.doc.layer_index(row.id) {
                entries.push(DisplayEntry::Layer {
                    index,
                    depth: row.depth,
                    effective_visible: row.effective_visible,
                });
                layers.push(index);
            }
        }
        Ok(Listing {
            data,
            entries,
            layers: Arc::from(layers),
        })
    }
    pub(super) fn group_toolbar_ui(
        &mut self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) {
        let focus = self.focused_group_ui(ui.ctx());
        let linked = self.session.doc.graph.linked;
        let enabled = linked && self.session.doc.ensure_group_renderer().is_ok();
        let selected = self.session.doc.selected_layer_ids();
        let selected_parent = selected
            .first()
            .and_then(|id| self.session.doc.layer_index(*id))
            .map(|i| self.session.doc.layer_parent_id(i));
        let siblings = selected.iter().all(|id| {
            self.session
                .doc
                .layer_index(*id)
                .is_some_and(|i| Some(self.session.doc.layer_parent_id(i)) == selected_parent)
        });
        egui::containers::menu::MenuButton::from_button(layer_icon_button(
            &self.settings.ui,
            '\u{e53b}',
            focus.is_some(),
        ))
        .ui(ui, |ui| {
            if ui
                .add_enabled(
                    enabled && siblings,
                    Button::new(tr("Group Selected Layers")).wrap(),
                )
                .clicked()
            {
                pending.push(crate::application::Action::new(
                    "group.create_selected",
                    json!({}),
                ));
                ui.close();
            }
            if ui
                .add_enabled(enabled, Button::new(tr("New Empty Group")).wrap())
                .clicked()
            {
                pending.push(crate::application::Action::new(
                    "group.create",
                    json!({"members":[],"parent_id":focus.unwrap_or(0)}),
                ));
                ui.close();
            }
            if let Some(id) = focus {
                if ui.button(tr("Wrap in New Group")).clicked() {
                    pending.push(crate::application::Action::new(
                        "group.create",
                        json!({"members":[{"kind":"group","id":id}]}),
                    ));
                    ui.close();
                }
                if ui.button(tr("Move Selected Layers Here")).clicked() {
                    let members = self
                        .session
                        .doc
                        .selected_layer_ids()
                        .iter()
                        .map(|id| json!({"kind":"layer","id":id}))
                        .collect::<Vec<_>>();
                    if let Ok(data) = self.group_query_ui(ui.ctx(), &LayerFilter::default()) {
                        let selected = self.session.doc.selected_layer_ids();
                        let index =
                            data.groups
                                .iter()
                                .find(|group| group.id == id)
                                .map_or(0, |group| {
                                    group
                                        .children
                                        .iter()
                                        .filter(|child| {
                                            child.kind != "layer" || !selected.contains(&child.id)
                                        })
                                        .count()
                                });
                        pending.push(crate::application::Action::new(
                            "group.move",
                            json!({"items":members,"parent_id":id,"index":index}),
                        ));
                        ui.close();
                    }
                }
            }
            if !enabled {
                ui.weak(tr("Groups require linked layer filters."));
            }
        })
        .0
        .on_hover_text(tr("Layer Groups"));
    }
    fn group_move_menu_ui(
        &self,
        ui: &mut Ui,
        group: &Group,
        data: &Data,
        pending: &mut Vec<crate::application::Action>,
    ) {
        ui.menu_button(tr("Move to Group"),|ui| {
            let descendants=|candidate:u64| {
                let mut next=candidate;
                for _ in 0..=16 {
                    if next==group.id {return true;}
                    let Some(parent)=data.groups.iter().find(|g|g.id==next).map(|g|g.parent_id) else {break;};
                    if parent==0 {break;} next=parent;
                }
                false
            };
            for destination in std::iter::once(None).chain(data.groups.iter().map(Some)) {
                let (id,count,title)=if let Some(destination)=destination {
                    let mut names=vec![destination.name.as_str()];
                    let mut parent=destination.parent_id;
                    for _ in 0..16 {
                        let Some(group)=data.groups.iter().find(|g|g.id==parent) else {break;};
                        names.push(group.name.as_str());parent=group.parent_id;
                    }
                    names.reverse();
                    (destination.id,destination.children.len(),names.join(" / "))
                } else {(0,data.roots.len(),localize("Root Stack").into_owned())};
                if ui.add_enabled(id!=group.parent_id && !descendants(id),Button::new(&title)).clicked() {
                    pending.push(crate::application::Action::new("group.move",json!({"items":[{"kind":"group","id":group.id}],"parent_id":id,"index":count})));ui.close();
                }
            }
        });
    }
    fn group_opacity_ui(&self, ui: &mut Ui, group: &Group) -> Option<OpacityCommit> {
        let key = opacity_key(self.session.doc.identity, group.id);
        let mut draft = ui.data_mut(|data| data.get_temp::<OpacityDraft>(key));
        let stale = draft.as_ref().is_some_and(|draft| {
            draft.generation != self.session.doc.groups_revision()
                || draft.revision != self.session.revision
        });
        let mut value = draft.as_ref().map_or(group.opacity, |draft| draft.value);
        let a = &self.settings.ui;
        let width = ui.available_width();
        let label_width = (width * 0.32).clamp(64., 104.);
        let number_width = 58.;
        let gap = a.item_spacing;
        let narrow = width < label_width + number_width + gap * 2. + 60.;
        let mut slider = None;
        let mut number = None;
        ui.add_enabled_ui(!stale, |ui| {
            ui.push_id(("group-opacity-field", group.id), |ui| {
                ui.horizontal(|ui| {
                    let label_width = if narrow {
                        width - number_width - gap
                    } else {
                        label_width
                    };
                    ui.add_sized(
                        [label_width, a.control_height],
                        Label::new(tr(RichText::new(localize("Opacity"))
                            .small()
                            .color(ui.visuals().weak_text_color())))
                        .truncate(),
                    )
                    .on_hover_text(tr("Group opacity applies when the adjustment is released."));
                    if !narrow {
                        ui.spacing_mut().slider_width =
                            (width - label_width - number_width - gap * 2.).max(24.);
                        slider = Some(ui.add(Slider::new(&mut value, 0.0..=1.).show_value(false)));
                    }
                    let mut percent = value * 100.;
                    let response = ui.add_sized(
                        [number_width, a.control_height],
                        DragValue::new(&mut percent)
                            .range(0.0..=100.)
                            .speed(1.)
                            .max_decimals(1)
                            .suffix("%")
                            .update_while_editing(false),
                    );
                    if response.changed() {
                        value = percent / 100.;
                    }
                    number = Some(response);
                });
                if narrow {
                    ui.spacing_mut().slider_width = width;
                    slider = Some(ui.add(Slider::new(&mut value, 0.0..=1.).show_value(false)));
                }
            });
        });
        let slider = slider.unwrap();
        let number = number.unwrap();
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                Id::new(("group-opacity-slider", self.session.doc.identity, group.id)),
                slider.rect,
            )
        });
        if stale {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Group changed during the opacity adjustment. Reset the draft to continue."),
            );
            if ui.small_button(tr("Reset Opacity Draft")).clicked() {
                ui.data_mut(|data| data.remove::<OpacityDraft>(key));
            }
            return None;
        }
        let dragging = slider.is_pointer_button_down_on()
            || slider.dragged()
            || number.is_pointer_button_down_on()
            || number.dragged()
            || ui.input(|input| input.pointer.primary_down())
                && (slider.hovered() || number.hovered());
        let changed = slider.changed() || number.changed();
        let released = slider.drag_stopped()
            || number.drag_stopped()
            || draft
                .as_ref()
                .is_some_and(|draft| draft.dragging && !ui.input(|input| input.pointer.any_down()));
        if changed || dragging {
            let entry = draft.get_or_insert(OpacityDraft {
                value: group.opacity,
                revision: self.session.revision,
                generation: self.session.doc.groups_revision(),
                dragging: false,
                submitted: false,
            });
            entry.value = value;
            entry.dragging = dragging;
            if changed {
                entry.submitted = false;
            }
        }
        let commit = (released || changed && !dragging)
            && draft.as_ref().is_some_and(|draft| !draft.submitted);
        if let Some(mut draft) = draft {
            draft.dragging = dragging && !released;
            if commit {
                if (draft.value * 255.).round() == (group.opacity * 255.).round() {
                    ui.data_mut(|data| data.remove::<OpacityDraft>(key));
                    return None;
                }
                draft.submitted = true;
                let result = OpacityCommit {
                    value: draft.value,
                    revision: draft.revision,
                };
                ui.data_mut(|data| data.insert_temp(key, draft));
                return Some(result);
            }
            ui.data_mut(|data| data.insert_temp(key, draft));
        }
        None
    }
    pub(super) fn group_properties_ui(
        &mut self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) -> bool {
        let Some(id) = self.focused_group_ui(ui.ctx()) else {
            return false;
        };
        let data = match self.group_query_ui(ui.ctx(), &LayerFilter::default()) {
            Ok(data) => data,
            Err(error) => {
                ui.colored_label(ui.visuals().error_fg_color, localize(&error));
                return true;
            }
        };
        let Some(group) = data.groups.iter().find(|group| group.id == id) else {
            self.focus_group_ui(ui.ctx(), None);
            return false;
        };
        let a = self.settings.ui.clone();
        let pending_start = pending.len();
        ui.strong(tr("Group Properties"));
        ui.weak(format!(
            "{}: {}",
            localize("Painting Layer"),
            self.session.doc.layer_name(self.session.doc.active)
        ));
        if ui.small_button(tr("Inspect Painting Layer")).clicked() {
            self.focus_group_ui(ui.ctx(), None);
            return false;
        }
        let key = name_key(self.session.doc.identity, id);
        let draft = ui.data_mut(|data| data.get_temp::<NameDraft>(key));
        let stale = draft
            .as_ref()
            .is_some_and(|draft| draft.generation != self.session.doc.groups_revision());
        let mut name = draft
            .as_ref()
            .map(|draft| draft.text.clone())
            .unwrap_or_else(|| group.name.clone());
        let response = ui.add(
            TextEdit::singleline(&mut name)
                .id(key)
                .desired_width(f32::INFINITY)
                .char_limit(120)
                .hint_text(crate::i18n::text("Group Name")),
        );
        if response.changed() {
            let original = draft.clone().unwrap_or_else(|| NameDraft {
                text: String::new(),
                generation: self.session.doc.groups_revision(),
            });
            ui.data_mut(|data| {
                data.insert_temp(
                    key,
                    NameDraft {
                        text: name.clone(),
                        ..original
                    },
                )
            });
        }
        let mut changes = json!({"group_id":id});
        // egui normally drops text focus on release; commit before a neighboring drag starts.
        let property_bounds = ui.max_rect().intersect(ui.clip_rect());
        let pressed_elsewhere = response.has_focus()
            && ui.input(|input| {
                input.pointer.any_pressed()
                    && input.pointer.interact_pos().is_some_and(|position| {
                        property_bounds.contains(position) && !response.rect.contains(position)
                    })
            });
        let commit = pressed_elsewhere
            || response.lost_focus()
            || response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
        if pressed_elsewhere {
            response.surrender_focus();
        }
        if response.has_focus() && ui.input(|input| input.key_pressed(Key::Escape)) {
            response.surrender_focus();
            ui.data_mut(|data| data.remove::<NameDraft>(key));
        } else if commit && !stale && name != group.name && !name.trim().is_empty() {
            changes["name"] = json!(name);
        }
        if stale {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Group changed externally. Reload its name before applying."),
            );
            if ui.small_button(tr("Reload Group Name")).clicked() {
                ui.data_mut(|data| data.remove::<NameDraft>(key));
            }
        }
        ui.add_space(a.item_spacing);
        let mut opacity_revision = None;
        ui.add_enabled_ui(!stale, |ui| {
            let mut visible = group.visible;
            if ui.checkbox(&mut visible, tr("Visible")).changed() {
                changes["visible"] = json!(visible);
            }
            if let Some(opacity) = self.group_opacity_ui(ui, group) {
                changes["opacity"] = json!(opacity.value);
                opacity_revision = Some(opacity.revision);
            }
            ui.label(tr("Group Compositing"));
            let policy_response = ComboBox::from_id_salt(("group-compositing", id))
                .width(ui.available_width())
                .height(a.control_height * 3. + a.item_spacing * 2.)
                .truncate()
                .selected_text(tr(group.compositing.title()))
                .show_ui(ui, |ui| {
                    for policy in [GroupPolicy::Isolated, GroupPolicy::PassThrough] {
                        let available =
                            policy == GroupPolicy::Isolated || data.pass_through_available;
                        if ui
                            .add_enabled(
                                available,
                                Button::selectable(group.compositing == policy, tr(policy.title()))
                                    .min_size(vec2(ui.available_width(), a.control_height)),
                            )
                            .on_disabled_hover_text(tr(
                                "Pass Through requires a compatible compositor.",
                            ))
                            .clicked()
                        {
                            changes.as_object_mut().unwrap().remove("opacity");
                            opacity_revision = None;
                            changes["compositing"] = json!(policy);
                            ui.close();
                        }
                    }
                })
                .response;
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(
                    Id::new(("group-policy-control", self.session.doc.identity, id)),
                    policy_response.rect,
                )
            });
            #[cfg(not(test))]
            let _ = policy_response;
            ui.label(tr("Group Blend Mode"));
            ui.add_enabled_ui(group.compositing == GroupPolicy::Isolated, |ui| {
                let choice = super::super::blend_modes::combo(
                    ui,
                    ("group-mode", id),
                    group.blend_mode,
                    false,
                    ui.available_width(),
                    super::super::blend_modes::supported(&self.session.doc),
                );
                #[cfg(test)]
                ui.data_mut(|data| {
                    data.insert_temp(
                        Id::new(("group-blend-control", self.session.doc.identity, id)),
                        (choice.response.rect, choice.response.enabled()),
                    )
                });
                if let Some(mode) = choice.inner {
                    changes["blend_mode"] = json!(mode);
                }
            })
            .response
            .on_disabled_hover_text(tr(
                "Pass Through uses child blend modes. The isolated group blend mode is retained.",
            ));
        });
        ui.weak(tr(if group.compositing == GroupPolicy::PassThrough {
            "Children blend into the parent backdrop; group opacity controls the combined change."
        } else {
            "Children composite together before group opacity and blending."
        }));
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !group.layer_ids.is_empty(),
                    Button::new(tr("Select Group Layers")).wrap(),
                )
                .clicked()
            {
                pending.push(crate::application::Action::new(
                    "group.select_layers",
                    json!({"group_id":id}),
                ));
            }
            ui.menu_button(tr("Group Actions"), |ui| {
                if ui.button(tr("New Layer in Group")).clicked() {
                    pending.push(crate::application::Action::new(
                        "layer.add",
                        json!({"parent_id":id}),
                    ));
                    ui.close();
                }
                self.group_move_menu_ui(ui, group, &data, pending);
                for (command, title) in [
                    ("group.duplicate", "Duplicate Group"),
                    ("group.merge", "Merge Group"),
                    ("group.ungroup", "Ungroup"),
                    ("group.delete", "Delete Group and Contents"),
                ] {
                    let enabled = match command {
                        "group.merge" => group.mergeable,
                        "group.delete" => group.layer_ids.len() < self.session.doc.layers.len(),
                        "group.duplicate" => {
                            self.session.doc.layers.len() + group.layer_ids.len() <= 16
                        }
                        _ => true,
                    };
                    let response = ui.add_enabled(enabled, Button::new(tr(title)).wrap());
                    let response = if command == "group.merge" {
                        if let Some(error) = &group.merge_error {
                            response.on_disabled_hover_text(localize(error))
                        } else {
                            response
                        }
                    } else {
                        response
                    };
                    if response.clicked() {
                        pending.push(crate::application::Action::new(
                            command,
                            json!({"group_id":id}),
                        ));
                        ui.close();
                    }
                }
            });
        });
        if changes.as_object().unwrap().len() > 1 {
            let mut action = crate::application::Action::new("group.set", changes);
            action.expected_revision = opacity_revision;
            pending.insert(pending_start, action);
        }
        ui.add_space(a.section_spacing);
        true
    }
    pub(super) fn group_row_ui(
        &mut self,
        ui: &mut Ui,
        row: &Row,
        filtering: bool,
        pending: &mut Vec<crate::application::Action>,
    ) {
        let a = self.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        let width = ui
            .available_width()
            .min((ui.clip_rect().right() - ui.next_widget_position().x).max(0.));
        let (_, rect) = ui.allocate_space(vec2(width, a.control_height + a.panel_padding));
        let response = ui.interact(
            rect,
            ui.id()
                .with(("group-row", self.session.doc.identity, row.id)),
            Sense::click(),
        );
        let focused = self.focused_group_ui(ui.ctx()) == Some(row.id);
        ui.painter().rect_filled(
            rect,
            a.corner_radius,
            if focused {
                colors.tint(0.15)
            } else if response.hovered() {
                colors.raised
            } else {
                colors.panel
            },
        );
        let indent = (row.depth as f32 * (a.panel_padding + a.item_spacing))
            .min((width - a.control_height * 4.).max(0.));
        let disclosure = Rect::from_min_size(
            rect.min + vec2(indent, a.item_spacing * 0.5),
            Vec2::splat(a.control_height),
        );
        let key = workspace_key(self.session.doc.identity);
        let mut workspace = ui
            .data_mut(|data| data.get_temp::<Workspace>(key))
            .unwrap_or_default();
        let open = filtering || !workspace.collapsed.contains(&row.id);
        let toggle = ui.interact(
            disclosure,
            ui.id()
                .with(("group-disclosure", self.session.doc.identity, row.id)),
            Sense::click(),
        );
        let c = disclosure.center();
        let span = a.icon_size * 0.22;
        let points = if open {
            [
                c + vec2(-span, -span * 0.5),
                c + vec2(0., span * 0.5),
                c + vec2(span, -span * 0.5),
            ]
        } else {
            [
                c + vec2(-span * 0.5, -span),
                c + vec2(span * 0.5, 0.),
                c + vec2(-span * 0.5, span),
            ]
        };
        ui.painter()
            .add(Shape::line(points.to_vec(), Stroke::new(1.5, colors.muted)));
        if toggle.clicked() && !filtering {
            if open {
                workspace.collapsed.insert(row.id);
            } else {
                workspace.collapsed.remove(&row.id);
            }
            ui.data_mut(|data| data.insert_temp(key, workspace));
        }
        let icon = Rect::from_min_size(
            pos2(disclosure.right(), rect.center().y - a.icon_size * 0.5),
            Vec2::splat(a.icon_size),
        );
        ui.painter().text(
            icon.center(),
            Align2::CENTER_CENTER,
            '\u{e53b}',
            FontId::new(a.icon_size, FontFamily::Name("Material".into())),
            colors.muted,
        );
        layer_text(
            ui,
            row.name.clone(),
            Rect::from_min_max(
                pos2(icon.right() + a.item_spacing, rect.top()),
                pos2(
                    rect.right() - a.panel_padding - a.control_height * 1.8,
                    rect.bottom(),
                ),
            ),
            theme::strong(ui.ctx(), a.font_size),
            if row.effective_visible {
                colors.text
            } else {
                colors.muted
            },
        );
        ui.painter().text(
            pos2(rect.right() - a.control_height * 1.35, rect.center().y),
            Align2::CENTER_CENTER,
            row.layer_ids.len(),
            FontId::proportional(a.small_font_size),
            colors.muted,
        );
        let eye = Rect::from_min_size(
            pos2(
                rect.right() - a.control_height,
                rect.center().y - a.control_height * 0.5,
            ),
            Vec2::splat(a.control_height),
        );
        let eye_response = ui.interact(
            eye,
            ui.id()
                .with(("group-eye", self.session.doc.identity, row.id)),
            Sense::click(),
        );
        ui.painter().circle_stroke(
            eye.center(),
            4.,
            Stroke::new(
                1.,
                if row.visible {
                    colors.accent
                } else {
                    colors.muted
                },
            ),
        );
        if row.visible {
            ui.painter().circle_filled(eye.center(), 2., colors.accent);
        }
        if eye_response.clicked() {
            pending.push(crate::application::Action::new(
                "group.set",
                json!({"group_id":row.id,"visible":!row.visible}),
            ));
        }
        if response.clicked() && !toggle.clicked() && !eye_response.clicked() {
            self.focus_group_ui(ui.ctx(), Some(row.id));
        }
        response.on_hover_text(format!(
            "{}\n{}",
            row.name,
            localize("Inspect group properties; painting stays on the current raster layer.")
        ));
    }
    pub(super) fn dispatch_group_ui(
        &mut self,
        ctx: &Context,
        mut action: crate::application::Action,
    ) -> bool {
        if action.expected_revision.is_none() {
            action.expected_revision = Some(self.session.revision);
        }
        let command = action.action.clone();
        let edited = action.args["group_id"].as_u64();
        let rename = action.args.get("name").is_some();
        let opacity = action.args.get("opacity").is_some();
        let policy = action.args.get("compositing").is_some();
        let before_revision = self.session.revision;
        let before_generation = self.session.doc.groups_revision();
        match self.session.apply(action) {
            Ok(response) => {
                if command == "group.set" {
                    if let Some(id) = edited {
                        let key = opacity_key(self.session.doc.identity, id);
                        if opacity || policy {
                            ctx.data_mut(|data| data.remove::<OpacityDraft>(key));
                        } else if let Some(mut draft) =
                            ctx.data_mut(|data| data.get_temp::<OpacityDraft>(key))
                        {
                            if draft.revision == before_revision
                                && draft.generation == before_generation
                            {
                                draft.revision = self.session.revision;
                                draft.generation = self.session.doc.groups_revision();
                                ctx.data_mut(|data| data.insert_temp(key, draft));
                            }
                        }
                    }
                }
                if matches!(
                    command.as_str(),
                    "group.create" | "group.create_selected" | "group.duplicate"
                ) {
                    if let Some(id) = response["result"]["group_id"].as_u64() {
                        self.focus_group_ui(ctx, Some(id));
                    }
                }
                if matches!(
                    command.as_str(),
                    "group.delete" | "group.ungroup" | "group.select_layers" | "group.merge"
                ) {
                    self.focus_group_ui(ctx, None);
                }
                if rename {
                    if let Some(id) = edited {
                        ctx.data_mut(|data| {
                            data.remove::<NameDraft>(name_key(self.session.doc.identity, id))
                        });
                    }
                }
                self.sync_from_session();
                true
            }
            Err(error) => {
                self.status = error.to_string();
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::studio;
    fn fixture() -> (Studio, u64, Vec<u64>) {
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
        for name in ["Ink", "Shade", "Paper"] {
            if name != "Ink" {
                assert!(app.editor.core("layer.add", json!({})));
            }
            assert!(app.editor.core("layer.rename", json!({"name":name})));
        }
        let ids = (0..3)
            .map(|i| app.editor.session.doc.layer_id(i))
            .collect::<Vec<_>>();
        let created=app.editor.session.apply(crate::application::Action::new("group.create",json!({"members":[{"kind":"layer","id":ids[0]},{"kind":"layer","id":ids[1]}],"name":"Ink Group"}))).unwrap();
        (app, created["result"]["group_id"].as_u64().unwrap(), ids)
    }
    fn frame(
        app: &mut Studio,
        ctx: &Context,
        size: Vec2,
        events: Vec<Event>,
        time: f64,
    ) -> FullOutput {
        crate::i18n::set_language(app.editor.settings.ui.language);
        if !ctx.data_mut(|data| {
            data.get_temp::<bool>(Id::new("group-fonts"))
                .unwrap_or_default()
        }) {
            theme::fonts(ctx);
            theme::apply(ctx, &app.editor.settings.ui);
            ctx.data_mut(|data| data.insert_temp(Id::new("group-fonts"), true));
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
            Shape::Text(text_shape) if text_shape.galley.text() == text => {
                Some(text_shape.pos + text_shape.galley.size() * 0.5)
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| label(shape, text)),
            _ => None,
        }
    }
    fn click(point: Pos2) -> Vec<Event> {
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
        ]
    }
    fn choose_policy(
        app: &mut Studio,
        ctx: &Context,
        size: Vec2,
        group: u64,
        policy: GroupPolicy,
        time: f64,
    ) {
        let rect = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "group-policy-control",
                    app.editor.session.doc.identity,
                    group,
                )))
            })
            .unwrap();
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
        frame(app, ctx, size, click(rect.center()), time);
        let output = frame(app, ctx, size, vec![], time + 1.);
        let title = crate::i18n::text_in(app.editor.settings.ui.language, policy.title());
        let point = output
            .shapes
            .iter()
            .find_map(|shape| {
                label(&shape.shape, &title).map(|point| {
                    assert!(shape.clip_rect.contains(point));
                    point
                })
            })
            .unwrap();
        frame(app, ctx, size, click(point), time + 2.);
        frame(app, ctx, size, vec![], time + 3.);
    }
    #[test]
    fn pass_through_policy_controls_retain_modes_and_painting_focus_in_narrow_cjk_panels() {
        let previous = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            for width in [176., 320.] {
                let (mut app, group, ids) = fixture();
                app.editor.settings.ui.language = language;
                assert!(app.editor.core(
                    "group.set",
                    json!({"group_id":group,"blend_mode":"multiply"})
                ));
                let ctx = Context::default();
                let size = vec2(width, 1200.);
                app.editor.focus_group_ui(&ctx, Some(group));
                for n in 0..3 {
                    frame(&mut app, &ctx, size, vec![], n as f64);
                }
                let key = opacity_key(app.editor.session.doc.identity, group);
                ctx.data_mut(|data| {
                    data.insert_temp(
                        key,
                        OpacityDraft {
                            value: 0.25,
                            revision: app.editor.session.revision,
                            generation: app.editor.session.doc.groups_revision(),
                            dragging: false,
                            submitted: false,
                        },
                    )
                });
                let raw = app
                    .editor
                    .session
                    .doc
                    .layers
                    .iter()
                    .map(|layer| layer.pixels.clone())
                    .collect::<Vec<_>>();
                app.editor.session.replay_boundary().unwrap();
                let events = app
                    .editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len();
                let revision = app.editor.session.revision;
                choose_policy(&mut app, &ctx, size, group, GroupPolicy::PassThrough, 3.);
                let query = app
                    .editor
                    .group_query_ui(&ctx, &LayerFilter::default())
                    .unwrap();
                let target = query.groups.iter().find(|value| value.id == group).unwrap();
                assert_eq!(
                    target.compositing,
                    GroupPolicy::PassThrough,
                    "{language:?}/{width}: {}",
                    app.editor.status
                );
                assert_eq!(target.blend_mode, crate::engine::graph::BlendMode::Multiply);
                assert_eq!(target.opacity, 1.);
                assert!(!target.mergeable && target.merge_error.is_some());
                assert!(ctx
                    .data_mut(|data| data.get_temp::<OpacityDraft>(key))
                    .is_none());
                assert_eq!(app.editor.session.revision, revision + 1);
                let journal = app.editor.session.replay_journal(false).unwrap();
                assert_eq!(journal.entries.len(), events + 1);
                let recorded = journal.events(events, 1).unwrap();
                assert_eq!(recorded["events"][0]["source"]["action"], "group.set");
                assert_eq!(
                    recorded["events"][0]["source"]["args"],
                    json!({"group_id":group,"compositing":"pass_through"})
                );
                let (rect, enabled) = ctx
                    .data_mut(|data| {
                        data.get_temp::<(Rect, bool)>(Id::new((
                            "group-blend-control",
                            app.editor.session.doc.identity,
                            group,
                        )))
                    })
                    .unwrap();
                assert!(!enabled && Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
                frame(&mut app, &ctx, size, click(rect.center()), 7.);
                assert_eq!(app.editor.session.revision, revision + 1);
                choose_policy(&mut app, &ctx, size, group, GroupPolicy::Isolated, 8.);
                let query = app
                    .editor
                    .group_query_ui(&ctx, &LayerFilter::default())
                    .unwrap();
                let target = query.groups.iter().find(|value| value.id == group).unwrap();
                assert_eq!(target.compositing, GroupPolicy::Isolated);
                assert_eq!(target.blend_mode, crate::engine::graph::BlendMode::Multiply);
                assert!(target.mergeable);
                assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
                assert_eq!(
                    app.editor
                        .session
                        .doc
                        .layer_id(app.editor.session.doc.active),
                    ids[2]
                );
                assert_eq!(
                    app.editor
                        .session
                        .doc
                        .layers
                        .iter()
                        .map(|layer| layer.pixels.clone())
                        .collect::<Vec<_>>(),
                    raw
                );
                assert!(app.editor.core("history.undo", json!({})));
                assert_eq!(
                    app.editor.session.doc.groups_state()["groups"][0]["compositing"],
                    "pass_through"
                );
                assert!(app.editor.core("history.undo", json!({})));
                assert_eq!(
                    app.editor.session.doc.groups_state()["groups"][0]["compositing"],
                    "isolated"
                );
            }
        }
        crate::i18n::set_language(previous);
    }
    #[test]
    fn pass_through_external_edits_preserve_stale_name_and_opacity_drafts() {
        let (mut app, group, _) = fixture();
        let ctx = Context::default();
        let size = vec2(244., 1200.);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let identity = app.editor.session.doc.identity;
        let revision = app.editor.session.revision;
        let generation = app.editor.session.doc.groups_revision();
        ctx.data_mut(|data| {
            data.insert_temp(
                name_key(identity, group),
                NameDraft {
                    text: "My uncommitted name".into(),
                    generation,
                },
            );
            data.insert_temp(
                opacity_key(identity, group),
                OpacityDraft {
                    value: 0.25,
                    revision,
                    generation,
                    dragging: true,
                    submitted: false,
                },
            );
        });
        assert!(app.editor.core(
            "group.set",
            json!({"group_id":group,"compositing":"pass_through"})
        ));
        let state = app.editor.session.state();
        frame(&mut app, &ctx, size, vec![], 3.);
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(
            ctx.data_mut(|data| data.get_temp::<NameDraft>(name_key(identity, group)))
                .unwrap()
                .text,
            "My uncommitted name"
        );
        assert_eq!(
            ctx.data_mut(|data| data.get_temp::<OpacityDraft>(opacity_key(identity, group)))
                .unwrap()
                .value,
            0.25
        );
        let mut stale = crate::application::Action::new(
            "group.set",
            json!({"group_id":group,"compositing":"isolated"}),
        );
        stale.expected_revision = Some(revision);
        assert!(!app.editor.dispatch_group_ui(&ctx, stale));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
    }
    #[test]
    fn pass_through_unavailable_policy_stays_visible_without_dispatching() {
        struct Legacy(Arc<dyn crate::engine::graph::Renderer>);
        impl crate::engine::graph::Renderer for Legacy {
            fn supports_transparent(&self) -> bool {
                self.0.supports_transparent()
            }
            fn render(
                &self,
                nodes: &[micopi_plugin_api::CompositeNode],
                layers: &[micopi_plugin_api::CompositeLayer],
                width: u32,
                height: u32,
                output: &mut micopi_plugin_api::Pixels,
            ) -> Result<(), String> {
                self.0.render(nodes, layers, width, height, output)
            }
        }
        let (mut app, group, _) = fixture();
        app.editor.session.doc.renderer = Some(Arc::new(Legacy(
            app.editor.session.doc.renderer.clone().unwrap(),
        )));
        let ctx = Context::default();
        let size = vec2(244., 1200.);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        assert!(
            !app.editor
                .group_query_ui(&ctx, &LayerFilter::default())
                .unwrap()
                .pass_through_available
        );
        let state = app.editor.session.state();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        choose_policy(&mut app, &ctx, size, group, GroupPolicy::PassThrough, 3.);
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
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
    }
    #[test]
    fn pass_through_group_inspect_effect_keeps_local_brush_and_hand_contacts_alive() {
        for tool in [Tool::Brush, Tool::Hand] {
            let (mut app, group, ids) = fixture();
            app.editor.session.runtime = "desktop";
            let ctx = Context::default();
            let sample = crate::application::Pointer {
                phase: crate::application::PointerPhase::Down,
                x: 3.,
                y: 3.,
                pressure: 0.3,
                time: 0,
                pen: true,
                eraser: false,
                tool_override: Some(tool),
                view_x: Some(300.),
                view_y: Some(300.),
            };
            app.editor.session.pointer(sample).unwrap();
            app.editor.sync_from_session();
            app.editor.input_owned = true;
            app.editor.pen_active = true;
            app.editor.pressure = sample.pressure;
            app.editor.panning = (tool == Tool::Hand).then_some(pos2(300., 300.));
            let composition = app.editor.session.doc.composition();
            let raw = app
                .editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>();
            let parameters = app.editor.session.state()["parameters"].clone();
            let history = app.editor.session.doc.history_bytes();
            let events = app
                .editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len();
            app.editor
                .session
                .apply(crate::application::Action::new(
                    "group.inspect",
                    json!({"group_id":group}),
                ))
                .unwrap();
            assert_eq!(app.editor.focused_group_ui(&ctx), None);
            app.consume_ui_effects(&ctx);
            assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
            assert_eq!(app.editor.session.doc.composition(), composition);
            assert_eq!(
                app.editor
                    .session
                    .doc
                    .layers
                    .iter()
                    .map(|layer| layer.pixels.clone())
                    .collect::<Vec<_>>(),
                raw
            );
            assert_eq!(app.editor.session.state()["parameters"], parameters);
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_eq!(
                app.editor
                    .session
                    .doc
                    .layer_id(app.editor.session.doc.active),
                ids[2]
            );
            assert!(
                app.editor.session.stroke.active()
                    && app.editor.session.pointer_checkpoint.is_some()
            );
            assert!(app.editor.input_owned && app.editor.pen_active);
            assert_eq!(app.editor.pressure, 0.3);
            assert_eq!(app.editor.drawing, tool == Tool::Brush);
            assert_eq!(
                app.editor.panning,
                (tool == Tool::Hand).then_some(pos2(300., 300.))
            );
            let journal = app.editor.session.replay_journal(false).unwrap();
            let recorded = journal
                .events(events, journal.entries.len() - events)
                .unwrap();
            let mut samples = recorded["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["source"]["kind"] == "pointer")
                .flat_map(|event| event["source"]["samples"].as_array().unwrap().clone())
                .collect::<Vec<_>>();
            samples.extend(
                serde_json::to_value(&app.editor.session.replay_samples)
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .clone(),
            );
            assert_eq!(samples, vec![serde_json::to_value(sample).unwrap()]);
            app.editor
                .session
                .pointer(crate::application::Pointer {
                    phase: crate::application::PointerPhase::Move,
                    x: 5.,
                    pressure: 0.85,
                    time: 20,
                    ..sample
                })
                .unwrap();
            assert!(app.editor.session.stroke.active());
            app.editor
                .session
                .pointer(crate::application::Pointer {
                    phase: crate::application::PointerPhase::Up,
                    x: 6.,
                    pressure: 0.4,
                    time: 40,
                    ..sample
                })
                .unwrap();
            assert!(!app.editor.session.stroke.active());
        }
    }
    #[test]
    fn pass_through_group_inspect_ignores_a_queued_previous_document_identity() {
        let (mut app, group, _) = fixture();
        app.editor.session.runtime = "desktop";
        let ctx = Context::default();
        let identity = app.editor.session.doc.identity;
        app.editor
            .session
            .apply(crate::application::Action::new(
                "group.inspect",
                json!({"group_id":group}),
            ))
            .unwrap();
        let effect = app.editor.session.ui_effects.pop_back().unwrap();
        assert_eq!(effect["document_identity"], identity);
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        let created = app
            .editor
            .session
            .apply(crate::application::Action::new(
                "group.create",
                json!({"members":[],"name":"New document group"}),
            ))
            .unwrap();
        assert_eq!(created["result"]["group_id"], group);
        assert_ne!(app.editor.session.doc.identity, identity);
        app.editor.session.ui_effects.push_back(effect);
        app.consume_ui_effects(&ctx);
        assert_eq!(app.editor.focused_group_ui(&ctx), None);
    }
    #[test]
    fn group_inspection_disclosure_and_shared_filter_preserve_raster_selection_and_pixels() {
        let (mut app, group, ids) = fixture();
        assert!(app.editor.core(
            "layer.selection.set",
            json!({"layer_ids":[ids[0],ids[2]],"active_layer_id":ids[2]})
        ));
        let state = app.editor.session.state();
        let history = app.editor.session.doc.history_bytes();
        let pixels = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        let mut output = frame(&mut app, &ctx, size, vec![], 0.);
        for n in 1..3 {
            output = frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let search = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, "Find Layers"))
            .unwrap();
        let point = output
            .shapes
            .iter()
            .filter_map(|shape| label(&shape.shape, "Ink Group"))
            .find(|point| point.y > search.y)
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
            3.,
        );
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
        let output = frame(&mut app, &ctx, size, vec![], 4.);
        assert!(output
            .shapes
            .iter()
            .any(|shape| label(&shape.shape, "Painting Layer: Paper").is_some()));
        let key = workspace_key(app.editor.session.doc.identity);
        let mut workspace = ctx
            .data_mut(|data| data.get_temp::<Workspace>(key))
            .unwrap();
        workspace.collapsed.insert(group);
        ctx.data_mut(|data| data.insert_temp(key, workspace));
        let collapsed = app
            .editor
            .group_listing_ui(&ctx, &LayerFilter::default())
            .unwrap();
        assert_eq!(&*collapsed.layers, &[2]);
        let filter = LayerFilter {
            query: "Ink Group".into(),
            ..Default::default()
        };
        let expanded = app.editor.group_listing_ui(&ctx, &filter).unwrap();
        assert!(expanded.layers.contains(&0) && expanded.layers.contains(&1));
        assert_eq!(
            &*app
                .editor
                .group_listing_ui(&ctx, &LayerFilter::default())
                .unwrap()
                .layers,
            &[2]
        );
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
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
    }
    #[test]
    fn group_name_drafts_reject_external_tree_changes_without_overwriting_artist_text() {
        let (mut app, group, _) = fixture();
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let key = name_key(app.editor.session.doc.identity, group);
        ctx.data_mut(|data| {
            data.insert_temp(
                key,
                NameDraft {
                    text: "My pending name".into(),
                    generation: app.editor.session.doc.groups_revision(),
                },
            )
        });
        assert!(app.editor.core(
            "group.set",
            json!({"group_id":group,"name":"External name","opacity":0.5})
        ));
        ctx.memory_mut(|memory| memory.request_focus(key));
        let output = frame(
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
            4.,
        );
        assert!(output.shapes.iter().any(|shape| label(
            &shape.shape,
            "Group changed externally. Reload its name before applying."
        )
        .is_some()));
        assert_eq!(
            ctx.data_mut(|data| data.get_temp::<NameDraft>(key))
                .unwrap()
                .text,
            "My pending name"
        );
        let data = app
            .editor
            .group_query_ui(&ctx, &LayerFilter::default())
            .unwrap();
        assert_eq!(
            data.groups
                .iter()
                .find(|value| value.id == group)
                .unwrap()
                .name,
            "External name"
        );
        assert!(app.editor.dispatch_group_ui(
            &ctx,
            crate::application::Action::new("group.duplicate", json!({"group_id":group}))
        ));
        let copy = app.editor.focused_group_ui(&ctx).unwrap();
        assert_ne!(copy, group);
        assert!(app.editor.dispatch_group_ui(
            &ctx,
            crate::application::Action::new("group.ungroup", json!({"group_id":copy}))
        ));
        assert_eq!(app.editor.focused_group_ui(&ctx), None);
    }
    #[test]
    fn group_merge_menu_targets_inspected_group_and_returns_to_raster_properties() {
        let (mut app, group, ids) = fixture();
        assert!(app.editor.core("layer.select", json!({"layer_id":ids[0]})));
        assert!(app
            .editor
            .core("layer.fill_color", json!({"color":"#cc3366"})));
        assert!(app.editor.core("layer.select", json!({"layer_id":ids[2]})));
        assert!(app
            .editor
            .core("layer.fill_color", json!({"color":"#99bbdd"})));
        let paper = app.editor.session.doc.layers[app.editor.session.doc.active]
            .pixels
            .clone();
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        app.editor.focus_group_ui(&ctx, Some(group));
        let before = app.editor.session.doc.composition();
        let mut stale = crate::application::Action::new("group.merge", json!({"group_id":group}));
        stale.expected_revision = Some(app.editor.session.revision - 1);
        assert!(!app.editor.dispatch_group_ui(&ctx, stale));
        assert!(app.editor.status.contains("revision_conflict"));
        assert_eq!(app.editor.session.doc.composition(), before);
        assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active),
            ids[2]
        );
        let mut output = frame(&mut app, &ctx, size, vec![], 0.);
        for n in 1..3 {
            output = frame(&mut app, &ctx, size, vec![], n as f64);
        }
        assert!(output
            .shapes
            .iter()
            .any(|shape| label(&shape.shape, "Painting Layer: Paper").is_some()));
        let menu = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, "Group Actions"))
            .unwrap();
        let click = |point| {
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
            ]
        };
        frame(&mut app, &ctx, size, click(menu), 3.);
        let output = frame(&mut app, &ctx, size, vec![], 4.);
        let merge = output
            .shapes
            .iter()
            .find_map(|shape| label(&shape.shape, "Merge Group"))
            .unwrap();
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let revision = app.editor.session.revision;
        frame(&mut app, &ctx, size, click(merge), 5.);
        assert_eq!(
            app.editor.session.revision,
            revision + 1,
            "{}",
            app.editor.status
        );
        assert_eq!(app.editor.focused_group_ui(&ctx), None);
        assert!(!app.editor.session.doc.has_groups());
        assert_eq!(app.editor.session.doc.layers.len(), 2);
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_name(app.editor.session.doc.active),
            "Ink Group"
        );
        assert!(!ids.contains(
            &app.editor
                .session
                .doc
                .layer_id(app.editor.session.doc.active)
        ));
        assert_eq!(
            app.editor.session.doc.layers[app.editor.session.doc.layer_index(ids[2]).unwrap()]
                .pixels,
            paper
        );
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        let recorded = journal.events(events, 1).unwrap();
        assert_eq!(recorded["events"][0]["source"]["action"], "group.merge");
        assert_eq!(
            recorded["events"][0]["source"]["args"],
            json!({"group_id":group})
        );
        let output = frame(&mut app, &ctx, size, vec![], 6.);
        assert!(!output
            .shapes
            .iter()
            .any(|shape| label(&shape.shape, "Group Properties").is_some()));
        assert!(output
            .shapes
            .iter()
            .any(|shape| label(&shape.shape, "Ink Group").is_some()));
    }
    #[test]
    fn empty_and_pass_through_group_merge_is_disabled_in_each_language() {
        let old = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            for pass_through in [false, true] {
                let (mut app, existing, _) = fixture();
                let group = if pass_through {
                    assert!(app.editor.core(
                        "group.set",
                        json!({"group_id":existing,"compositing":"pass_through"})
                    ));
                    existing
                } else {
                    app.editor
                        .session
                        .apply(crate::application::Action::new(
                            "group.create",
                            json!({"members":[],"name":"Empty"}),
                        ))
                        .unwrap()["result"]["group_id"]
                        .as_u64()
                        .unwrap()
                };
                app.editor.settings.ui.language = language;
                let ctx = Context::default();
                let size = vec2(244., 1100.);
                app.editor.focus_group_ui(&ctx, Some(group));
                let mut output = frame(&mut app, &ctx, size, vec![], 0.);
                for n in 1..3 {
                    output = frame(&mut app, &ctx, size, vec![], n as f64);
                }
                let title = crate::i18n::text_in(language, "Group Actions");
                let menu = output
                    .shapes
                    .iter()
                    .find_map(|shape| label(&shape.shape, &title))
                    .unwrap();
                let click = |point| {
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
                    ]
                };
                frame(&mut app, &ctx, size, click(menu), 3.);
                let output = frame(&mut app, &ctx, size, vec![], 4.);
                let title = crate::i18n::text_in(language, "Merge Group");
                let (clip, merge) = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        label(&shape.shape, &title).map(|point| (shape.clip_rect, point))
                    })
                    .unwrap();
                assert!(clip.contains(merge), "{language:?}: Merge Group is clipped");
                let before = app.editor.session.state();
                let events = app
                    .editor
                    .session
                    .replay_journal(false)
                    .unwrap()
                    .entries
                    .len();
                frame(&mut app, &ctx, size, click(merge), 5.);
                assert_eq!(app.editor.session.state(), before);
                assert_eq!(app.editor.focused_group_ui(&ctx), Some(group));
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
        }
        crate::i18n::set_language(old);
    }
    #[test]
    fn group_opacity_multiframe_drag_commits_one_revision_event_and_undo_on_release() {
        let (mut app, group, ids) = fixture();
        assert!(app.editor.core("layer.select", json!({"layer_id":ids[0]})));
        assert!(app
            .editor
            .core("layer.fill_color", json!({"color":"#336699"})));
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let slider = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "group-opacity-slider",
                    app.editor.session.doc.identity,
                    group,
                )))
            })
            .unwrap();
        let point =
            |fraction: f32| pos2(slider.left() + slider.width() * fraction, slider.center().y);
        app.editor.session.replay_boundary().unwrap();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let revision = app.editor.session.revision;
        let history = app.editor.session.doc.history_bytes();
        let before = app.editor.session.doc.composition();
        let pixels = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|layer| layer.pixels.clone())
            .collect::<Vec<_>>();
        for (n, fraction) in [0.9, 0.6, 0.35, 0.72].into_iter().enumerate() {
            let p = point(fraction);
            let mut input_events = vec![Event::PointerMoved(p)];
            if n == 0 {
                input_events.push(Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
            }
            frame(&mut app, &ctx, size, input_events, 3. + n as f64);
            assert_eq!(app.editor.session.revision, revision);
            assert_eq!(app.editor.session.doc.composition(), before);
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
        let p = point(0.72);
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                Event::PointerMoved(p),
                Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            7.,
        );
        assert_eq!(app.editor.session.revision, revision + 1);
        let journal = app.editor.session.replay_journal(false).unwrap();
        assert_eq!(journal.entries.len(), events + 1);
        let recorded = journal.events(events, 1).unwrap();
        assert_eq!(recorded["events"][0]["source"]["action"], "group.set");
        assert_eq!(recorded["events"][0]["source"]["args"]["group_id"], group);
        let opacity = app
            .editor
            .group_query_ui(&ctx, &LayerFilter::default())
            .unwrap()
            .groups
            .iter()
            .find(|value| value.id == group)
            .unwrap()
            .opacity;
        assert!(opacity > 0.6 && opacity < 0.85, "{opacity}");
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|layer| layer.pixels.clone())
                .collect::<Vec<_>>(),
            pixels
        );
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(app.editor.session.doc.composition(), before);
        assert!(ctx
            .data_mut(|data| data
                .get_temp::<OpacityDraft>(opacity_key(app.editor.session.doc.identity, group)))
            .is_none());
    }
    #[test]
    fn group_opacity_drag_acknowledges_its_pending_name_and_rejects_external_changes() {
        let (mut app, group, _) = fixture();
        let ctx = Context::default();
        let size = vec2(244., 1100.);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            frame(&mut app, &ctx, size, vec![], n as f64);
        }
        let name = name_key(app.editor.session.doc.identity, group);
        ctx.data_mut(|data| {
            data.insert_temp(
                name,
                NameDraft {
                    text: "Named before drag".into(),
                    generation: app.editor.session.doc.groups_revision(),
                },
            )
        });
        ctx.memory_mut(|memory| memory.request_focus(name));
        frame(&mut app, &ctx, size, vec![], 3.);
        assert!(ctx.memory(|memory| memory.has_focus(name)));
        let slider = ctx
            .data_mut(|data| {
                data.get_temp::<Rect>(Id::new((
                    "group-opacity-slider",
                    app.editor.session.doc.identity,
                    group,
                )))
            })
            .unwrap();
        let p = pos2(slider.center().x, slider.center().y);
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
            ],
            4.,
        );
        let key = opacity_key(app.editor.session.doc.identity, group);
        let draft = ctx
            .data_mut(|data| data.get_temp::<OpacityDraft>(key))
            .unwrap();
        assert_eq!(draft.revision, app.editor.session.revision);
        assert_eq!(draft.generation, app.editor.session.doc.groups_revision());
        assert_eq!(
            app.editor
                .group_query_ui(&ctx, &LayerFilter::default())
                .unwrap()
                .groups
                .iter()
                .find(|value| value.id == group)
                .unwrap()
                .name,
            "Named before drag"
        );
        assert!(app
            .editor
            .core("group.set", json!({"group_id":group,"opacity":0.4})));
        let state = app.editor.session.state();
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let output = frame(
            &mut app,
            &ctx,
            size,
            vec![Event::PointerButton {
                pos: p,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            5.,
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
        assert!(ctx
            .data_mut(|data| data.get_temp::<OpacityDraft>(key))
            .is_some());
        assert!(output.shapes.iter().any(|shape| label(
            &shape.shape,
            "Group changed during the opacity adjustment. Reset the draft to continue."
        )
        .is_some()));
    }
    #[test]
    fn group_name_draft_does_not_finish_a_new_canvas_contact_outside_its_panel() {
        fn panel_frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) {
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800., 1100.))),
                    events,
                    time: Some(time),
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    SidePanel::left("group-contact-properties")
                        .exact_width(244.)
                        .resizable(false)
                        .frame(Frame::NONE)
                        .show(ctx, |ui| app.editor.layers_ui(ui));
                    CentralPanel::default().show(ctx, |_| {});
                },
            );
        }
        let (mut app, group, _) = fixture();
        let ctx = Context::default();
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        app.editor.focus_group_ui(&ctx, Some(group));
        for n in 0..3 {
            panel_frame(&mut app, &ctx, vec![], n as f64);
        }
        let name = name_key(app.editor.session.doc.identity, group);
        ctx.data_mut(|data| {
            data.insert_temp(
                name,
                NameDraft {
                    text: "Pending while painting".into(),
                    generation: app.editor.session.doc.groups_revision(),
                },
            )
        });
        ctx.memory_mut(|memory| memory.request_focus(name));
        panel_frame(&mut app, &ctx, vec![], 3.);
        assert!(ctx.memory(|memory| memory.has_focus(name)));

        let sample = crate::application::Pointer {
            phase: crate::application::PointerPhase::Down,
            x: 4.,
            y: 4.,
            pressure: 0.65,
            time: 0,
            pen: true,
            eraser: false,
            tool_override: None,
            view_x: Some(500.),
            view_y: Some(300.),
        };
        app.editor.session.pointer(sample).unwrap();
        app.editor.sync_from_session();
        app.editor.input_owned = true;
        assert!(app.editor.session.is_drawing());
        let revision = app.editor.session.revision;
        let events = app
            .editor
            .session
            .replay_journal(false)
            .unwrap()
            .entries
            .len();
        let position = pos2(500., 300.);
        panel_frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            4.,
        );
        assert!(app.editor.input_owned);
        assert!(app.editor.drawing);
        assert!(app.editor.session.stroke.active());
        assert!(app.editor.session.pointer_checkpoint.is_some());
        assert_eq!(app.editor.session.revision, revision);
        assert_eq!(app.editor.session.replay_samples.len(), 1);
        assert_eq!(
            app.editor
                .session
                .replay_journal(false)
                .unwrap()
                .entries
                .len(),
            events
        );
        let draft = ctx
            .data_mut(|data| data.get_temp::<NameDraft>(name))
            .unwrap();
        assert_eq!(draft.text, "Pending while painting");
        assert_eq!(
            app.editor
                .group_query_ui(&ctx, &LayerFilter::default())
                .unwrap()
                .groups
                .iter()
                .find(|value| value.id == group)
                .unwrap()
                .name,
            "Ink Group"
        );
        app.editor
            .session
            .pointer(crate::application::Pointer {
                phase: crate::application::PointerPhase::Up,
                time: 1,
                ..sample
            })
            .unwrap();
    }
    #[test]
    fn group_sibling_moves_preserve_subtrees_and_disable_mixed_parent_layer_selections() {
        let (mut app, group, ids) = fixture();
        assert!(app
            .editor
            .group_sibling_move_ui(&[ids[0], ids[2]], true)
            .is_none());
        let move_up = app.editor.group_sibling_move_ui(&[ids[0]], true).unwrap();
        assert_eq!(move_up["parent_id"], group);
        assert_eq!(move_up["index"], 1);
        let ctx = Context::default();
        assert!(app
            .editor
            .dispatch_group_ui(&ctx, crate::application::Action::new("group.move", move_up)));
        let data = app
            .editor
            .group_query_ui(&ctx, &LayerFilter::default())
            .unwrap();
        let children = &data
            .groups
            .iter()
            .find(|value| value.id == group)
            .unwrap()
            .children;
        assert_eq!(
            children.iter().map(|child| child.id).collect::<Vec<_>>(),
            vec![ids[1], ids[0]]
        );
        assert_eq!(
            app.editor
                .session
                .doc
                .layer_parent_id(app.editor.session.doc.layer_index(ids[2]).unwrap()),
            0
        );
    }
    #[test]
    fn group_properties_and_hierarchy_fit_narrow_panels_in_all_four_languages() {
        let old = crate::i18n::current();
        for language in crate::i18n::Language::ALL {
            for width in [176., 244., 480.] {
                let (mut app, group, _) = fixture();
                app.editor.settings.ui.language = language;
                let ctx = Context::default();
                app.editor.focus_group_ui(&ctx, Some(group));
                let state = app.editor.session.state();
                let size = vec2(width, 1200.);
                for n in 0..3 {
                    frame(&mut app, &ctx, size, vec![], n as f64);
                }
                let output = frame(&mut app, &ctx, size, vec![], 4.);
                for source in ["Group Properties", "Find Layers", "Group Actions"] {
                    let title = crate::i18n::text_in(language, source);
                    let (clip, point) = output
                        .shapes
                        .iter()
                        .find_map(|shape| {
                            label(&shape.shape, &title).map(|point| (shape.clip_rect, point))
                        })
                        .unwrap_or_else(|| panic!("{language:?}: {source} absent"));
                    assert!(
                        clip.contains(point)
                            && Rect::from_min_size(Pos2::ZERO, size).contains(point),
                        "{language:?}: {source} at {width}: {point:?} vs {clip:?}"
                    );
                }
                assert_eq!(app.editor.session.state(), state);
            }
        }
        crate::i18n::set_language(old);
    }
}
