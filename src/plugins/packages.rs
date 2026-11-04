//! Package inventory and dependency plans; no loading or document mutation in the planner.
use super::{valid_id, Manifest, Registry};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Loaded,
    Disabled,
    Blocked,
    Failed,
}
impl PluginStatus {
    pub fn title(self) -> &'static str {
        match self {
            Self::Loaded => "Loaded",
            Self::Disabled => "Disabled",
            Self::Blocked => "Blocked",
            Self::Failed => "Failed",
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginInfo {
    pub id: Option<String>,
    pub name: String,
    pub version: Option<String>,
    pub description: String,
    pub author: Option<String>,
    pub min_host_version: Option<String>,
    pub locales: BTreeMap<String, String>,
    pub manifest: PathBuf,
    pub manifest_enabled: bool,
    pub status: PluginStatus,
    pub disabled_reason: Option<String>,
    pub requires: Vec<String>,
    pub blocked_by: Vec<String>,
    pub capabilities: Vec<String>,
    pub contributions: BTreeMap<String, usize>,
    pub error: Option<String>,
}
impl PluginInfo {
    pub(super) fn discovered(path: &Path, m: Option<&Manifest>) -> Self {
        let mut info = Self {
            id: m.map(|m| m.id.clone()),
            name: m.map(|m| m.name.clone()).unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into()
            }),
            version: m.and_then(|m| m.version.clone()),
            description: m.map(|m| m.description.clone()).unwrap_or_default(),
            author: m.and_then(|m| m.author.clone()),
            min_host_version: m.and_then(|m| m.min_host_version.clone()),
            locales: m.map(|m| m.locales.clone()).unwrap_or_default(),
            manifest: path.into(),
            manifest_enabled: m.is_some_and(|m| m.enabled),
            status: PluginStatus::Failed,
            disabled_reason: None,
            requires: m.map(|m| m.requires.clone()).unwrap_or_default(),
            blocked_by: vec![],
            capabilities: vec![],
            contributions: BTreeMap::new(),
            error: None,
        };
        if let Some(m) = m {
            info.count(m);
        }
        info
    }
    pub(super) fn resolve(&mut self, r: &Registry) {
        self.error = r
            .diagnostics
            .iter()
            .find(|d| d.path == self.manifest)
            .map(|d| d.error.clone());
        self.blocked_by = self
            .requires
            .iter()
            .filter(|id| !r.loaded.contains(id))
            .cloned()
            .collect();
        self.status = if self
            .error
            .as_deref()
            .is_some_and(|e| e.starts_with("Missing or cyclic dependencies:"))
        {
            PluginStatus::Blocked
        } else if self.error.is_some() {
            PluginStatus::Failed
        } else if !self.manifest_enabled
            || self.id.as_ref().is_some_and(|id| r.disabled.contains(id))
        {
            self.disabled_reason = Some(
                if !self.manifest_enabled {
                    "manifest"
                } else {
                    "preferences"
                }
                .into(),
            );
            PluginStatus::Disabled
        } else if self.id.as_ref().is_some_and(|id| r.loaded.contains(id)) {
            PluginStatus::Loaded
        } else {
            PluginStatus::Failed
        };
        if self.status == PluginStatus::Loaded {
            if let Some(m) = self
                .id
                .as_ref()
                .and_then(|id| r.available.iter().find(|m| &m.id == id))
            {
                self.count(m);
            }
        }
    }
    fn count(&mut self, m: &Manifest) {
        self.contributions = [
            ("tools", m.tools.len()),
            ("actions", m.actions.len()),
            ("states", m.states.len()),
            ("nodes", m.nodes.len()),
            ("panels", m.panels.len()),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect();
        self.capabilities = self
            .contributions
            .iter()
            .filter(|(_, v)| **v > 0)
            .map(|(k, _)| k.clone())
            .collect();
        if !m.locales.is_empty() {
            self.capabilities.push("locales".into());
        }
        if m.library.is_some() {
            self.capabilities.push("native_library".into());
        }
        if m.panels.iter().any(|p| p.builtin.is_some()) {
            self.capabilities.push("compiled_ui".into());
        }
    }
}
fn version(value: &str) -> Result<[u32; 3], String> {
    let parts: Vec<_> = value.split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || (p.len() > 1 && p.starts_with('0'))
                || !p.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return Err("Version must use major.minor.patch with numeric components".into());
    }
    Ok([
        parts[0]
            .parse()
            .map_err(|_| "Version component exceeds u32")?,
        parts[1]
            .parse()
            .map_err(|_| "Version component exceeds u32")?,
        parts[2]
            .parse()
            .map_err(|_| "Version component exceeds u32")?,
    ])
}
pub(super) fn validate_metadata(m: &Manifest) -> Result<(), String> {
    if m.api_version != 1 || !valid_id(&m.id) {
        return Err("Invalid plugin ID or API version".into());
    }
    if m.name.trim().is_empty()
        || m.name.len() > 512
        || m.description.len() > 4096
        || m.author.as_ref().is_some_and(|v| v.len() > 512)
    {
        return Err("Invalid plugin display metadata".into());
    }
    if let Some(v) = &m.version {
        version(v)?;
    }
    if let Some(v) = &m.min_host_version {
        let host = version(env!("CARGO_PKG_VERSION").split(['-', '+']).next().unwrap())?;
        if version(v)? > host {
            return Err(format!("Plugin requires MiCoPi {v} or newer"));
        }
    }
    for (language, file) in &m.locales {
        let path = Path::new(file);
        if !["zh-CN", "ja", "ko"].contains(&language.as_str())
            || file.len() > 256
            || file.is_empty()
            || file.contains([':', '\\', '\0'])
            || !path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            || path.extension().is_none_or(|e| e != "json")
        {
            return Err(
                "Plugin locale paths must be relative JSON files for zh-CN, ja or ko".into(),
            );
        }
    }
    let mut seen = BTreeSet::new();
    if m.requires.len() > 128
        || m.requires
            .iter()
            .any(|id| !valid_id(id) || !seen.insert(id) || id == &m.id)
    {
        return Err("Invalid, duplicate or self-referencing plugin dependency".into());
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct PluginChange {
    pub id: String,
    pub enabled: bool,
}
#[derive(Debug, Serialize)]
pub struct PluginPlan {
    pub disabled: Vec<String>,
    pub changes: Vec<PluginChange>,
    pub restart_required: bool,
}
impl Registry {
    pub fn contribution_owners(&self) -> BTreeMap<&str, BTreeMap<&str, &str>> {
        [
            ("actions", self.actions.keys().collect::<Vec<_>>()),
            ("tools", self.tools.keys().collect()),
            ("states", self.states.keys().collect()),
            ("nodes", self.nodes.keys().collect()),
            ("panels", self.panels.keys().collect()),
        ]
        .into_iter()
        .map(|(kind, ids)| {
            (
                kind,
                ids.into_iter()
                    .filter_map(|id| {
                        self.contribution_owner(kind, id)
                            .map(|owner| (id.as_str(), owner))
                    })
                    .collect(),
            )
        })
        .collect()
    }
    pub fn locale_sources(&self) -> Vec<crate::i18n::PluginCatalog> {
        self.packages
            .iter()
            .filter(|p| p.status == PluginStatus::Loaded)
            .flat_map(|p| {
                self.available
                    .iter()
                    .find(|m| Some(&m.id) == p.id.as_ref())
                    .into_iter()
                    .flat_map(move |m| {
                        m.locales.iter().filter_map(move |(code, file)| {
                            let language = crate::i18n::Language::ALL
                                .into_iter()
                                .find(|l| l.code() == code)?;
                            let root = p.manifest.parent()?.to_path_buf();
                            Some(crate::i18n::PluginCatalog {
                                owner: m.id.clone(),
                                language,
                                path: root.join(file),
                                root,
                            })
                        })
                    })
            })
            .collect()
    }
    pub fn contribution_owner(&self, kind: &str, id: &str) -> Option<&str> {
        match kind {
            "actions" => self.owners.get(id).map(String::as_str),
            "nodes" => self.nodes.get(id).map(|(id, _)| id.as_str()),
            _ => self
                .available
                .iter()
                .filter(|m| self.loaded.contains(&m.id))
                .find(|m| match kind {
                    "tools" => m.tools.iter().any(|v| v.id == id),
                    "panels" => m.panels.iter().any(|v| v.id == id),
                    "states" => m.states.iter().any(|v| v.id == id),
                    _ => false,
                })
                .map(|m| m.id.as_str()),
        }
    }
    pub fn plan_enabled(
        &self,
        disabled: &[String],
        id: &str,
        enabled: bool,
    ) -> Result<PluginPlan, String> {
        if !valid_id(id) {
            return Err("Invalid plugin ID".into());
        }
        let mut packages = BTreeMap::new();
        let mut duplicates = BTreeSet::new();
        for m in &self.available {
            if packages.insert(m.id.as_str(), m).is_some() {
                duplicates.insert(m.id.as_str());
            }
        }
        let find = |id: &str| -> Result<&Manifest, String> {
            if duplicates.contains(id) {
                return Err(format!("Ambiguous duplicate plugin ID: {id}"));
            }
            packages
                .get(id)
                .copied()
                .ok_or_else(|| format!("Plugin not found: {id}"))
        };
        find(id)?;
        if disabled.iter().any(|id| !valid_id(id)) {
            return Err("Invalid disabled plugin ID".into());
        }
        let before: BTreeSet<String> = disabled.iter().cloned().collect();
        let mut after = before.clone();
        if enabled {
            fn visit<'a>(
                id: &str,
                find: &impl Fn(&str) -> Result<&'a Manifest, String>,
                visiting: &mut BTreeSet<String>,
                done: &mut BTreeSet<String>,
                after: &mut BTreeSet<String>,
            ) -> Result<(), String> {
                if done.contains(id) {
                    return Ok(());
                }
                if !visiting.insert(id.into()) {
                    return Err(format!("Cyclic plugin dependency: {id}"));
                }
                let m = find(id)?;
                validate_metadata(m)?;
                if !m.enabled {
                    return Err(format!("Plugin {id} has enabled=false in its manifest"));
                }
                for dep in &m.requires {
                    visit(dep, find, visiting, done, after)?;
                }
                visiting.remove(id);
                done.insert(id.into());
                after.remove(id);
                Ok(())
            }
            visit(
                id,
                &find,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
                &mut after,
            )?;
        } else {
            let mut affected = BTreeSet::from([id.to_owned()]);
            loop {
                let mut changed = false;
                for m in self
                    .available
                    .iter()
                    .filter(|m| m.enabled && !before.contains(&m.id))
                {
                    if m.requires.iter().any(|id| affected.contains(id)) {
                        changed |= affected.insert(m.id.clone());
                    }
                }
                if !changed {
                    break;
                }
            }
            if affected
                .iter()
                .any(|id| ["core", "painting", "workbench"].contains(&id.as_str()))
            {
                return Err("Built-in core plugins cannot be disabled".into());
            }
            after.extend(affected);
        }
        let changes = before
            .symmetric_difference(&after)
            .map(|id| PluginChange {
                id: id.clone(),
                enabled: !after.contains(id),
            })
            .collect::<Vec<_>>();
        Ok(PluginPlan {
            disabled: after.into_iter().collect(),
            restart_required: !changes.is_empty(),
            changes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{Action, Session};
    use serde_json::json;
    fn fixture() -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output/plugin-packages")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn write(root: &Path, id: &str, patch: serde_json::Value) {
        let mut m = json!({"api_version":1,"id":id,"name":id});
        for (key, value) in patch.as_object().unwrap() {
            m[key] = value.clone();
        }
        std::fs::write(root.join(format!("{id}.json")), m.to_string()).unwrap();
    }
    #[test]
    fn inventory_classifies_disabled_blocked_invalid_and_loaded_packages_by_path() {
        let root = fixture();
        write(
            &root,
            "base",
            json!({"version":"1.2.3","description":"Example","author":"Artist","min_host_version":"0.21.0"}),
        );
        write(
            &root,
            "off",
            json!({"enabled":false,"library":"not-opened.dll"}),
        );
        write(&root, "preferences", json!({"library":"not-opened.dll"}));
        write(&root, "child", json!({"requires":["off"]}));
        write(&root, "cycle_a", json!({"requires":["cycle_b"]}));
        write(&root, "cycle_b", json!({"requires":["cycle_a"]}));
        write(
            &root,
            "future",
            json!({"min_host_version":"99.0.0","library":"not-opened.dll"}),
        );
        std::fs::write(root.join("broken.json"), "{").unwrap();
        let r = Registry::load_with_disabled(&root, &["preferences".into()]).unwrap();
        let info = |id: &str| {
            r.packages
                .iter()
                .find(|p| p.id.as_deref() == Some(id))
                .unwrap()
        };
        assert_eq!(info("base").status, PluginStatus::Loaded);
        assert_eq!(info("base").version.as_deref(), Some("1.2.3"));
        assert_eq!(info("off").status, PluginStatus::Disabled);
        assert_eq!(info("off").disabled_reason.as_deref(), Some("manifest"));
        assert_eq!(
            info("preferences").disabled_reason.as_deref(),
            Some("preferences")
        );
        assert!(
            r.available
                .iter()
                .find(|m| m.id == "preferences")
                .unwrap()
                .enabled
        );
        assert_eq!(info("child").status, PluginStatus::Blocked);
        assert_eq!(info("child").blocked_by, vec!["off"]);
        assert_eq!(info("cycle_a").status, PluginStatus::Blocked);
        assert_eq!(info("future").status, PluginStatus::Failed);
        assert!(info("future")
            .error
            .as_ref()
            .unwrap()
            .contains("requires MiCoPi"));
        assert_eq!(r.packages.iter().filter(|p| p.id.is_none()).count(), 1);
        assert!(!r
            .diagnostics
            .iter()
            .any(|d| d.error.contains("not-opened.dll")));
        let described = r.describe();
        assert_eq!(described["packages"].as_array().unwrap().len(), 8);
        assert_eq!(described["lifecycle"]["contributions"], "restart_required");
    }
    #[test]
    fn dependency_plans_enable_requirements_and_disable_dependents_without_mutation() {
        let root = fixture();
        write(&root, "a", json!({}));
        write(&root, "b", json!({"requires":["a"]}));
        write(&root, "c", json!({"requires":["b"]}));
        let r = Registry::load_with_disabled(&root, &["a".into(), "b".into(), "c".into()]).unwrap();
        let original = r.describe();
        let enabled = r.plan_enabled(&r.disabled, "c", true).unwrap();
        assert!(enabled.disabled.is_empty());
        assert_eq!(enabled.changes.len(), 3);
        assert!(enabled.changes.iter().all(|p| p.enabled));
        let disabled = r.plan_enabled(&[], "a", false).unwrap();
        assert_eq!(disabled.disabled, vec!["a", "b", "c"]);
        assert!(disabled.changes.iter().all(|p| !p.enabled));
        let noop = r.plan_enabled(&disabled.disabled, "a", false).unwrap();
        assert!(!noop.restart_required);
        assert!(noop.changes.is_empty());
        assert_eq!(r.describe(), original);
    }
    #[test]
    fn plans_reject_missing_cycles_duplicate_ids_manifest_disable_and_core_cascades() {
        let root = fixture();
        write(&root, "missing", json!({"requires":["absent"]}));
        write(&root, "a", json!({"requires":["b"]}));
        write(&root, "b", json!({"requires":["a"]}));
        write(&root, "off", json!({"enabled":false}));
        write(&root, "provider", json!({}));
        write(&root, "core", json!({"requires":["provider"]}));
        write(&root, "duplicate", json!({}));
        std::fs::copy(
            root.join("duplicate.json"),
            root.join("duplicate-copy.json"),
        )
        .unwrap();
        let r = Registry::load(&root).unwrap();
        for id in ["missing", "a", "off", "duplicate", "unknown"] {
            assert!(r.plan_enabled(&[], id, true).is_err(), "{id}");
        }
        assert!(r.plan_enabled(&[], "provider", false).is_err());
        assert!(r.plan_enabled(&[], "core", false).is_err());
        assert!(r.plan_enabled(&[], "bad id", false).is_err());
    }
    #[test]
    fn plan_action_is_read_only_and_schema_discoverable() {
        let mut s = Session::new(Registry::bundled()).unwrap();
        let before = s.state();
        let plan = s
            .apply(Action::new(
                "plugins.plan",
                json!({"id":"color","enabled":false}),
            ))
            .unwrap();
        assert!(plan["result"]["disabled"]
            .as_array()
            .unwrap()
            .contains(&json!("color")));
        assert_eq!(s.state(), before);
        assert!(s.registry.actions["plugins.plan"].is_read_only());
    }
    #[test]
    fn locale_sources_only_include_loaded_packages_and_owner_ids_are_shared() {
        let root = fixture();
        write(
            &root,
            "example",
            json!({"locales":{"zh-CN":"locale.json"},"actions":[{"id":"example.read","title":"Read","operation":"state"}],"panels":[{"id":"example.panel","title":"Example"}]}),
        );
        write(
            &root,
            "off",
            json!({"enabled":false,"locales":{"ja":"not-read.json"}}),
        );
        write(
            &root,
            "blocked",
            json!({"requires":["absent"],"locales":{"ko":"not-read.json"}}),
        );
        for (id, path) in [
            ("escape", "../outside.json"),
            ("absolute", "C:/outside.json"),
            ("extension", "locale.txt"),
        ] {
            write(&root, id, json!({"locales":{"ja":path}}));
        }
        write(&root, "english", json!({"locales":{"en":"english.json"}}));
        let r = Registry::load(&root).unwrap();
        let sources = r.locale_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].owner, "example");
        assert_eq!(sources[0].path, root.join("locale.json"));
        assert_eq!(
            r.contribution_owner("panels", "example.panel"),
            Some("example")
        );
        assert_eq!(
            r.contribution_owner("actions", "example.read"),
            Some("example")
        );
        assert!(r
            .packages
            .iter()
            .find(|p| p.id.as_deref() == Some("example"))
            .unwrap()
            .capabilities
            .contains(&"locales".into()));
        for id in ["escape", "absolute", "extension", "english"] {
            assert!(!r.loaded.contains(&id.to_owned()));
        }
    }
}
