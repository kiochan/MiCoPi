//! Cold raw-alpha mask baking with one tile/metadata Undo entry.
use super::*;
use crate::engine::raster_masks::{MaskOwner, PaintTarget};
use micopi_plugin_api as abi;

impl Document {
    /// Bake effective coverage into its explicit raster owner, preserving source
    /// filters, opacity, blend mode, active layer and the selected-layer set.
    /// Group-mask baking requires a separate subtree operation.
    pub fn apply_raster_mask(&mut self, id: u64) -> Result<u64, String> {
        if self.stroke.is_some() {
            return Err("Finish the current contact before applying a raster mask".into());
        }
        self.validate_mask_storage()?;
        let slot = self
            .raster_masks
            .items
            .iter()
            .position(|metadata| metadata.id == id)
            .ok_or("Raster mask does not exist")?;
        let layer_id = match self.raster_masks.items[slot].owner {
            MaskOwner::Layer { id } => id,
            MaskOwner::Group { .. } => {
                return Err("Applying a group mask requires mask-aware subtree baking".into());
            }
            MaskOwner::Adjustment { .. } => {
                return Err(
                    "Applying an adjustment mask requires a dedicated baking operation".into(),
                )
            }
        };
        let index = self
            .layer_index(layer_id)
            .ok_or("Raster mask owner does not exist")?;
        if self.pixel_locked(index) {
            return Err("Unlock layer pixels before applying its raster mask".into());
        }
        if self.alpha_locked(index) {
            return Err("Unlock layer transparency before applying its raster mask".into());
        }
        let mut candidate = self.raster_masks.clone();
        candidate.items.remove(slot);
        if candidate.target == (PaintTarget::Mask { id }) {
            candidate.target = PaintTarget::LayerPixels;
        }
        let prepared = self.prepare_raster_mask_plan(&candidate, true)?;
        self.prepare_mask_effects()?;
        let mask = super::mask_effects::descriptor(self.effective_mask_view(slot)?, self.width);
        let layer = &self.layers[index];
        let source = abi::CompositeLayer {
            data: layer.pixels.as_ptr(),
            len: layer.pixels.len(),
            opacity: 255,
            visible: 1,
            mode: 0,
        };
        let plan = [
            abi::CompositeNode {
                kind: 0,
                a: 0,
                b: 0,
                mode: 0,
                amount: 0.,
            },
            abi::CompositeNode {
                kind: 6,
                a: 0,
                b: 0,
                mode: 0,
                amount: 0.,
            },
            abi::CompositeNode {
                kind: 7,
                a: 0,
                b: 1,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 3,
                a: 2,
                b: 0,
                mode: 0,
                amount: 0.,
            },
        ];
        let mut pixels = vec![0; self.width * self.height];
        let mut output = abi::Pixels {
            data: pixels.as_mut_ptr(),
            len: pixels.len(),
            width: self.width as u32,
            height: self.height as u32,
            stride: self.width as u32,
            origin_x: 0,
            origin_y: 0,
        };
        self.renderer.as_ref().unwrap().render_masked(
            &plan,
            &[source],
            &[mask],
            self.width as u32,
            self.height as u32,
            &mut output,
        )?;
        // Fallible admission and native work precede publication. The existing
        // tile history captures only raster bytes that the staged bake changes.
        let before = self.composition();
        let removed = self.mask_planes.remove(slot);
        let mut edit = Self::composition_edit(before, None, index);
        edit.mask_change = Some(MaskChange::Toggle {
            slot,
            removed: Some(Arc::new(removed)),
        });
        self.stroke = Some(edit);
        self.write_layer_region(index, 0, 0, self.width, self.height, &pixels);
        self.raster_masks = candidate;
        self.replay_mask_tiles.retain(|(mask, _)| *mask != id);
        self.remove_mask_effect_cache(id);
        self.bump_mask_storage_epoch();
        self.install_group_plan(prepared);
        self.finish();
        self.invalidate();
        Ok(layer_id)
    }
}
