//! One bounded CPU display image; exact visible differences drive GPU uploads.
use eframe::egui::{Color32, ColorImage};
use std::sync::Arc;

const DISPLAY_LIMIT: usize = 64 * 1024 * 1024;
#[derive(Default)]
pub(super) struct DisplayCache {
    image: Option<Arc<ColorImage>>,
}
pub(super) enum Upload {
    Unchanged,
    Full(Arc<ColorImage>),
    Partial {
        origin: [usize; 2],
        image: ColorImage,
    },
}
fn displayed(word: u32, index: usize, width: usize) -> Color32 {
    let bg = if (index % width / 12 + index / width / 12) % 2 == 0 {
        0xfffdfdfd
    } else {
        0xfff3f5f7
    };
    let p = crate::engine::over(bg, word);
    Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, p as u8)
}
fn colors(source: &[u32], width: usize, bounds: [usize; 4]) -> Result<ColorImage, String> {
    let [x0, y0, x1, y1] = bounds;
    let (w, h) = (x1 - x0, y1 - y0);
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(w * h)
        .map_err(|_| "Cannot reserve Liquify display upload")?;
    for y in y0..y1 {
        for x in x0..x1 {
            let index = y * width + x;
            pixels.push(displayed(source[index], index, width));
        }
    }
    Ok(ColorImage::new([w, h], pixels))
}
impl DisplayCache {
    fn full(&mut self, source: &[u32], size: [usize; 2]) -> Result<Upload, String> {
        let image = Arc::new(colors(source, size[0], [0, 0, size[0], size[1]])?);
        self.image = Some(Arc::clone(&image));
        Ok(Upload::Full(image))
    }
    /// Composition still produces a full immutable RGBA frame. Retain only the
    /// already-converted display image, share initial/full uploads and patch a
    /// uniquely held cache in place. Never copy-on-write a pending upload image.
    pub(super) fn prepare(&mut self, source: &[u32], size: [usize; 2]) -> Result<Upload, String> {
        let [width, height] = size;
        let area = width
            .checked_mul(height)
            .ok_or("Liquify display extent overflows")?;
        let bytes = area
            .checked_mul(std::mem::size_of::<Color32>())
            .ok_or("Liquify display cost overflows")?;
        if width == 0 || height == 0 || source.len() != area || bytes > DISPLAY_LIMIT {
            return Err(
                "Liquify CPU display image exceeds its 64 MiB limit or has invalid storage".into(),
            );
        }
        let Some(previous) = self.image.as_ref().filter(|image| image.size == size) else {
            return self.full(source, size);
        };
        let mut bounds = [width, height, 0, 0];
        for (i, word) in source.iter().copied().enumerate() {
            if previous.pixels[i] != displayed(word, i, width) {
                let (x, y) = (i % width, i / width);
                bounds[0] = bounds[0].min(x);
                bounds[1] = bounds[1].min(y);
                bounds[2] = bounds[2].max(x + 1);
                bounds[3] = bounds[3].max(y + 1);
            }
        }
        if bounds[0] >= bounds[2] {
            return Ok(Upload::Unchanged);
        }
        if bounds == [0, 0, width, height]
            || Arc::strong_count(previous) > 1
            || Arc::weak_count(previous) > 0
        {
            return self.full(source, size);
        }
        let patch = colors(source, width, bounds)?;
        let cached = Arc::get_mut(self.image.as_mut().unwrap())
            .ok_or("Liquify display image is still shared")?;
        for row in 0..patch.size[1] {
            let start = (bounds[1] + row) * width + bounds[0];
            cached.pixels[start..start + patch.size[0]]
                .copy_from_slice(&patch.pixels[row * patch.size[0]..(row + 1) * patch.size[0]]);
        }
        Ok(Upload::Partial {
            origin: [bounds[0], bounds[1]],
            image: patch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_cache_shares_full_upload_skips_unchanged_and_patches_exact_bounds() {
        let mut cache = DisplayCache::default();
        let mut source = vec![0; 8 * 6];
        let Upload::Full(image) = cache.prepare(&source, [8, 6]).unwrap() else {
            panic!("Initial upload must be full")
        };
        assert_eq!(image.size, [8, 6]);
        assert_eq!(Arc::strong_count(&image), 2);
        assert!(Arc::ptr_eq(cache.image.as_ref().unwrap(), &image));
        drop(image);
        let address = Arc::as_ptr(cache.image.as_ref().unwrap());
        assert!(matches!(
            cache.prepare(&source, [8, 6]).unwrap(),
            Upload::Unchanged
        ));
        source[2 * 8 + 3] = 0xff123456;
        let Upload::Partial { origin, image } = cache.prepare(&source, [8, 6]).unwrap() else {
            panic!("One changed pixel must be partial")
        };
        assert_eq!(origin, [3, 2]);
        assert_eq!(image.size, [1, 1]);
        assert_eq!(image.pixels, [Color32::from_rgb(0x12, 0x34, 0x56)]);
        assert_eq!(Arc::as_ptr(cache.image.as_ref().unwrap()), address);
        assert_eq!(
            cache.image.as_ref().unwrap().pixels[2 * 8 + 3],
            image.pixels[0]
        );
        assert!(matches!(
            cache.prepare(&source, [8, 6]).unwrap(),
            Upload::Unchanged
        ));
        source[1 * 8 + 2] = 0xff456789;
        source[4 * 8 + 6] = 0xff987654;
        let Upload::Partial { origin, image } = cache.prepare(&source, [8, 6]).unwrap() else {
            panic!("Bounded patch expected")
        };
        assert_eq!(origin, [2, 1]);
        assert_eq!(image.size, [5, 4]);
        for y in 0..6 {
            for x in 0..8 {
                assert_eq!(
                    cache.image.as_ref().unwrap().pixels[y * 8 + x],
                    displayed(source[y * 8 + x], y * 8 + x, 8)
                );
            }
        }
    }
    #[test]
    fn pending_shared_upload_is_immutable_and_forces_full_replacement() {
        let mut cache = DisplayCache::default();
        let mut source = vec![0; 8 * 6];
        let Upload::Full(pending) = cache.prepare(&source, [8, 6]).unwrap() else {
            panic!()
        };
        let old = pending.pixels.clone();
        source[3] = 0xff123456;
        let Upload::Full(next) = cache.prepare(&source, [8, 6]).unwrap() else {
            panic!("A still-shared CPU image must not be mutated")
        };
        assert_eq!(pending.pixels, old);
        assert!(!Arc::ptr_eq(&pending, &next));
        assert_eq!(next.pixels[3], Color32::from_rgb(0x12, 0x34, 0x56));
        assert!(matches!(
            cache.prepare(&source, [8, 6]).unwrap(),
            Upload::Unchanged
        ));
    }
    #[test]
    fn display_validation_and_hidden_rgb_changes_leave_published_cache_unchanged() {
        let mut cache = DisplayCache::default();
        let mut source = vec![0; 8 * 6];
        drop(cache.prepare(&source, [8, 6]).unwrap());
        let address = Arc::as_ptr(cache.image.as_ref().unwrap());
        source[0] = 0x00123456;
        assert!(matches!(
            cache.prepare(&source, [8, 6]).unwrap(),
            Upload::Unchanged
        ));
        assert!(cache.prepare(&source, [0, 6]).is_err());
        assert!(cache.prepare(&source, [9, 6]).is_err());
        assert!(cache.prepare(&[], [usize::MAX, 2]).is_err());
        assert_eq!(Arc::as_ptr(cache.image.as_ref().unwrap()), address);
        assert!(matches!(
            cache.prepare(&vec![0; 6 * 8], [6, 8]).unwrap(),
            Upload::Full(_)
        ));
    }
}
