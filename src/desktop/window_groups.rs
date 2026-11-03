//! Group operations preserve tab order, active tabs and split topology.
use super::*;
use egui_dock::{Node, Surface, Tree};
use serde_json::Value;

fn graft(dst: &mut Tree<Panel>, at: NodeIndex, src: &Tree<Panel>, from: NodeIndex, seed: Panel) {
    match &src[from] {
        Node::Leaf(leaf) => dst[at] = Node::Leaf(leaf.clone()),
        Node::Horizontal(split) | Node::Vertical(split) => {
            let direction = if matches!(&src[from], Node::Horizontal(_)) {
                egui_dock::Split::Right
            } else {
                egui_dock::Split::Below
            };
            let [left, right] = dst.split(at, direction, split.fraction, Node::leaf(seed));
            graft(dst, left, src, from.left(), seed);
            graft(dst, right, src, from.right(), seed);
        }
        Node::Empty => unreachable!("validated group tree"),
    }
}
impl Studio {
    pub(super) fn apply_window_group(
        &mut self,
        ctx: &Context,
        args: &Value,
        panel: Panel,
    ) -> Result<(), String> {
        let operation = args["operation"]
            .as_str()
            .ok_or("Missing group operation")?;
        let mut dock = self
            .dock_before_focus
            .as_ref()
            .unwrap_or(&self.dock)
            .clone();
        let (source, node, _) = dock
            .find_tab(&panel)
            .ok_or("Open the group before moving it")?;
        let members: Vec<_> = if source.is_main() {
            dock[source][node].iter_tabs().copied().collect()
        } else {
            dock[source]
                .iter()
                .flat_map(|n| n.iter_tabs())
                .copied()
                .collect()
        };
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
        if target.is_some_and(|p| members.contains(&p)) {
            return Err("Choose a target outside this group".into());
        }
        if !matches!(
            operation,
            "float" | "hide" | "dock_left" | "dock_right" | "dock_top" | "dock_bottom" | "tab"
        ) {
            return Err("Unsupported group operation".into());
        }
        let mut state = None;
        let group = if source.is_main() {
            let leaf = dock[source][node].clone();
            dock.remove_leaf((source, node));
            let mut tree = Tree::new(vec![panel]);
            tree[NodeIndex::root()] = leaf;
            tree
        } else {
            match if operation == "float" {
                std::mem::replace(dock.get_surface_mut(source).unwrap(), Surface::Empty)
            } else {
                dock.remove_surface(source).unwrap()
            } {
                Surface::Window(tree, window) => {
                    state = Some(window);
                    tree
                }
                _ => unreachable!(),
            }
        };
        if operation == "float" {
            let old = state.as_ref().map(super::native_windows::geometry);
            let rect = floating_rect(ctx, args, panel, &self.editor.settings.ui, old);
            let surface = if source.is_main() {
                dock.add_window(vec![])
            } else {
                source
            };
            let mut window = state.unwrap_or_default();
            window.set_position(rect.min).set_size(rect.size());
            *dock.get_surface_mut(surface).unwrap() = Surface::Window(group, window);
        } else if operation != "hide" {
            let target = target
                .or_else(|| {
                    dock.iter_all_tabs()
                        .find(|((s, _), p)| s.is_main() && **p == Panel::Canvas)
                        .map(|(_, p)| *p)
                })
                .or_else(|| {
                    dock.iter_all_tabs()
                        .find(|((s, _), _)| s.is_main())
                        .map(|(_, p)| *p)
                });
            if let Some(target) = target {
                let (s, n, _) = dock
                    .find_tab(&target)
                    .ok_or("Open the target window before moving a group")?;
                if operation == "tab" {
                    for p in group.iter().flat_map(|n| n.iter_tabs()) {
                        dock[s][n].append_tab(*p);
                    }
                } else {
                    let direction = match operation {
                        "dock_left" => egui_dock::Split::Left,
                        "dock_right" => egui_dock::Split::Right,
                        "dock_top" => egui_dock::Split::Above,
                        _ => egui_dock::Split::Below,
                    };
                    let amount = args["fraction"].as_f64().unwrap_or(0.3) as f32;
                    let fraction =
                        if matches!(direction, egui_dock::Split::Right | egui_dock::Split::Below) {
                            1. - amount
                        } else {
                            amount
                        };
                    let [_, new] = dock.split((s, n), direction, fraction, Node::leaf(panel));
                    graft(&mut dock[s], new, &group, NodeIndex::root(), panel);
                }
            } else if operation == "tab" {
                return Err("Open the target window before grouping".into());
            } else {
                *dock.main_surface_mut() = group;
            }
        }
        if operation != "hide" {
            if let Some((s, n, _)) = dock.find_tab(&panel) {
                dock.set_focused_node_and_surface((s, n));
            }
        }
        if !valid_dock(&dock) {
            return Err("Window layout limit reached".into());
        }
        self.reset_window_input();
        self.dock = dock;
        self.dock_before_focus = None;
        self.editor.canvas = Rect::NOTHING;
        self.editor.image_rect = Rect::NOTHING;
        self.layout_changed = Some(Instant::now());
        Ok(())
    }
}

