//! Profile-owned scalar brush snapshots. User names are never translated.
use super::*;
use crate::configuration::brush_presets::{PresetParameters, UserPreset};
use serde_json::{json, Value};
use std::sync::Arc;

const CACHE: &str = "user-brush-preset-view";
const DRAFT: &str = "user-brush-preset-name";
const SEARCH: &str = "user-brush-preset-search";
#[derive(Clone)]
struct Card {
    preset: UserPreset,
    search: String,
    available: bool,
    reason: Option<String>,
}
#[derive(Clone)]
struct View {
    token: u64,
    cards: Vec<Card>,
}
#[derive(Clone)]
struct NameDraft {
    id: Option<u64>,
    name: String,
    owner: String,
    tool_id: String,
    parameters: PresetParameters,
    token: u64,
    error: String,
}

fn current_matches(p: &PresetParameters, s: &crate::application::Parameters) -> bool {
    p.radius == s.radius
        && p.opacity == s.opacity
        && p.hardness == s.hardness
        && p.stabilizer == s.stabilizer
        && p.pressure_size == s.pressure_size
        && p.pressure_opacity == s.pressure_opacity
        && p.pressure_gamma == s.pressure_gamma
        && p.minimum_radius == s.minimum_radius
        && p.spacing == s.spacing
        && p.minimum_opacity == s.minimum_opacity
        && p.flow == s.flow
        && p.opacity_mode == s.opacity_mode
}
fn button(ui: &mut Ui, a: &Appearance, title: &str, enabled: bool) -> Response {
    let natural = ui.painter().layout_no_wrap(
        crate::i18n::text(title).into_owned(),
        FontId::proportional(a.font_size),
        Color32::PLACEHOLDER,
    );
    let width = (natural.size().x + ui.spacing().button_padding.x * 2.)
        .max(a.control_height)
        .min(ui.max_rect().width().min(ui.clip_rect().width()))
        .max(1.);
    let size = vec2(width, a.control_height);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.scope_builder(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add_enabled_ui(enabled, |ui| {
                ui.add_sized(size, Button::new(tr(title)).truncate())
            })
            .inner
        },
    )
    .inner
}
fn preview(ui: &Ui, rect: Rect, p: &PresetParameters, color: Color32) {
    let width = (p.radius * 0.22).clamp(0.75, 4.);
    let points = (0..=20)
        .map(|n| {
            let t = n as f32 / 20.;
            pos2(
                rect.left() + rect.width() * t,
                rect.center().y + (t * std::f32::consts::TAU).sin() * rect.height() * 0.18,
            )
        })
        .collect::<Vec<_>>();
    ui.painter()
        .with_clip_rect(rect)
        .line(points, Stroke::new(width, color));
}

