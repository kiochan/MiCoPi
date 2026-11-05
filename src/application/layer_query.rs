//! Bounded layer discovery. Filtering never changes layer visibility or selection.
use super::{ActionError, Session};
use crate::engine::document::LayerColor;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Query {
    query: String,
    color_label: Option<LayerColor>,
    visible: Option<bool>,
    locked: Option<bool>,
    alpha_locked: Option<bool>,
    pixel_locked: Option<bool>,
    position_locked: Option<bool>,
    selected: Option<bool>,
}
fn parse(args: &Value) -> Result<Query, ActionError> {
    if args
        .as_object()
        .is_none_or(|m| m.values().any(Value::is_null))
    {
        return Err(ActionError::new(
            "invalid_layer_query",
            "Layer query fields must be a non-null object",
        ));
    }
    let q: Query = serde_json::from_value(args.clone())
        .map_err(|e| ActionError::new("invalid_layer_query", e.to_string()))?;
    if q.query.len() > 512 || q.query.chars().any(char::is_control) {
        return Err(ActionError::new(
            "invalid_layer_query",
            "Layer search must be single-line and at most 512 UTF-8 bytes",
        ));
    }
    Ok(q)
}
pub(crate) fn validate(args: &Value) -> Result<(), ActionError> {
    parse(args).map(|_| ())
}
pub(crate) fn execute(s: &Session, args: &Value) -> Result<Value, ActionError> {
    let q = parse(args)?;
    let needle = q.query.to_lowercase();
    let matches = |value: bool, wanted: Option<bool>| wanted.is_none_or(|v| value == v);
    let layers: Vec<_> = (0..s.doc.layers.len())
        .filter(|&i| {
            let alpha = s.doc.alpha_locked(i);
            let pixels = s.doc.pixel_locked(i);
            let position = s.doc.position_locked(i);
            (needle.is_empty() || s.doc.layer_name(i).to_lowercase().contains(&needle))
                && q.color_label.is_none_or(|v| s.doc.color_label(i) == v)
                && matches(s.doc.layers[i].visible, q.visible)
                && matches(alpha || pixels || position, q.locked)
                && matches(alpha, q.alpha_locked)
                && matches(pixels, q.pixel_locked)
                && matches(position, q.position_locked)
                && matches(s.doc.layer_selected(i), q.selected)
        })
        .map(|i| s.layer_state(i))
        .collect();
    Ok(
        json!({"total":s.doc.layers.len(),"matched":layers.len(),"layers":layers,
        "revision":s.revision,"drawing":s.is_drawing(),
        "selected_layer_ids":s.doc.selected_layer_ids(),"active_layer_id":s.doc.layer_id(s.doc.active)}),
    )
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","properties":{
        "query":{"type":"string","description":"Case-insensitive Unicode substring of public layer names; at most 512 UTF-8 bytes, no control characters."},
        "color_label":{"type":"string","enum":["none","red","orange","yellow","green","blue","violet","gray"]},
        "visible":{"type":"boolean"},"locked":{"type":"boolean","description":"Any transparency, pixel or position lock. All supplied filters are combined with AND."},
        "alpha_locked":{"type":"boolean"},"pixel_locked":{"type":"boolean"},"position_locked":{"type":"boolean"},
        "selected":{"type":"boolean","description":"Matches membership in the current layer selection, independently of the active painting layer."}
    },"required":[],"additionalProperties":false})
}
