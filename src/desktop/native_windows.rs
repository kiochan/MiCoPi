//! Native viewport presentation of plugin-owned dock trees. One Session, no pixel copies.
use super::*;
use super::{
    capture::PendingCapture,
    capture_writer::{CaptureSubmitter, Completion},
};
use egui_dock::{Surface, SurfaceIndex, WindowState};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering as AtomicOrdering, Arc};

pub(super) struct NativeWindow {
    pub native: Option<Native>,
    pub geometry: Rect,
    placement: Rect,
    pub viewport: ViewportId,
    pub focused: bool,
    pub pixels_per_point: f32,
    pub image_rect: Option<Rect>,
    pub outer_rect: Rect,
    pub maximized: bool,
    pub minimized: bool,
}
pub(super) fn dock_style(ctx: &Context, a: &Appearance) -> egui_dock::Style {
    let colors = theme::Colors::new(a);
    let mut style = egui_dock::Style::from_egui(ctx.style().as_ref());
    let radius = theme::card_radius(a);
    style.dock_area_padding = Some(Margin::ZERO);
    style.tab_bar.height = a.tab_height;
    style.tab_bar.bg_fill = colors.header;
    style.tab_bar.inner_margin = Margin::symmetric(8, 0);
    style.tab_bar.show_scroll_bar_on_overflow = false;
    // Group row and body read as one floating card.
    style.tab_bar.corner_radius = CornerRadius {
        nw: radius,
        ne: radius,
        sw: 0,
        se: 0,
    };
    style.tab_bar.hline_color = Color32::TRANSPARENT;
    style.tab.tab_body.inner_margin = Margin::same(a.panel_padding as i8);
    style.tab.tab_body.bg_fill = colors.panel;
    style.tab.tab_body.stroke = Stroke::NONE;
    style.tab.tab_body.corner_radius = CornerRadius {
        nw: 0,
        ne: 0,
        sw: radius,
        se: radius,
    };
    style.tab.hline_below_active_tab_name = false;
    style.tab.active.bg_fill = colors.panel;
    style.tab.focused.bg_fill = colors.panel;
    style.tab.focused.text_color = colors.accent_text;
    // Gaps between cards match the workspace edge inset.
    style.separator.width = a.outer_padding;
    style.separator.extra = a.tab_height + a.control_height + a.panel_padding * 2. + 8.;
    style.separator.color_idle = colors.background;
    style.separator.extra_interact_width = 4.;
    // Splitter gaps stay calm; the drawn grab handle carries the hover accent.
    style.separator.color_hovered = colors.background.lerp_to_gamma(colors.accent, 0.12);
    style.separator.color_dragged = colors.background.lerp_to_gamma(colors.accent, 0.2);
    style.overlay.selection_color = colors.accent.gamma_multiply(0.35);
    style.main_surface_border_stroke = Stroke::NONE;
    style
}
pub(super) fn geometry(state: &WindowState) -> Rect {
    // The pinned docking library has setters but no getters for pending geometry.
    // This layout-only conversion never runs in the native pen hook or DLL sample path.
    let value = serde_json::to_value(state).unwrap_or_default();
    let p = serde_json::from_value::<Pos2>(value["next_position"].clone()).ok();
    let size = serde_json::from_value::<Vec2>(value["next_size"].clone()).ok();
    let old = state.rect();
    Rect::from_min_size(
        p.unwrap_or(if old.is_finite() {
            old.min
        } else {
            pos2(80., 80.)
        }),
        size.unwrap_or(if old.is_finite() {
            old.size()
        } else {
            vec2(640., 460.)
        }),
    )
}

