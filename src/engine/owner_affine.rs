//! Explicit owner affine admission and one grouped raw/recipe publication.
use super::owner_movement::{same_samples, OWNER_MOVE_STAGING_LIMIT};
use super::*;
use crate::engine::{
    raster_masks::{MaskOwner, SamplePlane},
    transform::{OwnerScope, Recipe, Target},
};

impl Document {
    pub(crate) fn owner_transform_staging_bytes(
        &self,
        scope: &OwnerScope,
    ) -> Result<usize, String> {
        scope.validate()?;
        let channels = scope
            .layer_ids()
            .len()
            .checked_mul(4)
            .and_then(|count| count.checked_add(scope.mask_ids().len()))
            .ok_or("Owner transform staging size overflows")?;
        let bytes = self
            .width
            .checked_mul(self.height)
            .and_then(|area| area.checked_mul(channels))
            .ok_or("Owner transform staging size overflows")?;
        if bytes > OWNER_MOVE_STAGING_LIMIT {
            return Err("Owner transform staging exceeds the 192 MiB limit".into());
        }
        Ok(bytes)
    }

    /// Capture exact IDs for a read plan. Existing raster link sets never widen
    /// the request; mask-only scopes retain the mask's current stable owner.
    pub(crate) fn capture_owner_transform_scope(
        &self,
        layer_ids: &[u64],
        mask_id: Option<u64>,
    ) -> Result<OwnerScope, String> {
        self.validate_owner_transform_source()?;
        let scope = if let Some(id) = mask_id {
            if !layer_ids.is_empty() {
                return Err("Raster and mask-only transform targets cannot be combined".into());
            }
            self.owner_move_target_plan(None, Some(id), false)?;
            let owner = self
                .raster_masks
                .items
                .iter()
                .find(|mask| mask.id == id)
                .unwrap()
                .owner;
            let MaskOwner::Layer { id: owner } = owner else {
                return Err("Group raster-mask geometry is not supported".into());
            };
            OwnerScope::new(&[], &[id], Some(owner))?
        } else {
            let pairs = self.owner_geometry_pairs(layer_ids)?;
            let mask_ids: Vec<_> = pairs.iter().filter_map(|(_, mask)| *mask).collect();
            OwnerScope::new(layer_ids, &mask_ids, None)?
        };
        self.owner_transform_staging_bytes(&scope)?;
        Ok(scope)
    }

    /// Revalidate captured IDs and exact current linked-mask pairings. Changes
    /// to raster link membership alone intentionally do not widen or stale it.
    pub(crate) fn validate_owner_transform_scope(
        &self,
        scope: &OwnerScope,
    ) -> Result<Vec<(u64, Option<u64>)>, String> {
        scope.validate()?;
        self.validate_owner_transform_source()?;
        let pairs = if scope.is_mask_only() {
            let id = scope.mask_ids()[0];
            self.owner_move_target_plan(None, Some(id), false)?;
            let metadata = self
                .raster_masks
                .items
                .iter()
                .find(|mask| mask.id == id)
                .unwrap();
            if metadata.owner
                != (MaskOwner::Layer {
                    id: scope.mask_owner_layer_id().unwrap(),
                })
            {
                return Err("Captured raster-mask owner has changed".into());
            }
            Vec::new()
        } else {
            let pairs = self.owner_geometry_pairs(scope.layer_ids())?;
            let current: Vec<_> = pairs.iter().filter_map(|(_, mask)| *mask).collect();
            if current.as_slice() != scope.mask_ids() {
                return Err("Captured owned linked raster-mask pairings have changed".into());
            }
            pairs
        };
        self.owner_transform_staging_bytes(scope)?;
        Ok(pairs)
    }

