use super::*;
use crate::{
    engine::workflow::{Edge, Graph, Node},
    plugins::nodes::{compatible, Port, Resolved},
};
use egui_snarl::{
    ui::{PinInfo, SnarlStyle, SnarlViewer},
    InPin, InPinId, NodeId, OutPin, OutPinId, Snarl,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Clone)]
struct ViewNode {
    node: Node,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
}
#[derive(Default)]
pub(super) struct WorkflowEditor {
    #[cfg(test)]
    run_rect: Option<Rect>,
    snarl: Snarl<ViewNode>,
    base: Graph,
    loaded: bool,
    search: String,
    note: String,
    report: Option<Value>,
    fit: bool,
    identity: u64,
    sequential: bool,
}
fn ports(
    node: &Node,
    catalog: &BTreeMap<String, Resolved>,
    edges: &[Edge],
) -> (Vec<Port>, Vec<Port>) {
    let def = catalog.get(&node.node_type);
    let mut inputs = def
        .map(|d| {
            d.inputs
                .iter()
                .filter(|p| {
                    p.id == "$exec"
                        || p.required
                        || node.parameters.get(&p.id).is_some()
                        || edges.iter().any(|e| e.to == node.id && e.input == p.id)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut outputs = def.map(|d| d.outputs.clone()).unwrap_or_default();
    let missing = |id: &str| Port {
        id: id.into(),
        title: format!("{id} (Unavailable)"),
        schema: json!({}),
        data_type: "any".into(),
        required: false,
    };
    for e in edges {
        if e.to == node.id && !inputs.iter().any(|p| p.id == e.input) {
            inputs.push(missing(&e.input));
        }
        if e.from == node.id && !outputs.iter().any(|p| p.id == e.output) {
            outputs.push(missing(&e.output));
        }
    }
    (inputs, outputs)
}
impl WorkflowEditor {
    pub(super) fn fit(&mut self) {
        self.fit = true;
    }
    fn load(&mut self, graph: &Graph, identity: u64, catalog: &BTreeMap<String, Resolved>) {
        self.snarl = Snarl::new();
        let mut ids = BTreeMap::new();
        for node in &graph.nodes {
            let (inputs, outputs) = ports(node, catalog, &graph.edges);
            ids.insert(
                node.id,
                self.snarl.insert_node(
                    pos2(node.position[0], node.position[1]),
                    ViewNode {
                        node: node.clone(),
                        inputs,
                        outputs,
                    },
                ),
            );
        }
        for e in &graph.edges {
            let a = ids[&e.from];
            let b = ids[&e.to];
            let output = self.snarl[a]
                .outputs
                .iter()
                .position(|p| p.id == e.output)
                .unwrap();
            let input = self.snarl[b]
                .inputs
                .iter()
                .position(|p| p.id == e.input)
                .unwrap();
            self.snarl
                .connect(OutPinId { node: a, output }, InPinId { node: b, input });
        }
        self.base = graph.clone();
        self.identity = identity;
        self.loaded = true;
        self.fit = true;
        self.note.clear();
    }
    fn graph(&self) -> Graph {
        Graph {
            version: 1,
            nodes: self
                .snarl
                .nodes_pos_ids()
                .map(|(_, p, n)| {
                    let mut node = n.node.clone();
                    node.position = [p.x, p.y];
                    node
                })
                .collect(),
            edges: self
                .snarl
                .wires()
                .map(|(a, b)| Edge {
                    from: self.snarl[a.node].node.id,
                    output: self.snarl[a.node].outputs[a.output].id.clone(),
                    to: self.snarl[b.node].node.id,
                    input: self.snarl[b.node].inputs[b.input].id.clone(),
                })
                .collect(),
        }
    }
    fn add(&mut self, def: &Resolved) {
        if self.snarl.nodes().count() >= 128 {
            return;
        }
        let id = (0..u32::MAX)
            .find(|id| !self.snarl.nodes().any(|n| n.node.id == *id))
            .unwrap();
        let node = Node {
            id,
            node_type: def.id.clone(),
            position: [id as f32 % 4. * 420., (id / 4) as f32 * 280.],
            parameters: super::schema::initial_parameters(def),
        };
        let (inputs, outputs) = ports(&node, &BTreeMap::from([(def.id.clone(), def.clone())]), &[]);
        self.snarl.insert_node(
            pos2(node.position[0], node.position[1]),
            ViewNode {
                node,
                inputs,
                outputs,
            },
        );
        self.fit = true;
    }
}
struct Viewer<'a> {
    catalog: &'a BTreeMap<String, Resolved>,
    report: Option<&'a Value>,
    note: String,
    fit: bool,
    viewport: Rect,
}
#[allow(refining_impl_trait)]
impl SnarlViewer<ViewNode> for Viewer<'_> {
    fn title(&mut self, n: &ViewNode) -> String {
        let status = if self.report.is_some_and(|r| r["failed_node"] == n.node.id) {
            "Failed · "
        } else if self.report.is_some_and(|r| {
            r["completed_nodes"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v == n.node.id))
        }) {
            "Done · "
        } else {
            ""
        };
        format!(
            "{}{} · #{}",
            crate::i18n::text(status),
            self.catalog
                .get(&n.node.node_type)
                .map(|d| crate::i18n::translated_for(Some(&d.owner), d.title.as_str()))
                .unwrap_or_else(|| crate::i18n::text("Node Type Unavailable")),
            n.node.id
        )
    }
    fn inputs(&mut self, n: &ViewNode) -> usize {
        n.inputs.len()
    }
    fn outputs(&mut self, n: &ViewNode) -> usize {
        n.outputs.len()
    }
    fn current_transform(&mut self, t: &mut egui::emath::TSTransform, snarl: &mut Snarl<ViewNode>) {
        if !self.fit {
            return;
        }
        let mut bounds = Rect::NOTHING;
        for (_, p, n) in snarl.nodes_pos_ids() {
            bounds = bounds.union(Rect::from_min_size(
                p,
                vec2(
                    380.,
                    100. + n.inputs.len().max(n.outputs.len()) as f32 * 34.,
                ),
            ));
        }
        if bounds.is_finite() {
            let view = self.viewport.shrink(36.);
            let scale = (view.width() / bounds.width())
                .min(view.height() / bounds.height())
                .clamp(0.2, 1.);
            *t = egui::emath::TSTransform {
                scaling: scale,
                translation: view.center().to_vec2() - bounds.center().to_vec2() * scale,
            };
        }
    }
    fn show_input(&mut self, p: &InPin, ui: &mut Ui, snarl: &mut Snarl<ViewNode>) -> PinInfo {
        let port = snarl[p.id.node].inputs[p.id.input].clone();
        ui.vertical(|ui| {
            ui.set_width(150.);
            let title = if port.id == "$exec" {
                "Execution Order"
            } else {
                &port.title
            };
            ui.add(
                Label::new(tr(format!(
                    "{title}{}",
                    if port.required { " *" } else { "" }
                )))
                .truncate(),
            )
            .on_hover_text(tr(format!("{} · {}", port.id, port.data_type)));
            if port.id != "$exec" {
                if p.remotes.is_empty() {
                    let present = snarl[p.id.node].node.parameters.get(&port.id).is_some();
                    let mut enabled = port.required || present;
                    if !port.required
                        && ui
                            .checkbox(&mut enabled, tr("Use This Parameter"))
                            .changed()
                        && !enabled
                    {
                        snarl[p.id.node]
                            .node
                            .parameters
                            .as_object_mut()
                            .unwrap()
                            .remove(&port.id);
                    }
                    if !enabled {
                        return;
                    }
                    let value = snarl[p.id.node]
                        .node
                        .parameters
                        .as_object_mut()
                        .unwrap()
                        .entry(port.id.clone())
                        .or_insert_with(|| super::schema::default_value(&port.schema));
                    super::schema::edit(ui, (p.id.node, port.id.as_str()), &port.schema, value);
                } else {
                    ui.weak(tr("Connected Value"));
                }
            }
        });
        PinInfo::circle().with_fill(if port.data_type == "execution" {
            ui.visuals().text_color()
        } else {
            ui.visuals().selection.stroke.color
        })
    }
    fn show_output(&mut self, p: &OutPin, ui: &mut Ui, snarl: &mut Snarl<ViewNode>) -> PinInfo {
        let port = &snarl[p.id.node].outputs[p.id.output];
        let title = match port.id.as_str() {
            "$exec" => "Execution Complete",
            "result" => "Full Result",
            _ => &port.title,
        };
        ui.label(tr(title))
            .on_hover_text(tr(format!("{} · {}", port.id, port.data_type)));
        PinInfo::circle().with_fill(if port.data_type == "execution" {
            ui.visuals().text_color()
        } else {
            ui.visuals().selection.stroke.color
        })
    }
    fn has_body(&mut self, _: &ViewNode) -> bool {
        false
    }
    fn show_body(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<ViewNode>,
    ) {
        ui.vertical(|ui| {
            ui.set_width(100.);
            let Some(def) = self.catalog.get(&snarl[id].node.node_type) else {
                ui.colored_label(ui.visuals().error_fg_color, tr(&snarl[id].node.node_type));
                return;
            };
            ui.weak(tr(&def.owner));
            ui.menu_button(tr("Add Parameter"), |ui| {
                for port in &def.inputs {
                    if port.id != "$exec"
                        && !snarl[id].inputs.iter().any(|p| p.id == port.id)
                        && ui.button(tr(&port.title)).clicked()
                    {
                        snarl[id].inputs.push(port.clone());
                        snarl[id].node.parameters[&port.id] =
                            super::schema::default_value(&port.schema);
                        ui.close();
                    }
                }
            });
        });
    }
    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<ViewNode>) {
        let a = &snarl[from.id.node].outputs[from.id.output];
        let b = &snarl[to.id.node].inputs[to.id.input];
        if !compatible(a, b) {
            self.note = "Incompatible Port Types".into();
            return;
        }
        let key = b.id.clone();
        let mut stack = vec![to.id.node];
        let mut seen = vec![];
        while let Some(node) = stack.pop() {
            if node == from.id.node {
                self.note = "Cannot Create a Cycle".into();
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
        if key != "$exec" {
            for remote in &to.remotes {
                snarl.disconnect(*remote, to.id);
            }
            snarl[to.id.node]
                .node
                .parameters
                .as_object_mut()
                .unwrap()
                .remove(&key);
        }
        snarl.connect(from.id, to.id);
    }
    fn has_node_menu(&mut self, _: &ViewNode) -> bool {
        true
    }
    fn show_node_menu(
        &mut self,
        id: NodeId,
        _: &[InPin],
        _: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<ViewNode>,
    ) {
        if let Some(def) = self.catalog.get(&snarl[id].node.node_type) {
            ui.menu_button(tr("Add Parameter"), |ui| {
                for p in &def.inputs {
                    if p.id != "$exec"
                        && !snarl[id].inputs.iter().any(|v| v.id == p.id)
                        && ui.button(tr(&p.title)).clicked()
                    {
                        snarl[id].inputs.push(p.clone());
                        snarl[id].node.parameters[&p.id] = super::schema::default_value(&p.schema);
                        ui.close();
                    }
                }
            });
        }
        ui.collapsing(tr("Parameters JSON"), |ui| {
            super::schema::edit(
                ui,
                (id, "all-parameters"),
                &json!({"type":"object"}),
                &mut snarl[id].node.parameters,
            );
        });
        if ui.button(tr("Delete Node")).clicked() {
            snarl.remove_node(id);
            ui.close();
        }
    }
}
impl Editor {
    pub(super) fn workflow_ui(&mut self, ui: &mut Ui) {
        let mut state = std::mem::take(&mut self.workflow);
        if !state.loaded
            || (state.identity != self.session.doc.identity
                && state.base != self.session.doc.workflow)
        {
            state.load(
                &self.session.doc.workflow,
                self.session.doc.identity,
                &self.plugin_nodes,
            );
        }
        state.identity = self.session.doc.identity;
        let conflict = state.base != self.session.doc.workflow;
        let mut check = false;
        let mut apply = false;
        let mut run = false;
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(tr("Add Plugin Node"), |ui| {
                ui.add(
                    TextEdit::singleline(&mut state.search)
                        .hint_text(tr("Search Actions, State and Plugins…"))
                        .desired_width(260.),
                );
                ScrollArea::vertical().max_height(360.).show(ui, |ui| {
                    for def in self
                        .plugin_nodes
                        .values()
                        .filter(|d| d.domain == "workflow")
                    {
                        if !format!(
                            "{} {} {}",
                            crate::i18n::search_text_for(Some(&def.owner), &def.title),
                            def.id,
                            def.owner
                        )
                        .to_lowercase()
                        .contains(&state.search.to_lowercase())
                        {
                            continue;
                        }
                        if ui
                            .button(tr(format!(
                                "{} · {}",
                                crate::i18n::translated_for(Some(&def.owner), &def.title),
                                def.owner
                            )))
                            .on_hover_text(tr(&def.id))
                            .clicked()
                        {
                            state.add(def);
                            ui.close();
                        }
                    }
                });
            });
            if ui.button(tr("Fit Graph")).clicked() {
                state.fit = true;
            }
            if ui.button(tr("Load Current")).clicked() {
                state.load(
                    &self.session.doc.workflow,
                    self.session.doc.identity,
                    &self.plugin_nodes,
                );
            }
            if ui.button(tr("Validate")).clicked() {
                check = true;
            }
            if ui
                .add_enabled(!conflict, Button::new(tr("Save Workflow")))
                .clicked()
            {
                apply = true;
            }
            let mut atomic = !state.sequential;
            ui.checkbox(&mut atomic, tr("Atomic Document Changes"))
                .on_hover_text(tr(
                    "Roll back document changes if a node fails. File and UI effects are rejected.",
                ));
            state.sequential = !atomic;
            let response = ui.add_enabled(!conflict, Button::new(tr("Run All Nodes")));
            #[cfg(test)]
            {
                state.run_rect = Some(response.rect);
            }
            if response.clicked() {
                apply = true;
                run = true;
            }
        });
        ui.weak(tr("Editing does not run actions. White ports order execution; colored ports carry values. All nodes run."));
        if conflict {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr("Workflow Changed Externally; Load Current before Editing"),
            );
        }
        if !state.note.is_empty() {
            ui.label(tr(&state.note));
        }
        let mut viewer = Viewer {
            catalog: &self.plugin_nodes,
            report: state.report.as_ref(),
            note: String::new(),
            fit: state.fit,
            viewport: ui.available_rect_before_wrap(),
        };
        state.snarl.show(
            &mut viewer,
            &SnarlStyle {
                node_layout: Some(egui_snarl::ui::NodeLayout::sandwich()),
                pin_placement: Some(egui_snarl::ui::PinPlacement::Outside { margin: 4. }),
                crisp_magnified_text: Some(false),
                ..Default::default()
            },
            "plugin-workflow",
            ui,
        );
        state.fit = false;
        if !viewer.note.is_empty() {
            state.note = viewer.note;
        }
        let graph = state.graph();
        if check {
            match self.session.apply(crate::application::Action::new(
                "workflow.validate",
                json!({"graph":graph}),
            )) {
                Ok(_) => state.note = "Validation Passed; Not Executed".into(),
                Err(e) => state.note = e.message,
            }
        }
        if apply {
            match self.session.apply(crate::application::Action::new(
                "workflow.set",
                json!({"graph":graph}),
            )) {
                Ok(_) => {
                    state.base = self.session.doc.workflow.clone();
                    state.note = "Workflow Saved; Not Executed".into();
                    if run {
                        match self.session.apply(crate::application::Action::new(
                            "workflow.run",
                            json!({"atomic":!state.sequential}),
                        )) {
                            Ok(r) => {
                                state.note = if r["result"]["ok"] == true {
                                    "Execution Complete".into()
                                } else {
                                    format!(
                                        "Execution Stopped at Node {}",
                                        r["result"]["failed_node"]
                                    )
                                };
                                state.report = Some(r["result"].clone());
                            }
                            Err(e) => state.note = e.message,
                        }
                    }
                    self.sync_from_session();
                }
                Err(e) => state.note = e.message,
            }
        }
        self.workflow = state;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::tests::{frame, studio};
    #[test]
    fn schema_node_editor_is_inert_until_run_and_reports_completed_nodes() {
        let mut app = studio();
        let ctx = Context::default();
        let g = json!({"version":1,"nodes":[
            {"id":1,"type":"state.document.active","position":[0,0]},
            {"id":2,"type":"action.view.set","position":[420,0],"parameters":{}}
        ],"edges":[{"from":1,"output":"width","to":2,"input":"pan_x"},{"from":1,"output":"height","to":2,"input":"pan_y"}]});
        app.editor
            .session
            .apply(crate::application::Action::new(
                "workflow.set",
                json!({"graph":g}),
            ))
            .unwrap();
        app.focus_panel(Panel::Workflow);
        app.editor.fit = false;
        frame(&mut app, &ctx, vec2(1360., 920.), vec![], 0.);
        let revision = app.editor.session.revision;
        for i in 1..3 {
            frame(&mut app, &ctx, vec2(1360., 920.), vec![], i as f64);
        }
        assert_eq!(app.editor.session.view.pan_x, 0.);
        assert_eq!(app.editor.session.revision, revision);
        let r = app.editor.workflow.run_rect.unwrap();
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
        assert_eq!(app.editor.session.view.pan_x, 1200.);
        assert_eq!(
            app.editor.workflow.report.as_ref().unwrap()["completed_nodes"],
            json!([1, 2])
        );
    }
}
