//! Bounded mask ownership staging for copied or deleted group subtrees.
use super::*;
use crate::engine::raster_masks::{MaskOwner, MaskPlane, MaskState, PaintTarget, MAX_MASKS};

pub(super) struct GroupMaskRemoval {
    pub state: MaskState,
    slots: Vec<usize>,
}
pub(super) struct GroupMaskCopy {
    pub state: MaskState,
    sources: Vec<(usize, u64)>,
}
/// Plan only bounded metadata. Candidate owners are validated by the caller's
/// final group plan before the source samples are copied.
#[cfg(test)]
pub(super) fn plan_mask_copy(
    state: &MaskState,
    layer_map: &[(u64, u64)],
    group_map: &[(u64, u64)],
    width: usize,
    height: usize,
) -> Result<GroupMaskCopy, String> {
    plan_mask_copy_with_adjustments(state, layer_map, group_map, &[], width, height)
}
fn plan_mask_copy_with_adjustments(
    state: &MaskState,
    layer_map: &[(u64, u64)],
    group_map: &[(u64, u64)],
    adjustment_map: &[(u64, u64)],
    width: usize,
    height: usize,
) -> Result<GroupMaskCopy, String> {
    let sources: Vec<_> = state
        .items
        .iter()
        .enumerate()
        .filter_map(|(slot, item)| {
            let owner = match item.owner {
                MaskOwner::Layer { id } => layer_map
                    .iter()
                    .find(|(from, _)| *from == id)
                    .map(|(_, id)| MaskOwner::Layer { id: *id }),
                MaskOwner::Group { id } => group_map
                    .iter()
                    .find(|(from, _)| *from == id)
                    .map(|(_, id)| MaskOwner::Group { id: *id }),
                MaskOwner::Adjustment { id } => adjustment_map
                    .iter()
                    .find(|(from, _)| *from == id)
                    .map(|(_, id)| MaskOwner::Adjustment { id: *id }),
            };
            owner.map(|owner| (slot, owner))
        })
        .collect();
    if state.items.len() + sources.len() > MAX_MASKS {
        return Err("Group copy exceeds the raster mask count limit".into());
    }
    let mut candidate = state.clone();
    let mut copies = Vec::with_capacity(sources.len());
    for (slot, owner) in sources {
        let id = candidate.next_id.max(1);
        if id > u32::MAX as u64 {
            return Err("Raster mask identity limit reached".into());
        }
        let mut metadata = state.items[slot].clone();
        if candidate.target == (PaintTarget::Mask { id: metadata.id }) {
            candidate.target = PaintTarget::Mask { id };
        }
        metadata.id = id;
        metadata.owner = owner;
        candidate.items.push(metadata);
        candidate.next_id = id + 1;
        copies.push((slot, id));
    }
    candidate.validate_dimensions(width, height)?;
    Ok(GroupMaskCopy {
        state: candidate,
        sources: copies,
    })
}
impl Document {
    pub(super) fn plan_group_mask_copy_with_adjustments(
        &self,
        layer_map: &[(u64, u64)],
        group_map: &[(u64, u64)],
        adjustment_map: &[(u64, u64)],
    ) -> Result<GroupMaskCopy, String> {
        plan_mask_copy_with_adjustments(
            self.mask_state(),
            layer_map,
            group_map,
            adjustment_map,
            self.width,
            self.height,
        )
    }
    /// Only intentional copies allocate raw storage, after candidate compilation.
    pub(super) fn stage_group_mask_copy(&self, edit: &GroupMaskCopy) -> Vec<MaskPlane> {
        edit.sources
            .iter()
            .map(|(slot, id)| MaskPlane {
                id: *id,
                samples: self.mask_planes[*slot].samples.clone(),
                content_revision: 0,
            })
            .collect()
    }
    pub(super) fn publish_group_mask_copy(
        &mut self,
        edit: GroupMaskCopy,
        copies: Vec<MaskPlane>,
    ) -> Option<MaskChange> {
        debug_assert_eq!(copies.len(), edit.sources.len());
        let current: Vec<_> =
            (self.mask_planes.len()..self.mask_planes.len() + copies.len()).collect();
        for plane in copies {
            let id = plane.id;
            self.mask_planes.push(plane);
            self.mark_mask_plane(id);
        }
        self.raster_masks = edit.state;
        self.mask_contact = None;
        self.bump_mask_storage_epoch();
        (!current.is_empty()).then(|| {
            MaskChange::Structural(Arc::new(MaskStructuralChange {
                current,
                saved: Vec::new(),
            }))
        })
    }
    /// Source storage/capabilities and the complete candidate composition are
    /// validated by the caller before any plane or raster buffer is removed.
    pub(super) fn plan_group_mask_removal_with_adjustments(
        &self,
        layer_ids: &[u64],
        group_ids: &[u64],
        adjustment_ids: &[u64],
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
                    MaskOwner::Adjustment { id } => adjustment_ids.contains(&id),
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
