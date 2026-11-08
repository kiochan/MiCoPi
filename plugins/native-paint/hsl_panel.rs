//! Artist-facing local HSL draft. Only Apply publishes a document operation.
use super::*;
use crate::{
    application::Session,
    engine::hsl::Hsl,
    plugins::hsl_preview::{self, Request},
};
use std::time::Duration;

const SETTLE: Duration = Duration::from_millis(150);
pub(super) struct Draft {
    request: Request,
    layer_name: String,
    preview: bool,
    observed: Hsl,
    changed: Instant,
    attempted: Option<Hsl>,
    painted: Option<Hsl>,
    texture: Option<TextureHandle>,
    message: String,
    #[cfg(test)]
    renders: usize,
}
impl Draft {
    pub(super) fn new(s: &Session, args: &serde_json::Value) -> Option<Self> {
        let hsl: Hsl = serde_json::from_value(args.clone()).ok()?;
        Some(Self {
            request: Request {
                document_identity: s.doc.identity,
                document_revision: s.revision,
                layer_id: s.doc.layer_id(s.doc.active),
                hsl,
            },
            layer_name: s.doc.layer_name(s.doc.active),
            preview: true,
            observed: hsl,
            changed: Instant::now(),
            attempted: None,
            painted: None,
            texture: None,
            message: String::new(),
            #[cfg(test)]
            renders: 0,
        })
    }
    fn source_matches(&self, s: &Session) -> bool {
        self.request.document_identity == s.doc.identity
            && self.request.document_revision == s.revision
            && self.request.layer_id == s.doc.layer_id(s.doc.active)
    }
    fn contact(s: &Session) -> bool {
        s.stroke.active() || s.pointer_checkpoint.is_some()
    }
    pub(super) fn texture(&self, s: &Session) -> Option<&TextureHandle> {
        (self.preview
            && self.source_matches(s)
            && !Self::contact(s)
            && self.painted == Some(self.request.hsl))
        .then_some(self.texture.as_ref())
        .flatten()
    }
    fn refresh_source(&mut self, s: &Session) {
        self.request.document_identity = s.doc.identity;
        self.request.document_revision = s.revision;
        self.request.layer_id = s.doc.layer_id(s.doc.active);
        self.layer_name = s.doc.layer_name(s.doc.active);
        self.attempted = None;
        self.painted = None;
        self.texture = None;
        self.message.clear();
        self.changed = Instant::now();
    }
    fn update_preview(&mut self, ctx: &Context, s: &Session) {
        if self.observed != self.request.hsl {
            self.observed = self.request.hsl;
            self.changed = Instant::now();
            self.attempted = None;
            self.message.clear();
        }
        if !self.preview
            || !self.source_matches(s)
            || Self::contact(s)
            || self.attempted == Some(self.request.hsl)
        {
            return;
        }
        if self.changed.elapsed() < SETTLE {
            ctx.request_repaint_after(SETTLE.saturating_sub(self.changed.elapsed()));
            return;
        }
        self.attempted = Some(self.request.hsl);
        // A neutral draft already has the committed surface. Avoid rendering
        // and uploading a second whole-canvas texture merely to open controls.
        if self.request.hsl == Hsl::default() {
            self.painted = Some(self.request.hsl);
            self.texture = None;
            return;
        }
        #[cfg(test)]
        {
            self.renders += 1;
        }
        match hsl_preview::render(s, self.request) {
            Ok((pixels, _)) => {
                let width = s.doc.width;
                let colors = pixels
                    .into_iter()
                    .enumerate()
                    .map(|(index, pixel)| {
                        let (x, y) = (index % width, index / width);
                        let background = if (x / 12 + y / 12) % 2 == 0 {
                            0xfffdfdfd
                        } else {
                            0xfff3f5f7
                        };
                        let pixel = crate::engine::over(background, pixel);
                        Color32::from_rgb((pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8)
                    })
                    .collect();
                let image = ColorImage::new([width, s.doc.height], colors);
                if let Some(texture) = &mut self.texture {
                    texture.set(image, TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(ctx.load_texture("hsl-color-draft", image, TextureOptions::LINEAR));
                }
                self.painted = Some(self.request.hsl);
                self.message.clear();
            }
            Err(error) => {
                self.painted = None;
                self.message = error.to_string();
            }
        }
        ctx.request_repaint();
    }
}
impl Editor {
    pub(super) fn receive_hsl_draft(&mut self, effect: &serde_json::Value) {
        if effect["document_identity"].as_u64() != Some(self.session.doc.identity)
            || effect["revision"].as_u64() != Some(self.session.revision)
            || effect["layer_id"].as_u64()
                != Some(self.session.doc.layer_id(self.session.doc.active))
        {
            self.status = "Document changed; reopen Hue/Saturation.".into();
            return;
        }
        self.begin_plugin_command_with_args("layer.hue_saturation", effect["value"].clone());
    }
    pub(super) fn hsl_command_window(&mut self, ctx: &Context, draft: &mut Draft) -> bool {
        let a = &self.settings.ui;
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let available =
            (ctx.content_rect().width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 2.);
        let width = (a.control_height * 13.).min(available);
        let stale = !draft.source_matches(&self.session);
        let contact = Draft::contact(&self.session);
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        Window::new(tr_for(Some("native_paint"), "Hue/Saturation"))
            .id(Id::new("hsl-color-draft-window"))
            .open(&mut open)
            .collapsible(false)
            .frame(frame)
            .default_width(width)
            .min_width(width.min(a.control_height * 7.))
            .max_width(available)
            .resizable(true)
            .show(ctx, |ui| {
                ui.style_mut().wrap_mode = Some(TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing = vec2(a.item_spacing, a.item_spacing);
                ui.spacing_mut().interact_size.y = a.control_height.max(1.5 * a.font_size);
                ui.label(RichText::new(localize("Active Layer")).strong());
                ui.add(Label::new(&draft.layer_name).truncate())
                    .on_hover_text(&draft.layer_name);
                ui.weak(tr("Preview keeps the document unchanged until Apply."));
                ui.add_space(a.section_spacing);
                ScrollArea::vertical()
                    .max_height(
                        (ctx.content_rect().height()
                            - a.control_height * 7.
                            - a.font_size * 4.
                            - a.panel_padding * 4.)
                            .max(a.control_height * 2.),
                    )
                    .show(ui, |ui| {
                        nodes::hsl_ui::edit(
                            ui,
                            Id::new("direct-hsl-controls"),
                            &mut draft.request.hsl,
                        );
                        if stale {
                            ui.weak(tr("Document Changed; Refresh Preview Source"));
                            let refresh =
                                ui.add_enabled(!contact, Button::new(tr("Refresh Preview Source")));
                            #[cfg(test)]
                            ui.data_mut(|d| {
                                d.insert_temp(Id::new("hsl-refresh-source"), refresh.rect)
                            });
                            if refresh.clicked() {
                                draft.refresh_source(&self.session);
                            }
                        }
                        if contact {
                            ui.weak(tr(
                                "Finish the current contact before requesting a color preview",
                            ));
                        }
                        if !draft.message.is_empty() {
                            ui.label(localize(&draft.message));
                        }
                    });
                ui.add_space(a.section_spacing);
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut draft.preview, tr("Preview"));
                    let refresh = ui.add_enabled(
                        !stale && !contact && draft.preview,
                        Button::new(tr("Refresh Preview")),
                    );
                    if refresh.clicked() {
                        draft.attempted = None;
                        draft.changed = Instant::now() - SETTLE;
                    }
                });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let run = ui.add_enabled(!stale && !contact, Button::new(tr("Apply")));
                    let close = ui.button(tr("Cancel"));
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(Id::new("hsl-apply"), run.rect);
                        d.insert_temp(Id::new("hsl-cancel"), close.rect);
                    });
                    apply = run.clicked();
                    cancel = close.clicked();
                });
            });
        if !open || cancel || ctx.input(|i| i.key_pressed(Key::Escape)) {
            ctx.request_repaint();
            return false;
        }
        if apply {
            let previous = self.backend_contact_stamp();
            let result = self.session.apply(draft.request.apply_action());
            self.reconcile_command_contact(previous);
            self.sync_from_session();
            match result {
                Ok(_) => {
                    ctx.request_repaint();
                    return false;
                }
                Err(error) => draft.message = error.to_string(),
            }
        } else {
            draft.update_preview(ctx, &self.session);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn studio() -> Studio {
        let mut app = crate::desktop::tests::studio();
        app.editor.session = Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        app.editor.register_commands();
        app.editor
            .core("canvas.new", json!({"width":12,"height":8}));
        app.editor
            .core("layer.fill_color", json!({"color":"#ff000080"}));
        app.editor.sync_from_session();
        app
    }
    fn draft(app: &mut Studio) -> &mut Draft {
        app.editor
            .command_form
            .as_mut()
            .unwrap()
            .hsl
            .as_mut()
            .unwrap()
    }
    fn frame(app: &mut Studio, ctx: &Context, size: Vec2, events: Vec<Event>) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                focused: true,
                ..Default::default()
            },
            |ctx| app.editor.plugin_command_window(ctx),
        );
    }
    fn settle(app: &mut Studio, ctx: &Context) {
        draft(app).changed = Instant::now() - SETTLE;
        for _ in 0..3 {
            frame(app, ctx, vec2(1000., 900.), vec![]);
        }
    }
    fn click(app: &mut Studio, ctx: &Context, name: &str) {
        let position = ctx
            .data_mut(|d| d.get_temp::<Rect>(Id::new(name)).unwrap())
            .center();
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec2(1000., 900.),
                vec![
                    Event::PointerMoved(position),
                    Event::PointerButton {
                        pos: position,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn hsl_open_command_queues_parameterized_controls_and_rejects_stale_effects() {
        let mut app = studio();
        let ctx = Context::default();
        app.editor.session.ui_effects.clear();
        let before = app.editor.session.state();
        app.editor.begin_plugin_command_with_args(
            "layer.hue_saturation.open",
            json!({"parameters":{"hue":120}}),
        );
        assert!(app.editor.command_form.is_none());
        assert_eq!(app.editor.session.ui_effects.len(), 1);
        app.consume_ui_effects(&ctx);
        assert_eq!(draft(&mut app).request.hsl.hue, 120.);
        assert_eq!(app.editor.session.state(), before);
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-cancel");
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.hue_saturation.open",
                json!({}),
            ))
            .unwrap();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.rename",
                json!({"name":"Changed after queue"}),
            ))
            .unwrap();
        app.consume_ui_effects(&ctx);
        assert!(app.editor.command_form.is_none());
        assert_eq!(
            app.editor.status,
            "Document changed; reopen Hue/Saturation."
        );
    }
    #[test]
    fn hsl_draft_debounces_reuses_texture_and_cancel_keeps_document_unchanged() {
        let mut app = studio();
        let ctx = Context::default();
        let before = app.editor.session.state();
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        let events = app.editor.session.journal.as_ref().unwrap().entries.len();
        app.editor
            .begin_plugin_command_with_args("layer.hue_saturation", json!({"hue":120}));
        assert!(app.editor.guarded_command_form());
        draft(&mut app).changed = Instant::now() + Duration::from_secs(60);
        frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
        assert_eq!(draft(&mut app).renders, 0);
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        let texture = app.editor.color_preview_texture().unwrap().id();
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        draft(&mut app).request.hsl.hue = -120.;
        frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
        assert!(app.editor.color_preview_texture().is_none());
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 2);
        assert_eq!(app.editor.color_preview_texture().unwrap().id(), texture);
        draft(&mut app).preview = false;
        assert!(app.editor.color_preview_texture().is_none());
        draft(&mut app).preview = true;
        assert_eq!(app.editor.color_preview_texture().unwrap().id(), texture);
        click(&mut app, &ctx, "hsl-cancel");
        assert!(app.editor.command_form.is_none());
        assert!(app.editor.color_preview_texture().is_none());
        assert_eq!(app.editor.session.state(), before);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events
        );
    }
    #[test]
    fn hsl_apply_click_publishes_one_revision_event_and_undo() {
        let mut app = studio();
        let ctx = Context::default();
        let before = app.editor.session.doc.layers[0].pixels.clone();
        let revision = app.editor.session.revision;
        let events = app.editor.session.journal.as_ref().unwrap().entries.len();
        app.editor
            .begin_plugin_command_with_args("layer.hue_saturation", json!({"hue":120}));
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-apply");
        assert!(app.editor.command_form.is_none());
        assert_eq!(app.editor.session.revision, revision + 1);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events + 1
        );
        assert_eq!(
            app.editor.session.doc.layers[0].pixels,
            vec![0x8000ff00; 96]
        );
        app.editor.core("history.undo", json!({}));
        assert_eq!(app.editor.session.doc.layers[0].pixels, before);
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
    }
    #[test]
    fn hsl_stale_draft_hides_preview_and_requires_explicit_source_refresh() {
        let mut app = studio();
        let ctx = Context::default();
        app.editor
            .begin_plugin_command_with_args("layer.hue_saturation", json!({"hue":120}));
        settle(&mut app, &ctx);
        let captured = draft(&mut app).request.document_revision;
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.rename",
                json!({"name":"New target name"}),
            ))
            .unwrap();
        assert!(app.editor.color_preview_texture().is_none());
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).request.document_revision, captured);
        let revision = app.editor.session.revision;
        click(&mut app, &ctx, "hsl-apply");
        assert_eq!(app.editor.session.revision, revision);
        assert!(app.editor.command_form.is_some());
        click(&mut app, &ctx, "hsl-refresh-source");
        assert_eq!(draft(&mut app).request.document_revision, revision);
        assert_eq!(draft(&mut app).layer_name, "New target name");
        assert!(app.editor.color_preview_texture().is_none());
        settle(&mut app, &ctx);
        assert!(app.editor.color_preview_texture().is_some());
    }
    #[test]
    fn hsl_cancel_preserves_live_pen_contact_and_failed_preview_is_not_retried_each_frame() {
        let mut app = studio();
        let ctx = Context::default();
        app.editor.session.apply(crate::application::Action::new("canvas.pointer",
            json!({"phase":"down","x":2,"y":2,"pressure":0.3,"pen":true,"time":1,"tool_override":"brush"}))).unwrap();
        let revision = app.editor.session.revision;
        app.editor
            .begin_plugin_command_with_args("layer.hue_saturation", json!({"hue":120}));
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 0);
        click(&mut app, &ctx, "hsl-cancel");
        assert!(app.editor.session.stroke.active());
        assert_eq!(app.editor.session.revision, revision);
        for (phase, time) in [("move", 2), ("up", 3)] {
            app.editor.session.apply(crate::application::Action::new("canvas.pointer",
                json!({"phase":phase,"x":5,"y":3,"pressure":0.8,"pen":true,"time":time,"tool_override":"brush"}))).unwrap();
        }
        app.editor.core("layer.set", json!({"pixel_locked":true}));
        app.editor
            .begin_plugin_command_with_args("layer.hue_saturation", json!({"hue":120}));
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        assert!(!draft(&mut app).message.is_empty());
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        assert!(app.editor.color_preview_texture().is_none());
    }
    #[test]
    fn hsl_window_footer_fits_four_languages_and_configured_metrics() {
        crate::i18n::load_test_catalogs();
        let mut app = studio();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for width in [320., 960.] {
                for large in [false, true] {
                    app.editor.settings.ui = crate::configuration::Appearance::default();
                    if large {
                        app.editor.settings.ui.font_size = 22.;
                        app.editor.settings.ui.control_height = 48.;
                    }
                    app.editor.begin_plugin_command("layer.hue_saturation");
                    let ctx = Context::default();
                    theme::fonts(&ctx);
                    theme::apply(&ctx, &app.editor.settings.ui);
                    for _ in 0..3 {
                        frame(&mut app, &ctx, vec2(width, 1000.), vec![]);
                    }
                    assert_eq!(draft(&mut app).renders, 0);
                    for name in ["hsl-apply", "hsl-cancel"] {
                        let rect = ctx.data_mut(|d| d.get_temp::<Rect>(Id::new(name)).unwrap());
                        assert!(
                            rect.is_positive()
                                && rect.left() >= -1.
                                && rect.right() <= width + 1.
                                && rect.top() >= 0.
                                && rect.bottom() <= 1000.,
                            "{language:?}/{width}/{large}/{name}: {rect:?}"
                        );
                    }
                }
            }
        }
    }
}
