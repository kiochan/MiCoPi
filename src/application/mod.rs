//! The single document owner and action boundary, independent of windows and egui.
mod actions;
mod adjustment_boundary;
#[cfg(test)]
mod adjustment_tests;
mod atomic;
mod brush_presets;
mod brush_tip;
pub use brush_tip::TipShape;
pub(crate) use brush_tip::{default_tip_roundness, full_tip_roundness, zero_tip_angle};
#[cfg(test)]
mod brush_presets_tests;
#[cfg(test)]
mod brush_tip_tests;
pub(crate) mod canvas_geometry;
pub mod codec;
pub(crate) mod creation;
#[cfg(test)]
mod layer_arrangement_tests;
#[cfg(test)]
mod levels_adjustment_tests;
#[cfg(test)]
mod levels_tests;
mod mask_boundary;
pub mod pipeline;
pub(crate) mod png_io;
mod presentation_captures;
pub mod replay;
mod selection_sampling;
mod session;
pub use selection_sampling::SelectionSampling;
pub mod workflow;
pub(crate) use actions::empty_object as actions_empty_object;
pub use actions::{Action, ActionError, ActionResult, Operation};
pub use session::{
    parse_color, OpacityMode, Parameters, Pointer, PointerPhase, Session, ToolKind, View,
};
pub(crate) use session::{serialize_spacing, spacing_json};
#[cfg(test)]
mod tests;

#[cfg(test)]
mod alpha_lock_tests;
#[cfg(test)]
mod blend_mode_tests;
#[cfg(test)]
mod brush_opacity_tests;
#[cfg(test)]
mod canvas_editing_tests;
#[cfg(test)]
mod editing_tests;
#[cfg(test)]
mod graph_tests;
#[cfg(test)]
mod group_merge_tests;
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod layer_batch_tests;
#[cfg(test)]
mod layer_merge_tests;
#[cfg(test)]
mod layer_organization_tests;
pub(crate) mod layer_query;
#[cfg(test)]
mod linked_graph_tests;
#[cfg(test)]
mod mask_canvas_geometry_tests;
#[cfg(test)]
mod mask_group_delete_tests;
#[cfg(test)]
mod mask_group_duplicate_tests;
#[cfg(test)]
mod mask_group_editing_tests;
#[cfg(test)]
mod mask_owner_editing_tests;
#[cfg(test)]
mod owner_affine_tests;
#[cfg(test)]
mod pass_through_tests;
#[cfg(test)]
mod pipeline_tests;
#[cfg(test)]
mod raster_mask_failure_tests;
#[cfg(test)]
mod raster_mask_tests;
#[cfg(test)]
mod raster_mask_workflow_tests;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod result_revision_tests;
#[cfg(test)]
mod selected_merge_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod stroke_opacity_tests;
#[cfg(test)]
mod tool_tests;
#[cfg(test)]
mod workflow_tests;

#[cfg(test)]
mod clipboard_tests;

#[cfg(test)]
mod layer_transfer_tests;

#[cfg(test)]
mod system_clipboard_tests;

#[cfg(test)]
mod geometry_tests;

#[cfg(test)]
mod resample_tests;

#[cfg(test)]
mod trim_tests;

mod movement_preferences;
pub use movement_preferences::{MoveTarget, MovementPreferences};

#[cfg(test)]
mod affine_tests;
#[cfg(test)]
mod linked_movement_tests;
#[cfg(test)]
mod movement_tests;
#[cfg(test)]
mod owner_movement_tests;
#[cfg(test)]
mod skew_tests;
#[cfg(test)]
mod transform_repeat_tests;

#[cfg(test)]
mod perspective_tests;

#[cfg(test)]
mod document_setup_tests;

#[cfg(test)]
mod geometry_density_tests;
#[cfg(test)]
mod png_density_tests;

#[cfg(test)]
mod export_tests;
pub(crate) mod image_size;
pub(crate) mod output_size;
#[cfg(test)]
mod physical_creation_tests;
#[cfg(test)]
mod physical_resample_tests;
pub(crate) mod units;
