//! Cold output sizing shared by desktop forms, action schemas and automation.
use crate::application::{
    units::{rounded_pixels, Unit},
    ActionError,
};
use crate::engine::document_info::Info;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Nearest,
    Bilinear,
    #[default]
    Area,
}
impl Method {
    pub fn native(self) -> micopi_plugin_api::Resample {
        micopi_plugin_api::Resample {
            method: match self {
                Self::Nearest => 0,
                Self::Bilinear => 1,
                Self::Area => 2,
            },
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Parameters {
    width: Option<f64>,
    height: Option<f64>,
    unit: Unit,
    lock_aspect: bool,
    scale: Option<f64>,
    method: Method,
    resolution_ppi: Option<f64>,
}
impl Default for Parameters {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            unit: Unit::Pixels,
            lock_aspect: true,
            scale: None,
            method: Method::Area,
            resolution_ppi: None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Plan {
    pub source: [usize; 2],
    pub width: usize,
    pub height: usize,
    pub method: Method,
    pub info: Info,
}
impl Plan {
    pub fn identity(&self) -> bool {
        self.source == [self.width, self.height]
    }
    pub fn state(&self, revision: u64, drawing: bool) -> Value {
        json!({"source_width":self.source[0],"source_height":self.source[1],"width":self.width,"height":self.height,
            "method":self.method,"identity":self.identity(),"document_info":self.info.state(self.width,self.height),
            "estimated_output_bytes":self.width*self.height*4,"revision":revision,"drawing":drawing})
    }
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("invalid_export_size", message)
}
pub(crate) fn plan(source: [usize; 2], mut info: Info, args: &Value) -> Result<Plan, ActionError> {
    if source.iter().any(|v| !(1..=4096).contains(v)) {
        return Err(invalid("Invalid output source dimensions"));
    }
    info.validate().map_err(invalid)?;
    let mut args = args.clone();
    args.as_object_mut()
        .ok_or_else(|| invalid("Output parameters must be an object"))?
        .remove("path");
    for key in ["width", "height", "scale", "resolution_ppi"] {
        if args.get(key).is_some_and(Value::is_null) {
            return Err(invalid("Output dimensions and resolution cannot be null"));
        }
    }
    let p: Parameters = serde_json::from_value(args).map_err(|e| invalid(e.to_string()))?;
    if let Some(ppi) = p.resolution_ppi {
        if !ppi.is_finite() || ppi.fract() != 0. || !(1.0..=2400.0).contains(&ppi) {
            return Err(invalid("Output resolution must be between 1 and 2400 PPI"));
        }
        info.resolution_ppi = ppi as u32;
        info.resolution_explicit = true;
        info.png_density = None;
    }
    let dimensions = if let Some(scale) = p.scale {
        if p.width.is_some() || p.height.is_some() || p.unit != Unit::Pixels {
            return Err(invalid(
                "Percentage scale cannot be combined with dimensions or physical units",
            ));
        }
        if !scale.is_finite() || scale <= 0. || scale > 409600. {
            return Err(invalid(
                "Output scale must be positive and no more than 409600 percent",
            ));
        }
        [
            rounded_pixels(source[0] as f64 * scale / 100.).map_err(invalid)?,
            rounded_pixels(source[1] as f64 * scale / 100.).map_err(invalid)?,
        ]
    } else if p.width.is_none() && p.height.is_none() {
        if p.unit != Unit::Pixels {
            return Err(invalid("Specify a dimension when using physical units"));
        }
        source
    } else {
        let ppi = match info.png_density {
            Some(d) if d.metric() => Some([d.xppu as f64 * 0.0254, d.yppu as f64 * 0.0254]),
            None => Some([info.resolution_ppi as f64; 2]),
            _ => None,
        };
        let convert = |number: Option<f64>, axis: usize| -> Result<Option<usize>, ActionError> {
            number
                .map(|v| {
                    p.unit
                        .to_pixels(v, ppi.map(|axes| axes[axis]))
                        .map_err(invalid)
                })
                .transpose()
        };
        crate::plugins::resample::dimensions(
            source,
            convert(p.width, 0)?,
            convert(p.height, 1)?,
            p.lock_aspect,
        )
        .map_err(invalid)?
    };
    Ok(Plan {
        source,
        width: dimensions[0],
        height: dimensions[1],
        method: p.method,
        info,
    })
}
pub(crate) fn schema(with_path: bool) -> Value {
    let mut properties = json!({
        "width":{"type":"number","minimum":0,"maximum":1000000000,"description":"Target width in unit. Pixels require an integer; positive physical values round half up to whole pixels."},
        "height":{"type":"number","minimum":0,"maximum":1000000000,"description":"Target height in unit. With aspect lock supply exactly one dimension; otherwise supply both."},
        "unit":{"type":"string","enum":["px","mm","in"],"default":"px","description":"Physical units require known metric density or resolution_ppi override."},
        "lock_aspect":{"type":"boolean","default":true,"description":"Preserve source pixel aspect ratio with exactly one supplied dimension."},
        "scale":{"type":"number","minimum":0,"maximum":409600,"description":"Positive percentage of both source axes; exclusive of dimensions and physical units."},
        "method":{"type":"string","enum":["nearest","bilinear","area"],"default":"area","description":"Area averages shrinking footprints and uses bilinear for enlarging axes."},
        "resolution_ppi":{"type":"integer","minimum":1,"maximum":2400,"description":"Override output density only with square metric PPI. Omit to inherit exact document density."}
    });
    let required = if with_path {
        properties["path"] = json!({"type":"string","minLength":1});
        vec!["path"]
    } else {
        vec![]
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document_info::{Density, DensityUnit};
    #[test]
    fn output_plan_converts_metric_axes_before_aspect_rounding_and_does_not_clamp() {
        let info = Info::with_png_density(Density {
            xppu: 1000,
            yppu: 2000,
            unit: DensityUnit::Meter,
        });
        let p = plan(
            [400, 200],
            info,
            &json!({"width":100,"height":100,"unit":"mm","lock_aspect":false}),
        )
        .unwrap();
        assert_eq!([p.width, p.height], [100, 200]);
        assert_eq!(p.info, info);
        let p = plan([400, 200], info, &json!({"height":10,"unit":"mm"})).unwrap();
        assert_eq!([p.width, p.height], [40, 20]);
        let p = plan(
            [400, 200],
            info,
            &json!({"width":1,"unit":"in","resolution_ppi":100}),
        )
        .unwrap();
        assert_eq!([p.width, p.height], [100, 50]);
        assert!(p.info.png_density.is_none());
        assert!(plan([1, 4096], info, &json!({"width":2})).is_err());
        assert!(plan([1, 1], info, &json!({"scale":1})).is_err());
    }
    #[test]
    fn output_plan_unknown_units_require_an_override_and_legacy_identity_inherits_metadata() {
        for d in [
            Density {
                xppu: 4,
                yppu: 3,
                unit: DensityUnit::Unspecified,
            },
            Density {
                xppu: 0,
                yppu: 1000,
                unit: DensityUnit::Meter,
            },
        ] {
            let info = Info::with_png_density(d);
            let p = plan([20, 10], info, &json!({})).unwrap();
            assert!(p.identity());
            assert_eq!(p.info, info);
            assert!(plan([20, 10], info, &json!({"width":10,"unit":"mm"})).is_err());
            let p = plan(
                [20, 10],
                info,
                &json!({"width":25.4,"unit":"mm","resolution_ppi":72}),
            )
            .unwrap();
            assert_eq!([p.width, p.height], [72, 36]);
            assert_eq!(p.info.export_density(), Some(Density::from_ppi(72)));
        }
        for args in [
            json!({"width":1.5}),
            json!({"scale":100,"width":2}),
            json!({"width":2,"height":2}),
            json!({"width":-1}),
            json!({"unit":"mm"}),
            json!({"scale":0}),
            json!({"resolution_ppi":2401}),
        ] {
            assert!(plan([20, 10], Info::default(), &args).is_err(), "{args}");
        }
    }
}
