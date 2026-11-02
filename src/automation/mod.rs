//! Transport adapters use exactly the same action executor as the GUI.
pub mod batch;
#[cfg(windows)]
pub mod ipc;
pub mod mcp;
