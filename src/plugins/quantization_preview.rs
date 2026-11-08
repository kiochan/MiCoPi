//! Fenced direct quantization previews share immutable color staging.
use super::color_preview::{self, Cost, Scope};
use crate::application::{Action, ActionError, ActionResult, Session};
use crate::engine::quantization::{Effect, Quantization};
use micopi_plugin_api as abi;
use serde::Deserialize;
use serde_json::{json, Value};
pub(crate) fn action(effect: Effect) -> &'static str {
    match effect {
        Effect::Threshold => "layer.threshold",
        Effect::Posterize => "layer.posterize",
    }
}
pub(crate) fn key(effect: Effect) -> &'static str {
    match effect {
        Effect::Threshold => "level",
        Effect::Posterize => "levels",
    }
}
pub(crate) fn parameters_schema(effect: Effect, required: bool) -> Value {
    let (minimum, maximum, default, title) = match effect {
        Effect::Threshold => (0, 255, 128, "Threshold Level"),
        Effect::Posterize => (2, 256, 4, "Posterize Levels"),
    };
    let key = key(effect);
    let mut schema = json!({"type":"object","additionalProperties":false,"properties":{
        (key):{"type":"integer","minimum":minimum,"maximum":maximum,"default":default,"title":title}}});
    if required {
        schema["required"] = json!([key]);
    }
    schema
}
pub(crate) fn parse(effect: Effect, args: &Value) -> Result<Quantization, ActionError> {
    let error = || ActionError::new("invalid_arguments", "Invalid quantization parameters");
    let object = args.as_object().ok_or_else(error)?;
    let key = key(effect);
    if object.keys().any(|name| name != key) {
        return Err(error());
    }
    let value = if let Some(value) = object.get(key) {
        value
            .as_f64()
            .filter(|v| v.is_finite() && v.fract() == 0. && (0. ..=256.).contains(v))
            .ok_or_else(error)? as u32
    } else {
        match effect {
            Effect::Threshold => 128,
            Effect::Posterize => 4,
        }
    };
    let parameters = Quantization { effect, value };
    parameters.validate().map_err(|_| error())?;
    Ok(parameters)
}
pub(crate) fn arguments(parameters: Quantization) -> Value {
    json!({(key(parameters.effect)):parameters.value})
}
pub(crate) fn schema(effect: Effect, dialog: bool) -> Value {
    if dialog {
        return json!({"type":"object","additionalProperties":false,"properties":{"parameters":parameters_schema(effect,false)}});
    }
    json!({"type":"object","additionalProperties":false,"required":["document_identity","document_revision","layer_id","parameters"],"properties":{
        "document_identity":{"type":"integer","minimum":1},"document_revision":{"type":"integer","minimum":0},"layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},
        "parameters":parameters_schema(effect,true)}})
}
pub(crate) fn request_dialog(s: &mut Session, args: Value, effect: Effect) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new(
            "ui_unavailable",
            "Quantization controls require the desktop; use the direct layer action headlessly",
        ));
    }
    let parameters = parse(effect, args.get("parameters").unwrap_or(&json!({})))?;
    let id = match effect {
        Effect::Threshold => "threshold.open",
        Effect::Posterize => "posterize.open",
    };
    color_preview::queue_dialog(s, Action::new(action(effect), arguments(parameters)), id)
}
pub(crate) fn render(
    s: &Session,
    scope: Scope,
    parameters: Quantization,
) -> Result<(Vec<u32>, Cost), ActionError> {
    let apply = Action {
        action: action(parameters.effect).into(),
        args: arguments(parameters),
        expected_revision: Some(scope.document_revision),
    };
    let plan = color_preview::admit(s, scope, &apply, false, "quantization_preview")?;
    if !s
        .doc
        .renderer
        .as_ref()
        .is_some_and(|r| r.supports_quantization())
    {
        return Err(ActionError::new(
            "quantization_unavailable",
            "Native quantization preview is unavailable",
        ));
    }
    let nodes = [
        abi::CompositeNode {
            kind: 0,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: parameters.instruction(),
            a: 0,
            b: 0,
            mode: parameters.value,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 3,
            a: 1,
            b: 0,
            mode: 0,
            amount: 1.,
        },
    ];
    plan.render(|renderer, layers, output| {
        renderer.render_quantization(
            &nodes,
            layers,
            &[],
            &[],
            &[],
            s.doc.width as u32,
            s.doc.height as u32,
            output,
        )
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    document_identity: u64,
    document_revision: u64,
    layer_id: u64,
    parameters: Value,
}
pub(crate) fn execute(s: &Session, args: Value, effect: Effect) -> ActionResult {
    let request: Request = serde_json::from_value(args)?;
    let parameters = parse(effect, &request.parameters)?;
    let scope = Scope {
        document_identity: request.document_identity,
        document_revision: request.document_revision,
        layer_id: request.layer_id,
    };
    let (pixels, cost) = render(s, scope, parameters)?;
    let bytes = crate::application::png_io::encode(
        s.doc.width,
        s.doc.height,
        pixels.into_iter(),
        s.doc.info().export_density(),
    )?;
    let apply = Action {
        action: action(effect).into(),
        args: arguments(parameters),
        expected_revision: Some(scope.document_revision),
    };
    Ok(
        json!({"document_identity":scope.document_identity,"revision":scope.document_revision,"layer_id":scope.layer_id,
        "parameters":arguments(parameters),"width":s.doc.width,"height":s.doc.height,"mimeType":"image/png","data":crate::application::codec::base64(&bytes),"cost":cost,"apply":apply}),
    )
}
