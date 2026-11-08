//! Immutable pixel-preview pipeline shared by native color commands.
use crate::application::{Action, ActionError, ActionResult, Session};
use crate::engine::graph::Renderer;
use micopi_plugin_api as abi;
use serde::Serialize;
use serde_json::json;

/// Queue controls, never pixel execution. Delivery retains the captured source.
pub(crate) fn queue_dialog(s: &mut Session, action: Action, id: &str) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new(
            "ui_unavailable",
            "Color controls require the desktop",
        ));
    }
    s.validate_action(&action)?;
    if !s.registry.native_actions.contains_key(&action.action) {
        return Err(ActionError::new(
            "native_missing",
            "Native color action is not loaded",
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
    effect["id"] = json!(id);
    effect["value"] = action.args;
    s.ui_effects.push_back(effect);
    Ok(scope)
}

#[derive(Clone, Copy)]
pub(crate) struct Scope {
    pub document_identity: u64,
    pub document_revision: u64,
    pub layer_id: u64,
}
impl Scope {
    pub(crate) fn check(self, s: &Session) -> Result<(), ActionError> {
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
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct Cost {
    pub source_scratch_bytes: usize,
    pub composite_bytes: usize,
    pub mask_effect_bytes: usize,
    pub pixel_scratch_bytes: usize,
}
/// This immutable loan keeps checked scope and permissions fixed until render.
pub(crate) struct Plan<'a> {
    session: &'a Session,
    scope: Scope,
    pub cost: Cost,
    error_code: &'static str,
}
pub(crate) fn admit<'a>(
    s: &'a Session,
    scope: Scope,
    action: &Action,
    requires_hsl: bool,
    error_code: &'static str,
) -> Result<Plan<'a>, ActionError> {
    scope.check(s)?;
    s.validate_action(action)?;
    if !s.registry.native_actions.contains_key(&action.action) {
        return Err(ActionError::new(
            "native_missing",
            "Native color action is not loaded",
        ));
    }
    s.registry.actions[&action.action]
        .effective_pixel_access()
        .require_write(&s.doc, s.doc.active)?;
    if s.doc.has_adjustments() {
        super::adjustments::validate_document(&s.registry, &s.doc)?;
    }
    if requires_hsl && !s.doc.renderer.as_ref().is_some_and(|r| r.supports_hsl()) {
        return Err(ActionError::new(
            "hsl_unavailable",
            "Native HSL preview is unavailable",
        ));
    }
    if s.doc.renderer.is_none() {
        return Err(ActionError::new(
            "renderer_missing",
            "A compositor is required for color preview",
        ));
    }
    let owner = s
        .doc
        .owner_preview_cost(&[])
        .map_err(|e| ActionError::new(error_code, e))?;
    let source_scratch_bytes = s.doc.width * s.doc.height * 4;
    Ok(Plan {
        session: s,
        scope,
        error_code,
        cost: Cost {
            source_scratch_bytes,
            composite_bytes: owner.composite_bytes,
            mask_effect_bytes: owner.effect_bytes,
            pixel_scratch_bytes: source_scratch_bytes + owner.extra_bytes,
        },
    })
}
impl Plan<'_> {
    /// Kernels receive immutable raster descriptors and private output. Selection
    /// mixing and document graph/mask composition are common to every color edit.
    pub(crate) fn render(
        self,
        kernel: impl FnOnce(
            &dyn Renderer,
            &[abi::CompositeLayer],
            &mut abi::Pixels,
        ) -> Result<(), String>,
    ) -> Result<(Vec<u32>, Cost), ActionError> {
        let s = self.session;
        let failure = |message| ActionError::new(self.error_code, message);
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
        kernel(
            s.doc.renderer.as_ref().unwrap().as_ref(),
            &layers,
            &mut output,
        )
        .map_err(failure)?;
        // Restore hidden bytes canonicalized by compositors before mixing once.
        for (before, after) in source.iter().zip(&mut staged) {
            if before >> 24 == 0 {
                *after = *before;
            }
        }
        super::native::clip_region(s, 0, 0, s.doc.width, s.doc.height, &mut staged)
            .map_err(failure)?;
        let pixels = s
            .doc
            .preview_layer_color(self.scope.layer_id, &staged)
            .map_err(failure)?;
        Ok((pixels, self.cost))
    }
}
