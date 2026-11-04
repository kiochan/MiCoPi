//! Cross-viewport group docking from native move-loop observations.
use super::*;
use egui_dock::{Node, SurfaceIndex};
use serde_json::json;

#[derive(Clone)]
pub(super) struct GroupDrop {
    pub viewport: ViewportId,
    pub target: Option<Panel>,
    pub operation: &'static str,
    pub preview: Rect,
    pub zones: Vec<(Rect, &'static str)>,
}
fn zones(rect: Rect, a: &Appearance) -> Vec<(Rect, &'static str)> {
    let size = (a.control_height + 4.)
        .clamp(24., 40.)
        .min(rect.width() / 5.)
        .min(rect.height() / 5.);
    let center = rect.center();
    [
        (-1., 0., "dock_left"),
        (1., 0., "dock_right"),
        (0., -1., "dock_top"),
        (0., 1., "dock_bottom"),
        (0., 0., "tab"),
    ]
    .into_iter()
    .map(|(x, y, op)| {
        (
            Rect::from_center_size(center + vec2(x, y) * (size + 4.), Vec2::splat(size)),
            op,
        )
    })
    .collect()
}
fn selected(rect: Rect, point: Pos2, a: &Appearance) -> Option<&'static str> {
    if !rect.contains(point) {
        return None;
    }
    if let Some((_, operation)) = zones(rect, a).into_iter().find(|(r, _)| r.contains(point)) {
        return Some(operation);
    }
    let band = (a.control_height * 1.5)
        .min(rect.width().min(rect.height()) * 0.2)
        .max(12.);
    if point.y < rect.top() + a.tab_height
        && point.x > rect.left() + band
        && point.x < rect.right() - band
    {
        return Some("tab");
    }
    let edges = [
        (point.x - rect.left(), "dock_left"),
        (rect.right() - point.x, "dock_right"),
        (point.y - rect.top(), "dock_top"),
        (rect.bottom() - point.y, "dock_bottom"),
    ];
    edges
        .into_iter()
        .filter(|(distance, _)| *distance < band)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, op)| op)
}
fn preview(rect: Rect, operation: &str) -> Rect {
    match operation {
        "dock_left" => Rect::from_min_max(rect.min, pos2(rect.center().x, rect.bottom())),
        "dock_right" => Rect::from_min_max(pos2(rect.center().x, rect.top()), rect.max),
        "dock_top" => Rect::from_min_max(rect.min, pos2(rect.right(), rect.center().y)),
        "dock_bottom" => Rect::from_min_max(pos2(rect.left(), rect.center().y), rect.max),
        _ => rect,
    }
}
fn empty_workspace_layout(
    dock: &DockState<Panel>,
    source: SurfaceIndex,
    registry: &crate::plugins::Registry,
) -> Result<serde_json::Value, String> {
    let mut dock = dock.clone();
    if !dock.main_surface().is_empty() {
        return Err("Workspace is not empty".into());
    }
    let Some(egui_dock::Surface::Window(tree, _)) = dock.remove_surface(source) else {
        return Err("Missing floating group".into());
    };
    *dock.main_surface_mut() = tree;
    let text = encode_workspace(&dock, registry)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}
