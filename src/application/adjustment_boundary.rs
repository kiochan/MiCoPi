//! Shared pre-publication admission for mixed non-raster layer stacks.
use super::{ActionError, Operation, Session};
use serde_json::Value;

impl Session {
    pub(crate) fn validate_adjustment_boundary(
        &self,
        operation: Option<Operation>,
        args: &Value,
    ) -> Result<(), ActionError> {
        if !self.doc.has_adjustments() {
            return Ok(());
        }
        use Operation::*;
        if matches!(
            operation,
            Some(
                LayerBatch
                    | LayerViaCopy
                    | LayerViaCut
                    | MergeDown
                    | MergeLayers
                    | MergeSelectedLayers
                    | MergeSelectedLayersPlan
                    | OpenSelectedMerge
                    | MergeVisible
                    | FlattenImage
                    | StampVisible
                    | GroupUngroup
                    | GroupUngroupParent
                    | GroupMerge
                    | ClipboardCut
                    | ClipboardPaste
                    | ClipboardPasteInPlace
                    | SystemClipboardCut
                    | SystemClipboardPaste
                    | SystemClipboardPasteInPlace
                    | TransformRepeatCopy
            )
        ) || operation == Some(GraphSet) && args["graph"]["linked"] != true
            || operation == Some(TransformRepeat) && args["duplicate"] == true
        {
            return Err(ActionError::new(
                "adjustment_editing_pending",
                "This operation does not yet support non-raster adjustment stack items",
            ));
        }
        if operation.is_some_and(|op| op.document_atomic())
            && !matches!(operation, Some(Undo | Redo))
        {
            crate::plugins::adjustments::validate_document(&self.registry, &self.doc)?;
        }
        Ok(())
    }
}
