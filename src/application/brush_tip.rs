//! Scalar tip intent; native plugins own raster coverage and contact geometry.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipShape {
    #[default]
    Round,
    Square,
}
impl TipShape {
    pub const ALL: [Self; 2] = [Self::Round, Self::Square];
    pub fn is_round(&self) -> bool {
        *self == Self::Round
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Round => "Round",
            Self::Square => "Square",
        }
    }
    pub(crate) fn as_abi(self) -> u32 {
        match self {
            Self::Round => micopi_plugin_api::TIP_ROUND,
            Self::Square => micopi_plugin_api::TIP_SQUARE,
        }
    }
}
pub(crate) fn default_tip_roundness() -> f32 {
    1.
}
pub(crate) fn full_tip_roundness(value: &f32) -> bool {
    *value == 1.
}
pub(crate) fn zero_tip_angle(value: &f32) -> bool {
    *value == 0.
}