impl Studio {
    pub(super) fn group_drop_target(
        &self,
        root: &Context,
        source: usize,
        point: (i32, i32),
    ) -> Option<GroupDrop> {
        self.group_drop_target_hint(root, source, point, None)
    }
    pub(super) fn group_drop_target_hint(
        &self,
        root: &Context,
        source: usize,
        point: (i32, i32),
        hint: Option<Panel>,
    ) -> Option<GroupDrop> {
        let source_hwnd = self
            .native_windows
            .get(&source)?
            .native
            .as_ref()?
            .window_id();
        let below = if let Some(panel) = hint {
            let (surface, _, _) = self.dock.find_tab(&panel)?;
            if surface.0 == source {
                return None;
            }
            if surface.is_main() {
                self.editor.native.as_ref()?.window_id()
            } else {
                self.native_windows
                    .get(&surface.0)?
                    .native
                    .as_ref()?
                    .window_id()
            }
        } else {
            Native::window_under(point, source_hwnd)?
        };
        let root_native = self.editor.native.as_ref()?;
        let (surface, viewport, native, ppp) = if below == root_native.window_id() {
            (
                SurfaceIndex::main(),
                ViewportId::ROOT,
                root_native,
                root.pixels_per_point(),
            )
        } else {
            let (index, window) = self.native_windows.iter().find(|(index, w)| {
                **index != source && w.native.as_ref().is_some_and(|n| n.window_id() == below)
            })?;
            (
                SurfaceIndex(*index),
                window.viewport,
                window.native.as_ref()?,
                window.pixels_per_point,
            )
        };
        let pointer = native.client_point(point, ppp)?;
        let tree = self.dock.get_surface(surface)?.node_tree()?;
        let a = &self.editor.settings.ui;
        if tree.is_empty() && surface.is_main() {
            let area = root
                .data(|d| d.get_temp::<Rect>(Id::new("native-group-main-bounds")))
                .unwrap_or(root.content_rect());
            if !area.contains(pointer) {
                return None;
            }
            return Some(GroupDrop {
                viewport,
                target: None,
                operation: "dock_right",
                preview: area,
                zones: vec![],
            });
        }
        let leaf = tree.iter().find_map(|node| match node {
            Node::Leaf(leaf) if leaf.rect.contains(pointer) => Some(leaf),
            _ => None,
        })?;
        let operation = selected(leaf.rect, pointer, a);
        Some(GroupDrop {
            viewport,
            target: leaf.tabs.get(leaf.active.0).copied(),
            operation: operation.unwrap_or(""),
            preview: operation
                .map(|op| preview(leaf.rect, op))
                .unwrap_or(Rect::NOTHING),
            zones: zones(leaf.rect, a),
        })
    }
    pub(super) fn group_drag_preview(&self, root: &Context) -> Option<(usize, GroupDrop)> {
        self.native_windows.iter().find_map(|(source, w)| {
            w.native
                .as_ref()?
                .group_drag_cursor()
                .and_then(|point| self.group_drop_target(root, *source, point))
                .map(|target| (*source, target))
        })
    }
    pub(super) fn paint_group_drop(ctx: &Context, target: &GroupDrop, a: &Appearance) {
        if ctx.viewport_id() != target.viewport {
            return;
        }
        let colors = theme::Colors::new(a);
        let painter = ctx.layer_painter(LayerId::new(
            Order::Tooltip,
            Id::new("group-docking-preview"),
        ));
        if target.preview.is_positive() {
            painter.rect_filled(target.preview, 4., colors.accent.gamma_multiply(0.2));
            painter.rect_stroke(
                target.preview,
                4.,
                Stroke::new(2., colors.accent),
                StrokeKind::Inside,
            );
        }
        for (rect, operation) in &target.zones {
            let active = *operation == target.operation;
            painter.rect_filled(
                *rect,
                4.,
                if active { colors.accent } else { colors.raised },
            );
            painter.rect_stroke(
                *rect,
                4.,
                Stroke::new(1., colors.border),
                StrokeKind::Inside,
            );
            let c = rect.center();
            let d = rect.size() * 0.22;
            let symbol = match *operation {
                "dock_left" => Rect::from_min_max(c - d, pos2(c.x, c.y + d.y)),
                "dock_right" => Rect::from_min_max(pos2(c.x, c.y - d.y), c + d),
                "dock_top" => Rect::from_min_max(c - d, pos2(c.x + d.x, c.y)),
                "dock_bottom" => Rect::from_min_max(pos2(c.x - d.x, c.y), c + d),
                _ => Rect::from_center_size(c, rect.size() * 0.45),
            };
            painter.rect_filled(
                symbol,
                1.,
                if active {
                    colors.background
                } else {
                    colors.text
                },
            );
        }
    }
    pub(super) fn request_group_drop(&mut self, root: &Context, source: usize, point: (i32, i32)) {
        self.request_group_drop_hint(root, source, point, None);
    }
    pub(super) fn request_group_drop_hint(
        &mut self,
        root: &Context,
        source: usize,
        point: (i32, i32),
        hint: Option<Panel>,
    ) {
        let Some(target) = self
            .group_drop_target_hint(root, source, point, hint)
            .filter(|t| !t.operation.is_empty())
        else {
            let host_point = self
                .editor
                .native
                .as_ref()
                .and_then(|n| n.client_point(point, root.pixels_per_point()));
            let source_hwnd = self
                .native_windows
                .get(&source)
                .and_then(|w| w.native.as_ref())
                .map(Native::window_id)
                .unwrap_or(0);
            let hit = Native::window_under(point, source_hwnd);
            let hit_host = self
                .editor
                .native
                .as_ref()
                .is_some_and(|n| Some(n.window_id()) == hit);
            self.editor.session.ui["windows.drop"] = json!({"status":"no_target","point":point,"host_point":host_point,"host_scale":root.pixels_per_point(),"hit_host":hit_host,"hit_present":hit.is_some()});
            return;
        };
        if target.target.is_none() {
            match empty_workspace_layout(
                &self.dock,
                SurfaceIndex(source),
                &self.editor.session.registry,
            ) {
                Ok(layout) => {
                    self.editor.session.ui["windows.drop"] = json!({"status":"resolved","operation":"workspace.layout.apply","destination":"main"});
                    self.editor
                        .core("workspace.layout.apply", json!({"layout":layout}));
                    root.request_repaint();
                }
                Err(error) => {
                    self.editor.session.ui["windows.drop"] =
                        json!({"status":"failed","error":error})
                }
            }
            return;
        }
        let panel = self
            .dock
            .get_surface(SurfaceIndex(source))
            .and_then(|s| s.iter_all_tabs().next().map(|(_, p)| *p));
        let Some(id) = panel.and_then(|p| self.editor.panel_id(p)) else {
            return;
        };
        let mut args =
            json!({"panel":id,"operation":target.operation,"scope":"group","fraction":0.5});
        if let Some(id) = target.target.and_then(|p| self.editor.panel_id(p)) {
            args["target"] = json!(id);
        }
        self.editor.session.ui["windows.drop"] = json!({"status":"resolved","command":args});
        self.editor.core("window.command", args);
        root.request_repaint();
    }
    pub(super) fn finish_group_drops(&mut self, root: &Context) {
        let drops: Vec<_> = self
            .native_windows
            .iter()
            .filter_map(|(index, w)| {
                w.native
                    .as_ref()?
                    .take_group_drop()
                    .map(|point| (*index, point))
            })
            .collect();
        for (source, point) in drops {
            self.request_group_drop(root, source, point);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropping_into_an_empty_workspace_keeps_the_canvas_in_its_own_window() {
        let mut app = crate::desktop::tests::studio();
        let ctx = Context::default();
        crate::desktop::tests::frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        for args in [
            json!({"panel":"canvas","operation":"float"}),
            json!({"panel":"layers","operation":"float","scope":"group"}),
            json!({"panel":"toolbox","operation":"hide"}),
            json!({"panel":"navigator","operation":"hide"}),
        ] {
            app.apply_window_command(&ctx, &args).unwrap();
        }
        assert!(app.dock.main_surface().is_empty());
        let canvas = app.dock.find_tab(&Panel::Canvas).unwrap().0;
        let group = app.dock.find_tab(&Panel::Layers).unwrap().0;
        let layout =
            empty_workspace_layout(&app.dock, group, &app.editor.session.registry).unwrap();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "workspace.layout.apply",
                json!({"layout":layout}),
            ))
            .unwrap();
        app.consume_ui_effects(&ctx);
        assert!(app.dock.find_tab(&Panel::Layers).unwrap().0.is_main());
        assert_eq!(app.dock.find_tab(&Panel::Canvas).unwrap().0, canvas);
        assert!(valid_dock(&app.dock));
    }
    #[test]
    fn group_drop_zones_are_explicit_and_map_to_matching_previews() {
        let a = Appearance::default();
        let rect = Rect::from_min_size(pos2(80., 100.), vec2(640., 480.));
        for (zone, operation) in zones(rect, &a) {
            assert_eq!(selected(rect, zone.center(), &a), Some(operation));
            assert!(rect.contains_rect(preview(rect, operation)));
        }
        assert_eq!(selected(rect, pos2(82., 350.), &a), Some("dock_left"));
        assert_eq!(selected(rect, pos2(400., 110.), &a), Some("tab"));
        assert_eq!(selected(rect, pos2(200., 240.), &a), None);
        assert_eq!(selected(rect, pos2(20., 20.), &a), None);
    }
}
