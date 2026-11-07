//! Mask identity, raw A8 history and scoped region ownership. Pixel math stays native.
use super::*;
use crate::engine::raster_masks::{
    MaskLease, MaskMetadata, MaskOwner, MaskPlane, MaskState, PaintTarget, PlaneView, SamplePlane,
};
use std::sync::atomic::{AtomicU64, Ordering};

// Restoring an active checkpoint retains its cached lease, but the next contact
// must not reuse a nonce abandoned by rollback or document replacement.
static NEXT_MASK_STORAGE_EPOCH: AtomicU64 = AtomicU64::new(1);

impl Document {
    pub fn mask_state(&self) -> &MaskState {
        &self.raster_masks
    }
    pub fn has_raster_masks(&self) -> bool {
        !self.raster_masks.items.is_empty()
    }
    pub fn mask_storage_used(&self) -> bool {
        !self.raster_masks.is_default()
    }
    pub fn mask_plane(&self, id: u64) -> Result<PlaneView<'_>, String> {
        self.mask_planes
            .iter()
            .find(|plane| plane.id == id)
            .map(|plane| plane.samples.view())
            .ok_or_else(|| "Raster mask does not exist".into())
    }
    pub fn mask_content_revision(&self, id: u64) -> Result<u64, String> {
        self.mask_planes
            .iter()
            .find(|plane| plane.id == id)
            .map(|plane| plane.content_revision)
            .ok_or_else(|| "Raster mask does not exist".into())
    }
    pub fn validate_mask_storage(&self) -> Result<(), String> {
        self.validate_mask_state(&self.raster_masks)?;
        if self.mask_planes.len() != self.raster_masks.items.len()
            || self
                .mask_planes
                .iter()
                .zip(&self.raster_masks.items)
                .any(|(plane, item)| plane.id != item.id)
        {
            return Err("Raster mask metadata and raw plane identities differ".into());
        }
        for plane in &self.mask_planes {
            plane.samples.validate(self.width * self.height)?;
        }
        Ok(())
    }
    fn validate_mask_state(&self, state: &MaskState) -> Result<(), String> {
        state.validate(
            self.width,
            self.height,
            &self
                .layer_metadata
                .iter()
                .map(|item| item.id)
                .collect::<Vec<_>>(),
            &self
                .groups
                .as_ref()
                .map(|tree| tree.groups.iter().map(|group| group.id).collect::<Vec<_>>())
                .unwrap_or_default(),
        )
    }
    fn require_idle_mask_edit(&self) -> Result<(), String> {
        if self.stroke.is_some() {
            Err("Finish the current contact before changing raster masks".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn bump_mask_storage_epoch(&mut self) {
        self.mask_storage_epoch = NEXT_MASK_STORAGE_EPOCH.fetch_add(1, Ordering::Relaxed);
    }
    pub(super) fn mark_mask_plane(&mut self, id: u64) {
        let count = self.width.div_ceil(TILE) * self.height.div_ceil(TILE);
        self.replay_mask_tiles
            .extend((0..count).map(|tile| (id, tile)));
    }
    pub fn create_mask(&mut self, owner: MaskOwner, initial: u8) -> Result<u64, String> {
        self.require_idle_mask_edit()?;
        self.validate_mask_storage()?;
        let id = self.raster_masks.next_id.max(1);
        if id > u32::MAX as u64 {
            return Err("Raster mask identity limit reached".into());
        }
        let mut candidate = self.raster_masks.clone();
        candidate.next_id = id + 1;
        candidate.items.push(MaskMetadata::new(id, owner));
        self.validate_mask_state(&candidate)?;
        let prepared = self.prepare_raster_mask_plan(&candidate, true)?;
        let before = self.composition();
        let slot = self.mask_planes.len();
        self.mask_planes.push(MaskPlane {
            id,
            samples: SamplePlane::Uniform(initial),
            content_revision: 0,
        });
        self.raster_masks = candidate;
        self.bump_mask_storage_epoch();
        self.mark_mask_plane(id);
        self.install_group_plan(prepared);
        let mut edit = Self::composition_edit(before, None, self.active);
        edit.mask_change = Some(MaskChange::Toggle {
            slot,
            removed: None,
        });
        self.stroke = Some(edit);
        self.finish();
        self.invalidate();
        Ok(id)
    }
    pub fn delete_mask(&mut self, id: u64) -> Result<(), String> {
        self.require_idle_mask_edit()?;
        self.validate_mask_storage()?;
        let slot = self
            .mask_planes
            .iter()
            .position(|plane| plane.id == id)
            .ok_or("Raster mask does not exist")?;
        let mut candidate = self.raster_masks.clone();
        candidate.items.remove(slot);
        if candidate.target == (PaintTarget::Mask { id }) {
            candidate.target = PaintTarget::LayerPixels;
        }
        self.validate_mask_state(&candidate)?;
        let prepared = self.prepare_raster_mask_plan(&candidate, true)?;
        let before = self.composition();
        let removed = self.mask_planes.remove(slot);
        self.remove_mask_effect_cache(id);
        self.raster_masks = candidate;
        self.replay_mask_tiles.retain(|(mask, _)| *mask != id);
        self.bump_mask_storage_epoch();
        self.install_group_plan(prepared);
        let mut edit = Self::composition_edit(before, None, self.active);
        edit.mask_change = Some(MaskChange::Toggle {
            slot,
            removed: Some(Arc::new(removed)),
        });
        self.stroke = Some(edit);
        self.finish();
        self.invalidate();
        Ok(())
    }
    pub fn update_mask_metadata(&mut self, metadata: MaskMetadata) -> Result<bool, String> {
        self.require_idle_mask_edit()?;
        self.validate_mask_storage()?;
        let slot = self
            .raster_masks
            .items
            .iter()
            .position(|item| item.id == metadata.id)
            .ok_or("Raster mask does not exist")?;
        if self.raster_masks.items[slot].owner != metadata.owner {
            return Err("Mask property edits cannot change ownership".into());
        }
        if self.raster_masks.items[slot] == metadata {
            return Ok(false);
        }
        let mut candidate = self.raster_masks.clone();
        candidate.items[slot] = metadata;
        self.validate_mask_state(&candidate)?;
        let prepared = self.prepare_raster_mask_plan(&candidate, true)?;
        let before = self.composition();
        self.raster_masks = candidate;
        self.invalidate_mask_effect_id(self.raster_masks.items[slot].id);
        self.install_group_plan(prepared);
        self.commit_composition(before, None);
        Ok(true)
    }
    /// Target selection is recorded metadata but has no separate pixel Undo entry.
    pub fn set_mask_target(&mut self, target: PaintTarget) -> Result<(), String> {
        self.require_idle_mask_edit()?;
        let mut candidate = self.raster_masks.clone();
        candidate.target = target;
        self.validate_mask_state(&candidate)?;
        self.raster_masks = candidate;
        Ok(())
    }
    pub fn replace_mask_plane(&mut self, id: u64, samples: SamplePlane) -> Result<bool, String> {
        self.require_idle_mask_edit()?;
        self.validate_mask_storage()?;
        samples.validate(self.width * self.height)?;
        let slot = self
            .mask_planes
            .iter()
            .position(|plane| plane.id == id)
            .ok_or("Raster mask does not exist")?;
        let current = self.mask_planes[slot].samples.view();
        let equal = match (current, samples.view()) {
            (PlaneView::Uniform(a), PlaneView::Uniform(b)) => a == b,
            (PlaneView::Dense(a), PlaneView::Dense(b)) => a == b,
            (PlaneView::Uniform(value), PlaneView::Dense(data))
            | (PlaneView::Dense(data), PlaneView::Uniform(value)) => {
                data.iter().all(|sample| *sample == value)
            }
        };
        if equal {
            return Ok(false);
        }
        self.prepare_raster_mask_plan(&self.raster_masks, true)?;
        let before = self.composition();
        let revision = self.mask_planes[slot].content_revision.wrapping_add(1);
        let saved = std::mem::replace(
            &mut self.mask_planes[slot],
            MaskPlane {
                id,
                samples,
                content_revision: revision,
            },
        );
        self.invalidate_mask_effect_id(id);
        self.bump_mask_storage_epoch();
        self.mark_mask_plane(id);
        let mut edit = Self::composition_edit(before, None, self.active);
        edit.mask_change = Some(MaskChange::Replace {
            slot,
            saved: Arc::new(saved),
        });
        self.stroke = Some(edit);
        self.finish();
        self.invalidate();
        Ok(true)
    }
    fn mask_owner_visible(&self, owner: MaskOwner) -> bool {
        match owner {
            MaskOwner::Layer { id } => self
                .layer_index(id)
                .is_some_and(|index| self.layer_visible(index)),
            MaskOwner::Group { mut id } => {
                let Some(tree) = &self.groups else {
                    return false;
                };
                for _ in 0..=crate::engine::groups::MAX_DEPTH {
                    if id == 0 {
                        return true;
                    }
                    if tree.group(id).is_err() || !tree.group(id).unwrap().visible {
                        return false;
                    }
                    let Ok(parent) = tree.parent(crate::engine::groups::GroupItem::group(id))
                    else {
                        return false;
                    };
                    id = parent;
                }
                false
            }
        }
    }
    pub fn begin_mask_stroke(&mut self, id: u64) -> Result<MaskLease, String> {
        self.require_idle_mask_edit()?;
        self.validate_mask_storage()?;
        self.prepare_raster_mask_plan(&self.raster_masks, true)?;
        let slot = self
            .mask_planes
            .iter()
            .position(|plane| plane.id == id)
            .ok_or("Raster mask does not exist")?;
        if !self.mask_owner_visible(self.raster_masks.items[slot].owner) {
            return Err("Raster mask owner is hidden".into());
        }
        self.prepare_mask_effects()?;
        self.prepare_mask_contact_storage(slot)?;
        self.bump_mask_storage_epoch();
        let lease = MaskLease {
            document_identity: self.identity,
            id,
            slot,
            storage_epoch: self.mask_storage_epoch,
        };
        self.stroke = Some(Edit {
            layer: self.active,
            tiles: BTreeMap::new(),
            mask_tiles: BTreeMap::new(),
            mask_change: None,
            composition: None,
            metadata_bytes: 0,
            layer_change: None,
            group: None,
        });
        self.mask_contact = Some(lease);
        Ok(lease)
    }
    pub(super) fn valid_mask_lease(&self, lease: MaskLease) -> bool {
        lease.document_identity == self.identity
            && lease.storage_epoch == self.mask_storage_epoch
            && self.mask_contact == Some(lease)
            && self.stroke.is_some()
            && self.mask_planes.get(lease.slot).is_some_and(|plane| {
                plane.id == lease.id && matches!(plane.samples, SamplePlane::Dense(_))
            })
    }
    pub fn prepare_mask_region(
        &mut self,
        lease: MaskLease,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Result<(), String> {
        if !self.valid_mask_lease(lease)
            || width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|end| end > self.width)
            || y.checked_add(height).is_none_or(|end| end > self.height)
        {
            return Err("Invalid or stale raster mask region lease".into());
        }
        let columns = self.width.div_ceil(TILE);
        for ty in y / TILE..=(y + height - 1) / TILE {
            for tx in x / TILE..=(x + width - 1) / TILE {
                let tile = ty * columns + tx;
                self.replay_mask_tiles.insert((lease.id, tile));
                if self
                    .stroke
                    .as_ref()
                    .unwrap()
                    .mask_tiles
                    .contains_key(&(lease.id, tile))
                {
                    continue;
                }
                let mut saved = Vec::with_capacity(TILE * TILE);
                let SamplePlane::Dense(samples) = &self.mask_planes[lease.slot].samples else {
                    unreachable!();
                };
                for row in ty * TILE..((ty + 1) * TILE).min(self.height) {
                    saved.extend_from_slice(
                        &samples[row * self.width + tx * TILE
                            ..row * self.width + ((tx + 1) * TILE).min(self.width)],
                    );
                }
                self.stroke
                    .as_mut()
                    .unwrap()
                    .mask_tiles
                    .insert((lease.id, tile), Arc::new(saved));
            }
        }
        self.mask_planes[lease.slot].content_revision = self.mask_planes[lease.slot]
            .content_revision
            .wrapping_add(1);
        let influence =
            self.invalidate_mask_effect_region(lease.slot, [x, y, x + width, y + height]);
        let metadata = &self.raster_masks.items[lease.slot];
        if metadata.enabled && metadata.density > 0 {
            self.damage(influence);
        }
        Ok(())
    }
    /// The returned descriptor is valid only for the current synchronous loan.
    /// Callers must not retain its pointer or reenter the document during a transform.
    pub fn mask_region(
        &mut self,
        lease: MaskLease,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Result<micopi_plugin_api::A8Pixels, String> {
        self.prepare_mask_region(lease, x, y, width, height)?;
        let SamplePlane::Dense(samples) = &mut self.mask_planes[lease.slot].samples else {
            unreachable!();
        };
        Ok(micopi_plugin_api::A8Pixels {
            data: unsafe { samples.as_mut_ptr().add(y * self.width + x) },
            len: (height - 1) * self.width + width,
            width: width as u32,
            height: height as u32,
            stride: self.width as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        })
    }
    pub(super) fn swap_mask_change(&mut self, change: &mut MaskChange) {
        match change {
            MaskChange::Toggle { slot, removed } => {
                if let Some(plane) = removed.take() {
                    let id = plane.id;
                    self.mask_planes.insert(*slot, Arc::unwrap_or_clone(plane));
                    self.mark_mask_plane(id);
                    self.invalidate_mask_effect_id(id);
                } else {
                    let plane = self.mask_planes.remove(*slot);
                    self.remove_mask_effect_cache(plane.id);
                    self.replay_mask_tiles.retain(|(mask, _)| *mask != plane.id);
                    self.invalidate_mask_effect_id(plane.id);
                    *removed = Some(Arc::new(plane));
                }
            }
            MaskChange::Replace { slot, saved } => {
                let id = self.mask_planes[*slot].id;
                let current = std::mem::replace(
                    &mut self.mask_planes[*slot],
                    MaskPlane {
                        id,
                        samples: SamplePlane::Uniform(0),
                        content_revision: 0,
                    },
                );
                let old = std::mem::replace(saved, Arc::new(current));
                self.mask_planes[*slot] = Arc::unwrap_or_clone(old);
                self.mark_mask_plane(id);
                self.invalidate_mask_effect_id(id);
            }
            MaskChange::Structural(change) => {
                // Layer storage has already been swapped, but its composition
                // metadata has not. Move the matching planes without validating
                // owner identities in this deliberately transient state.
                let change = Arc::make_mut(change);
                let mut removed = Vec::with_capacity(change.current.len());
                for &slot in change.current.iter().rev() {
                    let plane = self.mask_planes.remove(slot);
                    self.remove_mask_effect_cache(plane.id);
                    self.replay_mask_tiles.retain(|(mask, _)| *mask != plane.id);
                    removed.push((slot, Arc::new(plane)));
                }
                removed.reverse();
                change.current = change.saved.iter().map(|(slot, _)| *slot).collect();
                for (slot, plane) in std::mem::replace(&mut change.saved, removed) {
                    let id = plane.id;
                    self.mask_planes.insert(slot, Arc::unwrap_or_clone(plane));
                    self.mark_mask_plane(id);
                    self.invalidate_mask_effect_id(id);
                }
            }
        }
        self.mask_contact = None;
        self.bump_mask_storage_epoch();
    }
    pub(super) fn swap_mask_tiles(&mut self, tiles: &mut BTreeMap<(u64, usize), Arc<Vec<u8>>>) {
        let columns = self.width.div_ceil(TILE);
        for (&(id, tile), saved) in tiles {
            let slot = self
                .mask_planes
                .iter()
                .position(|plane| plane.id == id)
                .expect("History owns this mask");
            if let SamplePlane::Uniform(value) = self.mask_planes[slot].samples {
                self.mask_planes[slot].samples =
                    SamplePlane::Dense(vec![value; self.width * self.height]);
                self.bump_mask_storage_epoch();
            }
            let SamplePlane::Dense(samples) = &mut self.mask_planes[slot].samples else {
                unreachable!();
            };
            let old = Arc::make_mut(saved);
            let (x, y) = ((tile % columns) * TILE, (tile / columns) * TILE);
            let mut index = 0;
            for row in y..(y + TILE).min(self.height) {
                for column in x..(x + TILE).min(self.width) {
                    std::mem::swap(&mut samples[row * self.width + column], &mut old[index]);
                    index += 1;
                }
            }
            self.mask_planes[slot].content_revision =
                self.mask_planes[slot].content_revision.wrapping_add(1);
            self.replay_mask_tiles.insert((id, tile));
            self.invalidate_mask_effect_region(
                slot,
                [
                    x,
                    y,
                    (x + TILE).min(self.width),
                    (y + TILE).min(self.height),
                ],
            );
        }
    }
    /// Restore verified byte cells without invoking a renderer or copying a whole
    /// dense plane through intermediate storage. Metadata was restored first.
    pub(crate) fn restore_mask_storage(
        &mut self,
        state: &MaskState,
        mut uniform: impl FnMut(u64) -> Option<u8>,
        mut fill_dense: impl FnMut(u64, &mut [u8]),
    ) {
        let mut old = std::mem::take(&mut self.mask_planes);
        for item in &state.items {
            let prior = old
                .iter()
                .position(|plane| plane.id == item.id)
                .map(|index| old.swap_remove(index));
            let revision = prior
                .as_ref()
                .map_or(0, |plane| plane.content_revision.wrapping_add(1));
            let samples = if let Some(value) = uniform(item.id) {
                SamplePlane::Uniform(value)
            } else {
                let mut samples = match prior.map(|plane| plane.samples) {
                    Some(SamplePlane::Dense(samples)) => samples,
                    _ => Vec::new(),
                };
                samples.resize(self.width * self.height, 0);
                fill_dense(item.id, &mut samples);
                SamplePlane::Dense(samples)
            };
            self.mask_planes.push(MaskPlane {
                id: item.id,
                samples,
                content_revision: revision,
            });
        }
        self.mask_contact = None;
        self.raster_masks = state.clone();
        self.retain_mask_effect_caches();
        self.invalidate_mask_effects();
        self.bump_mask_storage_epoch();
        self.replay_mask_tiles.clear();
        let ids = self
            .mask_planes
            .iter()
            .map(|plane| plane.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.mark_mask_plane(id);
        }
    }
}
