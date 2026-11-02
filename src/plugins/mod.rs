//! Startup-loaded contributions. Manifests describe tools, action recipes and panels;
//! Native DLL handlers use the versioned C ABI in micopi-plugin-api.
pub(crate) mod native;
pub mod nodes;
pub(crate) mod registry;
pub mod tools;
pub use registry::*;
pub fn panel_key(id: &str) -> u32 {
    id.bytes().fold(2166136261, |hash, b| {
        (hash ^ b as u32).wrapping_mul(16777619)
    })
}

mod compositing;
mod document;
mod layers;
mod painting;
mod system;
mod workbench;
pub(crate) fn execute(
    session: &mut crate::application::Session,
    op: crate::application::Operation,
    args: serde_json::Value,
) -> crate::application::ActionResult {
    use crate::application::Operation::*;
    match op {
        ReplayStatus | ReplayOpen | ReplayEvents | ReplayVerify | ReplayExport | ReplayLoad
        | ReplaySeek | ReplaySnapshot | ReplayFrames | ReplayRecover => {
            crate::application::replay::execute(session, op, args)
        }
        NodeCatalog | WorkflowRead | WorkflowSet | WorkflowPatch | WorkflowValidate
        | WorkflowRun | WorkflowState => crate::application::workflow::execute(session, op, args),
        GraphRead | GraphSet | GraphCatalog => compositing::execute(session, op, args),
        New | Open | Save | Export | Render | Undo | Redo => document::execute(session, op, args),
        AddLayer | SelectLayer | LayerProperties | ClearLayer => layers::execute(session, op, args),
        SelectTool | Parameters | Pointer | Stroke | Pick | View => {
            painting::execute(session, op, args)
        }
        SettingsRead | SettingsWrite | Ui => workbench::execute(session, op, args),
        State | Plugins | Events => system::execute(session, op, args),
        Describe => Ok(crate::application::pipeline::describe(session)),
        ContextRead => crate::application::pipeline::context(session, args),
        PipelineRun => crate::application::pipeline::run(session, args),
    }
}

#[cfg(test)]
#[path = "../../tests/reference/tools.rs"]
pub(crate) mod tool_reference;
