//! JSON adapter over shared native HSL math; no host dependency.
pub(super) use super::hsl_math::Parameters;
use serde_json::Value;
pub(super) fn parse(value: &Value) -> Result<Parameters, String> {
    let number = |name: &str, bound: f64| match value.get(name) {
        None => Ok(0.),
        Some(value) => value
            .as_f64()
            .filter(|v| v.is_finite() && (-bound..=bound).contains(v))
            .ok_or_else(|| format!("Invalid {name}")),
    };
    Parameters::new(
        number("hue", 180.)?,
        number("saturation", 100.)?,
        number("lightness", 100.)?,
    )
    .ok_or_else(|| "Invalid HSL parameters".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn invalid_parameters_reject_before_pixel_processing() {
        for value in [
            json!({"hue":181}),
            json!({"saturation":-101}),
            json!({"lightness":101}),
            json!({"hue":"30"}),
            json!({"saturation":null}),
        ] {
            assert!(parse(&value).is_err());
        }
    }
}
