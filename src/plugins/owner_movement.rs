//! Cold owner-aware integer Move planning. Native plugins retain all pixel math.
use crate::application::{ActionError, ActionResult, MoveTarget, Operation, Session};
use crate::engine::raster_masks::{MaskOwner, PaintTarget, PlaneView, SamplePlane};
use micopi_plugin_api as abi;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MoveDestination {
    Layer {
        layer_id: u64,
        linked_mask_id: Option<u64>,
    },
    SelectedPixels {
        layer_id: u64,
    },
    SelectionOutline,
    RasterMask {
        mask_id: u64,
        owner_layer_id: u64,
    },
}

impl MoveDestination {
    #[cfg(feature = "desktop")]
    pub(crate) fn action_args(self, dx: i32, dy: i32) -> Value {
        match self {
            Self::Layer { .. } => json!({"target":"layer","dx":dx,"dy":dy}),
            Self::SelectedPixels { .. } => json!({"target":"selection","dx":dx,"dy":dy}),
            Self::SelectionOutline => json!({"target":"selection_mask","dx":dx,"dy":dy}),
            Self::RasterMask { mask_id, .. } => json!({"mask_id":mask_id,"dx":dx,"dy":dy}),
        }
    }
}

#[cfg(feature = "desktop")]
pub(crate) struct MovePreview {
    pub destination: MoveDestination,
    pub bounds: [usize; 4],
}

fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("movement", message)
}
fn pending() -> ActionError {
    ActionError::new("mask_editing_pending", crate::engine::MASK_EDITING_PENDING)
}

/// A stable mask selector overrides preferences, but never an explicit other domain.
pub(crate) fn validate_literal(args: &Value) -> Result<(), ActionError> {
    super::linked_movement::validate(args)?;
    if let Some(id) = args.get("mask_id") {
        if super::bounded_integer(id).is_none_or(|id| id == 0) {
            return Err(ActionError::new(
                "invalid_arguments",
                "mask_id requires a positive stable raster-mask ID",
            ));
        }
        if args.get("target").is_some_and(|target| target != "auto") {
            return Err(ActionError::new(
                "invalid_arguments",
                "mask_id cannot be combined with an explicit non-auto movement target",
            ));
        }
    }
    Ok(())
}

fn requested(s: &Session, args: &Value) -> Result<(MoveTarget, Option<u64>), ActionError> {
    validate_literal(args)?;
    let target = args
        .get("target")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or(s.movement_options().target);
    let mask = args.get("mask_id").and_then(super::bounded_integer);
    Ok((target, mask))
}

/// Metadata only: no source scans, native calls, output allocation or contact changes.
pub(crate) fn destination(
    s: &Session,
    target: MoveTarget,
    mask_id: Option<u64>,
) -> Result<MoveDestination, ActionError> {
    let mask_id = mask_id.or_else(|| {
        if target == MoveTarget::Auto {
            match s.doc.mask_state().target {
                PaintTarget::Mask { id } => Some(id),
                _ => None,
            }
        } else {
            None
        }
    });
    if let Some(mask_id) = mask_id {
        let metadata = s
            .doc
            .mask_state()
            .items
            .iter()
            .find(|mask| mask.id == mask_id)
            .ok_or_else(|| ActionError::new("raster_mask", "Raster mask does not exist"))?;
        return match metadata.owner {
            MaskOwner::Layer { id } if s.doc.layer_index(id).is_some() => {
                Ok(MoveDestination::RasterMask {
                    mask_id,
                    owner_layer_id: id,
                })
            }
            MaskOwner::Layer { .. } => Err(ActionError::new(
                "raster_mask",
                "Raster-mask owner does not exist",
            )),
            MaskOwner::Group { .. } | MaskOwner::Adjustment { .. } => Err(pending()),
        };
    }
    let layer_id = s.doc.layer_id(s.doc.active);
    match super::movement::resolved_target(s, target) {
        MoveTarget::Layer => Ok(MoveDestination::Layer {
            layer_id,
            linked_mask_id: s
                .doc
                .mask_state()
                .items
                .iter()
                .find(|mask| mask.owner == MaskOwner::Layer { id: layer_id } && mask.linked)
                .map(|mask| mask.id),
        }),
        MoveTarget::Selection => Ok(MoveDestination::SelectedPixels { layer_id }),
        MoveTarget::SelectionMask => Ok(MoveDestination::SelectionOutline),
        MoveTarget::Auto => unreachable!("Legacy Auto resolution always selects a concrete domain"),
    }
}

