//! Bounded mask ownership removal for a deleted group and its descendants.
use super::*;
use crate::engine::raster_masks::{MaskOwner, MaskState, PaintTarget};

pub(super) struct GroupMaskRemoval {
    pub state: MaskState,
    slots: Vec<usize>,
}
impl Document {
    /// Source storage/capabilities and the complete candidate composition are
    /// validated by the caller before any plane or raster buffer is removed.
    pub(super) fn plan_group_mask_removal(
        &self,
        layer_ids: &[u64],
        group_ids: &[u64],
    ) -> GroupMaskRemoval {
        let mut state = self.raster_masks.clone();
        let slots: Vec<_> = state
            .items
            .iter()
            .enumerate()
            .filter_map(|(slot, item)| {
                let removed = match item.owner {
                    MaskOwner::Layer { id } => layer_ids.contains(&id),
                    MaskOwner::Group { id } => group_ids.contains(&id),
                };
                removed.then_some(slot)
            })
            .collect();
        if slots.iter().any(|slot| {
            state.target
                == (PaintTarget::Mask {
                    id: state.items[*slot].id,
                })
        }) {
            state.target = PaintTarget::LayerPixels;
        }
        for &slot in slots.iter().rev() {
            state.items.remove(slot);
        }
        GroupMaskRemoval { state, slots }
    }
    pub(super) fn publish_group_mask_removal(
        &mut self,
        edit: GroupMaskRemoval,
    ) -> Option<MaskChange> {
        let mut saved = Vec::with_capacity(edit.slots.len());
        for &slot in edit.slots.iter().rev() {
            let plane = self.mask_planes.remove(slot);
            self.remove_mask_effect_cache(plane.id);
            self.replay_mask_tiles.retain(|(id, _)| *id != plane.id);
            saved.push((slot, Arc::new(plane)));
        }
        saved.reverse();
        self.raster_masks = edit.state;
        self.mask_contact = None;
        self.bump_mask_storage_epoch();
        (!saved.is_empty()).then(|| {
            MaskChange::Structural(Arc::new(MaskStructuralChange {
                current: Vec::new(),
                saved,
            }))
        })
    }
}
