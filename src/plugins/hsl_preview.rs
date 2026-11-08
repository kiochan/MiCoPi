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
    s.validate_action(&Action::new("layer.hue_saturation", json!(hsl)))?;
    if !s
        .registry
        .native_actions
        .contains_key("layer.hue_saturation")
    {
        return Err(ActionError::new(
            "native_missing",
            "Native HSL action is not loaded",
        ));
    }
    if s.ui_effects.len() >= 128 {
        return Err(ActionError::new(
            "presentation_queue_full",
            "Wait for queued presentation commands before retrying",
        ));
    }
    let scope = json!({"queued":true,"document_identity":s.doc.identity,
        "revision":s.revision,"layer_id":s.doc.layer_id(s.doc.active)});
    let mut effect = scope.clone();
    effect["id"] = json!("hsl.open");
    effect["value"] = json!(hsl);
    s.ui_effects.push_back(effect);
    Ok(scope)
}
fn failure(message: impl Into<String>) -> ActionError {
    ActionError::new("hsl_preview", message)
}
impl Request {
    fn validate(self, s: &Session) -> Result<(), ActionError> {
        if self.document_identity != s.doc.identity || self.document_revision != s.revision {
            return Err(ActionError::new(
                "revision_conflict",
                "The color preview belongs to an older document or revision",
            ));
        }
        if self.layer_id != s.doc.layer_id(s.doc.active) {
            return Err(ActionError::new(
                "invalid_layer",
                "The color preview must name the active raster layer",
            ));
        }
        if s.stroke.active() || s.pointer_checkpoint.is_some() {
            return Err(ActionError::new(
                "stroke_active",
                "Finish the current contact before requesting a color preview",
            ));
        }
        self.hsl.validate().map_err(failure)?;
        super::PixelAccess::Pixels.require(&s.doc, s.doc.active)?;
        // Validate the exact eventual command, without invoking it or creating
        // its instance. Presentation and automation cannot preview an absent action.
        s.validate_action(&self.apply_action())?;
        if !s
            .registry
            .native_actions
            .contains_key("layer.hue_saturation")
        {
            return Err(ActionError::new(
                "native_missing",
                "Native HSL action is not loaded",
            ));
        }
        if s.doc.has_adjustments() {
            super::adjustments::validate_document(&s.registry, &s.doc)?;
        }
        if !s.doc.renderer.as_ref().is_some_and(|r| r.supports_hsl()) {
            return Err(ActionError::new(
                "hsl_unavailable",
                "Native HSL preview is unavailable",
            ));
        }
        Ok(())
    }
    pub(crate) fn apply_action(self) -> Action {
        Action {
            action: "layer.hue_saturation".into(),
            args: json!(self.hsl),
            expected_revision: Some(self.document_revision),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Cost {
    pub source_scratch_bytes: usize,
    pub composite_bytes: usize,
    pub mask_effect_bytes: usize,
    pub pixel_scratch_bytes: usize,
}
/// Explicit cold preflight. This does not warm caches, finish input or allocate
/// a pixel surface. PNG/transport allocations are not included in this cost.
pub(crate) fn plan(s: &Session, request: Request) -> Result<Cost, ActionError> {
    request.validate(s)?;
    let owner = s.doc.owner_preview_cost(&[]).map_err(failure)?;
    let source_scratch_bytes = s.doc.width * s.doc.height * 4;
    Ok(Cost {
        source_scratch_bytes,
        composite_bytes: owner.composite_bytes,
        mask_effect_bytes: owner.effect_bytes,
        pixel_scratch_bytes: source_scratch_bytes + owner.extra_bytes,
    })
}

/// Render the edited raw raster privately, clip selection once with the same
/// DLL as Apply, then compose through the existing immutable owner-preview path.
/// The caller owns the returned surface; dropping it cancels the draft.
pub(crate) fn render(s: &Session, request: Request) -> Result<(Vec<u32>, Cost), ActionError> {
    let cost = plan(s, request)?;
    let source = &s.doc.layers[s.doc.active].pixels;
    let mut staged = vec![0; source.len()];
    let mut output = abi::Pixels {
        data: staged.as_mut_ptr(),
        len: staged.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let layers = [abi::CompositeLayer {
        data: source.as_ptr(),
        len: source.len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    }];
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
    s.doc
        .renderer
        .as_ref()
        .unwrap()
        .render_hsl(
            &nodes,
            &layers,
            &[],
            &[],
            &[request.hsl.parameters()],
            s.doc.width as u32,
            s.doc.height as u32,
            &mut output,
        )
        .map_err(failure)?;
    // Composition canonicalizes transparent RGB. Direct edits preserve it,
    // including under transparency lock; restore only those immutable bytes.
    for (before, after) in source.iter().zip(&mut staged) {
        if before >> 24 == 0 {
            *after = *before;
        }
    }
    super::native::clip_region(s, 0, 0, s.doc.width, s.doc.height, &mut staged).map_err(failure)?;
    let pixels = s
        .doc
        .preview_layer_color(request.layer_id, &staged)
        .map_err(failure)?;
    Ok((pixels, cost))
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
