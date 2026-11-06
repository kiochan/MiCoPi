//! Isolated group baking and hidden-preserving visible-stack consolidation.
use super::super::groups::{GroupItem, GroupPolicy, GroupTree, ItemKind};
use super::group_editing::GroupPlan;
use super::*;

impl Document {
    /// Shared semantic gates for commands and public inspector feedback.
    /// Contacts and final staged topology/rendering are validated at execution.
    pub fn group_merge_eligibility(&self, group_id: u64) -> Result<(), String> {
        self.require_mask_aware_edit()?;
        self.ensure_group_renderer()?;
        self.ensure_mix_capability(&self.graph, self.groups.as_ref())?;
        self.ensure_blend_modes(
            &self.graph,
            self.groups.as_ref(),
            &self.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
        )?;
        let tree = self.groups.as_ref().ok_or("Group does not exist")?;
        let group = tree.group(group_id)?;
        if group.compositing == GroupPolicy::PassThrough {
            return Err("Pass Through groups depend on the outside backdrop; switch to Isolated or merge visible artwork".into());
        }
        let ids = tree.descendants(GroupItem::group(group_id))?;
        if ids.is_empty() {
            return Err("An empty group has no raster layers to merge".into());
        }
        let indices: Vec<_> = ids
            .iter()
            .map(|id| {
                self.layer_index(*id)
                    .ok_or("Group references a missing raster")
            })
            .collect::<Result<_, _>>()?;
        self.check_group_bake_locks(&indices)?;
        if self.next_layer_id > u32::MAX as u64 {
            return Err("Layer identity limit reached".into());
        }
        Ok(())
    }
    fn check_group_bake_locks(&self, indices: &[usize]) -> Result<(), String> {
        if indices.iter().any(|i| self.pixel_locked(*i)) {
            return Err("Unlock layer pixels before merging these layers".into());
        }
        if indices.iter().any(|i| self.alpha_locked(*i)) {
            return Err("Unlock layer transparency before merging these layers".into());
        }
        Ok(())
    }

    /// The only image allocation is the raster that will replace the sources.
    /// Global source indices let subtree rendering borrow every raw buffer.
    pub(super) fn render_group_bake(&self, graph: &Graph) -> Result<Vec<u32>, String> {
        self.require_mask_aware_edit()?;
        self.ensure_group_renderer()?;
        let plan = graph.compile(self.layers.len())?;
        let mut layers = [micopi_plugin_api::CompositeLayer {
            data: std::ptr::null(),
            len: 0,
            visible: 0,
            opacity: 0,
            mode: 0,
        }; MAX_LAYERS];
        for (slot, layer) in layers.iter_mut().zip(&self.layers) {
            *slot = micopi_plugin_api::CompositeLayer {
                data: layer.pixels.as_ptr(),
                len: layer.pixels.len(),
                visible: layer.visible as u32,
                opacity: layer.opacity as u32,
                mode: layer.mode as u32,
            };
        }
        let mut pixels = vec![0; self.width * self.height];
        let mut output = micopi_plugin_api::Pixels {
            data: pixels.as_mut_ptr(),
            len: pixels.len(),
            width: self.width as u32,
            height: self.height as u32,
            stride: self.width as u32,
            origin_x: 0,
            origin_y: 0,
        };
        self.renderer.as_ref().unwrap().render(
            &plan,
            &layers[..self.layers.len()],
            self.width as u32,
            self.height as u32,
            &mut output,
        )?;
        Ok(pixels)
    }

