//! Plugin node contributions and schema-derived action/state adapters.
use super::{valid_id, validate, Registry};
use crate::{
    application::{pipeline, ActionError},
    engine::graph::NodeKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    Action { action: String },
    State { resource: String },
    Image { operation: String, defaults: Value },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub category: String,
    pub binding: Binding,
    #[serde(default = "crate::application::actions_empty_object")]
    pub defaults: Value,
    #[serde(default)]
    pub parameters_schema: Option<Value>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Port {
    pub id: String,
    pub title: String,
    pub schema: Value,
    pub data_type: String,
    pub required: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct Resolved {
    pub argument_mode: String,
    pub id: String,
    pub owner: String,
    pub title: String,
    pub category: String,
    pub domain: String,
    pub binding: Binding,
    pub parameters_schema: Value,
    pub output_schema: Value,
    pub defaults: Value,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub read_only: bool,
}
pub fn data_type(s: &Value) -> String {
    s["type"].as_str().unwrap_or("any").into()
}
fn port(id: &str, schema: Value, required: bool) -> Port {
    Port {
        id: id.into(),
        title: schema["title"].as_str().unwrap_or(id).into(),
        data_type: data_type(&schema),
        schema,
        required,
    }
}
impl Definition {
    pub fn resolve(&self, registry: &Registry, owner: &str) -> Result<Resolved, String> {
        let (mut input, output, readonly, domain) = match &self.binding {
            Binding::Action { action } => {
                let a = registry
                    .actions
                    .get(action)
                    .ok_or_else(|| format!("Unknown action {action}"))?;
                if self.parameters_schema.is_some() {
                    return Err("Action nodes inherit the bound action schema".into());
                }
                (
                    a.schema(),
                    a.output_schema.clone().unwrap_or(json!({})),
                    a.is_read_only(),
                    "workflow",
                )
            }
            Binding::State { resource } => {
                if self.parameters_schema.is_some() || self.defaults != json!({}) {
                    return Err("State nodes have no mutable parameters".into());
                }
                let schema = if let Some(s) = registry.states.get(resource) {
                    s.output_schema.clone()
                } else if pipeline::RESOURCES.iter().any(|(id, _)| *id == resource) {
                    pipeline::resource_schema(resource)
                } else {
                    return Err(format!("Unknown public state {resource}"));
                };
                (
                    json!({"type":"object","additionalProperties":false}),
                    schema,
                    true,
                    "workflow",
                )
            }
            Binding::Image {
                operation,
                defaults,
            } => {
                if !defaults.is_object() {
                    return Err("Image node defaults must be an object".into());
                }
                if self.defaults != json!({}) {
                    return Err("Image node defaults belong in binding.defaults".into());
                }
                let schema = self
                    .parameters_schema
                    .clone()
                    .ok_or("Image nodes require parameters_schema")?;
                let fields: &[&str] = match operation.as_str() {
                    "layer" => &["index"],
                    "filter" => &["filter", "amount"],
                    "levels" => &["parameters"],
                    "curves" => &["curves"],
                    "hsl" => &["hsl"],
                    "blend" => &["mode", "opacity"],
                    "mix" => &["opacity"],
                    "output" | "transparent" => &[],
                    _ => return Err("Unknown image kernel operation".into()),
                };
                if schema["type"] != "object"
                    || defaults
                        .as_object()
                        .unwrap()
                        .keys()
                        .any(|key| !fields.contains(&key.as_str()))
                    || schema["properties"]
                        .as_object()
                        .is_some_and(|p| p.keys().any(|key| !fields.contains(&key.as_str())))
                {
                    return Err("Image node parameters must match the kernel fields".into());
                }
                let mut value = defaults.clone();
                value["kind"] = json!(operation);
                let kind: NodeKind = serde_json::from_value(value).map_err(|e| e.to_string())?;
                match kind {
                    NodeKind::Levels { parameters } => parameters.validate()?,
                    NodeKind::Curves { curves } => curves.validate()?,
                    NodeKind::Hsl { hsl } => hsl.validate()?,
                    NodeKind::Layer { index } if index >= 16 => {
                        return Err("Default layer index is out of range".into())
                    }
                    NodeKind::Filter { filter, amount }
                        if !amount.is_finite() || !(filter.minimum()..=1.).contains(&amount) =>
                    {
                        return Err("Default filter amount is out of range".into())
                    }
                    NodeKind::Blend { opacity, .. } | NodeKind::Mix { opacity }
                        if !opacity.is_finite() || !(0. ..=1.).contains(&opacity) =>
                    {
                        return Err("Default blend opacity is out of range".into())
                    }
                    _ => {}
                }
                validate(&schema, defaults).map_err(|e| e.to_string())?;
                (
                    schema,
                    json!({"type":"object","title":"Image","description":"Native ARGB image buffer"}),
                    true,
                    "image",
                )
            }
        };
        super::registry::check_schema(&input)?;
        super::registry::check_schema(&output)?;
        let scalar = matches!(self.binding, Binding::Action { .. }) && input["type"] != "object";
        if scalar {
            input = json!({"type":"object","required":["value"],"properties":{"value":input},"additionalProperties":false});
        }
        let mut partial = input.clone();
        partial.as_object_mut().unwrap().remove("required");
        validate(&partial, &self.defaults).map_err(|e| e.to_string())?;
        let (inputs, outputs) = if let Binding::Image { operation, .. } = &self.binding {
            let image =
                json!({"type":"object","title":"Image","description":"Native ARGB image buffer"});
            let names: &[&str] = match operation.as_str() {
                "layer" | "transparent" => &[],
                "blend" => &["background", "foreground"],
                "mix" => &["before", "after"],
                _ => &["image"],
            };
            let image_port = |id: &str| Port {
                id: id.into(),
                title: match id {
                    "background" => "Background",
                    "foreground" => "Foreground",
                    "before" => "Before",
                    "after" => "After",
                    _ => "Image",
                }
                .into(),
                schema: image.clone(),
                data_type: "image".into(),
                required: true,
            };
            (
                names.iter().map(|id| image_port(id)).collect(),
                if operation == "output" {
                    vec![]
                } else {
                    vec![image_port("image")]
                },
            )
        } else {
            let mut inputs = vec![Port {
                id: "$exec".into(),
                title: "Execution Order".into(),
                schema: json!({}),
                data_type: "execution".into(),
                required: false,
            }];
            if let Some(properties) = input["properties"].as_object() {
                for (id, schema) in properties {
                    if ["$exec", "result"].contains(&id.as_str()) && id == "$exec" {
                        return Err("$exec is a reserved port".into());
                    }
                    inputs.push(port(
                        id,
                        schema.clone(),
                        input["required"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|v| v == id)),
                    ));
                }
            }
            let mut outputs = vec![
                Port {
                    id: "$exec".into(),
                    title: "Execution Complete".into(),
                    schema: json!({}),
                    data_type: "execution".into(),
                    required: false,
                },
                port("result", output.clone(), true),
            ];
            if let Some(properties) = output["properties"].as_object() {
                for (id, schema) in properties {
                    if !["$exec", "result"].contains(&id.as_str()) {
                        outputs.push(port(
                            id,
                            schema.clone(),
                            output["required"]
                                .as_array()
                                .is_some_and(|a| a.iter().any(|v| v == id)),
                        ));
                    }
                }
            }
            (inputs, outputs)
        };
        Ok(Resolved {
            argument_mode: if scalar { "value" } else { "object" }.into(),
            id: self.id.clone(),
            owner: owner.into(),
            title: self.title.clone(),
            category: self.category.clone(),
            domain: domain.into(),
            binding: self.binding.clone(),
            parameters_schema: input,
            output_schema: output,
            defaults: match &self.binding {
                Binding::Image { defaults, .. } => defaults.clone(),
                _ => self.defaults.clone(),
            },
            inputs,
            outputs,
            read_only: readonly,
        })
    }
}
impl Registry {
    pub fn normalize_image_nodes(
        &self,
        mut graph: crate::engine::graph::Graph,
    ) -> Result<crate::engine::graph::Graph, ActionError> {
        for node in &mut graph.nodes {
            if node.node_type.is_none() {
                let data = serde_json::to_value(&node.data)?;
                node.node_type = Some(format!("compositing.{}", data["kind"].as_str().unwrap()));
            }
        }
        self.validate_image_nodes(&graph)?;
        Ok(graph)
    }
    pub fn node_catalog(&self) -> BTreeMap<String, Resolved> {
        let mut out = BTreeMap::new();
        for a in self.actions.values() {
            let def = Definition {
                id: format!("action.{}", a.id),
                title: a.title.clone(),
                category: "Actions".into(),
                binding: Binding::Action {
                    action: a.id.clone(),
                },
                defaults: json!({}),
                parameters_schema: None,
            };
            if let Ok(node) = def.resolve(
                self,
                self.owners.get(&a.id).map(String::as_str).unwrap_or("core"),
            ) {
                out.insert(node.id.clone(), node);
            }
        }
        for (id, title) in pipeline::RESOURCES.iter().copied().chain(
            self.states
                .values()
                .map(|s| (s.id.as_str(), s.title.as_str())),
        ) {
            let def = Definition {
                id: format!("state.{id}"),
                title: title.into(),
                category: "Public State".into(),
                binding: Binding::State {
                    resource: id.into(),
                },
                defaults: json!({}),
                parameters_schema: None,
            };
            let owner = self
                .loaded
                .iter()
                .filter(|owner| id.starts_with(&format!("{owner}.")))
                .max_by_key(|o| o.len())
                .map(String::as_str)
                .unwrap_or("core");
            if let Ok(node) = def.resolve(self, owner) {
                out.insert(node.id.clone(), node);
            }
        }
        for (id, (owner, def)) in &self.nodes {
            if let Ok(node) = def.resolve(self, owner) {
                out.insert(id.clone(), node);
            }
        }
        out
    }
    pub fn validate_image_nodes(
        &self,
        graph: &crate::engine::graph::Graph,
    ) -> Result<(), ActionError> {
        let catalog = self.node_catalog();
        for node in &graph.nodes {
            if let Some(id) = &node.node_type {
                let def = catalog
                    .get(id)
                    .ok_or_else(|| ActionError::new("missing_node_type", id))?;
                let Binding::Image { operation, .. } = &def.binding else {
                    return Err(ActionError::new(
                        "node_domain",
                        "Image graphs can only use image nodes",
                    ));
                };
                let mut value = serde_json::to_value(&node.data)?;
                let kind = value.as_object_mut().unwrap().remove("kind").unwrap();
                if kind != *operation {
                    return Err(ActionError::new(
                        "node_binding",
                        "Node type does not match the image operation",
                    ));
                }
                validate(&def.parameters_schema, &value)?;
            }
        }
        Ok(())
    }
}
pub fn compatible(from: &Port, to: &Port) -> bool {
    if let (Some(a), Some(b)) = (from.schema["enum"].as_array(), to.schema["enum"].as_array()) {
        if !a.iter().any(|v| b.contains(v)) {
            return false;
        }
    }
    if ["execution", "image"].contains(&from.data_type.as_str())
        || ["execution", "image"].contains(&to.data_type.as_str())
    {
        return from.data_type == to.data_type;
    }
    from.data_type == to.data_type
        || from.data_type == "any"
        || to.data_type == "any"
        || (from.data_type == "integer" && to.data_type == "number")
}
pub fn valid_definition_id(id: &str, owner: &str) -> bool {
    valid_id(id)
        && id.starts_with(&format!("{owner}."))
        && !id.starts_with("action.")
        && !id.starts_with("state.")
}
