use super::*;
use crate::ui::palette::Mode;
impl Studio {
    pub(super) fn draw_shell(&mut self, ctx: &Context) {
        let a = self.editor.settings.ui.clone();
        let c = theme::Colors::new(&a);
        let edge = ctx
            .data(|d| d.get_temp::<f32>(Id::new("workspace-card-left")))
            .unwrap_or(a.outer_padding);
        TopBottomPanel::top("title")
            .show_separator_line(false)
            .exact_height(a.title_height + a.outer_padding)
            .frame(Frame::new().fill(c.background).inner_margin(Margin {
                left: a.outer_padding as i8,
                right: a.outer_padding as i8,
                top: a.outer_padding as i8,
                bottom: 0,
            }))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                let drag = ui.interact(r, ui.id().with("window-drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    self.editor.core(if ctx.input(|i|i.viewport().maximized.unwrap_or(false)) {"application.restore"} else {"application.maximize"},serde_json::json!({}));
                }
                let center_y = r.center().y;
                // Brand mark: lavender to peach, like ink catching warm light.
                let mark = Rect::from_min_size(pos2(edge, center_y - 13.), Vec2::splat(26.));
                theme::gradient_rect(ui.painter(), mark, 8., c.accent, c.blush);
                ui.painter().text(
                    mark.center() + vec2(0., -0.5),
                    Align2::CENTER_CENTER,
                    localize("M"),
                    theme::strong(ctx, 14.),
                    Color32::WHITE,
                );
                let menu = Rect::from_min_max(
                    pos2(edge + 38., r.top()),
                    pos2(edge + 382., r.bottom()),
                );
                ui.scope_builder(
                    UiBuilder::new()
                        .max_rect(menu)
                        .layout(Layout::left_to_right(Align::Center)),
                    |ui| {
                        egui::MenuBar::new().ui(ui, |ui| {
                            for (title, entries) in [
                                (
                                    "File",
                                    vec![
                                        (1, "New Canvas"),
                                        (2, "Open…"),
                                        (3, "Save Project"),
                                        (7, "Save As…"),
                                        (4, "Export PNG…"),
                                    ],
                                ),
                                (
                                    "Edit",
                                    vec![
                                        (5, "Undo"),
                                        (6, "Redo"),
                                        (10161, "Copy"),
                                        (10162, "Copy Merged"),
                                        (10163, "Cut"),
                                        (10164, "Paste"),
                                        (10165, "Paste in Place"),
                                        (10144, "Canvas Size…"),
                                        (10145, "Crop Canvas…"),
                                        (42, "New Layer"),
                                        (10159, "Layer via Copy"),
                                        (10160, "Layer via Cut"),
                                        (43, "Clear Current Layer…"),
                                        (10146, "Merge Down"),
                                        (10147, "Merge Visible"),
                                        (10148, "Flatten Image"),
                                        (10149, "Stamp Visible"),
                                        (10150, "Select All"),
                                        (10151, "Deselect"),
                                        (10152, "Reselect"),
                                        (10153, "Invert Selection"),
                                    ],
                                ),
                                (
                                    "Image",
                                    vec![
                                        (10173,"Image Size…"),(10144,"Canvas Size…"),(10145,"Crop Canvas…"),(10174,"Trim Canvas…"),
                                        (10166,"Rotate Canvas 90° Clockwise"),(10167,"Rotate Canvas 90° Counterclockwise"),
                                        (10168,"Rotate Canvas 180°"),(10169,"Flip Canvas Horizontally"),(10170,"Flip Canvas Vertically"),
                                    ],
                                ),
                                (
                                    "View",
                                    vec![
                                        (40, "Fit to Window"),
                                        (41, "Actual Pixels"),
                                        (85, "Zoom In"),
                                        (86, "Zoom Out"),
                                        (10130, "Node Editor…"),
                                        (10131, "Time-lapse Replay…"),
                                        (69, "Toggle Panels"),
                                    ],
                                ),
                                (
                                    "Workspace",
                                    vec![
                                        (50, "Appearance and Spacing…"),
                                        (93, "Keyboard Shortcuts…"),
                                        (10121, "Reset Layout"),
                                        (10122, "Group Color and Brush"),
                                        (10123, "Group Navigator and Layers"),
                                    ],
                                ),
                            ] {
                                ui.menu_button(tr(title), |ui| {
                                    ui.set_min_width(a.control_height*10.);
                                    for (id,label) in entries {
                                        let background=ui.painter().add(Shape::Noop);
                                        let row=ui.horizontal(|ui|{
                                            ui.set_min_height(a.control_height+6.);
                                            ui.style_mut().wrap_mode=Some(egui::TextWrapMode::Extend);
                                            ui.add_space(a.item_spacing);
                                            ui.label(theme::icon(theme::action_icon(id),17.));
                                            ui.label(tr(label));
                                            let keys=self.editor.label(id);
                                            if !keys.is_empty(){ui.add_space(a.item_spacing);theme::keycaps(ui,&keys);}
                                            ui.add_space(a.item_spacing);
                                        });
                                        let r=row.response.rect;
                                        let response=ui.interact(r,ui.make_persistent_id(("menu-command",id)),Sense::click());
                                        if response.hovered(){ui.painter().set(background,Shape::rect_filled(r,a.corner_radius,c.raised));}
                                        if response.clicked() {
                                            self.editor.actions.push(Action::UiCommand(id));
                                            ui.close();
                                        }
                                    }
                                });
                            }
                            ui.menu_button(tr("Windows"), |ui| {
                                let panels:Vec<_>=self.editor.session.registry.panels.values().cloned().collect();
                                for panel in panels {
                                    ui.menu_button(tr(&panel.title), |ui| {
                                        for (operation,label) in [("show","Show Window"),("float","Float Window"),("hide","Hide Window")] {
                                            if ui.button(tr(label)).clicked() {
                                                self.editor.core("window.command",serde_json::json!({"panel":panel.id,"operation":operation})); ui.close();
                                            }
                                        }
                                    });
                                }
                            });
                        });
                    },
                );
                let caption = Rect::from_min_max(pos2(r.right() - 126., r.top()), r.max);
                for (index, (symbol, hint)) in [
                    ("−", "Minimize"),
                    (
                        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
                            "❐"
                        } else {
                            "□"
                        },
                        "Maximize / Restore",
                    ),
                    ("×", "Close"),
                ]
                .into_iter()
                .enumerate()
                {
                    let button = Rect::from_min_size(
                        caption.min + vec2(index as f32 * 42., 0.),
                        vec2(42., r.height()),
                    );
                    let response = ui
                        .put(
                            button,
                            Button::new(tr(RichText::new(localize(symbol)).size(18.))).frame(false),
                        )
                        .on_hover_text(tr(hint));
                    if response.clicked() {
                        let command=match index {
                            0=>"application.minimize",
                            1=>if ctx.input(|i|i.viewport().maximized.unwrap_or(false)) {"application.restore"} else {"application.maximize"},
                            _=>"application.close",
                        };
                        self.editor.core(command,serde_json::json!({}));
                    }
                }
                let width = (r.width() - 580.).clamp(180., 400.);
                let left = (r.center().x - width * 0.5).max(menu.right() + 16.);
                let search = Rect::from_center_size(
                    pos2(left + width * 0.5, center_y),
                    vec2(width, a.control_height),
                );
                let response = ui
                    .interact(search, ui.id().with("command-center"), Sense::click())
                    .on_hover_text(tr("Search Commands, Tools and Settings"));
                let pill = search.height() * 0.5;
                ui.painter().rect(
                    search,
                    pill,
                    if response.hovered() { c.raised } else { c.panel },
                    Stroke::new(1., if response.hovered() { c.tint(0.45) } else { c.border.gamma_multiply(if c.dark { 0.7 } else { 1. }) }),
                    StrokeKind::Inside,
                );
                ui.painter().text(
                    pos2(search.left() + 16., center_y),
                    Align2::CENTER_CENTER,
                    localize("\u{ef7a}"),
                    FontId::new(17., FontFamily::Name("Material".into())),
                    c.muted,
                );
                ui.painter().text(
                    pos2(search.left() + 34., center_y),
                    Align2::LEFT_CENTER,
                    localize("Find Tools and Commands…"),
                    FontId::proportional(a.small_font_size),
                    c.muted,
                );
                if width >= 310. {
                    ui.scope_builder(
                        UiBuilder::new()
                            .max_rect(Rect::from_min_max(
                                pos2(search.right() - 142., search.top()),
                                search.max,
                            ))
                            .layout(Layout::left_to_right(Align::Center)),
                        |ui| {
                            ui.visuals_mut().faint_bg_color = Color32::TRANSPARENT;
                            ui.visuals_mut().widgets.noninteractive.bg_stroke = Stroke::NONE;
                            theme::keycaps(ui, &self.editor.label(95));
                        },
                    );
                }
                if response.clicked() {
                    self.editor.open_palette(Mode::Commands);
                }
            });
    }
    pub(super) fn new_document_window(&mut self, ctx: &Context) {
        if !self.editor.new_open {
            return;
        }
        let mut open = true;
        let mut create = false;
        Window::new(tr("New Canvas"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.weak(tr("Transparent background · Up to 4096 × 4096 pixels"));
                ui.add_space(12.);
                Grid::new("new-size").num_columns(2).show(ui, |ui| {
                    ui.label(tr("Width"));
                    ui.add(
                        DragValue::new(&mut self.editor.new_width)
                            .range(1..=4096)
                            .suffix(" px"),
                    );
                    ui.end_row();
                    ui.label(tr("Height"));
                    ui.add(
                        DragValue::new(&mut self.editor.new_height)
                            .range(1..=4096)
                            .suffix(" px"),
                    );
                    ui.end_row();
                });
                ui.add_space(12.);
                if ui.button(tr("Create Canvas")).clicked() {
                    create = true;
                }
            });
        if create {
            self.editor.core(
                "canvas.new",
                serde_json::json!({"width":self.editor.new_width,"height":self.editor.new_height}),
            );
            self.editor.sync_from_session();
            open = false;
        }
        self.editor.new_open = open;
    }
}
