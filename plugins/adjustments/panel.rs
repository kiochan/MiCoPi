//! Non-raster stack inspection. Drafts remain local until one shared Apply.
use super::*;
use crate::engine::{adjustments::Adjustment, graph::Filter, raster_masks::MaskOwner};
use crate::plugins::nodes::{Binding, Resolved};
use serde_json::{json, Value};

#[derive(Clone)]
struct Draft {
    source: Adjustment,
    value: Adjustment,
    revision: u64,
    error: String,
}
fn draft_key(document: u64, id: u64) -> Id {
    Id::new(("adjustment-draft", document, id))
}
fn parameters(value: &Adjustment) -> Value {
    let mut result = crate::plugins::adjustments::item_parameters(value);
    // Apply is a partial action patch. Send explicit identity channels when
    // resetting a built-in draft so omitted defaults cannot retain old values.
    if value.node_type == "compositing.levels" {
        if let Some(levels) = value.levels {
            result["parameters"]["channels"] = json!(levels.channels);
        }
    }
    result
}
fn filter_definition(def: &Resolved) -> bool {
    matches!(&def.binding,Binding::Image{operation,..} if operation=="filter" || operation=="levels" || operation=="curves" || operation=="hsl" || operation=="quantization")
}
fn quantization_definition(def: &Resolved) -> bool {
    matches!(&def.binding,Binding::Image{operation,..} if operation=="quantization")
}
fn hsl_definition(def: &Resolved) -> bool {
    matches!(&def.binding,Binding::Image {operation,..} if operation=="hsl")
}
fn curves_definition(def: &Resolved) -> bool {
    matches!(&def.binding,Binding::Image {operation,..} if operation=="curves")
}
fn levels_definition(def: &Resolved) -> bool {
    matches!(&def.binding,Binding::Image {operation,..} if operation=="levels")
}
fn allowed(def: &Resolved, filter: Filter) -> Option<f32> {
    let (minimum, maximum) = amount_range(def, filter)?;
    let default = if def.defaults["filter"] == json!(filter) {
        def.defaults["amount"].as_f64().unwrap_or(0.) as f32
    } else if matches!(filter, Filter::Invert | Filter::Grayscale) {
        1.
    } else {
        0.
    };
    let value = default.clamp(minimum, maximum);
    crate::plugins::validate(
        &def.parameters_schema,
        &crate::plugins::adjustments::parameters(filter, value),
    )
    .ok()?;
    Some(value)
}
fn amount_range(def: &Resolved, filter: Filter) -> Option<(f32, f32)> {
    let schema = &def.parameters_schema["properties"]["amount"];
    let minimum = filter
        .minimum()
        .max(schema["minimum"].as_f64().unwrap_or(-1.) as f32);
    let maximum = 1f32.min(schema["maximum"].as_f64().unwrap_or(1.) as f32);
    (minimum.is_finite() && maximum.is_finite() && minimum <= maximum).then_some((minimum, maximum))
}

