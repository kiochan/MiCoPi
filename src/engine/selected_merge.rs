//! Selected-only raster baking. Metadata projection never widens pixel targets.
use super::super::groups::{GroupItem, GroupTree, ItemKind};
use super::group_editing::GroupPlan;
use super::*;

#[derive(Clone, Debug, Serialize)]
pub struct SelectedMergePlan {
    pub layer_ids: Vec<u64>,
    pub common_parent_id: u64,
    pub parent_index: usize,
    pub layer_id: u64,
    pub consumed_hidden_ids: Vec<u64>,
    pub interleaved_unselected_ids: Vec<u64>,
    pub lifted_layer_ids: Vec<u64>,
    pub projected_group_ids: Vec<u64>,
    pub changes_stacking: bool,
    pub backdrop_policy: &'static str,
    pub width: usize,
    pub height: usize,
    pub output_buffer_bytes: usize,
    pub projected_nodes: usize,
    pub final_nodes: usize,
}

struct PreparedMerge {
    summary: SelectedMergePlan,
    consumed: Vec<usize>,
    insert: usize,
    metadata: Vec<LayerMetadata>,
    groups: Option<GroupTree>,
    graph: Graph,
    prepared: GroupPlan,
    projection: Graph,
}

fn ancestor_chain(tree: &GroupTree, layer: u64) -> Result<Vec<u64>, String> {
    let mut parent = tree.parent(GroupItem::layer(layer))?;
    let mut chain = vec![parent];
    while parent != 0 {
        parent = tree.parent(GroupItem::group(parent))?;
        chain.push(parent);
    }
    chain.reverse();
    Ok(chain)
}

fn project_items(
    tree: &GroupTree,
    items: &[GroupItem],
    selected: &[u64],
    projected: &mut Vec<super::super::groups::Group>,
) -> Result<Vec<GroupItem>, String> {
    let mut result = Vec::new();
    for item in items {
        match item.kind {
            ItemKind::Layer if selected.contains(&item.id) => result.push(*item),
            ItemKind::Layer => {}
            ItemKind::Group => {
                let mut group = tree.group(item.id)?.clone();
                group.children = project_items(tree, &group.children, selected, projected)?;
                if !group.children.is_empty() {
                    result.push(*item);
                    projected.push(group);
                }
            }
        }
    }
    Ok(result)
}

impl Document {
    /// Read-only semantic and allocation planning, including provisional contacts.
    /// No pixels are read/rendered or copied and no history/contact is completed.
    pub fn selected_merge_plan(&self, ids: &[u64]) -> Result<SelectedMergePlan, String> {
        Ok(self.prepare_selected_merge(ids)?.summary)
    }

