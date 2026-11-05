use super::*;
use crate::ui::{
    keymap::{Chord, Kind, ALT, COMMANDS, CTRL, SHIFT},
    palette::Mode,
    shortcuts::brush_diameter,
};

pub(super) fn mod_bits(m: Modifiers) -> u8 {
    (if m.ctrl { CTRL } else { 0 })
        | (if m.shift { SHIFT } else { 0 })
        | (if m.alt { ALT } else { 0 })
}
pub(super) fn key_to_vk(key: Key) -> Option<u16> {
    Some(match key {
        Key::ArrowLeft => 37,
        Key::ArrowUp => 38,
        Key::ArrowRight => 39,
        Key::ArrowDown => 40,
        Key::Enter => 13,
        Key::Tab => 9,
        Key::Space => 32,
        Key::Escape => 27,
        Key::Backspace => 8,
        Key::Delete => 46,
        Key::Home => 36,
        Key::End => 35,
        Key::PageUp => 33,
        Key::PageDown => 34,
        Key::OpenBracket | Key::OpenCurlyBracket => 219,
        Key::CloseBracket | Key::CloseCurlyBracket => 221,
        Key::Plus | Key::Equals => 187,
        Key::Minus => 189,
        Key::Comma => 188,
        Key::Period => 190,
        Key::Slash | Key::Questionmark => 191,
        Key::Semicolon | Key::Colon => 186,
        Key::Quote => 222,
        Key::Backslash | Key::Pipe => 220,
        Key::Backtick => 192,
        _ => {
            let name = key.name();
            if name.len() == 1 && name.as_bytes()[0].is_ascii_alphanumeric() {
                name.as_bytes()[0].to_ascii_uppercase() as u16
            } else if name.starts_with('F') {
                111 + name[1..].parse::<u16>().ok()?
            } else {
                return None;
            }
        }
    })
}
pub(super) fn vk_to_key(vk: u16) -> Option<Key> {
    Key::ALL
        .iter()
        .copied()
        .find(|key| key_to_vk(*key) == Some(vk))
}
impl Editor {
    pub(super) fn open_palette(&mut self, mode: Mode) {
        self.register_commands();
        self.finish_stroke();
        self.palette = Some(Palette::new(mode, &self.settings.keys));
        self.palette_focus = true;
        self.binding_index = None;
        self.capture_binding = false;
    }
    pub(super) fn shortcuts(&mut self, ctx: &Context) {
        if self.settings.keys.bindings.len() == COMMANDS.len() {
            self.register_commands();
        }
        if self.palette.is_some() || self.blocked || ctx.wants_keyboard_input() {
            return;
        }
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            if let Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                repeat,
                ..
            } = event
            {
                // Physical punctuation retains PS bindings on non-US keyboard layouts.
                let Some(vk) = key_to_vk(physical_key.unwrap_or(key)) else {
                    continue;
                };
                if self
                    .settings
                    .keys
                    .hold_event(vk as usize, mod_bits(modifiers))
                {
                    continue;
                }
                if let Some(action) = self.settings.keys.resolve(vk as usize, mod_bits(modifiers)) {
                    if matches!(action, Action::UiCommand(10176..=10183)) && self.area_mode != 8 {
                        continue;
                    }
                    if !repeat
                        || matches!(
                            action,
                            Action::UiCommand(10176..=10183)
                                | Action::SmallerBrush
                                | Action::LargerBrush
                                | Action::SofterBrush
                                | Action::HarderBrush
                        )
                    {
                        self.actions.push(action);
                    }
                }
            }
        }
    }
    fn begin_binding(&mut self, index: usize, record: bool) {
        self.binding_index = Some(index);
        self.binding_draft = self.settings.keys.label(index);
        self.capture_binding = record;
        self.palette_focus = true;
    }
    fn change_binding(&mut self, index: usize, text: &str) -> Result<(), String> {
        let keys = if text.trim().is_empty() {
            vec![]
        } else {
            text.split(',')
                .map(Chord::parse)
                .collect::<Result<Vec<_>, _>>()?
        };
        let changed = self.settings.keys.changed(index, keys)?;
        let before = std::mem::replace(&mut self.settings.keys, changed);
        if let Err(e) = self.save_settings() {
            self.settings.keys = before;
            return Err(e);
        }
        self.binding_index = None;
        self.capture_binding = false;
        Ok(())
    }
    pub(super) fn palette_ui(&mut self, ctx: &Context) {
        let Some(mut p) = self.palette.take() else {
            return;
        };
        let mut open = true;
        let mut execute = None;
        let mut edit = None;
        let mut reset = false;
        let mut clear = false;
        let width = (ctx.content_rect().width() - 96.).min(760.);
        let a = self.settings.ui.clone();
        let c = theme::Colors::new(&a);
        let shown = Window::new(tr(if p.mode == Mode::Commands {
            "Command Palette"
        } else {
            "Keyboard Shortcuts"
        }))
        .id(Id::new("command-palette"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(width)
        .max_width(width)
        .anchor(
            Align2::CENTER_TOP,
            vec2(0., a.title_height + a.toolbar_height + a.outer_padding * 2.),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(tr(
                    theme::icon(
                        if p.mode == Mode::Commands {
                            '\u{ef7a}'
                        } else {
                            '\u{e312}'
                        },
                        20.,
                    )
                    .color(c.accent)),
                );
                let response = ui.add_sized(
                    [ui.available_width(), a.control_height + 8.],
                    TextEdit::singleline(&mut p.query).hint_text(tr("Search by name, keywords or shortcut…")),
                );
                if self.palette_focus && self.binding_index.is_none() {
                    response.request_focus();
                    self.palette_focus = false;
                }
                if response.changed() {
                    p.refresh(&self.settings.keys);
                }
            });
            ui.add_space(a.item_spacing);
            ui.horizontal(|ui| {
                ui.weak(tr("↑↓ Select"));
                theme::keycaps(ui, "Enter");
                ui.weak(tr("Confirm"));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .small_button(tr(if p.mode == Mode::Commands {
                            "Keyboard Shortcuts"
                        } else {
                            "Back to Commands"
                        }))
                        .clicked()
                    {
                        p.mode = if p.mode == Mode::Commands {
                            Mode::Bindings
                        } else {
                            Mode::Commands
                        };
                        p.refresh(&self.settings.keys);
                    }
                });
            });
            ui.add_space(a.item_spacing);
            if self.binding_index.is_none() {
                if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                    p.step(1);
                }
                if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                    p.step(-1);
                }
                if ctx.input(|i| i.key_pressed(Key::Tab)) {
                    p.mode = if p.mode == Mode::Commands {
                        Mode::Bindings
                    } else {
                        Mode::Commands
                    };
                    p.refresh(&self.settings.keys);
                }
                if ctx.input(|i| i.key_pressed(Key::Enter)) {
                    if p.mode == Mode::Commands {
                        execute = p.current();
                    } else {
                        edit = p.current().map(|i| (i, false));
                    }
                }
                if p.mode == Mode::Bindings && ctx.input(|i| i.key_pressed(Key::F2)) {
                    edit = p.current().map(|i| (i, false));
                }
                if p.mode == Mode::Bindings && ctx.input(|i| i.key_pressed(Key::F3)) {
                    edit = p.current().map(|i| (i, true));
                }
            }
            let row_height = a.control_height + 12.;
            ScrollArea::vertical()
                .max_height((ctx.content_rect().height() - 330.).clamp(120., row_height * 8.))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if p.results.is_empty() {
                        ui.add_space(12.);
                        ui.weak(tr("No Matching Commands"));
                    }
                    for (position, &index) in p.results.iter().enumerate() {
                        let (r, response) = ui.allocate_exact_size(
                            vec2(ui.available_width(), row_height),
                            Sense::click(),
                        );
                        if position == p.selected || response.hovered() {
                            ui.painter().rect_filled(r, a.corner_radius, c.raised);
                        }
                        if position == p.selected {
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    r.min + vec2(0., 6.),
                                    vec2(2., row_height - 12.),
                                ),
                                1.,
                                c.accent,
                            );
                        }
                        ui.painter().text(
                            pos2(r.left() + 20., r.center().y),
                            Align2::CENTER_CENTER,localize(
                            theme::action_icon(self.settings.keys.command(index).ui).to_string()),
                            FontId::new(a.icon_size, FontFamily::Name("Material".into())),
                            c.muted,
                        );
                        ui.painter().text(
                            pos2(r.left() + 44., r.center().y - 8.),
                            Align2::LEFT_CENTER,
                            crate::i18n::translated_for(
                                self.settings.keys.command(index).owner,
                                self.settings.keys.command(index).title,
                            ),
                            FontId::proportional(a.font_size),
                            c.text,
                        );
                        ui.painter().text(
                            pos2(r.left() + 44., r.center().y + 11.),
                            Align2::LEFT_CENTER,localize(
                            self.settings.keys.command(index).category),
                            FontId::proportional(a.small_font_size),
                            c.muted,
                        );
                        ui.scope_builder(
                            UiBuilder::new()
                                .max_rect(Rect::from_min_max(
                                    pos2(r.right() - 258., r.top()),
                                    r.max,
                                ))
                                .layout(Layout::left_to_right(Align::Center)),
                            |ui| {
                                let label = self.settings.keys.label(index);
                                if label.is_empty() {
                                    ui.weak(tr("Unbound"));
                                } else {
                                    theme::keycaps(ui, &label);
                                }
                            },
                        );
                        if position == p.selected
                            && ctx.input(|i| {
                                i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)
                            })
                        {
                            response.scroll_to_me(Some(Align::Center));
                        }
                        if response.clicked() {
                            p.selected = position;
                            if p.mode == Mode::Commands {
                                execute = Some(index);
                            } else {
                                edit = Some((index, false));
                            }
                        }
                    }
                });
            ui.add_space(a.item_spacing);
            ui.separator();
            if p.mode == Mode::Bindings {
                ui.horizontal(|ui| {
                    if ui.button(tr("Edit Shortcut")).clicked() {
                        edit = p.current().map(|i| (i, false));
                    }
                    theme::keycaps(ui, "F2");
                    if ui.button(tr("Record Shortcut")).clicked() {
                        edit = p.current().map(|i| (i, true));
                    }
                    theme::keycaps(ui, "F3");
                    if ui.button(tr("Clear Binding")).clicked() {
                        clear = true;
                    }
                    if ui.button(tr("Restore Defaults")).clicked() {
                        reset = true;
                    }
                });
            }
            if !p.message.is_empty() {
                ui.colored_label(c.accent,tr( &p.message));
            }
            if let Some(index) = self.binding_index {
                ui.add_space(a.item_spacing);
                ui.label(tr(format!("Edit: {}", self.settings.keys.command(index).title)));
                let response = ui.add_sized(
                    [ui.available_width(), a.control_height + 4.],
                    TextEdit::singleline(&mut self.binding_draft)
                        .hint_text(tr("Ctrl+K, Shift+[; Leave Empty to Disable")),
                );
                if self.palette_focus {
                    response.request_focus();
                    self.palette_focus = false;
                }
                if self.capture_binding {
                    ui.colored_label(
                        c.accent,tr(
                        "Press a shortcut or scroll. Enter modifiers such as Alt or Ctrl directly."),
                    );
                    let captured = ctx.input(|i| {
                        i.events.iter().find_map(|e| match e {
                            Event::Key {
                                key,
                                pressed: true,
                                modifiers,
                                ..
                            } if !matches!(key, Key::Escape | Key::Enter) => {
                                key_to_vk(*key).map(|key| Chord {
                                    key,
                                    mods: mod_bits(*modifiers),
                                })
                            }
                            Event::MouseWheel { modifiers, .. } => Some(Chord {
                                key: crate::ui::keymap::WHEEL,
                                mods: mod_bits(*modifiers),
                            }),
                            _ => None,
                        })
                    });
                    if let Some(chord) = captured {
                        self.binding_draft = chord.text();
                        self.capture_binding = false;
                    }
                } else {
                    ui.weak(tr("Examples: Ctrl+K, Shift+[, Alt, Ctrl+Wheel. Separate bindings with commas."));
                }
                ui.horizontal(|ui| {
                    if ui.button(tr("Save Binding")).clicked()
                        || (response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)))
                    {
                        let draft = self.binding_draft.clone();
                        match self.change_binding(index, &draft) {
                            Ok(()) => {
                                p.message = "Binding Saved".into();
                                p.refresh(&self.settings.keys);
                            }
                            Err(e) => p.message = e,
                        }
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        self.binding_index = None;
                        self.capture_binding = false;
                    }
                });
            }
        });
        if let Some(shown) = shown {
            self.palette_bounds = shown.response.rect;
        }
        if ctx.input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !self.palette_bounds.contains(p))
        }) {
            open = false;
        }
        if let Some(index) = p.current() {
            if clear || reset {
                let text = if reset {
                    Keymap::default().label(index)
                } else {
                    String::new()
                };
                match self.change_binding(index, &text) {
                    Ok(()) => {
                        p.message = "Binding Saved".into();
                        p.refresh(&self.settings.keys);
                    }
                    Err(e) => p.message = e,
                }
            }
        }
        if let Some((index, record)) = edit {
            self.begin_binding(index, record);
        }
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.binding_index.take().is_none() {
                open = false;
            }
            self.capture_binding = false;
        }
        if let Some(index) = execute {
            if let Kind::Press(action) = self.settings.keys.command(index).kind {
                self.actions.push(action);
                open = false;
            }
        }
        if open {
            self.palette = Some(p);
        } else {
            self.binding_index = None;
            self.capture_binding = false;
        }
    }
    fn confirm_discard(&self) -> bool {
        !self.session.doc.dirty || self.native.as_ref().is_some_and(Native::confirm_discard)
    }
    fn save_document(&mut self, save_as: bool) {
        let path = if save_as || self.session.file.is_none() {
            self.native.as_ref().and_then(|n| n.dialog(true, false))
        } else {
            self.session.file.clone()
        };
        if let Some(path) = path {
            if self.core("document.save", serde_json::json!({"path":path})) {
                self.status = "Project Saved".into();
            }
        }
    }
    fn open_document(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        if let Some(path) = self.native.as_ref().and_then(|n| n.dialog(false, false)) {
            if self.core("document.open", serde_json::json!({"path":path})) {
                self.sync_from_session();
                self.status = "Artwork Opened".into();
            }
        }
    }
    fn export(&mut self) {
        if self.session.replay_error.is_some() {
            if let Some(path) = self.native.as_ref().and_then(|n| n.dialog(true, true)) {
                if self.core("replay.recover_png", serde_json::json!({"path":path})) {
                    self.status = "PNG Exported".into();
                }
            }
        } else {
            self.finish_stroke();
            self.push_gui_parameters();
            self.push_gui_view();
            self.core("canvas.export.open", serde_json::json!({}));
        }
    }
}
impl Studio {
    pub(super) fn run_actions(&mut self, ctx: &Context) {
        while !self.editor.actions.is_empty() {
            let action = self.editor.actions.remove(0);
            self.editor.finish_stroke();
            self.editor.pen_active = false;
            let pure_ui = matches!(
                action,
                Action::Preferences
                    | Action::OpenPalette
                    | Action::ShowShortcuts
                    | Action::TogglePanels
                    | Action::ToggleLayers
            ) | matches!(
                action,
                Action::UiCommand(60..=69 | 79 | 10110..=10123 | 10130)
            );
            if pure_ui && !self.from_core_ui {
                if !self.editor.core("ui.dispatch",serde_json::json!({"id":"command","value":serde_json::to_value(action).unwrap()})) { continue; }
                self.editor.session.ui_effects.pop_back();
            }

            match action {
                Action::UiCommand(id) => {
                    if let Some(action) = self
                        .editor
                        .settings
                        .keys
                        .plugin_action(id)
                        .map(str::to_owned)
                    {
                        self.editor.begin_plugin_command(&action);
                        continue;
                    }
                    if let Some(mapped) = COMMANDS
                        .iter()
                        .find(|c| c.ui == id)
                        .and_then(|c| {
                            if let Kind::Press(a) = c.kind {
                                Some(a)
                            } else {
                                None
                            }
                        })
                        .filter(|a| *a != action)
                    {
                        self.editor.actions.push(mapped);
                        continue;
                    }
                    match id {
                        10144 | 10145 => self.editor.open_canvas_geometry(id == 10145),
                        10154..=10158 => {
                            let action = match id {
                                10154 => "toolbox.mode.selection_rectangle",
                                10155 => "toolbox.mode.selection_ellipse",
                                10156 => "toolbox.mode.selection_lasso",
                                10157 => "toolbox.mode.selection_polygon",
                                _ => "toolbox.mode.selection_wand",
                            };
                            self.editor.core(action, serde_json::json!({}));
                        }
                        10150..=10153 => {
                            self.editor.finish_stroke();
                            let action = match id {
                                10150 => "selection.all",
                                10151 => "selection.deselect",
                                10152 => "selection.reselect",
                                _ => "selection.invert",
                            };
                            self.editor.core(action, serde_json::json!({}));
                        }
                        10146..=10149 => {
                            self.editor.finish_stroke();
                            let action = match id {
                                10146 => "layer.merge_down",
                                10147 => "layer.merge_visible",
                                10148 => "layer.flatten",
                                _ => "layer.stamp_visible",
                            };
                            self.editor.core(action, serde_json::json!({}));
                        }
                        10142 | 10143 => {
                            self.editor.finish_stroke();
                            self.editor.push_gui_parameters();
                            let color = if id == 10142 {
                                self.editor.session.parameters.color.clone()
                            } else {
                                self.editor.session.parameters.background_color.clone()
                            };
                            self.editor.core(
                                "layer.fill_color",
                                serde_json::json!({"color":color,"opacity":self.editor.opacity}),
                            );
                        }
                        10175 => {
                            self.editor.core("toolbox.mode.move", serde_json::json!({}));
                        }
                        10185 | 10186 => {
                            self.editor.core(
                                if id == 10185 {
                                    "canvas.transform.repeat"
                                } else {
                                    "canvas.transform.repeat_copy"
                                },
                                serde_json::json!({}),
                            );
                            self.editor.sync_from_session();
                        }
                        10184 => {
                            self.editor
                                .core("canvas.transform.open", serde_json::json!({}));
                        }
                        10176..=10183 => {
                            let i = id - 10176;
                            self.editor.core("canvas.nudge",serde_json::json!({"direction":(["left","right","up","down"][(i%4) as usize]),"large":i>=4}));
                            self.editor.sync_from_session();
                        }
                        10174 => self.editor.open_trim_canvas(),
                        10173 => self.editor.open_image_size(),
                        10166..=10172 => {
                            let actions = [
                                "canvas.rotate_90_cw",
                                "canvas.rotate_90_ccw",
                                "canvas.rotate_180",
                                "canvas.flip_horizontal",
                                "canvas.flip_vertical",
                                "layer.rotate_90_cw",
                                "layer.rotate_90_ccw",
                            ];
                            self.editor
                                .core(actions[(id - 10166) as usize], serde_json::json!({}));
                        }
                        10161..=10165 => {
                            let action = match id {
                                10161 => "clipboard.system_copy",
                                10162 => "clipboard.system_copy_merged",
                                10163 => "clipboard.system_cut",
                                10164 => "clipboard.system_paste",
                                _ => "clipboard.system_paste_in_place",
                            };
                            self.editor.core(action, serde_json::json!({}));
                        }
                        10159 | 10160 => {
                            self.editor.core(
                                if id == 10159 {
                                    "layer.via_copy"
                                } else {
                                    "layer.via_cut"
                                },
                                serde_json::json!({}),
                            );
                        }
                        10140 => {
                            self.editor.core("layer.duplicate", serde_json::json!({}));
                        }
                        10130 => self.focus_panel(Panel::Nodes),
                        10131 => {
                            self.editor.finish_stroke();
                            self.replay.open = true;
                            self.replay.refresh = true;
                        }
                        43 => {
                            if self
                                .editor
                                .native
                                .as_ref()
                                .is_some_and(Native::confirm_clear_layer)
                            {
                                self.editor.core("layer.clear", serde_json::json!({}));
                            }
                        }
                        51 => {
                            self.editor.workspace.dynamics.size =
                                !self.editor.workspace.dynamics.size
                        }
                        52 => {
                            self.editor.workspace.dynamics.opacity =
                                !self.editor.workspace.dynamics.opacity
                        }
                        53 => {
                            self.editor.workspace.dynamics.gamma =
                                if self.editor.workspace.dynamics.gamma < 0.9 {
                                    1.
                                } else if self.editor.workspace.dynamics.gamma < 1.3 {
                                    1.7
                                } else {
                                    0.65
                                }
                        }
                        63..=65 => {
                            self.editor.settings.ui.theme = match id {
                                63 => "light",
                                64 => "warm",
                                _ => "graphite",
                            }
                            .into()
                        }
                        66 | 79 => self.editor.actions.push(Action::Preferences),
                        67 | 10121 => {
                            self.dock_before_focus = None;
                            self.reset_layout(
                                ctx.content_rect().width()
                                    - 48.
                                    - self.editor.settings.ui.outer_padding * 3.,
                            );
                        }
                        68 => self.editor.actions.push(Action::ReloadBindings),
                        60 => self.toggle_panel(Panel::Brush),
                        61 => self.toggle_panel(Panel::Layers),
                        62 => {
                            mirror_horizontal(self.dock.main_surface_mut());
                            if let Some((surface, node, _)) = self.dock.find_tab(&Panel::Canvas) {
                                self.dock.set_focused_node_and_surface((surface, node));
                            }
                        }
                        10110 => self.focus_panel(Panel::Color),
                        10111 => self.focus_panel(Panel::Brush),
                        10120 => self.editor.actions.push(Action::TogglePanels),
                        10122 | 10123 => {
                            let (panel, with) = if id == 10122 {
                                (Panel::Brush, Panel::Color)
                            } else {
                                (Panel::Layers, Panel::Navigator)
                            };
                            if let Some(location) = self.dock.find_tab(&panel) {
                                self.dock.remove_tab(location);
                            }
                            self.focus_panel(with);
                            self.dock.push_to_focused_leaf(panel);
                        }
                        72..=74 => {
                            self.editor.workspace.dynamics.gamma = match id {
                                72 => 0.65,
                                73 => 1.,
                                _ => 1.7,
                            }
                        }
                        75..=78 => {
                            self.editor.workspace.stabilizer = match id {
                                75 => 0.,
                                76 => 0.35,
                                77 => 0.7,
                                _ => 1.,
                            }
                        }
                        820..=825 => self.editor.select_tool(
                            ["pencil", "ink", "paint", "soft", "marker", "erase"]
                                [(id - 820) as usize],
                        ),
                        840..=849 => {
                            self.editor.radius = [1., 2., 3., 5., 8., 12., 20., 35., 60., 100.]
                                [(id - 840) as usize]
                                / 2.
                        }
                        19999 => {
                            if self.editor.confirm_discard() {
                                self.allow_close = true;
                                self.save_layout();
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        }
                        _ => self.editor.status = "Adjust This in Settings or Panel Tabs".into(),
                    }
                }
                Action::Brush => self.editor.select_tool("brush"),
                Action::Eraser => self.editor.select_tool("eraser"),
                Action::Picker => self.editor.select_tool("picker"),
                Action::Hand => self.editor.select_tool("hand"),
                Action::Zoom => self.editor.select_tool("zoom"),
                Action::Undo => {
                    self.editor.core("history.undo", serde_json::json!({}));
                }
                Action::Redo => {
                    self.editor.core("history.redo", serde_json::json!({}));
                }
                Action::New => {
                    if self.editor.confirm_discard() {
                        self.editor.open_new_canvas();
                    }
                }
                Action::Open => self.editor.open_document(),
                Action::Save => self.editor.save_document(false),
                Action::SaveAs => self.editor.save_document(true),
                Action::Export => self.editor.export(),
                Action::NewLayer => {
                    self.editor.core("layer.add", serde_json::json!({}));
                }
                Action::Fit => self.editor.fit = true,
                Action::ActualPixels => {
                    self.editor.fit = false;
                    self.editor.zoom = 1. / ctx.pixels_per_point();
                    self.editor.pan = Vec2::ZERO;
                }
                Action::ZoomIn => self.editor.zoom_at(self.editor.canvas.center(), 1.25),
                Action::ZoomOut => self.editor.zoom_at(self.editor.canvas.center(), 1. / 1.25),
                Action::Preferences => {
                    self.open_settings(None);
                }
                Action::OpenPalette => self.editor.open_palette(Mode::Commands),
                Action::ShowShortcuts => self.open_settings(Some(preferences::Category::Keys)),
                Action::TogglePanels => {
                    if let Some(before) = self.dock_before_focus.take() {
                        self.dock = before;
                    } else {
                        self.dock_before_focus = Some(std::mem::replace(
                            &mut self.dock,
                            DockState::new(vec![Panel::Canvas]),
                        ));
                    }
                }
                Action::ToggleLayers => self.toggle_panel(Panel::Layers),
                Action::SmallerBrush => {
                    self.editor.radius = brush_diameter(self.editor.radius * 2., false) / 2.
                }
                Action::LargerBrush => {
                    self.editor.radius = brush_diameter(self.editor.radius * 2., true) / 2.
                }
                Action::SofterBrush => self.editor.hardness = (self.editor.hardness - 0.25).max(0.),
                Action::HarderBrush => self.editor.hardness = (self.editor.hardness + 0.25).min(1.),
                Action::OpacityDigit(digit) => {
                    self.editor.opacity = self
                        .editor
                        .opacity_input
                        .digit(digit, ctx.input(|i| (i.time * 1000.) as u32))
                }
                Action::DefaultColors => {
                    self.editor.ink = [0; 3];
                    self.editor.background_ink = [255; 3];
                }
                Action::SwapColors => {
                    std::mem::swap(&mut self.editor.ink, &mut self.editor.background_ink)
                }
                Action::ReloadBindings => match Settings::load(self.editor.settings.path.clone()) {
                    Ok(s) => {
                        self.editor.settings = s;
                        self.editor.register_commands();
                        self.editor.status = "UI and Shortcuts Reloaded".into();
                    }
                    Err(e) => self.editor.status = e,
                },
                Action::ResetBindings => {
                    let before =
                        std::mem::replace(&mut self.editor.settings.keys, Keymap::default());
                    self.editor.register_commands();
                    if let Err(e) = self.editor.save_settings() {
                        self.editor.settings.keys = before;
                        self.editor.status = e;
                    }
                }
                Action::Cancel => {
                    self.editor.palette = None;
                    self.editor.command_form = None;
                    self.editor.new_open = false;
                    self.editor.export_draft = None;
                    self.editor.geometry = None;
                    self.editor.transform = None;
                    self.editor.finish_stroke();
                }
            }
        }
    }
    fn toggle_panel(&mut self, panel: Panel) {
        if let Some(location) = self.dock.find_tab(&panel) {
            self.dock.remove_tab(location);
        } else {
            self.focus_panel(panel);
        }
    }
}

fn mirror_horizontal(tree: &mut egui_dock::Tree<Panel>) {
    use egui_dock::Node;
    let source: Vec<_> = tree.iter().cloned().collect();
    let mut target = vec![Node::Empty; source.len()];
    fn copy(source: &[Node<Panel>], target: &mut [Node<Panel>], from: usize, to: usize) {
        if from >= source.len() || to >= target.len() {
            return;
        }
        target[to] = source[from].clone();
        match &source[from] {
            Node::Horizontal(_) => {
                if let Node::Horizontal(split) = &mut target[to] {
                    split.fraction = 1. - split.fraction;
                }
                copy(source, target, from * 2 + 2, to * 2 + 1);
                copy(source, target, from * 2 + 1, to * 2 + 2);
            }
            Node::Vertical(_) => {
                copy(source, target, from * 2 + 1, to * 2 + 1);
                copy(source, target, from * 2 + 2, to * 2 + 2);
            }
            _ => {}
        }
    }
    copy(&source, &mut target, 0, 0);
    for (node, mirrored) in tree.iter_mut().zip(target) {
        *node = mirrored;
    }
}