fn idle(s: &Session) -> Result<(), ActionError> {
    if s.stroke.active() || s.pointer_checkpoint.is_some() {
        Err(ActionError::new(
            "stroke_active",
            "Finish the current contact before moving raster layers or masks",
        ))
    } else {
        Ok(())
    }
}

/// Shared early admission precedes recording boundaries and all frontend cleanup.
pub(crate) fn admit_masked(s: &Session, args: &Value) -> Result<(), ActionError> {
    let (target, mask_id) = requested(s, args)?;
    if matches!(
        destination(s, target, mask_id)?,
        MoveDestination::SelectedPixels { .. }
    ) {
        return Err(pending());
    }
    idle(s)
}

fn source_preflight(s: &Session, target: MoveDestination) -> Result<(), ActionError> {
    idle(s)?;
    match target {
        MoveDestination::Layer {
            layer_id,
            linked_mask_id,
        } => {
            if s.doc.pixel_locked(s.doc.active) {
                return Err(ActionError::new(
                    "pixel_locked",
                    "Unlock layer pixels before editing them",
                ));
            }
            if s.doc.position_locked(s.doc.active) {
                return Err(ActionError::new(
                    "position_locked",
                    "Unlock layer position before transforming its pixels",
                ));
            }
            s.doc
                .validate_owner_move(Some(layer_id), linked_mask_id)
                .map_err(error)?;
            super::native::require_movement(s, true, false).map_err(error)?;
            if linked_mask_id.is_some() && !super::mask_geometry::available(s) {
                return Err(error(
                    "Painting DLL does not support raw raster-mask geometry",
                ));
            }
        }
        MoveDestination::RasterMask {
            mask_id,
            owner_layer_id,
        } => {
            let index = s
                .doc
                .layer_index(owner_layer_id)
                .ok_or_else(|| error("Raster-mask owner does not exist"))?;
            if s.doc.position_locked(index) {
                return Err(ActionError::new(
                    "position_locked",
                    "Unlock layer position before moving its raster mask",
                ));
            }
            s.doc
                .validate_owner_move(None, Some(mask_id))
                .map_err(error)?;
            if !super::mask_geometry::available(s) {
                return Err(error(
                    "Painting DLL does not support raw raster-mask geometry",
                ));
            }
        }
        MoveDestination::SelectionOutline => {
            s.doc.validate_owner_move_source().map_err(error)?;
            super::movement::preview_bounds(s, MoveTarget::SelectionMask).map_err(error)?;
            super::native::require_movement(s, false, true).map_err(error)?;
        }
        MoveDestination::SelectedPixels { .. } => return Err(pending()),
    }
    Ok(())
}

#[cfg(feature = "desktop")]
pub(crate) fn preview(
    s: &Session,
    target: MoveTarget,
    mask_id: Option<u64>,
) -> Result<MovePreview, ActionError> {
    let destination = destination(s, target, mask_id)?;
    if s.doc.has_raster_masks() || mask_id.is_some() {
        source_preflight(s, destination)?;
    }
    let bounds = match destination {
        MoveDestination::Layer { .. } | MoveDestination::RasterMask { .. } => {
            [0, 0, s.doc.width, s.doc.height]
        }
        MoveDestination::SelectedPixels { .. } => {
            super::movement::preview_bounds(s, MoveTarget::Selection).map_err(error)?
        }
        MoveDestination::SelectionOutline => {
            super::movement::preview_bounds(s, MoveTarget::SelectionMask).map_err(error)?
        }
    };
    Ok(MovePreview {
        destination,
        bounds,
    })
}

pub(super) fn offsets(s: &Session, op: Operation, args: &Value) -> (i32, i32) {
    if op == Operation::Nudge {
        let options = s.movement_options();
        let step = if args["large"] == true {
            options.large_step
        } else {
            options.step
        } as i32;
        match args["direction"].as_str().unwrap() {
            "left" => (-step, 0),
            "right" => (step, 0),
            "up" => (0, -step),
            _ => (0, step),
        }
    } else {
        (
            args["dx"].as_f64().unwrap() as i32,
            args["dy"].as_f64().unwrap() as i32,
        )
    }
}

