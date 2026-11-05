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
    #[serde(
        default = "default_denominator",
        skip_serializing_if = "standard_denominator"
    )]
    pub denominator: [f64; 3],
    pub target: Target,
    pub method: Method,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    matrix: [f64; 6],
    #[serde(default = "default_denominator")]
    denominator: [f64; 3],
    target: Target,
    method: Method,
}
impl Recipe {
    pub fn mapping(&self) -> super::homography::Mapping {
        super::homography::Mapping {
            matrix: self.matrix,
            denominator: self.denominator,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.mapping().validate()
    }
}
impl TryFrom<Stored> for Recipe {
    type Error = String;
    fn try_from(s: Stored) -> Result<Self, String> {
        let recipe = Self {
            matrix: s.matrix,
            denominator: s.denominator,
            target: s.target,
            method: s.method,
        };
        recipe.validate()?;
        Ok(recipe)
    }
}

fn default_denominator() -> [f64; 3] {
    [0., 0., 1.]
}
fn standard_denominator(value: &[f64; 3]) -> bool {
    *value == default_denominator()
}
