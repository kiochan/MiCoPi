//! Isolated whole-owner preview: borrow untouched storage and derived caches.
use super::*;
use crate::engine::raster_masks::{PlaneView, SamplePlane};
use micopi_plugin_api as abi;

const COMPOSITE_LIMIT: usize = 64 * 1024 * 1024;
const EFFECT_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct OwnerPreviewCost {
    pub composite_bytes: usize,
    pub effect_bytes: usize,
    pub extra_bytes: usize,
}

enum PreparedMask<'a> {
    Borrowed(PlaneView<'a>),
    Owned(SamplePlane),
}
impl PreparedMask<'_> {
    fn view(&self) -> PlaneView<'_> {
        match self {
            Self::Borrowed(view) => *view,
            Self::Owned(plane) => plane.view(),
        }
    }
}
fn identity(metadata: &crate::engine::raster_masks::MaskMetadata) -> bool {
    metadata.density == 255 && metadata.feather_radius_px == 0
}

impl Document {
    fn validate_owner_preview_source(&self) -> Result<usize, String> {
        self.validate_canvas_geometry_frame(self.width, self.height)?;
        let area = self
            .width
            .checked_mul(self.height)
            .ok_or("Owner preview dimensions overflow")?;
        if self.layers.iter().any(|layer| layer.pixels.len() != area)
            || self.composite.len() != area
        {
            return Err("Owner preview source storage does not match the canvas".into());
        }
        let composition = self.composition();
        composition.validate_layer_metadata()?;
        composition.validate_layer_selection(self.active)?;
        self.selection.validate(self.width, self.height)?;
        self.validate_mask_storage()?;
        self.ensure_graph_renderer()?;
        Ok(area)
    }

    /// Read-only worst-case extra storage. Changed masks may become dense even
    /// when their current source is uniform; untouched valid effects are borrowed.
    pub(crate) fn owner_preview_cost(&self, mask_ids: &[u64]) -> Result<OwnerPreviewCost, String> {
        let area = self.validate_owner_preview_source()?;
        if mask_ids.len() > crate::engine::raster_masks::MAX_MASKS
            || mask_ids.iter().enumerate().any(|(index, id)| {
                mask_ids[..index].contains(id)
                    || !self
                        .raster_masks
                        .items
                        .iter()
                        .any(|metadata| metadata.id == *id)
            })
        {
            return Err("Owner preview requires unique existing raster mask IDs".into());
        }
        let composite_bytes = area
            .checked_mul(4)
            .ok_or("Owner preview composite cost overflow")?;
        let mut effect_bytes = 0usize;
        for (slot, metadata) in self.raster_masks.items.iter().enumerate() {
            if !metadata.enabled || metadata.density == 0 || identity(metadata) {
                continue;
            }
            let changed = mask_ids.contains(&metadata.id);
            if !changed && self.effective_mask_view(slot).is_ok() {
                continue;
            }
            let bytes = if changed {
                area
            } else {
                match self.mask_planes[slot].samples.view() {
                    PlaneView::Uniform(255) => 0,
                    PlaneView::Uniform(_) => 1,
                    PlaneView::Dense(_) => area,
                }
            };
            effect_bytes = effect_bytes
                .checked_add(bytes)
                .ok_or("Owner preview effect cost overflow")?;
        }
        if composite_bytes > COMPOSITE_LIMIT || effect_bytes > EFFECT_LIMIT {
            return Err("Transform preview exceeds its separate composite or mask-effect memory limit; disable Preview to apply".into());
        }
        Ok(OwnerPreviewCost {
            composite_bytes,
            effect_bytes,
            extra_bytes: composite_bytes + effect_bytes,
        })
    }