    pub(super) fn publish_group_bake(
        &mut self,
        metadata: Vec<LayerMetadata>,
        groups: Option<GroupTree>,
        graph: Graph,
        prepared: GroupPlan,
        consumed: &[usize],
        insert: usize,
        layer: Layer,
        next_layer_id: u64,
    ) {
        self.finish();
        let before = self.composition();
        let active = self.active;
        let mut saved = Vec::with_capacity(consumed.len());
        for &index in consumed.iter().rev() {
            saved.push((index, Arc::new(self.layers.remove(index))));
        }
        saved.reverse();
        self.layers.insert(insert, layer);
        self.layer_metadata = metadata;
        self.groups = groups;
        self.next_layer_id = next_layer_id;
        self.select_layer(insert);
        self.graph = graph;
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Replace {
                current: vec![insert],
                saved,
                active,
            }),
        );
    }

    pub fn group_merge(&mut self, group_id: u64) -> Result<u64, String> {
        self.group_merge_eligibility(group_id)?;
        let original = self.groups.as_ref().ok_or("Group does not exist")?;
        let group = original.group(group_id)?.clone();
        let consumed_ids = original.descendants(GroupItem::group(group_id))?;
        if consumed_ids.is_empty() {
            return Err("An empty group has no raster layers to merge".into());
        }
        let consumed: Vec<_> = consumed_ids
            .iter()
            .map(|id| self.layer_index(*id).unwrap())
            .collect();
        self.check_group_bake_locks(&consumed)?;
        if self.next_layer_id > u32::MAX as u64 {
            return Err("Layer identity limit reached".into());
        }
        let raster_id = self.next_layer_id;
        let parent = original.parent(GroupItem::group(group_id))?;
        let mut tree = original.clone();
        let siblings = tree.children_mut(parent)?;
        let slot = siblings
            .iter()
            .position(|item| *item == GroupItem::group(group_id))
            .unwrap();
        siblings[slot] = GroupItem::layer(raster_id);
        let mut removed_groups = vec![group_id];
        let mut offset = 0;
        while offset < removed_groups.len() {
            removed_groups.extend(
                original
                    .group(removed_groups[offset])?
                    .children
                    .iter()
                    .filter(|item| item.kind == ItemKind::Group)
                    .map(|item| item.id),
            );
            offset += 1;
        }
        tree.groups.retain(|g| !removed_groups.contains(&g.id));
        let new_ids = tree.layer_ids()?;
        if new_ids.len() > MAX_LAYERS || self.width * self.height * new_ids.len() > 64 * 1024 * 1024
        {
            return Err("Group merge exceeds the document pixel limit".into());
        }
        let insert = new_ids.iter().position(|id| *id == raster_id).unwrap();
        let metadata: Vec<_> = new_ids
            .iter()
            .map(|id| {
                if *id == raster_id {
                    LayerMetadata {
                        id: raster_id,
                        name: group.name.clone(),
                        ..Default::default()
                    }
                } else {
                    self.layer_metadata[self.layer_index(*id).unwrap()].clone()
                }
            })
            .collect();
        let modes: Vec<_> = new_ids
            .iter()
            .map(|id| {
                if *id == raster_id {
                    group.blend_mode
                } else {
                    self.layers[self.layer_index(*id).unwrap()].mode
                }
            })
            .collect();
        let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let graph = self
            .graph
            .sync_baked_stack(&modes, &old_ids, &new_ids, None, &[])?;
        let groups = (!tree.groups.is_empty()).then_some(tree);
        let prepared = self.prepare_group_plan(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        // This temporary render forest is never persisted: its roots are the
        // selected group's children, excluding its own and all ancestor gates.
        let mut contents = original.clone();
        contents.roots = group.children.clone();
        let source_modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        let contents_graph = contents.compile_graph(&self.graph, &old_ids, &source_modes)?;
        let pixels = self.render_group_bake(&contents_graph)?;
        self.publish_group_bake(
            metadata,
            groups,
            graph,
            prepared,
            &consumed,
            insert,
            Layer {
                pixels,
                visible: group.visible,
                opacity: group.opacity,
                mode: group.blend_mode,
            },
            raster_id + 1,
        );
        Ok(raster_id)
    }

    pub(super) fn merge_grouped_visible(&mut self) -> Result<(), String> {
        self.ensure_group_renderer()?;
        let consumed: Vec<_> = (0..self.layers.len())
            .filter(|i| self.layer_visible(*i))
            .collect();
        let target = *consumed
            .last()
            .ok_or("There are no effectively visible raster layers to merge")?;
        self.check_group_bake_locks(&consumed)?;
        let retained = self.layer_metadata[target].clone();
        let consumed_ids: Vec<_> = consumed.iter().map(|i| self.layer_id(*i)).collect();
        let mut tree = self.groups.as_ref().ok_or("Group does not exist")?.clone();
        for id in &consumed_ids {
            tree.remove_layer(*id);
        }
        tree.roots.push(GroupItem::layer(retained.id));
        let new_ids = tree.layer_ids()?;
        let insert = new_ids.len() - 1;
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
        let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let graph =
            self.graph
                .sync_baked_stack(&modes, &old_ids, &new_ids, None, &[retained.id])?;
        let prepared = self.prepare_group_plan(
            &graph,
            Some(&tree),
            &metadata,
            &modes,
            self.next_group_id,
            true,
        )?;
        let pixels = self.render_group_bake(&self.effective_graph()?)?;
        self.publish_group_bake(
            metadata,
            Some(tree),
            graph,
            prepared,
            &consumed,
            insert,
            Layer {
                pixels,
                visible: true,
                opacity: 255,
                mode: BlendMode::Normal,
            },
            self.next_layer_id,
        );
        Ok(())
    }
}
