//! Preserve byte planes while the remaining mask-aware editing paths are added.
use super::Document;
pub(crate) const MASK_EDITING_PENDING: &str = "This operation requires mask-aware editing support; the document and raster masks are unchanged";

impl Document {
    pub(crate) fn require_mask_aware_edit(&self) -> Result<(), String> {
        if self.has_raster_masks() {
            Err(MASK_EDITING_PENDING.into())
        } else {
            Ok(())
        }
    }
}
