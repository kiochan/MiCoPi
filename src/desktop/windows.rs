//! Shared window lifecycle. Feature adapters never mutate dock internals themselves.
use super::*;
use serde_json::{json, Value};
impl Editor {
    pub(super) fn panel_id(&self, panel: Panel) -> Option<String> {
        self.session
            .registry
            .panels
            .values()
            .find(|p| contributed_panel(p) == panel)
            .map(|p| p.id.clone())
    }
    pub(super) fn window_menu(&mut self, panel: Panel, ui: &mut Ui) {
        let Some(id) = self.panel_id(panel) else {
            return;
        };
        {
            for (operation, label) in [
                ("focus", "Focus Window"),
                ("dock_left", "Dock Left"),
                ("dock_right", "Dock Right"),
                ("dock_top", "Dock Top"),
                ("dock_bottom", "Dock Bottom"),
            ] {
                if ui.button(tr(label)).clicked() {
                    self.core("window.command", json!({"panel":id,"operation":operation}));
                    ui.close();
                }
            }
            ui.menu_button(tr("Group as Tabs"), |ui| {
                let targets: Vec<_> = self
                    .session
                    .registry
                    .panels
                    .values()
                    .filter(|p| p.id != id)
                    .cloned()
                    .collect();
                for target in targets {
                    if ui.button(tr(&target.title)).clicked() {
                        self.core(
                            "window.command",
                            json!({"panel":id,"operation":"tab","target":target.id}),
                        );
                        ui.close();
                    }
                }
            });
        }
    }
}
impl Studio {
    pub(super) fn reset_window_input(&mut self) {
        self.editor.finish_stroke();
        self.editor.pen_active = false;
        self.editor.pen_hover = None;
        self.editor.mouse_position = None;
        self.editor.packets.clear();
        if let Some(native) = &self.editor.native {
            native.clear_pen();
        }
        for window in self.native_windows.values() {
            if let Some(native) = &window.native {
                native.clear_pen();
            }
        }
    }
    pub(super) fn report_windows(&mut self) {
        let actual:Vec<_>=self.editor.session.registry.panels.values().map(|definition| {
            let location=self.dock.find_tab(&contributed_panel(definition));
            let bounds=location.and_then(|(s,n,_)|match &self.dock[s][n] {egui_dock::Node::Leaf(leaf) if leaf.rect.is_finite() && leaf.rect.is_positive()=>Some(json!({"x":leaf.rect.left(),"y":leaf.rect.top(),"width":leaf.rect.width(),"height":leaf.rect.height()})),_=>None});
            let native=location.and_then(|(s,_,_)|self.native_window_state(s));
            let active=location.is_some_and(|(s,n,t)|matches!(&self.dock[s][n],egui_dock::Node::Leaf(leaf) if leaf.active==t));
            json!({"id":definition.id,"open":location.is_some(),"active":active,"floating":location.is_some_and(|(s,_,_)|!s.is_main()),"bounds":bounds,"native":native})
        }).collect();
        self.editor.session.ui["windows.actual"] = json!(actual);
    }
    pub(super) fn apply_window_command(
        &mut self,
        ctx: &Context,
        args: &Value,
    ) -> Result<(), String> {
        let definition = self
            .editor
            .session
            .registry
            .panels
            .get(args["panel"].as_str().ok_or("Missing window ID")?)
            .ok_or("Window plugin is unavailable")?;
        let panel = contributed_panel(definition);
        let operation = args["operation"]
            .as_str()
            .ok_or("Missing window operation")?;
        if matches!(operation, "minimize" | "maximize" | "restore") {
            self.reset_window_input();
            let (surface, _, _) = self.dock.find_tab(&panel).ok_or("Open the window first")?;
            let window = self
                .native_windows
                .get(&surface.0)
                .ok_or("Float the window first")?;
            match operation {
                "minimize" => {
                    ctx.send_viewport_cmd_to(window.viewport, ViewportCommand::Minimized(true))
                }
                "maximize" => {
                    ctx.send_viewport_cmd_to(window.viewport, ViewportCommand::Maximized(true))
                }
                _ => {
                    ctx.send_viewport_cmd_to(window.viewport, ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd_to(window.viewport, ViewportCommand::Maximized(false));
                }
            }
            return Ok(());
        }
        let target = args["target"]
            .as_str()
            .map(|id| {
                self.editor
                    .session
                    .registry
                    .panels
                    .get(id)
                    .map(contributed_panel)
                    .ok_or("Target window is unavailable")
            })
            .transpose()?;
        let operation = args["operation"]
            .as_str()
            .ok_or("Missing window operation")?;
        self.reset_window_input();
        let mut dock = self
            .dock_before_focus
            .as_ref()
            .unwrap_or(&self.dock)
            .clone();
        if operation == "hide" {
            if let Some(location) = dock.find_tab(&panel) {
                dock.remove_tab(location);
            }
        } else if matches!(operation, "show" | "focus") {
            if dock.find_tab(&panel).is_none() {
                if panel == Panel::Canvas {
                    if let Some((s, n, _)) = dock.find_tab(&Panel::Tools) {
                        dock[s].split_below(n, 0.15, vec![panel]);
                    } else {
                        dock.push_to_first_leaf(panel);
                    }
                } else if panel == Panel::Tools {
                    if let Some((s, n, _)) = dock.find_tab(&Panel::Canvas) {
                        dock[s].split_above(n, 0.15, vec![panel]);
                    } else {
                        dock.push_to_first_leaf(panel);
                    }
                } else if matches!(panel, Panel::Nodes | Panel::Workflow) {
                    if let Some((s, n, _)) = dock.find_tab(&Panel::Canvas) {
                        dock[s].split_below(n, 0.50, vec![panel]);
                    } else {
                        dock.push_to_first_leaf(panel);
                    }
                } else if let Some((s, n, _)) = dock
                    .find_tab(&Panel::Layers)
                    .or_else(|| dock.find_tab(&Panel::Navigator))
                {
                    dock.set_focused_node_and_surface((s, n));
                    dock.push_to_focused_leaf(panel);
                } else {
                    dock.push_to_first_leaf(panel);
                }
            }
            let (s, n, t) = dock.find_tab(&panel).unwrap();
            dock.set_active_tab((s, n, t));
            dock.set_focused_node_and_surface((s, n));
        } else if operation == "float" {
            if let Some(location) = dock.find_tab(&panel) {
                dock.remove_tab(location);
            }
            let surface = dock.add_window(vec![panel]);
            let appearance = &self.editor.settings.ui;
            let root = ctx.content_rect();
            let area = Rect::from_min_max(
                root.min
                    + vec2(
                        appearance.outer_padding,
                        appearance.title_height + appearance.outer_padding * 2.,
                    ),
                root.max
                    - vec2(
                        appearance.outer_padding,
                        24. + appearance.outer_padding * 2.,
                    ),
            );
            let decoration = ctx.style().spacing.window_margin.sum()
                + Vec2::splat(ctx.style().visuals.window_stroke.width * 2.);
            let mut size = vec2(
                args["width"]
                    .as_f64()
                    .map(|n| n as f32)
                    .unwrap_or(if panel == Panel::Canvas {
                        640.
                    } else if panel == Panel::Tools {
                        self.editor.settings.ui.toolbox_width
                    } else {
                        320.
                    }),
                args["height"]
                    .as_f64()
                    .map(|n| n as f32)
                    .unwrap_or(if panel == Panel::Tools {
                        self.editor.settings.ui.toolbox_height
                            + if ctx.embed_viewports() {
                                0.
                            } else {
                                appearance.title_height
                            }
                    } else {
                        460.
                    }),
            );
            if ctx.embed_viewports() {
                size = size.min((area.size() - decoration).max(vec2(48., 48.)));
            }
            let origin = ctx
                .input(|i| i.viewport().outer_rect.map(|r| r.min))
                .unwrap_or_default();
            let position = pos2(
                args["x"]
                    .as_f64()
                    .unwrap_or((origin.x + area.left() + 24.) as f64) as f32,
                args["y"]
                    .as_f64()
                    .unwrap_or((origin.y + area.top() + 24.) as f64) as f32,
            );
            let position = if ctx.embed_viewports() {
                position.clamp(area.min, area.max - size - decoration)
            } else {
                position
            };
            dock.get_window_state_mut(surface)
                .unwrap()
                .set_position(position)
                .set_size(size);
        } else {
            if let Some(location) = dock.find_tab(&panel) {
                dock.remove_tab(location);
            }
            let target = target
                .or_else(|| {
                    dock.iter_all_tabs()
                        .find(|((surface, _), p)| surface.is_main() && **p == Panel::Canvas)
                        .map(|(_, p)| *p)
                })
                .or_else(|| {
                    dock.iter_all_tabs()
                        .find(|((surface, _), _)| surface.is_main())
                        .map(|(_, p)| *p)
                });
            if let Some(target) = target {
                let (s, n, _) = dock
                    .find_tab(&target)
                    .ok_or("Open the target window before docking or grouping")?;
                if operation == "tab" {
                    dock.set_focused_node_and_surface((s, n));
                    dock.push_to_focused_leaf(panel);
                } else {
                    let split = match operation {
                        "dock_left" => egui_dock::Split::Left,
                        "dock_right" => egui_dock::Split::Right,
                        "dock_top" => egui_dock::Split::Above,
                        "dock_bottom" => egui_dock::Split::Below,
                        _ => return Err("Unknown window operation".into()),
                    };
                    let amount = args["fraction"]
                        .as_f64()
                        .unwrap_or(if panel == Panel::Tools {
                            if matches!(split, egui_dock::Split::Above | egui_dock::Split::Below) {
                                let height = match &dock[s][n] {
                                    egui_dock::Node::Leaf(leaf) => leaf.rect.height(),
                                    _ => ctx.content_rect().height(),
                                };
                                (self.editor.settings.ui.toolbox_height / height.max(1.))
                                    .clamp(0.1, 0.5) as f64
                            } else {
                                0.12
                            }
                        } else {
                            0.3
                        }) as f32;
                    let fraction =
                        if matches!(split, egui_dock::Split::Right | egui_dock::Split::Below) {
                            1. - amount
                        } else {
                            amount
                        };
                    dock.split((s, n), split, fraction, egui_dock::Node::leaf(panel));
                }
            } else if operation == "tab" {
                return Err("Open the target window before grouping".into());
            } else {
                dock.push_to_first_leaf(panel);
            }
        }
        if operation != "hide" {
            if let Some((s, n, t)) = dock.find_tab(&panel) {
                dock.set_active_tab((s, n, t));
                dock.set_focused_node_and_surface((s, n));
                if let Some(w) = self.native_windows.get(&s.0) {
                    ctx.send_viewport_cmd_to(w.viewport, ViewportCommand::Focus);
                }
            }
        }
        if !valid_dock(&dock) {
            return Err("Window layout limit reached".into());
        }
        self.dock_before_focus = None;
        self.dock = dock;
        self.editor.canvas = Rect::NOTHING;
        self.editor.image_rect = Rect::NOTHING;
        self.layout_changed = Some(Instant::now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    use crate::platform::windows::desktop_bridge::{PenPhase, PenSample};
    fn command(app: &mut Studio, ctx: &Context, args: Value) {
        app.editor
            .session
            .apply(crate::application::Action::new("window.command", args))
            .unwrap();
        app.consume_ui_effects(ctx);
        assert_eq!(app.editor.session.ui["windows.result"]["status"], "applied");
    }
    #[test]
    fn canvas_floats_groups_hides_and_restores_without_losing_pressure_strokes() {
        let mut app = studio();
        let ctx = Context::default();
        let size = vec2(1360., 920.);
        for t in 0..3 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        let parameters = app.editor.session.parameters.clone();
        let composition = app.editor.session.doc.composition();
        command(
            &mut app,
            &ctx,
            json!({"panel":"canvas","operation":"float","x":180,"y":160,"width":700,"height":550}),
        );
        assert!(!app.dock.find_tab(&Panel::Canvas).unwrap().0.is_main());
        for t in 3..7 {
            frame(&mut app, &ctx, size, vec![], t as f64);
        }
        assert_eq!(app.editor.session.doc.composition(), composition);
        assert_eq!(app.editor.session.parameters, parameters);
        let p = app.editor.image_rect.center();
        let ppp = ctx.pixels_per_point();
        app.editor.packets = vec![PenSample {
            phase: PenPhase::Down,
            x: p.x * ppp,
            y: p.y * ppp,
            pressure: 0.3,
            eraser: false,
            time: 7100,
        }];
        frame(&mut app, &ctx, size, vec![Event::PointerMoved(p)], 7.1);
        app.editor.packets = vec![
            PenSample {
                phase: PenPhase::Move,
                x: (p.x + 20.) * ppp,
                y: p.y * ppp,
                pressure: 0.9,
                eraser: false,
                time: 7200,
            },
            PenSample {
                phase: PenPhase::Up,
                x: (p.x + 30.) * ppp,
                y: p.y * ppp,
                pressure: 0.,
                eraser: false,
                time: 7300,
            },
        ];
        frame(&mut app, &ctx, size, vec![], 7.3);
        assert!(app.editor.session.doc.layers[0]
            .pixels
            .iter()
            .any(|p| *p != 0));
        assert!(!app.editor.session.is_drawing());
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        command(
            &mut app,
            &ctx,
            json!({"panel":"canvas","operation":"tab","target":"layers"}),
        );
        let (s, n, _) = app.dock.find_tab(&Panel::Canvas).unwrap();
        let (ls, ln, _) = app.dock.find_tab(&Panel::Layers).unwrap();
        assert_eq!((s, n), (ls, ln));
        command(&mut app, &ctx, json!({"panel":"canvas","operation":"hide"}));
        assert!(app.dock.find_tab(&Panel::Canvas).is_none());
        frame(&mut app, &ctx, size, vec![], 8.);
        assert!(!app.editor.canvas.is_positive());
        command(&mut app, &ctx, json!({"panel":"canvas","operation":"show"}));
        frame(&mut app, &ctx, size, vec![], 9.);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert!(valid_dock(&app.dock));
        let saved = encode_workspace(&app.dock, &app.editor.session.registry).unwrap();
        assert!(decode_workspace(&saved).is_some());
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
    }
    #[test]
    fn invalid_group_target_keeps_the_layout_and_workflow_has_its_own_provider() {
        let mut app = studio();
        let ctx = Context::default();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        assert_eq!(
            app.editor.session.registry.owners["window.workflows.editor.show"],
            "workflows"
        );
        let before = encode_dock(&app.dock).unwrap();
        assert!(app
            .apply_window_command(
                &ctx,
                &json!({"panel":"canvas","operation":"tab","target":"workflows.editor"})
            )
            .is_err());
        assert_eq!(encode_dock(&app.dock).unwrap(), before);
        command(
            &mut app,
            &ctx,
            json!({"panel":"workflows.editor","operation":"show"}),
        );
        assert!(app.dock.find_tab(&Panel::Workflow).is_some());
        assert!(app.dock.find_tab(&Panel::Nodes).is_none());
    }
    #[test]
    fn native_placement_preserves_negative_desktop_coordinates_in_saved_layouts() {
        let mut app = studio();
        let ctx = Context::default();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        ctx.set_embed_viewports(false);
        command(
            &mut app,
            &ctx,
            json!({"panel":"canvas","operation":"float","x":-1500,"y":-800,"width":900,"height":600}),
        );
        let (surface, _, _) = app.dock.find_tab(&Panel::Canvas).unwrap();
        let value = serde_json::to_value(app.dock.get_window_state(surface).unwrap()).unwrap();
        assert_eq!(value["next_position"]["x"], -1500.);
        assert_eq!(value["next_position"]["y"], -800.);
        let saved = encode_workspace(&app.dock, &app.editor.session.registry).unwrap();
        let (mut restored, _) = decode_workspace(&saved).unwrap();
        assert_eq!(
            serde_json::to_value(restored.get_window_state(surface).unwrap()).unwrap(),
            value
        );
    }
    #[test]
    fn all_content_can_float_with_an_empty_main_workspace() {
        let mut app = studio();
        let ctx = Context::default();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        command(
            &mut app,
            &ctx,
            json!({"panel":"canvas","operation":"float"}),
        );
        command(
            &mut app,
            &ctx,
            json!({"panel":"toolbox","operation":"float"}),
        );
        for panel in ["brush", "color", "layers", "navigator"] {
            command(&mut app, &ctx, json!({"panel":panel,"operation":"hide"}));
        }
        assert!(app.dock.main_surface().is_empty());
        assert!(valid_dock(&app.dock));
        assert!(decode_workspace(
            &encode_workspace(&app.dock, &app.editor.session.registry).unwrap()
        )
        .is_some());
    }
}
