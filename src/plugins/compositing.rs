use crate::{
    application::{ActionError, ActionResult, Operation, Session},
    engine::graph::{BlendMode, Filter, Graph},
};
use serde_json::{json, Value};
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    match op {
        Operation::GraphRead => Ok(
            json!({"graph":s.doc.graph,"mode":if s.doc.graph.linked {"linked"} else {"free"},"layers":s.state()["layers"],"default_graph":Graph::linked_from_layers(&s.doc.layers),"renderer_available":s.doc.renderer.is_some(),"render_error":s.doc.render_error}),
        ),
        Operation::GraphCatalog => Ok(
            json!({"version":1,"max_nodes":64,"definitions":s.registry.node_catalog().values().collect::<Vec<_>>(),"workflow_schema":crate::engine::workflow::schema(),"filters":Filter::ALL,"blend_modes":BlendMode::ALL,"node_kinds":["layer","filter","blend","output"],"graph_schema":crate::engine::graph::schema(),"image_type":"straight_alpha_argb","destructive":false}),
        ),
        Operation::GraphSet => {
            let graph: Graph = serde_json::from_value(args["graph"].clone())?;
            graph
                .compile(s.doc.layers.len())
                .map_err(|e| ActionError::new("invalid_graph", e))?;
            let graph = s.registry.normalize_image_nodes(graph)?;
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
