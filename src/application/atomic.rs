//! Document transactions share replay tiles and history; pixels are copied only on rollback.
use super::*;
use crate::engine::{document::HistoryCheckpoint, replay::State};
use serde_json::Value;
impl Session {
    pub(crate) fn validate_atomic_action(&self, id: &str, depth: usize) -> Result<(), ActionError> {
        if depth > 16 {
            return Err(ActionError::new(
                "execution_limit",
                "Atomic action nesting limit exceeded",
            ));
        }
        let action = self
            .registry
            .actions
            .get(id)
            .ok_or_else(|| ActionError::new("unknown_action", id))?;
        if action.operation.is_none() && action.native.is_none() {
            for step in &action.steps {
                self.validate_atomic_action(&step.action, depth + 1)?;
            }
            return Ok(());
        }
        if (action.is_read_only()
            && !matches!(
                action.operation,
                Some(Operation::ReplayLoad | Operation::ReplayOpen)
            ))
            || action.is_document_atomic()
            || action.operation.is_some_and(Operation::allowed_in_atomic)
        {
            return Ok(());
        }
        Err(ActionError::new(
            "non_atomic_action",
            format!("{id} does not support atomic document execution"),
        ))
    }
}
#[derive(Clone)]
pub(crate) struct Checkpoint {
    state: State,
    history: HistoryCheckpoint,
    last_pointer: Option<Pointer>,
    pen_observed: bool,
    ui_effects: std::collections::VecDeque<Value>,
}
impl Checkpoint {
    pub fn capture(s: &Session) -> Self {
        Self {
            state: s.journal.as_ref().expect("journal boundary").state.clone(),
            history: s.doc.checkpoint_history(),
            last_pointer: s.last_pointer,
            pen_observed: s.pen_observed,
            ui_effects: s.ui_effects.clone(),
        }
    }
    pub fn restore(self, s: &mut Session) {
        s.stroke = Default::default();
        s.tool_instances.clear();
        self.state.restore_document(&mut s.doc);
        s.doc.restore_history(self.history);
        let m = self.state.metadata;
        s.parameters = m.parameters;
        s.selected_tool = m.selected_tool;
        s.tool = m.tool;
        s.view = m.view;
        s.file = m.file;
        s.source_file = m.source_file;
        s.ui = m.ui;
        s.revision = m.revision;
        s.last_pointer = self.last_pointer;
        s.pen_observed = self.pen_observed;
        s.ui_effects = self.ui_effects;
        s.replay_samples.clear();
        s.replay_direct_contact = false;
    }
}
impl Operation {
    pub(crate) fn document_atomic(self) -> bool {
        matches!(
            self,
            Self::Stroke
                | Self::CanvasSize
                | Self::Crop
                | Self::Undo
                | Self::Redo
                | Self::AddLayer
                | Self::RenameLayer
                | Self::DuplicateLayer
                | Self::MoveLayer
                | Self::DeleteLayer
                | Self::MergeDown
                | Self::MergeVisible
                | Self::FlattenImage
                | Self::StampVisible
                | Self::SelectLayer
                | Self::LayerProperties
                | Self::ClearLayer
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
                | Self::GraphSet
                | Self::WorkflowSet
                | Self::WorkflowPatch
                | Self::Parameters
                | Self::SelectTool
                | Self::Pick
                | Self::View
        )
    }
    pub(crate) fn allowed_in_atomic(self) -> bool {
        self.document_atomic() || matches!(self, Self::PipelineRun | Self::WorkflowRun)
    }
}
