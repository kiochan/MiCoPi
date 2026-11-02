use crate::application::ActionError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
pub const MAX_BYTES: usize = 512 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: u32,
    #[serde(rename = "type")]
    pub node_type: String,
    pub position: [f32; 2],
    #[serde(default = "crate::application::actions_empty_object")]
    pub parameters: Value,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: u32,
    pub output: String,
    pub to: u32,
    pub input: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
impl Default for Graph {
    fn default() -> Self {
        Self {
            version: 1,
            nodes: vec![],
            edges: vec![],
        }
    }
}
impl Graph {
    pub fn bytes(&self) -> usize {
        serde_json::to_vec(self).map_or(MAX_BYTES + 1, |b| b.len())
    }
    pub fn validate_structure(&self) -> Result<(), ActionError> {
        let fail = |message| ActionError::new("invalid_workflow", message);
        if self.version != 1
            || self.nodes.len() > 128
            || self.edges.len() > 512
            || self.bytes() > MAX_BYTES
        {
            return Err(fail("Graph version, node count or size exceeds limits"));
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if !ids.insert(node.id)
                || !node.parameters.is_object()
                || node.node_type.len() > 200
                || node
                    .position
                    .iter()
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
            {
                return Err(fail("Invalid node ID, position or parameters"));
            }
        }
        let mut edges = BTreeSet::new();
        for edge in &self.edges {
            if !ids.contains(&edge.from)
                || !ids.contains(&edge.to)
                || !edges.insert((edge.from, &edge.output, edge.to, &edge.input))
            {
                return Err(fail("Invalid or duplicate edge reference"));
            }
        }
        Ok(())
    }
    pub fn patched(&self, operations: Vec<Patch>) -> Result<Self, ActionError> {
        if operations.len() > 256 {
            return Err(ActionError::new(
                "workflow_limit",
                "At most 256 patches are allowed",
            ));
        }
        let mut graph = self.clone();
        for op in operations {
            match op {
                Patch::AddNode { node } => {
                    if graph.nodes.iter().any(|n| n.id == node.id) {
                        return Err(ActionError::new("invalid_workflow", "Duplicate node ID"));
                    }
                    graph.nodes.push(node);
                }
                Patch::RemoveNode { id } => {
                    let index = graph.nodes.iter().position(|n| n.id == id).ok_or_else(|| {
                        ActionError::new("invalid_workflow", "Node does not exist")
                    })?;
                    graph.nodes.remove(index);
                    graph.edges.retain(|e| e.from != id && e.to != id);
                }
                Patch::SetParameters { id, parameters } => {
                    graph
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == id)
                        .ok_or_else(|| ActionError::new("invalid_workflow", "Node does not exist"))?
                        .parameters = parameters;
                }
                Patch::MoveNode { id, position } => {
                    graph
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == id)
                        .ok_or_else(|| ActionError::new("invalid_workflow", "Node does not exist"))?
                        .position = position;
                }
                Patch::Connect { edge } => graph.edges.push(edge),
                Patch::Disconnect { edge } => {
                    let i = graph.edges.iter().position(|e| *e == edge).ok_or_else(|| {
                        ActionError::new("invalid_workflow", "Edge does not exist")
                    })?;
                    graph.edges.remove(i);
                }
            }
        }
        graph.validate_structure()?;
        Ok(graph)
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Patch {
    AddNode { node: Node },
    RemoveNode { id: u32 },
    SetParameters { id: u32, parameters: Value },
    MoveNode { id: u32, position: [f32; 2] },
    Connect { edge: Edge },
    Disconnect { edge: Edge },
}
pub fn schema() -> Value {
    json!({"type":"object","required":["version","nodes","edges"],"additionalProperties":false,"properties":{
        "version":{"type":"integer","enum":[1]},"nodes":{"type":"array","maxItems":128,"items":{"type":"object","required":["id","type","position"],"additionalProperties":false,"properties":{"id":{"type":"integer","minimum":0,"maximum":4294967295u64},"type":{"type":"string"},"position":{"type":"array","minItems":2,"maxItems":2,"items":{"type":"number","minimum":-1000000,"maximum":1000000}},"parameters":{"type":"object"}}}},
        "edges":{"type":"array","maxItems":512,"items":{"type":"object","required":["from","output","to","input"],"additionalProperties":false,"properties":{"from":{"type":"integer","minimum":0,"maximum":4294967295u64},"to":{"type":"integer","minimum":0,"maximum":4294967295u64},"input":{"type":"string"},"output":{"type":"string"}}}}
    }})
}
