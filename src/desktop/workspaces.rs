//! Bounded named dock snapshots. Documents, settings and drafts stay with their owners.
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;

const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_LAYOUTS: usize = 16;

#[derive(Clone, Default)]
pub(super) struct Layouts {
    active: Option<String>,
    saved: BTreeMap<String, Value>,
    // Unsupported or corrupt files must survive startup and later autosaves.
    blocked: Option<String>,
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    current: Value,
    active: Option<String>,
    layouts: BTreeMap<String, Value>,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.trim() == name
        && name.chars().count() <= 64
        && !name.chars().any(char::is_control)
}

fn validate_snapshot(value: &Value) -> Result<(), String> {
    let text = value.to_string();
    if decode_workspace(&text).is_none() {
        return Err("Invalid workspace layout snapshot".into());
    }
    if let Some(known) = value.get("known_panels") {
        let ids: Vec<String> = serde_json::from_value(known.clone()).map_err(|e| e.to_string())?;
        if ids.len() > 128 || ids.iter().any(|id| id.len() > 256) {
            return Err("Workspace panel inventory exceeds its limit".into());
        }
    }
    Ok(())
}

pub(super) fn read(path: &std::path::Path) -> Result<Option<String>, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let mut text = String::new();
    file.take((MAX_FILE_BYTES + 1) as u64)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > MAX_FILE_BYTES {
        return Err("Workspace file exceeds 8 MiB".into());
    }
    Ok(Some(text))
}

impl Layouts {
    pub(super) fn initialize_default(&mut self, initial: Self) {
        self.active = initial.active;
        self.saved = initial.saved;
    }
    pub(super) fn blocked(error: String) -> Self {
        Self {
            blocked: Some(error),
            ..Self::default()
        }
    }

    pub(super) fn decode(text: &str) -> Result<(Self, String), String> {
        if text.len() > MAX_FILE_BYTES {
            return Err("Workspace file exceeds 8 MiB".into());
        }
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if value.get("version") == Some(&json!(2)) {
            let file: File = serde_json::from_value(value).map_err(|e| e.to_string())?;
            if file.layouts.len() > MAX_LAYOUTS
                || file.layouts.keys().any(|name| !valid_name(name))
                || file
                    .active
                    .as_ref()
                    .is_some_and(|name| !file.layouts.contains_key(name))
            {
                return Err("Invalid workspace layout inventory".into());
            }
            validate_snapshot(&file.current)?;
            for snapshot in file.layouts.values() {
                validate_snapshot(snapshot)?;
            }
            Ok((
                Self {
                    active: file.active,
                    saved: file.layouts,
                    ..Self::default()
                },
                file.current.to_string(),
            ))
        } else {
            validate_snapshot(&value)?;
            let mut saved = BTreeMap::new();
            saved.insert("Default".into(), value);
            Ok((
                Self {
                    active: Some("Default".into()),
                    saved,
                    ..Self::default()
                },
                text.into(),
            ))
        }
    }

    fn encode(&self, current: Value) -> Result<String, String> {
        if let Some(error) = &self.blocked {
            return Err(format!("Workspace file preserved: {error}"));
        }
        validate_snapshot(&current)?;
        let text = serde_json::to_string_pretty(&File {
            version: 2,
            current,
            active: self.active.clone(),
            layouts: self.saved.clone(),
        })
        .map_err(|e| e.to_string())?;
        if text.len() > MAX_FILE_BYTES {
            return Err("Workspace file exceeds 8 MiB".into());
        }
        Ok(text)
    }

    fn persist(&self, current: Value, path: &std::path::Path) -> Result<(), String> {
        settings::atomic_write(path, &self.encode(current)?)
    }
}

impl Studio {
    fn current_workspace_snapshot(&self) -> Result<Value, String> {
        let dock = self.dock_before_focus.as_ref().unwrap_or(&self.dock);
        serde_json::from_str(&encode_workspace(dock, &self.editor.session.registry)?)
            .map_err(|e| e.to_string())
    }