    fn prepare_selected_merge(&self, ids: &[u64]) -> Result<PreparedMerge, String> {
        if ids.len() < 2
            || ids.len() > MAX_LAYERS
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| self.layer_index(*id).is_none() || ids[..i].contains(id))
        {
            return Err("Select between two and sixteen unique existing layer IDs to merge".into());
        }
        let mut source = if self.graph.linked {
            self.graph.clone()
        } else if !self.graph.enabled && self.graph.nodes.is_empty() && self.graph.edges.is_empty()
        {
            Graph::linked_from_layers(&self.layers)
        } else {
            return Err("Selected-layer merge requires linked layer composition; use Flatten Image to bake a free graph".into());
        };
        source.enabled = self.graph.enabled;
        source.compile(self.layers.len())?;
        let old_ids: Vec<_> = (0..self.layers.len()).map(|i| self.layer_id(i)).collect();
        let original = self
            .groups
            .clone()
            .unwrap_or_else(|| GroupTree::flat(&old_ids));
        if self.groups.is_some() {
            original.validate(&old_ids, self.next_group_id)?;
        }
        let consumed: Vec<_> = (0..self.layers.len())
            .filter(|i| ids.contains(&self.layer_id(*i)))
            .collect();
        if consumed.iter().any(|i| self.pixel_locked(*i)) {
            return Err("Unlock layer pixels before merging these layers".into());
        }
        if consumed.iter().any(|i| self.alpha_locked(*i)) {
            return Err("Unlock layer transparency before merging these layers".into());
        }
        self.ensure_graph_renderer()?;
        self.ensure_group_renderer()?;
        let selected: Vec<_> = consumed.iter().map(|i| self.layer_id(*i)).collect();
        let retained = self.layer_metadata[*consumed.last().unwrap()].clone();
        let mut common = ancestor_chain(&original, selected[0])?;
        for id in &selected[1..] {
            let chain = ancestor_chain(&original, *id)?;
            common.truncate(
                common
                    .iter()
                    .zip(chain)
                    .take_while(|(a, b)| **a == *b)
                    .count(),
            );
        }
        let parent = *common
            .last()
            .ok_or("Selected layers have no common parent")?;
        let mut highest_branch = GroupItem::layer(retained.id);
        while original.parent(highest_branch)? != parent {
            highest_branch = GroupItem::group(original.parent(highest_branch)?);
        }
        let siblings = original.children(parent)?;
        let branch_index = siblings
            .iter()
            .position(|item| *item == highest_branch)
            .ok_or("Selected merge insertion branch is missing")?;
        // All folders survive, even when their last selected leaf is removed.
        // A selected direct leaf contributes its old slot rather than a folder.
        let parent_index = siblings[..=branch_index]
            .iter()
            .filter(|item| item.kind == ItemKind::Group || !selected.contains(&item.id))
            .count();
        let highest_end = original
            .descendants(highest_branch)?
            .iter()
            .filter_map(|id| self.layer_index(*id))
            .max()
            .ok_or("Selected merge insertion branch has no raster")?;
        let interleaved_unselected_ids: Vec<_> = (*consumed.first().unwrap()..=highest_end)
            .filter(|i| !consumed.contains(i))
            .map(|i| self.layer_id(i))
            .collect();
        let lifted_layer_ids: Vec<_> = selected
            .iter()
            .copied()
            .filter(|id| {
                original
                    .parent(GroupItem::layer(*id))
                    .is_ok_and(|p| p != parent)
            })
            .collect();
        let consumed_hidden_ids = consumed
            .iter()
            .filter(|i| !self.layer_visible(**i))
            .map(|i| self.layer_id(*i))
            .collect();
        let mut projected_groups = Vec::new();
        let roots = project_items(&original, siblings, &selected, &mut projected_groups)?;
        projected_groups.sort_by_key(|group| {
            original
                .groups
                .iter()
                .position(|g| g.id == group.id)
                .unwrap()
        });
        let projected_group_ids = projected_groups.iter().map(|group| group.id).collect();
        let forest = GroupTree {
            version: original.version,
            roots,
            groups: projected_groups,
        };
        let source_modes: Vec<_> = self.layers.iter().map(|layer| layer.mode).collect();
        // Keep global raster indices and local visibility. The LCA and its
        // ancestors remain around the result, so their gates are excluded here.
        let projection = forest.compile_graph(&source, &old_ids, &source_modes)?;
        let projected_nodes = projection.compile(self.layers.len())?.len();
        let mut tree = original;
        for id in &selected {
            tree.remove_layer(*id);
        }
        tree.children_mut(parent)?
            .insert(parent_index, GroupItem::layer(retained.id));
        let new_ids = tree.layer_ids()?;
        if new_ids.len() != self.layers.len() - selected.len() + 1
            || new_ids.len() > MAX_LAYERS
            || self.width * self.height * new_ids.len() > 64 * 1024 * 1024
        {
            return Err("Selected merge exceeds the document pixel limit".into());
        }
        let insert = new_ids.iter().position(|id| *id == retained.id).unwrap();
        let metadata: Vec<_> = new_ids
            .iter()
            .map(|id| {
                if *id == retained.id {
                    LayerMetadata {
                        alpha_locked: false,
                        pixel_locked: false,
                        ..retained.clone()
                    }
                } else {
                    self.layer_metadata[self.layer_index(*id).unwrap()].clone()
                }
            })
            .collect();
        let modes: Vec<_> = new_ids
            .iter()
            .map(|id| {
                if *id == retained.id {
                    BlendMode::Normal
                } else {
                    self.layers[self.layer_index(*id).unwrap()].mode
                }
            })
            .collect();
        let graph = source.sync_baked_stack(&modes, &old_ids, &new_ids, None, &[retained.id])?;
        let groups = self.groups.as_ref().map(|_| tree);
        let prepared = self.prepare_group_plan(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        let summary = SelectedMergePlan {
            layer_ids: selected,
            common_parent_id: parent,
            parent_index,
            layer_id: retained.id,
            consumed_hidden_ids,
            changes_stacking: !interleaved_unselected_ids.is_empty()
                || !lifted_layer_ids.is_empty(),
            interleaved_unselected_ids,
            lifted_layer_ids,
            projected_group_ids,
            backdrop_policy: "selected_only_transparent",
            width: self.width,
            height: self.height,
            output_buffer_bytes: self.width * self.height * 4,
            projected_nodes,
            final_nodes: prepared.plan.len(),
        };
        Ok(PreparedMerge {
            summary,
            consumed,
            insert,
            metadata,
            groups,
            graph,
            prepared,
            projection,
        })
    }

    pub fn merge_selected(&mut self, ids: &[u64]) -> Result<u64, String> {
        let merge = self.prepare_selected_merge(ids)?;
        let pixels = self.render_group_bake(&merge.projection)?;
        let layer_id = merge.summary.layer_id;
        self.publish_group_bake(
            merge.metadata,
            merge.groups,
            merge.graph,
            merge.prepared,
            &merge.consumed,
            merge.insert,
            Layer {
                pixels,
                visible: true,
                opacity: 255,
                mode: BlendMode::Normal,
            },
            self.next_layer_id,
        );
        Ok(layer_id)
    }
}
