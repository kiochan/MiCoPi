//! Immutable views reuse active-contact undo tiles without copying originals.
use super::*;
use crate::engine::raster_masks::MaskLease;
use micopi_plugin_api::{OriginA8Pixels, OriginPixels};

struct TileRegion {
    tile: usize,
    stride: usize,
    tile_len: usize,
    offset: usize,
    len: usize,
}

impl Document {
    fn origin_tile_region(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Result<TileRegion, String> {
        if width == 0
            || height == 0
            || x.checked_add(width).is_none_or(|end| end > self.width)
            || y.checked_add(height).is_none_or(|end| end > self.height)
            || x / TILE != (x + width - 1) / TILE
            || y / TILE != (y + height - 1) / TILE
        {
            return Err("An origin region must lie inside one captured undo tile".into());
        }
        let tx = x / TILE;
        let ty = y / TILE;
        let stride = (self.width - tx * TILE).min(TILE);
        let tile_height = (self.height - ty * TILE).min(TILE);
        Ok(TileRegion {
            tile: ty * self.width.div_ceil(TILE) + tx,
            stride,
            tile_len: stride * tile_height,
            offset: (y - ty * TILE) * stride + x - tx * TILE,
            len: (height - 1) * stride + width,
        })
    }

    /// The returned immutable pointer is valid only during the current
    /// synchronous tool loan. Prepare the ROI first; never retain this view or
    /// mutate/contact-swap the document while it is borrowed.
    pub(crate) fn plugin_origin_region(
        &self,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Result<OriginPixels, String> {
        let region = self.origin_tile_region(x, y, width, height)?;
        let edit = self
            .stroke
            .as_ref()
            .ok_or("No raster tool contact is active")?;
        if self.mask_contact.is_some()
            || edit.layer != self.active
            || self.pixel_locked(self.active)
            || !self.layer_visible(self.active)
        {
            return Err("Raster origin does not match the admitted tool contact".into());
        }
        let saved = edit
            .tiles
            .get(&region.tile)
            .ok_or("Raster origin tile was not captured")?;
        if saved.len() != region.tile_len {
            return Err("Raster origin tile has an invalid sample count".into());
        }
        Ok(OriginPixels {
            data: unsafe { saved.as_ptr().add(region.offset) },
            len: region.len,
            width: width as u32,
            height: height as u32,
            stride: region.stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        })
    }

    /// Raw A8 origin from the exact active lease. This is neither selected-pixel
    /// coverage nor effective density/feather coverage. The pointer is scoped to
    /// one synchronous loan and never owns or copies its saved samples.
    pub(crate) fn mask_origin_region(
        &self,
        lease: MaskLease,
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    ) -> Result<OriginA8Pixels, String> {
        if !self.valid_mask_lease(lease) {
            return Err("Invalid or stale raster mask origin lease".into());
        }
        let region = self.origin_tile_region(x, y, width, height)?;
        let saved = self
            .stroke
            .as_ref()
            .unwrap()
            .mask_tiles
            .get(&(lease.id, region.tile))
            .ok_or("Raster mask origin tile was not captured")?;
        if saved.len() != region.tile_len {
            return Err("Raster mask origin tile has an invalid sample count".into());
        }
        Ok(OriginA8Pixels {
            data: unsafe { saved.as_ptr().add(region.offset) },
            len: region.len,
            width: width as u32,
            height: height as u32,
            stride: region.stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        })
    }
}

#[cfg(test)]
#[path = "origin_loans_tests.rs"]
mod tests;
