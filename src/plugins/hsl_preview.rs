//! Explicit read-only color drafts. Pixel math and selection mixing stay native.
use crate::application::{Action, ActionError, ActionResult, Session};
use crate::engine::hsl::Hsl;
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub document_identity: u64,
    pub document_revision: u64,
    pub layer_id: u64,
    pub hsl: Hsl,
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,
        "required":["document_identity","document_revision","layer_id","hsl"],
        "properties":{
            "document_identity":{"type":"integer","minimum":1},
            "document_revision":{"type":"integer","minimum":0},
            "layer_id":{"type":"integer","minimum":1,"maximum":4294967295u64},
            "hsl":Hsl::schema()}})
}
/// Presentation-only request. Opening controls never invokes the pixel action;
/// a captured scope prevents a queued request from silently rebasing later.
pub(crate) fn request_dialog(s: &mut Session, args: Value) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new("ui_unavailable", "Hue/Saturation controls require the desktop; use layer.hue_saturation for headless editing"));
    }
    let hsl: Hsl =
        serde_json::from_value(args.get("parameters").cloned().unwrap_or_else(|| json!({})))?;
    hsl.validate().map_err(failure)?;
    super::color_preview::queue_dialog(
        s,
        Action::new("layer.hue_saturation", json!(hsl)),
        "hsl.open",
    )
}
fn failure(message: impl Into<String>) -> ActionError {
    ActionError::new("hsl_preview", message)
}
impl Request {
    fn scope(self) -> super::color_preview::Scope {
        super::color_preview::Scope {
            document_identity: self.document_identity,
            document_revision: self.document_revision,
            layer_id: self.layer_id,
        }
    }
    fn admission(self, s: &Session) -> Result<super::color_preview::Plan<'_>, ActionError> {
        self.scope().check(s)?;
        self.hsl.validate().map_err(failure)?;
        super::color_preview::admit(s, self.scope(), &self.apply_action(), true, "hsl_preview")
    }
    pub(crate) fn apply_action(self) -> Action {
        Action {
            action: "layer.hue_saturation".into(),
            args: json!(self.hsl),
            expected_revision: Some(self.document_revision),
        }
    }
}

pub(crate) use super::color_preview::Cost;
/// Cold preflight without warming caches, finishing input or allocating pixels.
pub(crate) fn plan(s: &Session, request: Request) -> Result<Cost, ActionError> {
    Ok(request.admission(s)?.cost)
}
/// The common admitted pipeline retains selection, masks and private storage.
pub(crate) fn render(s: &Session, request: Request) -> Result<(Vec<u32>, Cost), ActionError> {
    let plan = request.admission(s)?;
    let nodes = [
        abi::CompositeNode {
            kind: 0,
            a: 0,
            b: 0,
            mode: 0,
            amount: 1.,
        },
        abi::CompositeNode {
            kind: 11,
            a: 0,
            b: 0,
            mode: 0,
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
        renderer.render_hsl(
            &nodes,
            layers,
            &[],
            &[],
            &[request.hsl.parameters()],
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
        "layer_id":request.layer_id,"hsl":request.hsl,"width":s.doc.width,"height":s.doc.height,
        "mimeType":"image/png","data":crate::application::codec::base64(&bytes),
        "cost":cost,"apply":request.apply_action()}),
    )
}
