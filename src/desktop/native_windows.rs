//! Native viewport presentation of plugin-owned dock trees. One Session, no pixel copies.
use super::*;
use egui_dock::{Surface, SurfaceIndex, WindowState};
use serde_json::{json, Value};

pub(super) struct NativeWindow {
    pub native: Option<Native>,
    pub geometry: Rect,
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
    style.dock_area_padding = Some(Margin::ZERO);
    style.tab_bar.height = a.tab_height;
    style.tab_bar.bg_fill = colors.panel;
    style.tab_bar.inner_margin = Margin::symmetric(8, 0);
    style.tab.tab_body.inner_margin = Margin::same(a.panel_padding as i8);
    style.tab.tab_body.bg_fill = colors.panel;
    style.tab.tab_body.stroke = Stroke::new(1., colors.border);
    style.tab.tab_body.corner_radius = CornerRadius::same(a.corner_radius as u8);
    style.tab.hline_below_active_tab_name = true;
    style.tab.active.bg_fill = colors.panel;
    style.tab.focused.bg_fill = colors.panel;
    style.tab.focused.text_color = colors.accent;
    style.separator.width = a.item_spacing;
    style.separator.extra = a.tab_height + a.control_height + a.panel_padding * 2. + 8.;
    style.separator.color_idle = colors.background;
    style.main_surface_border_stroke = Stroke::NONE;
    style
}
fn geometry(state: &WindowState) -> Rect {
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
impl Studio {
    fn schedule_viewport_capture(&mut self, ctx: &Context, viewport: ViewportId) {
        let Some(path) = self.pending_window_captures.remove(&viewport) else {
            return;
        };
        let size = (ctx.content_rect().size() * ctx.pixels_per_point()).round();
        let results = self.window_capture_results.clone();
        let repaint = ctx.clone();
        ctx.layer_painter(LayerId::new(Order::Debug,Id::new("native-window-capture"))).add(PaintCallback {
            rect:ctx.content_rect(),
            callback:std::sync::Arc::new(eframe::egui_glow::CallbackFn::new(move |_,painter| {
                let image=painter.read_screen_rgba([size.x as u32,size.y as u32]);
                let rgba:Vec<u8>=image.pixels.iter().flat_map(|c|c.to_array()).collect();
                let result=image::save_buffer(&path,&rgba,image.width() as u32,image.height() as u32,image::ColorType::Rgba8);
                results.lock().unwrap().push(json!({"path":path,"status":if result.is_ok() {"saved"} else {"failed"},"error":result.err().map(|e|e.to_string())}));
                repaint.request_repaint_of(ViewportId::ROOT);
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
        for result in std::mem::take(&mut *self.window_capture_results.lock().unwrap()) {
            self.editor.session.ui["windows.capture"] = result;
        }
        let indices: Vec<_> = self
            .dock
            .iter_surfaces()
            .enumerate()
            .filter_map(|(i, s)| matches!(s, Surface::Window(_, _)).then_some(i))
            .collect();
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
            let desired = geometry(&state);
            let viewport = ViewportId::from_hash_of(("micopi-plugin-viewport", index));
            let bootstrap = format!("MiCoPi viewport {}:{index}", std::process::id());
            let label = panels
                .first()
                .and_then(|p| self.editor.panel_id(*p))
                .and_then(|id| self.editor.session.registry.panels.get(&id))
                .map(|p| localize(p.title.clone()))
                .unwrap_or_else(|| "MiCoPi".into());
            let title = format!("MiCoPi — {label}");
            let mut window = self.native_windows.remove(&index).unwrap_or(NativeWindow {
                native: None,
                geometry: desired,
                viewport,
                focused: false,
                pixels_per_point: 1.,
                image_rect: None,
                outer_rect: desired,
                maximized: false,
                minimized: false,
            });
            let first = window.native.is_none();
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
                .with_inner_size(desired.size())
                .with_position(desired.min)
                .with_min_inner_size(if contains_canvas {
                    vec2(260., 220.)
                } else if panels.contains(&Panel::Tools) {
                    vec2(
                        180.,
                        a.title_height
                            + a.tab_height
                            + a.control_height
                            + 2. * (a.outer_padding + a.panel_padding)
                            + 8.,
                    )
                } else {
                    vec2(180., 100.)
                });
            let mut close = false;
            let mut dock_back = false;
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
                    let theme_key = Id::new(("native-window-theme",index));
                    if first || ctx.data_mut(|d| {
                        let changed = d.get_temp::<Appearance>(theme_key).as_ref() != Some(&a);
                        if changed {
                            d.insert_temp(theme_key, a.clone());
                        }
                        changed
                    }) {
                        native.appearance(t, a.theme == "graphite");
                    }
                    if contains_canvas {
                        native.drain_pen(&mut self.editor.packets);
                    }
                }
                ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
                window.maximized=ctx.input(|i|i.viewport().maximized.unwrap_or(false));
                window.minimized=ctx.input(|i|i.viewport().minimized.unwrap_or(false));
                if let Some(rect) = ctx
                    .input(|i| i.viewport().outer_rect)
                    .filter(|r| r.is_finite() && r.is_positive())
                {
                    window.outer_rect=rect;
                    if !window.maximized && !window.minimized {window.geometry = rect;}
                }
                window.focused = ctx.input(|i| i.focused);
                window.pixels_per_point=ctx.pixels_per_point();
                close = ctx.input(|i| i.viewport().close_requested());
                if close {
                    ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                }
                self.editor.shortcuts(ctx);
                TopBottomPanel::top(Id::new(("native-window-title", index)))
                    .exact_height(a.title_height)
                    .frame(
                        Frame::new()
                            .fill(colors.background)
                            .inner_margin(Margin::symmetric(a.panel_padding as i8, 0)),
                    )
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            let width = (ui.available_width()
                                - a.control_height * 2.
                                - a.item_spacing * 2.)
                                .max(20.);
                            let response = ui.add_sized(
                                [width, a.title_height],
                                Label::new(tr(&label)).sense(Sense::click_and_drag()),
                            );
                            if response.drag_started() {
                                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                            }
                            if response.double_clicked() {
                                if let Some(id)=panels.first().and_then(|p|self.editor.panel_id(*p)) {
                                    self.editor.core("window.command",json!({"panel":id,"operation":if ctx.input(|i|i.viewport().maximized.unwrap_or(false)) {"restore"} else {"maximize"}}));
                                }
                            }
                            if ui.button("▣").on_hover_text(tr("Dock Window")).clicked() {
                                dock_back = true;
                            }
                            if ui.button("×").on_hover_text(tr("Hide Window")).clicked() {
                                close = true;
                            }
                        });
                    });
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
                            .show_leaf_close_all_buttons(false)
                            .show_leaf_collapse_buttons(false)
                            .show_inside(
                                ui,
                                &mut panels::Viewer {
                                    editor: &mut self.editor,
                                    can_float: true,
                                },
                            );
                    });
                window.image_rect=if contains_canvas && self.editor.image_rect.is_finite() {Some(self.editor.image_rect)} else {None};
                self.schedule_viewport_capture(ctx,viewport);
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
            if close || dock_back {
                for panel in panels {
                    if let Some(id) = self.editor.panel_id(panel) {
                        self.editor.core(
                            "window.command",
                            json!({"panel":id,"operation":if close {"hide"} else {"dock_right"}}),
                        );
                    }
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
        if report_changed {
            self.report_windows();
        }
    }
    pub(super) fn native_window_state(&self, surface: SurfaceIndex) -> Option<Value> {
        let w = self.native_windows.get(&surface.0)?;
        let r = w.outer_rect;
        Some(
            json!({"viewport":format!("{:?}",w.viewport),"hwnd":w.native.as_ref().map(Native::window_id),
            "focused":w.focused,"maximized":w.maximized,"minimized":w.minimized,"pixels_per_point":w.pixels_per_point,"image_rect":w.image_rect,"bounds":{"x":r.min.x,"y":r.min.y,"width":r.width(),"height":r.height()}}),
        )
    }
}
