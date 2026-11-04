//! The single document owner and action boundary, independent of windows and egui.
mod actions;
mod atomic;
pub mod codec;
pub mod pipeline;
pub mod replay;
mod session;
pub mod workflow;
pub(crate) use actions::empty_object as actions_empty_object;
pub use actions::{Action, ActionError, ActionResult, Operation};
pub use session::{parse_color, Parameters, Pointer, PointerPhase, Session, ToolKind, View};
#[cfg(test)]
mod tests;

#[cfg(test)]
mod alpha_lock_tests;
#[cfg(test)]
mod canvas_editing_tests;
#[cfg(test)]
mod editing_tests;
#[cfg(test)]
mod graph_tests;
#[cfg(test)]
mod layer_merge_tests;
#[cfg(test)]
mod linked_graph_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod tool_tests;
#[cfg(test)]
mod workflow_tests;
