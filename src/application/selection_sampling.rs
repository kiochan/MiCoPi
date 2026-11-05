//! Session-local selection controls; document commands receive explicit options.
use super::Session;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SelectionSampling {
    pub tolerance: u8,
    pub contiguous: bool,
    pub sample_merged: bool,
}
impl Default for SelectionSampling {
    fn default() -> Self {
        Self {
            tolerance: 32,
            contiguous: true,
            sample_merged: false,
        }
    }
}
impl Session {
    pub fn selection_sampling(&self) -> SelectionSampling {
        serde_json::from_value(self.ui["selection.options"].clone()).unwrap_or_default()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::Action;
    use serde_json::json;
    #[test]
    fn sampling_controls_validate_publish_state_and_require_desktop() {
        let mut s = Session::new(crate::plugins::Registry::bundled()).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let defaults = json!({"tolerance":32,"contiguous":true,"sample_merged":false});
        assert_eq!(s.state()["selection_sampling"], defaults);
        let before = s.state();
        for (id, args) in [
            ("toolbox.mode.selection_wand", json!({})),
            ("toolbox.selection.options", defaults.clone()),
        ] {
            assert_eq!(
                s.apply(Action::new(id, args)).unwrap_err().code,
                "ui_unavailable"
            );
            assert_eq!(s.state(), before);
        }
        s.runtime = "desktop";
        for bad in [
            json!({"tolerance":256,"contiguous":true,"sample_merged":false}),
            json!({"tolerance":-1}),
            json!({"tolerance":3,"contiguous":"yes"}),
            json!({"tolerance":0,"unknown":true}),
        ] {
            assert!(s
                .apply(Action::new(
                    "ui.dispatch",
                    json!({"id":"selection.options","value":bad})
                ))
                .is_err());
            assert_eq!(s.state(), before);
        }
        let changed = json!({"tolerance":7,"contiguous":false,"sample_merged":true});
        s.apply(Action::new("toolbox.selection.options", changed.clone()))
            .unwrap();
        assert_eq!(s.state()["selection_sampling"], changed);
        assert_eq!(s.state()["ui"]["selection.options"], changed);
        assert!(!s.doc.selection().current.is_some());
        assert!(s.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
}
