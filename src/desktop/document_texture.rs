//! Shared dirty-region texture cache for independent canvas and navigator views.
use super::*;
impl Editor {
    pub(super) fn refresh_texture(&mut self, ctx: &Context) {
        // Commands such as undo can change dimensions before an adapter sync.
        // Rebuild the complete surface only then; retain partial uploads for strokes.
        if self
            .texture
            .as_ref()
            .is_none_or(|t| t.size() != [self.session.doc.width, self.session.doc.height])
        {
            self.texture = None;
            self.display.clear();
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn texture_size_tracks_unsynchronized_undo_and_equal_area_rotation() {
        let mut app = crate::desktop::tests::studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        let ctx = Context::default();
        app.editor
            .core("canvas.new", serde_json::json!({"width":12,"height":8}));
        app.editor.refresh_texture(&ctx);
        let first = app.editor.texture.as_ref().unwrap().id();
        app.editor.core(
            "canvas.image_size",
            serde_json::json!({"width":6,"method":"area"}),
        );
        app.editor.refresh_texture(&ctx);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [6, 4]);
        assert_ne!(app.editor.texture.as_ref().unwrap().id(), first);
        app.editor.actions.push(Action::Undo);
        app.run_actions(&ctx);
        app.editor.refresh_texture(&ctx);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [12, 8]);
        assert_eq!(app.editor.display.len(), 96);
        app.editor
            .core("canvas.rotate_90_cw", serde_json::json!({}));
        app.editor.refresh_texture(&ctx);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [8, 12]);
        assert_eq!(app.editor.display.len(), 96);
        let id = app.editor.texture.as_ref().unwrap().id();
        app.editor
            .core("layer.fill_color", serde_json::json!({"color":"#336699"}));
        app.editor.refresh_texture(&ctx);
        assert_eq!(app.editor.texture.as_ref().unwrap().id(), id);
        app.editor.actions.push(Action::Undo);
        app.run_actions(&ctx);
        app.editor.actions.push(Action::Undo);
        app.run_actions(&ctx);
        app.editor.refresh_texture(&ctx);
        assert_eq!(app.editor.texture.as_ref().unwrap().size(), [12, 8]);
    }
}
