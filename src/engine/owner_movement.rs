//! Preflighted whole-domain owner movement; native plugins stage pixel math.
use super::*;
use crate::engine::raster_masks::{MaskOwner, MaskPlane, SamplePlane};

pub(crate) const OWNER_MOVE_STAGING_LIMIT: usize = 192 * 1024 * 1024;

pub(super) struct OwnerMove {
    pub(super) layer: Option<usize>,
    pub(super) mask: Option<usize>,
}

pub(super) fn same_samples(left: &SamplePlane, right: &SamplePlane) -> bool {
    match (left, right) {
        (SamplePlane::Uniform(a), SamplePlane::Uniform(b)) => a == b,
        (SamplePlane::Dense(a), SamplePlane::Dense(b)) => a == b,
        (SamplePlane::Uniform(value), SamplePlane::Dense(samples))
        | (SamplePlane::Dense(samples), SamplePlane::Uniform(value)) => {
            samples.iter().all(|sample| sample == value)
        }
    }
}

impl Document {
    /// Immutable admission shared with selection-outline movement, which must
    /// preserve raw owners without inheriting any unrelated owner's locks.
    pub(crate) fn validate_owner_move_source(&self) -> Result<(), String> {
        if self.stroke.is_some() || self.mask_contact.is_some() {
            return Err("Finish the current contact before moving raster owners".into());
        }
        self.validate_owner_transform_source()
    }

    /// Immutable source inspection may run during an admitted native contact.
    /// It neither changes contact ownership nor copies/materializes raw frames.
    pub(crate) fn validate_owner_transform_source(&self) -> Result<(), String> {
        self.validate_canvas_geometry_frame(self.width, self.height)?;
        let area = self.width * self.height;
        if self.layers.iter().any(|layer| layer.pixels.len() != area)
            || self.composite.len() != area
        {
            return Err("Owner movement source storage does not match the canvas".into());
        }
        let composition = self.composition();
        composition.validate_layer_metadata()?;
        composition.validate_layer_selection(self.active)?;
        self.selection.validate(self.width, self.height)?;
        self.validate_mask_storage()?;
        // The complete source includes hidden/disabled modes and mask effects.
        // No source plane or effective coverage is copied/materialized here.
        self.ensure_graph_renderer()
    }

    fn owner_move_plan(
        &self,
        layer_id: Option<u64>,
        mask_id: Option<u64>,
    ) -> Result<OwnerMove, String> {
        self.validate_owner_move_source()?;
        self.owner_move_target_plan(layer_id, mask_id, true)
    }

    pub(super) fn owner_move_target_plan(
        &self,
        layer_id: Option<u64>,
        mask_id: Option<u64>,
        require_active: bool,
    ) -> Result<OwnerMove, String> {
        if layer_id.is_none() && mask_id.is_none() {
            return Err("Owner movement requires a raster layer or raster mask".into());
        }
        let mask = mask_id
            .map(|id| {
                self.raster_masks
                    .items
                    .iter()
                    .position(|item| item.id == id)
                    .ok_or("Raster mask does not exist")
            })
            .transpose()?;
        let (layer, owner) = if let Some(id) = layer_id {
            let index = self.layer_index(id).ok_or("Layer does not exist")?;
            if require_active && index != self.active {
                return Err("Whole-layer movement requires the active raster layer".into());
            }
            if self.pixel_locked(index) {
                return Err("Unlock layer pixels before moving them".into());
            }
            let linked = self
                .raster_masks
                .items
                .iter()
                .position(|item| item.linked && item.owner == (MaskOwner::Layer { id }));
            if mask != linked {
                return Err("Layer movement requires only its owned linked raster mask".into());
            }
            (Some(index), index)
        } else {
            let slot = mask.ok_or("Mask movement requires a raster mask")?;
            let id = match self.raster_masks.items[slot].owner {
                MaskOwner::Layer { id } => id,
                MaskOwner::Group { .. } => {
                    return Err("Group raster-mask geometry is not supported".into());
                }
                MaskOwner::Adjustment { .. } => {
                    return Err("Adjustment raster-mask geometry is not supported".into())
                }
            };
            (
                None,
                self.layer_index(id)
                    .ok_or("Raster mask owner does not exist")?,
            )
        };
        if self.position_locked(owner) {
            return Err("Unlock layer position before moving its geometry".into());
        }
        // Derive inherited visibility from the validated source tree rather than
        // trusting a cache if engine clients have restored source metadata.
        let inherited = self.groups.as_ref().map_or(true, |tree| {
            let ids: Vec<_> = self.layer_metadata.iter().map(|item| item.id).collect();
            tree.effective_visibility(&ids)[owner]
        });
        if !self.layers[owner].visible || !inherited {
            return Err("Make the raster owner visible before moving it".into());
        }
        Ok(OwnerMove { layer, mask })
    }

