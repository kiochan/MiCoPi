use super::{Action, ActionError, ActionResult, Operation};
use crate::{
    engine::Document,
    plugins::{validate, Registry},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Brush,
    Eraser,
    Picker,
    Hand,
    Zoom,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpacityMode {
    #[default]
    Dab,
    Stroke,
}
impl OpacityMode {
    pub fn is_dab(&self) -> bool {
        *self == Self::Dab
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Parameters {
    pub radius: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub color: String,
    pub background_color: String,
    pub stabilizer: f32,
    pub pressure_size: bool,
    pub pressure_opacity: bool,
    pub pressure_gamma: f32,
    pub minimum_radius: f32,
    #[serde(
        default = "default_spacing",
        skip_serializing_if = "legacy_spacing",
        serialize_with = "serialize_spacing"
    )]
    pub spacing: f32,
    #[serde(default, skip_serializing_if = "zero_opacity")]
    pub minimum_opacity: f32,
    #[serde(default = "default_flow", skip_serializing_if = "full_flow")]
    pub flow: f32,
    #[serde(default, skip_serializing_if = "OpacityMode::is_dab")]
    pub opacity_mode: OpacityMode,
    #[serde(default, skip_serializing_if = "super::TipShape::is_round")]
    pub tip_shape: super::TipShape,
    #[serde(
        default = "super::default_tip_roundness",
        skip_serializing_if = "super::full_tip_roundness"
    )]
    pub tip_roundness: f32,
    #[serde(default, skip_serializing_if = "super::zero_tip_angle")]
    pub tip_angle: f32,
    #[serde(default, skip_serializing_if = "super::SymmetryMode::is_off")]
    pub symmetry: super::SymmetryMode,
    #[serde(
        default = "super::brush_symmetry::center",
        skip_serializing_if = "super::brush_symmetry::is_center"
    )]
    pub symmetry_x: f32,
    #[serde(
        default = "super::brush_symmetry::center",
        skip_serializing_if = "super::brush_symmetry::is_center"
    )]
    pub symmetry_y: f32,
}
fn default_flow() -> f32 {
    1.
}
fn full_flow(value: &f32) -> bool {
    *value == 1.
}
fn default_spacing() -> f32 {
    0.11
}
// The f32 representation of 1% widens below its decimal schema minimum.
// Canonicalize only this boundary; preserve every other existing wire value.
pub(crate) fn serialize_spacing<S: serde::Serializer>(
    value: &f32,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if *value == 0.01_f32 {
        serializer.serialize_f64(0.01)
    } else {
        serializer.serialize_f32(*value)
    }
}
pub(crate) fn spacing_json(value: f32) -> Value {
    if value == 0.01_f32 {
        json!(0.01)
    } else {
        json!(value)
    }
}
fn legacy_spacing(value: &f32) -> bool {
    *value == default_spacing()
}
fn zero_opacity(value: &f32) -> bool {
    *value == 0.
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            radius: 6.,
            opacity: 1.,
            hardness: 0.85,
            color: "#333743".into(),
            background_color: "#FFFFFF".into(),
            stabilizer: 0.35,
            pressure_size: true,
            pressure_opacity: false,
            pressure_gamma: 1.,
            minimum_radius: 0.08,
            spacing: default_spacing(),
            minimum_opacity: 0.,
            flow: 1.,
            opacity_mode: OpacityMode::Dab,
            tip_shape: super::TipShape::Round,
            tip_roundness: 1.,
            tip_angle: 0.,
            symmetry: super::SymmetryMode::Off,
            symmetry_x: 0.5,
            symmetry_y: 0.5,
        }
    }
}
impl Parameters {
    pub(crate) fn has_custom_symmetry(&self) -> bool {
        !self.symmetry.is_off() || self.symmetry_x != 0.5 || self.symmetry_y != 0.5
    }
    pub(crate) fn has_custom_tip(&self) -> bool {
        self.tip_shape != super::TipShape::Round || self.tip_roundness != 1. || self.tip_angle != 0.
    }
    pub fn patched(&self, patch: &Value) -> Result<Self, ActionError> {
        validate(&Operation::Parameters.schema(), patch)?;
        let mut value = serde_json::to_value(self)?;
        for (k, v) in patch
            .as_object()
            .ok_or_else(|| ActionError::new("invalid_arguments", "Arguments must be an object"))?
        {
            value[k] = v.clone();
        }
        let mut next: Self = serde_json::from_value(value)?;
        next.color = format!("#{:06X}", parse_color(&next.color)?);
        next.background_color = format!("#{:06X}", parse_color(&next.background_color)?);
        Ok(next)
    }
    pub fn brush(&self, erase: bool, pressure: f32, pen: bool) -> crate::engine::Brush {
        let brush = crate::engine::Brush {
            radius: self.radius,
            opacity: self.opacity,
            hardness: self.hardness,
            color: parse_color(&self.color).unwrap_or(0),
            erase,
        };
        if pen {
            crate::engine::input::Dynamics {
                size: self.pressure_size,
                opacity: self.pressure_opacity,
                gamma: self.pressure_gamma,
                minimum: self.minimum_radius,
            }
            .apply(brush, pressure)
        } else {
            brush
        }
    }
}
pub fn parse_color(text: &str) -> Result<u32, ActionError> {
    if text.len() == 7 && text.starts_with('#') {
        u32::from_str_radix(&text[1..], 16)
            .map_err(|_| ActionError::new("invalid_color", "Color requires #RRGGBB"))
    } else {
        Err(ActionError::new("invalid_color", "Color requires #RRGGBB"))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pointer {
    pub phase: PointerPhase,
    pub x: f32,
    pub y: f32,
    #[serde(default = "one")]
    pub pressure: f32,
    #[serde(default)]
    pub time: u32,
    #[serde(default)]
    pub pen: bool,
    #[serde(default)]
    pub eraser: bool,
    #[serde(default)]
    pub tool_override: Option<ToolKind>,
    #[serde(default)]
    pub view_x: Option<f32>,
    #[serde(default)]
    pub view_y: Option<f32>,
}
fn one() -> f32 {
    1.
}
impl Pointer {
    pub fn validate(&self) -> Result<(), ActionError> {
        if !self.x.is_finite()
            || !self.y.is_finite()
            || self.x.abs() > 32768.
            || self.y.abs() > 32768.
            || !self.pressure.is_finite()
            || !(0.0..=1.0).contains(&self.pressure)
            || self
                .view_x
                .is_some_and(|x| !x.is_finite() || x.abs() > 32768.)
            || self
                .view_y
                .is_some_and(|x| !x.is_finite() || x.abs() > 32768.)
        {
            Err(ActionError::new(
                "invalid_pointer",
                "Invalid coordinates or pressure",
            ))
        } else {
            Ok(())
        }
    }
}
fn view_mirror_is_off(value: &bool) -> bool {
    !*value
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
    pub fit: bool,
    #[serde(default, skip_serializing_if = "view_mirror_is_off")]
    pub mirror_x: bool,
    #[serde(default, skip_serializing_if = "view_mirror_is_off")]
    pub mirror_y: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan_x: 0.,
            pan_y: 0.,
            fit: true,
            mirror_x: false,
            mirror_y: false,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    Action {
        action: String,
        args: Value,
        success: bool,
        error: Option<String>,
    },
    Pointer {
        sample: Pointer,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub sequence: u64,
    pub revision: u64,
    #[serde(flatten)]
    pub event: EventKind,
}
pub struct Session {
    pub(crate) system_clipboard: Option<Box<dyn crate::platform::clipboard::SystemClipboard>>,
    pub(crate) system_clipboard_event: Option<Value>,
    pub(crate) transform_event: Option<Value>,
    pub(crate) export_event: Option<Value>,
    pub(crate) clipboard: Option<std::sync::Arc<crate::plugins::clipboard::Payload>>,
    pub(crate) pointer_checkpoint: Option<(
        super::atomic::Checkpoint,
        crate::engine::replay::JournalCheckpoint,
    )>,
    pub(crate) atomic_depth: usize,
    pub(crate) journal: Option<crate::engine::replay::Journal>,
    pub(crate) loaded_replay: Option<crate::engine::replay::Journal>,
    pub(crate) restored_replay: Option<crate::engine::replay::Journal>,
    pub(crate) replay_identity: u64,
    pub(crate) replay_error: Option<String>,
    pub(crate) replay_samples: Vec<Pointer>,
    pub(crate) replay_direct_contact: bool,
    pub(crate) pending_save: Option<(PathBuf, Option<PathBuf>, bool, u64)>,
    pub(crate) workflow_stack: Vec<crate::engine::workflow::Graph>,
    pub(crate) doc: Document,
    pub file: Option<PathBuf>,
    pub source_file: Option<PathBuf>,
    pub runtime: &'static str,
    pub(crate) last_pointer: Option<Pointer>,
    pub(crate) pen_observed: bool,
    pub registry: Registry,
    pub selected_tool: String,
    pub tool: ToolKind,
    pub parameters: Parameters,
    pub view: View,
    pub revision: u64,
    pub ui: Value,
    pub selection_defaults: super::SelectionSampling,
    pub movement_defaults: super::MovementPreferences,
    pub settings_path: Option<PathBuf>,
    pub(crate) brush_presets: crate::configuration::brush_presets::BrushPresetLibrary,
    pub(crate) preset_library_token: u64,
    pub ui_effects: VecDeque<Value>,
    pub(crate) presentation_captures: super::presentation_captures::Captures,
    pub(crate) stroke: crate::plugins::tools::StrokeEngine,
    pub(crate) events: VecDeque<Event>,
    pub(crate) sequence: u64,
    pub(crate) native_instances:
        std::collections::BTreeMap<String, crate::plugins::native::Instance>,
    pub(crate) native_stack: Vec<String>,
    pub(crate) read_depth: usize,
    pub(crate) execution_depth: usize,
    pub(crate) remaining_actions: usize,
    pub(crate) tool_instances:
        std::collections::BTreeMap<String, crate::plugins::native::ToolInstance>,
}
impl Session {
    pub fn new(registry: Registry) -> Result<Self, ActionError> {
        let fallback = crate::plugins::ToolDefinition {
            id: "brush".into(),
            title: "Brush (Plugin Unavailable)".into(),
            kind: ToolKind::Brush,
            preset: false,
            icon: None,
            native: None,
            parameters: Value::Null,
        };
        let tool = registry
            .tools
            .get("brush")
            .or_else(|| registry.tools.values().next())
            .unwrap_or(&fallback);
        let parameters = if tool.parameters.is_null() {
            Parameters::default()
        } else {
            Parameters::default().patched(&tool.parameters)?
        };
        let mut doc = Document::new(1200, 900);
        doc.renderer = registry.renderer();
        Ok(Self {
            pointer_checkpoint: None,
            atomic_depth: 0,
            journal: None,
            loaded_replay: None,
            restored_replay: None,
            replay_identity: 0,
            replay_error: None,
            replay_samples: Vec::with_capacity(256),
            replay_direct_contact: false,
            pending_save: None,
            workflow_stack: vec![],
            selected_tool: tool.id.clone(),
            tool: tool.kind,
            registry,
            doc,
            file: None,
            source_file: None,
            runtime: "headless",
            last_pointer: None,
            pen_observed: false,
            parameters,
            view: View::default(),
            revision: 0,
            ui: json!({}),
            selection_defaults: Default::default(),
            movement_defaults: Default::default(),
            settings_path: None,
            brush_presets: Default::default(),
            preset_library_token: 1,
            clipboard: None,
            system_clipboard: None,
            system_clipboard_event: None,
            transform_event: None,
            export_event: None,
            ui_effects: VecDeque::new(),
            presentation_captures: Default::default(),
            stroke: Default::default(),
            events: VecDeque::with_capacity(512),
            sequence: 0,
            native_instances: Default::default(),
            native_stack: vec![],
            read_depth: 0,
            execution_depth: 0,
            remaining_actions: 1024,
            tool_instances: Default::default(),
        })
    }
    pub fn is_drawing(&self) -> bool {
        self.stroke.drawing()
    }
    pub fn is_panning(&self) -> bool {
        self.stroke.panning()
    }
    pub(crate) fn parameter_state(&self) -> Value {
        let mut parameters = json!(self.parameters);
        parameters["spacing"] = spacing_json(self.parameters.spacing);
        parameters["minimum_opacity"] = json!(self.parameters.minimum_opacity);
        parameters["flow"] = json!(self.parameters.flow);
        parameters["opacity_mode"] = json!(self.parameters.opacity_mode);
        parameters["tip_shape"] = json!(self.parameters.tip_shape);
        parameters["tip_roundness"] = json!(self.parameters.tip_roundness);
        parameters["tip_angle"] = json!(self.parameters.tip_angle);
        parameters["symmetry"] = json!(self.parameters.symmetry);
        parameters["symmetry_x"] = json!(self.parameters.symmetry_x);
        parameters["symmetry_y"] = json!(self.parameters.symmetry_y);
        parameters
    }
    pub fn state(&self) -> Value {
        let parameters = self.parameter_state();
        json!({"document_info":self.doc.info().state(self.doc.width,self.doc.height),"revision":self.revision,"width":self.doc.width,"height":self.doc.height,"dirty":self.doc.dirty,"active_layer":self.doc.active,"selected_layer_ids":self.doc.selected_layer_ids(),"layer_links":self.doc.layer_link_sets(),"groups":self.doc.groups_state(),"raster_masks":crate::plugins::raster_masks::state(self),"layers":(0..self.doc.layers.len()).map(|index|self.layer_state(index)).collect::<Vec<_>>(),"selected_tool":self.selected_tool,"tool_kind":self.tool,"parameters":parameters,"view":self.view,"file":self.file,"ui":self.ui,"drawing":self.stroke.drawing(),"selection":{"active":self.doc.selection().current.is_some(),"can_reselect":self.doc.selection().previous.is_some()},"selection_sampling":self.selection_sampling(),"selection_defaults":self.selection_defaults,"last_transform":crate::plugins::affine::repeat_state(self),"movement_defaults":self.movement_defaults,"movement_options":self.movement_options(),"graph_enabled":self.doc.graph.enabled,"render_error":self.doc.render_error})
    }
    pub(crate) fn layer_state(&self, index: usize) -> Value {
        let l = &self.doc.layers[index];
        let mut linked_ids = self
            .doc
            .linked_layer_ids(self.doc.layer_id(index))
            .unwrap_or_default();
        if linked_ids.len() < 2 {
            linked_ids.clear();
        }
        json!({"index":index,"id":self.doc.layer_id(index),"name":self.doc.layer_name(index),
            "alpha_locked":self.doc.alpha_locked(index),"pixel_locked":self.doc.pixel_locked(index),
            "position_locked":self.doc.position_locked(index),"color_label":self.doc.color_label(index),
            "visible":l.visible,"opacity":l.opacity as f32/255.,"blend_mode":l.mode,"active":index==self.doc.active,
            "selected":self.doc.layer_selected(index),"effective_visible":self.doc.layer_visible(index),
            "parent_id":self.doc.layer_parent_id(index),"linked_layer_ids":linked_ids})
    }
    pub fn apply(&mut self, action: Action) -> ActionResult {
        let pointer_action = self
            .registry
            .actions
            .get(&action.action)
            .is_some_and(|d| d.operation == Some(Operation::Pointer));
        let recordable =
            self.registry.actions.get(&action.action).is_none_or(|d| {
                !d.is_read_only() && !d.operation.is_some_and(Operation::is_replay)
            });
        if recordable && self.read_depth == 0 && !pointer_action {
            self.replay_boundary()?;
        }
        let replay_sequence = self.journal.as_ref().map_or(0, |j| j.entries.len());
        let rejected = recordable.then(|| action.clone());
        if self.execution_depth == 0 {
            self.remaining_actions = 1024;
        }
        if self.execution_depth >= 16 || self.remaining_actions == 0 {
            return Err(ActionError::new(
                "execution_limit",
                "Pipeline depth or execution budget exceeded",
            ));
        }
        self.execution_depth += 1;
        let result = self.apply_inner(action);
        self.execution_depth -= 1;
        if let (Err(error), Some(action)) = (&result, rejected) {
            if self.read_depth == 0
                && self.replay_error.is_none()
                && self.journal.as_ref().map_or(0, |j| j.entries.len()) == replay_sequence
            {
                self.replay_commit(
                    crate::engine::replay::Source::Action {
                        action: action.action,
                        args: action.args,
                        depth: self.execution_depth,
                        context: None,
                    },
                    false,
                    Some(error.to_string()),
                )?;
            }
        }
        result
    }
    pub(crate) fn validate_action(&self, action: &Action) -> Result<(), ActionError> {
        self.plan(action, 0, &mut vec![])
    }
    fn apply_inner(&mut self, action: Action) -> ActionResult {
        if action.expected_revision.is_some_and(|r| r != self.revision) {
            return Err(ActionError::new(
                "revision_conflict",
                format!("Current revision is {}", self.revision),
            ));
        }
        let mut plan = vec![];
        self.plan(&action, 0, &mut plan)?;
        if self.read_depth != 0
            && plan
                .iter()
                .any(|(id, _, _)| !self.registry.actions[id].is_read_only())
        {
            return Err(ActionError::new(
                "read_only",
                "Read-only queries cannot execute mutations",
            ));
        }
        let mut result = json!({});
        for (index, (id, op, args)) in plan.into_iter().enumerate() {
            let event_args = args.clone();
            if op == Some(Operation::Export) {
                self.export_event = None;
            }
            if self.remaining_actions == 0 {
                return Err(ActionError::new(
                    "execution_limit",
                    "Execution budget exhausted",
                ));
            }
            self.remaining_actions -= 1;
            let read_only = self.registry.actions[&id].is_read_only();
            if op.is_none() {
                // Keep native admission before checkpoint/instance work. The
                // outer recording boundary may flush samples without ending input.
                crate::plugins::native::admit_contact(self, &id)?;
            }
            self.validate_adjustment_boundary(op, &args)?;
            self.validate_mask_boundary(op, &args)?;
            let recording = !read_only
                && !op.is_some_and(Operation::is_replay)
                && op != Some(Operation::Pointer);
            let atomic_group = matches!(op, Some(Operation::PipelineRun | Operation::WorkflowRun))
                && args["atomic"] == true;
            if self.atomic_depth != 0
                && ((!read_only
                    && !op.is_some_and(Operation::allowed_in_atomic)
                    && !self.registry.actions[&id].is_document_atomic())
                    || matches!(op, Some(Operation::ReplayLoad | Operation::ReplayOpen)))
            {
                return Err(ActionError::new("non_atomic_action",format!("{id} cannot run inside an atomic document group; file/UI/native action effects cannot be rolled back")));
            }
            if atomic_group && (self.stroke.active() || self.pointer_checkpoint.is_some()) {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before starting an atomic group",
                ));
            }
            if op.is_some_and(Operation::system_clipboard_mutation)
                && self.runtime == "desktop"
                && self.system_clipboard.is_some()
            {
                if op == Some(Operation::SystemClipboardCut) {
                    crate::plugins::PixelAccess::Pixels.require(&self.doc, self.doc.active)?;
                }
                self.finish_stroke();
            }
            if recording {
                self.replay_boundary()?;
            }
            let mut checkpoint = if recording
                && self.atomic_depth == 0
                && !self.stroke.active()
                && (atomic_group
                    || self.registry.actions[&id].is_document_atomic()
                    || op == Some(Operation::New)
                    || op.is_some_and(Operation::system_clipboard_mutation))
            {
                Some(super::atomic::Checkpoint::capture(self))
            } else {
                None
            };
            let scoped_atomic = atomic_group || (checkpoint.is_some() && op.is_none());
            let native_revision_start = (scoped_atomic && op.is_none()).then_some(self.revision);
            if read_only {
                self.read_depth += 1;
            }
            if scoped_atomic {
                self.atomic_depth += 1;
            }
            let outcome = match op {
                Some(op) => self.execute(op, args),
                None => crate::plugins::native::execute(self, &id, args),
            };
            if scoped_atomic {
                self.atomic_depth -= 1;
            }
            if read_only {
                self.read_depth -= 1;
            }
            match outcome {
                Ok(mut value) => {
                    let failed = value.get("ok") == Some(&Value::Bool(false));
                    if failed {
                        if let Some(saved) = checkpoint.take() {
                            saved.restore(self);
                        }
                    }
                    if atomic_group {
                        value["transactional"] = json!(true);
                        value["rolled_back"] = json!(failed && self.atomic_depth == 0);
                        value["revision"] = json!(self.revision);
                        value["committed"] = if failed {
                            json!([])
                        } else {
                            value
                                .get("completed")
                                .or_else(|| value.get("completed_nodes"))
                                .cloned()
                                .unwrap_or_else(|| json!([]))
                        };
                    }
                    if !failed {
                        if let Some(revision) = native_revision_start {
                            // Atomic native actions own one outer revision. Host
                            // loans/child calls may advance it provisionally.
                            self.revision = revision;
                        }
                    }
                    if !read_only
                        && !self.doc.history_group_active()
                        && !(failed && self.registry.actions[&id].is_document_atomic())
                        && !matches!(
                            op,
                            Some(
                                Operation::PipelineRun
                                    | Operation::WorkflowRun
                                    | Operation::Pointer
                                    | Operation::CaptureWorkspace
                                    | Operation::WindowCapture
                                    | Operation::TransformPreview
                                    | Operation::LiquifyDialog
                                    | Operation::HslDialog
                                    | Operation::TonalDialog
                                    | Operation::GrayscaleDialog
                                    | Operation::ThresholdDialog
                                    | Operation::PosterizeDialog
                                    | Operation::AdjustmentInspect
                                    | Operation::ExportDialog
                                    | Operation::OpenSelectedMerge
                                    | Operation::ReplayExport
                                    | Operation::ReplayFrames
                                    | Operation::ReplayRecover
                            )
                        )
                    {
                        self.revision += 1;
                    }
                    // Preserve a child's partial report when recording has already
                    // stopped; a second append cannot record or undo its effects.
                    if recording && !(failed && self.replay_error.is_some()) {
                        if let Err(error) = self.replay_action(
                            &id,
                            &event_args,
                            op,
                            !failed,
                            failed.then(|| value["error"].to_string()),
                            Some(&value),
                        ) {
                            if let Some(saved) = checkpoint.take() {
                                saved.restore(self);
                            }
                            if error.code == "render_error" {
                                self.replay_action(
                                    &id,
                                    &event_args,
                                    op,
                                    false,
                                    Some(error.to_string()),
                                    None,
                                )?;
                                self.record(EventKind::Action {
                                    action: id.clone(),
                                    args: event_args.clone(),
                                    success: false,
                                    error: Some(error.to_string()),
                                });
                            }
                            if op == Some(Operation::Export) && !failed {
                                return Err(ActionError::new("export_recording",format!(
                                    "PNG was written to {} (SHA-256 {}), but recording failed: {}",
                                    value["path"].as_str().unwrap_or_default(),value["sha256"].as_str().unwrap_or_default(),error)));
                            }
                            return Err(error);
                        }
                    }
                    if !read_only {
                        self.record(EventKind::Action {
                            action: id,
                            args: event_args,
                            success: !failed,
                            error: failed.then(|| value["error"].to_string()),
                        });
                    }
                    if op.is_some_and(Operation::publishes_session_result_revision) {
                        if let Some(revision) = value.get_mut("revision") {
                            *revision = json!(self.revision);
                        }
                    }
                    result = value;
                    if failed {
                        break;
                    }
                }
                Err(e) => {
                    if let Some(saved) = checkpoint.take() {
                        saved.restore(self);
                    }
                    if recording {
                        self.replay_action(&id, &event_args, op, false, Some(e.to_string()), None)?;
                    }
                    self.record(EventKind::Action {
                        action: id,
                        args: event_args,
                        success: false,
                        error: Some(e.message.clone()),
                    });
                    return Err(ActionError::new(
                        &e.code,
                        format!(
                            "{} ({index} steps completed; sequential actions do not roll back previous steps)",
                            e.message
                        ),
                    ));
                }
            }
        }
        Ok(json!({"revision":self.revision,"result":result}))
    }
    fn plan(
        &self,
        action: &Action,
        depth: usize,
        out: &mut Vec<(String, Option<Operation>, Value)>,
    ) -> Result<(), ActionError> {
        if depth > 8 || out.len() >= 256 {
            return Err(ActionError::new(
                "action_limit",
                "Macro action cycle or execution budget exceeded",
            ));
        }
        let definition = self
            .registry
            .actions
            .get(&action.action)
            .ok_or_else(|| ActionError::new("unknown_action", &action.action))?;
        validate(&definition.schema(), &action.args)?;
        if let Some(op) = definition.operation {
            validate(&op.schema(), &action.args)?;
            if op.is_brush_preset() {
                crate::plugins::brush_presets::validate_literal(op, &action.args)?;
            }
            if matches!(op, Operation::ArrangeLayers | Operation::ArrangeLayersPlan) {
                crate::plugins::layer_arrangement::validate_literal(&action.args)?;
            }
            if op.is_adjustment() {
                crate::plugins::adjustments::validate_literal(&self.registry, op, &action.args)?;
            }
            if matches!(op, Operation::MovePixels | Operation::Nudge) {
                crate::plugins::movement::validate_literal(&action.args)?;
            }
            if matches!(
                op,
                Operation::TransformPixels | Operation::TransformPlan | Operation::TransformPreview
            ) {
                let args = if op == Operation::TransformPreview {
                    &action.args["parameters"]
                } else {
                    &action.args
                };
                crate::plugins::owner_affine::validate_literal(args)?;
            }
            if op.is_mask() {
                crate::plugins::raster_masks::validate(op, &action.args)?;
            }
            if matches!(op, Operation::New | Operation::NewPlan) {
                // Unit-dependent bounds are not expressible by the shared schema
                // subset. Reject literal creation errors before earlier steps run.
                super::creation::plan(&action.args)?;
            }
            if matches!(op, Operation::ImageSize | Operation::ImageSizePlan) {
                super::image_size::validate(&action.args)?;
            }
            if op == Operation::CanvasGeometryPlan {
                super::canvas_geometry::validate(&action.args)?;
            }
            if op == Operation::HistogramQuery {
                crate::plugins::histogram::validate_literal(&action.args)?;
            }
            if op == Operation::LayerQuery {
                super::layer_query::validate(&action.args)?;
            }
            if op == Operation::LayerBatch {
                crate::plugins::layer_batch::validate(&action.args)?;
            }
            if op == Operation::LayerLinks {
                crate::plugins::layer_links::validate(&action.args)?;
            }
            if matches!(
                op,
                Operation::MergeSelectedLayers
                    | Operation::MergeSelectedLayersPlan
                    | Operation::OpenSelectedMerge
            ) {
                crate::plugins::selected_merge::validate(&action.args)?;
            }
            if matches!(
                op,
                Operation::GroupQuery
                    | Operation::GroupCreate
                    | Operation::GroupCreateSelected
                    | Operation::GroupProperties
                    | Operation::GroupMove
                    | Operation::GroupUngroup
                    | Operation::GroupUngroupParent
                    | Operation::GroupDelete
                    | Operation::GroupDuplicate
                    | Operation::GroupMerge
                    | Operation::GroupSelectLayers
            ) {
                crate::plugins::groups::validate(op, &action.args)?;
            }
            out.push((action.action.clone(), Some(op), action.args.clone()));
        } else if definition.native.is_some() {
            out.push((action.action.clone(), None, action.args.clone()));
        } else {
            for step in &definition.steps {
                let mut next = step.clone();
                next.args = expand(&step.args, &action.args)?;
                self.plan(&next, depth + 1, out)?;
            }
        }
        Ok(())
    }
    fn execute(&mut self, op: Operation, args: Value) -> ActionResult {
        crate::plugins::execute(self, op, args)
    }
    pub fn pointer(&mut self, sample: Pointer) -> Result<(), ActionError> {
        self.pointer_input(
            sample,
            self.execution_depth == 0 || self.replay_direct_contact,
        )
    }
    pub(crate) fn pointer_action(&mut self, sample: Pointer) -> Result<(), ActionError> {
        self.pointer_input(sample, true)
    }
    fn pointer_input(&mut self, sample: Pointer, recording: bool) -> Result<(), ActionError> {
        if self.read_depth != 0 {
            return Err(ActionError::new(
                "read_only",
                "Read-only queries cannot paint",
            ));
        }
        if let Err(error) = sample.validate() {
            if self.execution_depth == 0 {
                self.replay_boundary()?;
                self.replay_commit(
                    crate::engine::replay::Source::Rejected {
                        input: format!("Pointer: {sample:?}"),
                    },
                    false,
                    Some(error.to_string()),
                )?;
            }
            return Err(error);
        }
        if recording {
            self.replay_ensure()?;
            if sample.phase == PointerPhase::Down {
                if !self.stroke.active() {
                    self.replay_boundary()?;
                    self.pointer_checkpoint = Some((
                        super::atomic::Checkpoint::capture(self),
                        self.journal.as_ref().unwrap().checkpoint(),
                    ));
                }
                self.replay_direct_contact = true;
            }
        }
        let mut stroke = std::mem::take(&mut self.stroke);
        let result = stroke.input(self, sample);
        self.stroke = stroke;
        if result.is_err() && recording {
            if let Some((saved, _)) = &self.pointer_checkpoint {
                let saved = saved.clone();
                let samples = std::mem::take(&mut self.replay_samples);
                saved.restore(self);
                self.replay_samples = samples;
            }
        }
        if result.is_ok() {
            self.last_pointer = Some(sample);
            self.pen_observed |= sample.pen;
            self.revision += 1;
            self.record(EventKind::Pointer { sample });
        }
        if recording {
            self.replay_samples.push(sample);
            let ended = matches!(sample.phase, PointerPhase::Up | PointerPhase::Cancel)
                || result.is_err()
                || (sample.phase == PointerPhase::Down && !self.stroke.active());
            if ended || self.replay_samples.len() >= 256 {
                self.replay_flush(ended, result.as_ref().err().map(ToString::to_string))?;
            }
            if ended {
                self.replay_direct_contact = false;
                self.pointer_checkpoint = None;
            }
        }
        result
    }
    #[cfg(test)]
    pub(crate) fn pointer_reference(
        &mut self,
        backend: &mut crate::plugins::tool_reference::StrokeEngine,
        sample: Pointer,
    ) -> Result<(), ActionError> {
        sample.validate()?;
        let result = backend.input(self, sample);
        if result.is_ok() {
            self.revision += 1;
            self.record(EventKind::Pointer { sample });
        }
        result
    }
    pub fn finish_stroke(&mut self) {
        if let Some(last) = self.stroke.last_sample() {
            let _ = self.pointer(Pointer {
                phase: PointerPhase::Up,
                ..last
            });
        }
    }
    pub fn pick(&mut self, x: f32, y: f32) -> Result<(), ActionError> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.
            || y < 0.
            || x >= self.doc.width as f32
            || y >= self.doc.height as f32
        {
            return Err(ActionError::new(
                "outside_canvas",
                "Eyedropper coordinates are outside the canvas",
            ));
        }
        self.doc.compose();
        let c = self.doc.composite[y as usize * self.doc.width + x as usize];
        if c >> 24 != 0 {
            self.parameters.color = format!("#{:06X}", c & 0xffffff);
        }
        Ok(())
    }
    fn record(&mut self, event: EventKind) {
        self.sequence += 1;
        if self.events.len() == 512 {
            self.events.pop_front();
        }
        self.events.push_back(Event {
            sequence: self.sequence,
            revision: self.revision,
            event,
        });
    }
}
fn expand(template: &Value, args: &Value) -> Result<Value, ActionError> {
    if let Some(obj) = template.as_object() {
        if obj.len() == 1 {
            if let Some(key) = obj.get("$arg").and_then(Value::as_str) {
                return args.get(key).cloned().ok_or_else(|| {
                    ActionError::new(
                        "invalid_arguments",
                        format!("Macro action is missing argument {key}"),
                    )
                });
            }
        }
        let mut out = serde_json::Map::new();
        for (k, v) in obj {
            out.insert(k.clone(), expand(v, args)?);
        }
        return Ok(Value::Object(out));
    }
    if let Some(items) = template.as_array() {
        return items
            .iter()
            .map(|v| expand(v, args))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    Ok(template.clone())
}
