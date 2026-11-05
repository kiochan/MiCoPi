//! Creation plans validate without allocating pixels before document replacement.
use super::{units::Unit, ActionError};
use crate::engine::document_info::Info;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    width: f64,
    height: f64,
    #[serde(default)]
    unit: Unit,
    #[serde(default)]
    resolution_ppi: Option<f64>,
    #[serde(default = "background")]
    background: String,
}
fn background() -> String {
    "transparent".into()
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("invalid_document_size", message)
}

pub(crate) struct Plan {
    pub width: usize,
    pub height: usize,
    pub info: Info,
}
impl Plan {
    pub(crate) fn state(&self, revision: u64, drawing: bool) -> Value {
        json!({"width":self.width,"height":self.height,
            "document_info":self.info.state(self.width,self.height),
            "estimated_initial_pixel_bytes":self.width*self.height*8,
            "revision":revision,"drawing":drawing})
    }
}

pub(crate) fn plan(args: &Value) -> Result<Plan, ActionError> {
    if args.get("resolution_ppi").is_some_and(Value::is_null) {
        return Err(invalid("Resolution cannot be null"));
    }
    let p: Parameters = serde_json::from_value(args.clone()).map_err(|e| invalid(e.to_string()))?;
    let ppi = p.resolution_ppi.unwrap_or(72.);
    if !ppi.is_finite() || ppi.fract() != 0. || !(1.0..=2400.0).contains(&ppi) {
        return Err(invalid(
            "Resolution must be an integer between 1 and 2400 PPI",
        ));
    }
    let info = Info {
        resolution_ppi: ppi as u32,
        resolution_explicit: p.resolution_ppi.is_some() || p.unit != Unit::Pixels,
        png_density: None,
        initial_background: Info::parse_background(&p.background).map_err(invalid)?,
    };
    info.validate().map_err(invalid)?;
    Ok(Plan {
        width: p.unit.to_pixels(p.width, Some(ppi)).map_err(invalid)?,
        height: p.unit.to_pixels(p.height, Some(ppi)).map_err(invalid)?,
        info,
    })
}

pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "width":{"type":"number","minimum":0,"maximum":1000000000,"description":"Positive width in unit; pixels must be integers. Physical dimensions round half up to 1-4096 pixels."},
        "height":{"type":"number","minimum":0,"maximum":1000000000,"description":"Positive height in unit; pixels must be integers. Both dimensions are required."},
        "unit":{"type":"string","enum":["px","mm","in"],"default":"px","description":"Physical units use resolution_ppi, default 72, and record explicit physical intent."},
        "resolution_ppi":{"type":"integer","minimum":1,"maximum":2400,"default":72,"description":"Square pixels per inch; physical-unit creation writes density even at the default 72 PPI."},
        "background":{"type":"string","default":"transparent","description":"transparent or #RRGGBB; initializes editable pixels on the first layer."}
    },"required":["width","height"],"additionalProperties":false})
}
