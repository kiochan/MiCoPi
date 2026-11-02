use super::*;
#[derive(Default)]
pub(super) struct ReplayWindow {
    pub open: bool,
    pub refresh: bool,
    loaded: bool,
    cursor: usize,
    playing: bool,
    rate: f32,
    last: f64,
    last_frame: Option<(bool, usize)>,
    texture: Option<TextureHandle>,
    message: String,
    verified: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeking_and_playback_render_at_multiple_sizes_without_editing_the_live_canvas() {
        let mut app = crate::desktop::tests::studio();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "canvas.new",
                serde_json::json!({"width":64,"height":64}),
            ))
            .unwrap();
        for color in ["#5577DD", "#77BB99"] {
            app.editor
                .session
                .apply(crate::application::Action::new(
                    "brush.set",
                    serde_json::json!({"color":color}),
                ))
                .unwrap();
        }
        let before = app.editor.session.state();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let count = app.editor.session.journal.as_ref().unwrap().entries.len();
        app.replay.open = true;
        let ctx = Context::default();
        for (i, size) in [vec2(960., 680.), vec2(1700., 1100.)]
            .into_iter()
            .enumerate()
        {
            app.replay.cursor = 0;
            app.replay.playing = true;
            app.replay.last = 0.;
            let output = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    time: Some(i as f64 + 1.),
                    ..Default::default()
                },
                |ctx| app.replay_window(ctx),
            );
            assert_eq!(app.replay.cursor, 1);
            assert!(app.replay.texture.is_some());
            assert!(output.shapes.iter().all(|s| s.clip_rect.is_finite()));
        }
        assert_eq!(app.editor.session.state(), before);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            count
        );
    }
}
impl Studio {
    pub(super) fn replay_window(&mut self, ctx: &Context) {
        if !self.replay.open {
            return;
        }
        let mut panel = std::mem::take(&mut self.replay);
        let mut open = true;
        if panel.rate == 0. {
            panel.rate = 12.;
        }
        let count = self
            .editor
            .session
            .replay_journal(panel.loaded)
            .map(|j| j.entries.len())
            .unwrap_or(0);
        if panel.refresh {
            panel.cursor = count.saturating_sub(1);
            panel.last_frame = None;
            panel.refresh = false;
            panel.playing = false;
        }
        panel.cursor = panel.cursor.min(count.saturating_sub(1));
        let time = ctx.input(|i| i.time);
        if panel.playing && time - panel.last >= 1. / panel.rate as f64 {
            if panel.cursor + 1 < count {
                panel.cursor += 1;
            } else {
                panel.playing = false;
            }
            panel.last = time;
        }
        let a = self.editor.settings.ui.clone();
        let colors = theme::Colors::new(&a);
        Window::new(tr("Time-lapse Replay"))
            .id(Id::new("verified-replay-window"))
            .open(&mut open)
            .default_size(vec2(900., 680.))
            .min_size(vec2(460., 360.))
            .max_size((ctx.content_rect().size() - vec2(40., 60.)).max(vec2(320., 260.)))
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(!panel.loaded, tr("Current Artwork"))
                        .clicked()
                    {
                        panel.loaded = false;
                        panel.refresh = true;
                        panel.verified = None;
                    }
                    let available = self.editor.session.loaded_replay.is_some();
                    if ui
                        .add_enabled(
                            available,
                            Button::new(tr("Loaded Archive")).selected(panel.loaded),
                        )
                        .clicked()
                    {
                        panel.loaded = true;
                        panel.refresh = true;
                        panel.verified = None;
                    }
                    if ui.button(tr("Open Replay…")).clicked() {
                        if let Some(path) = self
                            .editor
                            .native
                            .as_ref()
                            .and_then(|n| n.replay_dialog(false, false))
                        {
                            match self.editor.session.apply(crate::application::Action::new(
                                "replay.load",
                                serde_json::json!({"path":path}),
                            )) {
                                Ok(_) => {
                                    panel.loaded = true;
                                    panel.refresh = true;
                                    panel.verified = Some("Archive verified".into());
                                }
                                Err(e) => panel.message = e.to_string(),
                            }
                        }
                    }
                    if ui
                        .add_enabled(count > 0, Button::new(tr("Verify")))
                        .clicked()
                    {
                        let source = if panel.loaded { "loaded" } else { "current" };
                        match self.editor.session.apply(crate::application::Action::new(
                            "replay.verify",
                            serde_json::json!({"source":source}),
                        )) {
                            Ok(r) => {
                                panel.verified =
                                    Some(format!("{} · SHA-256", r["result"]["checked_events"]))
                            }
                            Err(e) => {
                                panel.message = e.to_string();
                                panel.verified = None;
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let enabled = count > 0;
                    if ui
                        .add_enabled(
                            enabled,
                            Button::new(tr(if panel.playing { "Pause" } else { "Play" })),
                        )
                        .clicked()
                    {
                        if !panel.playing && panel.cursor + 1 >= count {
                            panel.cursor = 0;
                        }
                        panel.playing = !panel.playing;
                        panel.last = time;
                    }
                    if ui
                        .add_enabled(enabled, Button::new("|◀"))
                        .on_hover_text(tr("First Event"))
                        .clicked()
                    {
                        panel.cursor = 0;
                        panel.playing = false;
                    }
                    if ui
                        .add_enabled(panel.cursor > 0, Button::new("◀"))
                        .on_hover_text(tr("Previous Event"))
                        .clicked()
                    {
                        panel.cursor = panel.cursor.saturating_sub(1);
                        panel.playing = false;
                    }
                    if ui
                        .add_enabled(panel.cursor + 1 < count, Button::new("▶"))
                        .on_hover_text(tr("Next Event"))
                        .clicked()
                    {
                        panel.cursor += 1;
                        panel.playing = false;
                    }
                    if ui
                        .add_enabled(enabled, Button::new("▶|"))
                        .on_hover_text(tr("Last Event"))
                        .clicked()
                    {
                        panel.cursor = count.saturating_sub(1);
                        panel.playing = false;
                    }
                    ComboBox::from_id_salt("replay-rate")
                        .selected_text(format!(
                            "{} {}",
                            panel.rate as u32,
                            crate::i18n::text("events/s")
                        ))
                        .show_ui(ui, |ui| {
                            for rate in [3., 6., 12., 24., 48.] {
                                ui.selectable_value(
                                    &mut panel.rate,
                                    rate,
                                    format!("{} {}", rate as u32, crate::i18n::text("events/s")),
                                );
                            }
                        });
                    if count > 0 {
                        if ui
                            .add(Slider::new(&mut panel.cursor, 0..=count - 1).text(tr("Event")))
                            .changed()
                        {
                            panel.playing = false;
                        }
                    }
                });
                if count == 0 {
                    ui.weak(tr("Start drawing to record a replay."));
                    return;
                }
                if panel.last_frame != Some((panel.loaded, panel.cursor)) {
                    match self
                        .editor
                        .session
                        .replay_preview(panel.cursor, panel.loaded)
                    {
                        Ok(frame) => {
                            let (w, h, rgba) = frame.preview(1024);
                            let pixels = rgba
                                .into_iter()
                                .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
                                .collect();
                            let image = ColorImage::new([w, h], pixels);
                            if let Some(texture) = &mut panel.texture {
                                texture.set(image, TextureOptions::LINEAR);
                            } else {
                                panel.texture = Some(ctx.load_texture(
                                    "replay-preview",
                                    image,
                                    TextureOptions::LINEAR,
                                ));
                            }
                            panel.last_frame = Some((panel.loaded, panel.cursor));
                        }
                        Err(e) => panel.message = e.to_string(),
                    }
                }
                if let Ok(journal) = self.editor.session.replay_journal(panel.loaded) {
                    if let Some(entry) = journal.entries.get(panel.cursor) {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(tr(&entry.label));
                            ui.weak(format!(
                                "#{} · {:.2}s",
                                entry.sequence,
                                entry.elapsed_ms as f64 / 1000.
                            ));
                            if !entry.success {
                                ui.colored_label(colors.accent, tr("Operation Failed"));
                            }
                        });
                    }
                }
                let available = ui.available_size() - vec2(0., 100.);
                let size = available.max(vec2(1., 1.));
                let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter()
                    .rect_filled(r, a.corner_radius, colors.background);
                if let Some(texture) = &panel.texture {
                    let scale = (r.width() / texture.size()[0] as f32)
                        .min(r.height() / texture.size()[1] as f32);
                    let image = Rect::from_center_size(r.center(), texture.size_vec2() * scale);
                    ui.painter().rect_filled(image, 0, Color32::from_gray(225));
                    ui.painter().image(
                        texture.id(),
                        image,
                        Rect::from_min_max(Pos2::ZERO, pos2(1., 1.)),
                        Color32::WHITE,
                    );
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("Export Archive…")).clicked() {
                        if let Some(path) = self
                            .editor
                            .native
                            .as_ref()
                            .and_then(|n| n.replay_dialog(true, false))
                        {
                            let source = if panel.loaded { "loaded" } else { "current" };
                            match self.editor.session.apply(crate::application::Action::new(
                                "replay.export",
                                serde_json::json!({"path":path,"source":source}),
                            )) {
                                Ok(_) => panel.message = "Replay archive exported".into(),
                                Err(e) => panel.message = e.to_string(),
                            }
                        }
                    }
                    if ui
                        .button(tr("Export PNG Frames…"))
                        .on_hover_text(tr("Choose a new folder name for a PNG image sequence."))
                        .clicked()
                    {
                        if let Some(path) = self
                            .editor
                            .native
                            .as_ref()
                            .and_then(|n| n.replay_dialog(true, true))
                        {
                            let source = if panel.loaded { "loaded" } else { "current" };
                            match self.editor.session.apply(crate::application::Action::new(
                                "replay.frames",
                                serde_json::json!({"path":path,"source":source,"fps":12}),
                            )) {
                                Ok(r) => panel.message = format!("{} PNG", r["result"]["frames"]),
                                Err(e) => panel.message = e.to_string(),
                            }
                        }
                    }
                    if let Some(verified) = &panel.verified {
                        ui.weak(tr(verified));
                    }
                });
                ui.weak(tr(
                    "Preview uses recorded results. The live artwork is unchanged.",
                ));
                if !panel.message.is_empty() {
                    ui.label(tr(&panel.message));
                }
                if let Some(error) = &self.editor.session.replay_error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        panel.open = open;
        if panel.playing {
            ctx.request_repaint_after(Duration::from_secs_f32(1. / panel.rate));
        }
        self.replay = panel;
    }
}
