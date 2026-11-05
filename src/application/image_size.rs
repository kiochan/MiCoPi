//! Read-only all-layer sizing plans; native sampling and document history remain separate.
use super::{
    output_size::Method,
    units::{pixels_per_inch, Unit},
    ActionError,
};
use crate::engine::{document_info::Info, Document};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    width: Option<f64>,
    height: Option<f64>,
    #[serde(default)]
    unit: Unit,
    #[serde(default = "locked")]
    lock_aspect: bool,
    #[serde(default = "bilinear")]
    method: Method,
}
fn locked() -> bool {
    true
}
fn bilinear() -> Method {
    Method::Bilinear
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("image_size", message)
}
fn parameters(args: &Value) -> Result<Parameters, ActionError> {
    for key in ["width", "height"] {
        if args.get(key).is_some_and(Value::is_null) {
            return Err(invalid("Image dimensions cannot be null"));
        }
    }
    let p: Parameters = serde_json::from_value(args.clone()).map_err(|e| invalid(e.to_string()))?;
    if !matches!(
        (p.width.is_some(), p.height.is_some(), p.lock_aspect),
        (true, false, true) | (false, true, true) | (true, true, false)
    ) {
        return Err(invalid(
            "With aspect lock supply exactly one dimension; otherwise supply both",
        ));
    }
    for value in [p.width, p.height].into_iter().flatten() {
        if !value.is_finite() || value <= 0. || value > 1_000_000_000. {
            return Err(invalid(
                "Image dimensions must be positive and no more than 1000000000 units",
            ));
        }
        if p.unit == Unit::Pixels {
            p.unit.to_pixels(value, None).map_err(invalid)?;
        }
    }
    Ok(p)
}

/// Validate literal arguments without assuming the source state of an earlier pipeline step.
pub(crate) fn validate(args: &Value) -> Result<(), ActionError> {
    parameters(args).map(|_| ())
}
pub(crate) struct Plan {
    source: [usize; 2],
    pub width: usize,
    pub height: usize,
    pub method: Method,
    info: Info,
    layers: usize,
    masks: usize,
}
impl Plan {
    pub(crate) fn identity(&self) -> bool {
        self.source == [self.width, self.height]
    }
    pub(crate) fn state(&self, revision: u64, drawing: bool) -> Value {
        let pixels = self.width * self.height;
        let layers = pixels * self.layers * 4;
        let masks = pixels * self.masks;
        json!({"source_width":self.source[0],"source_height":self.source[1],
            "width":self.width,"height":self.height,"method":self.method,
            "identity":self.identity(),"document_info":self.info.state(self.width,self.height),
            "layer_count":self.layers,"mask_count":self.masks,
            "estimated_layer_bytes":layers,"estimated_mask_bytes":masks,
            "estimated_composite_bytes":pixels*4,
            "estimated_staged_bytes":if self.identity(){0}else{layers+masks},
            "revision":revision,"drawing":drawing})
    }
}
pub(crate) fn plan(doc: &Document, args: &Value) -> Result<Plan, ActionError> {
    let p = parameters(args)?;
    let source = [doc.width, doc.height];
    if source.iter().any(|v| !(1..=4096).contains(v))
        || !(1..=crate::engine::MAX_LAYERS).contains(&doc.layers.len())
    {
        return Err(invalid("Invalid image source dimensions or layer count"));
    }
    let info = doc.info();
    info.validate().map_err(invalid)?;
    let ppi = pixels_per_inch(info);
    let convert = |number: Option<f64>, axis: usize| {
        number
            .map(|v| {
                p.unit
                    .to_pixels(v, ppi.map(|axes| axes[axis]))
                    .map_err(invalid)
            })
            .transpose()
    };
    let [width, height] = crate::plugins::resample::dimensions(
        source,
        convert(p.width, 0)?,
        convert(p.height, 1)?,
        p.lock_aspect,
    )
    .map_err(invalid)?;
    if width * height * doc.layers.len() > 64 * 1024 * 1024 {
        return Err(invalid("Image size exceeds document pixel budget"));
    }
    let selection = doc.selection();
    Ok(Plan {
        source,
        width,
        height,
        method: p.method,
        info,
        layers: doc.layers.len(),
        masks: usize::from(selection.current.is_some()) + usize::from(selection.previous.is_some()),
    })
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "width":{"type":"number","minimum":0,"maximum":1000000000,"description":"Positive target width in unit. Pixels must be integers; physical values round half up to 1-4096 pixels."},
        "height":{"type":"number","minimum":0,"maximum":1000000000,"description":"Positive target height in unit. With aspect lock supply exactly one dimension; otherwise supply both."},
        "unit":{"type":"string","enum":["px","mm","in"],"default":"px","description":"Physical input uses exact current metric axes; unknown PNG density requires an explicit canvas.resolution change first."},
        "lock_aspect":{"type":"boolean","default":true,"description":"Preserve the source pixel ratio after converting the requested axis."},
        "method":{"type":"string","enum":["bilinear","nearest","area"],"default":"bilinear","description":"Area averages shrinking footprints and uses bilinear for enlarging axes."}
    },"required":[],"additionalProperties":false})
}
