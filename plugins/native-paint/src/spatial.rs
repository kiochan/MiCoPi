//! Exact integer transforms; no extra image buffer is needed, including translation.
#[derive(Clone, Copy)]
pub enum Spatial {
    FlipHorizontal,
    FlipVertical,
    Rotate180,
    Translate { dx: i32, dy: i32 },
}
pub fn apply(pixels: &mut [u32], width: usize, height: usize, operation: Spatial) {
    match operation {
        Spatial::FlipHorizontal => pixels.chunks_exact_mut(width).for_each(|row| row.reverse()),
        Spatial::FlipVertical => {
            for y in 0..height / 2 {
                for x in 0..width {
                    pixels.swap(y * width + x, (height - 1 - y) * width + x);
                }
            }
        }
        Spatial::Rotate180 => pixels.reverse(),
        Spatial::Translate { dx, dy } => {
            let w = width as i32;
            let h = height as i32;
            if dx.abs() >= w || dy.abs() >= h {
                pixels.fill(0);
                return;
            }
            // Visit rows against the movement direction, then use memmove for
            // horizontal overlap. Clear only after the source row was consumed.
            for step in 0..height {
                let y = if dy > 0 { height - 1 - step } else { step };
                let source_y = y as i32 - dy;
                let row = y * width;
                if source_y < 0 || source_y >= h {
                    pixels[row..row + width].fill(0);
                    continue;
                }
                let source_x = (-dx).max(0) as usize;
                let dest_x = dx.max(0) as usize;
                let count = width - dx.unsigned_abs() as usize;
                let source = source_y as usize * width + source_x;
                pixels.copy_within(source..source + count, row + dest_x);
                pixels[row..row + dest_x].fill(0);
                pixels[row + dest_x + count..row + width].fill(0);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asymmetric_pixels_have_exact_flip_and_rotation_results() {
        for (op, expected) in [
            (Spatial::FlipHorizontal, vec![3, 2, 1, 6, 5, 4]),
            (Spatial::FlipVertical, vec![4, 5, 6, 1, 2, 3]),
            (Spatial::Rotate180, vec![6, 5, 4, 3, 2, 1]),
        ] {
            let mut pixels = vec![1, 2, 3, 4, 5, 6];
            apply(&mut pixels, 3, 2, op);
            assert_eq!(pixels, expected);
            apply(&mut pixels, 3, 2, op);
            assert_eq!(pixels, vec![1, 2, 3, 4, 5, 6]);
        }
    }
    #[test]
    fn overlapping_translations_match_a_separate_buffer_oracle_in_every_direction() {
        for width in [1, 3, 8] {
            for height in [1, 4, 9] {
                let original: Vec<u32> = (1..=width * height)
                    .map(|i| 0x80000000 | i as u32)
                    .collect();
                for dx in -9i32..=9 {
                    for dy in -10i32..=10 {
                        let mut expected = vec![0; width * height];
                        for y in 0..height {
                            for x in 0..width {
                                let (tx, ty) = (x as i32 + dx, y as i32 + dy);
                                if tx >= 0 && ty >= 0 && tx < width as i32 && ty < height as i32 {
                                    expected[ty as usize * width + tx as usize] =
                                        original[y * width + x];
                                }
                            }
                        }
                        let mut actual = original.clone();
                        apply(&mut actual, width, height, Spatial::Translate { dx, dy });
                        assert_eq!(actual, expected, "{width}x{height} offset {dx},{dy}");
                    }
                }
            }
        }
    }
}
