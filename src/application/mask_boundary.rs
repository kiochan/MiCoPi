//! Temporary edit boundaries while document masks gain complete editing support.
//! Read-only inspection and ordinary RGBA painting retain their own contracts.
use super::{ActionError, Operation, Session};
use serde_json::Value;

pub(crate) const EDITING_PENDING: &str = crate::engine::MASK_EDITING_PENDING;

impl Session {
    pub(crate) fn validate_mask_boundary(
        &self,
        operation: Option<Operation>,
        args: &Value,
    ) -> Result<(), ActionError> {
        if matches!(operation, Some(Operation::MovePixels | Operation::Nudge))
            && args.get("layer_ids").is_some()
        {
            return crate::plugins::movement::admit_linked(self, args);
        }
        if !self.doc.has_raster_masks() {
            return Ok(());
        }
        use Operation::*;
        if matches!(operation, Some(MovePixels | Nudge)) {
            return crate::plugins::movement::admit_masked(self, args);
        }
        if matches!(
            operation,
            Some(
                TransformPixels
                    | TransformPreview
                    | TransformPlan
                    | TransformRepeat
                    | TransformRepeatCopy
                    | RotateLayerClockwise
                    | RotateLayerCounterclockwise
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
                    | LayerBatch
                    | GroupUngroup
                    | GroupUngroupParent
                    | GroupMerge
                    | ClipboardCut
                    | ClipboardPaste
                    | ClipboardPasteInPlace
                    | SystemClipboardCut
                    | SystemClipboardPaste
                    | SystemClipboardPasteInPlace
            )
        ) {
            return Err(ActionError::new("mask_editing_pending", EDITING_PENDING));
        }
        if operation == Some(GraphSet) && args["graph"]["linked"] != true {
            return Err(ActionError::new(
                "mask_editing_pending",
                "Raster masks require linked layer/filter composition",
            ));
        }
        Ok(())
    }
}
