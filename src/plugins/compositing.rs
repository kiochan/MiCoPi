use crate::{
    application::{ActionError, ActionResult, Operation, Session},
    engine::graph::{BlendMode, Filter, Graph},
};
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    match op {
        Operation::GraphRead => Ok(
            json!({"document_identity":s.doc.identity,"revision":s.revision,"graph":s.doc.graph,"histogram_views":{"source":s.doc.graph,"actual":s.doc.histogram_actual_graph()},"effective_graph":s.doc.effective_graph().map_err(|e|ActionError::new("invalid_graph",e))?,"groups":s.doc.groups_state(),"mode":if (s.doc.has_groups() || s.doc.has_adjustments()) {"grouped"} else if s.doc.graph.linked {"linked"} else {"free"},"layers":s.state()["layers"],"default_graph":Graph::linked_from_layers(&s.doc.layers),"renderer_available":s.doc.renderer.is_some(),"available_blend_modes":s.doc.available_blend_modes(),"renderer_transparent":s.doc.renderer.as_ref().is_some_and(|r|r.supports_transparent()),"renderer_mix":s.doc.renderer.as_ref().is_some_and(|r|r.supports_mix()),"renderer_histogram_plan":s.doc.renderer.as_ref().is_some_and(|r|r.supports_histogram_plan()),"renderer_levels":s.doc.renderer.as_ref().is_some_and(|r|r.supports_levels()),"renderer_channel_levels":s.doc.renderer.as_ref().is_some_and(|r|r.supports_levels_channels()),"renderer_curves":s.doc.renderer.as_ref().is_some_and(|r|r.supports_curves()),"renderer_hsl":s.doc.renderer.as_ref().is_some_and(|r|r.supports_hsl()),"render_error":s.doc.render_error}),
        ),
        Operation::GraphCatalog => Ok(
            json!({"version":1,"max_nodes":64,"definitions":s.registry.node_catalog().values().collect::<Vec<_>>(),"workflow_schema":crate::engine::workflow::schema(),"filters":Filter::ALL,"blend_modes":BlendMode::ALL,"available_blend_modes":s.doc.available_blend_modes(),"renderer_mix":s.doc.renderer.as_ref().is_some_and(|r|r.supports_mix()),"renderer_histogram_plan":s.doc.renderer.as_ref().is_some_and(|r|r.supports_histogram_plan()),"renderer_levels":s.doc.renderer.as_ref().is_some_and(|r|r.supports_levels()),"renderer_channel_levels":s.doc.renderer.as_ref().is_some_and(|r|r.supports_levels_channels()),"renderer_curves":s.doc.renderer.as_ref().is_some_and(|r|r.supports_curves()),"renderer_hsl":s.doc.renderer.as_ref().is_some_and(|r|r.supports_hsl()),"node_kinds":["transparent","layer","filter","levels","curves","hsl","blend","mix","output"],"graph_schema":crate::engine::graph::schema(),"image_type":"straight_alpha_argb","destructive":false}),
        ),
        Operation::GraphSet => {
            if (s.doc.has_groups() || s.doc.has_adjustments())
                && (s.stroke.active() || s.pointer_checkpoint.is_some())
            {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before changing grouped layer filters",
                ));
            }
            let graph: Graph = serde_json::from_value(args["graph"].clone())?;
            if (graph.has_prepared_color() || s.doc.graph.has_prepared_color())
                && (s.stroke.active() || s.pointer_checkpoint.is_some())
            {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before editing prepared color effects",
                ));
            }
            graph
                .compile(s.doc.layers.len())
                .map_err(|e| ActionError::new("invalid_graph", e))?;
            let graph = s.registry.normalize_image_nodes(graph)?;
            if (s.doc.has_groups() || s.doc.has_adjustments()) && !graph.linked {
                return Err(ActionError::new(
                    "group_graph",
                    "Ungroup layers before enabling free compositing",
                ));
            }
            if graph
                .nodes
                .iter()
                .any(|n| matches!(n.data, crate::engine::graph::NodeKind::Transparent))
                && !s
                    .doc
                    .renderer
                    .as_ref()
                    .is_some_and(|r| r.supports_transparent())
            {
                return Err(ActionError::new(
                    "renderer_missing",
                    "Compositor does not support transparent image nodes",
                ));
            }
            s.doc
                .ensure_graph_blend_modes(&graph)
                .map_err(|e| ActionError::new("blend_mode_unavailable", e))?;
            s.doc
                .ensure_mix_capability(&graph, None)
                .map_err(|e| ActionError::new("mix_unavailable", e))?;
            s.doc.ensure_levels_capability(&graph, true).map_err(|e| {
                ActionError::new(
                    if graph.has_hsl() {
                        "hsl_unavailable"
                    } else if graph.has_curves() {
                        "curves_unavailable"
                    } else {
                        "levels_unavailable"
                    },
                    e,
                )
            })?;
            s.finish_stroke();
            s.doc
                .set_graph(graph)
                .map_err(|e| ActionError::new("graph_error", e))?;
            Ok(json!({"graph":s.doc.graph}))
        }
        _ => Err(ActionError::new(
            "internal",
            "invalid compositing operation",
        )),
    }
}
