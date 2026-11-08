//! Brightness/Contrast preview parameters and native legacy-plan adapter.
use super::color_preview::{self, Cost, Scope};
use crate::application::{Action, ActionError, ActionResult, Session};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Parameters {
    pub brightness: f64,
    pub contrast: f64,
}
impl Parameters {
    pub(crate) fn schema() -> Value {
        json!({"type":"object","additionalProperties":false,"required":["brightness","contrast"],"properties":{
            "brightness":{"type":"number","minimum":-100,"maximum":100,"title":"Brightness"},
            "contrast":{"type":"number","minimum":-100,"maximum":100,"title":"Contrast"}}})
    }
}
pub(crate) fn request_dialog(s: &mut Session, args: Value) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new("ui_unavailable", "Brightness/Contrast controls require the desktop; use layer.brightness_contrast for headless editing"));
    }
    let parameters: Parameters =
        serde_json::from_value(args.get("parameters").cloned().unwrap_or_else(|| json!({})))?;
    color_preview::queue_dialog(
        s,
        Action::new("layer.brightness_contrast", json!(parameters)),
        "tonal.open",
    )
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub document_identity: u64,
    pub document_revision: u64,
    pub layer_id: u64,
    pub parameters: Parameters,
}
impl Request {
    fn scope(self) -> Scope {
        Scope {
            document_identity: self.document_identity,
            document_revision: self.document_revision,
            layer_id: self.layer_id,
        }
    }
    pub(crate) fn apply_action(self) -> Action {
        Action {
            action: "layer.brightness_contrast".into(),
            args: json!(self.parameters),
            expected_revision: Some(self.document_revision),
        }
    }
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["document_identity","document_revision","layer_id","parameters"],"properties":{
        "document_identity":{"type":"integer","minimum":1},"document_revision":{"type":"integer","minimum":0},
        "layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},"parameters":Parameters::schema()}})
}
pub(crate) fn render(s: &Session, request: Request) -> Result<(Vec<u32>, Cost), ActionError> {
    let plan = color_preview::admit(
        s,
        request.scope(),
        &request.apply_action(),
        false,
        "brightness_contrast_preview",
    )?;
    let nodes = [
        abi::CompositeNode {
            kind: 0,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 1,
            a: 0,
            b: 0,
            mode: 2,
            amount: (request.parameters.brightness / 100.) as f32,
        },
        abi::CompositeNode {
            kind: 1,
            a: 1,
            b: 0,
            mode: 3,
            amount: (request.parameters.contrast / 100.) as f32,
        },
        abi::CompositeNode {
            kind: 3,
            a: 2,
            b: 0,
            mode: 0,
            amount: 1.,
        },
    ];
    plan.render(|renderer, layers, output| {
        renderer.render(
            &nodes,
            layers,
            s.doc.width as u32,
            s.doc.height as u32,
            output,
        )
    })
}
pub(crate) fn execute(s: &Session, args: Value) -> ActionResult {
    let request: Request = serde_json::from_value(args)?;
    let (pixels, cost) = render(s, request)?;
    let bytes = crate::application::png_io::encode(
        s.doc.width,
        s.doc.height,
        pixels.into_iter(),
        s.doc.info().export_density(),
    )?;
    Ok(
        json!({"document_identity":request.document_identity,"revision":request.document_revision,
        "layer_id":request.layer_id,"parameters":request.parameters,"width":s.doc.width,"height":s.doc.height,
        "mimeType":"image/png","data":crate::application::codec::base64(&bytes),"cost":cost,"apply":request.apply_action()}),
    )
}
