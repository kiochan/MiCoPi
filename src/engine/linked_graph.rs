//! Layer-stack topology. Only metadata is rebuilt; pixel buffers and the pen path stay untouched.
use super::{graph::*, Layer};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub(crate) struct Branch {
    pub source: Node,
    pub filters: Vec<Node>,
    pub blend: Option<Node>,
}
impl Graph {
    pub fn linked_from_layers(layers: &[Layer]) -> Self {
        let mut graph = Self::from_layers(layers);
        graph.linked = true;
        graph.arrange_stack(layers.len()).unwrap();
        graph
    }
    pub fn arrange_stack(&mut self, count: usize) -> Result<(), String> {
        let branches = self.stack_branches(count)?;
        let column = branches.iter().map(|b| b.filters.len()).max().unwrap_or(0) as f32;
        let mut positions = BTreeMap::new();
        for (i, b) in branches.iter().enumerate() {
            let y = i as f32 * 280.;
            positions.insert(b.source.id, [0., y]);
            for (j, filter) in b.filters.iter().enumerate() {
                positions.insert(filter.id, [(j + 1) as f32 * 260., y]);
            }
            if let Some(blend) = &b.blend {
                positions.insert(blend.id, [(column + 1.) * 260., y]);
            }
        }
        for node in &mut self.nodes {
            node.position = positions
                .get(&node.id)
                .copied()
                .unwrap_or([(column + 2.) * 260., (count - 1) as f32 * 280.]);
        }
        Ok(())
    }
    pub fn has_filters(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| matches!(n.data, NodeKind::Filter { .. }))
    }
    /// Called after ordinary DAG validation, including missing ports and cycles.
    pub(crate) fn stack_branches(&self, count: usize) -> Result<Vec<Branch>, String> {
        let error = "Linked graph must contain every layer exactly once in stack order, with per-layer filters and unit-opacity blends";
        let nodes: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id, n)).collect();
        let inputs: BTreeMap<_, _> = self
            .edges
            .iter()
            .map(|e| ((e.to, e.input), e.from))
            .collect();
        let output = self
            .nodes
            .iter()
            .find(|n| matches!(n.data, NodeKind::Output))
            .ok_or(error)?;
        let mut used = BTreeSet::from([output.id]);
        let mut tail = *inputs.get(&(output.id, 0)).ok_or(error)?;
        let mut reversed = Vec::new();
        // Bounded even when inspecting an unvalidated draft from the desktop.
        for _ in 0..=self.nodes.len() {
            let node = nodes.get(&tail).ok_or(error)?;
            let (mut head, blend, background) = match node.data {
                NodeKind::Blend { opacity, .. } if opacity == 1. => {
                    if !used.insert(node.id) {
                        return Err(error.into());
                    }
                    (
                        *inputs.get(&(tail, 1)).ok_or(error)?,
                        Some((*node).clone()),
                        Some(*inputs.get(&(tail, 0)).ok_or(error)?),
                    )
                }
                _ => (tail, None, None),
            };
            let mut filters = Vec::new();
            let source = loop {
                let n = nodes.get(&head).ok_or(error)?;
                if !used.insert(n.id) {
                    return Err(error.into());
                }
                match n.data {
                    NodeKind::Layer { .. } => break (*n).clone(),
                    NodeKind::Filter { .. } => {
                        filters.push((*n).clone());
                        head = *inputs.get(&(head, 0)).ok_or(error)?;
                    }
                    _ => return Err(error.into()),
                }
            };
            filters.reverse();
            reversed.push(Branch {
                source,
                filters,
                blend,
            });
            if let Some(background) = background {
                tail = background;
            } else {
                break;
            }
        }
        reversed.reverse();
        if used.len() != self.nodes.len()
            || reversed.len() != count
            || reversed
                .iter()
                .enumerate()
                .any(|(i, b)| b.source.data != (NodeKind::Layer { index: i }))
        {
            return Err(error.into());
        }
        Ok(reversed)
    }
    /// Preserve branch/filter identity across layer reorder, insertion, duplication and deletion.
    pub(crate) fn sync_stack(
        &self,
        modes: &[BlendMode],
        old_ids: &[u64],
        new_ids: &[u64],
        duplicate: Option<(u64, u64)>,
    ) -> Result<Self, String> {
        self.sync_baked_stack(modes, old_ids, new_ids, duplicate, &[])
    }
    /// Preserve surviving branches while dropping filters already baked into a raster layer.
    pub(crate) fn sync_baked_stack(
        &self,
        modes: &[BlendMode],
        old_ids: &[u64],
        new_ids: &[u64],
        duplicate: Option<(u64, u64)>,
        baked: &[u64],
    ) -> Result<Self, String> {
        self.sync_stack_mapped(
            modes,
            old_ids,
            new_ids,
            &duplicate.into_iter().collect::<Vec<_>>(),
            baked,
        )
    }
    pub(crate) fn sync_stack_mapped(
        &self,
        modes: &[BlendMode],
        old_ids: &[u64],
        new_ids: &[u64],
        duplicates: &[(u64, u64)],
        baked: &[u64],
    ) -> Result<Self, String> {
        let mut branches = self.stack_branches(old_ids.len())?;
        for (id, branch) in old_ids.iter().zip(&mut branches) {
            if baked.contains(id) {
                branch.filters.clear();
            }
        }
        let by_id: BTreeMap<_, _> = old_ids.iter().copied().zip(branches).collect();
        let mut next = self
            .nodes
            .iter()
            .map(|n| n.id as u64 + 1)
            .max()
            .unwrap_or(0);
        let mut fresh = |data, position, kind: &str| -> Result<Node, String> {
            let id = u32::try_from(next).map_err(|_| "Node identity limit reached")?;
            next += 1;
            Ok(Node {
                id,
                data,
                position,
                node_type: Some(format!("compositing.{kind}")),
            })
        };
        let mut graph = Self {
            enabled: self.enabled,
            linked: true,
            nodes: vec![],
            edges: vec![],
        };
        let mut last = 0;
        for (index, (mode, id)) in modes.iter().zip(new_ids).enumerate() {
            let copy_from = duplicates
                .iter()
                .find(|(_, to)| to == id)
                .map(|(from, _)| *from);
            let old = by_id.get(&copy_from.unwrap_or(*id));
            let y = index as f32 * 240.;
            let mut source = match old.filter(|_| copy_from.is_none()) {
                Some(b) => b.source.clone(),
                None => fresh(NodeKind::Layer { index }, [0., y], "layer")?,
            };
            source.data = NodeKind::Layer { index };
            let mut head = source.id;
            graph.nodes.push(source);
            for filter in old.into_iter().flat_map(|b| &b.filters) {
                let mut node = if copy_from.is_some() {
                    fresh(filter.data.clone(), filter.position, "filter")?
                } else {
                    filter.clone()
                };
                node.node_type = filter.node_type.clone();
                graph.edges.push(Edge {
                    from: head,
                    to: node.id,
                    input: 0,
                });
                head = node.id;
                graph.nodes.push(node);
            }
            if index > 0 {
                let mut blend = match old
                    .filter(|_| copy_from.is_none())
                    .and_then(|b| b.blend.clone())
                {
                    Some(b) => b,
                    None => fresh(
                        NodeKind::Blend {
                            mode: *mode,
                            opacity: 1.,
                        },
                        [520., y],
                        "blend",
                    )?,
                };
                blend.data = NodeKind::Blend {
                    mode: *mode,
                    opacity: 1.,
                };
                graph.edges.extend([
                    Edge {
                        from: last,
                        to: blend.id,
                        input: 0,
                    },
                    Edge {
                        from: head,
                        to: blend.id,
                        input: 1,
                    },
                ]);
                head = blend.id;
                graph.nodes.push(blend);
            }
            last = head;
        }
        let output = self
            .nodes
            .iter()
            .find(|n| matches!(n.data, NodeKind::Output))
            .unwrap()
            .clone();
        graph.edges.push(Edge {
            from: last,
            to: output.id,
            input: 0,
        });
        graph.nodes.push(output);
        graph.compile(modes.len())?;
        if old_ids != new_ids {
            graph.arrange_stack(modes.len())?;
        }
        Ok(graph)
    }
}
