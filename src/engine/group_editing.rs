//! Group metadata, effective composition caches and structural buffer ownership.
use super::super::groups::{Group, GroupItem, GroupPolicy, GroupTree, ItemKind, MAX_GROUPS};
use super::*;
use serde_json::{json, Value};

pub(super) struct GroupPlan {
    pub plan: Vec<micopi_plugin_api::CompositeNode>,
    pub graph: Option<Graph>,
    pub visible: [bool; MAX_LAYERS],
    pub nodes: BTreeMap<u64, u32>,
    pub adjustment_nodes: BTreeMap<u64, u32>,
    pub masked_layers: [bool; MAX_LAYERS],
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
        if self.has_raster_masks() || self.has_adjustments() {
            self.validate_mask_storage()?;
            // Restored metadata can be inspected without a DLL, but editing or
            // publishing a project must validate the complete generated plan.
            self.prepare_raster_mask_plan(self.mask_state(), true)?;
        }
        self.ensure_blend_modes(
            &self.graph,
            self.groups.as_ref(),
            &self.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
        )?;
        self.ensure_mix_capability(&self.graph, self.groups.as_ref())?;
        if self.has_groups()
            || self.has_adjustments()
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
        project_layer_links(&self.layer_metadata, &mut metadata)?;
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
        let opacities: Vec<_> = metadata
            .iter()
            .map(|m| {
                self.layer_index(m.id)
                    .map_or(255, |i| self.layers[i].opacity)
            })
            .collect();
        self.prepare_group_plan_with_masks(
            graph,
            groups,
            metadata,
            modes,
            &opacities,
            next,
            self.mask_state(),
            require,
        )
    }
    pub(super) fn prepare_group_plan_with_masks(
        &self,
        graph: &Graph,
        groups: Option<&GroupTree>,
        metadata: &[LayerMetadata],
        modes: &[BlendMode],
        opacities: &[u8],
        next: u64,
        masks: &super::super::raster_masks::MaskState,
        require: bool,
    ) -> Result<GroupPlan, String> {
        validate_layer_links(metadata)?;
        if require {
            self.ensure_blend_modes(graph, groups, modes)?;
            self.ensure_mix_capability(graph, groups)?;
        }
        let ids: Vec<_> = metadata.iter().map(|m| m.id).collect();
        if ids.is_empty()
            || ids.len() > MAX_LAYERS
            || modes.len() != ids.len()
            || opacities.len() != ids.len()
        {
            return Err("Layer composition metadata does not match raster identities".into());
        }
        let group_ids: Vec<_> =
            groups.map_or_else(Vec::new, |tree| tree.groups.iter().map(|g| g.id).collect());
        let adjustment_ids: Vec<_> = groups.map_or_else(Vec::new, |tree| {
            tree.adjustments.iter().map(|a| a.id).collect()
        });
        masks.validate_with_adjustments(
            self.width,
            self.height,
            &ids,
            &group_ids,
            &adjustment_ids,
        )?;
        let mask_ids: Vec<_> = masks.items.iter().map(|m| m.id).collect();
        let mut masked_layers = [false; MAX_LAYERS];
        if !masks.items.is_empty() {
            if !graph.linked {
                return Err("Raster masks require linked layer-filter composition".into());
            }
            if require {
                self.ensure_raster_mask_renderer()?;
                self.ensure_raster_mask_properties(masks)?;
            }
            for (i, id) in ids.iter().enumerate() {
                masked_layers[i] = masks.items.iter().any(|m| {
                    m.enabled && m.owner == super::super::raster_masks::MaskOwner::Layer { id: *id }
                });
            }
        }
        // A retained v3 allocator floor has no composition semantics. Preserve
        // the old flat fast path and its complete source-node budget.
        if let Some(tree) = groups {
            tree.validate(&ids, next)?;
        }
        if let Some(tree) =
            groups.filter(|tree| !tree.groups.is_empty() || !tree.adjustments.is_empty())
        {
            if require && (!tree.groups.is_empty() || !tree.adjustments.is_empty()) {
                self.ensure_group_renderer()?;
            }
            tree.validate(&ids, next)?;
            // Disabled/hidden definitions retain full parameter/capability and
            // generated instruction-budget validation without executing them.
            if !tree.adjustments.is_empty() {
                tree.compile_stack_details(graph, &ids, modes, opacities, masks, true)?;
            }
            let (effective, nodes, adjustment_nodes) =
                tree.compile_stack_details(graph, &ids, modes, opacities, masks, false)?;
            Ok(GroupPlan {
                plan: effective.compile_with_masks(ids.len(), &mask_ids)?,
                graph: Some(effective),
                visible: tree.effective_visibility(&ids),
                nodes,
                adjustment_nodes,
                masked_layers,
            })
        } else if !masks.items.is_empty() {
            let effective =
                super::mask_composition::masked_flat_graph(graph, &ids, modes, opacities, masks)?;
            Ok(GroupPlan {
                plan: effective.compile_with_masks(ids.len(), &mask_ids)?,
                graph: Some(effective),
                visible: [true; MAX_LAYERS],
                nodes: BTreeMap::new(),
                adjustment_nodes: BTreeMap::new(),
                masked_layers,
            })
        } else {
            Ok(GroupPlan {
                plan: graph.compile(ids.len())?,
                graph: None,
                visible: [true; MAX_LAYERS],
                nodes: BTreeMap::new(),
                adjustment_nodes: BTreeMap::new(),
                masked_layers,
            })
        }
    }
    pub(super) fn install_group_plan(&mut self, prepared: GroupPlan) {
        self.plan = prepared.plan;
        self.effective_graph = prepared.graph;
        self.group_visibility = prepared.visible;
        self.group_nodes = prepared.nodes;
        self.adjustment_nodes = prepared.adjustment_nodes;
        self.masked_layers = prepared.masked_layers;
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
    pub(super) fn tree(&self) -> GroupTree {
        self.groups.clone().unwrap_or_else(|| {
            let mut tree =
                GroupTree::flat(&self.layer_metadata.iter().map(|m| m.id).collect::<Vec<_>>());
            if self.next_adjustment_id != 0 {
                tree.version = 3;
                tree.next_adjustment_id = self.next_adjustment_id;
            }
            tree
        })
    }
    fn validate_group_owner_edit(&self) -> Result<(), String> {
        if self.stroke.is_some() {
            return Err("Finish the current contact before editing groups".into());
        }
        self.validate_mask_storage()?;
        self.ensure_graph_renderer()
    }
    pub(super) fn commit_tree(&mut self, tree: GroupTree, next: u64) -> Result<(), String> {
        if !tree.groups.is_empty() || !tree.adjustments.is_empty() {
            self.ensure_group_renderer()?;
        }
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
        let mut metadata: Vec<_> = order
            .iter()
            .map(|i| self.layer_metadata[*i].clone())
            .collect();
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let modes: Vec<_> = order.iter().map(|i| self.layers[*i].mode).collect();
        let opacities: Vec<_> = order.iter().map(|i| self.layers[*i].opacity).collect();
        let groups = tree.retain_tree().then_some(tree);
        let graph = self.reindexed_graph(&metadata, None)?;
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            &opacities,
            next,
            self.mask_state(),
            true,
        )?;
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
        self.validate_group_owner_edit()?;
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
            compositing: GroupPolicy::Isolated,
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
        compositing: Option<GroupPolicy>,
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
        if let Some(v) = compositing {
            group.compositing = v;
            if v == GroupPolicy::PassThrough {
                tree.version = tree.version.max(2);
            }
        }
        self.commit_tree(tree, self.next_group_id)
    }
    pub fn group_move(
        &mut self,
        items: &[GroupItem],
        parent: u64,
        index: usize,
    ) -> Result<(), String> {
        self.validate_group_owner_edit()?;
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
        self.require_adjustment_aware_edit()?;
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
        self.validate_group_owner_edit()?;
        self.ensure_group_renderer()?;
        let mut tree = self.tree();
        let removed_ids = tree.descendants(GroupItem::group(id))?;
        let removed_adjustments = tree.adjustment_ids(GroupItem::group(id))?;
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
        tree.adjustments
            .retain(|a| !removed_adjustments.contains(&a.id));
        let groups = tree.retain_tree().then_some(tree);
        let mut metadata: Vec<_> = self
            .layer_metadata
            .iter()
            .filter(|m| !removed_ids.contains(&m.id))
            .cloned()
            .collect();
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].mode)
            .collect();
        let opacities: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].opacity)
            .collect();
        let mask_removal = self.plan_group_mask_removal_with_adjustments(
            &removed_ids,
            &descendants,
            &removed_adjustments,
        );
        let graph = self.reindexed_graph(&metadata, None)?;
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            &opacities,
            self.next_group_id,
            &mask_removal.state,
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
        let mut saved = Vec::with_capacity(removed_ids.len());
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
        let mask_change = self.publish_group_mask_removal(mask_removal);
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_owner_composition(
            before,
            Some(LayerChange::Replace {
                current: vec![],
                saved,
                active,
            }),
            mask_change,
        );
        Ok(())
    }
    pub fn group_duplicate(&mut self, id: u64) -> Result<u64, String> {
        self.validate_group_owner_edit()?;
        self.ensure_group_renderer()?;
        let mut tree = self.tree();
        let source = tree.clone();
        let parent = tree.parent(GroupItem::group(id))?;
        let layer_ids = tree.descendants(GroupItem::group(id))?;
        let descendant_adjustments = tree.adjustment_ids(GroupItem::group(id))?;
        let adjustment_ids: Vec<_> = source
            .adjustments
            .iter()
            .filter(|a| descendant_adjustments.contains(&a.id))
            .map(|a| a.id)
            .collect();
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
        let next_group = self.next_group_id.checked_add(group_ids.len() as u64);
        let next_layer = self.next_layer_id.checked_add(layer_ids.len() as u64);
        let next_adjustment = self
            .next_adjustment_id
            .checked_add(adjustment_ids.len() as u64);
        if self.layers.len() + layer_ids.len() > MAX_LAYERS
            || self.width * self.height * (self.layers.len() + layer_ids.len()) > 64 * 1024 * 1024
            || tree.groups.len() + group_ids.len() > MAX_GROUPS
            || self.next_group_id == 0
            || self.next_layer_id == 0
            || next_group.is_none_or(|next| next > u32::MAX as u64 + 1)
            || next_layer.is_none_or(|next| next > u32::MAX as u64 + 1)
            || tree.adjustments.len() + adjustment_ids.len()
                > crate::engine::adjustments::MAX_ADJUSTMENTS
            || next_adjustment.is_none_or(|next| next > u32::MAX as u64 + 1)
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
        let adjustment_map: Vec<_> = adjustment_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, self.next_adjustment_id + i as u64))
            .collect();
        for (from, to) in &adjustment_map {
            let mut adjustment = source.adjustment(*from)?.clone();
            adjustment.id = *to;
            tree.adjustments.push(adjustment);
        }
        if !adjustment_map.is_empty() {
            tree.next_adjustment_id = next_adjustment.unwrap();
        }
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
                    ItemKind::Adjustment => {
                        adjustment_map
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
        let mut metadata: Vec<_> = ids
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
        project_layer_links(&self.layer_metadata, &mut metadata)?;
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
        let opacities: Vec<_> = ids
            .iter()
            .map(|id| {
                let source = layer_map
                    .iter()
                    .find(|(_, to)| to == id)
                    .map_or(*id, |(from, _)| *from);
                self.layers[self.layer_index(source).unwrap()].opacity
            })
            .collect();
        let mask_copy =
            self.plan_group_mask_copy_with_adjustments(&layer_map, &group_map, &adjustment_map)?;
        let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let graph = self
            .graph
            .sync_stack_mapped(&modes, &old_ids, &ids, &layer_map, &[])?;
        let next_group = next_group.unwrap();
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            Some(&tree),
            &metadata,
            &modes,
            &opacities,
            next_group,
            &mask_copy.state,
            true,
        )?;
        let active = self.active;
        let active_id = self.layer_id(active);
        let selected = if layer_map.is_empty() {
            self.selected_layer_ids()
        } else {
            layer_map.iter().map(|(_, to)| *to).collect()
        };
        let selected_active = layer_map
            .iter()
            .find(|(from, _)| *from == active_id)
            .map(|(_, to)| *to)
            .unwrap_or_else(|| {
                if layer_map.is_empty() {
                    active_id
                } else {
                    *selected.last().unwrap()
                }
            });
        if selected.is_empty()
            || selected.len() > MAX_LAYERS
            || selected
                .iter()
                .enumerate()
                .any(|(i, id)| !ids.contains(id) || selected[..i].contains(id))
            || !selected.contains(&selected_active)
        {
            return Err("Invalid copied group raster selection".into());
        }
        let selected_active = ids.iter().position(|id| *id == selected_active).unwrap();
        let mut selected: Vec<_> = ids
            .iter()
            .copied()
            .filter(|id| selected.contains(id))
            .collect();
        if selected.len() == 1 {
            selected.clear();
        }
        // Copy only intentional source buffers after all metadata, selection,
        // identity, logical-budget and renderer checks have passed.
        let copied_masks = self.stage_group_mask_copy(&mask_copy);
        let mut copies: Vec<_> = layer_map
            .iter()
            .map(|(from, to)| (*to, self.layers[self.layer_index(*from).unwrap()].clone()))
            .collect();
        self.finish();
        let before = self.composition();
        let mut current = Vec::with_capacity(layer_map.len());
        for (index, id) in ids.iter().enumerate() {
            if let Some(position) = copies.iter().position(|(to, _)| to == id) {
                let (_, layer) = copies.remove(position);
                self.layers.insert(index, layer);
                current.push(index);
            }
        }
        self.layer_metadata = metadata;
        self.next_layer_id = next_layer.unwrap();
        self.next_group_id = next_group;
        self.next_adjustment_id = next_adjustment.unwrap();
        self.groups = Some(tree);
        self.graph = graph;
        self.active = selected_active;
        self.selected_layer_ids = selected;
        let mask_change = self.publish_group_mask_copy(mask_copy, copied_masks);
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_owner_composition(
            before,
            Some(LayerChange::Replace {
                current,
                saved: vec![],
                active,
            }),
            mask_change,
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
        if self.has_mixed_stack() {
            self.commit_tree(tree, self.next_group_id)
        } else {
            let mut metadata: Vec<_> = ids
                .iter()
                .map(|id| self.layer_metadata[self.layer_index(*id).unwrap()].clone())
                .collect();
            project_layer_links(&self.layer_metadata, &mut metadata)?;
            let graph = self.reindexed_graph(&metadata, None)?;
            let modes: Vec<_> = ids
                .iter()
                .map(|id| self.layers[self.layer_index(*id).unwrap()].mode)
                .collect();
            let opacities: Vec<_> = ids
                .iter()
                .map(|id| self.layers[self.layer_index(*id).unwrap()].opacity)
                .collect();
            let prepared = self.prepare_group_plan_with_masks(
                &graph,
                None,
                &metadata,
                &modes,
                &opacities,
                self.next_group_id,
                self.mask_state(),
                self.has_raster_masks(),
            )?;
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
            self.install_group_plan(prepared);
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
        validate_layer_links(&self.layer_metadata)?;
        if ids.len() != self.layers.len()
            || ids
                .iter()
                .enumerate()
                .any(|(i, id)| ids[..i].contains(id) || self.layer_index(*id).is_none())
        {
            return Err("Invalid raster permutation".into());
        }
        if self.has_mixed_stack() || self.has_raster_masks() {
            let tree = self
                .has_mixed_stack()
                .then(|| self.tree().reordered(ids))
                .transpose()?;
            let mut metadata: Vec<_> = ids
                .iter()
                .map(|id| self.layer_metadata[self.layer_index(*id).unwrap()].clone())
                .collect();
            project_layer_links(&self.layer_metadata, &mut metadata)?;
            let modes: Vec<_> = ids
                .iter()
                .map(|id| self.layers[self.layer_index(*id).unwrap()].mode)
                .collect();
            let opacities: Vec<_> = ids
                .iter()
                .map(|id| self.layers[self.layer_index(*id).unwrap()].opacity)
                .collect();
            let graph = self.reindexed_graph(&metadata, None)?;
            self.prepare_group_plan_with_masks(
                &graph,
                tree.as_ref(),
                &metadata,
                &modes,
                &opacities,
                self.next_group_id,
                self.mask_state(),
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
                    ItemKind::Adjustment => {
                        if let Ok(a) = tree.adjustment(item.id) {
                            rows.push(json!({"kind":"adjustment","id":a.id,"name":a.name,"parent_id":parent,"depth":depth,
                                "visible":a.visible,"effective_visible":visible&&a.visible,"opacity":a.opacity as f32/255.,
                                "node_type":a.node_type,"filter":a.filter,"amount":a.amount,"node_id":doc.adjustment_nodes.get(&a.id)}));
                        }
                    }
                    ItemKind::Group => {
                        if let Ok(g) = tree.group(item.id) {
                            let eligibility = doc.group_merge_eligibility(g.id);
                            let row = json!({"kind":"group","id":g.id,"name":g.name,"parent_id":parent,"depth":depth,"visible":g.visible,"effective_visible":visible&&g.visible,"opacity":g.opacity as f32/255.,"blend_mode":g.blend_mode,"compositing":g.compositing,"children":g.children,"layer_ids":tree.descendants(*item).unwrap_or_default(),"adjustment_ids":tree.adjustment_ids(*item).unwrap_or_default(),"item_count":g.children.len(),"node_id":doc.group_nodes.get(&g.id),"mergeable":eligibility.is_ok(),"merge_error":eligibility.err()});
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
        json!({"version":tree.version,"roots":tree.roots,"groups":groups,"adjustments":tree.adjustments,"next_adjustment_id":tree.next_adjustment_id,"rows":rows,"pass_through_available":self.pass_through_available()})
    }
}
