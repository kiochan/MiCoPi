//! Bounded stable-ID group hierarchy over raster-only storage.
use super::graph::{BlendMode, Edge, Graph, Node, NodeKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_GROUPS: usize = 16;
pub const MAX_DEPTH: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Layer,
    Group,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupItem {
    pub kind: ItemKind,
    pub id: u64,
}
impl GroupItem {
    pub fn layer(id: u64) -> Self {
        Self {
            kind: ItemKind::Layer,
            id,
        }
    }
    pub fn group(id: u64) -> Self {
        Self {
            kind: ItemKind::Group,
            id,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: u8,
    pub blend_mode: BlendMode,
    pub children: Vec<GroupItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupTree {
    pub version: u32,
    pub roots: Vec<GroupItem>,
    pub groups: Vec<Group>,
}
impl GroupTree {
    pub fn flat(ids: &[u64]) -> Self {
        Self {
            version: 1,
            roots: ids.iter().map(|id| GroupItem::layer(*id)).collect(),
            groups: Vec::new(),
        }
    }
    pub fn group(&self, id: u64) -> Result<&Group, String> {
        self.groups
            .iter()
            .find(|g| g.id == id)
            .ok_or_else(|| "Group does not exist".into())
    }
    pub fn group_mut(&mut self, id: u64) -> Result<&mut Group, String> {
        self.groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or_else(|| "Group does not exist".into())
    }
    pub fn children(&self, parent: u64) -> Result<&[GroupItem], String> {
        if parent == 0 {
            Ok(&self.roots)
        } else {
            Ok(&self.group(parent)?.children)
        }
    }
    pub fn children_mut(&mut self, parent: u64) -> Result<&mut Vec<GroupItem>, String> {
        if parent == 0 {
            Ok(&mut self.roots)
        } else {
            Ok(&mut self.group_mut(parent)?.children)
        }
    }
    pub fn parent(&self, item: GroupItem) -> Result<u64, String> {
        if self.roots.contains(&item) {
            return Ok(0);
        }
        self.groups
            .iter()
            .find(|g| g.children.contains(&item))
            .map(|g| g.id)
            .ok_or_else(|| "Tree item does not exist".into())
    }
    pub fn descendants(&self, item: GroupItem) -> Result<Vec<u64>, String> {
        fn visit(
            tree: &GroupTree,
            item: GroupItem,
            out: &mut Vec<u64>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > MAX_DEPTH {
                return Err("Group depth limit reached".into());
            }
            match item.kind {
                ItemKind::Layer => out.push(item.id),
                ItemKind::Group => {
                    for child in &tree.group(item.id)?.children {
                        visit(tree, *child, out, depth + 1)?;
                    }
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        visit(self, item, &mut out, 0)?;
        Ok(out)
    }
    pub fn layer_ids(&self) -> Result<Vec<u64>, String> {
        let mut out = Vec::new();
        for item in &self.roots {
            out.extend(self.descendants(*item)?);
        }
        Ok(out)
    }
    pub fn validate(&self, ids: &[u64], next: u64) -> Result<(), String> {
        if self.version != 1
            || self.groups.is_empty()
            || self.groups.len() > MAX_GROUPS
            || next == 0
            || next > u32::MAX as u64 + 1
        {
            return Err("Invalid group count or allocator".into());
        }
        let mut group_ids = BTreeSet::new();
        for group in &self.groups {
            if group.id == 0
                || group.id >= next
                || !group_ids.insert(group.id)
                || group.name.trim().is_empty()
                || group.name.len() > 512
                || group.name.chars().any(char::is_control)
            {
                return Err("Invalid group identity or name".into());
            }
        }
        fn visit(
            tree: &GroupTree,
            children: &[GroupItem],
            seen: &mut BTreeSet<GroupItem>,
            leaves: &mut Vec<u64>,
            depth: usize,
        ) -> Result<(), String> {
            for item in children {
                if item.id == 0 || !seen.insert(*item) {
                    return Err("Duplicate or cyclic group membership".into());
                }
                match item.kind {
                    ItemKind::Layer => leaves.push(item.id),
                    ItemKind::Group => {
                        if depth >= MAX_DEPTH {
                            return Err("Group depth limit reached".into());
                        }
                        visit(
                            tree,
                            &tree.group(item.id)?.children,
                            seen,
                            leaves,
                            depth + 1,
                        )?;
                    }
                }
            }
            Ok(())
        }
        let mut seen = BTreeSet::new();
        let mut leaves = Vec::new();
        visit(self, &self.roots, &mut seen, &mut leaves, 0)?;
        if leaves != ids
            || group_ids
                .iter()
                .any(|id| !seen.contains(&GroupItem::group(*id)))
        {
            return Err(
                "Group leaves must contain every raster exactly once in storage order".into(),
            );
        }
        Ok(())
    }
    pub fn remove_layer(&mut self, id: u64) {
        self.roots.retain(|item| *item != GroupItem::layer(id));
        for g in &mut self.groups {
            g.children.retain(|item| *item != GroupItem::layer(id));
        }
    }
    pub fn insert_layer_after(&mut self, source: u64, id: u64) -> Result<(), String> {
        let item = GroupItem::layer(source);
        let parent = self.parent(item)?;
        let children = self.children_mut(parent)?;
        let index = children.iter().position(|v| *v == item).unwrap();
        children.insert(index + 1, GroupItem::layer(id));
        Ok(())
    }
    /// Preserve membership and reject raster permutations that split subtrees.
    pub fn reordered(&self, order: &[u64]) -> Result<Self, String> {
        let positions: BTreeMap<_, _> = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        let mut tree = self.clone();
        fn arrange(
            tree: &mut GroupTree,
            parent: u64,
            positions: &BTreeMap<u64, usize>,
        ) -> Result<(), String> {
            let mut children = tree.children(parent)?.to_vec();
            for item in &children {
                if item.kind == ItemKind::Group {
                    arrange(tree, item.id, positions)?;
                }
            }
            let mut nonempty: Vec<_> = children
                .iter()
                .copied()
                .filter_map(|item| {
                    let leaves = tree.descendants(item).ok()?;
                    leaves
                        .first()
                        .and_then(|id| positions.get(id))
                        .map(|p| (*p, item))
                })
                .collect();
            nonempty.sort_by_key(|v| v.0);
            let mut next = nonempty.into_iter();
            for child in &mut children {
                if !tree.descendants(*child)?.is_empty() {
                    *child = next.next().unwrap().1;
                }
            }
            *tree.children_mut(parent)? = children;
            Ok(())
        }
        arrange(&mut tree, 0, &positions)?;
        if tree.layer_ids()? != order {
            return Err(
                "Raster move would split a group subtree; move or reparent a tree item instead"
                    .into(),
            );
        }
        Ok(tree)
    }
    pub fn effective_visibility(&self, ids: &[u64]) -> [bool; super::MAX_LAYERS] {
        fn visit(
            tree: &GroupTree,
            items: &[GroupItem],
            visible: bool,
            out: &mut BTreeMap<u64, bool>,
        ) {
            for item in items {
                match item.kind {
                    ItemKind::Layer => {
                        out.insert(item.id, visible);
                    }
                    ItemKind::Group => {
                        if let Ok(g) = tree.group(item.id) {
                            visit(tree, &g.children, visible && g.visible, out);
                        }
                    }
                }
            }
        }
        let mut map = BTreeMap::new();
        visit(self, &self.roots, true, &mut map);
        let mut out = [true; super::MAX_LAYERS];
        for (i, id) in ids.iter().enumerate() {
            out[i] = map.get(id).copied().unwrap_or(false);
        }
        out
    }
    /// Sources/filter identities remain owned by the existing linked graph.
    pub fn compile_graph(
        &self,
        source: &Graph,
        ids: &[u64],
        modes: &[BlendMode],
    ) -> Result<Graph, String> {
        self.compile_graph_details(source, ids, modes).map(|v| v.0)
    }
    pub fn compile_graph_details(
        &self,
        source: &Graph,
        ids: &[u64],
        modes: &[BlendMode],
    ) -> Result<(Graph, BTreeMap<u64, u32>), String> {
        if !source.linked {
            return Err("Groups require linked layer-filter composition".into());
        }
        let branches = source.stack_branches(ids.len())?;
        let mut graph = Graph {
            enabled: true,
            linked: false,
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        let mut next = source
            .nodes
            .iter()
            .map(|n| n.id as u64 + 1)
            .max()
            .unwrap_or(0);
        fn fresh(next: &mut u64, data: NodeKind, kind: &str) -> Result<Node, String> {
            let id = u32::try_from(*next).map_err(|_| "Node identity limit reached")?;
            *next += 1;
            Ok(Node {
                id,
                data,
                position: [0., 0.],
                node_type: Some(format!("compositing.{kind}")),
            })
        }
        let zero = fresh(&mut next, NodeKind::Transparent, "transparent")?;
        let zero_id = zero.id;
        graph.nodes.push(zero);
        let mut group_nodes = BTreeMap::new();
        fn build(
            tree: &GroupTree,
            items: &[GroupItem],
            source: &Graph,
            ids: &[u64],
            modes: &[BlendMode],
            branches: &[super::linked_graph::Branch],
            graph: &mut Graph,
            next: &mut u64,
            zero: u32,
            group_nodes: &mut BTreeMap<u64, u32>,
        ) -> Result<u32, String> {
            let mut last = zero;
            for item in items {
                let (head, mode, opacity) = match item.kind {
                    ItemKind::Layer => {
                        let index = ids
                            .iter()
                            .position(|id| *id == item.id)
                            .ok_or("Missing group raster")?;
                        let branch = &branches[index];
                        let mut head = branch.source.id;
                        graph.nodes.push(branch.source.clone());
                        if source.enabled {
                            for filter in &branch.filters {
                                graph.edges.push(Edge {
                                    from: head,
                                    to: filter.id,
                                    input: 0,
                                });
                                head = filter.id;
                                graph.nodes.push(filter.clone());
                            }
                        }
                        (head, modes[index], 1.)
                    }
                    ItemKind::Group => {
                        let g = tree.group(item.id)?;
                        (
                            build(
                                tree,
                                &g.children,
                                source,
                                ids,
                                modes,
                                branches,
                                graph,
                                next,
                                zero,
                                group_nodes,
                            )?,
                            g.blend_mode,
                            if g.visible {
                                g.opacity as f32 / 255.
                            } else {
                                0.
                            },
                        )
                    }
                };
                let blend = fresh(next, NodeKind::Blend { mode, opacity }, "blend")?;
                if item.kind == ItemKind::Group {
                    group_nodes.insert(item.id, blend.id);
                }
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
                last = blend.id;
                graph.nodes.push(blend);
            }
            Ok(last)
        }
        let head = build(
            self,
            &self.roots,
            source,
            ids,
            modes,
            &branches,
            &mut graph,
            &mut next,
            zero_id,
            &mut group_nodes,
        )?;
        let output = source
            .nodes
            .iter()
            .find(|n| matches!(n.data, NodeKind::Output))
            .ok_or("Missing output")?
            .clone();
        graph.edges.push(Edge {
            from: head,
            to: output.id,
            input: 0,
        });
        graph.nodes.push(output);
        let mut layout = BTreeMap::new();
        for node in &mut graph.nodes {
            let inputs: Vec<_> = graph
                .edges
                .iter()
                .filter(|e| e.to == node.id)
                .filter_map(|e| layout.get(&e.from).copied())
                .collect::<Vec<(usize, f32)>>();
            let column = inputs.iter().map(|v| v.0 + 1).max().unwrap_or(0);
            let row = match node.data {
                NodeKind::Layer { index } => index as f32 * 240.,
                NodeKind::Transparent => -160.,
                _ if !inputs.is_empty() => {
                    inputs.iter().map(|v| v.1).sum::<f32>() / inputs.len() as f32
                }
                _ => 0.,
            };
            node.position = [column as f32 * 260., row];
            layout.insert(node.id, (column, row));
        }
        graph.compile(ids.len())?;
        Ok((graph, group_nodes))
    }
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.roots.len() * std::mem::size_of::<GroupItem>()
            + self
                .groups
                .iter()
                .map(|g| {
                    std::mem::size_of::<Group>()
                        + g.name.len()
                        + g.children.len() * std::mem::size_of::<GroupItem>()
                })
                .sum::<usize>()
    }
}
