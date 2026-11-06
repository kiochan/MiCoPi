//! Derived A8 coverage. Raw samples, history and replay never own these buffers.
use super::*;
use crate::engine::raster_masks::{MaskMetadata, PlaneView, SamplePlane};
use micopi_plugin_api as abi;

pub(super) struct CoverageCache {
    id: u64,
    width: usize,
    height: usize,
    density: u8,
    radius: u16,
    revision: u64,
    samples: Option<SamplePlane>,
    dirty: Option<[usize; 4]>,
}
impl CoverageCache {
    fn new(id: u64) -> Self {
        Self {
            id,
            width: 0,
            height: 0,
            density: 255,
            radius: 0,
            revision: 0,
            samples: None,
            dirty: None,
        }
    }
}
fn needed(metadata: &MaskMetadata) -> bool {
    metadata.density != 255 || metadata.feather_radius_px != 0
}
fn union(before: Option<[usize; 4]>, after: [usize; 4]) -> [usize; 4] {
    before.map_or(after, |before| {
        [
            before[0].min(after[0]),
            before[1].min(after[1]),
            before[2].max(after[2]),
            before[3].max(after[3]),
        ]
    })
}
pub(super) fn descriptor(view: PlaneView<'_>, width: usize) -> abi::CompositeMask {
    match view {
        PlaneView::Uniform(value) => abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: value as u32,
        },
        PlaneView::Dense(samples) => abi::CompositeMask {
            data: samples.as_ptr(),
            len: samples.len(),
            stride: width as u32,
            constant: 0,
        },
    }
}
impl Document {
    /// Materialize a uniform contact using its already known effective constant.
    /// Reserving native capacity does not evaluate the feather kernel or fill
    /// native scratch; both raw/derived byte allocations remain cold here.
    pub(super) fn prepare_mask_contact_storage(&mut self, slot: usize) -> Result<(), String> {
        let metadata = &self.raster_masks.items[slot];
        if metadata.feather_radius_px > 0 && metadata.density > 0 {
            self.renderer
                .as_ref()
                .ok_or("Compositor DLL is not loaded")?
                .reserve_mask_effects(
                    self.width as u32,
                    self.height as u32,
                    metadata.feather_radius_px as u32,
                )?;
        }
        let SamplePlane::Uniform(value) = self.mask_planes[slot].samples else {
            return Ok(());
        };
        let effective = if needed(metadata) && metadata.density > 0 {
            match self.mask_effects[slot].samples.as_ref() {
                Some(SamplePlane::Uniform(value)) => Some(*value),
                _ => return Err("Uniform mask effective coverage has not been prepared".into()),
            }
        } else {
            None
        };
        let area = self.width * self.height;
        let raw = vec![value; area];
        let derived = effective.map(|value| vec![value; area]);
        self.mask_planes[slot].samples = SamplePlane::Dense(raw);
        if let Some(samples) = derived {
            let cache = &mut self.mask_effects[slot];
            cache.samples = Some(SamplePlane::Dense(samples));
            cache.dirty = None;
            cache.revision = self.mask_planes[slot].content_revision;
        }
        Ok(())
    }
    pub(super) fn retain_mask_effect_caches(&mut self) {
        let metadata = &self.raster_masks.items;
        self.mask_effects
            .retain(|cache| metadata.iter().any(|item| item.id == cache.id));
    }
    pub(super) fn remove_mask_effect_cache(&mut self, id: u64) {
        self.mask_effects.retain(|cache| cache.id != id);
    }
    pub(super) fn invalidate_mask_effect_id(&mut self, id: u64) {
        if let Some(cache) = self.mask_effects.iter_mut().find(|cache| cache.id == id) {
            if cache.samples.is_some() {
                cache.dirty = Some([0, 0, self.width, self.height]);
            }
        }
    }
    pub(super) fn invalidate_mask_effects(&mut self) {
        let full = [0, 0, self.width, self.height];
        for cache in &mut self.mask_effects {
            if cache.samples.is_some() {
                cache.dirty = Some(full);
            }
        }
    }
    /// The six native box passes have total support R along either axis.
    pub(super) fn invalidate_mask_effect_region(
        &mut self,
        slot: usize,
        raw: [usize; 4],
    ) -> [usize; 4] {
        let radius = self.raster_masks.items[slot].feather_radius_px as usize;
        let influence = [
            raw[0].saturating_sub(radius),
            raw[1].saturating_sub(radius),
            raw[2].saturating_add(radius).min(self.width),
            raw[3].saturating_add(radius).min(self.height),
        ];
        if let Some(cache) = self.mask_effects.get_mut(slot) {
            if cache.samples.is_some() {
                cache.dirty = Some(union(cache.dirty, influence));
            }
        }
        influence
    }
    fn sync_mask_effect_storage(&mut self) -> Result<(), String> {
        let aligned = self.mask_effects.len() == self.mask_planes.len()
            && self
                .mask_effects
                .iter()
                .zip(&self.mask_planes)
                .all(|(cache, plane)| cache.id == plane.id);
        if !aligned {
            if self.stroke.is_some() {
                return Err("Prepare raster mask effects before starting the contact".into());
            }
            let mut old = std::mem::take(&mut self.mask_effects);
            for plane in &self.mask_planes {
                let cache = old
                    .iter()
                    .position(|cache| cache.id == plane.id)
                    .map(|index| old.swap_remove(index))
                    .unwrap_or_else(|| CoverageCache::new(plane.id));
                self.mask_effects.push(cache);
            }
        }
        let full = [0, 0, self.width, self.height];
        let area = self.width * self.height;
        for ((cache, plane), metadata) in self
            .mask_effects
            .iter_mut()
            .zip(&self.mask_planes)
            .zip(&self.raster_masks.items)
        {
            let parameters_changed = cache.width != self.width
                || cache.height != self.height
                || cache.density != metadata.density
                || cache.radius != metadata.feather_radius_px;
            if !needed(metadata) {
                cache.samples = None;
                cache.dirty = None;
                cache.revision = plane.content_revision;
            } else if metadata.density == 0 || matches!(plane.samples, SamplePlane::Uniform(255)) {
                // These identities do not perform byte algorithms in the host.
                cache.samples = Some(SamplePlane::Uniform(255));
                cache.dirty = None;
                cache.revision = plane.content_revision;
            } else {
                match &plane.samples {
                    SamplePlane::Uniform(_) => {
                        if !matches!(cache.samples, Some(SamplePlane::Uniform(_))) {
                            cache.samples = Some(SamplePlane::Uniform(0));
                            cache.dirty = Some(full);
                        }
                    }
                    SamplePlane::Dense(_) => {
                        let dense = matches!(&cache.samples, Some(SamplePlane::Dense(data)) if data.len() == area);
                        if !dense {
                            if self.stroke.is_some() {
                                return Err(
                                    "Prepare raster mask effects before starting the contact"
                                        .into(),
                                );
                            }
                            cache.samples = Some(SamplePlane::Dense(vec![0; area]));
                            cache.dirty = Some(full);
                        }
                    }
                }
                if parameters_changed
                    || cache.revision != plane.content_revision && cache.dirty.is_none()
                {
                    cache.dirty = Some(full);
                }
            }
            cache.width = self.width;
            cache.height = self.height;
            cache.density = metadata.density;
            cache.radius = metadata.feather_radius_px;
        }
        Ok(())
    }
    /// Cold admission/Down boundary. Dense coverage is bounded by the same
    /// 64-MiB logical plane budget; native scratch is reused by the compositor.
    /// Call before a normal RGBA contact, including after verified restoration.
    pub fn prepare_mask_effects(&mut self) -> Result<(), String> {
        if self.mask_planes.len() != self.raster_masks.items.len() {
            return Err("Raster mask metadata and raw plane identities differ".into());
        }
        if self
            .mask_planes
            .iter()
            .zip(&self.raster_masks.items)
            .any(|(plane, metadata)| plane.id != metadata.id)
        {
            return Err("Raster mask metadata and raw plane identities differ".into());
        }
        if self.mask_planes.is_empty() {
            self.mask_effects.clear();
            return Ok(());
        }
        self.ensure_raster_mask_renderer()?;
        self.ensure_raster_mask_properties(self.mask_state())?;
        self.sync_mask_effect_storage()?;
        let renderer = self.renderer.as_ref().unwrap();
        for ((cache, plane), metadata) in self
            .mask_effects
            .iter_mut()
            .zip(&self.mask_planes)
            .zip(&self.raster_masks.items)
        {
            let Some([x0, y0, x1, y1]) = cache.dirty else {
                continue;
            };
            let source = descriptor(plane.samples.view(), self.width);
            let Some(samples) = cache.samples.as_mut() else {
                continue;
            };
            let mut output = match samples {
                SamplePlane::Uniform(value) => abi::A8Pixels {
                    data: value,
                    len: 1,
                    width: 1,
                    height: 1,
                    stride: 1,
                    origin_x: 0,
                    origin_y: 0,
                },
                SamplePlane::Dense(data) => abi::A8Pixels {
                    data: unsafe { data.as_mut_ptr().add(y0 * self.width + x0) },
                    len: (y1 - y0 - 1) * self.width + x1 - x0,
                    width: (x1 - x0) as u32,
                    height: (y1 - y0) as u32,
                    stride: self.width as u32,
                    origin_x: x0 as u32,
                    origin_y: y0 as u32,
                },
            };
            renderer.mask_effect_region(
                &abi::MaskEffectRequest {
                    flags: 0,
                    density: metadata.density as u32,
                    feather_radius: metadata.feather_radius_px as u32,
                    reserved: 0,
                },
                &source,
                self.width as u32,
                self.height as u32,
                &mut output,
            )?;
            cache.revision = plane.content_revision;
            cache.dirty = None;
        }
        Ok(())
    }
    pub(super) fn effective_mask_view(&self, slot: usize) -> Result<PlaneView<'_>, String> {
        let metadata = &self.raster_masks.items[slot];
        if !metadata.enabled {
            return Ok(PlaneView::Uniform(255));
        }
        if !needed(metadata) {
            return Ok(self.mask_planes[slot].samples.view());
        }
        let cache = self
            .mask_effects
            .get(slot)
            .filter(|cache| cache.id == metadata.id && cache.dirty.is_none())
            .ok_or("Raster mask effects have not been prepared")?;
        cache
            .samples
            .as_ref()
            .map(SamplePlane::view)
            .ok_or_else(|| "Raster mask effective coverage is unavailable".into())
    }
    /// Cold effective sampling for apply/selection operations. Returned samples
    /// are borrowed coverage, never raw storage or serialized document metadata.
    pub fn effective_mask_plane(&mut self, id: u64) -> Result<PlaneView<'_>, String> {
        let slot = self
            .mask_planes
            .iter()
            .position(|plane| plane.id == id)
            .ok_or("Raster mask does not exist")?;
        self.prepare_mask_effects()?;
        self.effective_mask_view(slot)
    }
}

#[cfg(test)]
#[path = "mask_effects_tests.rs"]
mod tests;
