use crate::application::{Action, ActionError, Operation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
const BUILTINS: &[(&str, &str)] = &[
    (
        "adjustments/manifest.json",
        include_str!("../../plugins/adjustments/manifest.json"),
    ),
    (
        "canvas/manifest.json",
        include_str!("../../plugins/canvas/manifest.json"),
    ),
    (
        "toolbox/manifest.json",
        include_str!("../../plugins/toolbox/manifest.json"),
    ),
    (
        "color/manifest.json",
        include_str!("../../plugins/color/manifest.json"),
    ),
    (
        "brush/manifest.json",
        include_str!("../../plugins/brush/manifest.json"),
    ),
    (
        "navigator/manifest.json",
        include_str!("../../plugins/navigator/manifest.json"),
    ),
    (
        "layers/manifest.json",
        include_str!("../../plugins/layers/manifest.json"),
    ),
    (
        "replay/manifest.json",
        include_str!("../../plugins/replay/manifest.json"),
    ),
    (
        "workflows/manifest.json",
        include_str!("../../plugins/workflows/manifest.json"),
    ),
    (
        "compositing/manifest.json",
        include_str!("../../plugins/compositing/manifest.json"),
    ),
    (
        "core/manifest.json",
        include_str!("../../plugins/core/manifest.json"),
    ),
    (
        "painting/manifest.json",
        include_str!("../../plugins/painting/manifest.json"),
    ),
    (
        "workbench/manifest.json",
        include_str!("../../plugins/workbench/manifest.json"),
    ),
];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub min_host_version: Option<String>,
    #[serde(default)]
    pub locales: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub library: Option<String>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub actions: Vec<ActionDefinition>,
    /// Access declarations for contributions returned by an unchanged DLL.
    #[serde(default)]
    pub action_access: BTreeMap<String, PixelAccess>,
    #[serde(default)]
    pub panels: Vec<PanelDefinition>,
    #[serde(default)]
    pub states: Vec<StateDefinition>,
    #[serde(default)]
    pub nodes: Vec<super::nodes::Definition>,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolDefinition {
    pub id: String,
    pub title: String,
    pub kind: crate::application::ToolKind,
    #[serde(default)]
    pub preset: bool,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub native: Option<u32>,
    #[serde(default)]
    pub parameters: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDefinition {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub operation: Option<Operation>,
    #[serde(default)]
    pub native: Option<String>,
    #[serde(default)]
    pub steps: Vec<Action>,
    #[serde(default)]
    pub input_schema: Option<Value>,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub document_atomic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_access: Option<PixelAccess>,
    #[serde(default)]
    pub output_schema: Option<Value>,
}
impl ActionDefinition {
    pub fn effective_pixel_access(&self) -> PixelAccess {
        if self.is_read_only() {
            PixelAccess::None
        } else {
            self.pixel_access.unwrap_or(PixelAccess::Geometry)
        }
    }
    pub fn is_document_atomic(&self) -> bool {
        self.operation
            .map(Operation::document_atomic)
            .unwrap_or(self.native.is_some() && self.document_atomic)
    }
    pub fn is_read_only(&self) -> bool {
        self.operation
            .map(Operation::read_only)
            .unwrap_or(self.native.is_some() && self.read_only)
    }
    pub fn schema(&self) -> Value {
        self.input_schema.clone().unwrap_or_else(|| {
            self.operation
                .map(Operation::schema)
                .unwrap_or(json!({"type":"object"}))
        })
    }
}
/// Host-enforced access to existing layer pixels; unrelated host actions retain
/// their own target-aware validation. Legacy opaque DLL actions are geometric.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelAccess {
    None,
    Pixels,
    Geometry,
}
impl PixelAccess {
    pub(crate) fn require(
        self,
        document: &crate::engine::Document,
        index: usize,
    ) -> Result<(), ActionError> {
        if self != Self::None
            && matches!(
                document.mask_state().target,
                crate::engine::raster_masks::PaintTarget::Mask { .. }
            )
        {
            return Err(ActionError::new(
                "mask_target_pending",
                "This operation edits layer pixels. Select Pixels before continuing.",
            ));
        }
        if self == Self::Geometry && document.has_raster_masks() {
            return Err(ActionError::new(
                "mask_editing_pending",
                crate::engine::MASK_EDITING_PENDING,
            ));
        }
        if self != Self::None && document.pixel_locked(index) {
            return Err(ActionError::new(
                "pixel_locked",
                "Unlock layer pixels before editing them",
            ));
        }
        if self == Self::Geometry && document.position_locked(index) {
            return Err(ActionError::new(
                "position_locked",
                "Unlock layer position before transforming its pixels",
            ));
        }
        Ok(())
    }
    pub(crate) fn require_write(
        self,
        document: &crate::engine::Document,
        index: usize,
    ) -> Result<(), ActionError> {
        if self == Self::None {
            return Err(ActionError::new(
                "pixel_access",
                "This contribution has no permission to edit layer pixels",
            ));
        }
        self.require(document, index)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDefinition {
    pub id: String,
    pub title: String,
    pub action: String,
    #[serde(default = "crate::application::actions_empty_object")]
    pub args: Value,
    pub output_schema: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelDefinition {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub builtin: Option<String>,
    #[serde(default)]
    pub buttons: Vec<PanelButton>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelButton {
    pub title: String,
    pub action: Action,
}
#[derive(Clone, Debug, Serialize)]
pub struct PluginDiagnostic {
    pub path: PathBuf,
    pub error: String,
}
#[derive(Clone, Debug)]
pub struct Registry {
    pub packages: Vec<super::PluginInfo>,
    pub disabled: Vec<String>,
    pub nodes: BTreeMap<String, (String, super::nodes::Definition)>,
    pub tools: BTreeMap<String, ToolDefinition>,
    pub actions: BTreeMap<String, ActionDefinition>,
    #[cfg(feature = "desktop")]
    pub(super) command_contacts: super::command_contact::Cache,
    pub states: BTreeMap<String, StateDefinition>,
    pub owners: BTreeMap<String, String>,
    pub panels: BTreeMap<String, PanelDefinition>,
    pub loaded: Vec<String>,
    pub(crate) native_plugins: BTreeMap<String, std::sync::Arc<super::native::NativePlugin>>,
    pub(crate) native_actions: BTreeMap<String, (String, String)>,
    pub(crate) native_tools: BTreeMap<String, (String, u32)>,
    pub available: Vec<Manifest>,
    pub diagnostics: Vec<PluginDiagnostic>,
    pub root: PathBuf,
}
impl Registry {
    pub fn renderer(&self) -> Option<std::sync::Arc<dyn crate::engine::graph::Renderer>> {
        self.native_plugins
            .get("compositing")
            .filter(|p| p.has_compositor())
            .map(|p| p.clone() as std::sync::Arc<dyn crate::engine::graph::Renderer>)
    }
    pub fn bundled() -> Self {
        #[cfg(test)]
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        #[cfg(not(test))]
        let root = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("plugins")))
            .unwrap_or_default();
        Self::assemble(
            BUILTINS
                .iter()
                .map(|(p, s)| {
                    (
                        root.join(p),
                        serde_json::from_str::<Manifest>(s).map_err(|e| e.to_string()),
                    )
                })
                .collect(),
            root.clone(),
            &[],
        )
    }
    pub fn load(root: &Path) -> Result<Self, ActionError> {
        Self::load_with_disabled(root, &[])
    }
    pub fn load_with_disabled(root: &Path, disabled: &[String]) -> Result<Self, ActionError> {
        if !root.exists() {
            std::fs::create_dir_all(root)?;
            for (name, text) in BUILTINS {
                let path = root.join(name);
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, text)?;
            }
            #[cfg(test)]
            let installed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("plugins/painting/micopi_painting_tools.dll");
            #[cfg(not(test))]
            let installed = std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.parent()
                        .map(|p| p.join("plugins/painting/micopi_painting_tools.dll"))
                })
                .unwrap_or_default();
            let target = root.join("painting/micopi_painting_tools.dll");
            if installed.is_file() && installed != target {
                std::fs::copy(&installed, target)?;
            }
            let renderer = installed
                .parent()
                .and_then(Path::parent)
                .map(|p| p.join("compositing/micopi_compositing.dll"));
            if let Some(renderer) = renderer.filter(|p| p.is_file()) {
                std::fs::copy(renderer, root.join("compositing/micopi_compositing.dll"))?;
            }
        }
        let mut paths = vec![];
        for entry in std::fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() && path.join("manifest.json").is_file() {
                paths.push(path.join("manifest.json"));
            } else if path.extension().is_some_and(|x| x == "json") {
                paths.push(path);
            }
        }
        paths.sort();
        if paths.len() > 128 {
            return Err(ActionError::new(
                "plugin_limit",
                "At most 128 plugin packages can be loaded",
            ));
        }
        let mut inputs: Vec<_> = paths
            .into_iter()
            .map(|path| {
                let parsed = std::fs::read_to_string(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|s| {
                        if s.len() > 1024 * 1024 {
                            Err("Plugin manifest exceeds 1 MB".into())
                        } else {
                            serde_json::from_str::<Manifest>(&s).map_err(|e| e.to_string())
                        }
                    });
                (path, parsed)
            })
            .collect();
        // Adapt the old monolithic workbench in memory; never rewrite customized manifests.
        let legacy = inputs
            .iter_mut()
            .find_map(|(_, m)| m.as_mut().ok().filter(|m| m.id == "workbench"))
            .map(|m| {
                let mut panels = Vec::new();
                m.panels.retain(|p| {
                    if matches!(
                        p.builtin.as_deref(),
                        Some("color" | "brush" | "navigator" | "layers")
                    ) && p.builtin.as_deref() == Some(p.id.as_str())
                    {
                        panels.push(p.clone());
                        false
                    } else {
                        true
                    }
                });
                if !panels.is_empty() {
                    let current: Manifest =
                        serde_json::from_str(include_str!("../../plugins/workbench/manifest.json"))
                            .unwrap();
                    for action in current.actions {
                        if !m.actions.iter().any(|a| a.id == action.id) {
                            m.actions.push(action);
                        }
                    }
                    for state in current.states {
                        if !m.states.iter().any(|s| s.id == state.id) {
                            m.states.push(state);
                        }
                    }
                }
                (panels, m.enabled)
            });
        if let Some((panels, enabled)) = legacy.filter(|(p, _)| !p.is_empty()) {
            if let Some(workflow) = inputs
                .iter_mut()
                .find_map(|(_, m)| m.as_mut().ok().filter(|m| m.id == "workflows"))
            {
                let current: Manifest =
                    serde_json::from_str(include_str!("../../plugins/workflows/manifest.json"))
                        .unwrap();
                for panel in current.panels {
                    if !workflow.panels.iter().any(|p| p.id == panel.id) {
                        workflow.panels.push(panel);
                    }
                }
            }
            for (path, text) in BUILTINS {
                let mut module: Manifest = serde_json::from_str(text).unwrap();
                if !["canvas", "toolbox", "color", "brush", "navigator", "layers"]
                    .contains(&module.id.as_str())
                    || inputs
                        .iter()
                        .any(|(_, m)| m.as_ref().is_ok_and(|m| m.id == module.id))
                {
                    continue;
                }
                if let Some(panel) = panels.iter().find(|p| p.id == module.id) {
                    module.panels = vec![panel.clone()];
                }
                module.enabled = enabled;
                inputs.push((root.join(path), Ok(module)));
            }
        }
        Ok(Self::assemble(inputs, root.to_path_buf(), disabled))
    }
    fn assemble(
        inputs: Vec<(PathBuf, Result<Manifest, String>)>,
        root: PathBuf,
        disabled: &[String],
    ) -> Self {
        let inventory: Vec<_> = inputs
            .iter()
            .map(|(path, manifest)| super::PluginInfo::discovered(path, manifest.as_ref().ok()))
            .collect();
        let mut out = Self {
            packages: vec![],
            disabled: disabled.to_vec(),
            nodes: BTreeMap::new(),
            tools: BTreeMap::new(),
            actions: BTreeMap::new(),
            #[cfg(feature = "desktop")]
            command_contacts: Default::default(),
            states: BTreeMap::new(),
            owners: BTreeMap::new(),
            panels: BTreeMap::new(),
            loaded: vec![],
            native_plugins: BTreeMap::new(),
            native_actions: BTreeMap::new(),
            native_tools: BTreeMap::new(),
            available: inputs
                .iter()
                .filter_map(|(_, m)| m.as_ref().ok().cloned())
                .collect(),
            diagnostics: vec![],
            root,
        };
        let mut pending = vec![];
        for (path, value) in inputs {
            match value {
                Ok(m) if m.enabled && !disabled.contains(&m.id) => {
                    match super::packages::validate_metadata(&m) {
                        Ok(()) => pending.push((path, m)),
                        Err(error) => out.diagnostics.push(PluginDiagnostic { path, error }),
                    }
                }
                Ok(_) => {}
                Err(error) => out.diagnostics.push(PluginDiagnostic { path, error }),
            }
        }
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .position(|(_, m)| m.requires.iter().all(|id| out.loaded.contains(id)));
            let Some(index) = ready else {
                for (path, m) in pending {
                    out.diagnostics.push(PluginDiagnostic {
                        path,
                        error: format!("Missing or cyclic dependencies: {}", m.requires.join(", ")),
                    });
                }
                break;
            };
            let (path, mut m) = pending.remove(index);
            if m.api_version != 1 || !valid_id(&m.id) || out.loaded.contains(&m.id) {
                out.diagnostics.push(PluginDiagnostic {
                    path,
                    error: "Invalid plugin API version or duplicate ID".into(),
                });
                continue;
            }
            let mut native_plugin = None;
            if let Some(library) = &m.library {
                match super::native::NativePlugin::load(
                    path.parent().unwrap_or(Path::new(".")),
                    library,
                    &m.id,
                ) {
                    Ok((plugin, contributions)) => {
                        m.actions.extend(contributions.actions);
                        m.tools.extend(contributions.tools);
                        m.panels.extend(contributions.panels);
                        m.states.extend(contributions.states);
                        m.nodes.extend(contributions.nodes);
                        native_plugin = Some(plugin);
                    }
                    Err(error) => {
                        out.diagnostics.push(PluginDiagnostic { path, error });
                        continue;
                    }
                }
            }
            if out.panels.len() + m.panels.len() > 128 {
                out.diagnostics.push(PluginDiagnostic {
                    path,
                    error: "At most 128 plugin windows can be registered".into(),
                });
                continue;
            }
            for panel in &m.panels {
                m.actions.extend(super::windows::commands(panel));
            }
            let validation = (|| -> Result<(), String> {
                if m.api_version != 1 {
                    return Err("Unsupported plugin API version".into());
                }
                if !valid_id(&m.id) || out.loaded.contains(&m.id) {
                    return Err("Invalid or duplicate plugin ID".into());
                }
                for (id, access) in &m.action_access {
                    let action = m.actions.iter_mut().find(|a| &a.id == id).ok_or_else(|| {
                        format!("Action access references an unknown owned action: {id}")
                    })?;
                    if action
                        .pixel_access
                        .is_some_and(|declared| declared != *access)
                    {
                        return Err(format!("Conflicting pixel access declarations: {id}"));
                    }
                    action.pixel_access = Some(*access);
                }
                let mut ids = BTreeSet::new();
                for a in &mut m.actions {
                    if !valid_id(&a.id)
                        || !ids.insert(a.id.clone())
                        || out.actions.contains_key(&a.id)
                    {
                        return Err(format!("Invalid or duplicate action ID: {}", a.id));
                    }
                    if usize::from(a.operation.is_some())
                        + usize::from(!a.steps.is_empty())
                        + usize::from(a.native.is_some())
                        != 1
                    {
                        return Err(format!(
                            "{} must specify exactly one of operation, steps or native",
                            a.id
                        ));
                    }
                    if let Some(handler) = &a.native {
                        if !native_plugin.as_ref().is_some_and(|p| p.has_actions())
                            || !valid_id(handler)
                        {
                            return Err(format!(
                                "{} has no DLL or an invalid native handler",
                                a.id
                            ));
                        }
                    }
                    if a.is_read_only()
                        && a.pixel_access
                            .is_some_and(|access| access != PixelAccess::None)
                    {
                        return Err(format!(
                            "Read-only actions cannot declare pixel writes: {}",
                            a.id
                        ));
                    }
                    if a.native.is_some() {
                        a.pixel_access = Some(a.effective_pixel_access());
                    }
                    if a.steps.len() > 256 {
                        return Err("Macro action exceeds 256 steps".into());
                    }
                    check_schema(&a.schema())?;
                    if let Some(schema) = &a.output_schema {
                        check_schema(schema)?;
                    }
                }
                let mut state_ids = BTreeSet::new();
                for state in &m.states {
                    if crate::application::pipeline::RESOURCES
                        .iter()
                        .any(|(id, _)| *id == state.id)
                        || !valid_id(&state.id)
                        || !state.id.starts_with(&format!("{}.", m.id))
                        || !state_ids.insert(&state.id)
                        || out.states.contains_key(&state.id)
                    {
                        return Err(format!(
                            "State IDs must be unique and within the plugin namespace: {}",
                            state.id
                        ));
                    }
                    let action = m
                        .actions
                        .iter()
                        .find(|a| a.id == state.action)
                        .or_else(|| out.actions.get(&state.action))
                        .ok_or_else(|| {
                            format!("State {} references an unknown action", state.id)
                        })?;
                    if !action.is_read_only() {
                        return Err(format!(
                            "State {} must reference a read-only action",
                            state.id
                        ));
                    }
                    validate(&action.schema(), &state.args).map_err(|e| e.to_string())?;
                    check_schema(&state.output_schema)?;
                }
                let mut tool_ids = BTreeSet::new();
                let mut candidate = out.clone();
                if m.nodes.len() > 256 {
                    return Err("At most 256 explicit node types per plugin".into());
                }
                for a in &m.actions {
                    candidate.actions.insert(a.id.clone(), a.clone());
                }
                for state in &m.states {
                    candidate.states.insert(state.id.clone(), state.clone());
                }
                let mut node_ids = BTreeSet::new();
                for node in &m.nodes {
                    if !super::nodes::valid_definition_id(&node.id, &m.id)
                        || !node_ids.insert(&node.id)
                        || out.nodes.contains_key(&node.id)
                    {
                        return Err(format!(
                            "Invalid, duplicate or incorrectly namespaced node ID: {}",
                            node.id
                        ));
                    }
                    node.resolve(&candidate, &m.id)?;
                }
                for t in &m.tools {
                    if let Some(handler) = t.native {
                        if handler == 0 || !native_plugin.as_ref().is_some_and(|p| p.has_tools()) {
                            return Err(format!(
                                "{} has no point tool DLL or an invalid handler",
                                t.id
                            ));
                        }
                    }
                    if !t.parameters.is_null() {
                        crate::application::Parameters::default()
                            .patched(&t.parameters)
                            .map_err(|e| e.to_string())?;
                    }
                    if !valid_id(&t.id) || !tool_ids.insert(&t.id) || out.tools.contains_key(&t.id)
                    {
                        return Err(format!("Invalid or duplicate tool ID: {}", t.id));
                    }
                }
                let mut panel_ids = BTreeSet::new();
                let mut panel_keys = BTreeSet::new();
                let mut builtins = BTreeSet::new();
                for p in &m.panels {
                    if !valid_id(&p.id)
                        || !panel_ids.insert(&p.id)
                        || out.panels.contains_key(&p.id)
                    {
                        return Err(format!("Invalid or duplicate panel ID: {}", p.id));
                    }
                    let key = super::panel_key(&p.id);
                    if !panel_keys.insert(key)
                        || out.panels.keys().any(|id| super::panel_key(id) == key)
                    {
                        return Err("Persistent panel ID collision".into());
                    }
                    if p.builtin.as_deref().is_some_and(|name| {
                        ![
                            "canvas",
                            "tools",
                            "tool_options",
                            "color",
                            "brush",
                            "navigator",
                            "layers",
                            "nodes",
                            "workflow",
                        ]
                        .contains(&name)
                    }) {
                        return Err("Unknown built-in panel adapter".into());
                    }
                    if let Some(builtin) = &p.builtin {
                        if !builtins.insert(builtin)
                            || out
                                .panels
                                .values()
                                .any(|p| p.builtin.as_ref() == Some(builtin))
                        {
                            return Err(format!("Duplicate built-in window adapter: {builtin}"));
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = validation {
                out.diagnostics.push(PluginDiagnostic { path, error });
                continue;
            }
            if let Some(available) = out.available.iter_mut().find(|p| p.id == m.id) {
                *available = m.clone();
            }
            if let Some(plugin) = native_plugin {
                out.native_plugins.insert(m.id.clone(), plugin);
            }
            out.loaded.push(m.id.clone());
            for t in m.tools {
                if let Some(handler) = t.native {
                    out.native_tools
                        .insert(t.id.clone(), (m.id.clone(), handler));
                }
                out.tools.insert(t.id.clone(), t);
            }
            for a in m.actions {
                out.owners.insert(a.id.clone(), m.id.clone());
                if let Some(handler) = &a.native {
                    out.native_actions
                        .insert(a.id.clone(), (m.id.clone(), handler.clone()));
                }
                out.actions.insert(a.id.clone(), a);
            }
            for state in m.states {
                out.states.insert(state.id.clone(), state);
            }
            for node in m.nodes {
                out.nodes.insert(node.id.clone(), (m.id.clone(), node));
            }
            for p in m.panels {
                out.panels.insert(p.id.clone(), p);
            }
        }
        out.packages = inventory
            .into_iter()
            .map(|mut package| {
                package.resolve(&out);
                package
            })
            .collect();
        #[cfg(feature = "desktop")]
        out.rebuild_command_contact_policies();
        out
    }
    pub fn describe(&self) -> Value {
        json!({"directory":self.root,"loaded":self.loaded,"packages":self.packages,"contribution_owners":self.contribution_owners(),"disabled":self.disabled,"lifecycle":{"contributions":"restart_required","native_library":"restart_required","locales":"hot_reload"},"native":self.native_plugins.iter().map(|(id,p)|json!({"id":id,"library":p.path,"abi_version":micopi_plugin_api::ABI_VERSION,"filters":{"levels_prepare":p.supports_levels_preparation(),"levels_render":p.supports_levels_rendering(),"curves_prepare":p.supports_curves_preparation(),"curves_render":p.supports_curves_rendering()},"analysis":{"histogram":p.supports_histogram()},"samplers":{"raw_a8_affine":p.supports_a8_affine(),"raw_a8_bounds":p.supports_a8_affine_bounds()},"tool_options":{"flow":p.supports_stroke_opacity(),"stroke_opacity":p.supports_stroke_opacity(),"tip_geometry":p.supports_tip_geometry()}})).collect::<Vec<_>>(),"diagnostics":self.diagnostics,"tools":self.tools.values().collect::<Vec<_>>(),"actions":self.actions.values().collect::<Vec<_>>(),"states":self.states.values().collect::<Vec<_>>(),"nodes":self.nodes.values().map(|(_,d)|d).collect::<Vec<_>>(),"panels":self.panels.values().collect::<Vec<_>>()})
    }
    /// Cold discovery of the selected tool's provider, never pointer admission.
    pub(crate) fn tool_option_capabilities(&self, tool: &str) -> Value {
        let base = self
            .tools
            .get(tool)
            .and_then(|definition| match definition.kind {
                crate::application::ToolKind::Brush => Some("brush"),
                crate::application::ToolKind::Eraser => Some("eraser"),
                _ => None,
            });
        let plugin = base
            .and_then(|base| {
                self.native_tools
                    .get(tool)
                    .or_else(|| self.native_tools.get(base))
            })
            .and_then(|(owner, _)| self.native_plugins.get(owner));
        let available = plugin.is_some_and(|plugin| plugin.supports_stroke_opacity());
        json!({"flow":available,"stroke_opacity":available,"tip_geometry":plugin.is_some_and(|plugin| plugin.supports_tip_geometry())})
    }
}
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

// A deliberately small, documented JSON Schema subset, shared by every transport.
pub(crate) fn check_schema(s: &Value) -> Result<(), String> {
    let obj = s.as_object().ok_or("input_schema must be an object")?;
    for (key, value) in obj {
        if key == "type"
            && !value.as_str().is_some_and(|t| {
                ["object", "array", "string", "boolean", "integer", "number"].contains(&t)
            })
        {
            return Err("Unsupported schema.type".into());
        }
        if key == "required"
            && !value
                .as_array()
                .is_some_and(|items| items.iter().all(Value::is_string))
        {
            return Err("schema.required must be an array of strings".into());
        }
        if key == "additionalProperties" && !value.is_boolean() {
            return Err("schema.additionalProperties must be boolean".into());
        }
        if ["minimum", "maximum"].contains(&key.as_str())
            && !value.as_f64().is_some_and(f64::is_finite)
        {
            return Err("Schema bounds must be finite numbers".into());
        }
        if ["minItems", "maxItems", "minLength"].contains(&key.as_str()) && !value.is_u64() {
            return Err("Schema lengths must be non-negative integers".into());
        }
        if key == "enum" && !value.is_array() {
            return Err("schema.enum must be an array".into());
        }
        if ![
            "type",
            "properties",
            "required",
            "additionalProperties",
            "enum",
            "minimum",
            "maximum",
            "minItems",
            "maxItems",
            "minLength",
            "items",
            "description",
            "title",
            "default",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("Unsupported schema keyword: {key}"));
        }
        if key == "properties" {
            for (_, schema) in value.as_object().ok_or("properties must be an object")? {
                check_schema(schema)?;
            }
        }
        if key == "items" {
            check_schema(value)?;
        }
    }
    Ok(())
}
pub fn validate(schema: &Value, value: &Value) -> Result<(), ActionError> {
    fn check(s: &Value, v: &Value, path: &str) -> Result<(), String> {
        if let Some(t) = s.get("type").and_then(Value::as_str) {
            let ok = match t {
                "object" => v.is_object(),
                "array" => v.is_array(),
                "string" => v.is_string(),
                "boolean" => v.is_boolean(),
                "integer" => v.as_f64().is_some_and(|n| n.fract() == 0.),
                "number" => v.as_f64().is_some_and(f64::is_finite),
                _ => false,
            };
            if !ok {
                return Err(format!("{path} requires {t}"));
            }
        }
        if let Some(options) = s.get("enum").and_then(Value::as_array) {
            if !options.contains(v) {
                return Err(format!("{path} is not an allowed value"));
            }
        }
        if let Some(obj) = v.as_object() {
            if let Some(required) = s.get("required").and_then(Value::as_array) {
                for k in required {
                    if let Some(k) = k.as_str() {
                        if !obj.contains_key(k) {
                            return Err(format!("{path} is missing {k}"));
                        }
                    }
                }
            }
            for (k, v) in obj {
                if let Some(rule) = s.get("properties").and_then(|p| p.get(k)) {
                    check(rule, v, &format!("{path}.{k}"))?;
                } else if s.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Err(format!("{path} has unknown field: {k}"));
                }
            }
        }
        if let Some(n) = v.as_f64() {
            if s.get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| n < min)
                || s.get("maximum")
                    .and_then(Value::as_f64)
                    .is_some_and(|max| n > max)
            {
                return Err(format!("{path} is out of range"));
            }
        }
        if let Some(a) = v.as_array() {
            if s.get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|n| a.len() < (n as usize))
                || s.get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| a.len() > n as usize)
            {
                return Err(format!("{path} array length is out of range"));
            }
            if let Some(rule) = s.get("items") {
                for (index, v) in a.iter().enumerate() {
                    check(rule, v, &format!("{path}[{index}]"))?;
                }
            }
        }
        if let Some(text) = v.as_str() {
            if s.get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|n| text.chars().count() < n as usize)
            {
                return Err(format!("{path} string is too short"));
            }
        }
        Ok(())
    }
    check(schema, value, "args").map_err(|e| ActionError::new("invalid_arguments", e))
}

