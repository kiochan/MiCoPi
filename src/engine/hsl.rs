//! Typed HSL document intent. Pixel arithmetic remains in native plugins.
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Hsl {
    pub hue: f64,
    pub saturation: f64,
    pub lightness: f64,
}
impl Hsl {
    pub fn schema() -> serde_json::Value {
        serde_json::json!({"type":"object","additionalProperties":false,"required":["hue","saturation","lightness"],"properties":{
            "hue":{"type":"number","minimum":-180,"maximum":180,"title":"Hue Shift"},
            "saturation":{"type":"number","minimum":-100,"maximum":100,"title":"Saturation Adjustment"},
            "lightness":{"type":"number","minimum":-100,"maximum":100,"title":"Lightness Adjustment"}}})
    }
    pub fn validate(self) -> Result<(), String> {
        if !self.hue.is_finite()
            || !(-180. ..=180.).contains(&self.hue)
            || !self.saturation.is_finite()
            || !(-100. ..=100.).contains(&self.saturation)
            || !self.lightness.is_finite()
            || !(-100. ..=100.).contains(&self.lightness)
        {
            return Err("HSL requires finite hue within 180 degrees and saturation/lightness within 100 percent".into());
        }
        Ok(())
    }
    pub fn parameters(self) -> abi::HslParameters {
        abi::HslParameters {
            hue: self.hue,
            saturation: self.saturation,
            lightness: self.lightness,
        }
    }
}