    pub(super) fn report_workspaces(&mut self) {
        self.editor.session.ui["workspaces.actual"] = json!({
            "active":self.layouts.active,"names":self.layouts.saved.keys().collect::<Vec<_>>(),
            "writable":self.layouts.blocked.is_none(),"error":self.layouts.blocked,
            "limit":MAX_LAYOUTS
        });
    }

    pub(super) fn persist_workspaces(&self) -> Result<(), String> {
        self.layouts
            .persist(self.current_workspace_snapshot()?, &self.layout_path)
    }

    fn switch_workspace_idle(&self) -> Result<(), String> {
        if self.editor.backend_contact_active()
            || self.editor.transform_contact_active()
            || self.editor.liquify_contact_active()
            || self.editor.wheel_drag.is_some()
            || self.editor.pen_active
            || self.editor.drawing
            || self.editor.input_owned
            || self.editor.panning.is_some()
            || self.editor.movement_gesture.is_some()
            || self.editor.selection_gesture.is_some()
            || self.editor.area_drag.is_some()
            || self.pending_group_drag.is_some()
            || self
                .native_windows
                .values()
                .any(|w| w.native.as_ref().is_some_and(Native::is_sizing))
        {
            return Err("Finish the current contact before switching workspace layouts".into());
        }
        Ok(())
    }

    pub(super) fn apply_named_workspace(
        &mut self,
        operation: &str,
        args: &Value,
    ) -> Result<(), String> {
        if self.preview.is_some() {
            return Err("Named workspace changes are unavailable during preview capture".into());
        }
        let name = args["name"].as_str().ok_or("Missing workspace name")?;
        if !valid_name(name) {
            return Err("Invalid workspace name".into());
        }
        let mut next = self.layouts.clone();
        let mut current = self.current_workspace_snapshot()?;
        let mut switched = None;
        match operation {
            "save" => {
                if next.saved.contains_key(name) && args["replace"] != true {
                    return Err(
                        "Workspace name already exists; explicitly replace it to save again".into(),
                    );
                }
                if !next.saved.contains_key(name) && next.saved.len() >= MAX_LAYOUTS {
                    return Err("At most 16 named workspace layouts are supported".into());
                }
                next.saved.insert(name.into(), current.clone());
                next.active = Some(name.into());
            }
            "switch" => {
                self.switch_workspace_idle()?;
                let snapshot = next
                    .saved
                    .get(name)
                    .ok_or("Saved workspace layout not found")?;
                let (mut dock, known) = decode_workspace(&snapshot.to_string())
                    .ok_or("Invalid saved workspace layout")?;
                reconcile_plugins(&mut dock, &self.editor.session.registry, Some(&known));
                current =
                    serde_json::from_str(&encode_workspace(&dock, &self.editor.session.registry)?)
                        .map_err(|e| e.to_string())?;
                next.active = Some(name.into());
                switched = Some(dock);
            }
            "delete" => {
                if next.saved.remove(name).is_none() {
                    return Err("Saved workspace layout not found".into());
                }
                if next.active.as_deref() == Some(name) {
                    next.active = None;
                }
            }
            _ => return Err("Unknown workspace operation".into()),
        }
        let snapshot = encode_dock(
            switched
                .as_ref()
                .unwrap_or_else(|| self.dock_before_focus.as_ref().unwrap_or(&self.dock)),
        )?;
        // Commit the candidate file before publishing either layout or inventory.
        next.persist(current, &self.layout_path)?;
        self.layouts = next;
        self.layout_snapshot = snapshot;
        if let Some(dock) = switched {
            self.dock = dock;
            self.dock_before_focus = None;
            self.report_windows();
        }
        self.layout_changed = None;
        self.report_workspaces();
        Ok(())
    }

