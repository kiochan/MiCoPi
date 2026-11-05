//! Validated movement defaults shared by desktop and headless actions.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveTarget {
    #[default]
    Auto,
    Layer,
    Selection,
    SelectionMask,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MovementPreferences {
    pub target: MoveTarget,
    pub step: u32,
    pub large_step: u32,
}
impl Default for MovementPreferences {
    fn default() -> Self {
        Self {
            target: MoveTarget::Auto,
            step: 1,
            large_step: 10,
        }
    }
}
impl MovementPreferences {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=4096).contains(&self.step) || !(1..=4096).contains(&self.large_step) {
            Err("Movement steps must be between 1 and 4096 pixels".into())
        } else {
            Ok(())
        }
    }
}
impl super::Session {
    pub fn apply_movement_defaults(&mut self, p: MovementPreferences) {
        if self.movement_defaults != p {
            self.movement_defaults = p;
            if let Some(ui) = self.ui.as_object_mut() {
                ui.remove("movement.options");
            }
        }
    }
    pub fn movement_options(&self) -> MovementPreferences {
        serde_json::from_value::<MovementPreferences>(self.ui["movement.options"].clone())
            .ok()
            .filter(|p| p.validate().is_ok())
            .unwrap_or(self.movement_defaults)
    }
}
