//! Structural edits move pixel buffers; only duplication allocates another layer.
use super::*;
impl Document {
    /// Simulate only bounded graph/identity metadata before a multi-target copy.
    pub(crate) fn preflight_layer_duplicates(&self, selected: &[u64]) -> Result<(), String> {
        self.require_adjustment_aware_edit()?;
        self.require_mask_aware_edit()?;
        let count = self.layers.len() + selected.len();
        if count > MAX_LAYERS
            || self.width * self.height * count > 64 * 1024 * 1024
            || self.next_layer_id + selected.len() as u64 > u32::MAX as u64 + 1
        {
            return Err("Layer count, document pixel or identity limit reached".into());
        }
        let mut ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let mut modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        let mut graph = self.graph.clone();
        let mut groups = self.groups.clone();
        let mut copies = Vec::new();
        for (offset, id) in selected.iter().enumerate() {
            let index = ids
                .iter()
                .position(|v| v == id)
                .ok_or("Layer does not exist")?;
            let old = ids.clone();
            let new = self.next_layer_id + offset as u64;
            copies.push((*id, new));
            if let Some(tree) = &mut groups {
                tree.insert_layer_after(*id, new)?;
            }
            ids.insert(index + 1, new);
            modes.insert(index + 1, modes[index]);
            if graph.linked {
                graph = graph.sync_stack(&modes, &old, &ids, Some((*id, new)))?;
            } else {
                for node in &mut graph.nodes {
                    if let NodeKind::Layer { index } = &mut node.data {
                        *index = ids
                            .iter()
                            .position(|id| *id == old[*index])
                            .ok_or("Invalid layer reference")?;
                    }
                }
            }
            graph.compile(ids.len())?;
        }
        let mut metadata: Vec<_> = ids
            .iter()
            .map(|id| {
                let source = copies
                    .iter()
                    .find(|(_, to)| to == id)
                    .map_or(*id, |(from, _)| *from);
                let mut m = self.layer_metadata[self.layer_index(source).unwrap()].clone();
                m.id = *id;
                m
            })
            .collect();
        project_layer_links(&self.layer_metadata, &mut metadata)?;
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
    pub(crate) fn preflight_layer_deletions(&self, selected: &[u64]) -> Result<(), String> {
        self.require_adjustment_aware_edit()?;
        self.require_mask_aware_edit()?;
        if selected.len() >= self.layers.len() {
            return Err("The final layer cannot be deleted".into());
        }
        let mut metadata: Vec<_> = self
            .layer_metadata
            .iter()
            .filter(|m| !selected.contains(&m.id))
            .cloned()
            .collect();
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        self.reindexed_graph(&metadata, None)?
            .compile(metadata.len())?;
        Ok(())
    }
    pub fn pixel_locked(&self, index: usize) -> bool {
        self.layer_metadata[index].pixel_locked
    }
    pub fn position_locked(&self, index: usize) -> bool {
        self.layer_metadata[index].position_locked
    }
    pub fn color_label(&self, index: usize) -> LayerColor {
        self.layer_metadata[index].color_label
    }
    pub fn alpha_locked(&self, index: usize) -> bool {
        self.layer_metadata[index].alpha_locked
    }

    pub fn layer_id(&self, index: usize) -> u64 {
        self.layer_metadata[index].id
    }
    pub fn layer_name(&self, index: usize) -> String {
        let name = &self.layer_metadata[index].name;
        if name.is_empty() {
            format!("Layer {:02}", index + 1)
        } else {
            name.clone()
        }
    }
    pub fn custom_layer_name(&self, index: usize) -> &str {
        &self.layer_metadata[index].name
    }
    pub fn layer_index(&self, id: u64) -> Option<usize> {
        self.layer_metadata.iter().position(|m| m.id == id)
    }
    pub(crate) fn track_layer_order(&mut self) {
        let count = self.width.div_ceil(TILE) * self.height.div_ceil(TILE);
        self.replay_tiles
            .extend((0..self.layers.len()).flat_map(|l| (0..count).map(move |t| (l, t))));
    }
    pub(super) fn reindexed_graph(
        &self,
        metadata: &[LayerMetadata],
        duplicate: Option<(u64, u64)>,
    ) -> Result<Graph, String> {
        if self.graph.linked {
            let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
            let new_ids: Vec<_> = metadata.iter().map(|m| m.id).collect();
            let modes: Vec<_> = new_ids
                .iter()
                .map(|id| {
                    let source = duplicate
                        .filter(|(_, to)| to == id)
                        .map_or(*id, |(from, _)| from);
                    let layer = self.layer_index(source).map(|i| &self.layers[i]);
                    layer.map_or(BlendMode::Normal, |l| l.mode)
                })
                .collect();
            return self.graph.sync_stack(&modes, &old_ids, &new_ids, duplicate);
        }
        let mut graph = self.graph.clone();
        for node in &mut graph.nodes {
            if let super::super::graph::NodeKind::Layer { index } = &mut node.data {
                let id = self
                    .layer_metadata
                    .get(*index)
                    .ok_or("Invalid layer reference")?
                    .id;
                *index = metadata
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or("Layer is referenced by an image node; remove that node first")?;
            }
        }
        graph.compile(metadata.len())?;
        Ok(graph)
    }
    pub fn rename_layer(&mut self, index: usize, name: String) -> Result<(), String> {
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if name.trim().is_empty() || name.len() > 512 || name.chars().any(char::is_control) {
            return Err(
                "Layer name must be nonempty, single-line and at most 512 UTF-8 bytes".into(),
            );
        }
        if self.layer_metadata[index].name == name {
            return Ok(());
        }
        self.finish();
        let before = self.composition();
        self.layer_metadata[index].name = name;
        self.commit_composition(before, None);
        Ok(())
    }
    pub fn duplicate_layer(&mut self, index: usize) -> Result<(), String> {
        self.duplicate_affine_layer(index, None)
    }
    /// A transformed copy inserts staged pixels/mask without a second undo entry.
    pub(crate) fn duplicate_affine_layer(
        &mut self,
        index: usize,
        staged: Option<(Vec<u32>, crate::engine::selection::SelectionState)>,
    ) -> Result<(), String> {
        if staged.is_some() {
            self.require_adjustment_aware_edit()?;
            // Transformed copies need owner-mask geometry, a separate contract.
            self.require_mask_aware_edit()?;
        }
        if let Some((pixels, selection)) = &staged {
            if pixels.len() != self.width * self.height {
                return Err("Invalid copied pixel count".into());
            }
            selection.validate(self.width, self.height)?;
        }
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if self.layers.len() >= MAX_LAYERS
            || self.width * self.height * (self.layers.len() + 1) > 64 * 1024 * 1024
        {
            return Err("Layer count or document pixel limit reached".into());
        }
        if self.next_layer_id > u32::MAX as u64 {
            return Err("Layer identity limit reached".into());
        }
        let mut metadata = self.layer_metadata.clone();
        metadata.insert(
            index + 1,
            LayerMetadata {
                id: self.next_layer_id,
                name: self.layer_metadata[index].name.clone(),
                alpha_locked: staged.is_none() && self.layer_metadata[index].alpha_locked,
                pixel_locked: staged.is_none() && self.pixel_locked(index),
                position_locked: staged.is_none() && self.position_locked(index),
                color_label: self.color_label(index),
                links: None,
            },
        );
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let graph =
            self.reindexed_graph(&metadata, Some((self.layer_id(index), self.next_layer_id)))?;
        let mut groups = self.groups.clone();
        if let Some(tree) = &mut groups {
            tree.insert_layer_after(self.layer_id(index), self.next_layer_id)?;
        }
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap_or(index)].mode)
            .collect();
        let opacities: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap_or(index)].opacity)
            .collect();
        let mask_edit = self.plan_owner_mask_copy(index, self.next_layer_id)?;
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            &opacities,
            self.next_group_id,
            &mask_edit.state,
            true,
        )?;
        let copied_mask = self.stage_owner_mask_copy(&mask_edit);
        let (layer, selection) = if let Some((pixels, selection)) = staged {
            let source = &self.layers[index];
            (
                Layer {
                    pixels,
                    visible: source.visible,
                    opacity: source.opacity,
                    mode: source.mode,
                },
                Some(selection),
            )
        } else {
            (self.layers[index].clone(), None)
        };
        self.finish();
        let before = self.composition();
        let active = self.active;
        if let Some(selection) = selection {
            self.selection = selection;
        }
        self.layers.insert(index + 1, layer);
        self.layer_metadata = metadata;
        self.next_layer_id += 1;
        self.select_layer(index + 1);
        let mask_change = self.publish_owner_mask_edit(mask_edit, copied_mask);
        self.graph = graph;
        self.groups = groups;
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_owner_composition(
            before,
            Some(LayerChange::Toggle {
                index: index + 1,
                active,
                removed: None,
            }),
            mask_change,
        );
        Ok(())
    }
    /// Insert exact pixels above the active layer as one structural undo entry.
    pub fn insert_pixels(&mut self, pixels: Vec<u32>) -> Result<(), String> {
        self.insert_transferred_pixels(pixels, None)
    }
    /// Optional source replacement and layer insertion share one tiled history entry.
    pub(crate) fn insert_transferred_pixels(
        &mut self,
        pixels: Vec<u32>,
        cut: Option<([usize; 4], &[u32])>,
    ) -> Result<(), String> {
        self.require_adjustment_aware_edit()?;
        if pixels.len() != self.width * self.height
            || self.layers.len() >= MAX_LAYERS
            || pixels.len() * (self.layers.len() + 1) > 64 * 1024 * 1024
            || self.next_layer_id > u32::MAX as u64
        {
            return Err("Layer dimensions, count or identity limit exceeded".into());
        }
        if self.graph.enabled && !self.graph.linked {
            return Err("Link the layer stack before inserting pasted pixels".into());
        }
        if let Some(([x, y, w, h], source)) = cut {
            if self.pixel_locked(self.active) {
                return Err("Cannot cut a pixel-locked layer".into());
            }
            if self.alpha_locked(self.active) {
                return Err("Cannot cut an alpha-locked layer".into());
            }
            if w == 0
                || h == 0
                || x.checked_add(w).is_none_or(|v| v > self.width)
                || y.checked_add(h).is_none_or(|v| v > self.height)
                || w.checked_mul(h) != Some(source.len())
            {
                return Err("Invalid source replacement region".into());
            }
        }
        let index = self.active + 1;
        let mut metadata = self.layer_metadata.clone();
        metadata.insert(
            index,
            LayerMetadata {
                id: self.next_layer_id,
                name: String::new(),
                alpha_locked: false,
                ..Default::default()
            },
        );
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let graph = self.reindexed_graph(&metadata, None)?;
        let mut groups = self.groups.clone();
        if let Some(tree) = &mut groups {
            tree.insert_layer_after(self.layer_id(self.active), self.next_layer_id)?;
        }
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| {
                self.layer_index(m.id)
                    .map_or(BlendMode::Normal, |i| self.layers[i].mode)
            })
            .collect();
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
        self.stroke = Some(Self::composition_edit(
            before,
            Some(LayerChange::Toggle {
                index,
                active,
                removed: None,
            }),
            active,
        ));
        if let Some(([x, y, w, h], source)) = cut {
            self.write_region(x, y, w, h, source);
        }
        self.layers.insert(
            index,
            Layer {
                pixels,
                visible: true,
                opacity: 255,
                mode: BlendMode::Normal,
            },
        );
        self.layer_metadata = metadata;
        self.next_layer_id += 1;
        self.select_layer(index);
        self.graph = graph;
        self.groups = groups;
        self.install_group_plan(prepared);
        if self.selection.current.is_some() {
            self.selection.previous = self.selection.current.take();
        }
        self.track_layer_order();
        self.finish();
        self.render_error = None;
        self.invalidate();
        Ok(())
    }

    pub fn move_layer(&mut self, index: usize, to: usize) -> Result<(), String> {
        if index >= self.layers.len() || to >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if index == to {
            return Ok(());
        }
        if self.has_mixed_stack() {
            let mut ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
            let moved = ids.remove(index);
            ids.insert(to, moved);
            return self.reorder_layers(&ids);
        }
        let mut metadata = self.layer_metadata.clone();
        let moved = metadata.remove(index);
        metadata.insert(to, moved);
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let graph = self.reindexed_graph(&metadata, None)?;
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].mode)
            .collect();
        let opacities: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].opacity)
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
        self.finish();
        let before = self.composition();
        let active = self.active;
        let selected = self.layer_id(active);
        let layer = self.layers.remove(index);
        self.layers.insert(to, layer);
        self.layer_metadata = metadata;
        self.active = self.layer_index(selected).unwrap();
        self.normalize_layer_selection();
        self.graph = graph;
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Move {
                from: to,
                to: index,
                active,
            }),
        );
        Ok(())
    }
    pub fn delete_layer(&mut self, index: usize) -> Result<(), String> {
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if self.layers.len() == 1 {
            return Err("The final layer cannot be deleted".into());
        }
        let mut metadata = self.layer_metadata.clone();
        metadata.remove(index);
        project_layer_links(&self.layer_metadata, &mut metadata)?;
        let graph = self.reindexed_graph(&metadata, None)?;
        let mut groups = self.groups.clone();
        if let Some(tree) = &mut groups {
            tree.remove_layer(self.layer_id(index));
        }
        let modes: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].mode)
            .collect();
        let opacities: Vec<_> = metadata
            .iter()
            .map(|m| self.layers[self.layer_index(m.id).unwrap()].opacity)
            .collect();
        let mask_edit = self.plan_owner_mask_delete(index)?;
        let prepared = self.prepare_group_plan_with_masks(
            &graph,
            groups.as_ref(),
            &metadata,
            &modes,
            &opacities,
            self.next_group_id,
            &mask_edit.state,
            true,
        )?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let selected = self.layer_id(active);
        let selected_survivor = self
            .selected_layer_ids()
            .into_iter()
            .filter(|id| *id != self.layer_id(index))
            .min_by_key(|id| {
                let i = self.layer_index(*id).unwrap();
                (i.abs_diff(active), i)
            });
        let removed = Some(Arc::new(self.layers.remove(index)));
        self.layer_metadata = metadata;
        self.active = self
            .layer_index(selected)
            .or_else(|| selected_survivor.and_then(|id| self.layer_index(id)))
            .unwrap_or(index.min(self.layers.len() - 1));
        self.normalize_layer_selection();
        let mask_change = self.publish_owner_mask_edit(mask_edit, None);
        self.graph = graph;
        self.groups = groups;
        self.install_group_plan(prepared);
        self.track_layer_order();
        self.commit_owner_composition(
            before,
            Some(LayerChange::Toggle {
                index,
                active,
                removed,
            }),
            mask_change,
        );
        Ok(())
    }
}
