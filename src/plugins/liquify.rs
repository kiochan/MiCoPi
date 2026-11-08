//! Captured whole-raster deformation drafts; pixels remain in independent owners.
use crate::application::{Action, ActionError, ActionResult, Session};
use crate::engine::{
    raster_masks::SamplePlane,
    transform::{Method, OwnerScope},
};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const FIELD_LIMIT: usize = 64 * 1024 * 1024;
const MAX_STEPS: usize = 8192;
fn spacing() -> u32 {
    4
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Push {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
    pub radius: f32,
    pub strength: f32,
}
impl Push {
    fn native(self) -> abi::LiquifyPush {
        abi::LiquifyPush {
            x: self.x,
            y: self.y,
            dx: self.dx,
            dy: self.dy,
            radius: self.radius,
            strength: self.strength,
        }
    }
    fn validate(self, spacing: u32) -> Result<(), String> {
        if ![self.x, self.y, self.dx, self.dy, self.radius, self.strength]
            .iter()
            .all(|v| v.is_finite())
            || self.x.abs() > 1_048_576.
            || self.y.abs() > 1_048_576.
            || self.radius < spacing as f32
            || self.radius > 2048.
            || !(0. ..=1.).contains(&self.strength)
            || self.dx.hypot(self.dy) > self.radius
        {
            return Err("Invalid Liquify push geometry or strength".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub document_identity: u64,
    pub document_revision: u64,
    #[serde(deserialize_with = "decode_ids")]
    pub layer_ids: Vec<u64>,
    #[serde(default = "spacing")]
    pub spacing: u32,
    #[serde(default)]
    pub method: Method,
    #[serde(default)]
    pub steps: Vec<Vec<Push>>,
}
fn decode_ids<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    super::layer_links::ids(&value, "invalid_arguments").map_err(serde::de::Error::custom)
}
fn failure(message: impl Into<String>) -> ActionError {
    ActionError::new("liquify", message)
}
pub(crate) fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["document_identity","document_revision","layer_ids"],"properties":{
        "document_identity":{"type":"integer","minimum":1},"document_revision":{"type":"integer","minimum":0},
        "layer_ids":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"integer","minimum":1,"maximum":4294967295u64},"description":"Exact whole rasters and their linked raw masks; no implicit group/link expansion. Pixel selection is preserved and does not clip this whole-owner operation."},
        "spacing":{"type":"integer","minimum":1,"maximum":64,"default":4},"method":{"type":"string","enum":["nearest","bilinear"],"default":"bilinear"},
        "steps":{"type":"array","maxItems":8192,"items":{"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["x","y","dx","dy","radius","strength"],"properties":{
            "x":{"type":"number","minimum":-1048576,"maximum":1048576},"y":{"type":"number","minimum":-1048576,"maximum":1048576},
            "dx":{"type":"number","minimum":-2048,"maximum":2048},"dy":{"type":"number","minimum":-2048,"maximum":2048},
            "radius":{"type":"number","minimum":1,"maximum":2048},"strength":{"type":"number","minimum":0,"maximum":1}}}}}}})
}
impl Request {
    pub(crate) fn apply_action(&self) -> Action {
        Action {
            action: "canvas.liquify".into(),
            args: json!(self),
            expected_revision: Some(self.document_revision),
        }
    }
    fn check(&self, s: &Session) -> Result<(), String> {
        if self.document_identity != s.doc.identity || self.document_revision != s.revision {
            return Err("Liquify source changed; reopen or refresh the draft".into());
        }
        if self.layer_ids.is_empty()
            || self.layer_ids.len() > 16
            || self.layer_ids.iter().enumerate().any(|(i, id)| {
                *id == 0 || *id > u32::MAX as u64 || self.layer_ids[..i].contains(id)
            })
        {
            return Err("Supply one through sixteen unique raster IDs".into());
        }
        if !(1..=64).contains(&self.spacing) || self.steps.len() > MAX_STEPS {
            return Err("Liquify spacing or step count exceeds its limit".into());
        }
        for arms in &self.steps {
            if !(1..=4).contains(&arms.len()) {
                return Err("Each Liquify step needs one through four arms".into());
            }
            for p in arms {
                p.validate(self.spacing)?;
            }
        }
        Ok(())
    }
}
pub(crate) struct Plan {
    pub scope: OwnerScope,
    pub field_bytes: usize,
    pub staged_bytes: usize,
}
pub(crate) fn plan(s: &Session, r: &Request) -> Result<Plan, String> {
    r.check(s)?;
    let scope = s.doc.capture_owner_transform_scope(&r.layer_ids, None)?;
    let plugin = s
        .registry
        .native_plugins
        .get("painting")
        .filter(|p| p.supports_liquify())
        .ok_or("Painting DLL does not support Liquify")?;
    let _ = plugin;
    let nodes = s
        .doc
        .width
        .div_ceil(r.spacing as usize)
        .checked_add(1)
        .and_then(|w| {
            s.doc
                .height
                .div_ceil(r.spacing as usize)
                .checked_add(1)
                .and_then(|h| w.checked_mul(h))
        })
        .ok_or("Liquify grid extent overflows")?;
    let field_bytes = nodes
        .checked_mul(8 + std::mem::size_of::<(usize, [f32; 2])>())
        .ok_or("Liquify grid cost overflows")?;
    if nodes > 2_097_152 || field_bytes > FIELD_LIMIT {
        return Err(
            "Liquify grid exceeds the 64 MiB field/workspace budget; increase spacing".into(),
        );
    }
    Ok(Plan {
        staged_bytes: s.doc.owner_transform_staging_bytes(&scope)?,
        scope,
        field_bytes,
    })
}
pub(crate) fn admit(s: &Session, args: &Value, require_idle: bool) -> Result<(), ActionError> {
    if require_idle {
        super::owner_affine::require_idle(s)?;
    }
    let r: Request = serde_json::from_value(args.clone())?;
    plan(s, &r).map_err(failure)?;
    Ok(())
}
pub(crate) fn describe(s: &Session, args: Value) -> ActionResult {
    let r: Request = serde_json::from_value(args)?;
    let p = plan(s, &r).map_err(failure)?;
    let preview = s.doc.owner_preview_cost(p.scope.mask_ids());
    Ok(
        json!({"document_identity":s.doc.identity,"revision":s.revision,"owner_scope":p.scope,"layer_ids":p.scope.layer_ids(),"mask_ids":p.scope.mask_ids(),"width":s.doc.width,"height":s.doc.height,"field_bytes":p.field_bytes,"staged_bytes":p.staged_bytes,"preview_cost":preview.as_ref().ok(),"preview_error":preview.err(),"contact_active":s.stroke.active()||s.pointer_checkpoint.is_some(),"selection_policy":"whole_owners_preserve_selection","exterior_rgba":"transparent","exterior_a8":"reveal"}),
    )
}
fn copied<T: Copy>(source: &[T]) -> Result<Vec<T>, String> {
    let mut v = Vec::new();
    v.try_reserve_exact(source.len())
        .map_err(|_| "Cannot reserve Liquify owner output")?;
    v.extend_from_slice(source);
    Ok(v)
}
fn dense_mask(s: &Session, id: u64) -> Result<SamplePlane, String> {
    let source = s.doc.mask_plane(id)?;
    let area = s.doc.width * s.doc.height;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(area)
        .map_err(|_| "Cannot reserve Liquify raw mask output")?;
    for i in 0..area {
        bytes.push(source.sample(i));
    }
    Ok(SamplePlane::Dense(bytes))
}
/// One cached field and initialized owner outputs per draft. Accepted typed
/// updates sample only dirty regions into existing buffers; no per-point JSON.
pub(crate) struct Draft {
    pub request: Request,
    pub scope: OwnerScope,
    field: super::native::NativeLiquify,
    outputs: super::owner_affine::OwnerOutputs,
    pending: [Option<abi::Region>; 4],
    valid: bool,
}
impl Draft {
    pub(crate) fn reset(&mut self, s: &Session) -> Result<(), String> {
        if self.request.document_identity != s.doc.identity
            || self.request.document_revision != s.revision
        {
            return Err("Liquify source changed; discard the stale draft".into());
        }
        super::owner_affine::require_idle(s).map_err(|e| e.to_string())?;
        s.doc.validate_owner_transform_scope(&self.scope)?;
        self.valid = false;
        self.field.clear()?;
        for (id, pixels, mask) in &mut self.outputs.layers {
            let index = s
                .doc
                .layer_index(*id)
                .ok_or("Liquify source layer disappeared")?;
            pixels.copy_from_slice(&s.doc.layers[index].pixels);
            if let Some((id, SamplePlane::Dense(bytes))) = mask {
                let source = s.doc.mask_plane(*id)?;
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = source.sample(i);
                }
            }
        }
        self.pending = [None; 4];
        self.request.steps.clear();
        self.valid = true;
        Ok(())
    }
    pub(crate) fn begin(s: &Session, mut request: Request) -> Result<Self, String> {
        super::owner_affine::require_idle(s).map_err(|e| e.to_string())?;
        let p = plan(s, &request)?;
        let steps = std::mem::take(&mut request.steps);
        let field = s.registry.native_plugins["painting"].create_liquify(
            s.doc.width as u32,
            s.doc.height as u32,
            request.spacing,
            FIELD_LIMIT as u64,
        )?;
        let pairs = s.doc.validate_owner_transform_scope(&p.scope)?;
        let mut layers = Vec::with_capacity(pairs.len());
        for (id, mask) in pairs {
            let index = s
                .doc
                .layer_index(id)
                .ok_or("Liquify source layer disappeared")?;
            layers.push((
                id,
                copied(&s.doc.layers[index].pixels)?,
                mask.map(|id| dense_mask(s, id).map(|p| (id, p)))
                    .transpose()?,
            ));
        }
        let mut draft = Self {
            request,
            scope: p.scope,
            field,
            outputs: super::owner_affine::OwnerOutputs {
                layers,
                mask_only: None,
            },
            valid: true,
            pending: [None; 4],
        };
        for step in steps {
            draft.push_deferred(s, &step)?;
        }
        draft.flush(s)?;
        Ok(draft)
    }
    fn check_source(&self, s: &Session) -> Result<(), String> {
        if !self.valid {
            return Err("Liquify draft failed; discard it and reopen".into());
        }
        if self.request.document_identity != s.doc.identity
            || self.request.document_revision != s.revision
        {
            return Err("Liquify source changed; discard the stale draft".into());
        }
        super::owner_affine::require_idle(s).map_err(|e| e.to_string())
    }
    pub(crate) fn push(&mut self, s: &Session, arms: &[Push]) -> Result<(), String> {
        self.push_deferred(s, arms)?;
        self.flush(s)
    }
    /// Accumulate typed field changes and four conservative dirty unions. A
    /// packet batch samples each owner once per union, instead of per raw packet.
    pub(crate) fn push_deferred(&mut self, s: &Session, arms: &[Push]) -> Result<(), String> {
        self.check_source(s)?;
        if !(1..=4).contains(&arms.len()) || self.request.steps.len() >= MAX_STEPS {
            return Err("Liquify contact exceeds the step/arm limit; apply or cancel".into());
        }
        for p in arms {
            p.validate(self.request.spacing)?;
        }
        // Reserve the compact input record before advancing the native field.
        self.request
            .steps
            .try_reserve(1)
            .map_err(|_| "Cannot reserve Liquify event step")?;
        let record = copied(arms)?;
        let mut native = [abi::LiquifyPush {
            x: 0.,
            y: 0.,
            dx: 0.,
            dy: 0.,
            radius: 0.,
            strength: 0.,
        }; 4];
        for (a, p) in native.iter_mut().zip(arms) {
            *a = p.native();
        }
        self.valid = false;
        let update = self.field.push(&native[..arms.len()])?;
        if update.count == 0 {
            self.valid = true;
            return Ok(());
        }
        for (slot, r) in update.regions[..update.count as usize].iter().enumerate() {
            self.pending[slot] = Some(match self.pending[slot] {
                None => *r,
                Some(old) => {
                    let x = old.x.min(r.x);
                    let y = old.y.min(r.y);
                    abi::Region {
                        x,
                        y,
                        width: (old.x + old.width).max(r.x + r.width) - x,
                        height: (old.y + old.height).max(r.y + r.height) - y,
                    }
                }
            });
        }
        self.request.steps.push(record);
        self.valid = true;
        Ok(())
    }
    pub(crate) fn flush(&mut self, s: &Session) -> Result<(), String> {
        self.check_source(s)?;
        self.valid = false;
        let method = if self.request.method == Method::Nearest {
            0
        } else {
            1
        };
        for region in self.pending.iter().flatten() {
            for (id, pixels, mask) in &mut self.outputs.layers {
                let index = s
                    .doc
                    .layer_index(*id)
                    .ok_or("Liquify source layer disappeared")?;
                self.field.pixels_frame_region(
                    &s.doc.layers[index].pixels,
                    method,
                    *region,
                    pixels,
                )?;
                if let Some((id, SamplePlane::Dense(bytes))) = mask {
                    self.field
                        .mask_frame_region(s.doc.mask_plane(*id)?, method, *region, bytes)?;
                }
            }
        }
        self.pending = [None; 4];
        self.valid = true;
        Ok(())
    }
    pub(crate) fn preview(&self, s: &Session) -> Result<Vec<u32>, String> {
        self.check_source(s)?;
        if self.pending.iter().any(Option::is_some) {
            return Err("Flush Liquify sampling before preview".into());
        }
        s.doc.owner_preview_cost(self.scope.mask_ids())?;
        let layers: Vec<_> = self
            .outputs
            .layers
            .iter()
            .map(|(id, pixels, _)| (*id, pixels.as_slice()))
            .collect();
        let masks: Vec<_> = self
            .outputs
            .layers
            .iter()
            .filter_map(|(_, _, mask)| mask.as_ref().map(|(id, p)| (*id, p.view())))
            .collect();
        s.doc.preview_owner_transform(&layers, &masks)
    }
    pub(crate) fn publish(self, s: &mut Session) -> Result<bool, String> {
        self.check_source(s)?;
        if self.pending.iter().any(Option::is_some) {
            return Err("Flush Liquify sampling before publication".into());
        }
        s.doc
            .apply_owner_deformation(self.outputs.layers, self.scope)
    }
}
pub(crate) fn execute(s: &mut Session, args: Value) -> ActionResult {
    let request: Request = serde_json::from_value(args)?;
    let draft = Draft::begin(s, request.clone()).map_err(failure)?;
    let mask_ids = draft.scope.mask_ids().to_vec();
    let changed = draft.publish(s).map_err(failure)?;
    Ok(
        json!({"changed":changed,"layer_ids":request.layer_ids,"mask_ids":mask_ids,"steps":request.steps.len(),"selection_policy":"whole_owners_preserve_selection"}),
    )
}
pub(crate) fn preview(s: &Session, args: Value) -> ActionResult {
    let request: Request = serde_json::from_value(args)?;
    let p = plan(s, &request).map_err(failure)?;
    let cost = s
        .doc
        .owner_preview_cost(p.scope.mask_ids())
        .map_err(failure)?;
    let draft = Draft::begin(s, request.clone()).map_err(failure)?;
    let pixels = draft.preview(s).map_err(failure)?;
    let bytes = crate::application::png_io::encode(
        s.doc.width,
        s.doc.height,
        pixels.into_iter(),
        s.doc.info().export_density(),
    )?;
    Ok(
        json!({"document_identity":request.document_identity,"revision":request.document_revision,"layer_ids":request.layer_ids,"mask_ids":p.scope.mask_ids(),"width":s.doc.width,"height":s.doc.height,"field_bytes":p.field_bytes,"staged_bytes":p.staged_bytes,"preview_cost":cost,"mimeType":"image/png","data":crate::application::codec::base64(&bytes),"apply":request.apply_action()}),
    )
}

pub(crate) fn request_dialog(s: &mut Session) -> ActionResult {
    if s.runtime != "desktop" {
        return Err(ActionError::new("ui_unavailable","Liquify controls require the desktop; use canvas.liquify.preview and canvas.liquify for headless editing"));
    }
    super::owner_affine::require_idle(s)?;
    let mut ids = s.doc.selected_layer_ids();
    if ids.is_empty() {
        ids.push(s.doc.layer_id(s.doc.active));
    }
    let r = Request {
        document_identity: s.doc.identity,
        document_revision: s.revision,
        layer_ids: ids,
        spacing: 4,
        method: Method::Bilinear,
        steps: Vec::new(),
    };
    let p = plan(s, &r).map_err(failure)?;
    s.doc
        .owner_preview_cost(p.scope.mask_ids())
        .map_err(failure)?;
    s.ui_effects
        .push_back(json!({"id":"liquify.open","request":r}));
    Ok(json!({"queued":true}))
}
