//! Scalar symmetry intent, independent of windows and native contact state.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymmetryMode {
    #[default]
    Off,
    Vertical,
    Horizontal,
    Both,
}
impl SymmetryMode {
    pub const ALL: [Self; 4] = [Self::Off, Self::Vertical, Self::Horizontal, Self::Both];
    pub fn is_off(&self) -> bool {
        *self == Self::Off
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Vertical => "Vertical Symmetry",
            Self::Horizontal => "Horizontal Symmetry",
            Self::Both => "Both Axes",
        }
    }
    pub(crate) fn axes(self) -> u32 {
        match self {
            Self::Off => 0,
            Self::Vertical => micopi_plugin_api::SYMMETRY_X,
            Self::Horizontal => micopi_plugin_api::SYMMETRY_Y,
            Self::Both => micopi_plugin_api::SYMMETRY_X | micopi_plugin_api::SYMMETRY_Y,
        }
    }
}
pub(crate) fn center() -> f32 {
    0.5
}
pub(crate) fn is_center(value: &f32) -> bool {
    *value == 0.5
}
