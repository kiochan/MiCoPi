//! Shared local color drafts. Only Apply publishes a document operation.
use super::*;
use crate::{
    application::Session,
    engine::hsl::Hsl,
    plugins::{
        color_preview::{Cost, Scope},
        grayscale_preview, hsl_preview, quantization_preview, tonal_preview,
    },
};
use std::time::Duration;

#[path = "tonal_controls.rs"]
mod tonal_controls;

const SETTLE: Duration = Duration::from_millis(150);
#[derive(Clone, Copy, Debug, PartialEq)]
enum Effect {
    Hsl(Hsl),
    Tonal(tonal_preview::Parameters),
    Grayscale(grayscale_preview::Parameters),
    Quantization(crate::engine::quantization::Quantization),
}
impl Effect {
    fn parse(action: &str, args: &serde_json::Value) -> Option<Self> {
        match action {
            "layer.hue_saturation" => serde_json::from_value(args.clone()).ok().map(Self::Hsl),
            "layer.brightness_contrast" => {
                serde_json::from_value(args.clone()).ok().map(Self::Tonal)
            }
            "layer.grayscale" => serde_json::from_value(args.clone())
                .ok()
                .map(Self::Grayscale),
            "layer.threshold" => {
                quantization_preview::parse(crate::engine::quantization::Effect::Threshold, args)
                    .ok()
                    .map(Self::Quantization)
            }
            "layer.posterize" => {
                quantization_preview::parse(crate::engine::quantization::Effect::Posterize, args)
                    .ok()
                    .map(Self::Quantization)
            }
            _ => None,
        }
    }
    fn action(self) -> &'static str {
        match self {
            Self::Hsl(_) => "layer.hue_saturation",
            Self::Tonal(_) => "layer.brightness_contrast",
            Self::Grayscale(_) => "layer.grayscale",
            Self::Quantization(value) => quantization_preview::action(value.effect),
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Hsl(_) => "Hue/Saturation",
            Self::Tonal(_) => "Brightness/Contrast",
            Self::Grayscale(_) => "Grayscale",
            Self::Quantization(value) => value.title(),
        }
    }
    fn window_id(self) -> &'static str {
        match self {
            Self::Hsl(_) => "hsl-color-draft-window",
            Self::Tonal(_) => "tonal-color-draft-window",
            Self::Grayscale(_) => "grayscale-color-draft-window",
            Self::Quantization(value) => match value.effect {
                crate::engine::quantization::Effect::Threshold => "threshold-color-draft-window",
                crate::engine::quantization::Effect::Posterize => "posterize-color-draft-window",
            },
        }
    }
    fn neutral(self) -> bool {
        match self {
            Self::Hsl(value) => value == Hsl::default(),
            Self::Tonal(value) => value == tonal_preview::Parameters::default(),
            Self::Grayscale(value) => value.amount == 0.,
            Self::Quantization(value) => {
                value.effect == crate::engine::quantization::Effect::Posterize && value.value == 256
            }
        }
    }
    fn edit(&mut self, ui: &mut Ui, id: Id) {
        match self {
            Self::Hsl(value) => {
                nodes::hsl_ui::edit(ui, id, value);
            }
            Self::Tonal(value) => {
                tonal_controls::edit(ui, id, value);
            }
            Self::Quantization(value) => {
                nodes::quantization_ui::edit(ui, id, value);
            }
            Self::Grayscale(value) => {
                tonal_controls::grayscale(ui, id, value);
            }
        }
    }
    fn apply_action(self, scope: Scope) -> crate::application::Action {
        crate::application::Action {
            action: self.action().into(),
            expected_revision: Some(scope.document_revision),
            args: match self {
                Self::Hsl(value) => serde_json::json!(value),
                Self::Tonal(value) => serde_json::json!(value),
                Self::Grayscale(value) => serde_json::json!(value),
                Self::Quantization(value) => quantization_preview::arguments(value),
            },
        }
    }
    fn render(
        self,
        s: &Session,
        scope: Scope,
    ) -> Result<(Vec<u32>, Cost), crate::application::ActionError> {
        match self {
            Self::Quantization(value) => quantization_preview::render(s, scope, value),
            Self::Hsl(hsl) => hsl_preview::render(
                s,
                hsl_preview::Request {
                    document_identity: scope.document_identity,
                    document_revision: scope.document_revision,
                    layer_id: scope.layer_id,
                    hsl,
                },
            ),
            Self::Tonal(parameters) => tonal_preview::render(
                s,
                tonal_preview::Request {
                    document_identity: scope.document_identity,
                    document_revision: scope.document_revision,
                    layer_id: scope.layer_id,
                    parameters,
                },
            ),
            Self::Grayscale(parameters) => grayscale_preview::render(
                s,
                grayscale_preview::Request {
                    document_identity: scope.document_identity,
                    document_revision: scope.document_revision,
                    layer_id: scope.layer_id,
                    parameters,
                },
            ),
        }
    }
}
pub(super) struct Draft {
    request: Scope,
    effect: Effect,
    layer_name: String,
    preview: bool,
    observed: Effect,
    changed: Instant,
    attempted: Option<Effect>,
    painted: Option<Effect>,
    texture: Option<TextureHandle>,
    message: String,
    #[cfg(test)]
    renders: usize,
}
impl Draft {
    pub(super) fn new(s: &Session, action: &str, args: &serde_json::Value) -> Option<Self> {
        let effect = Effect::parse(action, args)?;
        Some(Self {
            request: Scope {
                document_identity: s.doc.identity,
                document_revision: s.revision,
                layer_id: s.doc.layer_id(s.doc.active),
            },
            effect,
            layer_name: s.doc.layer_name(s.doc.active),
            preview: true,
            observed: effect,
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
            && self.painted == Some(self.effect))
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
        if self.observed != self.effect {
            self.observed = self.effect;
            self.changed = Instant::now();
            self.attempted = None;
            self.message.clear();
        }
        if !self.preview
            || !self.source_matches(s)
            || Self::contact(s)
            || self.attempted == Some(self.effect)
        {
            return;
        }
        if self.changed.elapsed() < SETTLE {
            ctx.request_repaint_after(SETTLE.saturating_sub(self.changed.elapsed()));
            return;
        }
        self.attempted = Some(self.effect);
        // A neutral draft already has the committed surface. Avoid rendering
        // and uploading a second whole-canvas texture merely to open controls.
        if self.effect.neutral() {
            self.painted = Some(self.effect);
            self.texture = None;
            return;
        }
        #[cfg(test)]
        {
            self.renders += 1;
        }
        match self.effect.render(s, self.request) {
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
                    self.texture = Some(ctx.load_texture(
                        self.effect.window_id(),
                        image,
                        TextureOptions::LINEAR,
                    ));
                }
                self.painted = Some(self.effect);
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
        self.receive_color_draft(
            effect,
            "layer.hue_saturation",
            "Document changed; reopen Hue/Saturation.",
        );
    }
    pub(super) fn receive_tonal_draft(&mut self, effect: &serde_json::Value) {
        self.receive_color_draft(
            effect,
            "layer.brightness_contrast",
            "Document changed; reopen Brightness/Contrast.",
        );
    }
    pub(super) fn receive_grayscale_draft(&mut self, effect: &serde_json::Value) {
        self.receive_color_draft(
            effect,
            "layer.grayscale",
            "Document changed; reopen Grayscale.",
        );
    }
    pub(super) fn receive_quantization_draft(
        &mut self,
        effect: &serde_json::Value,
        posterize: bool,
    ) {
        self.receive_color_draft(
            effect,
            if posterize {
                "layer.posterize"
            } else {
                "layer.threshold"
            },
            if posterize {
                "Document changed; reopen Posterize."
            } else {
                "Document changed; reopen Threshold."
            },
        );
    }
    fn receive_color_draft(&mut self, effect: &serde_json::Value, action: &str, stale: &str) {
        if effect["document_identity"].as_u64() != Some(self.session.doc.identity)
            || effect["revision"].as_u64() != Some(self.session.revision)
            || effect["layer_id"].as_u64()
                != Some(self.session.doc.layer_id(self.session.doc.active))
        {
            self.status = stale.into();
            return;
        }
        self.begin_plugin_command_with_args(action, effect["value"].clone());
    }
    pub(super) fn color_command_window(&mut self, ctx: &Context, draft: &mut Draft) -> bool {
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
        Window::new(tr_for(Some("native_paint"), draft.effect.title()))
            .id(Id::new(draft.effect.window_id()))
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
                        let id = Id::new(("direct-color-controls", draft.effect.action()));
                        draft.effect.edit(ui, id);
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
            let result = self.session.apply(draft.effect.apply_action(draft.request));
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
    fn grayscale_default_reset_apply_and_cancel_share_the_cached_draft() {
        let mut app = studio();
        let ctx = Context::default();
        let raw = app.editor.session.doc.layers[0].pixels.clone();
        let revision = app.editor.session.revision;
        let events = app.editor.session.journal.as_ref().unwrap().entries.len();
        app.editor.begin_plugin_command("layer.grayscale");
        assert_eq!(
            draft(&mut app).effect,
            Effect::Grayscale(grayscale_preview::Parameters { amount: 100. })
        );
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        assert!(app.editor.color_preview_texture().is_some());
        draft(&mut app).effect = Effect::Grayscale(grayscale_preview::Parameters { amount: 0. });
        frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
        settle(&mut app, &ctx);
        assert!(app.editor.color_preview_texture().is_none());
        assert_eq!(draft(&mut app).renders, 1);
        click(&mut app, &ctx, "grayscale-reset");
        settle(&mut app, &ctx);
        assert_eq!(
            draft(&mut app).effect,
            Effect::Grayscale(grayscale_preview::Parameters { amount: 100. })
        );
        assert_eq!(draft(&mut app).renders, 2);
        assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
        assert_eq!(app.editor.session.revision, revision);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events
        );
        click(&mut app, &ctx, "hsl-apply");
        assert!(app.editor.command_form.is_none());
        assert_eq!(
            app.editor.session.doc.layers[0].pixels,
            vec![0x80363636; 96]
        );
        assert_eq!(app.editor.session.revision, revision + 1);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events + 1
        );
        app.editor.core("history.undo", json!({}));
        assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
        let before = app.editor.session.state();
        app.editor.begin_plugin_command("layer.grayscale");
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-cancel");
        assert_eq!(app.editor.session.state(), before);
    }
    #[test]
    fn quantization_controls_reuse_preview_reset_apply_undo_and_discard_cancel() {
        use crate::engine::quantization::{Effect as Kind, Quantization};
        for (action, effect, default, expected) in [
            ("layer.threshold", Kind::Threshold, 128, 0x80000000),
            ("layer.posterize", Kind::Posterize, 4, 0x80005555),
        ] {
            let mut app = studio();
            let ctx = Context::default();
            app.editor
                .core("canvas.new", json!({"width":12,"height":8}));
            app.editor
                .core("layer.fill_color", json!({"color":"#20406080"}));
            let raw = app.editor.session.doc.layers[0].pixels.clone();
            assert_eq!(raw, vec![0x80204060; 96]);
            let revision = app.editor.session.revision;
            let events = app.editor.session.journal.as_ref().unwrap().entries.len();
            app.editor.begin_plugin_command(action);
            settle(&mut app, &ctx);
            assert_eq!(
                draft(&mut app).effect,
                Effect::Quantization(Quantization {
                    effect,
                    value: default
                })
            );
            assert_eq!(draft(&mut app).renders, 1);
            let texture = app.editor.color_preview_texture().unwrap().id();
            draft(&mut app).effect = Effect::Quantization(Quantization { effect, value: 2 });
            frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
            settle(&mut app, &ctx);
            assert_eq!(draft(&mut app).renders, 2);
            assert_eq!(app.editor.color_preview_texture().unwrap().id(), texture);
            let position = ctx
                .data_mut(|d| {
                    d.get_temp::<Rect>(Id::new(("direct-color-controls", action)).with("reset"))
                        .unwrap()
                })
                .center();
            for pressed in [true, false] {
                frame(
                    &mut app,
                    &ctx,
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
            settle(&mut app, &ctx);
            assert_eq!(
                draft(&mut app).effect,
                Effect::Quantization(Quantization {
                    effect,
                    value: default
                })
            );
            assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
            assert_eq!(app.editor.session.revision, revision);
            assert_eq!(
                app.editor.session.journal.as_ref().unwrap().entries.len(),
                events
            );
            click(&mut app, &ctx, "hsl-apply");
            assert!(app.editor.command_form.is_none());
            assert_eq!(app.editor.session.doc.layers[0].pixels, vec![expected; 96]);
            assert_eq!(app.editor.session.revision, revision + 1);
            assert_eq!(
                app.editor.session.journal.as_ref().unwrap().entries.len(),
                events + 1
            );
            app.editor.core("history.undo", json!({}));
            assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
            let before = app.editor.session.state();
            app.editor.begin_plugin_command(action);
            settle(&mut app, &ctx);
            click(&mut app, &ctx, "hsl-cancel");
            assert_eq!(app.editor.session.state(), before);
        }
    }
    #[test]
    fn posterize_identity_skips_render_and_quantization_open_discards_stale_delivery() {
        use crate::engine::quantization::{Effect as Kind, Quantization};
        let mut app = studio();
        let ctx = Context::default();
        app.editor
            .begin_plugin_command_with_args("layer.posterize", json!({"levels":256}));
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 0);
        assert!(app.editor.color_preview_texture().is_none());
        click(&mut app, &ctx, "hsl-cancel");
        for (action, posterize, effect, value) in [
            ("layer.threshold.open", false, Kind::Threshold, 128),
            ("layer.posterize.open", true, Kind::Posterize, 4),
        ] {
            app.editor.session.ui_effects.clear();
            let before = app.editor.session.state();
            app.editor.begin_plugin_command(action);
            app.consume_ui_effects(&ctx);
            assert_eq!(
                draft(&mut app).effect,
                Effect::Quantization(Quantization { effect, value })
            );
            assert_eq!(app.editor.session.state(), before);
            settle(&mut app, &ctx);
            click(&mut app, &ctx, "hsl-cancel");
            app.editor
                .session
                .apply(crate::application::Action::new(action, json!({})))
                .unwrap();
            app.editor.core(
                "layer.rename",
                json!({"name":format!("Changed source {action}")}),
            );
            app.consume_ui_effects(&ctx);
            assert!(app.editor.command_form.is_none());
            assert_eq!(
                app.editor.status,
                if posterize {
                    "Document changed; reopen Posterize."
                } else {
                    "Document changed; reopen Threshold."
                }
            );
        }
    }
    #[test]
    fn grayscale_open_delivery_preserves_defaults_and_discards_stale_requests() {
        let mut app = studio();
        let ctx = Context::default();
        app.editor.session.ui_effects.clear();
        let before = app.editor.session.state();
        app.editor.begin_plugin_command("layer.grayscale.open");
        assert!(app.editor.command_form.is_none());
        app.consume_ui_effects(&ctx);
        assert_eq!(
            draft(&mut app).effect,
            Effect::Grayscale(grayscale_preview::Parameters { amount: 100. })
        );
        assert_eq!(app.editor.session.state(), before);
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-cancel");
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.grayscale.open",
                json!({}),
            ))
            .unwrap();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.rename",
                json!({"name":"New source"}),
            ))
            .unwrap();
        app.consume_ui_effects(&ctx);
        assert!(app.editor.command_form.is_none());
        assert_eq!(app.editor.status, "Document changed; reopen Grayscale.");
    }
    #[test]
    fn tonal_controls_preview_reuse_reset_apply_undo_and_cancel_share_one_draft() {
        let mut app = studio();
        let ctx = Context::default();
        // Fill composites over existing pixels; start with a transparent source.
        app.editor
            .core("canvas.new", json!({"width":12,"height":8}));
        app.editor
            .core("layer.fill_color", json!({"color":"#20406080"}));
        let raw = app.editor.session.doc.layers[0].pixels.clone();
        assert_eq!(raw, vec![0x80204060; 96]);
        let revision = app.editor.session.revision;
        let events = app.editor.session.journal.as_ref().unwrap().entries.len();
        app.editor.begin_plugin_command_with_args(
            "layer.brightness_contrast",
            json!({"brightness":25,"contrast":100}),
        );
        settle(&mut app, &ctx);
        assert_eq!(draft(&mut app).renders, 1);
        let texture = app.editor.color_preview_texture().unwrap().id();
        draft(&mut app).effect = Effect::Tonal(tonal_preview::Parameters {
            brightness: 50.,
            contrast: 0.,
        });
        frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
        assert!(app.editor.color_preview_texture().is_none());
        settle(&mut app, &ctx);
        assert_eq!(app.editor.color_preview_texture().unwrap().id(), texture);
        assert_eq!(draft(&mut app).renders, 2);
        click(&mut app, &ctx, "tonal-reset");
        settle(&mut app, &ctx);
        assert!(draft(&mut app).effect.neutral());
        assert!(app.editor.color_preview_texture().is_none());
        assert_eq!(draft(&mut app).renders, 2);
        assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
        assert_eq!(app.editor.session.revision, revision);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events
        );
        draft(&mut app).effect = Effect::Tonal(tonal_preview::Parameters {
            brightness: 25.,
            contrast: 100.,
        });
        frame(&mut app, &ctx, vec2(1000., 900.), vec![]);
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-apply");
        assert!(app.editor.command_form.is_none());
        assert_eq!(
            app.editor.session.doc.layers[0].pixels,
            vec![0x800282ff; 96]
        );
        assert_eq!(app.editor.session.revision, revision + 1);
        assert_eq!(
            app.editor.session.journal.as_ref().unwrap().entries.len(),
            events + 1
        );
        app.editor.core("history.undo", json!({}));
        assert_eq!(app.editor.session.doc.layers[0].pixels, raw);
        let before = app.editor.session.state();
        app.editor
            .begin_plugin_command_with_args("layer.brightness_contrast", json!({"brightness":50}));
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-cancel");
        assert_eq!(app.editor.session.state(), before);
    }
    #[test]
    fn tonal_open_request_uses_the_shared_fenced_delivery_path() {
        let mut app = studio();
        let ctx = Context::default();
        app.editor.session.ui_effects.clear();
        let before = app.editor.session.state();
        app.editor.begin_plugin_command_with_args(
            "layer.brightness_contrast.open",
            json!({"parameters":{"brightness":25}}),
        );
        assert!(app.editor.command_form.is_none());
        app.consume_ui_effects(&ctx);
        assert_eq!(
            draft(&mut app).effect,
            Effect::Tonal(tonal_preview::Parameters {
                brightness: 25.,
                contrast: 0.
            })
        );
        assert_eq!(app.editor.session.state(), before);
        settle(&mut app, &ctx);
        click(&mut app, &ctx, "hsl-cancel");
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.brightness_contrast.open",
                json!({}),
            ))
            .unwrap();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.rename",
                json!({"name":"New source"}),
            ))
            .unwrap();
        app.consume_ui_effects(&ctx);
        assert!(app.editor.command_form.is_none());
        assert_eq!(
            app.editor.status,
            "Document changed; reopen Brightness/Contrast."
        );
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
        assert_eq!(
            draft(&mut app).effect,
            Effect::Hsl(Hsl {
                hue: 120.,
                ..Default::default()
            })
        );
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
        draft(&mut app).effect = Effect::Hsl(Hsl {
            hue: -120.,
            ..Default::default()
        });
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
            for (action, width) in [
                ("layer.hue_saturation", 320.),
                ("layer.hue_saturation", 960.),
                ("layer.brightness_contrast", 320.),
                ("layer.brightness_contrast", 960.),
                ("layer.grayscale", 320.),
                ("layer.grayscale", 960.),
                ("layer.threshold", 320.),
                ("layer.threshold", 960.),
                ("layer.posterize", 320.),
                ("layer.posterize", 960.),
            ] {
                for large in [false, true] {
                    app.editor.settings.ui = crate::configuration::Appearance::default();
                    if large {
                        app.editor.settings.ui.font_size = 22.;
                        app.editor.settings.ui.control_height = 48.;
                    }
                    app.editor.begin_plugin_command_with_args(
                        action,
                        if action == "layer.grayscale" {
                            json!({"amount":0})
                        } else if action == "layer.posterize" {
                            json!({"levels":256})
                        } else {
                            json!({})
                        },
                    );
                    if action == "layer.threshold" {
                        draft(&mut app).preview = false;
                    }
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
                            "{action}/{language:?}/{width}/{large}/{name}: {rect:?}"
                        );
                    }
                }
            }
        }
    }
}