fn read_native_capture(
    request: &PendingCapture,
    viewport: ViewportId,
    size: [u32; 2],
    submitter: &CaptureSubmitter,
    readback: impl FnOnce([u32; 2]) -> Arc<ColorImage>,
) {
    if request.claimed.load(AtomicOrdering::Acquire) {
        return;
    }
    let permit = match submitter.try_reserve() {
        Ok(Some(permit)) => permit,
        Ok(None) => return,
        Err(error) => {
            if request
                .claimed
                .compare_exchange(false, true, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
                .is_ok()
            {
                submitter.complete(Completion {
                    request_id: request.request_id,
                    viewport,
                    path: request.path.clone(),
                    token: request.token.clone(),
                    result: Err(error),
                });
            }
            return;
        }
    };
    if request
        .claimed
        .compare_exchange(false, true, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
        .is_err()
    {
        return;
    }
    let result =
        capture_writer::checked_size([size[0] as usize, size[1] as usize]).and_then(|size| {
            let image = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| readback(size)))
                .map_err(|_| "Floating viewport screenshot readback failed".to_owned())?;
            permit.submit(capture_writer::Job {
                request_id: request.request_id,
                viewport,
                path: request.path.clone(),
                token: request.token.clone(),
                image,
            })
        });
    if let Err(error) = result {
        submitter.complete(Completion {
            request_id: request.request_id,
            viewport,
            path: request.path.clone(),
            token: request.token.clone(),
            result: Err(error),
        });
    }
}

impl Studio {
    pub(super) fn queue_native_capture(&mut self, value: &Value) {
        let path = PathBuf::from(value["path"].as_str().unwrap_or_default());
        let request_id = value["request_id"].as_u64();
        let panel = value["panel"]
            .as_str()
            .and_then(|id| self.editor.session.registry.panels.get(id))
            .map(contributed_panel);
        let viewport = panel
            .and_then(|panel| self.dock.find_tab(&panel))
            .filter(|(surface, _, _)| {
                !surface.is_main()
                    && matches!(self.dock.get_surface(*surface), Some(Surface::Window(_, _)))
            })
            .map(|(surface, _, _)| ViewportId::from_hash_of(("micopi-plugin-viewport", surface.0)));
        let pending = self
            .pending_window_captures
            .values()
            .map(std::collections::VecDeque::len)
            .sum::<usize>();
        let error = if viewport.is_none() {
            Some("Float the window before capturing it")
        } else if pending >= capture::CAPTURE_QUEUE_LIMIT {
            Some("Screenshot presentation queue is full")
        } else {
            None
        };
        if let Some(error) = error {
            let result = Err(error.to_owned());
            if let Some(id) = request_id {
                self.editor.session.finish_capture(id, result);
            } else {
                self.editor.session.ui["windows.capture"] = json!({
                    "path": path, "status": "failed", "error": error
                });
            }
            return;
        }
        let mut request = PendingCapture::new(path, request_id);
        request.panel = value["panel"].as_str().map(str::to_owned);
        self.pending_window_captures
            .entry(viewport.unwrap())
            .or_default()
            .push_back(request);
    }

    pub(super) fn finish_native_capture(&mut self, completion: &Completion) -> bool {
        let Some(queue) = self.pending_window_captures.get_mut(&completion.viewport) else {
            return false;
        };
        let Some(front) = queue.front() else {
            return false;
        };
        if !front.matches(&completion.token)
            || front.request_id != completion.request_id
            || front.path != completion.path
        {
            return false;
        }
        queue.pop_front();
        if queue.is_empty() {
            self.pending_window_captures.remove(&completion.viewport);
        }
        if let Some(id) = completion.request_id {
            self.editor
                .session
                .finish_capture(id, completion.result.clone());
        } else {
            self.editor.session.ui["windows.capture"] = match &completion.result {
                Ok([width, height]) => json!({
                    "path": completion.path,
                    "status": "saved", "width": width, "height": height, "error": null
                }),
                Err(error) => json!({
                    "path": completion.path,
                    "status": "failed", "error": error
                }),
            };
        }
        true
    }

