//! Startup-loaded contributions. Manifests describe tools, action recipes and panels;
//! Native DLL handlers use the versioned C ABI in micopi-plugin-api.
pub(crate) mod native;
pub mod nodes;
mod packages;
pub(crate) mod registry;
pub mod tools;
pub use packages::{PluginInfo, PluginPlan, PluginStatus};
pub use registry::*;
pub fn panel_key(id: &str) -> u32 {
    id.bytes().fold(2166136261, |hash, b| {
        (hash ^ b as u32).wrapping_mul(16777619)
    })
}

pub(crate) mod clipboard;
mod compositing;
mod document;
mod geometry;
mod layer_transfer;
mod layers;
mod painting;
pub(crate) mod resample;
mod selection;
mod system;
mod system_clipboard;
pub(crate) mod windows;
mod workbench;
pub(crate) fn execute(
    session: &mut crate::application::Session,
    op: crate::application::Operation,
    args: serde_json::Value,
) -> crate::application::ActionResult {
    use crate::application::Operation::*;
    match op {
        ImageSize => resample::execute(session, args),
        RotateCanvasClockwise
        | RotateCanvasCounterclockwise
        | RotateCanvasHalf
        | FlipCanvasHorizontal
        | FlipCanvasVertical
        | RotateLayerClockwise
        | RotateLayerCounterclockwise => geometry::execute(session, op),
        SystemClipboardCopy
        | SystemClipboardCopyMerged
        | SystemClipboardCut
        | SystemClipboardPaste
        | SystemClipboardPasteInPlace
        | SystemClipboardState => system_clipboard::execute(session, op),
        LayerViaCopy | LayerViaCut => layer_transfer::execute(session, op),
        ClipboardCopy
        | ClipboardCopyMerged
        | ClipboardCut
        | ClipboardPaste
        | ClipboardPasteInPlace
        | ClipboardState => clipboard::execute(session, op, args),
        SelectionShape | SelectAll | Deselect | Reselect | InvertSelection | ReadSelection
        | SelectColor | GrowSelection | SimilarSelection | ExpandSelection | ContractSelection
        | FeatherSelection | SmoothSelection => selection::execute(session, op, args),
        ReplayStatus | ReplayOpen | ReplayEvents | ReplayVerify | ReplayExport | ReplayLoad
        | ReplaySeek | ReplaySnapshot | ReplayFrames | ReplayRecover => {
            crate::application::replay::execute(session, op, args)
        }
        NodeCatalog | WorkflowRead | WorkflowSet | WorkflowPatch | WorkflowValidate
        | WorkflowRun | WorkflowState => crate::application::workflow::execute(session, op, args),
        GraphRead | GraphSet | GraphCatalog => compositing::execute(session, op, args),
        New | CanvasSize | Crop | Open | Save | Export | Render | Undo | Redo => {
            document::execute(session, op, args)
        }
        AddLayer | RenameLayer | DuplicateLayer | MoveLayer | DeleteLayer | SelectLayer
        | MergeDown | MergeVisible | FlattenImage | StampVisible | LayerProperties | ClearLayer => {
            layers::execute(session, op, args)
        }
        SelectTool | Parameters | Pointer | Stroke | Pick | View => {
            painting::execute(session, op, args)
        }
        WindowCommand | WindowState | WindowLayout | WindowCapture | WindowDrop => {
            windows::execute(session, op, args)
        }
        CaptureWorkspace | SettingsRead | SettingsWrite | Ui => {
            workbench::execute(session, op, args)
        }
        State | Plugins | PluginPlan | Events => system::execute(session, op, args),
        Describe => Ok(crate::application::pipeline::describe(session)),
        ContextRead => crate::application::pipeline::context(session, args),
        PipelineRun => crate::application::pipeline::run(session, args),
    }
}

#[cfg(test)]
#[path = "../../tests/reference/tools.rs"]
pub(crate) mod tool_reference;
