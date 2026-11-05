//! Structural edits move pixel buffers; only duplication allocates another layer.
use super::*;
impl Document {
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
                alpha_locked: self.layer_metadata[index].alpha_locked,
            },
        );
        let graph =
            self.reindexed_graph(&metadata, Some((self.layer_id(index), self.next_layer_id)))?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let layer = self.layers[index].clone();
        self.layers.insert(index + 1, layer);
        self.layer_metadata = metadata;
        self.next_layer_id += 1;
        self.active = index + 1;
        self.graph = graph;
        self.plan = self.graph.compile(self.layers.len())?;
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Toggle {
                index: index + 1,
                active,
                removed: None,
            }),
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
            },
        );
        let graph = self.reindexed_graph(&metadata, None)?;
        let plan = graph.compile(metadata.len())?;
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
        self.active = index;
        self.graph = graph;
        self.plan = plan;
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
        let mut metadata = self.layer_metadata.clone();
        let moved = metadata.remove(index);
        metadata.insert(to, moved);
        let graph = self.reindexed_graph(&metadata, None)?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let selected = self.layer_id(active);
        let layer = self.layers.remove(index);
        self.layers.insert(to, layer);
        self.layer_metadata = metadata;
        self.active = self.layer_index(selected).unwrap();
        self.graph = graph;
        self.plan = self.graph.compile(self.layers.len())?;
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
        let graph = self.reindexed_graph(&metadata, None)?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let selected = self.layer_id(active);
        let removed = Some(Arc::new(self.layers.remove(index)));
        self.layer_metadata = metadata;
        self.active = self
            .layer_index(selected)
            .unwrap_or(index.min(self.layers.len() - 1));
        self.graph = graph;
        self.plan = self.graph.compile(self.layers.len())?;
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Toggle {
                index,
                active,
                removed,
            }),
        );
        Ok(())
    }
}
