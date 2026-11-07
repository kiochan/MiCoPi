//! Read-only bounded observations through the shared native rendering provider.
use crate::application::{ActionError, ActionResult, Session};
use crate::engine::document::{HistogramPort, HistogramView};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
const NODE_FIELDS: [&str; 6] = [
    "graph_view",
    "node_id",
    "port",
    "input_index",
    "document_identity",
    "document_revision",
];
#[derive(Clone, Copy)]
enum Source {
    Composite,
    Layer(u64),
    Node {
        view: HistogramView,
        node: u32,
        port: HistogramPort,
        document: u64,
        revision: u64,
    },
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "source":{"type":"string","enum":["composite","layer","node"],"default":"composite","title":"Histogram Source","description":"Visible composite, explicit raw raster, or a fenced published graph image port. Node scopes do not sample UI drafts."},
        "layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Required only for raw layer scope; never inferred from painting selection."},
        "graph_view":{"type":"string","enum":["source","actual"],"title":"Graph View","description":"Required only for node scope. Discover IDs from graph.read.histogram_views; source declarations and actual composition are different views."},
        "node_id":{"type":"integer","minimum":0,"maximum":4294967295u64,"title":"Node ID"},
        "port":{"type":"string","enum":["input","output"],"title":"Image Port"},
        "input_index":{"type":"integer","minimum":0,"maximum":2,"title":"Input Index","description":"Required for input, forbidden for output. Coverage ports reject."},
        "document_identity":{"type":"integer","minimum":1,"title":"Document Identity"},
        "document_revision":{"type":"integer","minimum":0,"title":"Document Revision","description":"Node selectors require both captured document identity and Session revision."},
        "max_samples":{"type":"integer","minimum":1,"maximum":65536,"default":65536,"title":"Maximum Samples","description":"Deterministic centered-grid budget. Alpha-zero pixels skip; other alpha values count once."}},"additionalProperties":false})
}
fn invalid(message: &str) -> ActionError {
    ActionError::new("histogram_source", message)
}
fn integer(args: &Value, key: &str) -> Result<u64, ActionError> {
    args.get(key)
        .and_then(super::bounded_integer)
        .ok_or_else(|| invalid("Histogram source requires an explicit integer selector"))
}
fn arguments(args: &Value) -> Result<(Source, u32), ActionError> {
    let name = args
        .get("source")
        .map_or(Some("composite"), Value::as_str)
        .ok_or_else(|| invalid("Unknown histogram source"))?;
    if name != "node" && NODE_FIELDS.iter().any(|key| args.get(key).is_some()) {
        return Err(invalid(
            "Node selectors are forbidden for composite and layer scopes",
        ));
    }
    if name != "layer" && args.get("layer_id").is_some() {
        return Err(invalid("Only raw layer scope accepts a layer_id"));
    }
    let source = match name {
        "composite" => Source::Composite,
        "layer" => {
            let id = integer(args, "layer_id")?;
            if id == 0 || id > u32::MAX as u64 {
                return Err(invalid("Raw histogram source requires a stable raster ID"));
            }
            Source::Layer(id)
        }
        "node" => {
            let view = match args["graph_view"].as_str() {
                Some("source") => HistogramView::Source,
                Some("actual") => HistogramView::Actual,
                _ => return Err(invalid("Node histogram requires an explicit graph view")),
            };
            let node = integer(args, "node_id")?;
            if node > u32::MAX as u64 {
                return Err(invalid("Invalid histogram node ID"));
            }
            let port = match args["port"].as_str() {
                Some("input") => {
                    let index = integer(args, "input_index")?;
                    if index > 2 {
                        return Err(invalid("Invalid histogram input index"));
                    }
                    HistogramPort::Input(index as u32)
                }
                Some("output") => {
                    if args.get("input_index").is_some() {
                        return Err(invalid("Output histogram does not accept an input index"));
                    }
                    HistogramPort::Output
                }
                _ => return Err(invalid("Node histogram requires an explicit image port")),
            };
            let document = integer(args, "document_identity")?;
            if document == 0 {
                return Err(invalid("Invalid histogram document identity"));
            }
            Source::Node {
                view,
                node: node as u32,
                port,
                document,
                revision: integer(args, "document_revision")?,
            }
        }
        _ => return Err(invalid("Unknown histogram source")),
    };
    let budget = args
        .get("max_samples")
        .map(|v| {
            super::bounded_integer(v)
                .filter(|v| (1..=65536).contains(v))
                .ok_or_else(|| invalid("Invalid histogram sample budget"))
        })
        .transpose()?
        .unwrap_or(65536) as u32;
    Ok((source, budget))
}
pub(crate) fn validate_literal(args: &Value) -> Result<(), ActionError> {
    arguments(args).map(|_| ())
}
fn output(s: &Session, source: &str, budget: u32, result: &abi::HistogramResult) -> Value {
    let total = (s.doc.width * s.doc.height) as u64;
    json!({"document_identity":s.doc.identity,"revision":s.revision,"source":source,
        "width":s.doc.width,"height":s.doc.height,"total_pixels":total,"max_samples":budget,"sampling":"grid_centers",
        "grid_width":result.grid_width,"grid_height":result.grid_height,"sampled_pixels":result.sampled_pixels,
        "nontransparent_pixels":result.nontransparent_pixels,"ignored_transparent_pixels":result.sampled_pixels-result.nontransparent_pixels,
        "exact":result.sampled_pixels==total,"red":&result.bins[0][..],"green":&result.bins[1][..],"blue":&result.bins[2][..],"rgb":&result.bins[3][..]})
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    let (source, budget) = arguments(&args)?;
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before sampling a histogram",
        ));
    }
    if let Source::Node {
        view,
        node,
        port,
        document,
        revision,
    } = source
    {
        if document != s.doc.identity || revision != s.revision {
            return Err(ActionError::new(
                "histogram_stale",
                "Histogram node selection belongs to another document or revision",
            ));
        }
        if s.doc
            .renderer
            .as_ref()
            .is_none_or(|r| !r.supports_histogram_plan())
        {
            return Err(ActionError::new(
                "histogram_plan_unavailable",
                "Compositor histogram plan sampling is unavailable",
            ));
        }
        let observation = s
            .doc
            .histogram_observation(view, node, port, budget)
            .map_err(|e| ActionError::new("histogram_node", e))?;
        let mut value = output(s, "node", budget, &observation.result);
        value["graph_view"] = json!(match view {
            HistogramView::Source => "source",
            HistogramView::Actual => "actual",
        });
        value["node_id"] = json!(node);
        value["resolved_node_id"] = json!(observation.resolved_node_id);
        value["graph_enabled"] = json!(observation.graph_enabled);
        value["evaluation"] = json!(match view {
            HistogramView::Source => "source_declaration",
            HistogramView::Actual => "actual_plan",
        });
        value["port"] = json!(match port {
            HistogramPort::Input(_) => "input",
            HistogramPort::Output => "output",
        });
        if let HistogramPort::Input(index) = port {
            value["input_index"] = json!(index);
        }
        return Ok(value);
    }
    let layer = if let Source::Layer(id) = source {
        Some((
            id,
            s.doc
                .layer_index(id)
                .ok_or_else(|| invalid("Histogram layer ID does not exist"))?,
        ))
    } else {
        None
    };
    let provider = s
        .registry
        .native_plugins
        .get("compositing")
        .filter(|p| p.supports_histogram())
        .cloned()
        .ok_or_else(|| {
            ActionError::new(
                "histogram_unavailable",
                "Native histogram sampling is unavailable",
            )
        })?;
    if layer.is_none() {
        s.doc.compose();
        if let Some(error) = &s.doc.render_error {
            return Err(ActionError::new("compositing", error));
        }
    }
    let pixels = layer.map_or(s.doc.composite.as_slice(), |(_, index)| {
        s.doc.layers[index].pixels.as_slice()
    });
    let result = provider.histogram(
        pixels,
        abi::HistogramRequest {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::HistogramRequest>() as u32,
            width: s.doc.width as u32,
            height: s.doc.height as u32,
            stride: s.doc.width as u32,
            max_samples: budget,
        },
    )?;
    let mut value = output(
        s,
        if layer.is_some() {
            "layer"
        } else {
            "composite"
        },
        budget,
        &result,
    );
    if let Some((id, _)) = layer {
        value["layer_id"] = json!(id);
    }
    Ok(value)
}