    fn service_native_capture_waits(&mut self, ctx: &Context, viewports: &[ViewportId]) {
        let submitter = self.capture_writer.submitter();
        for (viewport, queue) in &self.pending_window_captures {
            let Some(front) = queue.front() else {
                continue;
            };
            if front.claimed.load(AtomicOrdering::Acquire) {
                // Submitted encoding is acknowledged by the worker. A timeout
                // here could report failure before an authorized file write.
                continue;
            }
            let still_in_viewport = front.panel.as_deref().is_none_or(|id| {
                self.editor
                    .session
                    .registry
                    .panels
                    .get(id)
                    .map(contributed_panel)
                    .and_then(|panel| self.dock.find_tab(&panel))
                    .is_some_and(|(surface, _, _)| {
                        !surface.is_main()
                            && ViewportId::from_hash_of(("micopi-plugin-viewport", surface.0))
                                == *viewport
                    })
            });
            let error = if !viewports.contains(viewport) || !still_in_viewport {
                Some("The floating viewport closed or docked before screenshot readback")
            } else if front.created.elapsed() >= capture::CAPTURE_TIMEOUT {
                Some("Timed out waiting for the floating viewport screenshot")
            } else {
                None
            };
            if let Some(error) = error {
                if front
                    .claimed
                    .compare_exchange(false, true, AtomicOrdering::AcqRel, AtomicOrdering::Acquire)
                    .is_ok()
                {
                    submitter.complete(Completion {
                        request_id: front.request_id,
                        viewport: *viewport,
                        path: front.path.clone(),
                        token: front.token.clone(),
                        result: Err(error.into()),
                    });
                }
            }
        }
        if !self.pending_window_captures.is_empty() {
            ctx.request_repaint_after_for(Duration::from_millis(16), ViewportId::ROOT);
        }
    }

