//! Read-only statistics through the registered native provider, outside contacts.
use crate::application::{ActionError, ActionResult, Session};
use micopi_plugin_api as abi;
use serde_json::{json, Value};
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "source":{"type":"string","enum":["composite","layer"],"default":"composite","title":"Histogram Source","description":"Published visible composite, or explicit raw raster pixels ignoring visibility, opacity, masks and effects."},
        "layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64,"description":"Required for layer source, forbidden for composite; never defaults to the active painting layer."},
        "max_samples":{"type":"integer","minimum":1,"maximum":65536,"default":65536,"title":"Maximum Samples","description":"Deterministic centered-grid budget; exact only when every pixel is visited. Alpha-zero pixels do not contribute; other alpha values have equal weight."}},"additionalProperties":false})
}
fn invalid(message: &str) -> ActionError {
    ActionError::new("histogram_source", message)
}
fn arguments(args: &Value) -> Result<(Option<u64>, u32), ActionError> {
    let source = match args.get("source") {
        None => "composite",
        Some(Value::String(s)) => s.as_str(),
        _ => return Err(invalid("Unknown histogram source")),
    };
    let layer = match source {
        "layer" => Some(
            args.get("layer_id")
                .and_then(super::bounded_integer)
                .filter(|id| *id > 0)
                .ok_or_else(|| invalid("Raw histogram source requires an explicit layer_id"))?,
        ),
        "composite" => {
            if args.get("layer_id").is_some() {
                return Err(invalid("Composite histogram does not accept a layer_id"));
            }
            None
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
    Ok((layer, budget))
}
pub(crate) fn validate_literal(args: &Value) -> Result<(), ActionError> {
    arguments(args).map(|_| ())
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    let (requested, budget) = arguments(&args)?;
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before sampling a histogram",
        ));
    }
    let layer = requested
        .map(|id| {
            s.doc
                .layer_index(id)
                .map(|index| (id, index))
                .ok_or_else(|| invalid("Histogram layer ID does not exist"))
        })
        .transpose()?;
    let source = if layer.is_some() {
        "layer"
    } else {
        "composite"
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
    let total = (s.doc.width * s.doc.height) as u64;
    let mut output = json!({"document_identity":s.doc.identity,"revision":s.revision,"source":source,
        "width":s.doc.width,"height":s.doc.height,"total_pixels":total,"max_samples":budget,"sampling":"grid_centers",
        "grid_width":result.grid_width,"grid_height":result.grid_height,"sampled_pixels":result.sampled_pixels,
        "nontransparent_pixels":result.nontransparent_pixels,"ignored_transparent_pixels":result.sampled_pixels-result.nontransparent_pixels,
        "exact":result.sampled_pixels==total,"red":&result.bins[0][..],"green":&result.bins[1][..],"blue":&result.bins[2][..],"rgb":&result.bins[3][..]});
    if let Some((id, _)) = layer {
        output["layer_id"] = json!(id);
    }
    Ok(output)
}
