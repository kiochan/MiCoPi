//! Strict non-raster affect-below stack metadata. Pixel kernels remain native.
use super::graph::Filter;
use super::levels::Levels;
use serde::{Deserialize, Serialize};

pub const MAX_ADJUSTMENTS: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdjustmentSpec {
    pub name: String,
    pub visible: bool,
    pub opacity: u8,
    pub node_type: String,
    pub filter: Filter,
    pub amount: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<Levels>,
}
impl AdjustmentSpec {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(levels) = self.levels {
            levels.validate()?;
            if self.filter != Filter::Invert || self.amount != 0. {
                return Err("Levels adjustments require neutral legacy filter fields".into());
            }
        }
        if self.name.trim().is_empty()
            || self.name.len() > 512
            || self.name.chars().any(char::is_control)
            || self.node_type.is_empty()
            || self.node_type.len() > 256
            || self
                .node_type
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || !self.amount.is_finite()
            || !(self.filter.minimum()..=1.).contains(&self.amount)
        {
            return Err("Invalid adjustment name, registered filter or parameters".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Adjustment {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub opacity: u8,
    pub node_type: String,
    pub filter: Filter,
    pub amount: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub levels: Option<Levels>,
}
impl Adjustment {
    pub fn new(id: u64, spec: AdjustmentSpec) -> Self {
        Self {
            id,
            name: spec.name,
            visible: spec.visible,
            opacity: spec.opacity,
            node_type: spec.node_type,
            filter: spec.filter,
            amount: spec.amount,
            levels: spec.levels,
        }
    }
    pub fn spec(&self) -> AdjustmentSpec {
        AdjustmentSpec {
            name: self.name.clone(),
            visible: self.visible,
            opacity: self.opacity,
            node_type: self.node_type.clone(),
            filter: self.filter,
            amount: self.amount,
            levels: self.levels,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id == 0 || self.id > u32::MAX as u64 {
            return Err("Invalid adjustment identity".into());
        }
        self.spec().validate()
    }
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.name.len() + self.node_type.len()
    }
}
