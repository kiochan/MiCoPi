//! Layer consolidation delegates pixel composition to the existing native renderer.
//! History moves removed buffers; untouched hidden layers retain their allocations.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeLayers {
    Down,
    Visible,
    Flatten,
    Stamp,
}

impl Document {
    fn render_merge(&self, indices: &[usize], full_graph: bool) -> Result<Vec<u32>, String> {
        let renderer = self
            .renderer
            .as_ref()
            .ok_or("Compositor DLL is required to merge layers")?;
        let graph = if full_graph {
            self.graph.clone()
        } else if self.graph.linked {
            let old: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
            let ids: Vec<_> = indices.iter().map(|&i| self.layer_id(i)).collect();
            let modes: Vec<_> = indices.iter().map(|&i| self.layers[i].mode).collect();
            self.graph.sync_stack(&modes, &old, &ids, None)?
        } else {
            let mut graph = Graph::default();
            graph.enabled = false;
            graph
        };
        let plan = if graph.enabled && (!graph.linked || graph.has_filters()) {
            graph.compile(indices.len())?
        } else {
            vec![]
        };
        let layers: Vec<_> = indices
            .iter()
            .map(|&i| {
                let layer = &self.layers[i];
                micopi_plugin_api::CompositeLayer {
                    data: layer.pixels.as_ptr(),
                    len: layer.pixels.len(),
                    visible: layer.visible as u32,
                    opacity: layer.opacity as u32,
                    mode: layer.mode as u32,
                }
            })
            .collect();
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
        renderer.render(
            &plan,
            &layers,
            self.width as u32,
            self.height as u32,
            &mut output,
        )?;
        Ok(pixels)
    }

    /// Bake supported layer stacks. Flatten preserves the transparent canvas;
    /// Merge Down bakes a pair in isolation, using normal blending above a backdrop.
    pub fn merge_layers(&mut self, operation: MergeLayers, index: usize) -> Result<(), String> {
        if index >= self.layers.len() {
            return Err("Layer does not exist".into());
        }
        if !self.graph.linked && operation != MergeLayers::Flatten {
            return Err("Layer merges and Stamp Visible require linked composition; use Flatten Image to bake a free graph".into());
        }
        let visible: Vec<_> = self
            .layers
            .iter()
            .enumerate()
            .filter_map(|(i, l)| l.visible.then_some(i))
            .collect();
        let indices: Vec<_> = match operation {
            MergeLayers::Down => {
                if index == 0 {
                    return Err("There is no layer below this layer".into());
                }
                if !self.layers[index].visible || !self.layers[index - 1].visible {
                    return Err("Both layers must be visible before merging down".into());
                }
                if index > 1
                    && [index - 1, index]
                        .iter()
                        .any(|&i| self.layers[i].mode != BlendMode::Normal)
                {
                    return Err("Backdrop-dependent blend modes above other layers require Merge Visible or Flatten Image".into());
                }
                vec![index - 1, index]
            }
            MergeLayers::Visible => {
                if visible.is_empty() {
                    return Err("There are no visible layers to merge".into());
                }
                if visible.len() == 1 {
                    return Ok(());
                }
                visible.clone()
            }
            MergeLayers::Flatten | MergeLayers::Stamp => (0..self.layers.len()).collect(),
        };
        if operation == MergeLayers::Stamp {
            if self.layers.len() >= MAX_LAYERS
                || self.width * self.height * (self.layers.len() + 1) > 64 * 1024 * 1024
            {
                return Err("Layer count or document pixel limit reached".into());
            }
            if self.next_layer_id > u32::MAX as u64 {
                return Err("Layer identity limit reached".into());
            }
        } else {
            if indices.iter().any(|&i| self.pixel_locked(i)) {
                return Err("Unlock layer pixels before merging these layers".into());
            }
            if indices.iter().any(|&i| self.alpha_locked(i)) {
                return Err("Unlock layer transparency before merging these layers".into());
            }
        }
        let full = operation != MergeLayers::Down;
        // Full-stack rendering includes original visibility, filters and blend modes.
        // Removed hidden layers are included only by explicit Flatten Image.
        let all: Vec<_> = (0..self.layers.len()).collect();
        let pixels = self.render_merge(if full { &all } else { &indices }, full)?;
        let target = if operation == MergeLayers::Down {
            index - 1
        } else {
            visible.last().copied().unwrap_or(index)
        };
        let mut metadata = self.layer_metadata.clone();
        let mut modes: Vec<_> = self.layers.iter().map(|l| l.mode).collect();
        let (insert, removed) = if operation == MergeLayers::Stamp {
            metadata.push(LayerMetadata {
                id: self.next_layer_id,
                name: "Merged copy".into(),
                alpha_locked: false,
                ..Default::default()
            });
            modes.push(BlendMode::Normal);
            (self.layers.len(), vec![])
        } else {
            let insert = target - indices.iter().filter(|&&i| i < target).count();
            let retained = self.layer_metadata[target].clone();
            for &i in indices.iter().rev() {
                metadata.remove(i);
                modes.remove(i);
            }
            metadata.insert(
                insert,
                LayerMetadata {
                    alpha_locked: false,
                    ..retained
                },
            );
            modes.insert(insert, BlendMode::Normal);
            (insert, indices)
        };
        let ids: Vec<_> = metadata.iter().map(|m| m.id).collect();
        let old_ids: Vec<_> = self.layer_metadata.iter().map(|m| m.id).collect();
        let baked = if operation == MergeLayers::Stamp {
            vec![]
        } else {
            vec![self.layer_id(target)]
        };
        let graph = if self.graph.linked {
            self.graph
                .sync_baked_stack(&modes, &old_ids, &ids, None, &baked)?
        } else {
            // Build the supported result without retaining stale free-graph references.
            let layers: Vec<_> = modes
                .iter()
                .map(|&mode| Layer {
                    pixels: vec![],
                    visible: true,
                    opacity: 255,
                    mode,
                })
                .collect();
            Graph::linked_from_layers(&layers)
        };
        let plan = graph.compile(metadata.len())?;
        self.finish();
        let before = self.composition();
        let active = self.active;
        let mut saved = Vec::with_capacity(removed.len());
        for &i in removed.iter().rev() {
            saved.push((i, Arc::new(self.layers.remove(i))));
        }
        saved.reverse();
        self.layers.insert(
            insert,
            Layer {
                pixels,
                visible: true,
                opacity: 255,
                mode: BlendMode::Normal,
            },
        );
        self.layer_metadata = metadata;
        if operation == MergeLayers::Stamp {
            self.next_layer_id += 1;
        }
        self.active = insert;
        self.graph = graph;
        self.plan = plan;
        self.track_layer_order();
        self.commit_composition(
            before,
            Some(LayerChange::Replace {
                current: vec![insert],
                saved,
                active,
            }),
        );
        Ok(())
    }
}
