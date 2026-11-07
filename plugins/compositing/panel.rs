use super::*;
use crate::engine::graph::{Edge, Filter, Graph, Node, NodeKind, MAX_NODES};
use egui_snarl::{
    ui::{NodeLayout, PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, NodeId, OutPin, OutPinId, Snarl,
};
use std::collections::BTreeMap;
#[path = "curves_ui.rs"]
pub(super) mod curves_ui;
#[path = "groups_view.rs"]
mod groups_view;
#[path = "histogram_ui.rs"]
pub(super) mod histogram_ui;
#[path = "hsl_ui.rs"]
pub(super) mod hsl_ui;
#[path = "levels_ui.rs"]
pub(super) mod levels_ui;
#[derive(Default)]
pub(super) struct GraphEditor {
    #[cfg(test)]
    apply_rect: Option<Rect>,
    snarl: Snarl<Node>,
    base: Graph,
    base_groups: u64,
    effective: groups_view::EffectiveView,
    layer_filters: bool,
    had_groups: bool,
    identity: u64,
    enabled: bool,
    linked: bool,
    loaded: Option<Graph>,
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
    fn pass_through_mix_node_menu_and_opacity_edit_stay_in_the_source_draft() {
        fn label(shape: &Shape, title: &str) -> Option<Pos2> {
            match shape {
                Shape::Text(value) if value.galley.text() == title => {
                    Some(value.pos + value.galley.size() * 0.5)
                }
                Shape::Vec(values) => values.iter().find_map(|value| label(value, title)),
                _ => None,
            }
        }
        fn click(point: Pos2) -> Vec<Event> {
            vec![
                Event::PointerMoved(point),
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                },
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ]
        }
        let app = studio();
        let state = app.editor.session.state();
        for available in [false, true] {
            let ctx = Context::default();
            theme::fonts(&ctx);
            theme::apply(&ctx, &app.editor.settings.ui);
            let mut snarl = Snarl::new();
            let mut point = None;
            for n in 0..4 {
                let events = if n == 3 {
                    click(point.unwrap())
                } else {
                    vec![]
                };
                let output = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(360., 1000.))),
                        events,
                        time: Some(n as f64),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            add_menu(
                                ui,
                                &mut snarl,
                                pos2(20., 20.),
                                Some(&app.editor.plugin_nodes),
                                available,
                            )
                        });
                    },
                );
                if n == 2 {
                    point = output
                        .shapes
                        .iter()
                        .find_map(|shape| label(&shape.shape, "Backdrop Mix"));
                    assert!(point.is_some());
                }
            }
            assert_eq!(snarl.nodes().count(), usize::from(available));
            if !available {
                continue;
            }
            let id = snarl.node_ids().next().unwrap().0;
            assert_eq!(snarl[id].node_type.as_deref(), Some("compositing.mix"));
            assert!(matches!(snarl[id].data, NodeKind::Mix { opacity: 1. }));
            for n in 4..10 {
                let events = if n >= 7 {
                    let (rail, widget) = ctx
                        .data_mut(|data| {
                            data.get_temp::<(Rect, Rect)>(Id::new(("mix-opacity-control", id)))
                        })
                        .unwrap();
                    assert!(
                        widget.contains_rect(rail) && rail.width() < widget.width(),
                        "rail={rail:?}, widget={widget:?}"
                    );
                    let point = if n == 7 {
                        pos2(rail.left() + rail.width() * 0.9, rail.center().y)
                    } else {
                        rail.center()
                    };
                    let mut events = vec![Event::PointerMoved(point)];
                    if n == 7 || n == 9 {
                        events.push(Event::PointerButton {
                            pos: point,
                            button: PointerButton::Primary,
                            pressed: n == 7,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    events
                } else {
                    vec![]
                };
                let _ = ctx.run(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(360., 500.))),
                        events,
                        time: Some(n as f64),
                        focused: true,
                        ..Default::default()
                    },
                    |ctx| {
                        CentralPanel::default().show(ctx, |ui| {
                            let mut viewer = NodeViewer {
                                definitions: Some(&app.editor.plugin_nodes),
                                mix_available: true,
                                ..Default::default()
                            };
                            viewer.show_body(id, &[], &[], ui, &mut snarl);
                        });
                    },
                );
            }
            assert!(
                matches!(snarl[id].data,NodeKind::Mix {opacity} if opacity>0.2 && opacity<0.8),
                "draft after rail drag: {:?}",
                snarl[id].data
            );
            assert_eq!(app.editor.session.state(), state);
        }
    }
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
        app.editor.nodes.enabled = false;
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
            !app.editor.session.doc.graph.enabled,
            "button={r:?}, message={}, enabled={}, conflict={}, status={}",
            app.editor.nodes.message,
            app.editor.nodes.enabled,
            app.editor.nodes.base != app.editor.session.doc.graph,
            app.editor.status
        );
        assert_eq!(app.editor.session.doc.layers[0].pixels, before);
        app.editor.session.doc.undo();
        assert!(app.editor.session.doc.graph.enabled);
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
    #[test]
    fn linked_filter_menus_keep_connections_valid_and_source_controls_share_actions() {
        let mut app = studio();
        let mut state = GraphEditor::default();
        state.load(&app.editor.session.doc, false, &app.editor.plugin_nodes);
        let source = state
            .snarl
            .nodes_ids_data()
            .find(|(_, n)| matches!(n.value.data, NodeKind::Layer { .. }))
            .unwrap()
            .0;
        insert_filter(
            &mut state.snarl,
            source,
            pos2(260., 0.),
            NodeKind::Filter {
                filter: Filter::Invert,
                amount: 1.,
            },
            "compositing.filter",
        );
        state.graph().compile(1).unwrap();
        let (from, to) = state.snarl.wires().next().unwrap();
        let mut viewer = NodeViewer {
            linked: true,
            ..Default::default()
        };
        viewer.disconnect(
            &state.snarl.out_pin(from),
            &state.snarl.in_pin(to),
            &mut state.snarl,
        );
        viewer.drop_inputs(&state.snarl.in_pin(to), &mut state.snarl);
        viewer.drop_outputs(&state.snarl.out_pin(from), &mut state.snarl);
        assert_eq!(state.snarl.wires().count(), 2);
        let filter = state
            .snarl
            .nodes_ids_data()
            .find(|(_, n)| matches!(n.value.data, NodeKind::Filter { .. }))
            .unwrap()
            .0;
        remove_filter(&mut state.snarl, filter);
        state.graph().compile(1).unwrap();
        assert_eq!(state.snarl.wires().count(), 1);
        insert_filter(
            &mut state.snarl,
            source,
            pos2(260., 0.),
            NodeKind::Filter {
                filter: Filter::Invert,
                amount: 1.,
            },
            "compositing.filter",
        );
        state.apply(&mut app.editor);
        let widgets: Vec<_> = state
            .snarl
            .nodes_ids_data()
            .map(|(id, n)| (n.value.id, id))
            .collect();
        let before = app.editor.session.doc.composition();
        state.apply_layer_actions(
            &mut app.editor,
            vec![crate::application::Action::new(
                "layer.set",
                serde_json::json!({"layer_id":1,"visible":false,"opacity":0.5}),
            )],
        );
        assert!(state.message.is_empty(), "{}", state.message);
        assert!(!app.editor.session.doc.layers[0].visible);
        assert_eq!(app.editor.session.doc.layers[0].opacity, 128);
        assert_eq!(
            state
                .snarl
                .nodes_ids_data()
                .map(|(id, n)| (n.value.id, id))
                .collect::<Vec<_>>(),
            widgets
        );
        assert_eq!(state.base, app.editor.session.doc.graph);
        app.editor
            .session
            .apply(crate::application::Action::new(
                "history.undo",
                serde_json::json!({}),
            ))
            .unwrap();
        assert_eq!(app.editor.session.doc.composition(), before);
    }
    #[test]
    fn clean_node_views_follow_external_layers_but_unapplied_filters_are_preserved() {
        let mut app = studio();
        let ctx = Context::default();
        app.focus_panel(Panel::Nodes);
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.add",
                serde_json::json!({}),
            ))
            .unwrap();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 1.);
        assert_eq!(app.editor.nodes.base, app.editor.session.doc.graph);
        assert_eq!(app.editor.nodes.graph().nodes.len(), 4);
        let source = app
            .editor
            .nodes
            .snarl
            .nodes_ids_data()
            .find(|(_, n)| matches!(n.value.data, NodeKind::Layer { index: 0 }))
            .unwrap()
            .0;
        insert_filter(
            &mut app.editor.nodes.snarl,
            source,
            pos2(260., 0.),
            NodeKind::Filter {
                filter: Filter::Invert,
                amount: 1.,
            },
            "compositing.filter",
        );
        let draft = app.editor.nodes.graph();
        app.editor
            .session
            .apply(crate::application::Action::new(
                "layer.delete",
                serde_json::json!({"index":1}),
            ))
            .unwrap();
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 2.);
        assert_eq!(app.editor.nodes.graph(), draft);
        assert_ne!(app.editor.nodes.base, app.editor.session.doc.graph);
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
        self.base_groups = doc.groups_revision();
        self.identity = doc.identity;
        let graph = if default || doc.graph.nodes.is_empty() {
            Graph::linked_from_layers(&doc.layers)
        } else {
            doc.graph.clone()
        };
        self.enabled = graph.enabled;
        self.linked = graph.linked;
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
        let mut effect = self.graph();
        for n in &mut effect.nodes {
            n.position = [0.; 2];
        }
        self.previous = Some(effect);
        self.loaded = Some(self.graph());
        self.edited = None;
        self.message.clear();
        self.fit_view = true;
    }
    fn graph(&self) -> Graph {
        Graph {
            enabled: self.enabled,
            linked: self.linked,
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
        if self.identity != editor.session.doc.identity
            || self.base != editor.session.doc.graph
            || self.base_groups != editor.session.doc.groups_revision()
        {
            self.message = "Graph or Groups Changed Externally; Use Load Current".into();
            return;
        }
        let graph = self.graph();
        match editor.session.apply(crate::application::Action::new(
            "graph.set",
            serde_json::json!({"graph":graph}),
        )) {
            Ok(_) => {
                self.base = editor.session.doc.graph.clone();
                self.base_groups = editor.session.doc.groups_revision();
                self.loaded = Some(self.graph());
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
    fn apply_layer_actions(
        &mut self,
        editor: &mut Editor,
        actions: Vec<crate::application::Action>,
    ) {
        if self.identity != editor.session.doc.identity
            || self.base != editor.session.doc.graph
            || self.base_groups != editor.session.doc.groups_revision()
        {
            self.message = "Graph or Groups Changed Externally; Use Load Current".into();
            return;
        }
        let mut steps = vec![];
        if self.loaded.as_ref() != Some(&self.graph()) {
            steps.push(serde_json::json!({"id":"graph","action":"graph.set","args":{"graph":self.graph()}}));
        }
        for (i, action) in actions.iter().enumerate() {
            steps.push(serde_json::json!({"id":format!("layer{i}"),"action":action.action,"args":action.args}));
        }
        match editor.session.apply(crate::application::Action::new("pipeline.run", serde_json::json!({"atomic":true,"expected_revision":editor.session.revision,"steps":steps}))) {
            Ok(result) if result["result"]["ok"] == true => {
                // Preserve widget IDs during slider drags, including when the host
                // canonicalizes serialized branch order after a property edit.
                self.base = editor.session.doc.graph.clone();
                self.base_groups = editor.session.doc.groups_revision();
                for node in self.snarl.nodes_info_mut() {
                    if let Some(applied) = self.base.nodes.iter().find(|n| n.id == node.value.id) {
                        node.value = applied.clone();
                    }
                }
                self.loaded = Some(self.graph());
                self.edited = None;
                self.message.clear();
                editor.sync_from_session();
            }
            Ok(result) => { self.message = result["result"].to_string(); }
            Err(error) => { self.message = error.to_string(); }
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
    linked: bool,
    document: Option<&'a crate::engine::Document>,
    supported_blends: u32,
    mix_available: bool,
    levels_height: f32,
    curves_height: f32,
    hsl_height: f32,
    contact_active: bool,
    histogram: Option<std::sync::Arc<histogram_ui::Snapshot>>,
    actions: Vec<crate::application::Action>,
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
            linked: false,
            document: None,
            supported_blends: 0xff,
            mix_available: false,
            levels_height: levels_ui::estimated_height(&crate::configuration::Appearance::default()),
            curves_height: curves_ui::fallback_height(
                &crate::configuration::Appearance::default(),
                192.,
            ),
            hsl_height: hsl_ui::estimated_height(&crate::configuration::Appearance::default()),
            contact_active: false,
            histogram: None,
            actions: vec![],
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
    mix_available: bool,
) {
    let Some(definitions) = definitions else {
        ui.weak(tr("Node Definition Not Loaded"));
        return;
    };
    for def in definitions.values().filter(|d| d.domain == "image") {
        let available = mix_available
            || !matches!(&def.binding,crate::plugins::nodes::Binding::Image {operation,..} if operation=="mix");
        if ui
            .add_enabled(available, Button::new(tr(&def.title)))
            .on_hover_text(tr(&def.id))
            .on_disabled_hover_text(tr("Backdrop mixing requires a compatible compositor."))
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
fn insert_filter(
    snarl: &mut Snarl<Node>,
    after: NodeId,
    pos: Pos2,
    kind: NodeKind,
    definition: &str,
) {
    let output = OutPinId {
        node: after,
        output: 0,
    };
    let remotes = snarl.out_pin(output).remotes;
    if let Some(id) = add(snarl, pos, kind) {
        snarl[id].node_type = Some(definition.into());
        for remote in remotes {
            snarl.disconnect(output, remote);
            snarl.connect(
                OutPinId {
                    node: id,
                    output: 0,
                },
                remote,
            );
        }
        snarl.connect(output, InPinId { node: id, input: 0 });
    }
}
fn remove_filter(snarl: &mut Snarl<Node>, id: NodeId) {
    let from = snarl.in_pin(InPinId { node: id, input: 0 }).remotes;
    let to = snarl
        .out_pin(OutPinId {
            node: id,
            output: 0,
        })
        .remotes;
    snarl.remove_node(id);
    for from in from {
        for to in &to {
            snarl.connect(from, *to);
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
        for (_, pos, node) in snarl.nodes_pos_ids() {
            let height = if matches!(node.data, NodeKind::Curves { .. }) {
                self.curves_height
            } else if matches!(node.data, NodeKind::Hsl { .. }) {
                self.hsl_height
            } else if matches!(node.data, NodeKind::Levels { .. }) {
                self.levels_height
            } else if self.linked && matches!(node.data, NodeKind::Layer { .. }) {
                self.width * 1.4 + 24.
            } else {
                self.width + 24.
            };
            bounds = bounds.union(Rect::from_min_size(pos, vec2(self.width + 24., height)));
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
        if let NodeKind::Layer { index } = n.data {
            if let Some(doc) = self.document.filter(|d| index < d.layers.len()) {
                return doc.layer_name(index);
            }
        }
        if matches!(n.data, NodeKind::Blend { .. }) {
            if let NodeKind::Blend { mode, .. } = n.data {
                return format!("{} · {}", localize("Blend"), localize(mode.title()));
            }
        }
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
            if matches!(snarl[p.id.node].data, NodeKind::Mix { .. }) {
                if p.id.input == 0 {
                    "Before"
                } else {
                    "After"
                }
            } else if matches!(snarl[p.id.node].data, NodeKind::Blend { .. }) {
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
            if self.linked {
                if let NodeKind::Layer { index } = snarl[id].data {
                    if let Some(doc) = self.document.filter(|d| index < d.layers.len()) {
                        let layer = &doc.layers[index];
                        let mut visible = layer.visible;
                        let mut opacity = layer.opacity as f32 / 255.;
                        let mut mode = layer.mode;
                        ui.spacing_mut().slider_width = (self.width * 0.45).max(40.);
                        ui.weak(format!("ID {}", doc.layer_id(index)));
                        ui.checkbox(&mut visible, tr("Visible"));
                        ui.add(Slider::new(&mut opacity, 0. ..=1.).text(tr("Opacity")));
                        if let Some(next)=super::blend_modes::combo(ui,("node-layer-mode",id),mode,false,ui.available_width(),self.supported_blends).inner {mode=next;}
                        if visible != layer.visible || (opacity * 255.).round() as u8 != layer.opacity || mode != layer.mode {
                            self.actions.push(crate::application::Action::new("layer.set", serde_json::json!({"layer_id":doc.layer_id(index),"visible":visible,"opacity":opacity,"blend_mode":mode})));
                        }
                        ui.weak(tr("Right-click to insert a layer filter"));
                        return;
                    }
                    ui.weak(tr("Graph Changed Externally; Use Load Current"));
                    return;
                }
            }
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
            if def.id=="compositing.hsl" {if let NodeKind::Hsl{hsl}=&mut snarl[id].data {hsl_ui::edit(ui,Id::new(("hsl-node",id,self.document.map_or(0,|d|d.identity))),hsl);return;}}
            if def.id=="compositing.curves" {
                if let NodeKind::Curves {curves}=&mut snarl[id].data {
                    curves_ui::edit_with_histogram(ui,Id::new(("curves-node",id,self.document.map_or(0,|d|d.identity))),curves,self.document.and_then(|d|d.renderer.as_deref()),self.contact_active,self.histogram.as_deref());
                    return;
                }
            }
            if def.id=="compositing.levels" {
                if let NodeKind::Levels {parameters}=&mut snarl[id].data {
                    levels_ui::edit_with_histogram(ui,Id::new(("levels-node",id)),parameters,self.histogram.as_deref());
                    return;
                }
            }
            let mut data = serde_json::to_value(&snarl[id].data).unwrap();
            let operation = data.as_object_mut().unwrap().remove("kind").unwrap();
            if let Some(properties) = def.parameters_schema["properties"].as_object() {
                for (key, schema) in properties {
                    if self.linked && key == "opacity" && operation == "blend" { continue; }
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
                    } else if key == "opacity" && operation == "mix" {
                        let mut opacity=data[key].as_f64().unwrap_or(1.) as f32;
                        let response=ui.add_enabled(self.mix_available,Slider::new(&mut opacity,0. ..=1.)).on_disabled_hover_text(tr("Backdrop mixing requires a compatible compositor."));
                        #[cfg(test)] {
                            // Slider's union response includes its numeric editor; capture the
                            // actual rail widget for pointer-driven layout/input regressions.
                            let rail=ui.ctx().read_response(response.id).unwrap().rect;
                            ui.data_mut(|data|data.insert_temp(Id::new(("mix-opacity-control",id)),(rail,response.rect)));
                        }
                        if response.changed() {
                            data[key]=serde_json::json!(opacity);
                        }
                    } else if key == "mode" && operation == "blend" {
                        if let Ok(mode)=serde_json::from_value(data[key].clone()) {
                            if let Some(next)=super::blend_modes::combo(ui,("node-blend-mode",id),mode,false,ui.available_width(),self.supported_blends).inner {
                                data[key]=serde_json::json!(next);
                            }
                        }
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
        if self.linked {
            self.message =
                "Stack connections follow layer order; switch to Free Compositing to rewire".into();
            return;
        }
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
    fn disconnect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<Node>) {
        if !self.linked {
            snarl.disconnect(from.id, to.id);
        }
    }
    fn drop_inputs(&mut self, pin: &InPin, snarl: &mut Snarl<Node>) {
        if !self.linked {
            snarl.drop_inputs(pin.id);
        }
    }
    fn drop_outputs(&mut self, pin: &OutPin, snarl: &mut Snarl<Node>) {
        if !self.linked {
            snarl.drop_outputs(pin.id);
        }
    }
    fn has_graph_menu(&mut self, _: Pos2, _: &mut Snarl<Node>) -> bool {
        !self.linked
    }
    fn show_graph_menu(&mut self, pos: Pos2, ui: &mut Ui, snarl: &mut Snarl<Node>) {
        add_menu(ui, snarl, pos, self.definitions, self.mix_available);
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
        if self.linked {
            if matches!(
                snarl[id].data,
                NodeKind::Layer { .. }
                    | NodeKind::Filter { .. }
                    | NodeKind::Levels { .. }
                    | NodeKind::Curves { .. }
                    | NodeKind::Hsl { .. }
            ) {
                ui.menu_button(tr("Insert Filter"), |ui| {
                    if let Some(defs) = self.definitions {
                        for def in defs.values().filter(|d| matches!(&d.binding, crate::plugins::nodes::Binding::Image { operation, .. } if operation == "filter" || operation == "levels" || operation == "curves" || operation == "hsl")) {
                            if ui.button(tr(&def.title)).clicked() {
                                if let crate::plugins::nodes::Binding::Image { defaults, operation } = &def.binding {
                                    let mut data = defaults.clone();
                                    data["kind"] = serde_json::json!(operation);
                                    if let Ok(kind) = serde_json::from_value(data) {
                                        let pos = snarl.get_node_info(id).unwrap().pos + vec2(260., 0.);
                                        insert_filter(snarl, id, pos, kind, &def.id);
                                    }
                                }
                                ui.close();
                            }
                        }
                    }
                });
            }
            if matches!(
                snarl[id].data,
                NodeKind::Filter { .. }
                    | NodeKind::Levels { .. }
                    | NodeKind::Curves { .. }
                    | NodeKind::Hsl { .. }
            ) && ui.button(tr("Delete Node")).clicked()
            {
                remove_filter(snarl, id);
                ui.close();
            }
            return;
        }
        if ui.button(tr("Delete Node")).clicked() {
            snarl.remove_node(id);
            ui.close();
        }
    }
}
impl Editor {
    pub(super) fn nodes_ui(&mut self, ui: &mut Ui) {
        ui.label(tr("Image Compositing"));
        ui.separator();
        histogram_ui::controls(ui, &mut self.session);
        self.image_nodes_ui(ui);
    }

    fn image_nodes_ui(&mut self, ui: &mut Ui) {
        let mut state = std::mem::take(&mut self.nodes);
        if state.identity != self.session.doc.identity {
            state.layer_filters = false;
            state.had_groups = false;
            state.effective = Default::default();
            state.load(&self.session.doc, false, &self.plugin_nodes);
        }
        let groups = self.session.doc.has_groups()
            || self.session.doc.has_adjustments()
            || self.session.doc.has_raster_masks();
        if groups && !state.had_groups {
            state.layer_filters = false;
        }
        state.had_groups = groups;
        if groups {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut state.layer_filters, false, tr("Actual Composition"));
                ui.selectable_value(&mut state.layer_filters, true, tr("Layer Filters"));
            });
            if !state.layer_filters {
                state.effective.show(self, ui);
                self.nodes = state;
                return;
            }
            ui.weak(tr(if self.session.doc.has_adjustments(){"Layer Filters edits raster effects. Adjustment layers are shown in Actual Composition."}else{"Layer Filters edits individual source effects. Group composition is shown in Actual Composition."}));
        }
        if (state.base != self.session.doc.graph
            || state.base_groups != self.session.doc.groups_revision())
            && state.loaded.as_ref() == Some(&state.graph())
        {
            state.load(&self.session.doc, false, &self.plugin_nodes);
            state.fit_view = false;
        }
        let conflict = state.base != self.session.doc.graph
            || state.base_groups != self.session.doc.groups_revision();
        let mut apply = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(tr(if state.linked { "Linked Layer Stack" } else { "Free Compositing" }));
            ui.menu_button(tr("Graph Options"), |ui| {
                if state.linked {
                    if ui.add_enabled(!groups,Button::new(tr("Switch to Free Compositing"))).on_hover_text(tr("Groups require linked layer filters.")).clicked() {
                        state.linked = false;
                        ui.close();
                    }
                    if ui.button(tr("Arrange Stack")).clicked() {
                        let mut graph = state.graph();
                        if graph.arrange_stack(self.session.doc.layers.len()).is_ok() {
                            for node in state.snarl.nodes_info_mut() {
                                let p = graph.nodes.iter().find(|n| n.id == node.value.id).unwrap().position;
                                node.pos = pos2(p[0], p[1]);
                            }
                            state.fit_view = true;
                        }
                        ui.close();
                    }
                }
                if ui.button(tr("Rebuild from Layers")).on_hover_text(tr("Free Compositing uses explicit connections. Rebuild from Layers replaces this draft and its effects.")).clicked() {
                    state.load(&self.session.doc, true, &self.plugin_nodes);
                    ui.close();
                }
                if ui.button(tr("Load Current")).clicked() {
                    state.load(&self.session.doc, false, &self.plugin_nodes);
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut state.enabled,tr(if groups {"Layer Filters Enabled"} else {"Node Compositing"}));
            });
            if !state.linked {
                ui.menu_button(tr("Add Node"), |ui| {
                    add_menu(ui, &mut state.snarl, pos2(80., 80.), Some(&self.plugin_nodes),self.session.doc.renderer.as_ref().is_some_and(|renderer|renderer.supports_mix()));
                });
            }
            if ui.button(tr("Fit Graph")).on_hover_text(tr("Drag nodes · Ctrl+scroll to zoom · Double-click to center · Right-click for filters")).clicked() {
                state.fit_view = true;
            }
            let response = ui.add_enabled(!conflict, Button::new(tr("Apply to Canvas")));
            #[cfg(test)] { state.apply_rect = Some(response.rect); }
            if response.clicked() { apply = true; }
            ui.checkbox(&mut state.preview, tr("Live Preview"));
        });
        ui.weak(tr(if state.linked { "Layer edits keep this chain synchronized. Right-click a layer or filter to insert an effect." } else { "Free Compositing uses explicit connections. Rebuild from Layers replaces this draft and its effects." }));
        if conflict {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr(if state.base_groups != self.session.doc.groups_revision() {
                    "Graph or Groups Changed Externally; Use Load Current"
                } else {
                    "Graph Changed Externally; Use Load Current"
                }),
            );
        }

        if !state.message.is_empty() {
            ui.label(tr(&state.message));
        }
        if let Some(error) = &self.session.doc.render_error {
            ui.colored_label(ui.visuals().error_fg_color, tr(error));
        }
        ui.separator();
        let mut viewer = NodeViewer {
            definitions: Some(&self.plugin_nodes),
            histogram: histogram_ui::cached(ui.ctx(), &self.session),
            levels_height: levels_ui::estimated_height(&self.settings.ui),
            hsl_height: hsl_ui::estimated_height(&self.settings.ui),
            curves_height: if state.fit_view {
                curves_ui::estimated_height(ui, self.settings.ui.font_size * 12.)
            } else {
                0.
            },
            contact_active: self.session.stroke.active()
                || self.session.pointer_checkpoint.is_some(),
            layers: self.session.doc.layers.len(),
            message: String::new(),
            width: self.settings.ui.font_size * 12.,
            viewport: ui.available_rect_before_wrap(),
            fit_view: state.fit_view,
            linked: state.linked,
            document: if conflict {
                None
            } else {
                Some(&self.session.doc)
            },
            actions: vec![],
            supported_blends: super::blend_modes::supported(&self.session.doc),
            mix_available: self
                .session
                .doc
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.supports_mix()),
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
            state.message = viewer.message.clone();
        }
        let actions = std::mem::take(&mut viewer.actions);
        drop(viewer);
        let mut effect = state.graph();
        for n in &mut effect.nodes {
            n.position = [0.; 2];
        }
        if state.previous.as_ref() != Some(&effect) {
            state.previous = Some(effect);
            state.edited = Some(Instant::now());
        }
        let protected_contact = (self.session.stroke.active()
            || self.session.pointer_checkpoint.is_some())
            && (self.session.doc.graph.has_prepared_color()
                || state.snarl.nodes_ids_data().any(|(_, n)| {
                    matches!(
                        n.value.data,
                        NodeKind::Levels { .. } | NodeKind::Curves { .. } | NodeKind::Hsl { .. }
                    )
                }));
        if state.preview
            && !protected_contact
            && !conflict
            && state
                .edited
                .is_some_and(|t| t.elapsed() > Duration::from_millis(300))
        {
            apply = true;
        }
        if state.preview && state.edited.is_some() && !protected_contact {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if apply && !conflict {
            state.apply(self);
        }
        if !actions.is_empty() && !conflict {
            state.apply_layer_actions(self, actions);
        }
        self.nodes = state;
    }
}
