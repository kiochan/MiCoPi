//! Generated mask gates and scoped raw descriptors; editable filters stay separate.
use super::super::raster_masks::{MaskOwner, MaskState, MAX_MASKS};
use super::group_editing::GroupPlan;
use super::*;
use micopi_plugin_api as abi;
#[cfg(test)]
#[path = "mask_composition_tests.rs"]
mod tests;

pub(super) fn masked_flat_graph(
    source: &Graph,
    ids: &[u64],
    modes: &[BlendMode],
    opacities: &[u8],
    masks: &MaskState,
) -> Result<Graph, String> {
    if !source.linked || ids.len() != modes.len() || ids.len() != opacities.len() {
        return Err("Raster masks require matching linked layer composition".into());
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
    let mut last = None;
    for (index, branch) in branches.iter().enumerate() {
        graph.nodes.push(branch.source.clone());
        let mut head = branch.source.id;
        if source.enabled {
            for filter in &branch.filters {
                graph.edges.push(super::super::graph::Edge {
                    from: head,
                    to: filter.id,
                    input: 0,
                });
                head = filter.id;
                graph.nodes.push(filter.clone());
            }
        }
        if let Some(mask) = masks
            .items
            .iter()
            .find(|m| m.enabled && m.owner == MaskOwner::Layer { id: ids[index] })
        {
            let mask_node = fresh(
                &mut next,
                NodeKind::MaskSource { mask_id: mask.id },
                "mask_source",
            )?;
            let apply = fresh(
                &mut next,
                NodeKind::ApplyMask {
                    opacity: opacities[index] as f32 / 255.,
                },
                "apply_mask",
            )?;
            graph.edges.extend([
                super::super::graph::Edge {
                    from: head,
                    to: apply.id,
                    input: 0,
                },
                super::super::graph::Edge {
                    from: mask_node.id,
                    to: apply.id,
                    input: 1,
                },
            ]);
            head = apply.id;
            graph.nodes.extend([mask_node, apply]);
        }
        if let Some(before) = last {
            let mut blend = branch
                .blend
                .clone()
                .ok_or("Linked raster blend is missing")?;
            blend.data = NodeKind::Blend {
                mode: modes[index],
                opacity: 1.,
            };
            graph.edges.extend([
                super::super::graph::Edge {
                    from: before,
                    to: blend.id,
                    input: 0,
                },
                super::super::graph::Edge {
                    from: head,
                    to: blend.id,
                    input: 1,
                },
            ]);
            head = blend.id;
            graph.nodes.push(blend);
        }
        last = Some(head);
    }
    let output = source
        .nodes
        .iter()
        .find(|n| matches!(n.data, NodeKind::Output))
        .ok_or("Missing canvas output")?
        .clone();
    graph.edges.push(super::super::graph::Edge {
        from: last.ok_or("Image composition requires a raster")?,
        to: output.id,
        input: 0,
    });
    graph.nodes.push(output);
    arrange(&mut graph, ids, masks);
    graph.compile_with_masks(
        ids.len(),
        &masks.items.iter().map(|m| m.id).collect::<Vec<_>>(),
    )?;
    Ok(graph)
}
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
fn arrange(graph: &mut Graph, ids: &[u64], masks: &MaskState) {
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
            NodeKind::Layer { index } => index as f32 * 280.,
            NodeKind::MaskSource { mask_id } => masks
                .items
                .iter()
                .find(|m| m.id == mask_id)
                .and_then(|m| match m.owner {
                    MaskOwner::Layer { id } => ids.iter().position(|v| *v == id),
                    _ => None,
                })
                .map_or(0., |i| i as f32 * 280. + 140.),
            _ if !inputs.is_empty() => {
                inputs.iter().map(|v| v.1).sum::<f32>() / inputs.len() as f32
            }
            _ => 0.,
        };
        node.position = [
            if matches!(node.data, NodeKind::MaskSource { .. }) {
                -260.
            } else {
                column as f32 * 260.
            },
            row,
        ];
        layout.insert(node.id, (column, row));
    }
}
impl Document {
    pub(super) fn ensure_raster_mask_renderer(&self) -> Result<(), String> {
        if self.renderer.as_ref().is_none_or(|r| !r.supports_masks()) {
            return Err("Raster masks require a compositor DLL with mask coverage support".into());
        }
        Ok(())
    }
    pub(super) fn ensure_raster_mask_properties(&self, masks: &MaskState) -> Result<(), String> {
        if masks
            .items
            .iter()
            .any(|m| m.density != 255 || m.feather_radius_px != 0)
            && self
                .renderer
                .as_ref()
                .is_none_or(|renderer| !renderer.supports_mask_effects())
        {
            return Err(
                "Raster mask density and feather require compositor mask effects support".into(),
            );
        }
        Ok(())
    }
    /// Metadata-only preflight. Candidate planes need not exist until publication.
    pub(super) fn prepare_raster_mask_plan(
        &self,
        candidate: &MaskState,
        require: bool,
    ) -> Result<GroupPlan, String> {
        self.prepare_group_plan_with_masks(
            &self.graph,
            self.groups.as_ref(),
            &self.layer_metadata,
            &self.layers.iter().map(|l| l.mode).collect::<Vec<_>>(),
            &self.layers.iter().map(|l| l.opacity).collect::<Vec<_>>(),
            self.next_group_id,
            candidate,
            require,
        )
    }
    /// The cached mask/layer slots avoid ID resolution and allocation on damage.
    pub(super) fn render_masked_composition(
        &mut self,
        layers: &mut [abi::CompositeLayer],
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        self.ensure_raster_mask_renderer()?;
        self.prepare_mask_effects()?;
        if layers.len() != self.layers.len()
            || self.mask_planes.len() != self.mask_state().items.len()
            || self.mask_planes.len() > MAX_MASKS
        {
            return Err("Raster mask render storage does not match composition".into());
        }
        let mut descriptors = [abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: 255,
        }; MAX_MASKS];
        for (slot, descriptor) in descriptors
            .iter_mut()
            .take(self.mask_planes.len())
            .enumerate()
        {
            *descriptor =
                super::mask_effects::descriptor(self.effective_mask_view(slot)?, self.width);
        }
        for (i, layer) in layers.iter_mut().enumerate() {
            if self.masked_layers[i] {
                layer.opacity = 255;
            }
        }
        self.renderer.as_ref().unwrap().render_prepared(
            &self.plan,
            layers,
            &descriptors[..self.mask_planes.len()],
            self.levels_plan.as_ref().map_or(&[], |p| p.tables()),
            self.width as u32,
            self.height as u32,
            output,
        )
    }
}