    /// Commit fully staged raw outputs and their exact repeat recipe under one
    /// Undo group. All fallible source/output checks precede any publication.
    pub(crate) fn apply_owner_transform(
        &mut self,
        outputs: Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)>,
        mask_only: Option<(u64, SamplePlane)>,
        recipe: Recipe,
    ) -> Result<bool, String> {
        recipe.validate()?;
        if recipe.target != Target::Owners {
            return Err("Owner publication requires an owner-scoped repeat recipe".into());
        }
        let scope = recipe
            .owners
            .ok_or("Owner transform recipe has no captured scope")?;
        self.apply_owner_outputs(outputs, mask_only, scope, Some(recipe))
    }

    /// Publish a deformation without replacing the independent affine Repeat recipe.
    pub(crate) fn apply_owner_deformation(
        &mut self,
        outputs: Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)>,
        scope: OwnerScope,
    ) -> Result<bool, String> {
        self.apply_owner_outputs(outputs, None, scope, None)
    }

    fn apply_owner_outputs(
        &mut self,
        outputs: Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)>,
        mask_only: Option<(u64, SamplePlane)>,
        scope: OwnerScope,
        recipe: Option<Recipe>,
    ) -> Result<bool, String> {
        if self.stroke.is_some() || self.mask_contact.is_some() || self.history_group.is_some() {
            return Err("Finish the current contact before transforming raster owners".into());
        }
        let pairs = self.validate_owner_transform_scope(&scope)?;
        let mut plans = Vec::with_capacity(scope.layer_ids().len());
        let mut mask_plan = None;
        if scope.is_mask_only() {
            if !outputs.is_empty()
                || mask_only.as_ref().map(|(id, _)| *id) != Some(scope.mask_ids()[0])
            {
                return Err(
                    "Mask-only publication requires exactly its captured raw mask output".into(),
                );
            }
            let (id, samples) = mask_only.as_ref().unwrap();
            samples.validate(self.width * self.height)?;
            let plan = self.owner_move_target_plan(None, Some(*id), false)?;
            let changed = !same_samples(samples, &self.mask_planes[plan.mask.unwrap()].samples);
            mask_plan = Some((plan, changed));
        } else {
            if mask_only.is_some()
                || outputs.len() != scope.layer_ids().len()
                || outputs
                    .iter()
                    .map(|(id, _, _)| *id)
                    .ne(scope.layer_ids().iter().copied())
            {
                return Err(
                    "Owner publication requires the exact captured raster output order".into(),
                );
            }
            for ((id, pixels, mask), (_, expected_mask)) in outputs.iter().zip(pairs) {
                if mask.as_ref().map(|(id, _)| *id) != expected_mask {
                    return Err(
                        "Owner transform output does not match its captured linked mask".into(),
                    );
                }
                let plan = self.owner_move_target_plan(Some(*id), expected_mask, false)?;
                self.validate_layer_transform_pixels(plan.layer.unwrap(), pixels)?;
                if let Some((_, samples)) = mask {
                    samples.validate(self.width * self.height)?;
                }
                let rgba = *pixels != self.layers[plan.layer.unwrap()].pixels;
                let a8 = mask.as_ref().is_some_and(|(_, samples)| {
                    !same_samples(samples, &self.mask_planes[plan.mask.unwrap()].samples)
                });
                plans.push((plan, rgba, a8));
            }
        }
        let recipe_changed = recipe.is_some() && self.last_transform != recipe;
        let raw_changed = plans.iter().any(|(_, rgba, a8)| *rgba || *a8)
            || mask_plan.as_ref().is_some_and(|(_, changed)| *changed);
        if !recipe_changed && !raw_changed {
            return Ok(false);
        }
        self.begin_history_group()?;
        for ((_, pixels, mask), (plan, rgba, a8)) in outputs.into_iter().zip(plans) {
            if rgba || a8 {
                self.publish_owner_move(plan, Some(pixels), mask, rgba, a8);
            }
        }
        if let Some((plan, true)) = mask_plan {
            self.publish_owner_move(plan, None, mask_only, false, true);
        }
        if recipe_changed {
            let before = self.composition();
            self.last_transform = recipe;
            self.commit_composition(before, None);
        }
        self.end_history_group()
            .expect("Owner transform opened its history group");
        Ok(true)
    }
}

#[cfg(test)]
#[path = "owner_affine_tests.rs"]
mod tests;
