//! Serializable intent; all quantization pixel arithmetic remains native.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Threshold,
    Posterize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantization {
    pub effect: Effect,
    pub value: u32,
}
impl Quantization {
    pub fn schema() -> serde_json::Value {
        serde_json::json!({"type":"object","additionalProperties":false,"required":["effect","value"],"properties":{
            "effect":{"type":"string","enum":["threshold","posterize"]},"value":{"type":"integer","minimum":0,"maximum":256}}})
    }
    pub fn validate(self) -> Result<(), String> {
        if match self.effect {
            Effect::Threshold => self.value <= 255,
            Effect::Posterize => (2..=256).contains(&self.value),
        } {
            Ok(())
        } else {
            Err("Invalid Threshold/Posterize parameter range".into())
        }
    }
    pub fn title(self) -> &'static str {
        match self.effect {
            Effect::Threshold => "Threshold",
            Effect::Posterize => "Posterize",
        }
    }
    pub fn instruction(self) -> u32 {
        match self.effect {
            Effect::Threshold => 12,
            Effect::Posterize => 13,
        }
    }
}
