//! Published graph observations borrow pixels and prepared plans, never surfaces.
use super::*;
use crate::engine::graph::{Edge, PortDomain};
use micopi_plugin_api as abi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistogramView {
    Source,
    Actual,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistogramPort {
    Input(u32),
    Output,
}
pub struct HistogramObservation {
    pub resolved_node_id: u32,
    pub graph_enabled: bool,
    pub result: abi::HistogramResult,
}
impl Document {
    /// Presentation/discovery identity for a generated composition gate.
    /// Borrow names directly; avoid serializing the complete hierarchy to label
    /// or track a single observation target.
    pub fn histogram_node_owner(
        &self,
        node: u32,
    ) -> Option<(crate::engine::groups::GroupItem, &str)> {
        use crate::engine::groups::GroupItem;
        if let Some((id, _)) = self.group_nodes.iter().find(|(_, value)| **value == node) {
            let group = self.groups.as_ref()?.groups.iter().find(|g| g.id == *id)?;
            return Some((GroupItem::group(*id), &group.name));
        }
        if let Some((id, _)) = self
            .adjustment_nodes
            .iter()
            .find(|(_, value)| **value == node)
        {
            let adjustment = self.adjustments().iter().find(|a| a.id == *id)?;
            return Some((GroupItem::adjustment(*id), &adjustment.name));
        }
        None
    }
    /// Unlike the editable source, this graph reflects disabled flat filters.
    /// IDs belong to this explicit view at the current document revision.
    pub fn histogram_actual_graph(&self) -> Graph {
        self.effective_graph.clone().unwrap_or_else(|| {
            if self.graph.enabled && !self.graph.nodes.is_empty() {
                self.graph.clone()
            } else {
                Graph::from_layers(&self.layers)
            }
        })
    }
    pub fn histogram_observation(
        &mut self,
        view: HistogramView,
        node_id: u32,
        port: HistogramPort,
        budget: u32,
    ) -> Result<HistogramObservation, String> {
        if !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&budget) {
            return Err("Invalid histogram sample budget".into());
        }
        let mut graph = match view {
            HistogramView::Source => self.graph.clone(),
            HistogramView::Actual => self.histogram_actual_graph(),
        };
        let mask_ids = self
            .raster_masks
            .items
            .iter()
            .map(|m| m.id)
            .collect::<Vec<_>>();
        graph.compile_with_masks(self.layers.len(), &mask_ids)?;
        let target = graph
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .ok_or("Histogram node ID does not exist in this view")?;
        let resolved = match port {
            HistogramPort::Output => {
                if target.data.output_domain() != PortDomain::Image {
                    return Err("Histogram requires an image output, not coverage".into());
                }
                target.id
            }
            HistogramPort::Input(index) => {
                if target.data.input_domain(index as usize) != Some(PortDomain::Image) {
                    return Err("Histogram requires a valid image input port".into());
                }
                graph
                    .edges
                    .iter()
                    .find(|e| e.to == node_id && e.input == index)
                    .ok_or("Histogram input is not connected")?
                    .from
            }
        };
        let output = graph
            .nodes
            .iter()
            .find(|n| matches!(n.data, NodeKind::Output))
            .ok_or("Histogram graph has no output")?
            .id;
        let enabled = graph.enabled;
        if resolved != output {
            graph.edges.retain(|e| e.to != output);
            graph.edges.push(Edge {
                from: resolved,
                to: output,
                input: 0,
            });
        }
        // Explicit source observations evaluate declarations even when disabled;
        // original validation above retains linked-stack constraints and all IDs.
        graph.enabled = true;
        graph.linked = false;
        let plan = graph.compile_with_masks(self.layers.len(), &mask_ids)?;
        let renderer = self
            .renderer
            .clone()
            .filter(|r| r.supports_histogram_plan())
            .ok_or("Compositor histogram plan sampling is unavailable")?;
        self.ensure_graph_renderer()?;
        self.ensure_levels_capability(&graph, true)?;
        let prepared = self.prepare_levels_cache(&graph, true)?;
        let actual = view == HistogramView::Actual;
        if actual && self.has_raster_masks() {
            self.prepare_mask_effects()?;
        }
        let layers = self
            .layers
            .iter()
            .enumerate()
            .map(|(i, layer)| abi::CompositeLayer {
                data: layer.pixels.as_ptr(),
                len: layer.pixels.len(),
                visible: layer.visible as u32,
                opacity: if actual && self.masked_layers[i] {
                    255
                } else {
                    layer.opacity as u32
                },
                mode: layer.mode as u32,
            })
            .collect::<Vec<_>>();
        let mut masks = Vec::new();
        if actual {
            for slot in 0..self.mask_planes.len() {
                masks.push(super::mask_effects::descriptor(
                    self.effective_mask_view(slot)?,
                    self.width,
                ));
            }
        }
        let request = abi::HistogramRequest {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::HistogramRequest>() as u32,
            width: self.width as u32,
            height: self.height as u32,
            stride: self.width as u32,
            max_samples: budget,
        };
        let tables = prepared.as_ref().map_or(&[][..], |p| p.tables());
        let hsl = prepared.as_ref().map_or(&[][..], |p| p.hsl());
        let result = if graph.has_quantization() {
            renderer.histogram_quantization(&plan, &layers, &masks, tables, hsl, request)?
        } else if graph.has_hsl() {
            renderer.histogram_hsl(&plan, &layers, &masks, tables, hsl, request)?
        } else {
            renderer.histogram_plan(&plan, &layers, &masks, tables, request)?
        };
        Ok(HistogramObservation {
            resolved_node_id: resolved,
            graph_enabled: enabled,
            result,
        })
    }
}
