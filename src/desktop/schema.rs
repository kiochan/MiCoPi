//! Shared schema controls for plugin parameters. Editing never invokes an action.
use super::*;
use serde_json::{json, Value};
pub(super) fn default_value(schema: &Value) -> Value {
    if let Some(value) = schema.get("default") {
        return value.clone();
    }
    if let Some(value) = schema["enum"].as_array().and_then(|a| a.first()) {
        return value.clone();
    }
    match schema["type"].as_str() {
        Some("boolean") => json!(false),
        Some("string") => json!(""),
        Some("array") => json!([]),
        Some("object") => json!({}),
        Some("integer") | Some("number") => {
            let n = 0f64
                .max(schema["minimum"].as_f64().unwrap_or(f64::MIN))
                .min(schema["maximum"].as_f64().unwrap_or(f64::MAX));
            if schema["type"] == "integer" {
                json!(n as i64)
            } else {
                json!(n)
            }
        }
        _ => Value::Null,
    }
}
pub(super) fn initial_parameters(def: &crate::plugins::nodes::Resolved) -> Value {
    let mut value = def.defaults.clone();
    if let Some(props) = def.parameters_schema["properties"].as_object() {
        for (key, schema) in props {
            if value.get(key).is_none()
                && (schema.get("default").is_some()
                    || def.parameters_schema["required"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == key)))
            {
                value[key] = default_value(schema);
            }
        }
    }
    value
}
pub(super) fn edit(
    ui: &mut Ui,
    key: impl std::hash::Hash,
    schema: &Value,
    value: &mut Value,
) -> bool {
    ui.push_id(key, |ui| {
        if let Some(values) = schema["enum"].as_array() {
            let before = value.clone();
            ComboBox::from_id_salt("enum")
                .width(130.)
                .selected_text(tr(label(value)))
                .show_ui(ui, |ui| {
                    for item in values {
                        ui.selectable_value(value, item.clone(), tr(label(item)));
                    }
                });
            return before != *value;
        }
        match schema["type"].as_str() {
            Some("boolean") => {
                let mut v = value.as_bool().unwrap_or(false);
                let r = ui.checkbox(&mut v, tr(""));
                if r.changed() {
                    *value = json!(v);
                }
                r.changed()
            }
            Some("string") => {
                let mut v = value.as_str().unwrap_or("").to_owned();
                let r = ui.add(TextEdit::singleline(&mut v).desired_width(150.));
                if r.changed() {
                    *value = json!(v);
                }
                r.changed()
            }
            Some("number") | Some("integer") => {
                let mut v = value.as_f64().unwrap_or(0.);
                let integer = schema["type"] == "integer";
                let r = ui.add(
                    DragValue::new(&mut v)
                        .speed(if integer { 1. } else { 0.01 })
                        .range(
                            schema["minimum"].as_f64().unwrap_or(-1e12)
                                ..=schema["maximum"].as_f64().unwrap_or(1e12),
                        )
                        .max_decimals(if integer { 0 } else { 4 }),
                );
                if r.changed() {
                    *value = if integer {
                        json!(v.round() as i64)
                    } else {
                        json!(v)
                    };
                }
                r.changed()
            }
            _ => {
                let id = ui.id().with("json");
                let mut text = ui
                    .ctx()
                    .data_mut(|d| d.get_temp::<String>(id))
                    .unwrap_or_else(|| value.to_string());
                let r = ui.add(
                    TextEdit::multiline(&mut text)
                        .desired_width(190.)
                        .desired_rows(2)
                        .font(TextStyle::Monospace),
                );
                let mut changed = false;
                if r.changed() {
                    match serde_json::from_str::<Value>(&text) {
                        Ok(next) => {
                            changed = next != *value;
                            *value = next;
                        }
                        Err(_) => {
                            ui.colored_label(ui.visuals().warn_fg_color, tr("Incomplete JSON"));
                        }
                    }
                }
                ui.ctx().data_mut(|d| d.insert_temp(id, text));
                changed
            }
        }
    })
    .inner
}
fn label(value: &Value) -> String {
    if let Ok(mode) = serde_json::from_value::<crate::engine::graph::BlendMode>(value.clone()) {
        return mode.title().into();
    }
    if let Ok(filter) = serde_json::from_value::<crate::engine::graph::Filter>(value.clone()) {
        return filter.title().into();
    }
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
