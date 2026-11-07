//! Staged canvas geometry, frame-bounded pixel/mask publication and isolated previews.
use super::*;
impl Document {
    /// Cold preview composition borrows untouched layers; no document/history/cache mutation.
    #[cfg(feature = "desktop")]
    pub(crate) fn preview_pixel_transform(
        &self,
        active: Option<&[u32]>,
        selection: &crate::engine::selection::SelectionState,
    ) -> Result<Vec<u32>, String> {
        self.require_mask_aware_edit()?;
        self.validate_pixel_transform(active, selection)?;
        let source = |i: usize| {
            if i == self.active {
                active.unwrap_or(&self.layers[i].pixels)
            } else {
                &self.layers[i].pixels
            }
        };
        let mut result = vec![0; self.width * self.height];
        if self.requires_renderer() {
            let renderer = self
                .renderer
                .as_ref()
                .ok_or("Compositor DLL is not loaded")?;
            let layers: Vec<_> = self
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| micopi_plugin_api::CompositeLayer {
                    data: source(i).as_ptr(),
                    len: source(i).len(),
                    opacity: l.opacity as u32,
                    visible: l.visible as u32,
                    mode: l.mode as u32,
                })
                .collect();
            let mut out = micopi_plugin_api::Pixels {
                data: result.as_mut_ptr(),
                len: result.len(),
                width: self.width as u32,
                height: self.height as u32,
                stride: self.width as u32,
                origin_x: 0,
                origin_y: 0,
            };
            renderer.render(
                if self.has_groups()
                    || self.graph.enabled && (!self.graph.linked || self.graph.has_filters())
                {
                    &self.plan
                } else {
                    &[]
                },
                &layers,
                self.width as u32,
                self.height as u32,
                &mut out,
            )?;
        } else {
            for (i, c) in result.iter_mut().enumerate() {
                for (n, l) in self.layers.iter().enumerate().filter(|(_, l)| l.visible) {
                    let p = source(n)[i];
                    *c = over(
                        *c,
                        (p & 0xffffff) | (((p >> 24) * l.opacity as u32 / 255) << 24),
                    );
                }
            }
        }
        Ok(result)
    }
    /// Publish fully staged structural geometry, moving original buffers into undo.
    pub(crate) fn replace_geometry(
        &mut self,
        width: usize,
        height: usize,
        pixels: Vec<Vec<u32>>,
        selection: crate::engine::selection::SelectionState,
    ) -> Result<(), String> {
        self.replace_geometry_with_info(width, height, pixels, selection, self.document_info)
    }
    /// Quarter turns also exchange physical axes, including square uniform canvases.
    pub(crate) fn replace_geometry_with_info(
        &mut self,
        width: usize,
        height: usize,
        pixels: Vec<Vec<u32>>,
        selection: crate::engine::selection::SelectionState,
        info: crate::engine::document_info::Info,
    ) -> Result<(), String> {
        self.require_mask_aware_edit()?;
        self.replace_geometry_with_masks(width, height, pixels, Vec::new(), selection, info)
    }
    /// Validate staged frame-bounded transforms before publication or preview.
    pub(crate) fn validate_pixel_transform(
        &self,
        pixels: Option<&[u32]>,
        selection: &crate::engine::selection::SelectionState,
    ) -> Result<(), String> {
        if pixels.is_some() {
            self.require_mask_aware_edit()?;
        }
        selection.validate(self.width, self.height)?;
        if let Some(p) = pixels {
            self.validate_layer_transform_pixels(self.active, p)?;
        }
        Ok(())
    }
    /// Pixel permission/output validation shared by explicitly admitted geometry.
    /// Callers retain their own mask, contact and structural admission guards.
    pub(super) fn validate_layer_transform_pixels(
        &self,
        index: usize,
        pixels: &[u32],
    ) -> Result<(), String> {
        let layer = self.layers.get(index).ok_or("Layer does not exist")?;
        if self.pixel_locked(index) {
            return Err("Unlock layer pixels before transforming them".into());
        }
        if self.position_locked(index) {
            return Err("Unlock layer position before transforming pixels".into());
        }
        if pixels.len() != self.width * self.height {
            return Err("Invalid movement pixel count".into());
        }
        if self.alpha_locked(index)
            && layer
                .pixels
                .iter()
                .zip(pixels)
                .any(|(a, b)| a >> 24 != b >> 24 || a >> 24 == 0 && a != b)
        {
            return Err("Movement would change locked layer transparency".into());
        }
        Ok(())
    }
    /// Publish staged pixels and selection under one tiled history entry.
    pub(crate) fn apply_movement(
        &mut self,
        pixels: Option<&[u32]>,
        selection: crate::engine::selection::SelectionState,
    ) -> Result<(), String> {
        self.apply_affine(pixels, selection, self.last_transform)
    }
    /// Publish pixels, masks and the bounded repeat recipe in the same history entry.
    pub(crate) fn apply_affine(
        &mut self,
        pixels: Option<&[u32]>,
        selection: crate::engine::selection::SelectionState,
        recipe: Option<crate::engine::transform::Recipe>,
    ) -> Result<(), String> {
        if let Some(r) = recipe {
            r.validate()?;
        }
        self.validate_pixel_transform(pixels, &selection)?;
        let selection_changed = self.selection != selection || self.last_transform != recipe;
        if !selection_changed && pixels.is_none_or(|p| p == self.layers[self.active].pixels) {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        self.stroke = Some(if selection_changed {
            Self::composition_edit(before, None, self.active)
        } else {
            Edit {
                group: None,
                layer_change: None,
                layer: self.active,
                tiles: BTreeMap::new(),
                mask_tiles: BTreeMap::new(),
                mask_change: None,
                composition: None,
                metadata_bytes: 0,
            }
        });
        if let Some(p) = pixels {
            self.write_region(0, 0, self.width, self.height, p);
        }
        self.selection = selection;
        self.last_transform = recipe;
        self.finish();
        Ok(())
    }
    /// Preserve document coordinates within a new frame. Positive offsets move content right/down.
    pub fn reframe(&mut self, width: usize, height: usize, dx: i32, dy: i32) -> Result<(), String> {
        self.require_mask_aware_edit()?;
        self.reframe_with_masks(width, height, dx, dy, Vec::new())
    }
    /// Reuse the established RGBA/selection frame copy with complete raw masks
    /// already sampled by the native painting plugin. No A8 pixel math lives here.
    pub(crate) fn reframe_with_masks(
        &mut self,
        width: usize,
        height: usize,
        dx: i32,
        dy: i32,
        masks: Vec<(u64, crate::engine::raster_masks::SamplePlane)>,
    ) -> Result<(), String> {
        self.validate_canvas_mask_geometry(width, height, &masks)?;
        if (width, height, dx, dy) == (self.width, self.height, 0, 0)
            && self.canvas_mask_geometry_matches(&masks)
        {
            return Ok(());
        }
        let sx = (-i64::from(dx)).max(0).min(self.width as i64) as usize;
        let sy = (-i64::from(dy)).max(0).min(self.height as i64) as usize;
        let tx = i64::from(dx).max(0).min(width as i64) as usize;
        let ty = i64::from(dy).max(0).min(height as i64) as usize;
        let copy_width = (self.width - sx).min(width - tx);
        let copy_height = (self.height - sy).min(height - ty);
        let selection = self.selection.reframe(width, height, dx, dy)?;
        let mut layers = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            let mut pixels = vec![0; width * height];
            for row in 0..copy_height {
                pixels[(ty + row) * width + tx..(ty + row) * width + tx + copy_width]
                    .copy_from_slice(
                        &layer.pixels[(sy + row) * self.width + sx
                            ..(sy + row) * self.width + sx + copy_width],
                    );
            }
            layers.push(pixels);
        }
        self.replace_geometry_with_masks(
            width,
            height,
            layers,
            masks,
            selection,
            self.document_info,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crop_preserves_hidden_rgb_and_swaps_owned_buffers_in_history() {
        let mut d = Document::new(70, 67);
        for (i, p) in d.layers[0].pixels.iter_mut().enumerate() {
            *p = ((i % 4) as u32 * 85) << 24 | (i as u32 * 7919 & 0x00ffffff);
        }
        let original = d.layers[0].pixels.clone();
        let original_ptr = d.layers[0].pixels.as_ptr();
        d.reframe(65, 63, -3, -2).unwrap();
        let cropped_ptr = d.layers[0].pixels.as_ptr();
        for y in 0..63 {
            assert_eq!(
                &d.layers[0].pixels[y * 65..(y + 1) * 65],
                &original[(y + 2) * 70 + 3..(y + 2) * 70 + 68]
            );
        }
        assert!(d.history_bytes() >= original.len() * 4);
        d.undo();
        assert_eq!((d.width, d.height), (70, 67));
        assert_eq!(d.layers[0].pixels, original);
        assert_eq!(d.layers[0].pixels.as_ptr(), original_ptr);
        d.redo();
        assert_eq!((d.width, d.height), (65, 63));
        assert_eq!(d.layers[0].pixels.as_ptr(), cropped_ptr);
    }
}
