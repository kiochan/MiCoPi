use super::*;
use crate::{configuration::PaintingPreferences, ui::keymap::Chord};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) enum Category {
    #[default]
    All,
    Appearance,
    Layout,
    Painting,
    Keys,
    Plugins,
}
impl Category {
    fn title(self) -> &'static str {
        match self {
            Self::All => "All Settings",
            Self::Appearance => "Appearance",
            Self::Layout => "Workspace and Layout",
            Self::Painting => "Painting and Pen",
            Self::Keys => "Keyboard Shortcuts",
            Self::Plugins => "Plugins",
        }
    }
}
#[derive(Default)]
pub(super) struct Preferences {
    pub query: String,
    pub category: Category,
    pub json: String,
    pub bounds: Option<Rect>,
    pub painting_before: Option<PaintingPreferences>,
    pub selection_before: Option<crate::application::SelectionSampling>,
    pub movement_before: Option<crate::application::MovementPreferences>,
    json_mode: bool,
    focus: bool,
    message: String,
    binding: Option<usize>,
    binding_draft: String,
    capture: bool,
    result_count: usize,
}
struct Setting {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    keywords: &'static str,
    category: Category,
    range: Option<(f64, f64)>,
}
macro_rules! metric {
    ($id:literal, $title:literal, $desc:literal, $cat:ident, $low:literal, $high:literal) => {
        Setting {
            id: $id,
            title: $title,
            description: $desc,
            keywords: "spacing size padding 间距 大小",
            category: Category::$cat,
            range: Some(($low, $high)),
        }
    };
}
const SETTINGS: &[Setting] = &[
    Setting {
        id: "ui.language",
        title: "Display Language",
        description: "English is the default. Language changes apply immediately.",
        keywords: "language locale translation English 中文 日本語 한국어",
        category: Category::Appearance,
        range: None,
    },
    Setting {
        id: "ui.theme",
        title: "Color Theme",
        description: "Graphite, light or warm gray; menus and panels share the theme.",
        keywords: "theme color 主题 颜色",
        category: Category::Appearance,
        range: None,
    },
    Setting {
        id: "ui.accent",
        title: "Accent Color",
        description: "Color for selection, focus and highlights.",
        keywords: "accent color 主题 颜色",
        category: Category::Appearance,
        range: None,
    },
    metric!(
        "ui.font_size",
        "Font Size",
        "Main interface font size in logical pixels.",
        Appearance,
        11.,
        22.
    ),
    metric!(
        "ui.small_font_size",
        "Small Font Size",
        "Font size for descriptions, status and secondary labels.",
        Appearance,
        10.,
        18.
    ),
    metric!(
        "ui.item_spacing",
        "Item Spacing",
        "Space between adjacent controls.",
        Appearance,
        4.,
        20.
    ),
    metric!(
        "ui.section_spacing",
        "Section Spacing",
        "Space between setting groups.",
        Appearance,
        8.,
        40.
    ),
    metric!(
        "ui.panel_padding",
        "Panel Padding",
        "Space between panel content and its edges.",
        Layout,
        6.,
        32.
    ),
    metric!(
        "ui.outer_padding",
        "Window Padding",
        "Space between the workbench and window edges.",
        Layout,
        4.,
        24.
    ),
    metric!(
        "ui.control_height",
        "Control Height",
        "Default height of buttons and input fields.",
        Appearance,
        24.,
        48.
    ),
    metric!(
        "ui.title_height",
        "Title Bar Height",
        "Height of the custom title bar and menu area.",
        Layout,
        36.,
        60.
    ),
    metric!(
        "ui.toolbar_height",
        "Toolbar Height",
        "Height of the painting and view toolbar.",
        Layout,
        32.,
        60.
    ),
    metric!(
        "ui.tab_height",
        "Tab Height",
        "Height of document and panel tabs.",
        Layout,
        28.,
        52.
    ),
    metric!(
        "ui.corner_radius",
        "Corner Radius",
        "Corner radius of buttons, menus and panels.",
        Appearance,
        0.,
        12.
    ),
    metric!(
        "ui.icon_size",
        "Icon Size",
        "Default size of Google Material icons.",
        Appearance,
        16.,
        28.
    ),
    metric!(
        "ui.toolbox_width",
        "Floating Toolbox Width",
        "Default width when floating the independent control bar.",
        Layout,
        200.,
        1200.
    ),
    metric!(
        "ui.toolbox_height",
        "Toolbox Height",
        "Default height of the independent control bar.",
        Layout,
        80.,
        200.
    ),
    metric!(
        "ui.right_panel_width",
        "Default Right Panel Width",
        "Used for the default layout; custom layouts are saved separately.",
        Layout,
        200.,
        440.
    ),
    metric!(
        "ui.canvas_margin",
        "Canvas Margin",
        "Margin reserved when fitting the canvas.",
        Layout,
        8.,
        96.
    ),
    Setting {id:"movement.target",title:"Move Target",description:"Auto moves selected pixels when a selection exists; otherwise it moves the active layer.",keywords:"move nudge selection layer",category:Category::Painting,range:None},
    metric!("movement.step","Nudge Step","Document pixels moved by one arrow-key command.",Painting,1.,4096.),
    metric!("movement.large_step","Large Nudge Step","Document pixels moved by Shift plus an arrow key.",Painting,1.,4096.),
    Setting {
        id: "selection.tolerance",
        title: "Selection Tolerance",
        description: "Default color tolerance for Wand, Grow and Similar.",
        keywords: "selection wand sampling tolerance",
        category: Category::Painting,
        range: Some((0., 255.)),
    },
    Setting {
        id: "selection.contiguous",
        title: "Contiguous Selection",
        description: "Match only connected pixels when starting a Wand selection.",
        keywords: "selection wand sampling tolerance",
        category: Category::Painting,
        range: None,
    },
    Setting {
        id: "selection.sample_merged",
        title: "Sample Merged",
        description: "Sample the rendered image instead of the active raw layer.",
        keywords: "selection wand sampling tolerance",
        category: Category::Painting,
        range: None,
    },
    Setting {
        id: "painting.flow",
        title: "Brush Flow",
        description: "How quickly paint builds up as you draw.",
        keywords: "flow brush paint glazing buildup 流量",
        category: Category::Painting,
        range: Some((0., 1.)),
    },
    Setting {
        id: "painting.opacity_mode",
        title: "Opacity Mode",
        description: "Choose build-up painting or limit opacity within each stroke.",
        keywords: "opacity stroke flow cap glazing 不透明度 单笔",
        category: Category::Painting,
        range: None,
    },
    Setting {
        id: "painting.stabilizer",
        title: "Brush Stabilizer",
        description: "0 disables stabilization; 1 is strongest. Applies to future strokes.",
        keywords: "stabilizer smoothing fangdou 笔刷 防抖",
        category: Category::Painting,
        range: Some((0., 1.)),
    },
    Setting {
        id: "painting.pressure_size",
        title: "Pressure Controls Size",
        description: "Use pen pressure to control brush radius.",
        keywords: "pressure tablet yagan 压感 数位板",
        category: Category::Painting,
        range: None,
    },
    Setting {
        id: "painting.pressure_opacity",
        title: "Pressure Controls Opacity",
        description: "Use pen pressure to control brush opacity.",
        keywords: "pressure tablet yagan 压感 数位板",
        category: Category::Painting,
        range: None,
    },
    Setting {
        id: "painting.pressure_gamma",
        title: "Pressure Curve",
        description: "Below 1 reaches high pressure sooner; above 1 responds more gently.",
        keywords: "pressure gamma curve yagan 压感 曲线",
        category: Category::Painting,
        range: Some((0.25, 4.)),
    },
    Setting {
        id: "painting.minimum_radius",
        title: "Minimum Brush Size",
        description: "Brush radius retained at light pressure, from 0 to 1.",
        keywords: "pressure size yagan 压感 大小",
        category: Category::Painting,
        range: Some((0., 1.)),
    },
    Setting {
        id: "painting.spacing",
        title: "Dab Spacing",
        description: "Distance between brush dabs as a fraction of diameter.",
        keywords: "spacing dabs brush jianju 间距",
        category: Category::Painting,
        range: Some((0.01, 2.)),
    },
    Setting {
        id: "painting.minimum_opacity",
        title: "Minimum Opacity",
        description: "Opacity retained at light pressure; zero pressure deposits no paint.",
        keywords: "pressure opacity floor yagan 压感 不透明度",
        category: Category::Painting,
        range: Some((0., 1.)),
    },
];
fn matches(query: &str, text: &str, modified: bool) -> bool {
    let text = text.to_lowercase();
    query.to_lowercase().split_whitespace().all(|word| {
        if word == "@modified" {
            modified
        } else {
            text.contains(word)
        }
    })
}
fn value<'a>(document: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    let (section, key) = id.split_once('.').unwrap();
    &document[section][key]
}
fn set_value(document: &mut serde_json::Value, id: &str, v: serde_json::Value) {
    let (section, key) = id.split_once('.').unwrap();
    document[section][key] = v;
}
fn normalize(a: &mut Appearance) {
    a.control_height = a.control_height.max(a.font_size + 8.);
    a.tab_height = a.tab_height.max(a.font_size + 10.);
    a.toolbar_height = a.toolbar_height.max(a.control_height + 6.);
    a.title_height = a.title_height.max(a.control_height + 6.);
    a.section_spacing = a.section_spacing.max(a.item_spacing);
}
/// Preserve a complete editable snapshot even when its canonical file would
/// exceed the save ceiling. Validation and the byte cap still apply on Save.
fn settings_editor_json(settings: &Settings) -> Result<String, String> {
    settings.encode().or_else(|_| {
        serde_json::to_string_pretty(&settings.editor_document()?)
            .map(|text| text + "\n")
            .map_err(|e| e.to_string())
    })
}
impl Studio {
    pub(super) fn open_settings(&mut self, category: Option<Category>) {
        if !self.settings_open || self.settings_before.is_none() {
            self.preferences.selection_before = Some(self.editor.session.selection_sampling());
            self.preferences.movement_before = Some(self.editor.session.movement_options());
            self.preferences.painting_before =
                Some(PaintingPreferences::from(&self.editor.session.parameters));
            self.settings_before = Some(self.editor.settings.clone());
            self.preferences.message.clear();
            match settings_editor_json(&self.editor.settings) {
                Ok(text) => self.preferences.json = text,
                Err(error) => {
                    self.preferences.message = error.clone();
                    self.editor.status = error;
                }
            }
        }
        self.editor.palette = None;
        self.preferences.focus = true;
        if let Some(category) = category {
            self.preferences.category = category;
            self.preferences.query.clear();
            self.preferences.json_mode = false;
        }
        self.settings_open = true;
    }
    pub(super) fn cancel_settings(&mut self) {
        if let Some(before) = self.settings_before.take() {
            self.editor.settings = before;
            if let Some(selection) = self.preferences.selection_before.take() {
                if selection != self.editor.session.selection_sampling() {
                    self.editor.core(
                        "ui.dispatch",
                        serde_json::json!({"id":"selection.options","value":selection}),
                    );
                }
            }

            if let Some(movement) = self.preferences.movement_before.take() {
                if movement != self.editor.session.movement_options() {
                    self.editor.core(
                        "ui.dispatch",
                        serde_json::json!({"id":"movement.options","value":movement}),
                    );
                }
            }
            if let Some(painting) = self.preferences.painting_before.take() {
                if painting != PaintingPreferences::from(&self.editor.session.parameters) {
                    self.editor
                        .core("brush.set", serde_json::to_value(painting).unwrap());
                    self.editor.sync_from_session();
                }
            }
        }
        self.settings_open = false;
        self.preferences.binding = None;
    }
    pub(super) fn settings_window(&mut self, ctx: &Context) {
        if !self.settings_open {
            return;
        }
        if self.settings_before.is_none() {
            self.open_settings(None);
            self.settings_before = Some(self.editor.settings.clone());
        }
        let mut prefs = std::mem::take(&mut self.preferences);
        let mut draft = self.editor.settings.clone();
        let documents = draft.editor_document().and_then(|document| {
            Settings::defaults(PathBuf::new())
                .editor_document()
                .map(|defaults| (document, defaults))
        });
        let (mut document, defaults) = match documents {
            Ok(documents) => documents,
            Err(error) => {
                prefs.message = error.clone();
                self.editor.status = error;
                self.preferences = prefs;
                return;
            }
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut reset_layout = false;
        let mut apply_json = false;
        let size = ctx.content_rect().size();
        let width = (size.x - 48.).clamp(300., 1040.);
        let height = (size.y - 64.).clamp(260., 760.);
        let colors = theme::Colors::new(&self.editor.settings.ui);
        let shown = Window::new(tr("Settings"))
            .id(Id::new("appearance-settings"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(width, height))
            .min_size(vec2(300., 260.))
            .max_size(vec2(width, height))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let search = ui.add_sized(
                        [(ui.available_width() - 185.).max(120.), 34.],
                        TextEdit::singleline(&mut prefs.query)
                            .hint_text(tr("Search settings: spacing, pressure, Ctrl+S, @modified…")),
                    );
                    if prefs.focus {
                        search.request_focus();
                        prefs.focus = false;
                    }
                    if search.changed() {
                        prefs.category = Category::All;
                        prefs.json_mode = false;
                    }
                    if ui.selectable_label(!prefs.json_mode,tr( "Settings")).clicked() {
                        prefs.json_mode = false;
                    }
                    if ui.selectable_label(prefs.json_mode,tr( "{} JSON")).clicked() {
                        if !prefs.json_mode {
                            match settings_editor_json(&draft) {
                                Ok(text)=>prefs.json=text,
                                Err(error)=>prefs.message=error,
                            }
                        }
                        prefs.json_mode = true;
                    }
                });
                ui.add_space(6.);
                ui.horizontal_wrapped(|ui| {
                    ui.weak(tr(draft.path.display().to_string()));
                    if ui.small_button(tr("Copy Path")).clicked() {
                        ui.ctx().copy_text(draft.path.display().to_string());
                    }
                    let mut modified = prefs.query.split_whitespace().any(|q| q == "@modified");
                    if ui.checkbox(&mut modified,tr( "Modified Only")).changed() {
                        prefs.query = prefs
                            .query
                            .split_whitespace()
                            .filter(|q| *q != "@modified")
                            .collect::<Vec<_>>()
                            .join(" ");
                        if modified {
                            prefs.query.push_str(" @modified");
                        }
                    }
                });
                ui.separator();
                let body_height = (ui.available_height() - 108.).max(120.);
                if prefs.json_mode {
                    ui.weak(tr("Edit settings.json; validate before applying or saving. Unknown root fields are preserved."));
                    ScrollArea::both()
                        .id_salt("settings-json")
                        .max_height(body_height - 24.)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                TextEdit::multiline(&mut prefs.json)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(24),
                            );
                        });
                } else {
                    let narrow = ui.available_width() < 640.;
                    if narrow {
                        ComboBox::from_id_salt("settings-category")
                            .selected_text(tr(prefs.category.title()))
                            .show_ui(ui, |ui| {
                                category_buttons(ui, &mut prefs);
                            });
                        ScrollArea::vertical()
                            .id_salt("settings-content")
                            .max_height(body_height - 40.)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.settings_content(
                                    ui,
                                    ctx,
                                    &mut prefs,
                                    &mut draft,
                                    &mut document,
                                    &defaults,
                                    &mut reset_layout,
                                );
                            });
                    } else {
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                vec2(168., body_height),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    category_buttons(ui, &mut prefs);
                                },
                            );
                            ui.separator();
                            ScrollArea::vertical()
                                .id_salt("settings-content")
                                .max_height(body_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    let width = ui.available_width();
                                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                        ui.set_width(width);
                                        self.settings_content(
                                            ui,
                                            ctx,
                                            &mut prefs,
                                            &mut draft,
                                            &mut document,
                                            &defaults,
                                            &mut reset_layout,
                                        );
                                    });
                                });
                        });
                    }
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.weak(tr(if prefs.json_mode {
                        "Apply after validation · Unsaved previews revert on close".into()
                    } else {
                        format!(
                            "{} matches · Live preview · Plugin changes require restart",
                            prefs.result_count
                        )
                    }));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        save = ui.button(tr("Save Settings")).clicked();
                        cancel = ui.button(tr("Close")).clicked();
                        if prefs.json_mode {
                            apply_json = ui.button(tr("Validate and Preview")).clicked();
                        }
                    });
                });
                if !prefs.message.is_empty() {
                    ui.colored_label(colors.accent,tr( &prefs.message));
                }
            });
        prefs.bounds = shown.map(|shown| shown.response.rect);
        if prefs.json_mode {
            if save || apply_json {
                match Settings::decode(&prefs.json, draft.path.clone()) {
                    Ok(decoded) => {
                        draft = decoded;
                        prefs.message = "JSON Validated; Preview Applied".into();
                    }
                    Err(error) => {
                        prefs.message = error;
                        save = false;
                    }
                }
            }
        } else {
            match (
                serde_json::from_value::<Appearance>(document["ui"].clone()),
                serde_json::from_value::<PaintingPreferences>(document["painting"].clone()),
                serde_json::from_value::<crate::application::SelectionSampling>(
                    document["selection"].clone(),
                ),
                serde_json::from_value::<crate::application::MovementPreferences>(
                    document["movement"].clone(),
                ),
                serde_json::from_value::<crate::configuration::NewCanvasPreferences>(
                    document["new_canvas"].clone(),
                ),
            ) {
                (Ok(mut appearance), Ok(painting), Ok(selection), Ok(movement), Ok(new_canvas)) => {
                    normalize(&mut appearance);
                    if appearance.validate().is_ok()
                        && painting.validate().is_ok()
                        && movement.validate().is_ok()
                        && new_canvas.validate().is_ok()
                    {
                        draft.ui = appearance;
                        draft.painting = painting;
                        draft.selection = selection;
                        draft.movement = movement;
                        draft.new_canvas = new_canvas;
                    }
                }
                _ => prefs.message = "Invalid Setting Value".into(),
            }
        }
        let movement_changed = draft.movement != self.editor.settings.movement;
        let selection_changed = draft.selection != self.editor.settings.selection;
        let painting_changed = draft.painting != self.editor.settings.painting;
        self.editor.settings = draft;
        if movement_changed {
            self.editor.core(
                "ui.dispatch",
                serde_json::json!({"id":"movement.options","value":self.editor.settings.movement}),
            );
        }
        if selection_changed {
            self.editor.core("ui.dispatch", serde_json::json!({"id":"selection.options","value":self.editor.settings.selection}));
        }
        if painting_changed {
            self.editor.core(
                "brush.set",
                serde_json::to_value(&self.editor.settings.painting).unwrap(),
            );
            self.editor.sync_from_session();
        }
        if reset_layout {
            self.dock_before_focus = None;
            self.reset_layout(size.x - 48. - self.editor.settings.ui.outer_padding * 3.);
        }
        if save {
            match self.editor.save_settings_document() {
                Ok(()) => {
                    self.settings_before = Some(self.editor.settings.clone());
                    prefs.selection_before = Some(self.editor.session.selection_sampling());
                    prefs.movement_before = Some(self.editor.session.movement_options());
                    prefs.painting_before =
                        Some(PaintingPreferences::from(&self.editor.session.parameters));
                    prefs.json = self.editor.settings.encode().unwrap_or_default();
                    prefs.message = "Saved to settings.json".into();
                    self.editor.status = prefs.message.clone();
                }
                Err(error) => prefs.message = format!("Save failed: {error}"),
            }
        }
        self.preferences = prefs;
        if !open || cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.cancel_settings();
        }
    }
    /// Reorder and show or hide Tools-strip icons; writes `ui.tool_order` and `ui.hidden_tools`.
    fn toolbar_settings(
        &mut self,
        ui: &mut Ui,
        draft: &mut Settings,
        document: &mut serde_json::Value,
    ) {
        ui.strong(tr("Tools Icons"));
        ui.weak(tr("Order and Visibility of Icons in the Tools Window"));
        ui.label(tr(RichText::new(localize(
            "ui.tool_order · ui.hidden_tools",
        ))
        .small()
        .monospace()));
        let mut catalog = self.editor.toolbar_catalog();
        let mut items = vec![];
        for id in &draft.ui.tool_order {
            if let Some(index) = catalog.iter().position(|item| &item.id == id) {
                items.push(catalog.remove(index));
            }
        }
        items.extend(catalog);
        let mut order: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
        let mut hidden = draft.ui.hidden_tools.clone();
        let mut changed = false;
        let count = items.len();
        for (index, item) in items.iter().enumerate() {
            ui.push_id(("toolbar-item", &item.id), |ui| {
                ui.horizontal(|ui| {
                    let mut visible = !hidden.contains(&item.id);
                    if ui.checkbox(&mut visible, "").changed() {
                        hidden.retain(|id| id != &item.id);
                        if !visible {
                            hidden.push(item.id.clone());
                        }
                        changed = true;
                    }
                    ui.label(tr(theme::icon(item.icon, 18.)));
                    ui.allocate_ui_with_layout(
                        vec2(140., ui.spacing().interact_size.y),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_width(140.);
                            ui.add(
                                Label::new(tr_for(item.owner.as_deref(), &item.title)).truncate(),
                            );
                        },
                    );
                    if ui
                        .add_enabled(index > 0, Button::new(tr(theme::icon('\u{e5d8}', 16.))))
                        .on_hover_text(tr("Move Up"))
                        .clicked()
                    {
                        order.swap(index, index - 1);
                        changed = true;
                    }
                    if ui
                        .add_enabled(
                            index + 1 < count,
                            Button::new(tr(theme::icon('\u{e5db}', 16.))),
                        )
                        .on_hover_text(tr("Move Down"))
                        .clicked()
                    {
                        order.swap(index, index + 1);
                        changed = true;
                    }
                });
            });
        }
        if (!draft.ui.tool_order.is_empty() || !draft.ui.hidden_tools.is_empty())
            && ui.small_button(tr("Reset")).clicked()
        {
            order.clear();
            hidden.clear();
            changed = true;
        }
        if changed {
            draft.ui.tool_order = order;
            draft.ui.hidden_tools = hidden;
            document["ui"]["tool_order"] = serde_json::json!(draft.ui.tool_order);
            document["ui"]["hidden_tools"] = serde_json::json!(draft.ui.hidden_tools);
        }
        ui.add_space(8.);
        ui.separator();
        ui.add_space(8.);
    }
    #[allow(clippy::too_many_arguments)]
    fn settings_content(
        &mut self,
        ui: &mut Ui,
        ctx: &Context,
        prefs: &mut Preferences,
        draft: &mut Settings,
        document: &mut serde_json::Value,
        defaults: &serde_json::Value,
        reset_layout: &mut bool,
    ) {
        let mut count = 0;
        if prefs.query.trim().is_empty()
            && matches!(prefs.category, Category::All | Category::Appearance)
        {
            ui.horizontal_wrapped(|ui| {
                ui.strong(tr("UI Density"));
                if ui.button(tr("Compact")).clicked() {
                    draft.ui.compact();
                    document["ui"] = serde_json::to_value(&draft.ui).unwrap();
                }
                if ui.button(tr("Comfortable")).clicked() {
                    let theme = draft.ui.theme.clone();
                    let accent = draft.ui.accent.clone();
                    draft.ui = Appearance::default();
                    draft.ui.theme = theme;
                    draft.ui.accent = accent;
                    document["ui"] = serde_json::to_value(&draft.ui).unwrap();
                }
            });
            ui.add_space(12.);
        }
        for item in SETTINGS {
            let modified = value(document, item.id) != value(defaults, item.id);
            if prefs.category != Category::All && prefs.category != item.category {
                continue;
            }
            if !matches(
                &prefs.query,
                &format!(
                    "{} {} {} {} {}",
                    item.id,
                    crate::i18n::search_text(item.title),
                    crate::i18n::search_text(item.description),
                    item.keywords,
                    item.category.title()
                ),
                modified,
            ) {
                continue;
            }
            count += 1;
            ui.push_id(item.id, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(tr(item.title));
                    if modified {
                        ui.weak(tr("Modified"));
                    }
                });
                ui.weak(tr(item.description));
                ui.label(tr(RichText::new(localize(item.id)).small().monospace()));
                let mut v = value(document, item.id).clone();
                ui.horizontal_wrapped(|ui| {
                    if let Some((low, high)) = item.range {
                        let mut number = v.as_f64().unwrap();
                        let step = if high <= 4. { 0.01 } else { 1. };
                        let response =
                            ui.add(DragValue::new(&mut number).range(low..=high).speed(step));
                        #[cfg(test)]
                        ui.data_mut(|data| {
                            data.insert_temp(Id::new(("setting-number", item.id)), response.rect)
                        });
                        if response.changed() {
                            v = if v.is_u64() {
                                serde_json::json!(number as u64)
                            } else {
                                serde_json::json!(number)
                            };
                        }
                    } else if let Some(mut enabled) = v.as_bool() {
                        if ui.checkbox(&mut enabled, tr("Enabled")).changed() {
                            v = serde_json::json!(enabled);
                        }
                    } else if item.id == "painting.opacity_mode" {
                        let mut mode: crate::application::OpacityMode =
                            serde_json::from_value(v.clone()).unwrap_or_default();
                        ComboBox::from_id_salt(item.id)
                            .selected_text(tr(brush::opacity_mode_title(mode)))
                            .show_ui(ui, |ui| {
                                for candidate in brush::OPACITY_MODES {
                                    ui.selectable_value(
                                        &mut mode,
                                        candidate,
                                        tr(brush::opacity_mode_title(candidate)),
                                    );
                                }
                            });
                        v = serde_json::to_value(mode).unwrap();
                    } else if item.id == "movement.target" {
                        let mut target: crate::application::MoveTarget =
                            serde_json::from_value(v.clone()).unwrap_or_default();
                        ComboBox::from_id_salt(item.id)
                            .selected_text(tr(canvas_movement::target_title(target)))
                            .show_ui(ui, |ui| {
                                for t in canvas_movement::TARGETS {
                                    ui.selectable_value(
                                        &mut target,
                                        t,
                                        tr(canvas_movement::target_title(t)),
                                    );
                                }
                            });
                        v = serde_json::to_value(target).unwrap();
                    } else if item.id == "ui.language" {
                        let mut language = v.as_str().unwrap_or("en").to_owned();
                        ComboBox::from_id_salt("display-language")
                            .selected_text(
                                crate::i18n::Language::ALL
                                    .into_iter()
                                    .find(|l| l.code() == language)
                                    .unwrap_or_default()
                                    .name(),
                            )
                            .show_ui(ui, |ui| {
                                for locale in crate::i18n::Language::ALL {
                                    ui.selectable_value(
                                        &mut language,
                                        locale.code().into(),
                                        locale.name(),
                                    );
                                }
                            });
                        v = serde_json::json!(language);
                    } else if item.id == "ui.theme" {
                        let mut name = v.as_str().unwrap().to_owned();
                        ComboBox::from_id_salt(item.id)
                            .selected_text(tr(match name.as_str() {
                                "light" => "Light",
                                "warm" => "Warm Gray",
                                _ => "Graphite",
                            }))
                            .show_ui(ui, |ui| {
                                for (id, title) in [
                                    ("graphite", "Graphite"),
                                    ("light", "Light"),
                                    ("warm", "Warm Gray"),
                                ] {
                                    ui.selectable_value(&mut name, id.into(), tr(title));
                                }
                            });
                        v = serde_json::json!(name);
                    } else {
                        let color = theme::Colors::new(&draft.ui).accent;
                        let mut rgb = [color.r(), color.g(), color.b()];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            v = serde_json::json!(format!(
                                "#{:02X}{:02X}{:02X}",
                                rgb[0], rgb[1], rgb[2]
                            ));
                        }
                        ui.label(tr(v.as_str().unwrap_or("")));
                    }
                    if modified && ui.small_button(tr("Reset")).clicked() {
                        v = value(defaults, item.id).clone();
                    }
                });
                set_value(document, item.id, v);
                ui.add_space(8.);
                ui.separator();
                ui.add_space(8.);
            });
        }
        let presets_modified = document["new_canvas"] != defaults["new_canvas"];
        if matches!(prefs.category, Category::All | Category::Painting)
            && matches(
                &prefs.query,
                &format!(
                    "new_canvas.default_preset new_canvas.presets document paper background resolution {} {} {}",
                    crate::i18n::search_text("New Canvas Presets"),
                    crate::i18n::search_text("Choose the starting preset. Presets affect new documents only."),
                    draft.new_canvas.presets.iter().map(|p| format!("{} {}", p.id, p.name)).collect::<Vec<_>>().join(" ")
                ),
                presets_modified,
            )
        {
            count += 1;
            ui.strong(tr("New Canvas Presets"));
            ui.weak(tr("Choose the starting preset. Presets affect new documents only."));
            ui.monospace("new_canvas.default_preset · new_canvas.presets");
            let mut presets = draft.new_canvas.clone();
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("Default Preset"));
                ComboBox::from_id_salt("new-canvas-default-preset")
                    .selected_text(presets.selected().map(canvas_new_document::preset_title).unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for preset in &presets.presets {
                            ui.selectable_value(
                                &mut presets.default_preset,
                                preset.id.clone(),
                                canvas_new_document::preset_title(preset),
                            );
                        }
                    });
                if presets_modified && ui.small_button(tr("Reset")).clicked() {
                    presets = Default::default();
                }
            });
            for preset in &presets.presets {
                ui.horizontal_wrapped(|ui| {
                    ui.label(canvas_new_document::preset_title(preset));
                    ui.weak(format!("{} × {} px · {} PPI",preset.width,preset.height,preset.resolution_ppi));
                    ui.weak(if preset.background == "transparent" { localize("Transparent").into_owned() } else {preset.background.clone()});
                });
            }
            ui.weak(tr("Edit preset names, dimensions, resolution and backgrounds in JSON settings."));
            document["new_canvas"] = serde_json::to_value(presets).unwrap();
            ui.add_space(draft.ui.section_spacing);
            ui.separator();
            ui.add_space(draft.ui.item_spacing);
        }
        if matches!(prefs.category, Category::All | Category::Layout)
            && matches(
                &prefs.query,
                "workspace layout 工作区 布局 重置面板 停靠",
                false,
            )
        {
            count += 1;
            ui.strong(tr("Panel Layout"));
            ui.weak(tr(
                "Groups, Floating Panels and Positions Are Saved in workspace.egui.json.",
            ));
            if ui.button(tr("Reset Layout to Default Widths")).clicked() {
                *reset_layout = true;
            }
            ui.add_space(12.);
        }
        if matches!(prefs.category, Category::All | Category::Layout)
            && matches(
                &prefs.query,
                "tools toolbar icons order visible ui.tool_order ui.hidden_tools 工具 图标 顺序 显示",
                draft.ui.tool_order != defaults_ui(defaults).tool_order
                    || draft.ui.hidden_tools != defaults_ui(defaults).hidden_tools,
            )
        {
            count += 1;
            self.toolbar_settings(ui, draft, document);
        }
        if matches!(prefs.category, Category::All | Category::Keys) {
            count += keybindings(ui, ctx, prefs, draft);
        }
        if matches!(prefs.category, Category::All | Category::Plugins) {
            let registry = &self.editor.session.registry;
            if matches(&prefs.query, "plugins directory 插件 目录", false) {
                count += 1;
                ui.strong(tr("Plugin Directory"));
                ui.weak(tr(registry.root.display().to_string()));
                if ui.small_button(tr("Copy Plugin Directory")).clicked() {
                    ctx.copy_text(registry.root.display().to_string());
                }
            }
            ui.weak(tr(
                "Enabling includes dependencies; disabling includes dependent plugins.",
            ));
            for plugin in &registry.packages {
                let id = plugin.id.as_deref();
                let disabled = id.is_some_and(|id| draft.plugins.disabled.iter().any(|v| v == id));
                if !matches(
                    &prefs.query,
                    &format!(
                        "plugins {} {} {} {}",
                        id.unwrap_or(""),
                        plugin.name,
                        plugin.status.title(),
                        plugin.error.as_deref().unwrap_or("")
                    ),
                    disabled,
                ) {
                    continue;
                }
                count += 1;
                ui.push_id(&plugin.manifest, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(tr_for(plugin.id.as_deref(), &plugin.name));
                        if let Some(version) = &plugin.version {
                            ui.weak(version);
                        }
                        ui.label(tr(plugin.status.title()));
                    });
                    if let Some(author) = &plugin.author {
                        ui.weak(author);
                    }
                    if !plugin.description.is_empty() {
                        ui.label(tr_for(plugin.id.as_deref(), &plugin.description));
                    }
                    if let Some(id) = id {
                        ui.weak(id);
                        let essential = ["core", "painting", "workbench"].contains(&id);
                        let mut enabled = !disabled && plugin.manifest_enabled;
                        if ui
                            .add_enabled(
                                !essential
                                    && plugin.manifest_enabled
                                    && crate::plugins::valid_id(id),
                                Checkbox::new(&mut enabled, tr("Enable Plugin")),
                            )
                            .changed()
                        {
                            match registry.plan_enabled(&draft.plugins.disabled, id, enabled) {
                                Ok(plan) => draft.plugins.disabled = plan.disabled,
                                Err(error) => self.editor.status = error,
                            }
                        }
                        if essential {
                            ui.weak(tr("Built-in Core Plugin"));
                        }
                        if disabled != registry.disabled.iter().any(|v| v == id) {
                            ui.label(tr("Restart Required"));
                        }
                    }
                    if let Some(minimum) = &plugin.min_host_version {
                        ui.weak(tr(format!("Requires MiCoPi {minimum}")));
                    }
                    if !plugin.requires.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.weak(tr("Dependencies"));
                            ui.weak(plugin.requires.join(", "));
                        });
                    }
                    ui.weak(tr(format!(
                        "{} tools · {} actions · {} panels · {}",
                        plugin.contributions.get("tools").copied().unwrap_or(0),
                        plugin.contributions.get("actions").copied().unwrap_or(0),
                        plugin.contributions.get("panels").copied().unwrap_or(0),
                        crate::i18n::text(plugin.status.title())
                    )));
                    if !plugin.manifest_enabled && id.is_some() {
                        ui.weak(tr("Manifest Has enabled=false; Edit It and Restart."));
                    }
                    if let Some(error) = &plugin.error {
                        ui.colored_label(ui.visuals().error_fg_color, tr(error));
                    }
                    ui.weak(plugin.manifest.display().to_string());
                    ui.separator();
                    ui.add_space(8.);
                });
            }
        }
        prefs.result_count = count;
        if count == 0 {
            ui.add_space(24.);
            ui.weak(tr(
                "No matching settings. Try a setting ID or clear @modified.",
            ));
        }
    }
}