    /// Metadata/storage/capability admission before native output allocation.
    /// A layer target names the active raster and its exact linked companion;
    /// a mask-only target names one raster-owned plane independently of active.
    pub(crate) fn validate_owner_move(
        &self,
        layer_id: Option<u64>,
        mask_id: Option<u64>,
    ) -> Result<(), String> {
        self.owner_move_plan(layer_id, mask_id).map(|_| ())
    }

    /// Dense worst-case output storage, checked before native staging. Source
    /// uniform A8 storage does not reduce the possible dense output cost.
    pub(crate) fn owner_move_staging_bytes(
        &self,
        layer_count: usize,
        mask_count: usize,
    ) -> Result<usize, String> {
        if !(1..=MAX_LAYERS).contains(&layer_count) || mask_count > layer_count {
            return Err("Owner movement requires between one and sixteen raster outputs".into());
        }
        let bytes = self
            .width
            .checked_mul(self.height)
            .and_then(|area| {
                layer_count
                    .checked_mul(4)
                    .and_then(|rgba| rgba.checked_add(mask_count))
                    .and_then(|channels| area.checked_mul(channels))
            })
            .ok_or("Owner movement staging size overflows")?;
        if bytes > OWNER_MOVE_STAGING_LIMIT {
            return Err("Owner movement staging exceeds the 192 MiB limit".into());
        }
        Ok(bytes)
    }

    /// Resolve explicit raster identities in caller order. This never expands
    /// links, changes the active layer or temporarily changes paint targets.
    pub(crate) fn validate_owner_moves(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Option<u64>)>, String> {
        self.validate_owner_move_source()?;
        if self.history_group.is_some() {
            return Err("Finish the current layer history group before moving owners".into());
        }
        self.owner_geometry_pairs(ids)
    }
    /// Read-only owner admission for cold plans during an existing contact.
    /// No contact is completed and no raster/effective plane is materialized.
    pub(crate) fn inspect_owner_moves(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Option<u64>)>, String> {
        self.validate_owner_transform_source()?;
        self.owner_geometry_pairs(ids)
    }

