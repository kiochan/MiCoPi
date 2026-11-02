use super::*;
use crate::engine::graph::{Edge, Filter, Graph, Node, NodeKind, MAX_NODES};
use egui_snarl::{
    ui::{NodeLayout, PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, NodeId, OutPin, OutPinId, Snarl,
};
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct GraphEditor {
    #[cfg(test)]
    apply_rect: Option<Rect>,
    snarl: Snarl<Node>,
    base: Graph,
    identity: u64,
    enabled: bool,
    message: String,
    preview: bool,
    previous: Option<Graph>,
    edited: Option<Instant>,
    fit_view: bool,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    #[test]
    fn dockable_nodes_apply_button_updates_the_document_and_connections_reject_cycles() {
        let mut app = studio();
        let ctx = Context::default();
        app.focus_panel(Panel::Nodes);
        let location = app.dock.find_tab(&Panel::Nodes).unwrap();
        assert_ne!(location.1, app.dock.find_tab(&Panel::Canvas).unwrap().1);
        for i in 0..3 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], i as f64);
        }
        let r = app.editor.nodes.apply_rect.unwrap();
        assert!(r.width() > 50.);
        let before = app.editor.session.doc.layers[0].pixels.clone();
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![
                Event::PointerMoved(r.center()),
                Event::PointerButton {
                    pos: r.center(),
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
            ],
            4.,
        );
        frame(
            &mut app,
            &ctx,
            vec2(1360., 920.),
            vec![Event::PointerButton {
                pos: r.center(),
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            }],
            4.1,
        );
        assert!(
            app.editor.session.doc.graph.enabled,
            "button={r:?}, message={}, enabled={}, conflict={}, status={}",
            app.editor.nodes.message,
            app.editor.nodes.enabled,
            app.editor.nodes.base != app.editor.session.doc.graph,
            app.editor.status
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, before);
        app.editor.session.doc.undo();
        assert!(!app.editor.session.doc.graph.enabled);
        let mut snarl = Snarl::new();
        let a = snarl.insert_node(
            Pos2::ZERO,
            Node {
                node_type: None,
                id: 1,
                position: [0.; 2],
                data: NodeKind::Filter {
                    filter: Filter::Invert,
                    amount: 1.,
                },
            },
        );
        let b = snarl.insert_node(
            Pos2::ZERO,
            Node {
                node_type: None,
                id: 2,
                position: [0.; 2],
                data: NodeKind::Filter {
                    filter: Filter::Grayscale,
                    amount: 1.,
                },
            },
        );
        snarl.connect(
            OutPinId { node: a, output: 0 },
            InPinId { node: b, input: 0 },
        );
        let mut viewer = NodeViewer {
            layers: 1,
            message: String::new(),
            ..Default::default()
        };
        viewer.connect(
            &snarl.out_pin(OutPinId { node: b, output: 0 }),
            &snarl.in_pin(InPinId { node: a, input: 0 }),
            &mut snarl,
        );
        assert_eq!(snarl.wires().count(), 1);
        assert!(!viewer.message.is_empty());
    }
}
impl GraphEditor {
    pub(super) fn fit(&mut self) {
        self.fit_view = true;
    }
    fn load(
        &mut self,
        doc: &crate::engine::Document,
        default: bool,
        definitions: &BTreeMap<String, crate::plugins::nodes::Resolved>,
    ) {
        self.snarl = Snarl::new();
        self.base = doc.graph.clone();
        self.identity = doc.identity;
        let graph = if default || doc.graph.nodes.is_empty() {
            Graph::from_layers(&doc.layers)
        } else {
            doc.graph.clone()
        };
        self.enabled = graph.enabled;
        let mut ids = BTreeMap::new();
        for mut node in graph.nodes {
            if node.node_type.is_none() {
                let mut data = serde_json::to_value(&node.data).unwrap();
                let operation = data.as_object_mut().unwrap().remove("kind").unwrap();
                let canonical = format!("compositing.{}", operation.as_str().unwrap());
                node.node_type = definitions
                    .get(&canonical)
                    .filter(|d| crate::plugins::validate(&d.parameters_schema, &data).is_ok())
                    .map(|d| d.id.clone());
            }
            ids.insert(
                node.id,
                self.snarl
                    .insert_node(pos2(node.position[0], node.position[1]), node),
            );
        }
        for edge in graph.edges {
            self.snarl.connect(
                OutPinId {
                    node: ids[&edge.from],
                    output: 0,
                },
                InPinId {
                    node: ids[&edge.to],
                    input: edge.input as usize,
                },
            );
        }
        self.previous = Some(self.graph());
        self.edited = None;
        self.message.clear();
        self.fit_view = true;
    }
    fn graph(&self) -> Graph {
        Graph {
            enabled: self.enabled,
            nodes: self
                .snarl
                .nodes_pos_ids()
                .map(|(_, p, n)| Node {
                    node_type: n.node_type.clone(),
                    id: n.id,
                    position: [p.x, p.y],
                    data: n.data.clone(),
                })
                .collect(),
            edges: self
                .snarl
                .wires()
                .map(|(from, to)| Edge {
                    from: self.snarl[from.node].id,
                    to: self.snarl[to.node].id,
                    input: to.input as u32,
                })
                .collect(),
        }
    }
    fn apply(&mut self, editor: &mut Editor) {
        let graph = self.graph();
        match editor.session.apply(crate::application::Action::new(
            "graph.set",
            serde_json::json!({"graph":graph}),
        )) {
            Ok(_) => {
                self.base = editor.session.doc.graph.clone();
                self.edited = None;
                self.message = "Applied · Original Strokes Preserved · Undo Available".into();
                editor.sync_from_session();
            }
            Err(e) => {
                self.message = e.to_string();
                self.edited = None;
            }
        }
    }
}
struct NodeViewer<'a> {
    definitions: Option<&'a BTreeMap<String, crate::plugins::nodes::Resolved>>,
    layers: usize,
    message: String,
    width: f32,
    viewport: Rect,
    fit_view: bool,
}
impl Default for NodeViewer<'_> {
    fn default() -> Self {
        Self {
            definitions: None,
            layers: 0,
            message: String::new(),
            width: 192.,
            viewport: Rect::NOTHING,
            fit_view: false,
        }
    }
}
fn add(snarl: &mut Snarl<Node>, pos: Pos2, data: NodeKind) -> Option<NodeId> {
    if snarl.nodes().count() >= MAX_NODES {
        return None;
    }
    let id = (0..=u32::MAX)
        .find(|id| !snarl.nodes().any(|n| n.id == *id))
        .unwrap();
    Some(snarl.insert_node(
        pos,
        Node {
            node_type: None,
            id,
            position: [pos.x, pos.y],
            data,
        },
    ))
}
fn add_menu(
    ui: &mut Ui,
    snarl: &mut Snarl<Node>,
    pos: Pos2,
    definitions: Option<&BTreeMap<String, crate::plugins::nodes::Resolved>>,
) {
    let Some(definitions) = definitions else {
        ui.weak(tr("Node Definition Not Loaded"));
        return;
    };
    for def in definitions.values().filter(|d| d.domain == "image") {
        if ui
            .button(tr(&def.title))
            .on_hover_text(tr(&def.id))
            .clicked()
        {
            if snarl.nodes().count() < MAX_NODES {
                if let crate::plugins::nodes::Binding::Image {
                    operation,
                    defaults,
                } = &def.binding
                {
                    let mut data = defaults.clone();
                    data["kind"] = serde_json::json!(operation);
                    if let Ok(data) = serde_json::from_value(data) {
                        if let Some(id) = add(snarl, pos, data) {
                            snarl[id].node_type = Some(def.id.clone());
                        }
                    }
                }
            }
            ui.close();
        }
    }
}
#[allow(refining_impl_trait)]
impl SnarlViewer<Node> for NodeViewer<'_> {
    fn current_transform(&mut self, t: &mut egui::emath::TSTransform, snarl: &mut Snarl<Node>) {
        if !self.fit_view {
            return;
        }
        let mut bounds = Rect::NOTHING;
        for (_, pos, _) in snarl.nodes_pos_ids() {
            bounds = bounds.union(Rect::from_min_size(pos, vec2(self.width + 24., 185.)));
        }
        if bounds.is_finite() {
            let view = self.viewport.shrink(16.);
            let scale = (view.width() / bounds.width())
                .min(view.height() / bounds.height())
                .clamp(0.2, 1.);
            *t = egui::emath::TSTransform {
                scaling: scale,
                translation: view.center().to_vec2() - bounds.center().to_vec2() * scale,
            };
        }
    }
    fn title(&mut self, n: &Node) -> String {
        n.node_type
            .as_ref()
            .and_then(|id| self.definitions.and_then(|d| d.get(id)))
            .map(|d| localize(d.title.clone()))
            .unwrap_or_else(|| n.data.title())
    }
    fn inputs(&mut self, n: &Node) -> usize {
        n.data.inputs()
    }
    fn outputs(&mut self, n: &Node) -> usize {
        usize::from(!matches!(n.data, NodeKind::Output))
    }
    fn show_input(&mut self, p: &InPin, ui: &mut Ui, snarl: &mut Snarl<Node>) -> PinInfo {
        ui.label(tr(
            if matches!(snarl[p.id.node].data, NodeKind::Blend { .. }) {
                if p.id.input == 0 {
                    "Background"
                } else {
                    "Foreground"
                }
            } else {
                "Image"
            },
        ));
        PinInfo::circle().with_fill(ui.visuals().selection.stroke.color)
    }
    fn show_output(&mut self, _: &OutPin, ui: &mut Ui, _: &mut Snarl<Node>) -> PinInfo {
        ui.label(tr("Image"));
        PinInfo::circle().with_fill(ui.visuals().selection.stroke.color)
    }
    fn has_body(&mut self, _: &Node) -> bool {
        true
    }
    fn show_body(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Node>,
    ) {
        ui.vertical(|ui| {
            ui.set_width(self.width);
            let Some(def) = snarl[id]
                .node_type
                .as_ref()
                .and_then(|id| self.definitions.and_then(|defs| defs.get(id)))
            else {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr("Node Definition Unavailable"),
                );
                return;
            };
            let mut data = serde_json::to_value(&snarl[id].data).unwrap();
            let operation = data.as_object_mut().unwrap().remove("kind").unwrap();
            if let Some(properties) = def.parameters_schema["properties"].as_object() {
                for (key, schema) in properties {
                    ui.label(tr(schema["title"].as_str().unwrap_or(key)));
                    if key == "index" && operation == "layer" {
                        let mut index = data[key].as_u64().unwrap_or(0) as usize;
                        ComboBox::from_id_salt(("layer", id))
                            .selected_text(tr(format!("Layer {}", index + 1)))
                            .show_ui(ui, |ui| {
                                for i in 0..self.layers {
                                    ui.selectable_value(
                                        &mut index,
                                        i,
                                        tr(format!("Layer {}", i + 1)),
                                    );
                                }
                            });
                        data[key] = serde_json::json!(index);
                    } else {
                        let mut schema = schema.clone();
                        if key == "amount" {
                            if let NodeKind::Filter { filter, .. } = snarl[id].data {
                                schema["minimum"] = serde_json::json!(filter
                                    .minimum()
                                    .max(schema["minimum"].as_f64().unwrap_or(-1.) as f32));
                            }
                        }
                        let value = data
                            .as_object_mut()
                            .unwrap()
                            .entry(key.clone())
                            .or_insert_with(|| super::schema::default_value(&schema));
                        super::schema::edit(ui, (id, key), &schema, value);
                    }
                }
            }
            data["kind"] = operation;
            if let Ok(mut next) = serde_json::from_value::<NodeKind>(data) {
                if let NodeKind::Filter { filter, amount } = &mut next {
                    if let NodeKind::Filter { filter: before, .. } = snarl[id].data {
                        if *filter != before {
                            *amount = if matches!(filter, Filter::Invert | Filter::Grayscale) {
                                1.
                            } else {
                                0.
                            };
                        }
                    }
                }
                snarl[id].data = next;
            }
            if matches!(snarl[id].data, NodeKind::Output) {
                ui.add(
                    Label::new(tr(RichText::new(localize(
                        "Canvas and Export Use This Output",
                    ))
                    .weak()))
                    .wrap(),
                );
            }
        });
    }
    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<Node>) {
        let mut stack = vec![to.id.node];
        let mut seen = vec![];
        while let Some(node) = stack.pop() {
            if node == from.id.node {
                self.message = "Cannot Connect: This Would Create a Cycle".into();
                return;
            }
            if seen.contains(&node) {
                continue;
            }
            seen.push(node);
            stack.extend(
                snarl
                    .wires()
                    .filter(|(a, _)| a.node == node)
                    .map(|(_, b)| b.node),
            );
        }
        for remote in &to.remotes {
            snarl.disconnect(*remote, to.id);
        }
        snarl.connect(from.id, to.id);
    }
    fn has_graph_menu(&mut self, _: Pos2, _: &mut Snarl<Node>) -> bool {
        true
    }
    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut Ui, snarl: &mut Snarl<Node>) {
        add_menu(ui, snarl, pos, self.definitions);
    }
    fn has_node_menu(&mut self, _: &Node) -> bool {
        true
    }
    fn show_node_menu(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<Node>,
    ) {
        if ui.button(tr("Delete Node")).clicked() {
            snarl.remove_node(id);
            ui.close();
        }
    }
}
impl Editor {
    pub(super) fn nodes_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.nodes_workflow, false, tr("Image Compositing"));
            ui.selectable_value(&mut self.nodes_workflow, true, tr("Plugin Workflow"));
        });
        ui.separator();
        if self.nodes_workflow {
            self.workflow_ui(ui);
        } else {
            self.image_nodes_ui(ui);
        }
    }
    fn image_nodes_ui(&mut self, ui: &mut Ui) {
        let mut state = std::mem::take(&mut self.nodes);
        if state.identity != self.session.doc.identity {
            state.load(&self.session.doc, false, &self.plugin_nodes);
        }
        let conflict = state.base != self.session.doc.graph;
        let mut apply = false;
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(tr("Add Node"), |ui| {
                add_menu(
                    ui,
                    &mut state.snarl,
                    pos2(80., 80.),
                    Some(&self.plugin_nodes),
                );
            });
            if ui.button(tr("Rebuild from Layers")).clicked() {
                state.load(&self.session.doc, true, &self.plugin_nodes);
            }
            if ui.button(tr("Load Current")).clicked() {
                state.load(&self.session.doc, false, &self.plugin_nodes);
            }
            if ui.button(tr("Fit Graph")).clicked() {
                state.fit_view = true;
            }
            ui.separator();
            ui.checkbox(&mut state.enabled, tr("Node Compositing"));
            let response = ui.add_enabled(!conflict, Button::new(tr("Apply to Canvas")));
            #[cfg(test)]
            {
                state.apply_rect = Some(response.rect);
            }
            if response.clicked() {
                apply = true;
            }
            ui.checkbox(&mut state.preview, tr("Live Preview"));
        });
        if conflict {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Graph Changed Externally; Use Load Current"),
            );
        }
        ui.weak(tr("Drag nodes · Connect ports · Ctrl+scroll to zoom · Double-click to center · Right-click to add"));
        if !state.message.is_empty() {
            ui.label(tr(&state.message));
        }
        if let Some(error) = &self.session.doc.render_error {
            ui.colored_label(ui.visuals().error_fg_color, tr(error));
        }
        ui.separator();
        let mut viewer = NodeViewer {
            definitions: Some(&self.plugin_nodes),
            layers: self.session.doc.layers.len(),
            message: String::new(),
            width: self.settings.ui.font_size * 12.,
            viewport: ui.available_rect_before_wrap(),
            fit_view: state.fit_view,
        };
        let style = SnarlStyle {
            node_layout: Some(NodeLayout::sandwich()),
            crisp_magnified_text: Some(false),
            ..SnarlStyle::default()
        };
        state.snarl.show(
            &mut viewer,
            &style,
            ("document-nodes", self.session.doc.identity),
            ui,
        );
        state.fit_view = false;
        if !viewer.message.is_empty() {
            state.message = viewer.message;
        }
        let mut effect = state.graph();
        for n in &mut effect.nodes {
            n.position = [0.; 2];
        }
        if state.previous.as_ref() != Some(&effect) {
            state.previous = Some(effect);
            state.edited = Some(Instant::now());
        }
        if state.preview
            && !conflict
            && state
                .edited
                .is_some_and(|t| t.elapsed() > Duration::from_millis(300))
        {
            apply = true;
        }
        if state.preview && state.edited.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if apply && !conflict {
            state.apply(self);
        }
        self.nodes = state;
    }
}
