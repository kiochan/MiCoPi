//! Cached selection presentation, independent of artwork and image composition.
use super::*;
impl Editor {
    pub(super) fn refresh_selection(&mut self, ctx: &Context) {
        let Some(mask) = self.session.doc.selection().current.as_ref() else {
            self.selection_texture = None;
            return;
        };
        if self
            .selection_texture
            .as_ref()
            .is_some_and(|(old, _)| old == mask)
        {
            return;
        }
        let (dw, dh) = mask.dimensions();
        let ratio = (1024. / dw.max(dh) as f32).min(1.);
        let (w, h) = (
            (dw as f32 * ratio).round().max(1.) as usize,
            (dh as f32 * ratio).round().max(1.) as usize,
        );
        let mut selected = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                selected[y * w + x] = mask.coverage()[(y * dh / h) * dw + x * dw / w] >= 128;
            }
        }
        let mut pixels = vec![Color32::TRANSPARENT; w * h];
        for y in 0..h {
            for x in 0..w {
                if selected[y * w + x]
                    && (x == 0
                        || y == 0
                        || x == w - 1
                        || y == h - 1
                        || !selected[y * w + x - 1]
                        || !selected[y * w + x + 1]
                        || !selected[(y - 1) * w + x]
                        || !selected[(y + 1) * w + x])
                {
                    pixels[y * w + x] = if (x / 4 + y / 4) % 2 == 0 {
                        Color32::WHITE
                    } else {
                        Color32::BLACK
                    };
                }
            }
        }
        self.selection_texture = Some((
            mask.clone(),
            ctx.load_texture(
                "selection-outline",
                ColorImage::new([w, h], pixels),
                TextureOptions::NEAREST,
            ),
        ));
    }
}
