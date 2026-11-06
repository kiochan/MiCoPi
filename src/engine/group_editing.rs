//! Group metadata, effective composition caches and structural buffer ownership.
use super::super::groups::{Group, GroupItem, GroupTree, ItemKind, MAX_GROUPS};
use super::*;
use serde_json::{json, Value};

pub(super) struct GroupPlan {
    pub plan: Vec<micopi_plugin_api::CompositeNode>,
    pub graph: Option<Graph>,
    pub visible: [bool; MAX_LAYERS],
    pub nodes: BTreeMap<u64, u32>,
}
impl Document {
    pub fn has_groups(&self) -> bool {
        self.groups.as_ref().is_some_and(|t| !t.groups.is_empty())
    }
    pub fn groups_revision(&self) -> u64 {
        self.groups_revision
    }
    pub fn layer_parent_id(&self, index: usize) -> u64 {
        self.groups
            .as_ref()
            .and_then(|t| t.parent(GroupItem::layer(self.layer_id(index))).ok())
            .unwrap_or(0)
    }
    pub fn layer_visible(&self, index: usize) -> bool {
        self.layers[index].visible && self.group_visibility[index]
    }
    pub fn ensure_group_renderer(&self) -> Result<(), String> {
        if self
            .renderer
            .as_ref()
            .is_none_or(|r| !r.supports_transparent())
        {
            return Err("Groups require a compositor DLL with transparent-source support".into());
        }
        Ok(())
    }
    pub fn ensure_graph_renderer(&self) -> Result<(), String> {
        self.ensure_blend_modes(
            &self.graph,
            self.groups.as_ref(),
            &self.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
        )?;
        if self.has_groups()
            || self
                .graph
                .nodes
                .iter()
                .any(|n| matches!(n.data, NodeKind::Transparent))
        {
            self.ensure_group_renderer()?;
        }
        if self.requires_renderer() && self.renderer.is_none() {
            return Err("Compositor DLL is not loaded".into());
        }
        Ok(())
    }
    pub fn effective_graph(&self) -> Result<Graph, String> {
        Ok(self
            .effective_graph
            .clone()
            .unwrap_or_else(|| self.graph.clone()))
    }
    pub fn group_layer_ids(&self, id: u64) -> Result<Vec<u64>, String> {
        self.groups
            .as_ref()
            .ok_or("Group does not exist")?
            .descendants(GroupItem::group(id))
    }
    pub fn preflight_new_layer(&self, parent: Option<u64>) -> Result<(), String> {
        if self.layers.len() >= MAX_LAYERS
            || self.width * self.height * (self.layers.len() + 1) > 64 * 1024 * 1024
            || self.next_layer_id > u32::MAX as u64
        {
            return Err("Layer count, pixel or identity limit reached".into());
        }
        let mut groups = self.groups.clone();
        if let Some(tree) = &mut groups {
            if let Some(parent) = parent {
                tree.children_mut(parent)?
                    .push(GroupItem::layer(self.next_layer_id));
            } else {
                tree.insert_layer_after(self.layer_id(self.active), self.next_layer_id)?;
            }
        } else if parent.is_some_and(|v| v != 0) {
            return Err("Group does not exist".into());
        }
        let index = groups
            .as_ref()
            .map(|t| t.layer_ids())
            .transpose()?
            .map(|ids| ids.iter().position(|id| *id == self.next_layer_id).unwrap())
            .unwrap_or(self.layers.len());
        let mut metadata = self.layer_metadata.clone();
        metadata.insert(
            index,
            LayerMetadata {
                id: self.next_layer_id,
                ..Default::default()
            },
        );
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| {
                self.layer_index(m.id)
                    .map_or(BlendMode::Normal, |i| self.layers[i].mode)
            })
            .collect();
        let graph = self.reindexed_graph(&metadata, None)?;
        self.prepare_group_plan(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        Ok(())
    }
    pub(super) fn prepare_group_plan(
        &self,
        graph: &Graph,
        groups: Option<&GroupTree>,
        metadata: &[LayerMetadata],
        modes: &[BlendMode],
        next: u64,
        require: bool,
    ) -> Result<GroupPlan, String> {
        if require {
            self.ensure_blend_modes(graph, groups, modes)?;
        }
        let ids: Vec<_> = metadata.iter().map(|m| m.id).collect();
        if let Some(tree) = groups {
            if require {
                self.ensure_group_renderer()?;
            }
            tree.validate(&ids, next)?;
            let (effective, nodes) = tree.compile_graph_details(graph, &ids, modes)?;
            Ok(GroupPlan {
                plan: effective.compile(ids.len())?,
                graph: Some(effective),
                visible: tree.effective_visibility(&ids),
                nodes,
            })
        } else {
            Ok(GroupPlan {
                plan: graph.compile(ids.len())?,
                graph: None,
                visible: [true; MAX_LAYERS],
                nodes: BTreeMap::new(),
            })
        }
    }
    pub(super) fn install_group_plan(&mut self, prepared: GroupPlan) {
        self.plan = prepared.plan;
        self.effective_graph = prepared.graph;
        self.group_visibility = prepared.visible;
        self.group_nodes = prepared.nodes;
        self.groups_revision = self.groups_revision.wrapping_add(1);
    }
    pub(super) fn refresh_group_cache(&mut self) -> Result<(), String> {
        let modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        let prepared = self.prepare_group_plan(
            &self.graph,
            self.groups.as_ref(),
            &self.layer_metadata,
            &modes,
            self.next_group_id,
            false,
        )?;
        self.install_group_plan(prepared);
        Ok(())
    }
    pub(super) fn inverse_order(order: &[usize]) -> Vec<usize> {
        let mut inverse = vec![0; order.len()];
        for (i, old) in order.iter().enumerate() {
            inverse[*old] = i;
        }
        inverse
    }
    pub(super) fn permute_buffers(&mut self, order: &[usize]) {
        let mut old: Vec<_> = std::mem::take(&mut self.layers)
            .into_iter()
            .map(Some)
            .collect();
        self.layers = order.iter().map(|i| old[*i].take().unwrap()).collect();
    }
    fn tree(&self) -> GroupTree {
        self.groups.clone().unwrap_or_else(|| {
            GroupTree::flat(&self.layer_metadata.iter().map(|m| m.id).collect::<Vec<_>>())
        })
    }
    fn commit_tree(&mut self, tree: GroupTree, next: u64) -> Result<(), String> {
        self.ensure_group_renderer()?;
        let ids = tree.layer_ids()?;
        let order: Vec<_> = ids
            .iter()
            .map(|id| {
                self.layer_index(*id)
                    .ok_or("Tree references a missing raster")
            })
            .collect::<Result<_, _>>()?;
        if order.len() != self.layers.len() {
            return Err("Tree must retain every raster".into());
        }
        let metadata: Vec<_> = order
            .iter()
            .map(|i| self.layer_metadata[*i].clone())
            .collect();
        let modes: Vec<_> = order.iter().map(|i| self.layers[*i].mode).collect();
        let groups = (!tree.groups.is_empty()).then_some(tree);
        let graph = self.reindexed_graph(&metadata, None)?;
        let prepared =
            self.prepare_group_plan(&graph, groups.as_ref(), &metadata, &modes, next, true)?;
        if self.groups == groups && order.iter().enumerate().all(|(i, j)| i == *j) {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        let active = self.active;
        let active_id = self.layer_id(active);
        let change = if order.iter().enumerate().any(|(i, j)| i != *j) {
            let inverse = Self::inverse_order(&order);
            self.permute_buffers(&order);
            Some(LayerChange::Permutation {
                order: inverse,
                active,
            })
        } else {
            None
        };
        self.layer_metadata = metadata;
        self.groups = groups;
        self.next_group_id = next;
        self.active = self.layer_index(active_id).unwrap();
        self.normalize_layer_selection();
        self.graph = graph;
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_composition(before, change);
        Ok(())
    }
    pub fn group_create(
        &mut self,
        members: &[GroupItem],
        name: Option<String>,
        parent: Option<u64>,
    ) -> Result<u64, String> {
        self.ensure_group_renderer()?;
        if !self.graph.linked {
            return Err("Groups require linked layer-filter composition".into());
        }
        let mut tree = self.tree();
        if tree.groups.len() >= MAX_GROUPS || self.next_group_id > u32::MAX as u64 {
            return Err("Group count or identity limit reached".into());
        }
        let parent = parent.unwrap_or(if let Some(item) = members.first() {
            tree.parent(*item)?
        } else {
            0
        });
        let siblings = tree.children(parent)?.to_vec();
        if members
            .iter()
            .enumerate()
            .any(|(i, item)| members[..i].contains(item) || !siblings.contains(item))
        {
            return Err("Group members must be unique siblings".into());
        }
        let children: Vec<_> = siblings
            .iter()
            .copied()
            .filter(|v| members.contains(v))
            .collect();
        let position = siblings
            .iter()
            .position(|v| members.contains(v))
            .unwrap_or(siblings.len());
        let id = self.next_group_id;
        tree.children_mut(parent)?.retain(|v| !members.contains(v));
        tree.children_mut(parent)?
            .insert(position, GroupItem::group(id));
        tree.groups.push(Group {
            id,
            name: name.unwrap_or_else(|| format!("Group {id:02}")),
            visible: true,
            opacity: 255,
            blend_mode: BlendMode::Normal,
            children,
        });
        self.commit_tree(tree, id + 1)?;
        Ok(id)
    }
    pub fn group_set(
        &mut self,
        id: u64,
        name: Option<String>,
        visible: Option<bool>,
        opacity: Option<u8>,
        mode: Option<BlendMode>,
    ) -> Result<(), String> {
        let mut tree = self.tree();
        let group = tree.group_mut(id)?;
        if let Some(v) = name {
            group.name = v;
        }
        if let Some(v) = visible {
            group.visible = v;
        }
        if let Some(v) = opacity {
            group.opacity = v;
        }
        if let Some(v) = mode {
            group.blend_mode = v;
        }
        self.commit_tree(tree, self.next_group_id)
    }
    pub fn group_move(
        &mut self,
        items: &[GroupItem],
        parent: u64,
        index: usize,
    ) -> Result<(), String> {
        if items.is_empty() {
            return Err("Move at least one tree item".into());
        }
        let mut tree = self.tree();
        tree.children(parent)?;
        for (i, item) in items.iter().enumerate() {
            tree.parent(*item)?;
            if items[..i].contains(item) {
                return Err("Duplicate move item".into());
            }
            if item.kind == ItemKind::Group {
                let mut p = parent;
                while p != 0 {
                    if p == item.id {
                        return Err("A group cannot contain itself".into());
                    }
                    p = tree.parent(GroupItem::group(p))?;
                }
                for other in items {
                    if other != item {
                        let mut p = tree.parent(*other)?;
                        while p != 0 {
                            if p == item.id {
                                return Err(
                                    "Do not move an ancestor and its descendant together".into()
                                );
                            }
                            p = tree.parent(GroupItem::group(p))?;
                        }
                    }
                }
            }
        }
        fn preorder(
            tree: &GroupTree,
            values: &[GroupItem],
            wanted: &[GroupItem],
            out: &mut Vec<GroupItem>,
        ) -> Result<(), String> {
            for item in values {
                if wanted.contains(item) {
                    out.push(*item);
                }
                if item.kind == ItemKind::Group {
                    preorder(tree, &tree.group(item.id)?.children, wanted, out)?;
                }
            }
            Ok(())
        }
        let mut ordered = Vec::new();
        preorder(&tree, &tree.roots, items, &mut ordered)?;
        tree.roots.retain(|v| !items.contains(v));
        for group in &mut tree.groups {
            group.children.retain(|v| !items.contains(v));
        }
        let children = tree.children_mut(parent)?;
        if index > children.len() {
            return Err("Group destination exceeds the remaining children".into());
        }
        children.splice(index..index, ordered);
        self.commit_tree(tree, self.next_group_id)
    }
    pub fn group_ungroup(&mut self, id: u64) -> Result<(), String> {
        let mut tree = self.tree();
        let parent = tree.parent(GroupItem::group(id))?;
        let children = tree.group(id)?.children.clone();
        let siblings = tree.children_mut(parent)?;
        let index = siblings
            .iter()
            .position(|v| *v == GroupItem::group(id))
            .unwrap();
        siblings.splice(index..=index, children);
        tree.groups.retain(|g| g.id != id);
        self.commit_tree(tree, self.next_group_id)
    }
    pub fn group_delete(&mut self, id: u64) -> Result<(), String> {
        self.ensure_group_renderer()?;
        let mut tree = self.tree();
        let removed_ids = tree.descendants(GroupItem::group(id))?;
        if removed_ids.len() == self.layers.len() {
            return Err("The final raster cannot be deleted".into());
        }
        let parent = tree.parent(GroupItem::group(id))?;
        tree.children_mut(parent)?
            .retain(|item| *item != GroupItem::group(id));
        let mut descendants = vec![id];
        let mut offset = 0;
        while offset < descendants.len() {
            let children = tree.group(descendants[offset])?.children.clone();
            descendants.extend(
                children
                    .iter()
                    .filter(|i| i.kind == ItemKind::Group)
                    .map(|i| i.id),
            );
            offset += 1;
        }
        tree.groups.retain(|g| !descendants.contains(&g.id));
        let groups = (!tree.groups.is_empty()).then_some(tree);
        let metadata: Vec<_> = self
            .layer_metadata
            .iter()
            .filter(|m| !removed_ids.contains(&m.id))
            .cloned()
            .collect();
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].mode)
            .collect();
        let graph = self.reindexed_graph(&metadata, None)?;
        let prepared = self.prepare_group_plan(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let active_id = self.layer_id(active);
        let selected = self.selected_layer_ids();
        let next = selected
            .iter()
            .filter_map(|id| self.layer_index(*id).filter(|_| !removed_ids.contains(id)))
            .min_by_key(|i| (i.abs_diff(active), *i))
            .or_else(|| {
                (active..self.layers.len()).find(|i| !removed_ids.contains(&self.layer_id(*i)))
            })
            .or_else(|| {
                (0..active)
                    .rev()
                    .find(|i| !removed_ids.contains(&self.layer_id(*i)))
            })
            .map(|i| self.layer_id(i))
            .unwrap();
        let mut saved = Vec::new();
        for i in (0..self.layers.len()).rev() {
            if removed_ids.contains(&self.layer_id(i)) {
                saved.push((i, Arc::new(self.layers.remove(i))));
            }
        }
        saved.reverse();
        self.layer_metadata = metadata;
        self.groups = groups;
        self.graph = graph;
        self.active = self
            .layer_index(active_id)
            .or_else(|| self.layer_index(next))
            .unwrap();
        self.normalize_layer_selection();
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Replace {
                current: vec![],
                saved,
                active,
            }),
        );
        Ok(())
    }
    pub fn group_duplicate(&mut self, id: u64) -> Result<u64, String> {
        self.ensure_group_renderer()?;
        let mut tree = self.tree();
        let source = tree.clone();
        let parent = tree.parent(GroupItem::group(id))?;
        let layer_ids = tree.descendants(GroupItem::group(id))?;
        let mut group_ids = vec![id];
        let mut offset = 0;
        while offset < group_ids.len() {
            group_ids.extend(
                source
                    .group(group_ids[offset])?
                    .children
                    .iter()
                    .filter(|i| i.kind == ItemKind::Group)
                    .map(|i| i.id),
            );
            offset += 1;
        }
        if self.layers.len() + layer_ids.len() > MAX_LAYERS
            || self.width * self.height * (self.layers.len() + layer_ids.len()) > 64 * 1024 * 1024
            || tree.groups.len() + group_ids.len() > MAX_GROUPS
            || self.next_group_id + group_ids.len() as u64 > u32::MAX as u64 + 1
            || self.next_layer_id + layer_ids.len() as u64 > u32::MAX as u64 + 1
        {
            return Err("Group copy exceeds layer, group, pixel or identity limits".into());
        }
        let layer_map: Vec<_> = layer_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, self.next_layer_id + i as u64))
            .collect();
        let group_map: Vec<_> = group_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, self.next_group_id + i as u64))
            .collect();
        let copy = self.next_group_id;
        for (old, new) in &group_map {
            let mut group = source.group(*old)?.clone();
            group.id = *new;
            for item in &mut group.children {
                item.id = match item.kind {
                    ItemKind::Layer => {
                        layer_map
                            .iter()
                            .find(|(from, _)| *from == item.id)
                            .unwrap()
                            .1
                    }
                    ItemKind::Group => {
                        group_map
                            .iter()
                            .find(|(from, _)| *from == item.id)
                            .unwrap()
                            .1
                    }
                };
            }
            tree.groups.push(group);
        }
        let siblings = tree.children_mut(parent)?;
        let position = siblings
            .iter()
            .position(|i| *i == GroupItem::group(id))
            .unwrap();
        siblings.insert(position + 1, GroupItem::group(copy));
        let ids = tree.layer_ids()?;
        let metadata: Vec<_> = ids
            .iter()
            .map(|id| {
                let source = layer_map
                    .iter()
                    .find(|(_, to)| to == id)
                    .map_or(*id, |(from, _)| *from);
                let mut m = self.layer_metadata[self.layer_index(source).unwrap()].clone();
                m.id = *id;
                m
            })
            .collect();
        let modes: Vec<_> = ids
            .iter()
            .map(|id| {
                let source = layer_map
                    .iter()
                    .find(|(_, to)| to == id)
                    .map_or(*id, |(from, _)| *from);
                self.layers[self.layer_index(source).unwrap()].mode
            })
            .collect();
        let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let graph = self
            .graph
            .sync_stack_mapped(&modes, &old_ids, &ids, &layer_map, &[])?;
        let next_group = self.next_group_id + group_map.len() as u64;
        let prepared =
            self.prepare_group_plan(&graph, Some(&tree), &metadata, &modes, next_group, true)?;
        // Only intentional duplicated rasters allocate image buffers, after every preflight.
        let mut copies: Vec<_> = layer_map
            .iter()
            .map(|(from, to)| (*to, self.layers[self.layer_index(*from).unwrap()].clone()))
            .collect();
        self.finish();
        let before = self.composition();
        let active = self.active;
        let active_id = self.layer_id(active);
        let mut current = Vec::new();
        for (index, id) in ids.iter().enumerate() {
            if let Some(position) = copies.iter().position(|(to, _)| to == id) {
                let (_, layer) = copies.remove(position);
                self.layers.insert(index, layer);
                current.push(index);
            }
        }
        self.layer_metadata = metadata;
        self.next_layer_id += layer_map.len() as u64;
        self.next_group_id = next_group;
        self.groups = Some(tree);
        self.graph = graph;
        self.active = self.layer_index(active_id).unwrap();
        self.normalize_layer_selection();
        if !layer_map.is_empty() {
            let copies: Vec<_> = layer_map.iter().map(|(_, to)| *to).collect();
            let copy_active = layer_map
                .iter()
                .find(|(from, _)| *from == active_id)
                .map(|(_, to)| *to)
                .unwrap_or(*copies.last().unwrap());
            self.set_layer_selection(&copies, Some(copy_active))?;
        }
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Replace {
                current,
                saved: vec![],
                active,
            }),
        );
        Ok(copy)
    }
    pub fn reorder_layers(&mut self, ids: &[u64]) -> Result<(), String> {
        if ids.len() != self.layers.len()
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| ids[..i].contains(id) || self.layer_index(*id).is_none())
        {
            return Err("Invalid raster permutation".into());
        }
        let tree = self.tree().reordered(ids)?;
        if self.has_groups() {
            self.commit_tree(tree, self.next_group_id)
        } else {
            let metadata: Vec<_> = ids
                .iter()
                .map(|id| self.layer_metadata[self.layer_index(*id).unwrap()].clone())
                .collect();
            let graph = self.reindexed_graph(&metadata, None)?;
            let plan = graph.compile(ids.len())?;
            let order: Vec<_> = ids
                .iter()
                .map(|id| self.layer_index(*id).unwrap())
                .collect();
            if order.iter().enumerate().all(|(i, j)| i == *j) {
                return Ok(());
            }
            self.finish();
            let before = self.composition();
            let active = self.active;
            let active_id = self.layer_id(active);
            let inverse = Self::inverse_order(&order);
            self.permute_buffers(&order);
            self.layer_metadata = metadata;
            self.active = self.layer_index(active_id).unwrap();
            self.normalize_layer_selection();
            self.graph = graph;
            self.plan = plan;
            self.track_layer_order();
            self.commit_composition(
                before,
                Some(LayerChange::Permutation {
                    order: inverse,
                    active,
                }),
            );
            Ok(())
        }
    }
    pub fn preflight_layer_order(&self, ids: &[u64]) -> Result<(), String> {
        if ids.len() != self.layers.len()
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| ids[..i].contains(id) || self.layer_index(*id).is_none())
        {
            return Err("Invalid raster permutation".into());
        }
        if self.has_groups() {
            let tree = self.tree().reordered(ids)?;
            let metadata: Vec<_> = ids
                .iter()
                .map(|id| self.layer_metadata[self.layer_index(*id).unwrap()].clone())
                .collect();
            let modes: Vec<_> = ids
                .iter()
                .map(|id| self.layers[self.layer_index(*id).unwrap()].mode)
                .collect();
            let graph = self.reindexed_graph(&metadata, None)?;
            self.prepare_group_plan(
                &graph,
                Some(&tree),
                &metadata,
                &modes,
                self.next_group_id,
                true,
            )?;
        }
        Ok(())
    }
    pub fn groups_state(&self) -> Value {
        let tree = self.tree();
        let mut groups = Vec::new();
        let mut rows = Vec::new();
        fn walk(
            doc: &Document,
            tree: &GroupTree,
            items: &[GroupItem],
            parent: u64,
            depth: usize,
            visible: bool,
            groups: &mut Vec<Value>,
            rows: &mut Vec<Value>,
        ) {
            for item in items.iter().rev() {
                match item.kind {
                    ItemKind::Layer => {
                        if let Some(index) = doc.layer_index(item.id) {
                            rows.push(json!({"kind":"layer","id":item.id,"index":index,"depth":depth,"parent_id":parent,"name":doc.layer_name(index),"visible":doc.layers[index].visible,"effective_visible":doc.layer_visible(index)}));
                        }
                    }
                    ItemKind::Group => {
                        if let Ok(g) = tree.group(item.id) {
                            let row = json!({"kind":"group","id":g.id,"name":g.name,"parent_id":parent,"depth":depth,"visible":g.visible,"effective_visible":visible&&g.visible,"opacity":g.opacity as f32/255.,"blend_mode":g.blend_mode,"children":g.children,"layer_ids":tree.descendants(*item).unwrap_or_default(),"node_id":doc.group_nodes.get(&g.id)});
                            groups.push(row.clone());
                            rows.push(row);
                            walk(
                                doc,
                                tree,
                                &g.children,
                                g.id,
                                depth + 1,
                                visible && g.visible,
                                groups,
                                rows,
                            );
                        }
                    }
                }
            }
        }
        walk(self, &tree, &tree.roots, 0, 0, true, &mut groups, &mut rows);
        json!({"version":1,"roots":tree.roots,"groups":groups,"rows":rows})
    }
}