/// Sample exactly one borrowed raw plane through the existing stateless native table.
pub(super) fn stage_mask(s: &Session, id: u64, dx: i32, dy: i32) -> Result<SamplePlane, String> {
    let plugin = s
        .registry
        .native_plugins
        .get("painting")
        .filter(|plugin| plugin.supports_a8_sampler())
        .ok_or("Painting DLL does not support raw raster-mask geometry")?;
    let source = match s.doc.mask_plane(id)? {
        PlaneView::Uniform(value) => abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: u32::from(value),
        },
        PlaneView::Dense(data) => abi::CompositeMask {
            data: data.as_ptr(),
            len: data.len(),
            stride: s.doc.width as u32,
            constant: 0,
        },
    };
    let mut data = vec![0; s.doc.width * s.doc.height];
    let mut output = abi::A8Pixels {
        data: data.as_mut_ptr(),
        len: data.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    plugin.a8_reframe(
        &abi::A8Reframe {
            x: dx,
            y: dy,
            fill: 255,
            reserved: 0,
        },
        &source,
        s.doc.width as u32,
        s.doc.height as u32,
        &mut output,
    )?;
    Ok(if data.iter().all(|sample| *sample == data[0]) {
        SamplePlane::Uniform(data[0])
    } else {
        SamplePlane::Dense(data)
    })
}

pub(crate) fn execute(s: &mut Session, op: Operation, args: &Value) -> ActionResult {
    let (requested, mask_id) = requested(s, args)?;
    let target = destination(s, requested, mask_id)?;
    source_preflight(s, target)?;
    let (dx, dy) = offsets(s, op, args);
    if (dx, dy) != (0, 0) {
        let options = abi::Move { kind: 0, dx, dy };
        match target {
            MoveDestination::Layer {
                layer_id,
                linked_mask_id,
            } => {
                let pixels = super::native::move_pixels(s, &options).map_err(error)?;
                let mask = linked_mask_id
                    .map(|id| stage_mask(s, id, dx, dy).map(|plane| (id, plane)))
                    .transpose()
                    .map_err(error)?;
                s.doc
                    .apply_owner_move(Some(layer_id), Some(pixels), mask)
                    .map_err(error)?;
            }
            MoveDestination::RasterMask { mask_id, .. } => {
                let samples = stage_mask(s, mask_id, dx, dy).map_err(error)?;
                s.doc
                    .apply_owner_move(None, None, Some((mask_id, samples)))
                    .map_err(error)?;
            }
            MoveDestination::SelectionOutline => {
                let options = abi::Move { kind: 2, dx, dy };
                let mut selection = s.doc.selection().clone();
                selection.current = Some(super::native::move_mask(s, &options).map_err(error)?);
                s.doc.apply_movement(None, selection).map_err(error)?;
            }
            MoveDestination::SelectedPixels { .. } => return Err(pending()),
        }
    }
    Ok(s.state())
}

pub(crate) fn context(s: &Session, op: Operation, args: &Value) -> Value {
    let (dx, dy) = offsets(s, op, args);
    let resolved = requested(s, args).and_then(|(target, id)| destination(s, target, id));
    match resolved {
        Ok(MoveDestination::Layer {
            layer_id,
            linked_mask_id,
        }) => {
            json!({"target":"layer","layer_id":layer_id,"linked_mask_id":linked_mask_id,"dx":dx,"dy":dy})
        }
        Ok(MoveDestination::RasterMask {
            mask_id,
            owner_layer_id,
        }) => {
            json!({"target":"raster_mask","mask_id":mask_id,"owner_layer_id":owner_layer_id,"dx":dx,"dy":dy})
        }
        Ok(MoveDestination::SelectionOutline) => json!({"target":"selection_mask","dx":dx,"dy":dy}),
        Ok(MoveDestination::SelectedPixels { .. }) => json!({"target":"selection","dx":dx,"dy":dy}),
        Err(_) => json!({"target":"unresolved","dx":dx,"dy":dy}),
    }
}