    pub(super) fn workspace_menu(&mut self, ui: &mut Ui) {
        let a = self.editor.settings.ui.clone();
        ui.separator();
        ui.label(tr("Saved Workspace Layouts"));
        let names: Vec<_> = self.layouts.saved.keys().cloned().collect();
        ScrollArea::vertical()
            .id_salt("named-workspaces")
            .max_height(a.control_height * 6.)
            .show(ui, |ui| {
                for name in names {
                    if ui
                        .add(
                            Button::new(&name)
                                .selected(self.layouts.active.as_ref() == Some(&name))
                                .truncate(),
                        )
                        .on_hover_text(&name)
                        .clicked()
                    {
                        self.editor
                            .core("workspace.layout.switch", json!({"name":name}));
                        ui.close();
                    }
                }
            });
        ui.add(
            TextEdit::singleline(&mut self.layouts.name)
                .hint_text(tr("Layout Name"))
                .char_limit(64)
                .desired_width(a.control_height * 9.),
        );
        if ui
            .add_enabled(
                valid_name(&self.layouts.name),
                Button::new(tr("Save New Layout")),
            )
            .clicked()
        {
            self.editor
                .core("workspace.layout.save", json!({"name":self.layouts.name}));
            ui.close();
        }
        if let Some(active) = self.layouts.active.clone() {
            if ui.button(tr("Update Active Layout")).clicked() {
                self.editor.core(
                    "workspace.layout.save",
                    json!({"name":active,"replace":true}),
                );
                ui.close();
            }
            if ui.button(tr("Delete Saved Layout")).clicked() {
                self.editor
                    .core("workspace.layout.delete", json!({"name":active}));
                ui.close();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Action as CoreAction;

    fn app() -> Studio {
        let mut app = super::super::tests::studio();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output/workspace-layouts")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        app.layout_path = root.join("workspace.egui.json");
        app
    }

    fn command(app: &mut Studio, id: &str, args: Value) {
        let revision = app.editor.session.revision;
        assert!(app.editor.core(id, args), "{}", app.editor.status);
        assert_eq!(app.editor.session.revision, revision);
        app.consume_ui_effects(&Context::default());
        assert_eq!(
            app.editor.session.ui["workspaces.result"]["status"],
            "applied"
        );
    }

    #[test]
    fn workspace_layout_save_switch_restart_preserves_hidden_floating_tabs_and_document() {
        let mut app = app();
        app.dock
            .remove_tab(app.dock.find_tab(&Panel::Brush).unwrap());
        app.apply_window_command(&Context::default(), &json!({"panel":"canvas","operation":"float","x":-800,"y":120,"width":700,"height":500})).unwrap();
        let original = encode_dock(&app.dock).unwrap();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let composition = app.editor.session.doc.composition();
        let history = app.editor.session.doc.history_bytes();
        let identity = app.editor.session.doc.identity;
        let dirty = app.editor.session.doc.dirty;
        let settings = app.editor.settings.encode().unwrap();
        command(
            &mut app,
            "workspace.layout.save",
            json!({"name":"绘画 / Рисование"}),
        );
        app.dock = DockState::new(vec![Panel::Canvas, Panel::Layers]);
        let compact = encode_dock(&app.dock).unwrap();
        command(&mut app, "workspace.layout.save", json!({"name":"Compact"}));
        command(
            &mut app,
            "workspace.layout.switch",
            json!({"name":"绘画 / Рисование"}),
        );
        assert_eq!(encode_dock(&app.dock).unwrap(), original);
        assert!(app.dock.find_tab(&Panel::Brush).is_none());
        assert!(!app.dock.find_tab(&Panel::Canvas).unwrap().0.is_main());
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(app.editor.session.doc.composition(), composition);
        assert_eq!(app.editor.session.doc.history_bytes(), history);
        assert_eq!(app.editor.session.doc.identity, identity);
        assert_eq!(app.editor.session.doc.dirty, dirty);
        assert_eq!(app.editor.settings.encode().unwrap(), settings);
        let text = read(&app.layout_path).unwrap().unwrap();
        let (restored, current) = Layouts::decode(&text).unwrap();
        assert_eq!(restored.active.as_deref(), Some("绘画 / Рисование"));
        assert_eq!(
            encode_dock(&decode_workspace(&current).unwrap().0).unwrap(),
            original
        );
        assert_eq!(
            encode_dock(
                &decode_workspace(&restored.saved["Compact"].to_string())
                    .unwrap()
                    .0
            )
            .unwrap(),
            compact
        );
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["version"], 2);
    }

    #[test]
    fn workspace_layout_legacy_migration_and_autosave_keep_named_snapshots() {
        let mut app = app();
        let legacy = encode_workspace(&app.dock, &app.editor.session.registry).unwrap();
        let (layouts, current) = Layouts::decode(&legacy).unwrap();
        assert_eq!(current, legacy);
        let saved = layouts.saved.clone();
        app.layouts = layouts;
        app.dock = DockState::new(vec![Panel::Canvas]);
        app.save_layout();
        let (restored, current) =
            Layouts::decode(&read(&app.layout_path).unwrap().unwrap()).unwrap();
        assert_eq!(restored.saved, saved);
        assert_eq!(
            decode_workspace(&current)
                .unwrap()
                .0
                .iter_all_tabs()
                .count(),
            1
        );
        let bare = serde_json::to_string(&serde_json::from_str::<Value>(&legacy).unwrap()["dock"])
            .unwrap();
        assert!(Layouts::decode(&bare).is_ok());
    }

    #[test]
    fn workspace_layout_failures_preserve_file_inventory_and_current_dock() {
        let mut app = app();
        command(&mut app, "workspace.layout.save", json!({"name":"One"}));
        let before = std::fs::read(&app.layout_path).unwrap();
        let dock = encode_dock(&app.dock).unwrap();
        for (operation, args) in [
            ("save", json!({"name":"One"})),
            ("switch", json!({"name":"Missing"})),
            ("delete", json!({"name":"Missing"})),
            ("save", json!({"name":" invalid "})),
        ] {
            assert!(app.apply_named_workspace(operation, &args).is_err());
            assert_eq!(std::fs::read(&app.layout_path).unwrap(), before);
            assert_eq!(app.layouts.active.as_deref(), Some("One"));
            assert_eq!(encode_dock(&app.dock).unwrap(), dock);
        }
        // An unwritable destination fails before publishing the candidate switch.
        app.dock = DockState::new(vec![Panel::Canvas]);
        let dock = encode_dock(&app.dock).unwrap();
        app.layout_path = app.layout_path.join("missing/child.json");
        assert!(app
            .apply_named_workspace("switch", &json!({"name":"One"}))
            .is_err());
        assert_eq!(encode_dock(&app.dock).unwrap(), dock);
        assert_eq!(app.layouts.saved.len(), 1);
    }

    #[test]
    fn workspace_layout_rejects_corrupt_future_and_oversized_files_without_overwrite() {
        let mut app = app();
        let current = app.current_workspace_snapshot().unwrap();
        for value in [
            json!({"version":3,"dock":{}}),
            json!({"version":2,"active":"missing","current":current,"layouts":{}}),
            json!({"version":2,"active":null,"current":current,"layouts":{"bad":{}}}),
        ] {
            assert!(Layouts::decode(&value.to_string()).is_err());
        }
        assert!(Layouts::decode(&" ".repeat(MAX_FILE_BYTES + 1)).is_err());
        std::fs::write(&app.layout_path, "future workspace data").unwrap();
        app.layouts = Layouts::blocked("Unsupported workspace version".into());
        app.save_layout();
        assert_eq!(
            std::fs::read_to_string(&app.layout_path).unwrap(),
            "future workspace data"
        );
        assert!(app
            .apply_named_workspace("save", &json!({"name":"One"}))
            .is_err());
    }

    #[test]
    fn workspace_layout_explicit_update_delete_and_capacity_are_bounded() {
        let mut app = app();
        for index in 0..MAX_LAYOUTS {
            app.apply_named_workspace("save", &json!({"name":format!("Layout {index}")}))
                .unwrap();
        }
        assert!(app
            .apply_named_workspace("save", &json!({"name":"Overflow"}))
            .is_err());
        app.dock = DockState::new(vec![Panel::Canvas]);
        app.apply_named_workspace("save", &json!({"name":"Layout 0","replace":true}))
            .unwrap();
        app.apply_named_workspace("delete", &json!({"name":"Layout 0"}))
            .unwrap();
        assert!(app.layouts.active.is_none());
        assert_eq!(app.dock.iter_all_tabs().count(), 1);
        assert_eq!(app.layouts.saved.len(), MAX_LAYOUTS - 1);
    }

    #[test]
    fn workspace_layout_switch_rechecks_contact_and_preserves_preview_drafts() {
        let mut app = app();
        command(&mut app, "workspace.layout.save", json!({"name":"One"}));
        app.editor.open_transform();
        assert!(app.editor.transform.is_some());
        app.editor.transform.as_mut().unwrap().params.translate_x = 7.;
        command(&mut app, "workspace.layout.switch", json!({"name":"One"}));
        assert_eq!(
            app.editor.transform.as_ref().unwrap().params.translate_x,
            7.
        );
        // A contact may start after transport admission but before presentation.
        assert!(app
            .editor
            .core("workspace.layout.switch", json!({"name":"One"})));
        app.editor.pen_active = true;
        let file = std::fs::read(&app.layout_path).unwrap();
        app.consume_ui_effects(&Context::default());
        assert_eq!(
            app.editor.session.ui["workspaces.result"]["status"],
            "failed"
        );
        assert!(app.editor.pen_active);
        assert_eq!(std::fs::read(&app.layout_path).unwrap(), file);
        assert!(app
            .editor
            .session
            .apply(CoreAction::new("workspace.layout.list", json!({})))
            .unwrap()["result"]["available"]
            .as_bool()
            .unwrap());
    }

    #[test]
    fn workspace_layout_plugin_reconciliation_keeps_the_original_named_snapshot() {
        let mut app = app();
        command(&mut app, "workspace.layout.save", json!({"name":"One"}));
        let saved = app.layouts.saved["One"].clone();
        let brush = app.editor.session.registry.panels.remove("brush").unwrap();
        command(&mut app, "workspace.layout.switch", json!({"name":"One"}));
        assert!(app.dock.find_tab(&Panel::Brush).is_none());
        assert_eq!(app.layouts.saved["One"], saved);
        app.editor
            .session
            .registry
            .panels
            .insert("brush".into(), brush);
        command(&mut app, "workspace.layout.switch", json!({"name":"One"}));
        assert!(app.dock.find_tab(&Panel::Brush).is_some());
        assert_eq!(app.layouts.saved["One"], saved);
    }

    #[test]
    fn workspace_layout_menu_uses_localized_controls_and_real_save_clicks() {
        fn locate(shape: &Shape, text: &str) -> Option<Pos2> {
            match shape {
                Shape::Text(s) if s.galley.text() == text => Some(s.pos + s.galley.size() * 0.5),
                Shape::Vec(shapes) => shapes.iter().find_map(|s| locate(s, text)),
                _ => None,
            }
        }
        fn render(app: &mut Studio, ctx: &Context, width: f32, events: Vec<Event>) -> FullOutput {
            ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 700.))),
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| app.workspace_menu(ui));
                },
            )
        }
        for language in crate::i18n::Language::ALL {
            for width in [280., 600.] {
                let mut app = app();
                app.layouts.name = "绘画 / Рисование".into();
                crate::i18n::set_language(language);
                let ctx = Context::default();
                theme::fonts(&ctx);
                theme::apply(&ctx, &app.editor.settings.ui);
                let output = render(&mut app, &ctx, width, vec![]);
                let label = crate::i18n::text_in(language, "Save New Layout");
                let position = output
                    .shapes
                    .iter()
                    .find_map(|s| locate(&s.shape, &label))
                    .expect("localized Save control");
                assert!(output.shapes.iter().all(|s| s.clip_rect.is_finite()));
                render(
                    &mut app,
                    &ctx,
                    width,
                    vec![
                        Event::PointerMoved(position),
                        Event::PointerButton {
                            pos: position,
                            button: PointerButton::Primary,
                            pressed: true,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                render(
                    &mut app,
                    &ctx,
                    width,
                    vec![Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
                assert_eq!(
                    app.editor.session.ui_effects.back().unwrap()["id"],
                    "workspace.named"
                );
                app.consume_ui_effects(&ctx);
                assert_eq!(app.layouts.active.as_deref(), Some("绘画 / Рисование"));
                assert_eq!(
                    app.editor.session.ui["workspaces.result"]["status"],
                    "applied"
                );
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
