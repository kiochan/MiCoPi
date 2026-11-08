//! Typed channel intents; native preparation compiles master then RGB tables.
use super::{graph::Renderer, levels::Levels};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    #[serde(deserialize_with = "super::levels::byte")]
    pub input: u8,
    #[serde(deserialize_with = "super::levels::byte")]
    pub output: u8,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    #[default]
    Smooth,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Curve {
    pub interpolation: Interpolation,
    pub points: Vec<Point>,
}
impl Default for Curve {
    fn default() -> Self {
        Self {
            interpolation: Interpolation::Smooth,
            points: vec![
                Point {
                    input: 0,
                    output: 0,
                },
                Point {
                    input: 255,
                    output: 255,
                },
            ],
        }
    }
}
impl Curve {
    pub fn schema() -> Value {
        json!({"type":"object","required":["interpolation","points"],"additionalProperties":false,
            "properties":{"interpolation":{"type":"string","enum":["linear","smooth"],"title":"Interpolation"},
            "points":{"type":"array","minItems":2,"maxItems":32,"title":"Control Points","items":{
            "type":"object","required":["input","output"],"additionalProperties":false,"properties":{
            "input":{"type":"integer","minimum":0,"maximum":255},
            "output":{"type":"integer","minimum":0,"maximum":255}}}}}})
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=abi::CURVES_MAX_POINTS).contains(&self.points.len())
            || self.points.windows(2).any(|p| p[0].input >= p[1].input)
        {
            return Err("Curves require 2 to 32 strictly input-ordered byte points".into());
        }
        Ok(())
    }
    pub fn parameters(&self) -> Result<abi::CurvesParameters, String> {
        self.validate()?;
        let mut p = abi::CurvesParameters {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::CurvesParameters>() as u32,
            count: self.points.len() as u32,
            interpolation: match self.interpolation {
                Interpolation::Linear => abi::CURVES_LINEAR,
                Interpolation::Smooth => abi::CURVES_SMOOTH,
            },
            points: [abi::CurvePoint::default(); abi::CURVES_MAX_POINTS],
        };
        for (target, source) in p.points.iter_mut().zip(&self.points) {
            *target = abi::CurvePoint {
                input: u32::from(source.input),
                output: u32::from(source.output),
            };
        }
        Ok(p)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Curves {
    pub master: Curve,
    pub red: Curve,
    pub green: Curve,
    pub blue: Curve,
}
impl Curves {
    pub fn schema() -> Value {
        json!({"type":"object","properties":{"master":Curve::schema(),"red":Curve::schema(),"green":Curve::schema(),"blue":Curve::schema()},
            "required":["master","red","green","blue"],"additionalProperties":false})
    }
    pub fn validate(&self) -> Result<(), String> {
        for curve in [&self.master, &self.red, &self.green, &self.blue] {
            curve.validate()?;
        }
        Ok(())
    }
    pub fn prepare(&self, renderer: &dyn Renderer) -> Result<[abi::CompositeLut; 3], String> {
        self.validate()?;
        if !renderer.supports_curves() {
            return Err("The compositor does not support Curves rendering".into());
        }
        let master = renderer.prepare_curves(&self.master.parameters()?)?;
        let mut result = [abi::CompositeLut { values: [0; 256] }; 3];
        for (output, curve) in result.iter_mut().zip([&self.red, &self.green, &self.blue]) {
            let channel = renderer.prepare_curves(&curve.parameters()?)?;
            for (target, value) in output.values.iter_mut().zip(master.values) {
                *target = channel.values[value as usize];
            }
        }
        Ok(result)
    }
}
/// Source-storage order binds each intent to its first table, independently of
/// topological render order. Derived tables never enter project/replay metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum ColorIntent {
    Levels(Levels),
    Curves(Curves),
    Hsl(super::hsl::Hsl),
    Quantization(super::quantization::Quantization),
}
impl ColorIntent {
    pub fn table_count(&self) -> usize {
        match self {
            Self::Levels(p) => p.table_count(),
            Self::Curves(_) => 3,
            Self::Hsl(_) | Self::Quantization(_) => 0,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Levels(p) => p.validate(),
            Self::Curves(p) => p.validate(),
            Self::Hsl(p) => p.validate(),
            Self::Quantization(p) => p.validate(),
        }
    }
    pub fn prepare(
        &self,
        renderer: &dyn Renderer,
        tables: &mut Vec<abi::CompositeLut>,
    ) -> Result<(), String> {
        match self {
            Self::Levels(p) => p.prepare_color(renderer, tables)?,
            Self::Curves(p) => tables.extend_from_slice(&p.prepare(renderer)?),
            Self::Hsl(_) if !renderer.supports_hsl() => {
                return Err("The compositor does not support HSL rendering".into())
            }
            Self::Hsl(_) => {}
            Self::Quantization(_) if !renderer.supports_quantization() => {
                return Err("The compositor does not support quantization rendering".into())
            }
            Self::Quantization(_) => {}
        }
        Ok(())
    }
}
