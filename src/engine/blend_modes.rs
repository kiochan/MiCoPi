//! Cold capability preflight for append-only color blend modes.
use super::*;

impl Document {
    pub fn ensure_blend_mode(&self, mode: BlendMode) -> Result<(), String> {
        if mode as u32 >= 8
            && !self
                .renderer
                .as_ref()
                .is_some_and(|r| r.supports_blend_mode(mode as u32))
        {
            return Err("The compositor does not support this blend mode".into());
        }
        Ok(())
    }
    pub fn ensure_blend_modes(
        &self,
        graph: &Graph,
        groups: Option<&super::super::groups::GroupTree>,
        modes: &[BlendMode],
    ) -> Result<(), String> {
        for mode in modes {
            self.ensure_blend_mode(*mode)?;
        }
        for node in &graph.nodes {
            if let NodeKind::Blend { mode, .. } = node.data {
                self.ensure_blend_mode(mode)?;
            }
        }
        if let Some(tree) = groups {
            for group in &tree.groups {
                self.ensure_blend_mode(group.blend_mode)?;
            }
        }
        Ok(())
    }
    pub fn available_blend_modes(&self) -> Vec<BlendMode> {
        BlendMode::MENU
            .into_iter()
            .filter(|mode| {
                *mode == BlendMode::Normal
                    || self
                        .renderer
                        .as_ref()
                        .is_some_and(|r| r.supports_blend_mode(*mode as u32))
            })
            .collect()
    }
    pub fn ensure_graph_blend_modes(&self, graph: &Graph) -> Result<(), String> {
        let mut modes: Vec<_> = self.layers.iter().map(|layer| layer.mode).collect();
        if graph.linked {
            for (mode, branch) in modes
                .iter_mut()
                .zip(graph.stack_branches(self.layers.len())?)
            {
                if let Some(Node {
                    data: NodeKind::Blend { mode: next, .. },
                    ..
                }) = branch.blend
                {
                    *mode = next;
                }
            }
        }
        self.ensure_blend_modes(graph, self.groups.as_ref(), &modes)
    }
}