impl Editor {
    /// Resolve placement only at the explicit create/menu boundary. Painting
    /// remains attached to the current raster, even while another item is inspected.
    fn adjustment_placement_ui(&self, ctx: &Context) -> (u64, usize) {
        let state = self.session.doc.groups_state();
        if let Some(id) = self.focused_group_ui(ctx) {
            let count = state["groups"]
                .as_array()
                .and_then(|groups| groups.iter().find(|group| group["id"] == id))
                .and_then(|group| group["children"].as_array())
                .map_or(0, Vec::len);
            return (id, count);
        }
        let (kind, id) = self.focused_adjustment_ui(ctx).map_or(
            ("layer", self.session.doc.layer_id(self.session.doc.active)),
            |id| ("adjustment", id),
        );
        let parent = state["rows"]
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| row["kind"] == kind && row["id"] == id)
            })
            .and_then(|row| row["parent_id"].as_u64())
            .unwrap_or(0);
        let children = if parent == 0 {
            state["roots"].as_array()
        } else {
            state["groups"]
                .as_array()
                .and_then(|groups| groups.iter().find(|group| group["id"] == parent))
                .and_then(|group| group["children"].as_array())
        };
        let index = children
            .and_then(|children| {
                children
                    .iter()
                    .position(|child| child["kind"] == kind && child["id"] == id)
            })
            .map_or(self.session.doc.layers.len(), |index| index + 1);
        (parent, index)
    }

    pub(super) fn adjustment_create_menu_ui(
        &mut self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) {
        if !self
            .session
            .registry
            .actions
            .contains_key("adjustment.create")
        {
            return;
        }
        let available = self
            .session
            .doc
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.supports_mix());
        // Announce a bounded trigger to the wrapping toolbar before entering
        // an enabled scope. Long CJK labels must not widen the whole inspector.
        let a = &self.settings.ui;
        let title = ui.painter().layout_no_wrap(
            localize("New Adjustment").into_owned(),
            FontId::proportional(a.font_size),
            Color32::PLACEHOLDER,
        );
        let width = (title.size().x + ui.spacing().button_padding.x * 2.)
            .max(a.control_height)
            .min(ui.max_rect().width().min(ui.clip_rect().width()))
            .max(1.);
        let size = vec2(width, a.control_height);
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.scope_builder(UiBuilder::new().max_rect(rect).layout(Layout::left_to_right(Align::Center)),|ui|ui.add_enabled_ui(available,|ui|{
            egui::containers::menu::MenuButton::from_button(Button::new(tr("New Adjustment")).truncate().min_size(size))
                .config(egui::containers::menu::MenuConfig::new().close_behavior(PopupCloseBehavior::CloseOnClickOutside))
                .ui(ui,|ui|{
                ui.set_max_width((a.control_height*12.).min(ui.ctx().content_rect().width()-a.panel_padding*4.).max(a.control_height*4.));
                let key=Id::new(("adjustment-selection-mask",self.session.doc.identity));
                let mut selection=ui.data_mut(|data|data.get_temp::<bool>(key)).unwrap_or(false);
                if self.session.doc.selection().current.is_none() {selection=false;}
                if ui.add_enabled(self.session.doc.selection().current.is_some(),Checkbox::new(&mut selection,tr("Create Mask from Selection"))).changed() {
                    ui.data_mut(|data|data.insert_temp(key,selection));
                }
                ui.add(Label::new(tr("The selection is copied once; later selection changes do not affect this mask.")).wrap());
                ui.separator();
                let definitions=self.plugin_nodes.values().filter(|def|filter_definition(def)).cloned().collect::<Vec<_>>();
                for definition in definitions {
                    let title=crate::i18n::translated_for(Some(&definition.owner),&definition.title).into_owned();
                    if levels_definition(&definition) || curves_definition(&definition) || hsl_definition(&definition) || quantization_definition(&definition) {
                        let supported=self.session.doc.renderer.as_ref().is_some_and(|r|if quantization_definition(&definition) {r.supports_quantization()} else if hsl_definition(&definition) {r.supports_hsl()} else if curves_definition(&definition) {r.supports_curves()} else {r.supports_levels()});
                        if ui.add_enabled(supported,Button::new(title)).clicked() {
                            let (parent,index)=self.adjustment_placement_ui(ui.ctx());
                            let mut args=json!({"node_type":definition.id,"parameters":definition.defaults,"parent_id":parent,"index":index});
                            if selection {args["mask_source"]=json!("selection");}
                            pending.push(crate::application::Action::new("adjustment.create",args));ui.close();
                        }
                        continue;
                    }
                    let mut show=|ui:&mut Ui|{
                        for filter in Filter::ALL {
                            let Some(amount)=allowed(&definition,filter) else{continue;};
                            let choice=ui.button(tr(filter.title()));
                            #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("adjustment-create-choice",definition.id.as_str(),filter as u32)),choice.rect));
                            if choice.clicked() {
                                let (parent,index)=self.adjustment_placement_ui(ui.ctx());
                                let mut args=json!({"node_type":definition.id,"parameters":crate::plugins::adjustments::parameters(filter,amount),"parent_id":parent,"index":index});
                                if selection {args["mask_source"]=json!("selection");}
                                pending.push(crate::application::Action::new("adjustment.create",args));
                                ui.close();
                            }
                        }
                    };
                    if definition.id=="compositing.filter" {show(ui);}else{ui.menu_button(title,show);}
                }
            }).0.on_hover_text(tr("New Adjustment")).on_disabled_hover_text(tr("Adjustment layers require a compatible compositor."));
        }));
    }

    pub(super) fn adjustment_properties_ui(
        &mut self,
        ui: &mut Ui,
        pending: &mut Vec<crate::application::Action>,
    ) -> bool {
        let Some(id) = self.focused_adjustment_ui(ui.ctx()) else {
            return false;
        };
        let Ok(current) = self.session.doc.adjustment(id).cloned() else {
            self.focus_adjustment_ui(ui.ctx(), None);
            return false;
        };
        let a = self.settings.ui.clone();
        let key = draft_key(self.session.doc.identity, id);
        let mut draft = ui
            .data_mut(|data| data.get_temp::<Draft>(key))
            .unwrap_or(Draft {
                source: current.clone(),
                value: current,
                revision: self.session.revision,
                error: String::new(),
            });
        let changed = draft.value != draft.source;
        let stale = changed && draft.revision != self.session.revision;
        ui.strong(tr("Adjustment Properties"));
        ui.add(
            Label::new(format!(
                "{}: {}",
                localize("Painting Layer"),
                self.session.doc.layer_name(self.session.doc.active)
            ))
            .wrap(),
        );
        if ui.small_button(tr("Inspect Painting Layer")).clicked() {
            self.focus_adjustment_ui(ui.ctx(), None);
            return false;
        }
        let old = draft.value.clone();
        let mut valid = Ok(());
        ScrollArea::vertical().id_salt(("adjustment-properties",self.session.doc.identity,id))
            .max_height((ui.available_height()-a.control_height*7.-a.section_spacing).max(a.control_height*3.))
            .auto_shrink([false,true]).show(ui,|ui|{
                if draft.value.curves.is_some() || draft.value.levels.is_some() || draft.value.hsl.is_some() || draft.value.quantization.is_some() {super::nodes::histogram_ui::controls(ui,&mut self.session);}
                ui.add_enabled_ui(!stale,|ui|{
                    let _name=ui.add(TextEdit::singleline(&mut draft.value.name).id(Id::new(("adjustment-name",self.session.doc.identity,id))).char_limit(120).desired_width(f32::INFINITY).hint_text(crate::i18n::text("Adjustment Name")));
                    #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("adjustment-name",self.session.doc.identity,id)),_name.rect));
                    ui.horizontal_wrapped(|ui|{
                        ui.checkbox(&mut draft.value.visible,tr("Visible"));
                        ComboBox::from_id_salt(("adjustment-provider",id))
                            .width(ui.available_width().max(a.control_height*3.)).truncate()
                            .selected_text(self.plugin_nodes.get(&draft.value.node_type).map_or_else(||draft.value.node_type.clone(),|definition|crate::i18n::translated_for(Some(&definition.owner),&definition.title).into_owned()))
                            .show_ui(ui,|ui|{
                                for definition in self.plugin_nodes.values().filter(|definition|filter_definition(definition)) {
                                    let title=crate::i18n::translated_for(Some(&definition.owner),&definition.title);
                                    if ui.selectable_label(draft.value.node_type==definition.id,title).clicked()&&draft.value.node_type!=definition.id {
                                        if quantization_definition(definition) {
                                            if let Ok(quantization)=serde_json::from_value(definition.defaults["quantization"].clone()) {
                                                draft.value.node_type=definition.id.clone();draft.value.filter=Filter::Invert;draft.value.amount=0.;draft.value.levels=None;draft.value.curves=None;draft.value.hsl=None;draft.value.quantization=Some(quantization);
                                            }
                                        } else if hsl_definition(definition) {
                                            if let Ok(hsl)=serde_json::from_value::<crate::engine::hsl::Hsl>(definition.defaults["hsl"].clone()) {draft.value.node_type=definition.id.clone();draft.value.filter=Filter::Invert;draft.value.amount=0.;draft.value.levels=None;draft.value.curves=None;draft.value.hsl=Some(hsl);draft.value.quantization=None;}
                                        } else if curves_definition(definition) {
                                            if let Ok(curves)=serde_json::from_value::<crate::engine::curves::Curves>(definition.defaults["curves"].clone()) {
                                                draft.value.node_type=definition.id.clone();draft.value.filter=Filter::Invert;draft.value.amount=0.;draft.value.levels=None;draft.value.curves=Some(curves);draft.value.hsl=None;draft.value.quantization=None;
                                            }
                                        } else if levels_definition(definition) {
                                            if let Ok(levels)=serde_json::from_value::<crate::engine::levels::Levels>(definition.defaults["parameters"].clone()) {
                                                draft.value.node_type=definition.id.clone();draft.value.filter=Filter::Invert;draft.value.amount=0.;draft.value.levels=Some(levels);draft.value.curves=None;draft.value.hsl=None;draft.value.quantization=None;
                                            }
                                        }else{
                                        if let Some((filter,amount))=Filter::ALL.into_iter().find_map(|filter|allowed(definition,filter).map(|amount|(filter,amount))) {
                                            draft.value.node_type=definition.id.clone();draft.value.filter=filter;draft.value.amount=amount;draft.value.levels=None;draft.value.curves=None;draft.value.hsl=None;draft.value.quantization=None;
                                        }
                                        }
                                    }
                                }
                            });
                    });
                    if let Some(definition)=self.plugin_nodes.get(&draft.value.node_type).filter(|definition|filter_definition(definition)) {
                        if quantization_definition(definition) {
                            if let Some(quantization)=&mut draft.value.quantization {
                                if matches!(definition.id.as_str(),"compositing.threshold"|"compositing.posterize") {
                                    super::nodes::quantization_ui::edit(ui,Id::new(("adjustment-quantization",self.session.doc.identity,id)),quantization);
                                } else {
                                    let mut value=crate::plugins::adjustments::quantization_parameters(*quantization);
                                    super::schema::edit(ui,("adjustment-quantization",id),&definition.parameters_schema,&mut value);
                                    if let Ok(next)=serde_json::from_value(value["quantization"].clone()) {*quantization=next;}
                                }
                            }
                        } else if hsl_definition(definition) {
                            if let Some(hsl)=&mut draft.value.hsl {
                                if definition.id=="compositing.hsl" {super::nodes::hsl_ui::edit(ui,Id::new(("adjustment-hsl",self.session.doc.identity,id)),hsl);} else {
                                let mut value=crate::plugins::adjustments::hsl_parameters(*hsl);
                                super::schema::edit(ui,("adjustment-hsl",id),&definition.parameters_schema,&mut value);
                                if let Ok(next)=serde_json::from_value(value["hsl"].clone()) {*hsl=next;}
                                }
                            }
                        } else if curves_definition(definition) {
                            if let Some(curves)=&mut draft.value.curves {
                                if definition.id=="compositing.curves" {
                                    let histogram=super::nodes::histogram_ui::cached(ui.ctx(),&self.session);
                                    super::nodes::curves_ui::edit_with_histogram(ui,Id::new(("adjustment-curves",self.session.doc.identity,id)),curves,self.session.doc.renderer.as_deref(),self.session.stroke.active() || self.session.pointer_checkpoint.is_some(),histogram.as_deref());
                                } else {
                                    let mut value=crate::plugins::adjustments::curve_parameters(curves);
                                    super::schema::edit(ui,("adjustment-curves",id),&definition.parameters_schema,&mut value);
                                    if let Ok(next)=serde_json::from_value(value["curves"].clone()) {*curves=next;}
                                }
                            }
                        } else if levels_definition(definition) {
                            if let Some(levels)=&mut draft.value.levels {
                                if definition.id=="compositing.levels" {
                                    let histogram=super::nodes::histogram_ui::cached(ui.ctx(),&self.session);
                                    super::nodes::levels_ui::edit_with_histogram(ui,Id::new(("adjustment-levels",id)),levels,histogram.as_deref());
                                }else{
                                    let mut value=crate::plugins::adjustments::level_parameters(*levels);
                                    super::schema::edit(ui,("adjustment-levels",id),&definition.parameters_schema,&mut value);
                                    if let Ok(next)=serde_json::from_value(value["parameters"].clone()) {*levels=next;}
                                }
                            }
                        }else{
                        ComboBox::from_id_salt(("adjustment-filter",id)).width(ui.available_width()).truncate().selected_text(tr(draft.value.filter.title())).show_ui(ui,|ui|{
                            for filter in Filter::ALL {if let Some(amount)=allowed(definition,filter) {
                                if ui.selectable_label(draft.value.filter==filter,tr(filter.title())).clicked() {
                                    draft.value.filter=filter;draft.value.amount=amount;
                                }
                            }}
                        });
                        if let Some((minimum,maximum))=amount_range(definition,draft.value.filter) {
                            panels::parameter(ui,"Amount",&mut draft.value.amount,minimum..=maximum,"%",true,false);
                        }
                        }
                        valid=crate::plugins::validate(&definition.parameters_schema,&parameters(&draft.value)).map_err(|error|error.message);
                        if valid.is_ok() {valid=draft.value.spec().validate();}
                    }else {valid=Err("The registered adjustment filter is unavailable".into());}
                    let mut opacity=draft.value.opacity as f32/255.;
                    panels::parameter(ui,"Opacity",&mut opacity,0. ..=1.,"%",true,false);
                    draft.value.opacity=(opacity*255.).round() as u8;
                });
                ui.add(Label::new(tr("Adjusts the artwork below this item. Painting stays on the current raster layer.")).wrap());
            });
        if draft.value != old {
            draft.error.clear();
        }
        let dirty = draft.value != draft.source;
        if stale {
            ui.add(
                Label::new(tr(
                    "The document changed. Reload this adjustment or cancel your draft.",
                ))
                .wrap(),
            );
        } else if let Err(error) = &valid {
            ui.colored_label(ui.visuals().error_fg_color, localize(error));
        }
        if !draft.error.is_empty() {
            ui.add(Label::new(localize(&draft.error)).wrap());
        }
        let mut discard = false;
        ui.horizontal_wrapped(|ui|{
            let cancel=ui.add_enabled(dirty||!draft.error.is_empty(),Button::new(tr("Cancel")));
            let apply=ui.add_enabled(dirty&&!stale&&valid.is_ok()&&!draft.value.name.trim().is_empty(),Button::new(tr("Apply")));
            #[cfg(test)] ui.data_mut(|data|{
                data.insert_temp(Id::new(("adjustment-cancel",self.session.doc.identity,id)),(cancel.rect,cancel.enabled()));
                data.insert_temp(Id::new(("adjustment-apply",self.session.doc.identity,id)),(apply.rect,apply.enabled()));
            });
            if cancel.clicked() {discard=true;}
            if apply.clicked() {
                let mut action=crate::application::Action::new("adjustment.set",json!({"adjustment_id":id,"name":draft.value.name,"visible":draft.value.visible,"opacity":draft.value.opacity as f32/255.,"node_type":draft.value.node_type,"parameters":parameters(&draft.value)}));
                action.expected_revision=Some(draft.revision);pending.push(action);
            }
            if stale&&ui.button(tr("Reload Adjustment")).clicked() {discard=true;}
        });
        ui.menu_button(tr("Adjustment Actions"), |ui| {
            if ui.button(tr("Delete Adjustment")).clicked() {
                pending.push(crate::application::Action::new(
                    "adjustment.delete",
                    json!({"adjustment_id":id}),
                ));
                ui.close();
            }
            let state = self.session.doc.groups_state();
            if let Some(row) = state["rows"].as_array().and_then(|rows| {
                rows.iter()
                    .find(|row| row["kind"] == "adjustment" && row["id"] == id)
            }) {
                let parent = row["parent_id"].as_u64().unwrap_or(0);
                let children = if parent == 0 {
                    state["roots"].as_array()
                } else {
                    state["groups"]
                        .as_array()
                        .and_then(|groups| groups.iter().find(|group| group["id"] == parent))
                        .and_then(|group| group["children"].as_array())
                };
                if let Some(children) = children {
                    if let Some(index) = children
                        .iter()
                        .position(|child| child["kind"] == "adjustment" && child["id"] == id)
                    {
                        for (title, next, enabled) in [
                            ("Move Up", index + 1, index + 1 < children.len()),
                            ("Move Down", index.saturating_sub(1), index > 0),
                        ] {
                            if ui.add_enabled(enabled, Button::new(tr(title))).clicked() {
                                pending.push(crate::application::Action::new(
                                    "adjustment.move",
                                    json!({"adjustment_id":id,"parent_id":parent,"index":next}),
                                ));
                                ui.close();
                            }
                        }
                    }
                }
            }
        });
        if discard {
            ui.data_mut(|data| data.remove::<Draft>(key));
        } else if dirty || !draft.error.is_empty() {
            ui.data_mut(|data| data.insert_temp(key, draft));
        }
        ui.add_space(a.item_spacing);
        true
    }

    pub(super) fn adjustment_row_ui(
        &mut self,
        ui: &mut Ui,
        id: u64,
        depth: usize,
        effective_visible: bool,
        pending: &mut Vec<crate::application::Action>,
    ) {
        let Ok(item) = self.session.doc.adjustment(id).cloned() else {
            return;
        };
        let a = self.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        let width = ui
            .available_width()
            .min((ui.clip_rect().right() - ui.next_widget_position().x).max(0.));
        let (rect, response) = ui.allocate_exact_size(
            vec2(width, a.control_height + a.panel_padding),
            Sense::click(),
        );
        let focused = self.focused_adjustment_ui(ui.ctx()) == Some(id);
        if focused || response.hovered() {
            ui.painter().rect_filled(
                rect,
                a.corner_radius,
                if focused {
                    colors.tint(0.15)
                } else {
                    colors.raised
                },
            );
        }
        let indent = (depth as f32 * (a.panel_padding + a.item_spacing))
            .min((width - a.control_height * 4.).max(0.));
        let icon = Rect::from_min_size(
            pos2(
                rect.left() + indent,
                rect.center().y - a.control_height * 0.5,
            ),
            Vec2::splat(a.control_height),
        );
        ui.painter().text(
            icon.center(),
            Align2::CENTER_CENTER,
            '\u{e429}',
            FontId::new(a.icon_size, FontFamily::Name("Material".into())),
            colors.muted,
        );
        let mask_rect = icon.translate(vec2(icon.width() + a.item_spacing, 0.));
        let mask = self.mask_thumbnail_ui(ui, MaskOwner::Adjustment { id }, mask_rect, pending);
        let left = mask.as_ref().map_or(icon.right(), |_| mask_rect.right()) + a.item_spacing;
        let eye = Rect::from_center_size(
            pos2(rect.right() - a.control_height * 0.5, rect.center().y),
            Vec2::splat(a.control_height),
        );
        let eye_response = ui.interact(
            eye,
            ui.id()
                .with(("adjustment-eye", self.session.doc.identity, id)),
            Sense::click(),
        );
        ui.painter().circle_stroke(
            eye.center(),
            4.,
            Stroke::new(
                1.,
                if item.visible {
                    colors.accent
                } else {
                    colors.muted
                },
            ),
        );
        if item.visible {
            ui.painter().circle_filled(eye.center(), 2., colors.accent);
        }
        layers::layer_text(
            ui,
            item.name.clone(),
            Rect::from_min_max(
                pos2(left, rect.top()),
                pos2(eye.left() - a.item_spacing, rect.bottom()),
            ),
            FontId::proportional(a.font_size),
            if effective_visible {
                colors.text
            } else {
                colors.muted
            },
        );
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(
                Id::new(("adjustment-row", self.session.doc.identity, id)),
                rect,
            );
            data.insert_temp(
                Id::new(("adjustment-eye", self.session.doc.identity, id)),
                eye,
            );
        });
        if eye_response.clicked() {
            pending.push(crate::application::Action::new(
                "adjustment.set",
                json!({"adjustment_id":id,"visible":!item.visible}),
            ));
        }
        if response.clicked()
            && !eye_response.clicked()
            && !mask.as_ref().is_some_and(|response| response.clicked())
        {
            self.focus_adjustment_ui(ui.ctx(), Some(id));
        }
        response.on_hover_text(format!(
            "{}\n{}",
            item.name,
            localize("Inspect adjustment properties; painting stays on the current raster layer.")
        ));
    }

    pub(super) fn dispatch_adjustment_ui(
        &mut self,
        ctx: &Context,
        mut action: crate::application::Action,
    ) -> bool {
        if action.expected_revision.is_none() {
            action.expected_revision = Some(self.session.revision);
        }
        let command = action.action.clone();
        let id = action.args["adjustment_id"].as_u64();
        let applied = command == "adjustment.set"
            && ["name", "visible", "opacity", "node_type", "parameters"]
                .iter()
                .all(|key| action.args.get(key).is_some());
        let previous = self.backend_contact_stamp();
        let result = self.session.apply(action);
        self.reconcile_command_contact(previous);
        match result {
            Ok(response) => {
                if command == "adjustment.create" {
                    if let Some(id) = response["result"]["adjustment_id"].as_u64() {
                        self.focus_adjustment_ui(ctx, Some(id));
                    }
                }
                if command == "adjustment.delete" && self.focused_adjustment_ui(ctx) == id {
                    self.focus_adjustment_ui(ctx, None);
                }
                if applied || command == "adjustment.delete" {
                    if let Some(id) = id {
                        ctx.data_mut(|data| {
                            data.remove::<Draft>(draft_key(self.session.doc.identity, id))
                        });
                    }
                }
                self.sync_from_session();
                true
            }
            Err(error) => {
                self.status = error.to_string();
                if let Some(id) = id {
                    let key = draft_key(self.session.doc.identity, id);
                    if let Some(mut draft) = ctx.data_mut(|data| data.get_temp::<Draft>(key)) {
                        draft.error = error.to_string();
                        ctx.data_mut(|data| data.insert_temp(key, draft));
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;
