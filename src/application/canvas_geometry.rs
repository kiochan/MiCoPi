//! Read-only frame/permutation plans shared by commands and document staging.
use super::{ActionError, Operation};
use crate::engine::{document_info::Info, Document};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Resize,
    Crop,
    #[serde(rename = "rotate_90_cw")]
    Rotate90Cw,
    #[serde(rename = "rotate_90_ccw")]
    Rotate90Ccw,
    #[serde(rename = "rotate_180")]
    Rotate180,
    FlipHorizontal,
    FlipVertical,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    operation: Kind,
    width: Option<usize>,
    height: Option<usize>,
    x: Option<usize>,
    y: Option<usize>,
    anchor: Option<String>,
}
const ANCHORS: [&str; 9] = [
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("canvas_geometry", message)
}
fn parameters(args: &Value) -> Result<Parameters, ActionError> {
    for key in ["width", "height", "x", "y", "anchor"] {
        if args.get(key).is_some_and(Value::is_null) {
            return Err(invalid("Geometry parameters cannot be null"));
        }
    }
    // The shared integer schema also accepts JSON 12.0. Preserve that existing
    // canvas action contract before deserializing bounded integer storage.
    let mut normalized = args.clone();
    for key in ["width", "height", "x", "y"] {
        if let Some(value) = args.get(key) {
            let number = value
                .as_f64()
                .filter(|v| v.is_finite() && v.fract() == 0. && (0.0..=4096.0).contains(v))
                .ok_or_else(|| {
                    invalid("Geometry dimensions and coordinates must be bounded integers")
                })?;
            normalized[key] = json!(number as usize);
        }
    }
    let p: Parameters = serde_json::from_value(normalized).map_err(|e| invalid(e.to_string()))?;
    let dimensions = || {
        p.width.is_some_and(|v| (1..=4096).contains(&v))
            && p.height.is_some_and(|v| (1..=4096).contains(&v))
    };
    match p.operation {
        Kind::Resize => {
            if !dimensions() || p.x.is_some() || p.y.is_some() {
                return Err(invalid(
                    "Resize requires width/height and accepts only an optional anchor",
                ));
            }
            if p.anchor
                .as_ref()
                .is_some_and(|a| !ANCHORS.contains(&a.as_str()))
            {
                return Err(invalid("Unknown canvas anchor"));
            }
        }
        Kind::Crop => {
            if !dimensions()
                || !p.x.is_some_and(|v| v <= 4095)
                || !p.y.is_some_and(|v| v <= 4095)
                || p.anchor.is_some()
            {
                return Err(invalid(
                    "Crop requires x/y/width/height and does not take an anchor",
                ));
            }
        }
        _ => {
            if p.width.is_some()
                || p.height.is_some()
                || p.x.is_some()
                || p.y.is_some()
                || p.anchor.is_some()
            {
                return Err(invalid(
                    "Document rotations and flips take no frame parameters",
                ));
            }
        }
    }
    Ok(p)
}
/// Literal validation deliberately does not use an earlier pipeline step's source frame.
pub(crate) fn validate(args: &Value) -> Result<(), ActionError> {
    parameters(args).map(|_| ())
}
pub(crate) struct Plan {
    kind: Kind,
    source: [usize; 2],
    pub width: usize,
    pub height: usize,
    pub dx: i32,
    pub dy: i32,
    pub geometry: Option<abi::Geometry>,
    pub info: Info,
    layers: usize,
    selections: usize,
    raster_masks: usize,
    effects: usize,
}
impl Plan {
    pub(crate) fn identity(&self) -> bool {
        self.geometry.is_none()
            && self.source == [self.width, self.height]
            && (self.dx, self.dy) == (0, 0)
    }
    pub(crate) fn state(
        &self,
        revision: u64,
        drawing: bool,
        sampler_available: bool,
        contact_active: bool,
    ) -> Value {
        let area = self.width * self.height;
        let layers = area * self.layers * 4;
        let selections = area * self.selections;
        let masks = area * self.raster_masks;
        json!({"operation":self.kind,"source_width":self.source[0],"source_height":self.source[1],
            "width":self.width,"height":self.height,"offset_x":self.dx,"offset_y":self.dy,
            "identity":self.identity(),"layer_count":self.layers,
            "pixel_selection_count":self.selections,"raster_mask_count":self.raster_masks,
            "estimated_layer_bytes":layers,"estimated_selection_bytes":selections,
            "estimated_raster_mask_bytes":masks,"estimated_effective_mask_bytes":area*self.effects,
            "estimated_composite_bytes":area*4,
            "estimated_staged_bytes":if self.identity(){0}else{layers+selections+masks},
            "cost_model":"dense_outputs","raster_mask_fill":255,
            "raster_mask_feather_policy":"fixed_canvas_pixels",
            "requires_a8_sampler":self.raster_masks!=0&&!self.identity(),
            "a8_sampler_available":sampler_available,
            "document_info":self.info.state(self.width,self.height),"revision":revision,"drawing":drawing,
            "contact_active":contact_active})
    }
}
pub(crate) fn plan(doc: &Document, args: &Value) -> Result<Plan, ActionError> {
    let p = parameters(args)?;
    let (sw, sh) = (doc.width, doc.height);
    doc.validate_canvas_geometry_frame(sw, sh)
        .map_err(invalid)?;
    let mut info = doc.info();
    info.validate().map_err(invalid)?;
    let (width, height, dx, dy, geometry) = match p.operation {
        Kind::Resize => {
            let (w, h) = (p.width.unwrap(), p.height.unwrap());
            let a = p.anchor.as_deref().unwrap_or("center");
            let x = match a {
                "left" | "top_left" | "bottom_left" => 0,
                "right" | "top_right" | "bottom_right" => w as i32 - sw as i32,
                _ => (w as i32 - sw as i32).div_euclid(2),
            };
            let y = match a {
                "top" | "top_left" | "top_right" => 0,
                "bottom" | "bottom_left" | "bottom_right" => h as i32 - sh as i32,
                _ => (h as i32 - sh as i32).div_euclid(2),
            };
            (w, h, x, y, None)
        }
        Kind::Crop => {
            let (w, h, x, y) = (
                p.width.unwrap(),
                p.height.unwrap(),
                p.x.unwrap(),
                p.y.unwrap(),
            );
            if x + w > sw || y + h > sh {
                return Err(ActionError::new(
                    "invalid_crop",
                    "Crop rectangle must be inside the canvas",
                ));
            }
            (w, h, -(x as i32), -(y as i32), None)
        }
        kind => {
            let kind = match kind {
                Kind::Rotate90Cw => 0,
                Kind::Rotate90Ccw => 1,
                Kind::Rotate180 => 2,
                Kind::FlipHorizontal => 3,
                _ => 4,
            };
            let (w, h) = if kind <= 1 {
                info = info.rotated_quarter();
                (sh, sw)
            } else {
                (sw, sh)
            };
            (w, h, 0, 0, Some(abi::Geometry { kind, x: 0, y: 0 }))
        }
    };
    doc.validate_canvas_geometry_frame(width, height)
        .map_err(invalid)?;
    Ok(Plan {
        kind: p.operation,
        source: [sw, sh],
        width,
        height,
        dx,
        dy,
        geometry,
        info,
        layers: doc.layers.len(),
        selections: usize::from(doc.selection().current.is_some())
            + usize::from(doc.selection().previous.is_some()),
        raster_masks: doc.mask_state().items.len(),
        effects: doc
            .mask_state()
            .items
            .iter()
            .filter(|m| m.density != 255 || m.feather_radius_px != 0)
            .count(),
    })
}
pub(crate) fn for_operation(
    doc: &Document,
    op: Operation,
    args: &Value,
) -> Result<Plan, ActionError> {
    let kind = match op {
        Operation::CanvasSize => Kind::Resize,
        Operation::Crop => Kind::Crop,
        Operation::RotateCanvasClockwise => Kind::Rotate90Cw,
        Operation::RotateCanvasCounterclockwise => Kind::Rotate90Ccw,
        Operation::RotateCanvasHalf => Kind::Rotate180,
        Operation::FlipCanvasHorizontal => Kind::FlipHorizontal,
        Operation::FlipCanvasVertical => Kind::FlipVertical,
        _ => return Err(invalid("Unsupported document geometry operation")),
    };
    let mut args = args
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("Expected geometry arguments"))?;
    args.insert("operation".into(), serde_json::to_value(kind).unwrap());
    plan(doc, &Value::Object(args))
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "operation":{"type":"string","enum":["resize","crop","rotate_90_cw","rotate_90_ccw","rotate_180","flip_horizontal","flip_vertical"]},
        "width":{"type":"integer","minimum":1,"maximum":4096},
        "height":{"type":"integer","minimum":1,"maximum":4096},
        "x":{"type":"integer","minimum":0,"maximum":4095},
        "y":{"type":"integer","minimum":0,"maximum":4095},
        "anchor":{"type":"string","enum":ANCHORS,"description":"Resize only; defaults to center when absent."}
    },"required":["operation"],"additionalProperties":false,
    "description":"Read-only all-layer/raw-mask frame plan. Resize/crop require their frame fields; document flips/rotations accept no other fields. Dense-output estimates exclude retained history, native scratch and the derived coverage cache. Raw-mask feather radius stays numerically fixed in canvas pixels."})
}