    /// Only the temporary surface and necessary effective overrides are owned.
    /// This never prepares/mutates Document caches, metadata, history or pixels.
    pub(crate) fn preview_owner_transform<'a>(
        &'a self,
        layers: &[(u64, &'a [u32])],
        masks: &[(u64, PlaneView<'a>)],
    ) -> Result<Vec<u32>, String> {
        let mask_ids = masks.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let _cost = self.owner_preview_cost(&mask_ids)?;
        let area = self.width * self.height;
        if layers.len() > MAX_LAYERS
            || layers
                .iter()
                .enumerate()
                .any(|(index, (id, _))| layers[..index].iter().any(|(other, _)| other == id))
        {
            return Err("Owner preview requires unique raster IDs".into());
        }
        for (id, pixels) in layers {
            let index = self
                .layer_index(*id)
                .ok_or("Owner preview layer does not exist")?;
            self.validate_layer_transform_pixels(index, pixels)?;
        }
        if masks
            .iter()
            .any(|(_, view)| matches!(view,PlaneView::Dense(samples) if samples.len()!=area))
        {
            return Err("Owner preview mask storage does not match the canvas".into());
        }
        let mut prepared = Vec::with_capacity(self.mask_planes.len());
        for (slot, metadata) in self.raster_masks.items.iter().enumerate() {
            if !metadata.enabled || metadata.density == 0 {
                prepared.push(PreparedMask::Borrowed(PlaneView::Uniform(255)));
                continue;
            }
            let replacement = masks
                .iter()
                .find(|(id, _)| *id == metadata.id)
                .map(|(_, view)| *view);
            if replacement.is_none() {
                if let Ok(view) = self.effective_mask_view(slot) {
                    prepared.push(PreparedMask::Borrowed(view));
                    continue;
                }
            }
            let raw = replacement.unwrap_or_else(|| self.mask_planes[slot].samples.view());
            if identity(metadata) || matches!(raw, PlaneView::Uniform(255)) {
                prepared.push(PreparedMask::Borrowed(raw));
                continue;
            }
            let source = super::mask_effects::descriptor(raw, self.width);
            let mut samples = match raw {
                PlaneView::Uniform(_) => SamplePlane::Uniform(0),
                PlaneView::Dense(_) => SamplePlane::Dense(vec![0; area]),
            };
            let mut output = match &mut samples {
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
                    data: data.as_mut_ptr(),
                    len: data.len(),
                    width: self.width as u32,
                    height: self.height as u32,
                    stride: self.width as u32,
                    origin_x: 0,
                    origin_y: 0,
                },
            };
            self.renderer
                .as_ref()
                .ok_or("Compositor DLL is not loaded")?
                .mask_effect_region(
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
            prepared.push(PreparedMask::Owned(samples));
        }
        let source = |index: usize| {
            layers
                .iter()
                .find(|(id, _)| *id == self.layer_id(index))
                .map_or(self.layers[index].pixels.as_slice(), |(_, pixels)| *pixels)
        };
        let mut result = vec![0; area];
        if self.requires_renderer() {
            let descriptors = prepared
                .iter()
                .map(|mask| super::mask_effects::descriptor(mask.view(), self.width))
                .collect::<Vec<_>>();
            let layers = self
                .layers
                .iter()
                .enumerate()
                .map(|(index, layer)| abi::CompositeLayer {
                    data: source(index).as_ptr(),
                    len: source(index).len(),
                    opacity: if self.has_raster_masks() && self.masked_layers[index] {
                        255
                    } else {
                        layer.opacity as u32
                    },
                    visible: layer.visible as u32,
                    mode: layer.mode as u32,
                })
                .collect::<Vec<_>>();
            let mut output = abi::Pixels {
                data: result.as_mut_ptr(),
                len: result.len(),
                width: self.width as u32,
                height: self.height as u32,
                stride: self.width as u32,
                origin_x: 0,
                origin_y: 0,
            };
            let renderer = self
                .renderer
                .as_ref()
                .ok_or("Compositor DLL is not loaded")?;
            if self.has_raster_masks() {
                renderer.render_prepared(
                    &self.plan,
                    &layers,
                    &descriptors,
                    self.levels_plan.as_ref().map_or(&[], |p| p.tables()),
                    self.width as u32,
                    self.height as u32,
                    &mut output,
                )?;
            } else {
                renderer.render_prepared(
                    if self.has_mixed_stack()
                        || self.graph.enabled && (!self.graph.linked || self.graph.has_filters())
                    {
                        &self.plan
                    } else {
                        &[]
                    },
                    &layers,
                    &[],
                    self.levels_plan.as_ref().map_or(&[], |p| p.tables()),
                    self.width as u32,
                    self.height as u32,
                    &mut output,
                )?;
            }
        } else {
            for (index, pixel) in result.iter_mut().enumerate() {
                for (layer, metadata) in self
                    .layers
                    .iter()
                    .enumerate()
                    .filter(|(_, metadata)| metadata.visible)
                {
                    let word = source(layer)[index];
                    *pixel = over(
                        *pixel,
                        (word & 0xffffff) | (((word >> 24) * metadata.opacity as u32 / 255) << 24),
                    );
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "owner_preview_tests.rs"]
mod tests;
