use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub action: String,
    #[serde(default = "empty_object")]
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
}
pub fn empty_object() -> Value {
    json!({})
}
impl Action {
    pub fn new(id: impl Into<String>, args: Value) -> Self {
        Self {
            action: id.into(),
            args,
            expected_revision: None,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionError {
    pub code: String,
    pub message: String,
}
impl ActionError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}
impl std::fmt::Display for ActionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ActionError {}
impl From<std::io::Error> for ActionError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e.to_string())
    }
}
impl From<serde_json::Error> for ActionError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("invalid_arguments", e.to_string())
    }
}
pub type ActionResult = Result<Value, ActionError>;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    RotateCanvasClockwise,
    RotateCanvasCounterclockwise,
    RotateCanvasHalf,
    FlipCanvasHorizontal,
    FlipCanvasVertical,
    RotateLayerClockwise,
    RotateLayerCounterclockwise,
    SystemClipboardCopy,
    SystemClipboardCopyMerged,
    SystemClipboardCut,
    SystemClipboardPaste,
    SystemClipboardPasteInPlace,
    SystemClipboardState,
    ClipboardCopy,
    ClipboardCopyMerged,
    ClipboardCut,
    ClipboardPaste,
    ClipboardPasteInPlace,
    ClipboardState,
    ReplayStatus,
    ReplayOpen,
    ReplayEvents,
    ReplayVerify,
    ReplayExport,
    ReplayLoad,
    ReplaySeek,
    ReplaySnapshot,
    ReplayFrames,
    ReplayRecover,
    NodeCatalog,
    WorkflowRead,
    WorkflowSet,
    WorkflowPatch,
    WorkflowValidate,
    WorkflowRun,
    WorkflowState,
    GraphRead,
    HistogramQuery,
    HslPreview,
    TonalPreview,
    HslDialog,
    TonalDialog,
    GrayscalePreview,
    GrayscaleDialog,
    ThresholdPreview,
    PosterizePreview,
    ThresholdDialog,
    PosterizeDialog,
    GraphSet,
    GraphCatalog,
    Describe,
    ContextRead,
    PipelineRun,
    CaptureWorkspace,
    SettingsRead,
    SettingsWrite,
    State,
    New,
    NewPlan,
    DocumentInfo,
    DocumentResolution,
    CanvasSize,
    MovePixels,
    TransformPixels,
    TransformPreview,
    TransformPlan,
    TransformRepeat,
    TransformRepeatCopy,
    TransformState,
    Nudge,
    CanvasTrim,
    TrimBounds,
    ImageSize,
    ImageSizePlan,
    CanvasGeometryPlan,
    Crop,
    Open,
    Save,
    Export,
    ExportPlan,
    ExportPreview,
    ExportDialog,
    Render,
    SelectTool,
    Parameters,
    BrushAngleAdjust,
    BrushPresetsQuery,
    BrushPresetUpsert,
    BrushPresetDelete,
    BrushPresetApply,
    BrushPresetsImport,
    BrushPresetsExport,
    BrushPresetsReload,
    Pointer,
    Stroke,
    Undo,
    Redo,
    AddLayer,
    RenameLayer,
    DuplicateLayer,
    LayerViaCopy,
    LayerViaCut,
    MoveLayer,
    DeleteLayer,
    MergeDown,
    MergeLayers,
    MergeSelectedLayers,
    MergeSelectedLayersPlan,
    OpenSelectedMerge,
    MergeVisible,
    FlattenImage,
    StampVisible,
    SelectLayer,
    LayerProperties,
    LayerQuery,
    ArrangeLayers,
    ArrangeLayersPlan,
    LayerLinks,
    LayerLinksQuery,
    MaskQuery,
    MaskCreate,
    MaskDelete,
    MaskProperties,
    MaskTarget,
    MaskFill,
    MaskInvert,
    MaskFromSelection,
    MaskToSelection,
    MaskSnapshot,
    MaskApply,
    LayerSelection,
    LayerBatch,
    GroupQuery,
    AdjustmentCatalog,
    AdjustmentQuery,
    AdjustmentRead,
    AdjustmentCreate,
    AdjustmentSet,
    AdjustmentMove,
    AdjustmentDelete,
    AdjustmentInspect,
    GroupCreate,
    GroupCreateSelected,
    GroupProperties,
    GroupMove,
    GroupUngroup,
    GroupUngroupParent,
    GroupDelete,
    GroupDuplicate,
    GroupMerge,
    GroupSelectLayers,
    ClearLayer,
    SelectionShape,
    SelectAll,
    Deselect,
    Reselect,
    InvertSelection,
    ReadSelection,
    ExpandSelection,
    ContractSelection,
    FeatherSelection,
    SmoothSelection,
    SelectColor,
    GrowSelection,
    SimilarSelection,
    Pick,
    View,
    MirrorView,
    MirrorViewVertical,
    Ui,
    WindowCommand,
    WindowState,
    WindowLayout,
    WindowCapture,
    WindowDrop,
    Plugins,
    PluginPlan,
    Events,
}
impl Operation {
    /// These handlers return a Session-owned state revision assembled before
    /// publication. Opaque provider/native results and nested reports have their
    /// own contracts; never infer ownership from a JSON field name alone.
    pub(crate) fn publishes_session_result_revision(self) -> bool {
        matches!(
            self,
            Self::State
                | Self::BrushAngleAdjust
                | Self::BrushPresetApply
                | Self::AdjustmentCreate
                | Self::AdjustmentSet
                | Self::AdjustmentMove
                | Self::AdjustmentDelete
                | Self::New
                | Self::Open
                | Self::CanvasSize
                | Self::Crop
                | Self::Undo
                | Self::Redo
                | Self::RotateCanvasClockwise
                | Self::RotateCanvasCounterclockwise
                | Self::RotateCanvasHalf
                | Self::FlipCanvasHorizontal
                | Self::FlipCanvasVertical
                | Self::RotateLayerClockwise
                | Self::RotateLayerCounterclockwise
                | Self::MovePixels
                | Self::Nudge
                | Self::TransformPixels
                | Self::TransformRepeat
                | Self::TransformRepeatCopy
                | Self::CanvasTrim
                | Self::ImageSize
                | Self::LayerViaCopy
                | Self::LayerViaCut
                | Self::RenameLayer
                | Self::DuplicateLayer
                | Self::MoveLayer
                | Self::DeleteLayer
                | Self::SelectLayer
                | Self::LayerProperties
                | Self::ArrangeLayers
                | Self::LayerLinks
                | Self::LayerSelection
                | Self::LayerBatch
                | Self::MergeDown
                | Self::MergeLayers
                | Self::MergeSelectedLayers
                | Self::MergeVisible
                | Self::FlattenImage
                | Self::StampVisible
                | Self::ClearLayer
                | Self::GroupCreate
                | Self::GroupCreateSelected
                | Self::GroupProperties
                | Self::GroupMove
                | Self::GroupUngroup
                | Self::GroupUngroupParent
                | Self::GroupDelete
                | Self::GroupDuplicate
                | Self::GroupMerge
                | Self::GroupSelectLayers
                | Self::SelectionShape
                | Self::SelectAll
                | Self::Deselect
                | Self::Reselect
                | Self::InvertSelection
                | Self::ExpandSelection
                | Self::ContractSelection
                | Self::FeatherSelection
                | Self::SmoothSelection
                | Self::SelectColor
                | Self::GrowSelection
                | Self::SimilarSelection
                | Self::Export
        )
    }
    #[cfg(any(feature = "desktop", test))]
    pub(crate) fn is_contact_guarded_command(self) -> bool {
        self.is_canvas_geometry_command()
            || matches!(
                self,
                Self::DuplicateLayer
                    | Self::MirrorView
                    | Self::MirrorViewVertical
                    | Self::HslDialog
                    | Self::TonalDialog
                    | Self::GrayscaleDialog
                    | Self::ThresholdDialog
                    | Self::PosterizeDialog
                    | Self::BrushAngleAdjust
                    | Self::BrushPresetUpsert
                    | Self::BrushPresetDelete
                    | Self::BrushPresetApply
                    | Self::BrushPresetsImport
                    | Self::BrushPresetsExport
                    | Self::BrushPresetsReload
                    | Self::AdjustmentCreate
                    | Self::AdjustmentSet
                    | Self::AdjustmentMove
                    | Self::AdjustmentDelete
                    | Self::AdjustmentInspect
                    | Self::ArrangeLayers
                    | Self::DeleteLayer
                    | Self::LayerLinks
                    | Self::LayerBatch
                    | Self::GroupCreate
                    | Self::GroupCreateSelected
                    | Self::GroupMove
                    | Self::GroupDelete
                    | Self::GroupDuplicate
            )
    }
    /// Presentation adapters preserve active contacts until these commands run
    /// through their shared admission checks; opening a form is not admission.
    #[cfg(any(feature = "desktop", test))]
    pub(crate) fn is_canvas_geometry_command(self) -> bool {
        matches!(
            self,
            Self::CanvasSize
                | Self::Crop
                | Self::ImageSize
                | Self::ImageSizePlan
                | Self::CanvasGeometryPlan
                | Self::CanvasTrim
                | Self::TrimBounds
                | Self::RotateCanvasClockwise
                | Self::RotateCanvasCounterclockwise
                | Self::RotateCanvasHalf
                | Self::FlipCanvasHorizontal
                | Self::FlipCanvasVertical
                | Self::RotateLayerClockwise
                | Self::RotateLayerCounterclockwise
                | Self::DocumentResolution
        )
    }
    pub fn is_replay(self) -> bool {
        matches!(
            self,
            Self::ReplayStatus
                | Self::ReplayOpen
                | Self::ReplayEvents
                | Self::ReplayVerify
                | Self::ReplayExport
                | Self::ReplayLoad
                | Self::ReplaySeek
                | Self::ReplaySnapshot
                | Self::ReplayFrames
                | Self::ReplayRecover
        )
    }
    pub fn schema(self) -> Value {
        if matches!(self, Self::MirrorView | Self::MirrorViewVertical) {
            return json!({"type":"object","additionalProperties":false});
        }
        use crate::engine::quantization::Effect;
        match self {
            Self::ThresholdPreview => {
                return crate::plugins::quantization_preview::schema(Effect::Threshold, false)
            }
            Self::PosterizePreview => {
                return crate::plugins::quantization_preview::schema(Effect::Posterize, false)
            }
            Self::ThresholdDialog => {
                return crate::plugins::quantization_preview::schema(Effect::Threshold, true)
            }
            Self::PosterizeDialog => {
                return crate::plugins::quantization_preview::schema(Effect::Posterize, true)
            }
            _ => {}
        }

        if self == Self::GrayscalePreview {
            return crate::plugins::grayscale_preview::schema();
        }
        if self == Self::GrayscaleDialog {
            let mut parameters = crate::plugins::grayscale_preview::Parameters::schema();
            parameters.as_object_mut().unwrap().remove("required");
            return json!({"type":"object","additionalProperties":false,"properties":{"parameters":parameters}});
        }
        if self == Self::TonalDialog {
            let mut parameters = crate::plugins::tonal_preview::Parameters::schema();
            parameters.as_object_mut().unwrap().remove("required");
            return json!({"type":"object","additionalProperties":false,"properties":{"parameters":parameters}});
        }
        if self == Self::TonalPreview {
            return crate::plugins::tonal_preview::schema();
        }
        if self == Self::HslDialog {
            let mut parameters = crate::engine::hsl::Hsl::schema();
            parameters.as_object_mut().unwrap().remove("required");
            return json!({"type":"object","additionalProperties":false,"properties":{"parameters":parameters}});
        }
        if self == Self::HslPreview {
            return crate::plugins::hsl_preview::schema();
        }
        if self == Self::HistogramQuery {
            return crate::plugins::histogram::schema();
        }
        if self == Self::BrushAngleAdjust {
            return crate::plugins::brush_tip::schema();
        }
        if self.is_brush_preset() {
            return crate::plugins::brush_presets::schema(self);
        }
        if matches!(self, Self::ArrangeLayers | Self::ArrangeLayersPlan) {
            return crate::plugins::layer_arrangement::schema(self);
        }
        if self.is_adjustment() {
            return crate::plugins::adjustments::schema(self);
        }
        if self.is_mask() {
            return crate::plugins::raster_masks::schema(self);
        }
        if matches!(
            self,
            Self::MergeLayers
                | Self::MergeSelectedLayers
                | Self::MergeSelectedLayersPlan
                | Self::OpenSelectedMerge
        ) {
            return crate::plugins::selected_merge::schema(self);
        }
        if self == Self::MergeVisible {
            return json!({"type":"object","properties":{},"additionalProperties":false,
                "description":"Bake visible artwork. In grouped documents, consume effectively visible rasters and retain the top visible raster identity as a Normal/full-opacity root result, preserving all hidden rasters, filter branches and group records. Later unhiding can interact differently with the baked root. Requires idle input for grouped documents."});
        }
        if self == Self::AddLayer {
            return json!({"type":"object","properties":{
                "parent_id":{"type":"integer","minimum":0,"maximum":4294967295u64,"description":"Parent group ID; zero means document root. When omitted, insert beside the active raster layer."}
            },"additionalProperties":false});
        }
        if matches!(
            self,
            Self::GroupQuery
                | Self::GroupCreate
                | Self::GroupCreateSelected
                | Self::GroupProperties
                | Self::GroupMove
                | Self::GroupUngroup
                | Self::GroupUngroupParent
                | Self::GroupDelete
                | Self::GroupDuplicate
                | Self::GroupMerge
                | Self::GroupSelectLayers
        ) {
            return crate::plugins::groups::schema(self);
        }
        if self == Self::LayerQuery {
            return super::layer_query::schema();
        }
        if self == Self::LayerBatch {
            return crate::plugins::layer_batch::schema();
        }
        if self == Self::LayerSelection {
            return json!({"type":"object","properties":{
                "layer_ids":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"integer","minimum":1,"maximum":4294967295u64},"description":"Unique stable layer IDs. The result is normalized to stack order."},
                "active_layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Must be selected. If absent, retain the current active member or use the last supplied ID."}
            },"required":["layer_ids"],"additionalProperties":false});
        }
        if matches!(self, Self::ImageSize | Self::ImageSizePlan) {
            return super::image_size::schema();
        }
        if self == Self::CanvasGeometryPlan {
            return super::canvas_geometry::schema();
        }
        if matches!(self, Self::New | Self::NewPlan) {
            return super::creation::schema();
        }
        if matches!(
            self,
            Self::Export | Self::ExportPlan | Self::ExportPreview | Self::Render
        ) {
            return super::output_size::schema(self == Self::Export);
        }
        let number = json!({"type":"number"});
        let point = json!({"type":"object","properties":{"x":number,"y":number,"pressure":{"type":"number","minimum":0,"maximum":1},"time":{"type":"integer","minimum":0},"pen":{"type":"boolean"},"eraser":{"type":"boolean"},"tool_override":{"type":"string","enum":["brush","eraser","picker","hand","zoom"]},"view_x":number,"view_y":number},"required":["x","y"],"additionalProperties":false});
        let (props, required) = match self {
            Self::ClipboardPaste | Self::ClipboardPasteInPlace => (
                json!({"x":{"type":"integer","minimum":-4096,"maximum":4096},"y":{"type":"integer","minimum":-4096,"maximum":4096}}),
                vec![],
            ),
            Self::ExpandSelection
            | Self::ContractSelection
            | Self::FeatherSelection
            | Self::SmoothSelection => (
                json!({"radius":{"type":"integer","minimum":1,"maximum":256,"default":4},"edge":{"type":"string","enum":["extend","zero"],"default":"extend"}}),
                vec!["radius"],
            ),
            Self::SelectColor => (
                json!({"x":{"type":"integer","minimum":0,"maximum":4095},"y":{"type":"integer","minimum":0,"maximum":4095},"tolerance":{"type":"integer","minimum":0,"maximum":255,"default":32},"contiguous":{"type":"boolean","default":true},"sample_merged":{"type":"boolean","default":false},"mode":{"type":"string","enum":["replace","add","subtract","intersect"],"default":"replace"}}),
                vec!["x", "y"],
            ),
            Self::GrowSelection | Self::SimilarSelection => (
                json!({"tolerance":{"type":"integer","minimum":0,"maximum":255,"default":32},"sample_merged":{"type":"boolean","default":false}}),
                vec![],
            ),
            Self::SelectionShape => (
                json!({"kind":{"type":"string","enum":["rectangle","ellipse","polygon"]},"mode":{"type":"string","enum":["replace","add","subtract","intersect"]},"x":{"type":"number","minimum":-16384,"maximum":16384},"y":{"type":"number","minimum":-16384,"maximum":16384},"width":{"type":"number","minimum":0,"maximum":16384},"height":{"type":"number","minimum":0,"maximum":16384},"points":{"type":"array","minItems":3,"maxItems":4096,"items":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number","minimum":-16384,"maximum":16384}}}}),
                vec!["kind"],
            ),
            Self::ReplayStatus | Self::ReplayOpen => (json!({}), vec![]),
            Self::ReplayEvents => (
                json!({"source":{"type":"string","enum":["current","loaded"]},"start":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":256}}),
                vec![],
            ),
            Self::ReplayVerify => (
                json!({"source":{"type":"string","enum":["current","loaded"]},"expected_head":{"type":"string","minLength":64}}),
                vec![],
            ),
            Self::ReplaySeek | Self::ReplaySnapshot => (
                json!({"source":{"type":"string","enum":["current","loaded"]},"event":{"type":"integer","minimum":0}}),
                vec![],
            ),
            Self::ReplayExport | Self::ReplayLoad | Self::ReplayRecover => (
                json!({"source":{"type":"string","enum":["current","loaded"]},"path":{"type":"string","minLength":1}}),
                vec!["path"],
            ),
            Self::ReplayFrames => (
                json!({"source":{"type":"string","enum":["current","loaded"]},"path":{"type":"string","minLength":1},"max_frames":{"type":"integer","minimum":1,"maximum":1000},"fps":{"type":"integer","minimum":1,"maximum":60}}),
                vec!["path"],
            ),
            Self::WorkflowSet => (
                json!({"graph":crate::engine::workflow::schema()}),
                vec!["graph"],
            ),
            Self::WorkflowPatch => (
                json!({"operations":{"type":"array","maxItems":256,"items":{"type":"object"}}}),
                vec!["operations"],
            ),
            Self::WorkflowValidate | Self::WorkflowRun => (
                json!({"graph":crate::engine::workflow::schema(),"dry_run":{"type":"boolean"},"atomic":{"type":"boolean"},"expected_revision":{"type":"integer","minimum":0}}),
                vec![],
            ),
            Self::WorkflowState => (json!({"resource":{"type":"string"}}), vec!["resource"]),
            Self::GraphSet => (
                json!({"graph":crate::engine::graph::schema()}),
                vec!["graph"],
            ),
            Self::ContextRead => (
                json!({"resources":{"type":"array","maxItems":128,"items":{"type":"string"}}}),
                vec![],
            ),
            Self::PipelineRun => (
                json!({"steps":{"type":"array","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"id":{"type":"string"},"action":{"type":"string"},"args":{},"output_schema":{"type":"object"}},"required":["id","action"],"additionalProperties":false}},"dry_run":{"type":"boolean"},"atomic":{"type":"boolean"},"expected_revision":{"type":"integer","minimum":0}}),
                vec!["steps"],
            ),
            Self::DocumentResolution => (
                json!({"resolution_ppi":{"type":"integer","minimum":1,"maximum":2400,"description":"Square pixels per inch; replaces imported PNG density without resampling. Writes physical density even for an explicit 72 PPI."}}),
                vec!["resolution_ppi"],
            ),
            Self::CanvasTrim | Self::TrimBounds => (
                json!({"mode":{"type":"string","enum":["transparent","top_left","bottom_right"],"default":"transparent","description":"Analyze the visible merged image; corner modes match its corner RGBA."},"tolerance":{"type":"integer","minimum":0,"maximum":255,"default":0,"description":"Removable alpha threshold, or maximum RGBA channel difference for corner modes."},"top":{"type":"boolean","default":true},"right":{"type":"boolean","default":true},"bottom":{"type":"boolean","default":true},"left":{"type":"boolean","default":true,"description":"At least one of top/right/bottom/left must be true."}}),
                vec![],
            ),
            Self::MovePixels => (
                json!({"dx":{"type":"integer","minimum":-4096,"maximum":4096},"dy":{"type":"integer","minimum":-4096,"maximum":4096},"target":{"type":"string","enum":["auto","layer","selection","selection_mask"],"description":"Defaults to movement preferences. Auto first moves the selected raster painting mask, otherwise selected pixels or the active layer. Explicit layer moves its linked mask too. Masked selected-pixel and group-mask movement remain unsupported."},"mask_id":{"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Move this whole raw raster-owner mask independently of painting target, active layer and pixel selection. Use only with an omitted or auto target; overrides stored target preferences. Uncovered mask samples reveal white."}}),
                vec!["dx", "dy"],
            ),
            Self::TransformPixels => (
                json!({"target":{"type":"string","enum":["auto","layer","selection","selection_mask"]},"quad":{"type":"array","minItems":4,"maxItems":4,"items":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number","minimum":-8192,"maximum":8192}},"description":"Destination corners in source TL,TR,BR,BL order; cannot combine with affine controls. Must be finite and convex."},"translate_x":{"type":"number","minimum":-8192,"maximum":8192,"default":0},"translate_y":{"type":"number","minimum":-8192,"maximum":8192,"default":0},"scale_x":{"type":"number","minimum":0.01,"maximum":100,"default":1},"scale_y":{"type":"number","minimum":0.01,"maximum":100,"default":1},"angle":{"type":"number","minimum":-360,"maximum":360,"default":0,"description":"Clockwise degrees in document coordinates."},"skew_x":{"type":"number","minimum":-80,"maximum":80,"default":0,"description":"Local horizontal shear angle, after scale/flip and before vertical shear and rotation."},"skew_y":{"type":"number","minimum":-80,"maximum":80,"default":0,"description":"Local vertical shear angle. Combined coefficients must fit native affine limits."},"flip_x":{"type":"boolean","default":false},"flip_y":{"type":"boolean","default":false},"pivot_x":{"type":"number","minimum":-8192,"maximum":8192,"description":"Document-edge pivot; supply both coordinates or neither (source bounds center)."},"pivot_y":{"type":"number","minimum":-8192,"maximum":8192},"method":{"type":"string","enum":["nearest","bilinear"],"default":"bilinear"}}),
                vec![],
            ),
            Self::ExportDialog => (
                json!({"parameters":super::output_size::schema(false)}),
                vec![],
            ),
            Self::TransformPreview => (
                json!({"parameters":Self::TransformPixels.schema()}),
                vec!["parameters"],
            ),
            Self::TransformRepeat => (
                json!({"duplicate":{"type":"boolean","default":false,"description":"Insert a transformed copy above the source; selected targets copy selected raw pixels only. Outline-only recipes cannot duplicate."}}),
                vec![],
            ),
            Self::TransformPlan => return Self::TransformPixels.schema(),
            Self::Nudge => (
                json!({"direction":{"type":"string","enum":["left","right","up","down"]},"large":{"type":"boolean","default":false},"target":{"type":"string","enum":["auto","layer","selection","selection_mask"],"description":"Same domain rules as canvas.move; offsets use the configured movement steps."},"mask_id":{"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Nudge this whole raw raster-owner mask. Requires an omitted or auto target and overrides stored target preferences."}}),
                vec!["direction"],
            ),
            Self::CanvasSize => (
                json!({"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096},"anchor":{"type":"string","enum":["top_left","top","top_right","left","center","right","bottom_left","bottom","bottom_right"]}}),
                vec!["width", "height"],
            ),
            Self::Crop => (
                json!({"x":{"type":"integer","minimum":0,"maximum":4095},"y":{"type":"integer","minimum":0,"maximum":4095},"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096}}),
                vec!["x", "y", "width", "height"],
            ),
            Self::CaptureWorkspace => (
                json!({"path":{"type":"string","minLength":1,"description":"Capture destination, at most 32768 UTF-8 bytes"}}),
                vec!["path"],
            ),
            Self::Open | Self::Save | Self::SettingsRead => (
                json!({"path":{"type":"string","minLength":1}}),
                vec!["path"],
            ),
            Self::SettingsWrite => (
                json!({"path":{"type":"string","minLength":1},"ui":{"type":"object"},"keybindings":{"type":"object"},"painting":{"type":"object"},"brush_presets":{"type":"object"},"selection":{"type":"object"},"movement":{"type":"object"},"new_canvas":{"type":"object"},"plugins":{"type":"object"},"document":{"type":"object"}}),
                vec!["path"],
            ),
            Self::SelectTool => (json!({"tool":{"type":"string"}}), vec!["tool"]),
            Self::Parameters => (
                json!({"symmetry":{"type":"string","enum":["off","vertical","horizontal","both"],"default":"off","title":"Brush Symmetry"},"symmetry_x":{"type":"number","minimum":0,"maximum":1,"default":0.5,"title":"Symmetry X Anchor","description":"Vertical axis position as a fraction of canvas width."},"symmetry_y":{"type":"number","minimum":0,"maximum":1,"default":0.5,"title":"Symmetry Y Anchor","description":"Horizontal axis position as a fraction of canvas height."},"radius":{"type":"number","minimum":0.5,"maximum":200},"tip_shape":{"type":"string","enum":["round","square"],"default":"round"},"tip_roundness":{"type":"number","minimum":0.25,"maximum":1,"default":1,"description":"Short/long-axis ratio; nondefault tips use minor-diameter spacing."},"tip_angle":{"type":"number","minimum":-180,"maximum":180,"default":0,"description":"Fixed clockwise document-space degrees; not tilt or stroke direction."},"opacity":{"type":"number","minimum":0,"maximum":1},"flow":{"type":"number","minimum":0,"maximum":1,"default":1,"description":"Paint deposited by each dab. In stroke mode it builds toward the stroke opacity limit."},"opacity_mode":{"type":"string","enum":["dab","stroke"],"default":"dab","description":"Dab preserves per-deposit opacity; stroke limits cumulative paint influence during each continuous contact."},"hardness":{"type":"number","minimum":0,"maximum":1},"color":{"type":"string"},"background_color":{"type":"string"},"stabilizer":{"type":"number","minimum":0,"maximum":1},"pressure_size":{"type":"boolean"},"pressure_opacity":{"type":"boolean"},"pressure_gamma":{"type":"number","minimum":0.25,"maximum":4},"minimum_radius":{"type":"number","minimum":0,"maximum":1},"spacing":{"type":"number","minimum":0.01,"maximum":2},"minimum_opacity":{"type":"number","minimum":0,"maximum":1}}),
                vec![],
            ),
            Self::Pointer => {
                let mut props = point["properties"].clone();
                props["phase"] = json!({"type":"string","enum":["down","move","up","cancel"]});
                (props, vec!["phase", "x", "y"])
            }
            Self::Stroke => (
                json!({"tool":{"type":"string"},"points":{"type":"array","items":point,"minItems":1,"maxItems":65536}}),
                vec!["points"],
            ),
            Self::SelectLayer | Self::DuplicateLayer | Self::DeleteLayer | Self::MergeDown => (
                json!({"index":{"type":"integer","minimum":0,"maximum":15},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64}}),
                vec![],
            ),
            Self::RenameLayer => (
                json!({"index":{"type":"integer","minimum":0,"maximum":15},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},"name":{"type":"string","minLength":1}}),
                vec!["name"],
            ),
            Self::MoveLayer => (
                json!({"index":{"type":"integer","minimum":0,"maximum":15},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},"to":{"type":"integer","minimum":0,"maximum":15}}),
                vec!["to"],
            ),
            Self::LayerProperties => (
                json!({"index":{"type":"integer","minimum":0,"maximum":15},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},"visible":{"type":"boolean"},"alpha_locked":{"type":"boolean"},"pixel_locked":{"type":"boolean"},"position_locked":{"type":"boolean"},"color_label":{"type":"string","enum":["none","red","orange","yellow","green","blue","violet","gray"]},"opacity":{"type":"number","minimum":0,"maximum":1},"blend_mode":{"type":"string","enum":crate::engine::graph::BlendMode::ALL}}),
                vec![],
            ),
            Self::LayerLinks => (
                json!({"layer_ids":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"integer","minimum":1,"maximum":4294967295u64},"description":"Explicit raster IDs, independent of active painting or multi-selection."},"linked":{"type":"boolean","description":"Link at least two layers, joining their existing link sets, or unlink the explicit layers and retain other members."}}),
                vec!["layer_ids", "linked"],
            ),
            Self::Pick => (json!({"x":number,"y":number}), vec!["x", "y"]),
            Self::View => (
                json!({"zoom":{"type":"number","minimum":0.02,"maximum":32},"pan_x":{"type":"number","minimum":-1000000,"maximum":1000000},"pan_y":{"type":"number","minimum":-1000000,"maximum":1000000},"fit":{"type":"boolean"},"mirror_x":{"type":"boolean","title":"Horizontal View Mirror"},"mirror_y":{"type":"boolean","title":"Vertical View Mirror"}}),
                vec![],
            ),
            Self::Ui => (json!({"id":{"type":"string"},"value":{}}), vec!["id"]),
            Self::WindowDrop => (
                json!({"panel":{"type":"string"},"target":{"type":"string"},"x":{"type":"integer","minimum":-100000,"maximum":100000},"y":{"type":"integer","minimum":-100000,"maximum":100000}}),
                vec!["panel", "x", "y"],
            ),
            Self::WindowCapture => (
                json!({"panel":{"type":"string"},"path":{"type":"string","minLength":1,"description":"Capture destination, at most 32768 UTF-8 bytes"}}),
                vec!["panel", "path"],
            ),
            Self::WindowLayout => (json!({"layout":{"type":"object"}}), vec!["layout"]),
            Self::WindowCommand => (
                json!({"panel":{"type":"string"},"scope":{"type":"string","enum":["panel","group","tabs"]},"drag":{"type":"boolean"},"index":{"type":"integer","minimum":0,"maximum":127},"operation":{"type":"string","enum":["show","hide","focus","float","dock_left","dock_right","dock_top","dock_bottom","tab","minimize","maximize","restore"]},"target":{"type":"string"},"fraction":{"type":"number","minimum":0.05,"maximum":0.95},"x":{"type":"number","minimum":-100000,"maximum":100000},"y":{"type":"number","minimum":-100000,"maximum":100000},"width":{"type":"number","minimum":48,"maximum":10000},"height":{"type":"number","minimum":48,"maximum":10000}}),
                vec!["panel", "operation"],
            ),
            Self::Events => (json!({"after":{"type":"integer","minimum":0}}), vec![]),
            Self::PluginPlan => (
                json!({"id":{"type":"string","minLength":1},"enabled":{"type":"boolean"},"disabled":{"type":"array","maxItems":256,"items":{"type":"string"}}}),
                vec!["id", "enabled"],
            ),
            _ => (json!({}), vec![]),
        };
        let mut props = props;
        if matches!(self, Self::MovePixels | Self::Nudge) {
            props["layer_ids"] = crate::plugins::layer_links::ids_schema();
        }
        if self == Self::TransformPixels {
            props["layer_ids"] = crate::plugins::layer_links::ids_schema();
            props["mask_id"] = json!({"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Transform this whole raw raster-owner mask independently of the active layer. Requires an omitted or auto target. Affine controls only; exterior samples reveal white."});
            props["target"]["description"] = json!("Auto prioritizes the selected raster painting mask, then selected pixels or the active layer. Explicit layer ignores pixel selection and carries only its linked mask. layer_ids is an exact whole-raster set; never expands current link membership. Owner/masked perspective and masked selected pixels remain unsupported.");
        }
        json!({"type":"object","properties":props,"required":required,"additionalProperties":false})
    }
    pub fn read_only(self) -> bool {
        matches!(
            self,
            Self::NodeCatalog
                | Self::BrushPresetsQuery
                | Self::AdjustmentCatalog
                | Self::AdjustmentQuery
                | Self::AdjustmentRead
                | Self::ReplayStatus
                | Self::ReplayOpen
                | Self::ReplayEvents
                | Self::ReplayVerify
                | Self::ReplayLoad
                | Self::ReplaySeek
                | Self::ReplaySnapshot
                | Self::WorkflowRead
                | Self::WorkflowValidate
                | Self::WorkflowState
                | Self::GraphRead
                | Self::HistogramQuery
                | Self::HslPreview
                | Self::TonalPreview
                | Self::GrayscalePreview
                | Self::ThresholdPreview
                | Self::PosterizePreview
                | Self::GraphCatalog
                | Self::Describe
                | Self::ContextRead
                | Self::State
                | Self::LayerQuery
                | Self::ArrangeLayersPlan
                | Self::LayerLinksQuery
                | Self::GroupQuery
                | Self::MaskQuery
                | Self::MaskSnapshot
                | Self::MergeSelectedLayersPlan
                | Self::NewPlan
                | Self::ImageSizePlan
                | Self::CanvasGeometryPlan
                | Self::DocumentInfo
                | Self::TrimBounds
                | Self::TransformState
                | Self::TransformPlan
                | Self::ReadSelection
                | Self::ClipboardState
                | Self::SystemClipboardState
                | Self::Plugins
                | Self::PluginPlan
                | Self::Events
                | Self::Render
                | Self::ExportPlan
                | Self::ExportPreview
                | Self::SettingsRead
                | Self::WindowState
        )
    }
    pub(crate) fn is_mask(self) -> bool {
        matches!(
            self,
            Self::MaskQuery
                | Self::MaskCreate
                | Self::MaskDelete
                | Self::MaskProperties
                | Self::MaskTarget
                | Self::MaskFill
                | Self::MaskInvert
                | Self::MaskFromSelection
                | Self::MaskToSelection
                | Self::MaskSnapshot
                | Self::MaskApply
        )
    }
    pub(crate) fn is_brush_preset(self) -> bool {
        matches!(
            self,
            Self::BrushPresetsQuery
                | Self::BrushPresetUpsert
                | Self::BrushPresetDelete
                | Self::BrushPresetApply
                | Self::BrushPresetsImport
                | Self::BrushPresetsExport
                | Self::BrushPresetsReload
        )
    }
    pub(crate) fn is_adjustment(self) -> bool {
        matches!(
            self,
            Self::AdjustmentCatalog
                | Self::AdjustmentQuery
                | Self::AdjustmentRead
                | Self::AdjustmentCreate
                | Self::AdjustmentSet
                | Self::AdjustmentMove
                | Self::AdjustmentDelete
                | Self::AdjustmentInspect
        )
    }
}
