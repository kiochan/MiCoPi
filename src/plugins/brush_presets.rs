//! Typed profile data, never runtime plugin contributions or pointer payloads.
use crate::application::{ActionError, ActionResult, Operation, Session, ToolKind};
use crate::configuration::{
    brush_presets::{
        canonical_value, BrushPresetBundle, BrushPresetLibrary, PresetParameters, UserPreset,
        MAX_BUNDLE_BYTES, MAX_PRESETS,
    },
    Settings,
};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};

fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("brush_presets", message)
}
fn io(message: impl Into<String>) -> ActionError {
    ActionError::new("preset_io", message)
}
pub(crate) fn parameters_schema() -> Value {
    let mut schema = Operation::Parameters.schema();
    let fields = schema["properties"].as_object_mut().unwrap();
    fields.remove("color");
    fields.remove("background_color");
    // Legacy bundles remain complete snapshots: omitted tip fields reset to
    // circular defaults. Only the twelve original scalar fields are required.
    let required = fields
        .keys()
        .filter(|key| !matches!(key.as_str(), "tip_shape" | "tip_roundness" | "tip_angle"))
        .cloned()
        .collect::<Vec<_>>();
    schema["required"] = json!(required);
    schema
}
pub(crate) fn schema(op: Operation) -> Value {
    use Operation::*;
    let id = json!({"type":"integer","minimum":1,"maximum":4294967295u64});
    let mut properties = json!({});
    let mut required = vec![];
    if op != BrushPresetsQuery {
        properties["expected_library_token"] =
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    }
    if matches!(op, BrushPresetUpsert | BrushPresetDelete | BrushPresetApply) {
        properties["preset_id"] = id.clone();
    }
    if matches!(op, BrushPresetDelete | BrushPresetApply) {
        required.push("preset_id");
    }
    if op == BrushPresetUpsert {
        properties["name"] = json!({"type":"string"});
        properties["owner"] = json!({"type":"string"});
        properties["tool_id"] = json!({"type":"string"});
        properties["parameters"] = parameters_schema();
        required.extend(["name", "owner", "tool_id", "parameters"]);
    }
    if matches!(op, BrushPresetsImport | BrushPresetsExport) {
        properties["path"] = json!({"type":"string"});
        required.push("path");
    }
    if op == BrushPresetsExport {
        properties["preset_ids"] =
            json!({"type":"array","minItems":1,"maxItems":MAX_PRESETS,"items":id});
        required.push("preset_ids");
    }
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub(crate) fn validate_literal(op: Operation, args: &Value) -> Result<(), ActionError> {
    if op == Operation::BrushPresetUpsert {
        let values = PresetParameters::decode_value(&args["parameters"]).map_err(error)?;
        UserPreset::new(
            1,
            args["name"].as_str().unwrap_or("").into(),
            args["owner"].as_str().unwrap_or("").into(),
            args["tool_id"].as_str().unwrap_or("").into(),
            values,
        )
        .map_err(error)?;
    }
    if op == Operation::BrushPresetsExport {
        ids(&args["preset_ids"])?;
    }
    Ok(())
}
#[derive(Clone)]
pub(crate) struct CapturedPreset {
    pub owner: String,
    pub tool_id: String,
    pub parameters: PresetParameters,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct PresetStatus {
    pub available: bool,
    pub reason: Option<String>,
}

fn resolve(
    s: &Session,
    owner: &str,
    tool_id: &str,
    values: &PresetParameters,
) -> Result<ToolKind, ActionError> {
    let tool = s.registry.tools.get(tool_id).ok_or_else(|| {
        ActionError::new(
            "preset_provider",
            "The preset's registered tool is unavailable",
        )
    })?;
    if s.registry.contribution_owner("tools", tool_id) != Some(owner) {
        return Err(ActionError::new(
            "preset_provider",
            "The preset tool belongs to a different contribution owner",
        ));
    }
    let base = match tool.kind {
        ToolKind::Brush => "brush",
        ToolKind::Eraser => "eraser",
        _ => {
            return Err(ActionError::new(
                "preset_provider",
                "User presets require a registered Brush or Eraser tool",
            ))
        }
    };
    let native = s
        .registry
        .native_tools
        .get(tool_id)
        .or_else(|| s.registry.native_tools.get(base))
        .and_then(|(owner, _)| s.registry.native_plugins.get(owner))
        .ok_or_else(|| {
            ActionError::new(
                "preset_provider",
                "The tool's native provider is unavailable",
            )
        })?;
    let parameters = values.apply_to(&s.parameters).map_err(error)?;
    native.validate_preset_parameters(&parameters)?;
    Ok(tool.kind)
}
pub(crate) fn provider_status(
    s: &Session,
    owner: &str,
    tool_id: &str,
    parameters: &PresetParameters,
) -> PresetStatus {
    match resolve(s, owner, tool_id, parameters) {
        Ok(_) => PresetStatus {
            available: true,
            reason: None,
        },
        Err(e) => PresetStatus {
            available: false,
            reason: Some(e.message),
        },
    }
}
pub(crate) fn capture_current(s: &Session) -> Result<CapturedPreset, ActionError> {
    let tool = s
        .registry
        .tools
        .get(&s.selected_tool)
        .ok_or_else(|| error("The selected registered tool is unavailable"))?;
    if !matches!(tool.kind, ToolKind::Brush | ToolKind::Eraser) {
        return Err(error("Select a Brush or Eraser before saving a preset"));
    }
    let owner = s
        .registry
        .contribution_owner("tools", &s.selected_tool)
        .ok_or_else(|| error("The selected tool contribution has no owner"))?;
    Ok(CapturedPreset {
        owner: owner.into(),
        tool_id: s.selected_tool.clone(),
        parameters: PresetParameters::capture(&s.parameters).map_err(error)?,
    })
}
fn idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() || s.doc.history_group_active() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before editing preset files or reloading the library",
        ));
    }
    Ok(())
}
fn token(s: &Session, args: &Value) -> Result<(), ActionError> {
    if args
        .get("expected_library_token")
        .is_some_and(|value| value.as_f64() != Some(s.preset_library_token() as f64))
    {
        return Err(ActionError::new(
            "preset_library_changed",
            "The preset library changed; refresh the library before retrying",
        ));
    }
    Ok(())
}
fn path(s: &Session) -> Result<PathBuf, ActionError> {
    s.settings_path.clone().ok_or_else(|| {
        ActionError::new(
            "preset_profile",
            "An active settings profile is required for preset file operations",
        )
    })
}
fn file_path(args: &Value) -> Result<PathBuf, ActionError> {
    let value = args["path"]
        .as_str()
        .filter(|path| !path.trim().is_empty() && !path.contains('\0') && path.len() <= 32768)
        .ok_or_else(|| {
            error("A nonempty preset file path of at most 32768 UTF-8 bytes is required")
        })?;
    Ok(PathBuf::from(value))
}
fn file_identity(path: &std::path::Path) -> Option<PathBuf> {
    path.canonicalize()
        .ok()
        .or_else(|| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?)))
}
fn same_file_target(a: &std::path::Path, b: &std::path::Path) -> bool {
    if a == b {
        return true;
    }
    let equal = |a: &std::path::Path, b: &std::path::Path| {
        #[cfg(windows)]
        {
            a.as_os_str().to_string_lossy().to_lowercase()
                == b.as_os_str().to_string_lossy().to_lowercase()
        }
        #[cfg(not(windows))]
        {
            a == b
        }
    };
    file_identity(a)
        .zip(file_identity(b))
        .is_some_and(|(a, b)| equal(&a, &b))
        || std::path::absolute(a)
            .ok()
            .zip(std::path::absolute(b).ok())
            .is_some_and(|(a, b)| equal(&a, &b))
}
fn ids(value: &Value) -> Result<Vec<u64>, ActionError> {
    let rows = value
        .as_array()
        .ok_or_else(|| error("preset_ids must be an explicit array"))?;
    if rows.is_empty() || rows.len() > MAX_PRESETS {
        return Err(error("Select one to 128 user presets"));
    }
    let mut ids = Vec::with_capacity(rows.len());
    for row in rows {
        let id = super::bounded_integer(row)
            .filter(|id| *id > 0)
            .ok_or_else(|| error("Preset IDs must be bounded positive integers"))?;
        if ids.contains(&id) {
            return Err(error("Preset IDs must be unique"));
        }
        ids.push(id);
    }
    Ok(ids)
}
fn preset_id(args: &Value) -> u64 {
    super::bounded_integer(&args["preset_id"]).unwrap()
}
fn selected(s: &Session, id: u64) -> Result<&UserPreset, ActionError> {
    s.preset_library()
        .get(id)
        .ok_or_else(|| ActionError::new("unknown_preset", "The user preset does not exist"))
}
fn notify(s: &mut Session) {
    if s.runtime == "desktop" {
        s.ui_effects.push_back(json!({"id":"brush.presets.changed","value":s.settings_path,"library_token":s.preset_library_token()}));
    }
}
fn queue_room(s: &Session) -> Result<(), ActionError> {
    if s.runtime == "desktop" && s.ui_effects.len() >= 128 {
        Err(ActionError::new(
            "presentation_queue_full",
            "Wait for queued presentation commands before retrying",
        ))
    } else {
        Ok(())
    }
}
fn profile(s: &Session) -> Result<Settings, ActionError> {
    let settings = Settings::load(path(s)?).map_err(io)?;
    if &settings.brush_presets != s.preset_library() {
        return Err(ActionError::new("preset_library_changed","The profile library changed outside this session; reload the preset library before retrying"));
    }
    Ok(settings)
}
fn publish_library(
    s: &mut Session,
    mut settings: Settings,
    library: BrushPresetLibrary,
) -> Result<(), ActionError> {
    library.validate().map_err(error)?;
    if &library == s.preset_library() {
        return Ok(());
    }
    // Preflight the only fallible cache change before replacing the file.
    let next = s
        .preset_library_token()
        .checked_add(1)
        .filter(|value| *value <= 9_007_199_254_740_991)
        .ok_or_else(|| error("Preset library token limit reached"))?;
    settings.brush_presets = library;
    settings.save().map_err(io)?;
    s.brush_presets = settings.brush_presets;
    s.preset_library_token = next;
    notify(s);
    Ok(())
}
fn query(s: &Session) -> ActionResult {
    let current = PresetParameters::capture(&s.parameters).map_err(error)?;
    let mut matching = Vec::new();
    let user=s.preset_library().items.iter().map(|item| {
        let status=provider_status(s,&item.owner,&item.tool_id,&item.parameters);
        let matches=s.registry.contribution_owner("tools",&item.tool_id)==Some(item.owner.as_str())&&s.selected_tool==item.tool_id&&current==item.parameters;
        if matches {matching.push(item.id);}
        Ok(json!({"preset":canonical_value(item).map_err(error)?,"available":status.available,"reason":status.reason,"matching":matches}))
    }).collect::<Result<Vec<Value>,ActionError>>()?;
    let builtin=s.registry.tools.values().filter(|tool|tool.preset&&matches!(tool.kind,ToolKind::Brush|ToolKind::Eraser)).map(|tool| {
        let owner=s.registry.contribution_owner("tools",&tool.id).unwrap_or("");
        let next=if tool.parameters.is_null(){Ok(s.parameters.clone())}else{s.parameters.patched(&tool.parameters)}?;
        let parameters=PresetParameters::capture(&next).map_err(error)?;
        let status=provider_status(s,owner,&tool.id,&parameters);
        Ok(json!({"tool_id":tool.id,"owner":owner,"title":tool.title,"parameters":parameters.value().map_err(error)?,"available":status.available,"reason":status.reason,"matching":s.selected_tool==tool.id&&current==parameters}))
    }).collect::<Result<Vec<Value>,ActionError>>()?;
    Ok(
        json!({"version":1,"library_token":s.preset_library_token(),"library":canonical_value(s.preset_library()).map_err(error)?,"user":user,"builtin":builtin,"matching_preset_ids":matching,"profile_path":s.settings_path}),
    )
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    use Operation::*;
    if op == BrushPresetsQuery {
        return query(s);
    }
    token(s, &args)?;
    if op == BrushPresetApply {
        let record = selected(s, preset_id(&args))?.clone();
        let kind = resolve(s, &record.owner, &record.tool_id, &record.parameters)?;
        let mut next = record.parameters.apply_to(&s.parameters).map_err(error)?;
        queue_room(s)?;
        if s.pointer_checkpoint.is_some() && !s.stroke.active() {
            return Err(ActionError::new(
                "stroke_active",
                "Wait for the pending input boundary before applying a preset",
            ));
        }
        if let Some(last) = s.stroke.last_sample() {
            s.pointer(crate::application::Pointer {
                phase: crate::application::PointerPhase::Up,
                ..last
            })?;
        }
        next.color = s.parameters.color.clone();
        next.background_color = s.parameters.background_color.clone();
        s.selected_tool = record.tool_id;
        s.tool = kind;
        s.parameters = next;
        if s.runtime == "desktop" {
            s.ui_effects.push_back(json!({"id":"brush.preset.applied","value":s.selected_tool,"document_identity":s.doc.identity}));
        }
        return Ok(
            json!({"preset_id":record.id,"selected_tool":s.selected_tool,"parameters":s.parameter_state(),"revision":s.revision}),
        );
    }
    idle(s)?;
    queue_room(s)?;
    if op == BrushPresetsReload {
        let settings = Settings::load(path(s)?).map_err(io)?;
        s.load_profile_presets(&settings)?;
        notify(s);
        return Ok(json!({"library_token":s.preset_library_token()}));
    }
    if op == BrushPresetsExport {
        let ids = ids(&args["preset_ids"])?;
        let records = ids
            .iter()
            .map(|id| selected(s, *id).cloned())
            .collect::<Result<Vec<_>, _>>()?;
        let text = BrushPresetBundle::from_items(records)
            .map_err(error)?
            .encode()
            .map_err(error)?;
        let target = file_path(&args)?;
        if s.settings_path
            .as_ref()
            .is_some_and(|profile| same_file_target(profile, &target))
        {
            return Err(error(
                "Preset exports cannot replace the active settings profile",
            ));
        }
        crate::configuration::atomic_write(&target, &text).map_err(io)?;
        return Ok(
            json!({"path":target,"preset_ids":ids,"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"bytes":text.len()}),
        );
    }
    let settings = profile(s)?;
    let mut library = s.preset_library().clone();
    match op {
        BrushPresetUpsert => {
            let parameters = PresetParameters::decode_value(&args["parameters"]).map_err(error)?;
            let name = args["name"].as_str().unwrap().to_owned();
            let owner = args["owner"].as_str().unwrap().to_owned();
            let tool = args["tool_id"].as_str().unwrap().to_owned();
            let id = if args.get("preset_id").is_some() {
                let id = preset_id(&args);
                selected(s, id)?;
                let record = UserPreset::new(id, name, owner, tool, parameters).map_err(error)?;
                *library.items.iter_mut().find(|item| item.id == id).unwrap() = record;
                id
            } else {
                library
                    .insert(name, owner, tool, parameters)
                    .map_err(error)?
            };
            publish_library(s, settings, library)?;
            Ok(json!({"preset_id":id,"library_token":s.preset_library_token()}))
        }
        BrushPresetDelete => {
            let id = preset_id(&args);
            selected(s, id)?;
            library.remove(id).map_err(error)?;
            publish_library(s, settings, library)?;
            Ok(json!({"deleted_preset_id":id,"library_token":s.preset_library_token()}))
        }
        BrushPresetsImport => {
            let mut text = String::new();
            File::open(file_path(&args)?)
                .map_err(|e| io(e.to_string()))?
                .take(MAX_BUNDLE_BYTES as u64 + 1)
                .read_to_string(&mut text)
                .map_err(|e| io(e.to_string()))?;
            let bundle = BrushPresetBundle::decode(&text).map_err(error)?;
            if library.items.len() + bundle.items.len() > MAX_PRESETS {
                return Err(error("Imported presets exceed the library count limit"));
            }
            let mut imported = Vec::new();
            let mut map = Vec::new();
            for item in bundle.items {
                let id = library
                    .insert(item.name, item.owner, item.tool_id, item.parameters)
                    .map_err(error)?;
                imported.push(id);
                map.push(json!({"source_id":item.id,"preset_id":id}));
            }
            publish_library(s, settings, library)?;
            Ok(
                json!({"imported_ids":imported,"id_map":map,"library_token":s.preset_library_token(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"bytes":text.len()}),
            )
        }
        _ => Err(error("Operation routed to the wrong preset handler")),
    }
}