impl Editor {
    fn preset_view(&self, ctx: &Context) -> Arc<View> {
        let token = self.session.preset_library_token();
        if let Some(view) = ctx
            .data_mut(|data| data.get_temp::<Arc<View>>(Id::new(CACHE)))
            .filter(|view| view.token == token)
        {
            return view;
        }
        let cards = self
            .session
            .preset_library()
            .items
            .iter()
            .map(|preset| {
                let status = crate::plugins::brush_presets::provider_status(
                    &self.session,
                    &preset.owner,
                    &preset.tool_id,
                    &preset.parameters,
                );
                Card {
                    preset: preset.clone(),
                    search: preset.name.to_lowercase(),
                    available: status.available,
                    reason: status.reason,
                }
            })
            .collect();
        let view = Arc::new(View { token, cards });
        ctx.data_mut(|data| data.insert_temp(Id::new(CACHE), view.clone()));
        view
    }
    fn preset_current_draft(&mut self, ctx: &Context, id: Option<u64>, name: String) {
        match crate::plugins::brush_presets::capture_current(&self.session) {
            Ok(captured) => ctx.data_mut(|data| {
                data.insert_temp(
                    Id::new(DRAFT),
                    NameDraft {
                        id,
                        name,
                        owner: captured.owner,
                        tool_id: captured.tool_id,
                        parameters: captured.parameters,
                        token: self.session.preset_library_token(),
                        error: String::new(),
                    },
                )
            }),
            Err(error) => self.status = error.to_string(),
        }
    }
    fn preset_upsert_args(draft: &NameDraft) -> Result<Value, String> {
        let mut args = json!({"name":draft.name,"owner":draft.owner,"tool_id":draft.tool_id,"parameters":draft.parameters.value()?,"expected_library_token":draft.token});
        if let Some(id) = draft.id {
            args["preset_id"] = json!(id);
        }
        Ok(args)
    }
    fn preset_update_current(&mut self, id: u64, name: &str, token: u64) {
        let result = crate::plugins::brush_presets::capture_current(&self.session)
            .map_err(|error| error.message)
            .and_then(|captured| {
                Self::preset_upsert_args(&NameDraft {
                    id: Some(id),
                    name: name.into(),
                    owner: captured.owner,
                    tool_id: captured.tool_id,
                    parameters: captured.parameters,
                    token,
                    error: String::new(),
                })
            });
        match result {
            Ok(args) => {
                self.core("brush.presets.upsert", args);
            }
            Err(error) => self.status = error,
        }
    }
    fn preset_file_ui(&mut self, save: bool, token: u64, ids: &[u64]) {
        let command = if save {
            "brush.presets.export"
        } else {
            "brush.presets.import"
        };
        if let Some(native) = self.native.as_ref() {
            if let Some(path) = native.preset_dialog(save) {
                let mut args = json!({"path":path,"expected_library_token":token});
                if save {
                    args["preset_ids"] = json!(ids);
                }
                self.core(command, args);
            }
        } else {
            let mut args = json!({"expected_library_token":token});
            if save {
                args["preset_ids"] = json!(ids);
            }
            self.begin_plugin_command_with_args(command, args);
        }
    }
    fn preset_name_ui(&mut self, ui: &mut Ui) {
        let Some(mut draft) = ui.data_mut(|data| data.get_temp::<NameDraft>(Id::new(DRAFT))) else {
            return;
        };
        let a = self.settings.ui.clone();
        let stale = draft.token != self.session.preset_library_token();
        ui.add_space(a.item_spacing);
        ui.strong(tr(if draft.id.is_some() {
            "Rename Brush"
        } else {
            "Save Current Brush"
        }));
        let _name = ui.add(
            TextEdit::singleline(&mut draft.name)
                .id(Id::new("user-brush-preset-name-input"))
                .desired_width(f32::INFINITY)
                .char_limit(120)
                .hint_text(crate::i18n::text("Brush Name")),
        );
        #[cfg(test)]
        ui.data_mut(|data| data.insert_temp(Id::new("user-brush-preset-name-input"), _name.rect));
        if stale {
            ui.add(
                Label::new(tr(
                    "Your brush library changed. Cancel this draft and reopen it before saving.",
                ))
                .wrap(),
            );
        }
        if !draft.error.is_empty() {
            ui.add(Label::new(crate::i18n::text(&draft.error)).wrap());
        }
        let mut close = false;
        ui.horizontal_wrapped(|ui| {
            let save = button(
                ui,
                &a,
                "Save Preset",
                !stale && !draft.name.trim().is_empty(),
            );
            let cancel = button(ui, &a, "Cancel", true);
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(
                    Id::new("user-brush-preset-save"),
                    (save.rect, save.enabled()),
                );
                data.insert_temp(Id::new("user-brush-preset-cancel"), cancel.rect);
            });
            if cancel.clicked() {
                close = true;
            }
            if save.clicked() {
                match Self::preset_upsert_args(&draft) {
                    Ok(args) => {
                        if self.core("brush.presets.upsert", args) {
                            close = true;
                        } else {
                            draft.error = self.status.clone();
                        }
                    }
                    Err(error) => draft.error = error,
                }
            }
        });
        if close {
            ui.data_mut(|data| data.remove::<NameDraft>(Id::new(DRAFT)));
        } else {
            ui.data_mut(|data| data.insert_temp(Id::new(DRAFT), draft));
        }
    }
    pub(super) fn user_brush_presets_ui(&mut self, ui: &mut Ui) {
        if !self
            .session
            .registry
            .actions
            .contains_key("brush.presets.query")
        {
            return;
        }
        let a = self.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        let view = self.preset_view(ui.ctx());
        section(ui, "My Brushes", a.section_spacing);
        let current =
            self.area_mode == 0 && matches!(self.session.tool, Tool::Brush | Tool::Eraser);
        let idle = !self.backend_contact_active();
        ui.horizontal_wrapped(|ui| {
            let save = button(ui, &a, "Save Current Brush", current);
            #[cfg(test)]
            ui.data_mut(|data| {
                data.insert_temp(
                    Id::new("user-brush-save-current"),
                    (save.rect, save.enabled()),
                )
            });
            if save.clicked() {
                self.preset_current_draft(
                    ui.ctx(),
                    None,
                    crate::i18n::text("My Brush").into_owned(),
                );
            }
            let menu = ui
                .menu_button(tr("···"), |ui| {
                    for (title, enabled, save) in [
                        ("Import Brushes…", idle, false),
                        ("Export Brushes…", idle && !view.cards.is_empty(), true),
                    ] {
                        let response = ui.add_enabled(enabled, Button::new(tr(title)));
                        #[cfg(test)]
                        ui.data_mut(|data| {
                            data.insert_temp(
                                Id::new(("user-brush-library-action", title)),
                                (response.rect, response.enabled()),
                            )
                        });
                        if response.clicked() {
                            let ids = view
                                .cards
                                .iter()
                                .map(|card| card.preset.id)
                                .collect::<Vec<_>>();
                            self.preset_file_ui(save, view.token, &ids);
                            ui.close();
                        }
                    }
                    if ui.button(tr("Reload Brush Library")).clicked() {
                        self.core(
                            "brush.presets.reload",
                            json!({"expected_library_token":view.token}),
                        );
                        ui.close();
                    }
                })
                .response
                .on_hover_text(tr("Brush Library"));
            #[cfg(test)]
            ui.data_mut(|data| data.insert_temp(Id::new("user-brush-library-menu"), menu.rect));
        });
        self.preset_name_ui(ui);
        if view.cards.is_empty() {
            ui.add(Label::new(tr("Save Brush or Eraser settings here. Your foreground and background colors stay independent.")).wrap());
            return;
        }
        let mut search = ui
            .data_mut(|data| data.get_temp::<String>(Id::new(SEARCH)))
            .unwrap_or_default();
        let search_input = ui.add(
            TextEdit::singleline(&mut search)
                .id(Id::new(SEARCH))
                .desired_width(f32::INFINITY)
                .hint_text(crate::i18n::text("Find Saved Brushes")),
        );
        #[cfg(test)]
        ui.data_mut(|data| {
            data.insert_temp(Id::new("user-brush-preset-search-input"), search_input.rect)
        });
        if search_input.changed() {
            ui.data_mut(|data| data.insert_temp(Id::new(SEARCH), search.clone()));
        }
        let query = search.to_lowercase();
        let filtered = view
            .cards
            .iter()
            .filter(|card| card.search.contains(&query))
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            ui.weak(tr("No saved brushes match."));
            return;
        }
        let columns = if ui.available_width() >= a.control_height * 5. {
            2
        } else {
            1
        };
        let width = ((ui.available_width() - a.item_spacing * (columns - 1) as f32)
            / columns as f32)
            .max(1.);
        let height = a.control_height * 2. + a.item_spacing;
        let rows = filtered.len().div_ceil(columns);
        ScrollArea::vertical().id_salt("user-brush-preset-cards").max_height(height*2.+a.item_spacing)
            .show_rows(ui,height,rows,|ui,range|{
                for row in range {ui.horizontal(|ui|{
                    for card in filtered.iter().skip(row*columns).take(columns) {
                        let (rect,_)=ui.allocate_exact_size(vec2(width,height),Sense::hover());
                        let body=Rect::from_min_max(rect.min,pos2(rect.right()-a.control_height,rect.bottom()));
                        let response=ui.scope_builder(UiBuilder::new().max_rect(body),|ui|ui.add_enabled_ui(card.available,|ui|ui.interact(body,ui.id().with(("user-preset-apply",card.preset.id)),Sense::click())).inner).inner;
                        response.widget_info(||egui::WidgetInfo::labeled(egui::WidgetType::Button,card.available,card.preset.name.clone()));
                        let matching=card.available&&self.session.registry.contribution_owner("tools",&card.preset.tool_id)==Some(card.preset.owner.as_str())
                            &&self.session.selected_tool==card.preset.tool_id&&current_matches(&card.preset.parameters,&self.session.parameters);
                        ui.painter().rect_filled(rect,a.corner_radius,if matching{colors.tint(0.16)}else if response.hovered(){colors.border}else{colors.raised});
                        let mark=Rect::from_min_max(rect.min+vec2(a.item_spacing,a.item_spacing),pos2(rect.right()-a.item_spacing,rect.top()+a.control_height));
                        preview(ui,mark,&card.preset.parameters,if card.available{if matching{colors.accent}else{colors.text}}else{colors.muted});
                        let title=WidgetText::from(RichText::new(&card.preset.name).size(a.small_font_size)).into_galley(ui,Some(TextWrapMode::Truncate),(body.width()-a.item_spacing*2.).max(0.),TextStyle::Small);
                        ui.painter().with_clip_rect(body).galley(pos2(body.left()+a.item_spacing,rect.bottom()-a.item_spacing-title.size().y),title,colors.muted);
                        let tooltip=card.reason.as_ref().map_or_else(||card.preset.name.clone(),|reason|format!("{}\n{}",card.preset.name,crate::i18n::text(reason)));
                        let response=response.on_hover_text(tooltip.as_str()).on_disabled_hover_text(tooltip);
                        #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("user-brush-preset-card",card.preset.id)),(body,response.enabled())));
                        if response.clicked()&&self.core("brush.presets.apply",json!({"preset_id":card.preset.id,"expected_library_token":view.token})) {
                            self.area_mode=0;self.sync_from_session();
                        }
                        let menu_rect=Rect::from_min_size(pos2(rect.right()-a.control_height,rect.bottom()-a.control_height),Vec2::splat(a.control_height));
                        ui.scope_builder(UiBuilder::new().max_rect(menu_rect).layout(Layout::left_to_right(Align::Center)),|ui|{
                            ui.push_id(card.preset.id,|ui|{
                            let menu=ui.menu_button(tr("···"),|ui|{
                                let rename=ui.button(tr("Rename Brush"));
                                #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("user-brush-preset-action",card.preset.id,"rename")),rename.rect));
                                if rename.clicked() {
                                    ui.data_mut(|data|data.insert_temp(Id::new(DRAFT),NameDraft {id:Some(card.preset.id),name:card.preset.name.clone(),owner:card.preset.owner.clone(),tool_id:card.preset.tool_id.clone(),parameters:card.preset.parameters.clone(),token:view.token,error:String::new()}));ui.close();
                                }
                                let update=ui.add_enabled(current,Button::new(tr("Update from Current Brush")));
                                #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("user-brush-preset-action",card.preset.id,"update")),update.rect));
                                if update.clicked() {self.preset_update_current(card.preset.id,&card.preset.name,view.token);ui.close();}
                                let delete=ui.button(tr("Delete Saved Brush"));
                                #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("user-brush-preset-action",card.preset.id,"delete")),delete.rect));
                                if delete.clicked() {self.core("brush.presets.delete",json!({"preset_id":card.preset.id,"expected_library_token":view.token}));ui.close();}
                            });
                            #[cfg(test)] ui.data_mut(|data|data.insert_temp(Id::new(("user-brush-preset-menu",card.preset.id)),menu.response.rect));
                            });
                        });
                    }
                });}
            });
    }
}

#[cfg(test)]
#[path = "presets_ui_tests.rs"]
mod tests;
