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
impl Studio {
    pub(super) fn open_settings(&mut self, category: Option<Category>) {
        if !self.settings_open || self.settings_before.is_none() {
            self.preferences.painting_before =
                Some(PaintingPreferences::from(&self.editor.session.parameters));
            self.settings_before = Some(self.editor.settings.clone());
            self.preferences.json = self.editor.settings.encode().unwrap_or_default();
            self.preferences.message.clear();
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
        let mut document: serde_json::Value =
            serde_json::from_str(&draft.encode().unwrap()).unwrap();
        let defaults: serde_json::Value =
            serde_json::from_str(&Settings::defaults(PathBuf::new()).encode().unwrap()).unwrap();
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
                            prefs.json = draft.encode().unwrap_or_default();
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
            ) {
                (Ok(mut appearance), Ok(painting)) => {
                    normalize(&mut appearance);
                    if appearance.validate().is_ok() && painting.validate().is_ok() {
                        draft.ui = appearance;
                        draft.painting = painting;
                    }
                }
                _ => prefs.message = "Invalid Setting Value".into(),
            }
        }
        let painting_changed = draft.painting != self.editor.settings.painting;
        self.editor.settings = draft;
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
            match self.editor.save_settings() {
                Ok(()) => {
                    self.settings_before = Some(self.editor.settings.clone());
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
                        if ui
                            .add(DragValue::new(&mut number).range(low..=high).speed(step))
                            .changed()
                        {
                            v = serde_json::json!(number);
                        }
                    } else if let Some(mut enabled) = v.as_bool() {
                        if ui.checkbox(&mut enabled, tr("Enabled")).changed() {
                            v = serde_json::json!(enabled);
                        }
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
                        ui.strong(tr(&plugin.name));
                        if let Some(version) = &plugin.version {
                            ui.weak(version);
                        }
                        ui.label(tr(plugin.status.title()));
                    });
                    if let Some(author) = &plugin.author {
                        ui.weak(author);
                    }
                    if !plugin.description.is_empty() {
                        ui.label(tr(&plugin.description));
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
                crate::i18n::search_text(command.title),
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
                ui.strong(tr(command.title));
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
}
