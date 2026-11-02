//! The single document owner and action boundary, independent of windows and egui.
mod actions;
pub mod codec;
pub mod pipeline;
mod session;
pub mod workflow;
pub(crate) use actions::empty_object as actions_empty_object;
pub use actions::{Action, ActionError, ActionResult, Operation};
pub use session::{parse_color, Parameters, Pointer, PointerPhase, Session, ToolKind, View};
#[cfg(test)]
mod tests;

#[cfg(test)]
mod graph_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod tool_tests;
#[cfg(test)]
mod workflow_tests;
