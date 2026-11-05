//! Bounded document-owned repeat recipe; independent of UI and plugin instances.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Layer,
    Selection,
    SelectionMask,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Nearest,
    #[default]
    Bilinear,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Stored")]
pub struct Recipe {
    pub matrix: [f64; 6],
    pub target: Target,
    pub method: Method,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    matrix: [f64; 6],
    target: Target,
    method: Method,
}
impl Recipe {
    pub fn validate(&self) -> Result<(), String> {
        let [a, b, c, d, e, f] = self.matrix;
        if self.matrix.iter().any(|v| !v.is_finite())
            || [a, b, d, e].iter().any(|v| v.abs() > 256.)
            || [c, f].iter().any(|v| v.abs() > 4_194_304.)
            || (a * e - b * d).abs() < 1e-8
        {
            return Err("Combined transform exceeds affine limits".into());
        }
        Ok(())
    }
}
impl TryFrom<Stored> for Recipe {
    type Error = String;
    fn try_from(s: Stored) -> Result<Self, String> {
        let recipe = Self {
            matrix: s.matrix,
            target: s.target,
            method: s.method,
        };
        recipe.validate()?;
        Ok(recipe)
    }
}
