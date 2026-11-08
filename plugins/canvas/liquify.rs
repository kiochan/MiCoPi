//! Cached whole-owner Liquify controls and typed canvas input adapter.
use super::*;
use crate::platform::windows::desktop_bridge::PenPhase;
use crate::plugins::liquify::{self, Push, Request};

pub(super) struct Draft {
    native: liquify::Draft,
    radius: f32,
    strength: f32,
    pressure: bool,
    previous: Option<(Pos2, u32)>,
    texture: Option<TextureHandle>,
    dirty: bool,
    last_preview: Instant,
    error: String,
    symmetry: (u32, f32, f32),
}
fn source_matches(d: &Draft, s: &crate::application::Session) -> bool {
    d.native.request.document_identity == s.doc.identity
        && d.native.request.document_revision == s.revision
}
fn arms(base: Push, axes: (u32, f32, f32)) -> ([Push; 4], usize) {
    let mut result = [base; 4];
    let mut n = 1;
    if axes.0 & micopi_plugin_api::SYMMETRY_X != 0 {
        result[n] = Push {
            x: 2. * axes.1 - base.x,
            dx: -base.dx,
            ..base
        };
        n += 1;
    }
    if axes.0 & micopi_plugin_api::SYMMETRY_Y != 0 {
        for i in 0..n {
            result[n + i] = Push {
                y: 2. * axes.2 - result[i].y,
                dy: -result[i].dy,
                ..result[i]
            };
        }
        n *= 2;
    }
    (result, n)
}
impl Editor {
    pub(super) fn receive_liquify(&mut self, effect: &serde_json::Value) {
        let parsed = serde_json::from_value::<Request>(effect["request"].clone());
        match parsed
            .map_err(|e| e.to_string())
            .and_then(|r| liquify::Draft::begin(&self.session, r))
        {
            Ok(native) => {
                self.transform = None;
                self.command_form = None;
                self.liquify = Some(Draft {
                    native,
                    radius: self.radius.max(4.).min(2048.),
                    strength: 0.6,
                    pressure: true,
                    previous: None,
                    texture: None,
                    dirty: true,
                    last_preview: Instant::now() - Duration::from_secs(1),
                    error: String::new(),
                    symmetry: (
                        self.session.parameters.symmetry.axes(),
                        self.session.parameters.symmetry_x * self.session.doc.width as f32,
                        self.session.parameters.symmetry_y * self.session.doc.height as f32,
                    ),
                });
            }
            Err(e) => self.status = e,
        }
    }
    pub(super) fn liquify_texture(&self) -> Option<&TextureHandle> {
        self.liquify
            .as_ref()
            .filter(|d| source_matches(d, &self.session) && d.error.is_empty())?
            .texture
            .as_ref()
    }
    pub(super) fn liquify_input(
        &mut self,
        ui: &Ui,
        bounds: Rect,
        layer: LayerId,
        events: &[Event],
    ) {
        let Some(mut d) = self.liquify.take() else {
            return;
        };
        let ctx = ui.ctx();
        let ppp = ctx.pixels_per_point();
        let on_canvas = |p: Pos2| bounds.contains(p) && ctx.layer_id_at(p) == Some(layer);
        let packets = std::mem::take(&mut self.packets);
        let has_pen = packets.iter().any(|p| {
            matches!(
                p.phase,
                PenPhase::Down | PenPhase::Move | PenPhase::Up | PenPhase::Cancel | PenPhase::Leave
            )
        });
        let mut sample = |phase: u8, p: Pos2, pressure: f32, time: u32, editor: &mut Editor| {
            if phase == 3 {
                d.previous = None;
                return;
            }
            editor.pen_hover = Some(p);
            if !source_matches(&d, &editor.session) || !d.error.is_empty() {
                d.previous = None;
                return;
            }
            let q = editor.document_position(p);
            if phase == 0 {
                if on_canvas(p) {
                    d.previous = Some((q, time));
                }
                return;
            }
            if let Some((old, t)) = d.previous {
                let dt = time.wrapping_sub(t).clamp(1, 100) as f32;
                let stabilization = editor.session.parameters.stabilizer.clamp(0., 1.);
                let alpha = 1. - (stabilization * 0.95).powf(dt / 16.);
                let q = old + (q - old) * alpha;
                let delta = q - old;
                let count = (delta.length() / d.radius).ceil().max(1.) as usize;
                if count > 64 {
                    d.error =
                        "Liquify movement exceeds the input batch limit; cancel and reopen".into();
                    d.previous = None;
                    return;
                }
                if delta.length_sq() > 0.000001 {
                    for i in 0..count {
                        let at = old + delta * ((i + 1) as f32 / count as f32);
                        let p = Push {
                            x: at.x,
                            y: at.y,
                            dx: delta.x / count as f32,
                            dy: delta.y / count as f32,
                            radius: d.radius,
                            strength: d.strength
                                * if d.pressure {
                                    pressure.clamp(0., 1.)
                                } else {
                                    1.
                                },
                        };
                        let (pushes, n) = arms(p, d.symmetry);
                        if let Err(e) = d.native.push_deferred(&editor.session, &pushes[..n]) {
                            d.error = e;
                            break;
                        }
                        d.dirty = true;
                    }
                }
                d.previous = if phase == 2 { None } else { Some((q, time)) };
            }
        };
        if has_pen {
            for p in &packets {
                let phase = match p.phase {
                    PenPhase::Down => 0,
                    PenPhase::Move => 1,
                    PenPhase::Up => 2,
                    PenPhase::Cancel | PenPhase::Leave => 3,
                    _ => continue,
                };
                sample(phase, pos2(p.x, p.y) / ppp, p.pressure, p.time, self);
            }
        } else {
            let time = ui.input(|i| (i.time * 1000.) as u32);
            for e in events {
                match e {
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        ..
                    } => sample(if *pressed { 0 } else { 2 }, *pos, 1., time, self),
                    Event::PointerMoved(p) => sample(1, *p, 1., time, self),
                    Event::WindowFocused(false) | Event::PointerGone => {
                        sample(3, Pos2::ZERO, 1., time, self)
                    }
                    _ => {}
                }
            }
        }
        let mut packets = packets;
        packets.clear();
        self.packets = packets;
        if d.dirty && d.error.is_empty() {
            if let Err(e) = d.native.flush(&self.session) {
                d.error = e;
            }
        }
        self.liquify = Some(d);
    }
    pub(super) fn liquify_cursor(&self, painter: &Painter, color: Color32) {
        let Some(d) = self
            .liquify
            .as_ref()
            .filter(|d| source_matches(d, &self.session))
        else {
            return;
        };
        if let Some(p) = self
            .pen_hover
            .or(self.mouse_position)
            .filter(|p| self.canvas.contains(*p))
        {
            painter.circle_stroke(p, d.radius * self.zoom, Stroke::new(1., color));
        }
    }
    pub(super) fn liquify_window(&mut self, ctx: &Context) {
        let Some(mut d) = self.liquify.take() else {
            return;
        };
        let a = self.settings.ui.clone();
        let c = theme::Colors::new(&a);
        let stale = !source_matches(&d, &self.session);
        let mut open = true;
        let mut apply = false;
        let mut reset = false;
        let mut cancel = false;
        let frame = Frame::window(&ctx.style()).inner_margin(Margin::same(a.panel_padding as i8));
        let width = (a.control_height * 13.).min(
            (ctx.content_rect().width() - a.outer_padding * 2. - frame.total_margin().sum().x)
                .max(a.control_height * 6.),
        );
        Window::new(tr("Multi-layer Liquify"))
            .id(Id::new("multi-layer-liquify"))
            .open(&mut open)
            .collapsible(false)
            .default_width(width)
            .min_width(width)
            .max_width(width)
            .frame(frame)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = vec2(a.item_spacing, a.item_spacing);
                ui.label(tr("Drag on the canvas to reshape the captured layers."));
                ui.label(tr("Whole layers; pixel selection is preserved."));
                ui.label(format!(
                    "{}: {:?}",
                    localize("Layers"),
                    d.native.request.layer_ids.len()
                ));
                ui.add(
                    Slider::new(&mut d.radius, d.native.request.spacing as f32..=2048.)
                        .logarithmic(true)
                        .text(tr("Radius")),
                );
                ui.add(Slider::new(&mut d.strength, 0. ..=1.).text(tr("Strength")));
                ui.checkbox(&mut d.pressure, tr("Pressure controls strength"));
                ui.label(format!(
                    "{}: {}",
                    localize("Stabilization"),
                    (self.session.parameters.stabilizer * 100.).round()
                ));
                if stale {
                    ui.colored_label(c.accent, tr("Liquify source changed; cancel and reopen."));
                }
                if !d.error.is_empty() {
                    ui.colored_label(c.accent, tr(&d.error));
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let reset_button = ui.add_enabled(!stale, Button::new(tr("Reset")));
                    reset = reset_button.clicked();
                    let cancel_button = ui.button(tr("Cancel"));
                    if cancel_button.clicked() {
                        cancel = true;
                    }
                    let apply_button = ui.add_enabled(
                        !stale
                            && d.error.is_empty()
                            && !d.native.request.steps.is_empty()
                            && d.previous.is_none(),
                        Button::new(tr("Apply")),
                    );
                    apply = apply_button.clicked();
                    #[cfg(test)]
                    ctx.data_mut(|data| {
                        data.insert_temp(Id::new("liquify-reset"), reset_button.rect);
                        data.insert_temp(Id::new("liquify-cancel"), cancel_button.rect);
                        data.insert_temp(Id::new("liquify-apply"), apply_button.rect);
                    });
                });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            open = false;
        }
        if reset {
            match d.native.reset(&self.session) {
                Ok(()) => {
                    d.previous = None;
                    d.error.clear();
                    d.dirty = true;
                }
                Err(e) => d.error = e,
            }
        }
        if apply {
            let action = d.native.request.apply_action();
            drop(d);
            self.core_at_revision(&action.action, action.args, action.expected_revision);
            self.sync_from_session();
            self.pen_hover = None;
            return;
        }
        if !open || cancel {
            self.pen_hover = None;
            return;
        }
        if d.dirty && !stale && d.error.is_empty() {
            // Full composition/texture upload is bounded and throttled; typed
            // field/owner ROI updates do not wait for this presentation step.
            if d.last_preview.elapsed() >= Duration::from_millis(100) {
                match d.native.preview(&self.session) {
                    Ok(pixels) => {
                        let w = self.session.doc.width;
                        let h = self.session.doc.height;
                        let colors = pixels
                            .into_iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let bg = if (i % w / 12 + i / w / 12) % 2 == 0 {
                                    0xfffdfdfd
                                } else {
                                    0xfff3f5f7
                                };
                                let p = crate::engine::over(bg, p);
                                Color32::from_rgb((p >> 16) as u8, (p >> 8) as u8, p as u8)
                            })
                            .collect();
                        let image = ColorImage::new([w, h], colors);
                        if let Some(t) = &mut d.texture {
                            t.set(image, TextureOptions::LINEAR);
                        } else {
                            d.texture = Some(ctx.load_texture(
                                "liquify-preview",
                                image,
                                TextureOptions::LINEAR,
                            ));
                        }
                        d.dirty = false;
                        d.last_preview = Instant::now();
                    }
                    Err(e) => d.error = e,
                }
            } else {
                ctx.request_repaint_after(
                    Duration::from_millis(100)
                        - d.last_preview.elapsed().min(Duration::from_millis(100)),
                );
            }
        }
        self.liquify = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn studio() -> Studio {
        let mut app = crate::desktop::tests::studio();
        app.editor.session = crate::application::Session::new(
            crate::plugins::Registry::load(
                &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
            )
            .unwrap(),
        )
        .unwrap();
        app.editor.session.runtime = "desktop";
        assert!(app
            .editor
            .core("canvas.new", json!({"width":12,"height":8})));
        assert!(app.editor.core("native.checkerboard", json!({"cell":2})));
        assert!(app.editor.core("layer.duplicate", json!({"layer_id":1})));
        app.editor.sync_from_session();
        app
    }
    fn open(app: &mut Studio) {
        let s = &app.editor.session;
        let effect = json!({"request":{"document_identity":s.doc.identity,"document_revision":s.revision,"layer_ids":[2,1],"spacing":1}});
        app.editor.receive_liquify(&effect);
        assert!(app.editor.liquify.is_some());
    }
    fn deform(app: &mut Studio) {
        let d = app.editor.liquify.as_mut().unwrap();
        d.native
            .push(
                &app.editor.session,
                &[Push {
                    x: 5.,
                    y: 4.,
                    dx: 1.,
                    dy: 0.5,
                    radius: 4.,
                    strength: 0.8,
                }],
            )
            .unwrap();
        d.dirty = true;
        d.last_preview = Instant::now() - Duration::from_secs(1);
    }
    fn frame(app: &mut Studio, ctx: &Context, size: Vec2, events: Vec<Event>) {
        let _ = ctx.run(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                events,
                focused: true,
                ..Default::default()
            },
            |ctx| app.editor.liquify_window(ctx),
        );
    }
    fn click(app: &mut Studio, ctx: &Context, id: &str) {
        let pos = ctx
            .data_mut(|d| d.get_temp::<Rect>(Id::new(id)).unwrap())
            .center();
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec2(1000., 800.),
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn liquify_controls_reset_cancel_apply_and_reuse_texture_without_source_edits() {
        let mut app = studio();
        let ctx = Context::default();
        theme::fonts(&ctx);
        let before: Vec<_> = app
            .editor
            .session
            .doc
            .layers
            .iter()
            .map(|l| l.pixels.clone())
            .collect();
        let rev = app.editor.session.revision;
        open(&mut app);
        deform(&mut app);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec2(1000., 800.), vec![]);
        }
        let texture = app
            .editor
            .liquify
            .as_ref()
            .unwrap()
            .texture
            .as_ref()
            .unwrap()
            .id();
        frame(&mut app, &ctx, vec2(1000., 800.), vec![]);
        assert_eq!(
            app.editor
                .liquify
                .as_ref()
                .unwrap()
                .texture
                .as_ref()
                .unwrap()
                .id(),
            texture
        );
        assert_eq!(app.editor.session.revision, rev);
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>(),
            before
        );
        click(&mut app, &ctx, "liquify-reset");
        assert!(app
            .editor
            .liquify
            .as_ref()
            .unwrap()
            .native
            .request
            .steps
            .is_empty());
        click(&mut app, &ctx, "liquify-cancel");
        assert!(app.editor.liquify.is_none());
        assert_eq!(app.editor.session.revision, rev);
        open(&mut app);
        deform(&mut app);
        for _ in 0..3 {
            frame(&mut app, &ctx, vec2(1000., 800.), vec![]);
        }
        click(&mut app, &ctx, "liquify-apply");
        assert!(app.editor.liquify.is_none());
        assert_eq!(app.editor.session.revision, rev + 1);
        assert!(app.editor.core("history.undo", json!({})));
        assert_eq!(
            app.editor
                .session
                .doc
                .layers
                .iter()
                .map(|l| l.pixels.clone())
                .collect::<Vec<_>>(),
            before
        );
    }
    #[test]
    fn reflected_liquify_arms_reflect_displacement_and_keep_strength() {
        let base = Push {
            x: 3.,
            y: 2.,
            dx: 1.5,
            dy: -0.5,
            radius: 8.,
            strength: 0.7,
        };
        let (p, n) = arms(
            base,
            (
                micopi_plugin_api::SYMMETRY_X | micopi_plugin_api::SYMMETRY_Y,
                10.,
                8.,
            ),
        );
        assert_eq!(n, 4);
        assert_eq!((p[1].x, p[1].dx, p[1].dy), (17., -1.5, -0.5));
        assert_eq!((p[2].y, p[2].dy), (14., 0.5));
        assert_eq!((p[3].x, p[3].y, p[3].dx, p[3].dy), (17., 14., -1.5, 0.5));
        assert!(p.iter().all(|p| p.strength == 0.7));
    }

    #[test]
    fn native_liquify_input_keeps_pressure_view_mapping_and_committed_canvas_idle() {
        use crate::platform::windows::desktop_bridge::PenSample;
        for mirrored in [false, true] {
            let mut app = studio();
            app.editor.session.parameters.stabilizer = 0.;
            app.editor.session.view.mirror_x = mirrored;
            app.editor.session.view.mirror_y = mirrored;
            open(&mut app);
            app.editor.zoom = 10.;
            app.editor.image_rect = Rect::from_min_size(Pos2::ZERO, vec2(120., 80.));
            let before = app.editor.session.doc.layers[0].pixels.clone();
            let revision = app.editor.session.revision;
            let ctx = Context::default();
            let bounds = Rect::from_min_size(Pos2::ZERO, vec2(800., 600.));
            let run = |app: &mut Studio| {
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(bounds),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            app.editor.liquify_input(ui, bounds, ui.layer_id(), &[]);
                        });
                    },
                );
            };
            run(&mut app);
            for (phase, x, pressure, time) in [
                (PenPhase::Down, 20., 0.25, 100),
                (PenPhase::Move, 30., 0.25, 116),
                (PenPhase::Up, 30., 0., 132),
            ] {
                app.editor.packets.push(PenSample {
                    phase,
                    x,
                    y: 20.,
                    pressure,
                    eraser: false,
                    time,
                });
            }
            run(&mut app);
            let d = app.editor.liquify.as_ref().unwrap();
            assert!(d.error.is_empty(), "{}", d.error);
            assert!(d.previous.is_none());
            assert_eq!(d.native.request.steps.len(), 1);
            let p = d.native.request.steps[0][0];
            assert!((p.strength - 0.15).abs() < 0.00001);
            assert_eq!(p.dx, if mirrored { -1. } else { 1. });
            assert_eq!(app.editor.session.doc.layers[0].pixels, before);
            assert_eq!(app.editor.session.revision, revision);
            assert!(!app.editor.session.stroke.active());
        }
    }

    #[test]
    fn liquify_footer_fits_english_cjk_and_configured_spacing() {
        crate::i18n::load_test_catalogs();
        for language in crate::i18n::Language::ALL {
            crate::i18n::set_language(language);
            for width in [360., 1000.] {
                let mut app = studio();
                app.editor.settings.ui.item_spacing = 10.;
                app.editor.settings.ui.panel_padding = 16.;
                open(&mut app);
                let ctx = Context::default();
                theme::fonts(&ctx);
                theme::apply(&ctx, &app.editor.settings.ui);
                for _ in 0..3 {
                    frame(&mut app, &ctx, vec2(width, 900.), vec![]);
                }
                for id in ["liquify-reset", "liquify-cancel", "liquify-apply"] {
                    let rect = ctx.data_mut(|d| d.get_temp::<Rect>(Id::new(id)).unwrap());
                    assert!(
                        rect.left() >= 0.
                            && rect.right() <= width + 1.
                            && rect.top() >= 0.
                            && rect.bottom() <= 900.,
                        "{language:?}/{width}/{id}: {rect:?}"
                    );
                }
            }
        }
        crate::i18n::set_language(crate::i18n::Language::English);
    }
}
