//! Active profile data is separate from document history and plugin ownership.
use super::{ActionError, Session};
use crate::configuration::{brush_presets::BrushPresetLibrary, Settings};

impl Session {
    pub fn preset_library(&self) -> &BrushPresetLibrary {
        &self.brush_presets
    }
    pub fn preset_library_token(&self) -> u64 {
        self.preset_library_token
    }
    /// Install an already validated profile snapshot without applying painting
    /// defaults or touching a native instance. Callers perform file I/O first.
    pub fn load_profile_presets(&mut self, settings: &Settings) -> Result<(), ActionError> {
        self.validate_profile_presets(settings)?;
        if self.brush_presets != settings.brush_presets {
            self.preset_library_token += 1;
            self.brush_presets = settings.brush_presets.clone();
        }
        if self.settings_path.is_none() {
            self.settings_path = Some(settings.path.clone());
        }
        Ok(())
    }
    pub(crate) fn validate_profile_presets(&self, settings: &Settings) -> Result<(), ActionError> {
        settings
            .brush_presets
            .validate()
            .map_err(|e| ActionError::new("brush_presets", e))?;
        if self
            .settings_path
            .as_ref()
            .is_some_and(|path| path != &settings.path)
        {
            return Err(ActionError::new(
                "preset_profile",
                "Preset data belongs to a different active settings path",
            ));
        }
        if settings.brush_presets.next_id < self.brush_presets.next_id {
            return Err(ActionError::new("preset_library_changed","The replacement profile would reuse committed preset IDs; restart to load a replacement identity space"));
        }
        if self.brush_presets != settings.brush_presets {
            if self.stroke.active()
                || self.pointer_checkpoint.is_some()
                || self.doc.history_group_active()
            {
                return Err(ActionError::new(
                    "stroke_active",
                    "Finish the current contact before changing the preset library",
                ));
            }
            self.preset_library_token
                .checked_add(1)
                .filter(|value| *value <= 9_007_199_254_740_991)
                .ok_or_else(|| {
                    ActionError::new("preset_library_limit", "Preset library token limit reached")
                })?;
        }
        Ok(())
    }
}
