//! Complete raw-mask geometry staging. Native plugins own all A8 sampling.
use super::*;
use crate::engine::raster_masks::{MaskPlane, SamplePlane, MAX_MASKS};

fn same_samples(left: &SamplePlane, right: &SamplePlane) -> bool {
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
    pub(super) fn canvas_mask_geometry_matches(&self, masks: &[(u64, SamplePlane)]) -> bool {
        masks.len() == self.mask_planes.len()
            && masks
                .iter()
                .zip(&self.mask_planes)
                .all(|((id, samples), plane)| {
                    *id == plane.id && same_samples(samples, &plane.samples)
                })
    }
    /// Scalar target-frame admission for read-only geometry plans. Uniform
    /// storage still consumes its complete logical A8 sample budget.
    pub(crate) fn validate_canvas_geometry_frame(
        &self,
        width: usize,
        height: usize,
    ) -> Result<(), String> {
        if width == 0
            || height == 0
            || width > MAX_SIDE
            || height > MAX_SIDE
            || self.layers.is_empty()
            || self.layers.len() > MAX_LAYERS
            || width
                .checked_mul(height)
                .and_then(|area| area.checked_mul(self.layers.len()))
                .is_none_or(|pixels| pixels > 64 * 1024 * 1024)
            || self.raster_masks.items.len() > MAX_MASKS
        {
            return Err("Canvas geometry exceeds document frame or pixel limits".into());
        }
        self.raster_masks.validate_dimensions(width, height)
    }

    pub(super) fn validate_canvas_mask_geometry(
        &self,
        width: usize,
        height: usize,
        masks: &[(u64, SamplePlane)],
    ) -> Result<(), String> {
        self.validate_canvas_geometry_frame(width, height)?;
        if self.has_raster_masks() && self.stroke.is_some() {
            return Err("Finish the current contact before changing masked canvas geometry".into());
        }
        self.validate_mask_storage()?;
        if masks.len() != self.raster_masks.items.len()
            || masks
                .iter()
                .zip(&self.raster_masks.items)
                .any(|((id, _), metadata)| *id != metadata.id)
        {
            return Err("Canvas geometry requires every raw mask in metadata order".into());
        }
        let area = width * height;
        for (_, samples) in masks {
            samples.validate(area)?;
        }
        // Dimensions do not change graph topology. Source storage validates
        // owners/targets; target dimensions were checked independently above.
        // Capability validation includes hidden/disabled branches and effects,
        // even when staged pixels and mask samples happen to be equal.
        self.ensure_graph_renderer()
    }

    /// Publish only a complete staged document geometry outcome. IDs, ownership,
    /// properties, paint target, active raster and layer selection are retained.
    pub(crate) fn replace_geometry_with_masks(
        &mut self,
        width: usize,
        height: usize,
        pixels: Vec<Vec<u32>>,
        masks: Vec<(u64, SamplePlane)>,
        selection: crate::engine::selection::SelectionState,
        info: crate::engine::document_info::Info,
    ) -> Result<(), String> {
        self.validate_canvas_mask_geometry(width, height, &masks)?;
        info.validate()?;
        let area = width * height;
        if pixels.len() != self.layers.len() || pixels.iter().any(|data| data.len() != area) {
            return Err("Invalid staged geometry pixel dimensions".into());
        }
        selection.validate(width, height)?;
        if width == self.width
            && height == self.height
            && self.selection == selection
            && self.document_info == info
            && pixels
                .iter()
                .zip(&self.layers)
                .all(|(data, layer)| *data == layer.pixels)
            && self.canvas_mask_geometry_matches(&masks)
        {
            return Ok(());
        }
        // All validation precedes publication. Required output allocations are
        // prepared before original buffers move into the Canvas Undo record.
        let layers = pixels
            .into_iter()
            .zip(&self.layers)
            .map(|(pixels, old)| Layer {
                pixels,
                visible: old.visible,
                opacity: old.opacity,
                mode: old.mode,
            })
            .collect();
        let planes = masks
            .into_iter()
            .zip(&self.mask_planes)
            .map(|((id, samples), old)| MaskPlane {
                id,
                samples,
                content_revision: old.content_revision.wrapping_add(1),
            })
            .collect();
        let composite = vec![0; area];
        self.finish();
        let before = self.composition();
        let saved = CanvasState {
            width: self.width,
            height: self.height,
            layers: std::mem::replace(&mut self.layers, layers),
            mask_planes: std::mem::replace(&mut self.mask_planes, planes),
        };
        self.last_transform = None;
        self.document_info = info;
        self.width = width;
        self.height = height;
        self.selection = selection;
        self.composite = composite;
        self.replay_tiles.clear();
        self.invalidate_canvas_mask_geometry();
        self.track_layer_order();
        self.commit_composition(before, Some(LayerChange::Canvas(Arc::new(saved))));
        Ok(())
    }

    /// Also used for same-size Canvas Undo/Redo: metadata equality or an equal
    /// frame area cannot make transformed raw samples a valid effective cache.
    pub(super) fn invalidate_canvas_mask_geometry(&mut self) {
        self.mask_contact = None;
        self.bump_mask_storage_epoch();
        self.invalidate_mask_effects();
        self.replay_mask_tiles.clear();
        let count = self.width.div_ceil(TILE) * self.height.div_ceil(TILE);
        self.replay_mask_tiles.extend(
            self.mask_planes
                .iter()
                .flat_map(|plane| (0..count).map(move |tile| (plane.id, tile))),
        );
    }
}

#[cfg(test)]
#[path = "mask_geometry_tests.rs"]
mod tests;
