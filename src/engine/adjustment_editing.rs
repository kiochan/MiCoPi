//! Cold metadata-only adjustment edits with real optional A8 ownership.
use super::*;
use crate::engine::{
    adjustments::{Adjustment, AdjustmentSpec, MAX_ADJUSTMENTS},
    groups::{GroupItem, GroupTree},
    raster_masks::{MaskMetadata, MaskOwner, MaskPlane, PaintTarget, SamplePlane},
};

struct CreatePlan {
    tree: GroupTree,
    masks: crate::engine::raster_masks::MaskState,
    prepared: super::group_editing::GroupPlan,
    id: u64,
    mask_id: Option<u64>,
}

#[cfg(test)]
#[path = "adjustment_tests.rs"]
mod tests;
impl Document {
    pub fn adjustments(&self) -> &[Adjustment] {
        self.groups
            .as_ref()
            .map_or(&[], |tree| tree.adjustments.as_slice())
    }
    pub fn adjustment(&self, id: u64) -> Result<&Adjustment, String> {
        self.adjustments()
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| "Adjustment does not exist".into())
    }
    pub fn has_adjustments(&self) -> bool {
        !self.adjustments().is_empty()
    }
    pub fn has_mixed_stack(&self) -> bool {
        self.groups.is_some()
    }
    pub fn adjustment_visible(&self, id: u64) -> bool {
        let Some(tree) = &self.groups else {
            return false;
        };
        let Ok(adjustment) = tree.adjustment(id) else {
            return false;
        };
        if !adjustment.visible {
            return false;
        }
        let Ok(mut parent) = tree.parent(GroupItem::adjustment(id)) else {
            return false;
        };
        for _ in 0..=crate::engine::groups::MAX_DEPTH {
            if parent == 0 {
                return true;
            }
            let Ok(group) = tree.group(parent) else {
                return false;
            };
            if !group.visible {
                return false;
            }
            let Ok(next) = tree.parent(GroupItem::group(parent)) else {
                return false;
            };
            parent = next;
        }
        false
    }
    pub fn adjustment_parent_id(&self, id: u64) -> Result<u64, String> {
        self.groups
            .as_ref()
            .ok_or("Adjustment does not exist")?
            .parent(GroupItem::adjustment(id))
    }
    pub(crate) fn require_adjustment_aware_edit(&self) -> Result<(), String> {
        if self.has_adjustments() {
            Err("This operation does not support non-raster adjustment stack items".into())
        } else {
            Ok(())
        }
    }
    fn require_idle_adjustment_edit(&self) -> Result<(), String> {
        if self.stroke.is_some() || self.mask_contact.is_some() || self.history_group.is_some() {
            Err("Finish the current contact before editing adjustments".into())
        } else {
            Ok(())
        }
    }
    /// Preserve the old-reader rejection barrier after committed Undo/deletion.
    /// Verified restore calls restore_composition directly and remains exact.
    pub(super) fn retain_adjustment_floor(&mut self, floor: u64) {
        let floor = self.next_adjustment_id.max(floor);
        if floor == 0 {
            return;
        }
        let mut tree = self.tree();
        tree.version = 3;
        tree.next_adjustment_id = floor;
        self.next_adjustment_id = floor;
        self.groups = Some(tree);
        let _ = self.refresh_group_cache();
    }
    fn prepare_adjustment_tree(
        &self,
        tree: &GroupTree,
        masks: &crate::engine::raster_masks::MaskState,
    ) -> Result<super::group_editing::GroupPlan, String> {
        self.prepare_group_plan_with_masks(
            &self.graph,
            Some(tree),
            &self.layer_metadata,
            &self.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
            &self.layers.iter().map(|l| l.opacity).collect::<Vec<_>>(),
            self.next_group_id,
            masks,
            true,
        )
    }
    fn plan_adjustment_create(
        &self,
        spec: &AdjustmentSpec,
        parent: u64,
        index: usize,
        with_mask: bool,
    ) -> Result<CreatePlan, String> {
        spec.validate()?;
        self.validate_owner_transform_source()?;
        let id = self.next_adjustment_id.max(1);
        if self.adjustments().len() >= MAX_ADJUSTMENTS || id > u32::MAX as u64 {
            return Err("Adjustment count or identity limit reached".into());
        }
        let mut tree = self.tree();
        let children = tree.children_mut(parent)?;
        if index > children.len() {
            return Err("Adjustment destination exceeds the children".into());
        }
        children.insert(index, GroupItem::adjustment(id));
        tree.version = 3;
        tree.next_adjustment_id = id + 1;
        tree.adjustments.push(Adjustment::new(id, spec.clone()));
        let mut masks = self.raster_masks.clone();
        let mask_id = if with_mask {
            let mask = masks.next_id.max(1);
            if mask > u32::MAX as u64 {
                return Err("Raster mask identity limit reached".into());
            }
            masks.version = 2;
            masks.next_id = mask + 1;
            masks
                .items
                .push(MaskMetadata::new(mask, MaskOwner::Adjustment { id }));
            Some(mask)
        } else {
            None
        };
        let prepared = self.prepare_adjustment_tree(&tree, &masks)?;
        Ok(CreatePlan {
            tree,
            masks,
            prepared,
            id,
            mask_id,
        })
    }
    /// Read-only full source/candidate preflight before a selection is copied.
    pub fn preflight_create_adjustment(
        &self,
        spec: &AdjustmentSpec,
        parent: u64,
        index: usize,
        with_mask: bool,
    ) -> Result<(), String> {
        self.plan_adjustment_create(spec, parent, index, with_mask)
            .map(|_| ())
    }
    pub fn create_adjustment(
        &mut self,
        spec: AdjustmentSpec,
        parent: u64,
        index: usize,
        mask: Option<SamplePlane>,
    ) -> Result<(u64, Option<u64>), String> {
        self.require_idle_adjustment_edit()?;
        let plan = self.plan_adjustment_create(&spec, parent, index, mask.is_some())?;
        if let Some(samples) = &mask {
            samples.validate(self.width * self.height)?;
        }
        let before = self.composition();
        let slot = self.mask_planes.len();
        self.next_adjustment_id = plan.tree.next_adjustment_id;
        self.groups = Some(plan.tree);
        self.raster_masks = plan.masks;
        let mask_change = if let (Some(id), Some(samples)) = (plan.mask_id, mask) {
            self.mask_planes.push(MaskPlane {
                id,
                samples,
                content_revision: 0,
            });
            self.mark_mask_plane(id);
            self.bump_mask_storage_epoch();
            Some(MaskChange::Toggle {
                slot,
                removed: None,
            })
        } else {
            None
        };
        self.install_group_plan(plan.prepared);
        self.commit_owner_composition(before, None, mask_change);
        Ok((plan.id, plan.mask_id))
    }
    pub fn set_adjustment(&mut self, id: u64, spec: AdjustmentSpec) -> Result<bool, String> {
        self.require_idle_adjustment_edit()?;
        spec.validate()?;
        self.validate_owner_transform_source()?;
        let mut tree = self.tree();
        let old = tree.adjustment(id)?;
        if old.spec() == spec {
            return Ok(false);
        }
        *tree.adjustments.iter_mut().find(|a| a.id == id).unwrap() = Adjustment::new(id, spec);
        self.commit_tree(tree, self.next_group_id)?;
        Ok(true)
    }
    pub fn move_adjustment(&mut self, id: u64, parent: u64, index: usize) -> Result<bool, String> {
        self.require_idle_adjustment_edit()?;
        self.validate_owner_transform_source()?;
        let mut tree = self.tree();
        tree.adjustment(id)?;
        tree.children(parent)?;
        let item = GroupItem::adjustment(id);
        let old_parent = tree.parent(item)?;
        tree.children_mut(old_parent)?.retain(|v| *v != item);
        let children = tree.children_mut(parent)?;
        if index > children.len() {
            return Err("Adjustment destination exceeds the remaining children".into());
        }
        children.insert(index, item);
        if self.groups.as_ref() == Some(&tree) {
            return Ok(false);
        }
        self.commit_tree(tree, self.next_group_id)?;
        Ok(true)
    }
    pub fn delete_adjustment(&mut self, id: u64) -> Result<Option<u64>, String> {
        self.require_idle_adjustment_edit()?;
        self.validate_owner_transform_source()?;
        let mut tree = self.tree();
        tree.adjustment(id)?;
        let item = GroupItem::adjustment(id);
        let parent = tree.parent(item)?;
        tree.children_mut(parent)?.retain(|v| *v != item);
        tree.adjustments.retain(|a| a.id != id);
        let mut masks = self.raster_masks.clone();
        let slot = masks
            .items
            .iter()
            .position(|m| m.owner == (MaskOwner::Adjustment { id }));
        let mask_id = slot.map(|slot| masks.items[slot].id);
        if let Some(slot) = slot {
            masks.items.remove(slot);
            if masks.target
                == (PaintTarget::Mask {
                    id: mask_id.unwrap(),
                })
            {
                masks.target = PaintTarget::LayerPixels;
            }
        }
        let prepared = self.prepare_adjustment_tree(&tree, &masks)?;
        let before = self.composition();
        let mask_change = if let Some(slot) = slot {
            let removed = self.mask_planes.remove(slot);
            self.remove_mask_effect_cache(removed.id);
            self.replay_mask_tiles
                .retain(|(mask, _)| *mask != removed.id);
            self.bump_mask_storage_epoch();
            Some(MaskChange::Toggle {
                slot,
                removed: Some(Arc::new(removed)),
            })
        } else {
            None
        };
        self.groups = Some(tree);
        self.raster_masks = masks;
        self.install_group_plan(prepared);
        self.commit_owner_composition(before, None, mask_change);
        Ok(mask_id)
    }
}