pub(super) fn floating_rect(
    ctx: &Context,
    args: &Value,
    panel: Panel,
    a: &Appearance,
    old: Option<Rect>,
) -> Rect {
    let root = ctx.content_rect();
    let area = Rect::from_min_max(
        root.min + vec2(a.outer_padding, a.title_height + a.outer_padding * 2.),
        root.max - vec2(a.outer_padding, 24. + a.outer_padding * 2.),
    );
    let decoration = ctx.style().spacing.window_margin.sum()
        + Vec2::splat(ctx.style().visuals.window_stroke.width * 2.);
    let size = vec2(
        args["width"]
            .as_f64()
            .map(|v| v as f32)
            .or(old.map(|r| r.width()))
            .unwrap_or(if panel == Panel::Canvas {
                640.
            } else if panel == Panel::Tools {
                a.toolbox_width
            } else {
                320.
            }),
        args["height"]
            .as_f64()
            .map(|v| v as f32)
            .or(old.map(|r| r.height()))
            .unwrap_or(if panel == Panel::Tools {
                a.toolbox_height
                    + if ctx.embed_viewports() {
                        0.
                    } else {
                        a.title_height
                    }
            } else {
                460.
            }),
    );
    let origin = ctx
        .input(|i| i.viewport().outer_rect.map(|r| r.min))
        .unwrap_or_default();
    let position = pos2(
        args["x"]
            .as_f64()
            .map(|v| v as f32)
            .or(old.map(|r| r.min.x))
            .unwrap_or(origin.x + area.left() + 24.),
        args["y"]
            .as_f64()
            .map(|v| v as f32)
            .or(old.map(|r| r.min.y))
            .unwrap_or(origin.y + area.top() + 24.),
    );
    if ctx.embed_viewports() {
        let size = size.min((area.size() - decoration).max(vec2(48., 48.)));
        Rect::from_min_size(position.clamp(area.min, area.max - size - decoration), size)
    } else {
        Rect::from_min_size(position, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    use serde_json::json;
    fn command(
        app: &mut Studio,
        ctx: &Context,
        panel: &str,
        operation: &str,
        scope: &str,
        target: Option<&str>,
    ) {
        let mut args = json!({"panel":panel,"operation":operation,"scope":scope});
        if let Some(target) = target {
            args["target"] = json!(target);
        }
        app.editor
            .session
            .apply(crate::application::Action::new("window.command", args))
            .unwrap();
        app.consume_ui_effects(ctx);
        assert_eq!(app.editor.session.ui["windows.result"]["status"], "applied");
    }
    fn shape(tree: &Tree<Panel>, at: NodeIndex) -> Value {
        match &tree[at] {
            Node::Leaf(leaf) => json!({"tabs":leaf.tabs,"active":leaf.active.0}),
            Node::Horizontal(split) | Node::Vertical(split) => {
                json!({"horizontal":matches!(&tree[at],Node::Horizontal(_)),"fraction":split.fraction,"left":shape(tree,at.left()),"right":shape(tree,at.right())})
            }
            Node::Empty => Value::Null,
        }
    }
    #[test]
    fn moving_groups_preserves_active_tabs_nested_splits_pixels_and_replay() {
        let mut app = studio();
        let ctx = Context::default();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        let pixels = app.editor.session.doc.layers[0].pixels.clone();
        command(&mut app, &ctx, "brush", "focus", "panel", None);
        let (s, n, _) = app.dock.find_tab(&Panel::Layers).unwrap();
        let expected = shape(&app.dock[s], n);
        command(&mut app, &ctx, "layers", "float", "group", None);
        let (s, n, _) = app.dock.find_tab(&Panel::Layers).unwrap();
        assert!(!s.is_main());
        assert_eq!(shape(&app.dock[s], n), expected);
        for p in [Panel::Brush, Panel::Color] {
            let location = app.dock.find_tab(&p).unwrap();
            assert_eq!((location.0, location.1), (s, n));
        }
        command(
            &mut app,
            &ctx,
            "color",
            "dock_bottom",
            "panel",
            Some("brush"),
        );
        let (s, _, _) = app.dock.find_tab(&Panel::Layers).unwrap();
        let expected = shape(&app.dock[s], NodeIndex::root());
        let (_, destination, _) = app.dock.find_tab(&Panel::Canvas).unwrap();
        command(
            &mut app,
            &ctx,
            "layers",
            "dock_left",
            "group",
            Some("canvas"),
        );
        assert_eq!(shape(app.dock.main_surface(), destination.left()), expected);
        assert!(valid_dock(&app.dock));
        let saved = encode_workspace(&app.dock, &app.editor.session.registry).unwrap();
        let (restored, _) = decode_workspace(&saved).unwrap();
        assert_eq!(shape(restored.main_surface(), destination.left()), expected);
        assert_eq!(app.editor.session.doc.layers[0].pixels, pixels);
        assert_eq!(
            app.editor
                .session
                .apply(crate::application::Action::new("replay.verify", json!({})))
                .unwrap()["result"]["valid"],
            true
        );
    }
    #[test]
    fn group_rejects_its_own_target_without_mutating_layout_and_hides_together() {
        let mut app = studio();
        let ctx = Context::default();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        let before = encode_dock(&app.dock).unwrap();
        assert!(app
            .apply_window_command(
                &ctx,
                &json!({"panel":"layers","scope":"group","operation":"dock_right","target":"brush"})
            )
            .is_err());
        assert_eq!(encode_dock(&app.dock).unwrap(), before);
        command(&mut app, &ctx, "layers", "float", "group", None);
        let surface = app.dock.find_tab(&Panel::Layers).unwrap().0;
        command(&mut app, &ctx, "layers", "float", "group", None);
        assert_eq!(app.dock.find_tab(&Panel::Layers).unwrap().0, surface);
        command(&mut app, &ctx, "layers", "hide", "group", None);
        for p in [Panel::Layers, Panel::Brush, Panel::Color] {
            assert!(app.dock.find_tab(&p).is_none());
        }
        assert!(app.dock.find_tab(&Panel::Canvas).is_some());
    }
}