#[cfg(test)]
#[path = "brush_opacity_settings_tests.rs"]
mod brush_opacity_settings_tests;
fn category_buttons(ui: &mut Ui, prefs: &mut Preferences) {
    for category in [
        Category::All,
        Category::Appearance,
        Category::Layout,
        Category::Painting,
        Category::Keys,
        Category::Plugins,
    ] {
        if ui
            .selectable_label(prefs.category == category, tr(category.title()))
            .clicked()
        {
            prefs.category = category;
        }
    }
}
fn keybindings(ui: &mut Ui, ctx: &Context, prefs: &mut Preferences, draft: &mut Settings) -> usize {
    let defaults = Keymap::default();
    let mut count = 0;
    let commands = draft.keys.clone();
    for (index, command) in commands.commands().enumerate() {
        let current = draft.keys.label(index);
        let modified = current != defaults.label(index);
        if !matches(
            &prefs.query,
            &format!(
                "keybindings keyboard 快捷键 按键 {} {} {} {} {}",
                command.id,
                crate::i18n::search_text_for(command.owner, command.title),
                crate::i18n::search_text(command.category),
                command.keywords,
                current
            ),
            modified,
        ) {
            continue;
        }
        count += 1;
        ui.push_id(command.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(tr_for(command.owner, command.title));
                ui.weak(tr(command.category));
                if modified {
                    ui.weak(tr("Modified"));
                }
            });
            ui.label(tr(RichText::new(localize(format!(
                "keybindings.{}",
                command.id
            )))
            .small()
            .monospace()));
            if prefs.binding == Some(index) {
                ui.add(
                    TextEdit::singleline(&mut prefs.binding_draft)
                        .hint_text(tr("Ctrl+K, Shift+[; Leave Empty to Disable")),
                );
                if prefs.capture {
                    ui.weak(tr("Press a Shortcut or Scroll…"));
                    let chord = ctx.input(|i| {
                        i.events.iter().find_map(|event| match event {
                            Event::Key {
                                key,
                                physical_key,
                                pressed: true,
                                modifiers,
                                ..
                            } if !matches!(key, Key::Escape | Key::Enter) => {
                                super::commands::key_to_vk(physical_key.unwrap_or(*key)).map(
                                    |key| Chord {
                                        key,
                                        mods: super::commands::mod_bits(*modifiers),
                                    },
                                )
                            }
                            Event::MouseWheel { modifiers, .. } => Some(Chord {
                                key: crate::ui::keymap::WHEEL,
                                mods: super::commands::mod_bits(*modifiers),
                            }),
                            _ => None,
                        })
                    });
                    if let Some(chord) = chord {
                        prefs.binding_draft = chord.text();
                        prefs.capture = false;
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Apply Binding")).clicked() {
                        let keys = if prefs.binding_draft.trim().is_empty() {
                            Ok(vec![])
                        } else {
                            prefs
                                .binding_draft
                                .split(',')
                                .map(Chord::parse)
                                .collect::<Result<Vec<_>, _>>()
                        };
                        match keys.and_then(|keys| draft.keys.changed(index, keys)) {
                            Ok(keys) => {
                                draft.keys = keys;
                                prefs.binding = None;
                                prefs.message =
                                    "Binding Previewed; Save Settings to Keep It".into();
                            }
                            Err(error) => prefs.message = error,
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        prefs.binding = None;
                        prefs.capture = false;
                    }
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    if current.is_empty() {
                        ui.weak(tr("Unbound"));
                    } else {
                        theme::keycaps(ui, &current);
                    }
                    if ui.small_button(tr("Edit")).clicked() {
                        prefs.binding = Some(index);
                        prefs.binding_draft = current.clone();
                        prefs.capture = false;
                    }
                    if ui.small_button(tr("Record")).clicked() {
                        prefs.binding = Some(index);
                        prefs.binding_draft = current;
                        prefs.capture = true;
                    }
                    if modified && ui.small_button(tr("Reset")).clicked() {
                        let keys = defaults
                            .label(index)
                            .split(',')
                            .filter(|s| !s.trim().is_empty())
                            .map(Chord::parse)
                            .collect::<Result<Vec<_>, _>>();
                        match keys.and_then(|keys| draft.keys.changed(index, keys)) {
                            Ok(keys) => draft.keys = keys,
                            Err(error) => prefs.message = error,
                        }
                    }
                });
            }
            ui.separator();
            ui.add_space(8.);
        });
    }
    count
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn search_matches_names_ids_multiple_terms_and_modified_filter() {
        let item = "ui.panel_padding 面板内边距 spacing 布局";
        assert!(matches("面板 边距", item, false));
        assert!(matches("UI.PANEL", item, false));
        assert!(!matches("@modified 边距", item, false));
        assert!(matches("@modified 边距", item, true));
        assert!(!matches("压感", item, true));
    }
    #[test]
    fn json_editor_rejects_invalid_values_and_previews_valid_documents() {
        use crate::desktop::tests::{frame, studio};
        fn text_center(shapes: &[egui::epaint::ClippedShape], label: &str) -> Pos2 {
            fn find(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| find(shape, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| find(&shape.shape, label))
                .unwrap()
        }
        let mut app = studio();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        app.open_settings(None);
        app.preferences.json_mode = true;
        let baseline = app.editor.settings.ui.panel_padding;
        for (step, json, valid) in [
            (0, r#"{"ui":{"panel_padding":-9}}"#, false),
            (
                1,
                r#"{"ui":{"panel_padding":24},"extension":{"test":7}}"#,
                true,
            ),
        ] {
            app.preferences.json = json.into();
            let mut output = frame(&mut app, &ctx, size, vec![], step as f64 * 10.);
            for i in 1..3 {
                output = frame(&mut app, &ctx, size, vec![], step as f64 * 10. + i as f64);
            }
            let pos = text_center(&output.shapes, "Validate and Preview");
            let _ = frame(
                &mut app,
                &ctx,
                size,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                step as f64 * 10. + 3.,
            );
            let _ = frame(
                &mut app,
                &ctx,
                size,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
                step as f64 * 10. + 3.1,
            );
            assert_eq!(
                app.editor.settings.ui.panel_padding,
                if valid { 24. } else { baseline }
            );
            if valid {
                assert!(app
                    .editor
                    .settings
                    .encode()
                    .unwrap()
                    .contains("\"test\": 7"));
            }
        }
        app.cancel_settings();
        assert_eq!(app.editor.settings.ui.panel_padding, baseline);
    }
    #[test]
    fn near_limit_profile_remains_editable_and_failed_save_preserves_the_file() {
        fn frame(app: &mut Studio, ctx: &Context, events: Vec<Event>, time: f64) -> FullOutput {
            ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1360., 920.))),
                    events,
                    time: Some(time),
                    focused: true,
                    ..Default::default()
                },
                |ctx| app.settings_window(ctx),
            )
        }
        fn text_center(shape: &Shape, label: &str) -> Option<Pos2> {
            match shape {
                Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, label)),
                _ => None,
            }
        }
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/brush-presets/settings-editor-limit/{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("settings.json");
        let notes = "x".repeat(1024 * 1024 - 64);
        let raw = serde_json::json!({"artist_notes":notes}).to_string();
        assert!(raw.len() < 1024 * 1024);
        std::fs::write(&path, &raw).unwrap();
        let mut app = crate::desktop::tests::studio();
        app.editor.settings = Settings::decode(&raw, path.clone()).unwrap();
        assert!(app.editor.settings.encode().unwrap_err().contains("1 MB"));
        app.editor.session.settings_path = Some(path.clone());
        app.open_settings(None);
        let editable: serde_json::Value = serde_json::from_str(&app.preferences.json).unwrap();
        assert_eq!(editable["artist_notes"], notes);
        let ctx = Context::default();
        crate::i18n::set_language(crate::i18n::Language::English);
        theme::fonts(&ctx);
        theme::apply(&ctx, &app.editor.settings.ui);
        let mut output = frame(&mut app, &ctx, vec![], 0.);
        for step in 1..3 {
            output = frame(&mut app, &ctx, vec![], step as f64 / 10.);
        }
        let save = output
            .shapes
            .iter()
            .find_map(|shape| text_center(&shape.shape, "Save Settings"))
            .unwrap();
        for (step, pressed) in [true, false].into_iter().enumerate() {
            frame(
                &mut app,
                &ctx,
                vec![
                    Event::PointerMoved(save),
                    Event::PointerButton {
                        pos: save,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                1. + step as f64 / 10.,
            );
        }
        assert!(app.preferences.message.starts_with("Save failed:"));
        assert!(app.preferences.message.contains("1 MB"));
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        app.preferences.json_mode = true;
        frame(&mut app, &ctx, vec![], 2.);
        assert!(app.settings_open);
        let editable: serde_json::Value = serde_json::from_str(&app.preferences.json).unwrap();
        assert_eq!(editable["artist_notes"], notes);
        assert!(Settings::decode(&app.preferences.json, path.clone())
            .err()
            .unwrap()
            .contains("1 MB"));
        assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
        app.cancel_settings();
    }
}
fn defaults_ui(defaults: &serde_json::Value) -> Appearance {
    serde_json::from_value(defaults["ui"].clone()).unwrap_or_default()
}

#[cfg(test)]
mod selection_settings_tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    use serde_json::json;
    fn text_rect(shape: &Shape, text: &str) -> Option<Rect> {
        match shape {
            Shape::Text(t) if t.galley.text() == text => {
                Some(Rect::from_min_size(t.pos, t.galley.size()))
            }
            Shape::Vec(v) => v.iter().find_map(|s| text_rect(s, text)),
            _ => None,
        }
    }
    fn locate(app: &mut Studio, ctx: &Context, text: &str, time: f64) -> Pos2 {
        let out = frame(app, ctx, vec2(1360., 920.), vec![], time);
        out.shapes
            .iter()
            .find_map(|s| text_rect(&s.shape, text))
            .unwrap_or_else(|| panic!("Missing control: {text}"))
            .center()
    }
    fn click(app: &mut Studio, ctx: &Context, p: Pos2, t: f64) {
        for (i, pressed) in [true, false].into_iter().enumerate() {
            frame(
                app,
                ctx,
                vec2(1360., 920.),
                vec![
                    Event::PointerMoved(p),
                    Event::PointerButton {
                        pos: p,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
                t + i as f64 * 0.01,
            );
        }
    }
    fn edit_tolerance(app: &mut Studio, ctx: &Context, t: f64) {
        edit_integer(app, ctx, t, "selection.tolerance", "32", "7");
    }
    #[test]
    fn new_canvas_search_default_preview_cancel_save_keeps_artwork_and_custom_names() {
        let mut app = studio();
        let ctx = Context::default();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/new-canvas-settings-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        app.editor.settings = Settings::defaults(dir.join("settings.json"));
        app.editor.settings.new_canvas.presets[3].name = "私の原稿".into();
        for i in 0..3 {
            frame(
                &mut app,
                &ctx,
                vec2(1360., 920.),
                vec![],
                -1. + i as f64 * 0.1,
            );
        }
        let revision = app.editor.session.revision;
        let composition = app.editor.session.doc.composition();
        let info = app.editor.session.doc.info();
        let dimensions = (
            app.editor.session.doc.width,
            app.editor.session.doc.height,
            app.editor.session.doc.active,
            app.editor.session.doc.dirty,
        );
        let history = app.editor.session.doc.history_bytes();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        for pass in 0..2 {
            let t = pass as f64 * 10.;
            app.open_settings(None);
            app.preferences.query = "new_canvas.presets".into();
            for i in 0..3 {
                frame(
                    &mut app,
                    &ctx,
                    vec2(1360., 920.),
                    vec![],
                    t + i as f64 * 0.1,
                );
            }
            let p = locate(&mut app, &ctx, "Illustration", t + 0.3);
            click(&mut app, &ctx, p, t + 0.4);
            let out = frame(&mut app, &ctx, vec2(1360., 920.), vec![], t + 0.5);
            let p = out
                .shapes
                .iter()
                .rev()
                .find_map(|s| text_rect(&s.shape, "Square"))
                .expect("Square choice in the preset popup")
                .center();
            click(&mut app, &ctx, p, t + 0.6);
            assert_eq!(app.editor.settings.new_canvas.default_preset, "square");
            assert_eq!(app.editor.session.revision, revision);
            if pass == 1 {
                let p = locate(&mut app, &ctx, "Save Settings", t + 0.8);
                click(&mut app, &ctx, p, t + 0.9);
                let stored = Settings::load(app.editor.settings.path.clone()).unwrap();
                assert_eq!(stored.new_canvas.default_preset, "square");
                assert_eq!(stored.new_canvas.presets[3].name, "私の原稿");
            }
            app.cancel_settings();
            assert_eq!(
                app.editor.settings.new_canvas.default_preset,
                if pass == 0 { "illustration" } else { "square" }
            );
            assert_eq!(app.editor.settings.new_canvas.presets[3].name, "私の原稿");
            assert_eq!(app.editor.session.revision, revision + pass);
            assert_eq!(app.editor.session.doc.composition(), composition);
            assert_eq!(app.editor.session.doc.info(), info);
            assert_eq!(
                (
                    app.editor.session.doc.width,
                    app.editor.session.doc.height,
                    app.editor.session.doc.active,
                    app.editor.session.doc.dirty
                ),
                dimensions
            );
            assert_eq!(app.editor.session.doc.history_bytes(), history);
            assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        }
    }
    fn edit_integer(app: &mut Studio, ctx: &Context, t: f64, query: &str, old: &str, value: &str) {
        app.open_settings(None);
        app.preferences.query = query.into();
        // Allow the settings window's first-frame position/size to settle.
        for i in 0..3 {
            frame(app, ctx, vec2(1360., 920.), vec![], t + i as f64 * 0.1);
        }
        let p = locate(app, ctx, old, t + 0.3);
        click(app, ctx, p, t + 0.4);
        click(app, ctx, p, t + 0.5);
        frame(
            app,
            ctx,
            vec2(1360., 920.),
            vec![
                Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::CTRL,
                },
                Event::Text(value.into()),
                Event::Key {
                    key: Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
            ],
            t + 0.6,
        );
        frame(app, ctx, vec2(1360., 920.), vec![], t + 0.7);
    }
    #[test]
    fn movement_searchable_steps_preview_cancel_save_without_mutating_pixels() {
        let mut app = studio();
        let ctx = Context::default();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/movement-settings-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        app.editor.settings = Settings::defaults(dir.join("settings.json"));
        app.editor.session.settings_path = Some(app.editor.settings.path.clone());
        app.editor.core("ui.dispatch",json!({"id":"movement.options","value":{"target":"selection_mask","step":3,"large_step":30}}));
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        edit_integer(&mut app, &ctx, 0., "movement.step", "1", "7");
        assert_eq!(app.editor.session.movement_options().step, 7);
        assert_eq!(app.editor.session.movement_defaults.step, 1);
        app.cancel_settings();
        assert_eq!(app.editor.session.movement_options().step, 3);
        assert_eq!(
            app.editor.session.movement_options().target,
            crate::application::MoveTarget::SelectionMask
        );
        edit_integer(&mut app, &ctx, 2., "movement.step", "1", "7");
        let p = locate(&mut app, &ctx, "Save Settings", 3.);
        click(&mut app, &ctx, p, 3.1);
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 3.2);
        assert_eq!(
            Settings::load(app.editor.settings.path.clone())
                .unwrap()
                .movement
                .step,
            7
        );
        assert_eq!(app.editor.session.movement_defaults.step, 7);
        app.cancel_settings();
        assert_eq!(app.editor.session.movement_options().step, 7);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
    }
    #[test]
    fn searchable_integer_control_previews_cancels_saves_and_preserves_other_profile_writes() {
        let mut app = studio();
        let ctx = Context::default();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-output/selection-settings-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        app.editor.settings = Settings::defaults(dir.join("settings.json"));
        app.editor.session.settings_path = Some(app.editor.settings.path.clone());
        app.editor.core("ui.dispatch",json!({"id":"selection.options","value":{"tolerance":99,"contiguous":false,"sample_merged":true}}));
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        edit_tolerance(&mut app, &ctx, 0.);
        assert_eq!(app.editor.settings.selection.tolerance, 7);
        assert_eq!(app.editor.session.selection_sampling().tolerance, 7);
        assert_eq!(app.editor.session.selection_defaults.tolerance, 32);
        app.cancel_settings();
        assert_eq!(app.editor.session.selection_sampling().tolerance, 99);
        assert_eq!(app.editor.settings.selection.tolerance, 32);
        edit_tolerance(&mut app, &ctx, 2.);
        let save = locate(&mut app, &ctx, "Save Settings", 3.);
        click(&mut app, &ctx, save, 3.1);
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 3.2);
        assert_eq!(
            Settings::load(app.editor.settings.path.clone())
                .unwrap()
                .selection
                .tolerance,
            7
        );
        assert_eq!(app.editor.session.selection_defaults.tolerance, 7);
        app.cancel_settings();
        let other = dir.join("other.json");
        app.editor.core(
            "settings.update",
            json!({"path":other,"selection":{"tolerance":13}}),
        );
        app.consume_ui_effects(&ctx);
        assert_eq!(app.editor.session.selection_defaults.tolerance, 7);
        assert_eq!(app.editor.session.selection_sampling().tolerance, 7);
        let active = app.editor.settings.path.clone();
        app.editor.core(
            "settings.update",
            json!({"path":active,"selection":{"tolerance":9}}),
        );
        assert_eq!(app.editor.session.selection_defaults.tolerance, 9);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
    }
}
