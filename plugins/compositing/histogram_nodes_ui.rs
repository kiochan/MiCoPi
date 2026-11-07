//! Cached published-node choices; no graph execution or pixel inspection here.
use super::{active, text, translated, ActionError, Session};
use crate::engine::{
    graph::{NodeKind, PortDomain},
    groups::GroupItem,
};
use eframe::egui::*;
use serde::Deserialize;
use std::{mem::Discriminant, sync::Arc};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum View {
    #[default]
    Source,
    Actual,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::Action, plugins::Registry};
    use serde_json::json;
    use std::path::PathBuf;

    fn session() -> Session {
        let mut s = Session::new(
            Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")).unwrap(),
        )
        .unwrap();
        s.apply(Action::new("canvas.new", json!({"width":4,"height":4})))
            .unwrap();
        s
    }

    #[test]
    fn choices_reuse_idle_metadata_preserve_names_and_invalidate_structural_ids() {
        let mut s = session();
        let mut model = Model::default();
        model.refresh_choices(&s);
        let first = model.choices.clone().unwrap();
        model.refresh_choices(&s);
        assert!(Arc::ptr_eq(&first, model.choices.as_ref().unwrap()));
        let layer = first.nodes.iter().find(|n| n.name.is_some()).unwrap().id;
        model.node = Some(layer);
        model.direction = Direction::Output;
        s.apply(Action::new("layer.rename", json!({"name":"私の ink"})))
            .unwrap();
        assert!(model.request(&s).is_none());
        model.refresh_choices(&s);
        assert_eq!(model.node, Some(layer));
        assert!(model.choice().unwrap().label().starts_with("私の ink"));
        assert!(model.request(&s).is_some());
        s.apply(Action::new("layer.fill_color", json!({"color":"#123456"})))
            .unwrap();
        model.refresh_choices(&s);
        assert_eq!(model.node, Some(layer));
        s.apply(Action::new("layer.add", json!({}))).unwrap();
        model.refresh_choices(&s);
        assert!(model.node.is_none());
        assert!(model.structure_changed);
        model.node = model.choices.as_ref().unwrap().nodes.first().map(|n| n.id);
        model.view = View::Actual;
        model.refresh_choices(&s);
        assert!(model.node.is_none());
        assert!(!model.structure_changed);
        model.node = model.choices.as_ref().unwrap().nodes.first().map(|n| n.id);
        s.apply(Action::new("canvas.new", json!({"width":2,"height":2})))
            .unwrap();
        model.refresh_choices(&s);
        assert!(model.node.is_none());
        assert!(!model.structure_changed);
    }

    #[test]
    fn wire_requires_complete_consistent_image_scope() {
        let value = json!({"graph_view":"actual","node_id":22,"port":"input","input_index":0,"resolved_node_id":11,"graph_enabled":true,"evaluation":"actual_plan"});
        let good: Wire = serde_json::from_value(value.clone()).unwrap();
        assert!(good.present());
        assert_eq!(good.decode().unwrap().resolved, 11);
        for (key, replacement) in [
            ("graph_enabled", json!(false)),
            ("evaluation", json!("source_declaration")),
            ("input_index", json!(3)),
            ("port", json!("output")),
            ("node_id", serde_json::Value::Null),
        ] {
            let mut bad = value.clone();
            bad[key] = replacement;
            assert!(
                serde_json::from_value::<Wire>(bad)
                    .unwrap()
                    .decode()
                    .is_err(),
                "{key}"
            );
        }
        let output: Wire = serde_json::from_value(json!({"graph_view":"source","node_id":22,"port":"output","resolved_node_id":22,"graph_enabled":false,"evaluation":"source_declaration"})).unwrap();
        assert!(!output.decode().unwrap().enabled);
        assert!(!Wire::default().present());
    }

    #[test]
    fn generated_owner_names_are_borrowed_and_coverage_ports_are_not_choices() {
        let mut s = session();
        let group = s
            .apply(Action::new(
                "group.create",
                json!({"members":[{"kind":"layer","id":1}],"name":"My 色 group"}),
            ))
            .unwrap()["result"]["group_id"]
            .as_u64()
            .unwrap();
        s.apply(Action::new(
            "mask.create",
            json!({"owner":{"kind":"group","id":group}}),
        ))
        .unwrap();
        let mut model = Model {
            view: View::Actual,
            ..Default::default()
        };
        model.refresh_choices(&s);
        let choices = model.choices.as_ref().unwrap();
        let gate = choices
            .nodes
            .iter()
            .find(|n| {
                s.doc
                    .histogram_node_owner(n.id)
                    .is_some_and(|(owner, _)| owner == GroupItem::group(group))
            })
            .unwrap();
        assert_eq!(gate.name.as_deref(), Some("My 色 group"));
        let graph = s.doc.histogram_actual_graph();
        for node in &graph.nodes {
            let choice = choices.nodes.iter().find(|n| n.id == node.id);
            if node.data.output_domain() == PortDomain::Coverage {
                assert!(choice.is_none());
            }
            if let Some(choice) = choice {
                assert!(choice
                    .inputs
                    .iter()
                    .all(|(i, _)| node.data.input_domain(*i as usize) == Some(PortDomain::Image)));
            }
        }
    }
}
impl View {
    pub fn id(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Actual => "actual",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Source => "Source Graph",
            Self::Actual => "Actual Composition",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Direction {
    #[default]
    Input,
    Output,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Request {
    pub view: View,
    pub node: u32,
    pub direction: Direction,
    pub input: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Observation {
    pub request: Request,
    pub resolved: u32,
    pub enabled: bool,
}
#[derive(Default, Deserialize)]
pub(super) struct Wire {
    graph_view: Option<View>,
    node_id: Option<u32>,
    port: Option<Direction>,
    input_index: Option<u32>,
    resolved_node_id: Option<u32>,
    graph_enabled: Option<bool>,
    evaluation: Option<String>,
}
impl Wire {
    pub fn present(&self) -> bool {
        self.graph_view.is_some()
            || self.node_id.is_some()
            || self.port.is_some()
            || self.input_index.is_some()
            || self.resolved_node_id.is_some()
            || self.graph_enabled.is_some()
            || self.evaluation.is_some()
    }
    pub fn decode(self) -> Result<Observation, ActionError> {
        let invalid =
            || ActionError::new("histogram_result", "Invalid histogram node scope metadata");
        let view = self.graph_view.ok_or_else(invalid)?;
        let direction = self.port.ok_or_else(invalid)?;
        let node = self.node_id.ok_or_else(invalid)?;
        let resolved = self.resolved_node_id.ok_or_else(invalid)?;
        let enabled = self.graph_enabled.ok_or_else(invalid)?;
        if self.evaluation.as_deref()
            != Some(match view {
                View::Source => "source_declaration",
                View::Actual => "actual_plan",
            })
            || view == View::Actual && !enabled
            || match direction {
                Direction::Input => self.input_index.is_none_or(|i| i > 2),
                Direction::Output => self.input_index.is_some() || resolved != node,
            }
        {
            return Err(invalid());
        }
        Ok(Observation {
            request: Request {
                view,
                node,
                direction,
                input: self.input_index,
            },
            resolved,
            enabled,
        })
    }
}
impl Observation {
    pub fn caption(self) -> String {
        let mut value = format!(
            "{} · #{} · {}",
            translated(self.request.view.title()),
            self.request.node,
            translated(match self.request.direction {
                Direction::Input => "Input",
                Direction::Output => "Output",
            })
        );
        if let Some(index) = self.request.input {
            value.push_str(&format!(" {index} ← #{}", self.resolved));
        }
        if self.request.view == View::Source && !self.enabled {
            value.push_str(&format!(" · {}", translated("Disabled Source Declaration")));
        }
        value
    }
    pub fn short(self) -> String {
        let title = match (self.request.view, self.request.direction) {
            (View::Source, Direction::Input) => "Source Input",
            (View::Source, Direction::Output) => "Source Output",
            (View::Actual, Direction::Input) => "Actual Input",
            (View::Actual, Direction::Output) => "Actual Output",
        };
        format!("{} #{}", translated(title), self.request.node)
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Identity {
    id: u32,
    kind: Discriminant<NodeKind>,
    node_type: Option<String>,
    layer: Option<u64>,
    mask: Option<u64>,
    owner: Option<GroupItem>,
}
struct Choice {
    id: u32,
    title: String,
    owner: Option<String>,
    name: Option<String>,
    inputs: Vec<(u32, Option<u32>)>,
}
impl Choice {
    fn label(&self) -> String {
        let title = self.name.as_ref().map_or_else(
            || crate::i18n::translated_for(self.owner.as_deref(), &self.title).into_owned(),
            |name| name.clone(),
        );
        format!("{title} · #{}", self.id)
    }
}
struct Choices {
    document: u64,
    revision: u64,
    view: View,
    identities: Vec<Identity>,
    edges: Vec<(u32, u32, u32)>,
    nodes: Vec<Choice>,
}
#[derive(Clone, Default)]
pub(super) struct Model {
    pub view: View,
    pub node: Option<u32>,
    pub direction: Direction,
    pub input: u32,
    choices: Option<Arc<Choices>>,
    structure_changed: bool,
}
impl Model {
    pub fn reset(&mut self) {
        let view = self.view;
        *self = Self {
            view,
            ..Self::default()
        };
    }
    pub fn refresh_choices(&mut self, s: &Session) {
        if active(s)
            || self.choices.as_ref().is_some_and(|c| {
                c.document == s.doc.identity && c.revision == s.revision && c.view == self.view
            })
        {
            return;
        }
        let graph = match self.view {
            View::Source => &s.doc.graph,
            View::Actual => &s.doc.histogram_actual_graph(),
        };
        let mut identities = Vec::with_capacity(graph.nodes.len());
        let mut nodes = Vec::new();
        for node in &graph.nodes {
            let generated = if self.view == View::Actual {
                s.doc.histogram_node_owner(node.id)
            } else {
                None
            };
            identities.push(Identity {
                id: node.id,
                kind: std::mem::discriminant(&node.data),
                node_type: node.node_type.clone(),
                layer: match node.data {
                    NodeKind::Layer { index } if index < s.doc.layers.len() => {
                        Some(s.doc.layer_id(index))
                    }
                    _ => None,
                },
                mask: match node.data {
                    NodeKind::MaskSource { mask_id } => Some(mask_id),
                    _ => None,
                },
                owner: generated.map(|g| g.0),
            });
            if node.data.output_domain() != PortDomain::Image {
                continue;
            }
            let name = generated
                .map(|g| g.1.to_owned())
                .or_else(|| match node.data {
                    NodeKind::Layer { index } if index < s.doc.layers.len() => {
                        Some(s.doc.layer_name(index).to_owned())
                    }
                    _ => None,
                });
            let definition = node
                .node_type
                .as_ref()
                .and_then(|id| s.registry.nodes.get(id));
            let canonical = node.node_type.as_ref().is_none_or(|id| {
                [
                    "compositing.layer",
                    "compositing.filter",
                    "compositing.levels",
                    "compositing.curves",
                    "compositing.blend",
                    "compositing.mix",
                    "compositing.apply_mask",
                    "compositing.masked_mix",
                    "compositing.output",
                    "compositing.transparent",
                    "compositing.mask_source",
                ]
                .contains(&id.as_str())
            });
            let title = if !canonical {
                definition
                    .map(|(_, d)| d.title.clone())
                    .unwrap_or_else(|| node.data.title())
            } else {
                match node.data {
                    NodeKind::Layer { .. } => "Layer".into(),
                    NodeKind::Filter { filter, .. } => filter.title().into(),
                    NodeKind::Levels { .. } => "Levels".into(),
                    NodeKind::Curves { .. } => "Curves".into(),
                    NodeKind::Blend { .. } => "Blend".into(),
                    NodeKind::Mix { .. } => "Backdrop Mix".into(),
                    NodeKind::ApplyMask { .. } => "Apply Mask".into(),
                    NodeKind::MaskedMix { .. } => "Masked Backdrop Mix".into(),
                    NodeKind::Output => "Canvas Output".into(),
                    NodeKind::Transparent => "Transparent".into(),
                    NodeKind::MaskSource { .. } => unreachable!(),
                }
            };
            let inputs = (0..node.data.inputs())
                .filter(|i| node.data.input_domain(*i) == Some(PortDomain::Image))
                .map(|i| {
                    (
                        i as u32,
                        graph
                            .edges
                            .iter()
                            .find(|e| e.to == node.id && e.input == i as u32)
                            .map(|e| e.from),
                    )
                })
                .collect();
            nodes.push(Choice {
                id: node.id,
                title,
                owner: definition
                    .map(|(owner, _)| owner.clone())
                    .or_else(|| Some("compositing".into())),
                name,
                inputs,
            });
        }
        identities.sort_by_key(|i| i.id);
        let mut edges = graph
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.input))
            .collect::<Vec<_>>();
        edges.sort_unstable();
        if let Some(old) = &self.choices {
            if old.document != s.doc.identity || old.view != self.view {
                self.node = None;
                self.structure_changed = false;
            } else if old.identities != identities || old.edges != edges {
                self.structure_changed = self.node.is_some();
                self.node = None;
            }
        }
        self.choices = Some(Arc::new(Choices {
            document: s.doc.identity,
            revision: s.revision,
            view: self.view,
            identities,
            edges,
            nodes,
        }));
    }
    fn choice(&self) -> Option<&Choice> {
        self.choices
            .as_ref()?
            .nodes
            .iter()
            .find(|n| Some(n.id) == self.node)
    }
    pub fn request(&self, s: &Session) -> Option<Request> {
        let c = self.choices.as_ref()?;
        if c.document != s.doc.identity || c.revision != s.revision || c.view != self.view {
            return None;
        }
        let choice = self.choice()?;
        if self.direction == Direction::Input
            && !choice
                .inputs
                .iter()
                .any(|(i, from)| *i == self.input && from.is_some())
        {
            return None;
        }
        Some(Request {
            view: self.view,
            node: choice.id,
            direction: self.direction,
            input: (self.direction == Direction::Input).then_some(self.input),
        })
    }
    pub fn inject(request: Request, s: &Session, args: &mut serde_json::Value) {
        args["graph_view"] = serde_json::json!(request.view.id());
        args["node_id"] = serde_json::json!(request.node);
        args["port"] = serde_json::json!(match request.direction {
            Direction::Input => "input",
            Direction::Output => "output",
        });
        if let Some(index) = request.input {
            args["input_index"] = serde_json::json!(index);
        }
        args["document_identity"] = serde_json::json!(s.doc.identity);
        args["document_revision"] = serde_json::json!(s.revision);
    }
    pub fn controls(&mut self, ui: &mut Ui, s: &Session) -> bool {
        let before = (self.view, self.node, self.direction, self.input);
        ui.add_enabled_ui(!active(s), |ui| {
            ui.label(text("Graph View"));
            ComboBox::from_id_salt("histogram-view")
                .width(ui.available_width())
                .truncate()
                .selected_text(text(self.view.title()))
                .show_ui(ui, |ui| {
                    for view in [View::Source, View::Actual] {
                        ui.selectable_value(&mut self.view, view, text(view.title()));
                    }
                });
            self.refresh_choices(s);
            ui.label(text("Node"));
            let label = self
                .choice()
                .map(Choice::label)
                .unwrap_or_else(|| translated("Select a Node").into_owned());
            let width = ui.available_width();
            let node_combo = ComboBox::from_id_salt("histogram-node")
                .width(width)
                .truncate()
                .selected_text(label)
                .show_ui(ui, |ui| {
                    ui.set_width(width);
                    ui.style_mut().wrap_mode = Some(TextWrapMode::Truncate);
                    if let Some(choices) = &self.choices {
                        for choice in &choices.nodes {
                            let response = ui.selectable_value(
                                &mut self.node,
                                Some(choice.id),
                                choice.label(),
                            );
                            if response.clicked() {
                                self.structure_changed = false;
                                self.direction = if choice.inputs.is_empty() {
                                    Direction::Output
                                } else {
                                    Direction::Input
                                };
                                self.input = choice.inputs.first().map_or(0, |p| p.0);
                            }
                            #[cfg(test)]
                            ui.data_mut(|d| {
                                d.insert_temp(
                                    Id::new(("histogram-node-option", s.doc.identity, choice.id)),
                                    response.rect,
                                )
                            });
                        }
                    }
                });
            #[cfg(test)]
            ui.data_mut(|d| {
                d.insert_temp(
                    Id::new(("histogram-node", s.doc.identity)),
                    node_combo.response.rect,
                )
            });
            #[cfg(not(test))]
            let _ = node_combo;
            ui.label(text("Image Port"));
            let inputs = self.choice().is_some_and(|n| !n.inputs.is_empty());
            let port_combo = ComboBox::from_id_salt("histogram-port")
                .width(ui.available_width())
                .truncate()
                .selected_text(text(match self.direction {
                    Direction::Input => "Input",
                    Direction::Output => "Output",
                }))
                .show_ui(ui, |ui| {
                    ui.add_enabled_ui(inputs, |ui| {
                        ui.selectable_value(&mut self.direction, Direction::Input, text("Input"));
                    });
                    let output_option =
                        ui.selectable_value(&mut self.direction, Direction::Output, text("Output"));
                    #[cfg(test)]
                    ui.data_mut(|d| {
                        d.insert_temp(
                            Id::new(("histogram-port-output", s.doc.identity)),
                            output_option.rect,
                        )
                    });
                    #[cfg(not(test))]
                    let _ = output_option;
                });
            #[cfg(test)]
            ui.data_mut(|d| {
                d.insert_temp(
                    Id::new(("histogram-port", s.doc.identity)),
                    port_combo.response.rect,
                )
            });
            #[cfg(not(test))]
            let _ = port_combo;
            if self.direction == Direction::Input {
                ui.label(text("Input Index"));
                let label = format!("{} {}", translated("Input"), self.input);
                ComboBox::from_id_salt("histogram-input")
                    .width(ui.available_width())
                    .truncate()
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        let ports = self.choice().map_or(&[][..], |n| n.inputs.as_slice());
                        let mut selected = self.input;
                        for (index, from) in ports {
                            let label = from.map_or_else(
                                || {
                                    format!(
                                        "{} {index} · {}",
                                        translated("Input"),
                                        translated("Unconnected")
                                    )
                                },
                                |id| format!("{} {index} ← #{id}", translated("Input")),
                            );
                            ui.add_enabled_ui(from.is_some(), |ui| {
                                ui.selectable_value(&mut selected, *index, label);
                            });
                        }
                        self.input = selected;
                    });
            }
        });
        if self.structure_changed {
            ui.weak(text("Graph structure changed. Select the node again."));
        } else if self.node.is_none() {
            ui.weak(text("Select a published image node to inspect."));
        } else if !active(s) && self.request(s).is_none() {
            ui.weak(text("Choose a connected image port."));
        }
        ui.add(Label::new(text(match self.view {View::Source=>"Source declarations include disabled effects; masks and ancestor gates are excluded.",View::Actual=>"Actual Composition includes generated masks and group strength."})).wrap());
        before != (self.view, self.node, self.direction, self.input)
    }
}
