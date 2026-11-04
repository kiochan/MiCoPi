//! Shared dirty-region texture cache for independent canvas and navigator views.
use super::*;
impl Editor {
    pub(super) fn refresh_texture(&mut self, ctx: &Context) {
        if let Some([x0, y0, x1, y1]) = self.session.doc.update_display(&mut self.display) {
            if let Some(texture) = &mut self.texture {
                let pixels = (y0..y1)
                    .flat_map(|y| {
                        self.display
                            [y * self.session.doc.width + x0..y * self.session.doc.width + x1]
                            .iter()
                            .map(|p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, *p as u8))
                    })
                    .collect();
                texture.set_partial(
                    [x0, y0],
                    ColorImage::new([x1 - x0, y1 - y0], pixels),
                    TextureOptions::LINEAR,
                );
            } else {
                let pixels = self
                    .display
                    .iter()
                    .map(|p| Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, *p as u8))
                    .collect();
                self.texture = Some(ctx.load_texture(
                    "document",
                    ColorImage::new([self.session.doc.width, self.session.doc.height], pixels),
                    TextureOptions::LINEAR,
                ));
            }
        }
    }
}
