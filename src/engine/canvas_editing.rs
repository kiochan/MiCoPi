//! Integer canvas framing: copy retained rows, without sampling or color processing.
use super::*;
impl Document {
    /// Preserve document coordinates within a new frame. Positive offsets move content right/down.
    pub fn reframe(&mut self, width: usize, height: usize, dx: i32, dy: i32) -> Result<(), String> {
        if width == 0
            || height == 0
            || width > MAX_SIDE
            || height > MAX_SIDE
            || width * height * self.layers.len() > 64 * 1024 * 1024
        {
            return Err("Canvas size exceeds document limits".into());
        }
        if (width, height, dx, dy) == (self.width, self.height, 0, 0) {
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
            layers.push(Layer {
                pixels,
                visible: layer.visible,
                opacity: layer.opacity,
                mode: layer.mode,
            });
        }
        self.finish();
        let before = self.composition();
        self.selection = selection;
        let saved = CanvasState {
            width: self.width,
            height: self.height,
            layers: std::mem::replace(&mut self.layers, layers),
        };
        self.width = width;
        self.height = height;
        self.composite = vec![0; width * height];
        self.replay_tiles.clear();
        self.track_layer_order();
        self.commit_composition(before, Some(LayerChange::Canvas(Arc::new(saved))));
        Ok(())
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
