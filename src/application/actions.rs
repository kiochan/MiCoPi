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
    CanvasSize,
    ImageSize,
    Crop,
    Open,
    Save,
    Export,
    Render,
    SelectTool,
    Parameters,
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
    MergeVisible,
    FlattenImage,
    StampVisible,
    SelectLayer,
    LayerProperties,
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
            Self::New => (
                json!({"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096}}),
                vec!["width", "height"],
            ),
            Self::ImageSize => (
                json!({"width":{"type":"integer","minimum":1,"maximum":4096,"description":"Target width; with lock_aspect use exactly one dimension."},"height":{"type":"integer","minimum":1,"maximum":4096,"description":"Target height; with lock_aspect use exactly one dimension."},"lock_aspect":{"type":"boolean","default":true,"description":"Preserve source ratio; supply exactly one dimension. When false supply both."},"method":{"type":"string","enum":["bilinear","nearest"],"default":"bilinear"}}),
                vec![],
            ),
            Self::CanvasSize => (
                json!({"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096},"anchor":{"type":"string","enum":["top_left","top","top_right","left","center","right","bottom_left","bottom","bottom_right"]}}),
                vec!["width", "height"],
            ),
            Self::Crop => (
                json!({"x":{"type":"integer","minimum":0,"maximum":4095},"y":{"type":"integer","minimum":0,"maximum":4095},"width":{"type":"integer","minimum":1,"maximum":4096},"height":{"type":"integer","minimum":1,"maximum":4096}}),
                vec!["x", "y", "width", "height"],
            ),
            Self::Open
            | Self::Save
            | Self::Export
            | Self::SettingsRead
            | Self::CaptureWorkspace => (
                json!({"path":{"type":"string","minLength":1}}),
                vec!["path"],
            ),
            Self::SettingsWrite => (
                json!({"path":{"type":"string","minLength":1},"ui":{"type":"object"},"keybindings":{"type":"object"},"painting":{"type":"object"},"selection":{"type":"object"},"plugins":{"type":"object"},"document":{"type":"object"}}),
                vec!["path"],
            ),
            Self::SelectTool => (json!({"tool":{"type":"string"}}), vec!["tool"]),
            Self::Parameters => (
                json!({"radius":{"type":"number","minimum":0.5,"maximum":200},"opacity":{"type":"number","minimum":0,"maximum":1},"hardness":{"type":"number","minimum":0,"maximum":1},"color":{"type":"string"},"background_color":{"type":"string"},"stabilizer":{"type":"number","minimum":0,"maximum":1},"pressure_size":{"type":"boolean"},"pressure_opacity":{"type":"boolean"},"pressure_gamma":{"type":"number","minimum":0.25,"maximum":4},"minimum_radius":{"type":"number","minimum":0,"maximum":1},"spacing":{"type":"number","minimum":0.01,"maximum":2},"minimum_opacity":{"type":"number","minimum":0,"maximum":1}}),
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
                json!({"index":{"type":"integer","minimum":0,"maximum":15},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},"visible":{"type":"boolean"},"alpha_locked":{"type":"boolean"},"opacity":{"type":"number","minimum":0,"maximum":1},"blend_mode":{"type":"string","enum":["normal","multiply","screen","overlay","darken","lighten","add","difference"]}}),
                vec![],
            ),
            Self::Pick => (json!({"x":number,"y":number}), vec!["x", "y"]),
            Self::View => (
                json!({"zoom":{"type":"number","minimum":0.02,"maximum":32},"pan_x":{"type":"number","minimum":-1000000,"maximum":1000000},"pan_y":{"type":"number","minimum":-1000000,"maximum":1000000},"fit":{"type":"boolean"}}),
                vec![],
            ),
            Self::Ui => (json!({"id":{"type":"string"},"value":{}}), vec!["id"]),
            Self::WindowDrop => (
                json!({"panel":{"type":"string"},"target":{"type":"string"},"x":{"type":"integer","minimum":-100000,"maximum":100000},"y":{"type":"integer","minimum":-100000,"maximum":100000}}),
                vec!["panel", "x", "y"],
            ),
            Self::WindowCapture => (
                json!({"panel":{"type":"string"},"path":{"type":"string"}}),
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
        json!({"type":"object","properties":props,"required":required,"additionalProperties":false})
    }
    pub fn read_only(self) -> bool {
        matches!(
            self,
            Self::NodeCatalog
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
                | Self::GraphCatalog
                | Self::Describe
                | Self::ContextRead
                | Self::State
                | Self::ReadSelection
                | Self::ClipboardState
                | Self::SystemClipboardState
                | Self::Plugins
                | Self::PluginPlan
                | Self::Events
                | Self::Render
                | Self::SettingsRead
                | Self::WindowState
        )
    }
}
