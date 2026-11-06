//! Bounded owner-mask staging for a single raster insertion or removal.
use super::*;
use crate::engine::raster_masks::{MaskOwner, MaskPlane, MaskState, PaintTarget};

pub(super) struct OwnerMaskEdit {
    pub state: MaskState,
    change: Option<OwnerMaskChange>,
}
#[derive(Clone, Copy)]
enum OwnerMaskChange {
    Copy { source: usize, id: u64 },
    Remove { slot: usize },
}
impl Document {
    fn validate_owner_mask_edit(&self) -> Result<(), String> {
        if self.stroke.is_some() {
            return Err("Finish the current contact before editing layer owners".into());
        }
        self.validate_mask_storage()?;
        self.ensure_graph_renderer()
    }
    pub(super) fn plan_owner_mask_copy(
        &self,
        index: usize,
        owner_id: u64,
    ) -> Result<OwnerMaskEdit, String> {
        self.validate_owner_mask_edit()?;
        let mut state = self.raster_masks.clone();
        let source = state.items.iter().position(|item| {
            item.owner
                == (MaskOwner::Layer {
                    id: self.layer_id(index),
                })
        });
        let change = if let Some(source) = source {
            let id = state.next_id.max(1);
            if id > u32::MAX as u64 {
                return Err("Raster mask identity limit reached".into());
            }
            let mut metadata = state.items[source].clone();
            if state.target == (PaintTarget::Mask { id: metadata.id }) {
                state.target = PaintTarget::Mask { id };
            }
            metadata.id = id;
            metadata.owner = MaskOwner::Layer { id: owner_id };
            state.items.push(metadata);
            state.next_id = id + 1;
            Some(OwnerMaskChange::Copy { source, id })
        } else {
            None
        };
        // Candidate owner IDs are checked by the final group-plan preflight.
        state.validate_dimensions(self.width, self.height)?;
        Ok(OwnerMaskEdit { state, change })
    }
    pub(super) fn plan_owner_mask_delete(&self, index: usize) -> Result<OwnerMaskEdit, String> {
        self.validate_owner_mask_edit()?;
        let mut state = self.raster_masks.clone();
        let slot = state.items.iter().position(|item| {
            item.owner
                == (MaskOwner::Layer {
                    id: self.layer_id(index),
                })
        });
        let change = slot.map(|slot| {
            let removed = state.items.remove(slot);
            if state.target == (PaintTarget::Mask { id: removed.id }) {
                state.target = PaintTarget::LayerPixels;
            }
            OwnerMaskChange::Remove { slot }
        });
        Ok(OwnerMaskEdit { state, change })
    }
    /// Called only after every candidate metadata/capability/budget check. Dense
    /// source storage is copied exactly once and never shared for later writes.
    pub(super) fn stage_owner_mask_copy(&self, edit: &OwnerMaskEdit) -> Option<MaskPlane> {
        match edit.change {
            Some(OwnerMaskChange::Copy { source, id }) => Some(MaskPlane {
                id,
                samples: self.mask_planes[source].samples.clone(),
                content_revision: 0,
            }),
            _ => None,
        }
    }
    pub(super) fn publish_owner_mask_edit(
        &mut self,
        edit: OwnerMaskEdit,
        copied: Option<MaskPlane>,
    ) -> Option<MaskChange> {
        let change = match edit.change {
            Some(OwnerMaskChange::Copy { .. }) => {
                let plane = copied.expect("A validated owner copy is staged before publication");
                let slot = self.mask_planes.len();
                let id = plane.id;
                self.mask_planes.push(plane);
                self.mark_mask_plane(id);
                Some(MaskChange::Toggle {
                    slot,
                    removed: None,
                })
            }
            Some(OwnerMaskChange::Remove { slot }) => {
                let removed = self.mask_planes.remove(slot);
                self.remove_mask_effect_cache(removed.id);
                self.replay_mask_tiles.retain(|(id, _)| *id != removed.id);
                Some(MaskChange::Toggle {
                    slot,
                    removed: Some(Arc::new(removed)),
                })
            }
            None => None,
        };
        // Selection of the new raster may have temporarily selected Pixels.
        // Publish the preflighted target together with its matching owner set.
        self.raster_masks = edit.state;
        self.mask_contact = None;
        self.bump_mask_storage_epoch();
        change
    }
}