#[cfg(test)]
mod access_tests {
    use super::*;

    #[test]
    fn legacy_native_access_is_conservative_and_readonly_has_no_write_access() {
        let mut action: ActionDefinition = serde_json::from_value(json!({
            "id":"legacy.action", "title":"Legacy action", "native":"run"
        }))
        .unwrap();
        assert_eq!(action.effective_pixel_access(), PixelAccess::Geometry);
        action.read_only = true;
        assert_eq!(action.effective_pixel_access(), PixelAccess::None);
        action.read_only = false;
        action.pixel_access = Some(PixelAccess::Pixels);
        assert_eq!(action.effective_pixel_access(), PixelAccess::Pixels);
    }

    fn assemble(access: Value, declared: Option<PixelAccess>) -> Registry {
        let mut action = json!({"id":"access.state", "title":"State", "operation":"state"});
        if let Some(declared) = declared {
            action["pixel_access"] = json!(declared);
        }
        let manifest = serde_json::from_value(json!({
            "api_version":1, "id":"access", "name":"Access policy fixture",
            "actions":[action], "action_access":access,
            "panels":[{"id":"access.panel","title":"Fixture"}]
        }))
        .unwrap();
        Registry::assemble(
            vec![(PathBuf::from("access/manifest.json"), Ok(manifest))],
            PathBuf::new(),
            &[],
        )
    }

    #[test]
    fn access_overlay_is_applied_and_invalid_owned_references_reject_the_whole_package() {
        let registry = assemble(json!({"access.state":"none"}), None);
        assert_eq!(registry.loaded, ["access"]);
        assert_eq!(
            registry.actions["access.state"].pixel_access,
            Some(PixelAccess::None)
        );
        assert!(registry.panels.contains_key("access.panel"));
        for (overlay, declared) in [
            (json!({"another.owner":"pixels"}), None),
            (json!({"access.state":"pixels"}), None),
            (json!({"access.state":"none"}), Some(PixelAccess::Pixels)),
        ] {
            let registry = assemble(overlay, declared);
            assert!(registry.loaded.is_empty());
            assert!(registry.actions.is_empty());
            assert!(registry.panels.is_empty());
            assert_eq!(registry.diagnostics.len(), 1);
        }
    }
}