    fn schedule_viewport_capture(&mut self, ctx: &Context, viewport: ViewportId) {
        if ctx.viewport_id() != viewport {
            return;
        }
        let Some(request) = self
            .pending_window_captures
            .get(&viewport)
            .and_then(|queue| queue.front())
            .filter(|request| !request.claimed.load(AtomicOrdering::Acquire))
            .cloned()
        else {
            return;
        };
        let submitter = self.capture_writer.submitter();
        let rect = ctx.viewport_rect();
        ctx.layer_painter(LayerId::new(Order::Debug, Id::new("native-window-capture")))
            .with_clip_rect(rect)
            .add(PaintCallback {
                rect,
                callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                    // UI layout can be rerun and discard these shapes. Only the
                    // actual paint callback claims a request, and all copies share
                    // that claim until the matching encoder acknowledgment.
                    read_native_capture(
                        &request,
                        viewport,
                        info.screen_size_px,
                        &submitter,
                        |size| Arc::new(painter.read_screen_rgba(size)),
                    );
                })),
            });
    }
    pub(super) fn absorb_native_surfaces(&mut self, mut local: DockState<Panel>) {
        for surface in local.iter_surfaces_mut().skip(1) {
            if let Surface::Window(_, _) = surface {
                let destination = self.dock.add_window(vec![]);
                *self.dock.get_surface_mut(destination).unwrap() =
                    std::mem::replace(surface, Surface::Empty);
            }
        }
    }
    pub(super) fn render_native_windows(&mut self, root: &Context) {
        let drag_preview = self.group_drag_preview(root);
        if let Some((_, target)) = &drag_preview {
            Self::paint_group_drop(root, target, &self.editor.settings.ui);
        }
        let indices: Vec<_> = self
            .dock
            .iter_surfaces()
            .enumerate()
            .filter_map(|(i, s)| matches!(s, Surface::Window(_, _)).then_some(i))
            .collect();
        let viewports: Vec<_> = indices
            .iter()
            .map(|index| ViewportId::from_hash_of(("micopi-plugin-viewport", *index)))
            .collect();
        self.service_native_capture_waits(root, &viewports);
        self.native_windows.retain(|i, _| indices.contains(i));
        let mut report_changed = false;
        for index in indices {
            let slot = self.dock.get_surface_mut(SurfaceIndex(index)).unwrap();
            let Surface::Window(tree, mut state) = std::mem::replace(slot, Surface::Empty) else {
                continue;
            };
            let mut local = DockState::new(vec![]);
            *local.main_surface_mut() = tree;
            local.translations = self.dock.translations.clone();
            let panels: Vec<_> = local.iter_all_tabs().map(|(_, p)| *p).collect();
            let contains_canvas = panels.contains(&Panel::Canvas);
            let single_leaf = local.main_surface().iter().filter(|n| n.is_leaf()).count() == 1;
            let desired = geometry(&state);
            let viewport = ViewportId::from_hash_of(("micopi-plugin-viewport", index));
            let bootstrap = format!("MiCoPi viewport {}:{index}", std::process::id());
            let mut label = panels
                .first()
                .and_then(|p| self.editor.panel_id(*p))
                .and_then(|id| self.editor.session.registry.panels.get(&id))
                .map(|p| localize(p.title.clone()))
                .unwrap_or_else(|| "MiCoPi".into());
            if panels.len() > 1 {
                label = format!("{} · {}", crate::i18n::text("Panel Group"), panels.len());
            }
            let title = format!("MiCoPi — {label}");
            let mut window = self.native_windows.remove(&index).unwrap_or(NativeWindow {
                native: None,
                geometry: desired,
                placement: desired,
                viewport,
                focused: false,
                pixels_per_point: 1.,
                image_rect: None,
                outer_rect: desired,
                maximized: false,
                minimized: false,
            });
            let first = window.native.is_none();
            // Only explicit layout commands change the builder. OS observations are persisted
            // separately, so asynchronous resize/DPI events cannot feed back stale dimensions.
            if desired != window.geometry {
                window.placement = desired;
            }

            let previous = (
                window.geometry,
                window.focused,
                window.pixels_per_point,
                window.image_rect,
                window.outer_rect,
                window.maximized,
                window.minimized,
            );
            let a = self.editor.settings.ui.clone();
            let builder = ViewportBuilder::default()
                .with_title(if first { &bootstrap } else { &title })
                .with_active(false)
                .with_decorations(false)
                .with_resizable(true)
                .with_inner_size(window.placement.size())
                .with_position(window.placement.min)
                .with_min_inner_size(if contains_canvas {
                    vec2(260., 220.)
                } else if panels.contains(&Panel::Tools) || panels.contains(&Panel::ToolOptions) {
                    vec2(
                        180.,
                        a.tab_height
                            + a.control_height
                            + 2. * (a.outer_padding + a.panel_padding)
                            + 8.,
                    )
                } else {
                    vec2(180., 100.)
                });
            let mut close = false;

            root.show_viewport_immediate(viewport, builder, |ctx, class| {
                if class == ViewportClass::Embedded {
                    return;
                }

                if window.native.is_none() {
                    window.native = Native::for_viewport(&bootstrap, ctx.clone());
                }
                let colors = theme::Colors::new(&a);
                if let Some(native) = &window.native {
                    let mut t = crate::ui::Theme::preset(if a.theme == "graphite" { 2 } else { 0 });
                    t.selected = u32::from_be_bytes([
                        0,
                        colors.border.r(),
                        colors.border.g(),
                        colors.border.b(),
                    ]);
                    // DWM updates only when the configured theme changes.
                    let theme_key = Id::new(("native-window-theme", index));
                    if first
                        || ctx.data_mut(|d| {
                            let changed = d.get_temp::<Appearance>(theme_key).as_ref() != Some(&a);
                            if changed {
                                d.insert_temp(theme_key, a.clone());
                            }
                            changed
                        })
                    {
                        native.appearance(t, a.theme == "graphite");
                    }
                    if contains_canvas {
                        native.drain_pen(&mut self.editor.packets);
                    }
                }
                // Preserve the unique bootstrap title until HWND discovery has
                // succeeded, so a transient miss remains retryable next frame.
                if window.native.is_some() {
                    ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
                }
                window.maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                window.minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
                if let Some(rect) = ctx
                    .input(|i| i.viewport().outer_rect)
                    .filter(|r| r.is_finite() && r.is_positive())
                {
                    window.outer_rect = rect;
                    if !window.maximized && !window.minimized {
                        let size = ctx
                            .input(|i| i.viewport().inner_rect.map(|r| r.size()))
                            .unwrap_or(rect.size());
                        window.geometry = Rect::from_min_size(rect.min, size);
                    }
                }
                window.focused = ctx.input(|i| i.focused);
                window.pixels_per_point = ctx.pixels_per_point();
                close = ctx.input(|i| i.viewport().close_requested());
                if close {
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                }
                self.editor.shortcuts(ctx);
                if self.pending_group_drag.is_some_and(|p| panels.contains(&p))
                    && window.native.is_some()
                {
                    self.pending_group_drag = None;
                    if Native::primary_down() {
                        let native = window.native.as_ref().unwrap();
                        native.begin_group_drag(
                            pos2(
                                a.outer_padding + a.control_height * 0.5,
                                a.outer_padding + a.tab_height * 0.5,
                            ) * ctx.pixels_per_point(),
                        );
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                }
                CentralPanel::default()
                    .frame(
                        Frame::new()
                            .fill(colors.background)
                            .inner_margin(Margin::same(a.outer_padding as i8)),
                    )
                    .show(ctx, |ui| {
                        DockArea::new(&mut local)
                            .id(Id::new(("native-dock-tree", index)))
                            .style(dock_style(ctx, &a))
                            .draggable_tabs(false)
                            .show_close_buttons(false)
                            .show_leaf_close_all_buttons(false)
                            .show_leaf_collapse_buttons(false)
                            .show_inside(
                                ui,
                                &mut panels::Viewer {
                                    editor: &mut self.editor,
                                    can_float: true,
                                    native: window.native.as_ref(),
                                    single_leaf,
                                    focused: false,
                                },
                            );
                        // Splitters inside a floating group get the same grab handles.
                        let cards: Vec<Rect> = panels
                            .iter()
                            .filter_map(|p| self.editor.headers.get(p))
                            .map(|h| h.card)
                            .collect();
                        group_bar::splitter_handles(ui, &cards, &a);
                    });
                window.image_rect = if contains_canvas && self.editor.image_rect.is_finite() {
                    Some(self.editor.image_rect)
                } else {
                    None
                };
                if let Some((source, target)) = &drag_preview {
                    Self::paint_group_drop(ctx, target, &a);
                    if *source == index && !target.operation.is_empty() {
                        let painter = ctx.layer_painter(LayerId::new(
                            Order::Tooltip,
                            Id::new("group-docking-hint"),
                        ));
                        let hint = Rect::from_min_size(
                            pos2(a.outer_padding, a.tab_height + a.outer_padding),
                            vec2(
                                (ctx.content_rect().width() - a.outer_padding * 2.).max(1.),
                                a.control_height * 2.,
                            ),
                        );
                        painter.rect_filled(hint, 4., colors.raised);
                        let operation = match target.operation {
                            "dock_left" => "Dock Group Left",
                            "dock_right" => "Dock Group Right",
                            "dock_top" => "Dock Group Top",
                            "dock_bottom" => "Dock Group Bottom",
                            _ => "Merge Panel Group as Tabs",
                        };
                        let name = target
                            .target
                            .and_then(|p| self.editor.panel_id(p))
                            .and_then(|id| self.editor.session.registry.panels.get(&id))
                            .map(|p| localize(p.title.clone()))
                            .unwrap_or_else(|| crate::i18n::text("Workspace").into());
                        painter.text(
                            hint.center() - vec2(0., a.font_size * 0.7),
                            Align2::CENTER_CENTER,
                            crate::i18n::text("Release to Dock the Entire Group"),
                            FontId::proportional(a.small_font_size),
                            colors.muted,
                        );
                        painter.text(
                            hint.center() + vec2(0., a.font_size * 0.7),
                            Align2::CENTER_CENTER,
                            format!("{} · {name}", crate::i18n::text(operation)),
                            FontId::proportional(a.font_size),
                            colors.accent,
                        );
                    }
                }
                self.schedule_viewport_capture(ctx, viewport);
                if contains_canvas && !window.focused && self.editor.input_owned {
                    self.editor.finish_stroke();
                    self.editor.pen_active = false;
                }
            });
            if window.geometry != desired {
                self.layout_changed = Some(Instant::now());
            }
            state
                .set_position(window.geometry.min)
                .set_size(window.geometry.size());
            let tree = std::mem::replace(local.main_surface_mut(), egui_dock::Tree::new(vec![]));
            *self.dock.get_surface_mut(SurfaceIndex(index)).unwrap() = if tree.is_empty() {
                Surface::Empty
            } else {
                Surface::Window(tree, state)
            };
            self.absorb_native_surfaces(local);
            if close {
                if let Some(id) = panels.first().and_then(|p| self.editor.panel_id(*p)) {
                    self.editor.core(
                        "window.command",
                        json!({"panel":id,"scope":"group","operation":"hide"}),
                    );
                }
            }
            report_changed |= first
                || previous
                    != (
                        window.geometry,
                        window.focused,
                        window.pixels_per_point,
                        window.image_rect,
                        window.outer_rect,
                        window.maximized,
                        window.minimized,
                    );
            self.native_windows.insert(index, window);
        }
        self.maintain_window_owners();
        self.finish_group_drops(root);
        if report_changed {
            self.report_windows();
        }
    }
    fn maintain_window_owners(&self) {
        let root = self
            .editor
            .native
            .as_ref()
            .map(Native::window_id)
            .unwrap_or(0);
        if root == 0 {
            return;
        }
        let canvas = self.dock.find_tab(&Panel::Canvas).map(|(s, _, _)| s.0);
        let canvas_hwnd = canvas
            .and_then(|s| self.native_windows.get(&s))
            .and_then(|w| w.native.as_ref())
            .map(Native::window_id)
            .unwrap_or(root);
        for (index, window) in &self.native_windows {
            if let Some(native) = &window.native {
                native.set_owner(if Some(*index) == canvas {
                    root
                } else {
                    canvas_hwnd
                });
            }
        }
    }
    pub(super) fn native_window_state(&self, surface: SurfaceIndex) -> Option<Value> {
        let w = self.native_windows.get(&surface.0)?;
        let r = w.outer_rect;
        Some(
            json!({"viewport":format!("{:?}",w.viewport),"hwnd":w.native.as_ref().map(Native::window_id),
            "owner":w.native.as_ref().map(Native::owner),"focused":w.focused,"maximized":w.maximized,"minimized":w.minimized,"pixels_per_point":w.pixels_per_point,"image_rect":w.image_rect,"bounds":{"x":r.min.x,"y":r.min.y,"width":r.width(),"height":r.height()}}),
        )
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    fn floated() -> (Studio, Context, ViewportId) {
        let mut app = crate::desktop::tests::studio();
        let ctx = Context::default();
        app.apply_window_command(
            &ctx,
            &json!({"panel":"tool-options","operation":"float","width":280,"height":300}),
        )
        .unwrap();
        let (surface, _, _) = app.dock.find_tab(&Panel::ToolOptions).unwrap();
        let viewport = ViewportId::from_hash_of(("micopi-plugin-viewport", surface.0));
        (app, ctx, viewport)
    }

    #[test]
    fn native_capture_survives_discarded_layout_and_acknowledges_only_its_front() {
        let (mut app, ctx, viewport) = floated();
        for path in ["first.png", "second.png"] {
            app.queue_native_capture(&json!({"panel":"tool-options","path":path}));
        }
        let first = app.pending_window_captures[&viewport][0].clone();
        let mut passes = 0;
        let mut input = RawInput {
            viewport_id: viewport,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(280., 300.))),
            ..Default::default()
        };
        input.viewports.insert(viewport, Default::default());
        let output = ctx.run(input, |ctx| {
            passes += 1;
            app.schedule_viewport_capture(ctx, viewport);
            if passes == 1 {
                ctx.request_discard("capture layout regression");
            }
        });
        assert!(passes > 1);
        assert!(output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, Shape::Callback(_))));
        assert_eq!(app.pending_window_captures[&viewport].len(), 2);
        assert!(!first.claimed.load(AtomicOrdering::Acquire));

        let mut completion = Completion {
            request_id: first.request_id,
            viewport,
            path: first.path.clone(),
            token: UserData::new(()),
            result: Err("Synthetic encoder failure".into()),
        };
        assert!(!app.finish_native_capture(&completion));
        completion.token = first.token;
        assert!(app.finish_native_capture(&completion));
        assert_eq!(app.pending_window_captures[&viewport].len(), 1);
        assert!(!app.finish_native_capture(&completion));
        assert_eq!(
            app.pending_window_captures[&viewport][0].path,
            PathBuf::from("second.png")
        );
    }

    #[test]
    fn queued_native_capture_fails_if_its_panel_docks_or_reuses_another_viewport() {
        let (mut app, ctx, viewport) = floated();
        app.queue_native_capture(&json!({"panel":"tool-options","path":"docked.png"}));
        app.apply_window_command(
            &ctx,
            &json!({"panel":"tool-options","operation":"dock_top","target":"canvas"}),
        )
        .unwrap();
        // Even if a new group reuses the old viewport index, the original panel
        // no longer belongs there and must never receive that group's image.
        app.service_native_capture_waits(&ctx, &[viewport]);
        let completions = app.capture_writer.drain();
        assert_eq!(completions.len(), 1);
        assert!(completions[0].result.is_err());
        assert!(app.finish_native_capture(&completions[0]));
        assert!(app.pending_window_captures.is_empty());
        app.queue_native_capture(&json!({"panel":"tool-options","path":"not-floating.png"}));
        assert!(app.pending_window_captures.is_empty());
        assert_eq!(app.editor.session.ui["windows.capture"]["status"], "failed");
    }

    #[test]
    fn submitted_native_encoding_is_not_failed_by_a_later_viewport_close() {
        let (mut app, ctx, viewport) = floated();
        app.queue_native_capture(&json!({"panel":"tool-options","path":"submitted.png"}));
        app.pending_window_captures[&viewport][0]
            .claimed
            .store(true, AtomicOrdering::Release);
        app.service_native_capture_waits(&ctx, &[]);
        assert!(app.capture_writer.drain().is_empty());
        assert_eq!(app.pending_window_captures[&viewport].len(), 1);
    }

    #[test]
    fn identified_native_completions_and_queue_failures_preserve_recorded_public_state() {
        let (mut app, ctx, viewport) = floated();
        let state = app.editor.session.state();
        let id = app
            .editor
            .session
            .queue_capture(Some("tool-options"), "normal.png")
            .unwrap();
        app.queue_native_capture(&json!({
            "panel":"tool-options", "path":"normal.png", "request_id":id
        }));
        let request = app.pending_window_captures[&viewport][0].clone();
        assert!(app.finish_native_capture(&Completion {
            request_id: Some(id),
            viewport,
            path: request.path,
            token: request.token,
            result: Ok([280, 300]),
        }));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(
            app.editor.session.capture_diagnostics()["last"]["status"],
            "saved"
        );
        app.apply_window_command(
            &ctx,
            &json!({"panel":"tool-options","operation":"dock_top","target":"canvas"}),
        )
        .unwrap();
        let id = app
            .editor
            .session
            .queue_capture(Some("tool-options"), "docked.png")
            .unwrap();
        app.queue_native_capture(&json!({
            "panel":"tool-options", "path":"docked.png", "request_id":id
        }));
        assert_eq!(app.editor.session.state(), state);
        assert_eq!(
            app.editor.session.capture_diagnostics()["last"]["status"],
            "failed"
        );
        assert!(app.pending_window_captures.is_empty());
    }

    #[test]
    fn native_readback_waits_for_capacity_and_runs_once_with_physical_dimensions() {
        let writer = capture_writer::CaptureWriter::default();
        let submitter = writer.submitter();
        let first = submitter.try_reserve().unwrap().unwrap();
        let second = submitter.try_reserve().unwrap().unwrap();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-output")
            .join(format!(
                "missing-native-capture-parent-{}",
                std::process::id()
            ))
            .join("frame.png");
        let request = PendingCapture::new(path, None);
        let viewport = ViewportId::from_hash_of("native-readback-test");
        let reads = std::cell::Cell::new(0);
        let readback = |size| {
            assert_eq!(size, [7, 3]);
            reads.set(reads.get() + 1);
            Arc::new(ColorImage::new([7, 3], vec![Color32::BLUE; 21]))
        };
        read_native_capture(&request, viewport, [7, 3], &submitter, readback);
        assert_eq!(reads.get(), 0);
        assert!(!request.claimed.load(AtomicOrdering::Acquire));
        drop(first);
        read_native_capture(&request, viewport, [7, 3], &submitter, readback);
        read_native_capture(&request, viewport, [7, 3], &submitter, readback);
        assert_eq!(reads.get(), 1);
        assert!(request.claimed.load(AtomicOrdering::Acquire));
        drop(second);
    }

    #[test]
    fn invalid_native_framebuffer_fails_without_invoking_readback() {
        let writer = capture_writer::CaptureWriter::default();
        let submitter = writer.submitter();
        let request = PendingCapture::new("unused.png".into(), None);
        let viewport = ViewportId::from_hash_of("invalid-native-readback-test");
        read_native_capture(&request, viewport, [0, 34], &submitter, |_| {
            panic!("Invalid framebuffer dimensions must reject before GL readback")
        });
        let completions = writer.drain();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].token, request.token);
        assert!(completions[0].result.is_err());
    }
}