    /// Pair/lock/budget validation after the caller's immutable source checks.
    /// Contact admission belongs to the operation, not read-only planning.
    pub(super) fn owner_geometry_pairs(
        &self,
        ids: &[u64],
    ) -> Result<Vec<(u64, Option<u64>)>, String> {
        if ids.is_empty()
            || ids.len() > MAX_LAYERS
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| self.layer_index(*id).is_none() || ids[..i].contains(id))
        {
            return Err("Owner movement requires one to sixteen unique existing raster IDs".into());
        }
        let mut pairs = Vec::with_capacity(ids.len());
        for id in ids {
            let mask = self
                .raster_masks
                .items
                .iter()
                .find(|item| item.linked && item.owner == (MaskOwner::Layer { id: *id }))
                .map(|item| item.id);
            self.owner_move_target_plan(Some(*id), mask, false)?;
            pairs.push((*id, mask));
        }
        self.owner_move_staging_bytes(
            pairs.len(),
            pairs.iter().filter(|(_, mask)| mask.is_some()).count(),
        )?;
        Ok(pairs)
    }

    /// Stage and validate every owner before publishing any raw child edit.
    /// Each child retains its own raster tile namespace and one optional A8
    /// replacement; the existing history group exposes a single Undo step.
    pub(crate) fn apply_owner_moves(
        &mut self,
        outputs: Vec<(u64, Vec<u32>, Option<(u64, SamplePlane)>)>,
    ) -> Result<bool, String> {
        let ids: Vec<_> = outputs.iter().map(|(id, _, _)| *id).collect();
        let pairs = self.validate_owner_moves(&ids)?;
        let mut plans = Vec::with_capacity(outputs.len());
        for ((id, pixels, mask), (_, expected_mask)) in outputs.iter().zip(&pairs) {
            if mask.as_ref().map(|(id, _)| id) != expected_mask.as_ref() {
                return Err("Layer movement requires only its owned linked raster mask".into());
            }
            let plan = self.owner_move_target_plan(Some(*id), *expected_mask, false)?;
            self.validate_layer_transform_pixels(plan.layer.unwrap(), pixels)?;
            if let Some((_, samples)) = mask {
                samples.validate(self.width * self.height)?;
            }
            let pixels_changed = *pixels != self.layers[plan.layer.unwrap()].pixels;
            let mask_changed = mask.as_ref().is_some_and(|(_, samples)| {
                !same_samples(samples, &self.mask_planes[plan.mask.unwrap()].samples)
            });
            plans.push((plan, pixels_changed, mask_changed));
        }
        if plans.iter().all(|(_, rgba, mask)| !*rgba && !*mask) {
            return Ok(false);
        }
        self.begin_history_group()?;
        for ((_, pixels, mask), (plan, pixels_changed, mask_changed)) in
            outputs.into_iter().zip(plans)
        {
            if pixels_changed || mask_changed {
                self.publish_owner_move(plan, Some(pixels), mask, pixels_changed, mask_changed);
            }
        }
        self.end_history_group()
            .expect("Owner movement opened its history group");
        Ok(true)
    }

    /// Publish fully staged RGBA/A8 under one raw-storage history edit. Selections,
    /// paint target, owner metadata and repeat recipe are deliberately untouched.
    pub(crate) fn apply_owner_move(
        &mut self,
        layer_id: Option<u64>,
        pixels: Option<Vec<u32>>,
        mask: Option<(u64, SamplePlane)>,
    ) -> Result<bool, String> {
        if layer_id.is_some() != pixels.is_some() {
            return Err("Layer movement requires exactly one staged raster output".into());
        }
        let plan = self.owner_move_plan(layer_id, mask.as_ref().map(|(id, _)| *id))?;
        if let Some(pixels) = &pixels {
            self.validate_layer_transform_pixels(plan.layer.unwrap(), pixels)?;
        }
        if let Some((_, samples)) = &mask {
            samples.validate(self.width * self.height)?;
        }
        let pixels_changed = pixels
            .as_ref()
            .is_some_and(|pixels| *pixels != self.layers[plan.layer.unwrap()].pixels);
        let mask_changed = mask.as_ref().is_some_and(|(_, samples)| {
            !same_samples(samples, &self.mask_planes[plan.mask.unwrap()].samples)
        });
        if !pixels_changed && !mask_changed {
            return Ok(false);
        }
        self.publish_owner_move(plan, pixels, mask, pixels_changed, mask_changed);
        Ok(true)
    }

    pub(super) fn publish_owner_move(
        &mut self,
        plan: OwnerMove,
        pixels: Option<Vec<u32>>,
        mask: Option<(u64, SamplePlane)>,
        pixels_changed: bool,
        mask_changed: bool,
    ) {
        // All fallible admission and staging checks precede publication. This
        // edit owns only changed raster tiles and the one replaced raw plane.
        self.stroke = Some(Edit {
            layer: plan.layer.unwrap_or(self.active),
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
            layer_change: None,
            group: None,
        });
        if pixels_changed {
            self.write_layer_region(
                plan.layer.unwrap(),
                0,
                0,
                self.width,
                self.height,
                pixels.as_deref().unwrap(),
            );
        }
        if mask_changed {
            let (id, samples) = mask.unwrap();
            let slot = plan.mask.unwrap();
            let revision = self.mask_planes[slot].content_revision.wrapping_add(1);
            let saved = std::mem::replace(
                &mut self.mask_planes[slot],
                MaskPlane {
                    id,
                    samples,
                    content_revision: revision,
                },
            );
            self.stroke.as_mut().unwrap().mask_change = Some(MaskChange::Replace {
                slot,
                saved: Arc::new(saved),
            });
            self.invalidate_mask_effect_id(id);
            self.bump_mask_storage_epoch();
            self.mark_mask_plane(id);
            self.invalidate();
        }
        self.finish();
    }
}

#[cfg(test)]
#[path = "owner_movement_tests.rs"]
mod tests;
