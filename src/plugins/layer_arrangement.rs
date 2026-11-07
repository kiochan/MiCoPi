//! Cold exact-raster layout. Native plugins own bounds and pixel sampling.
use crate::application::{ActionError, ActionResult, Operation, Session};
use micopi_plugin_api as abi;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
    DistributeHorizontalCenters,
    DistributeVerticalCenters,
}
impl Mode {
    const ALL: [Self; 8] = [
        Self::Left,
        Self::HorizontalCenter,
        Self::Right,
        Self::Top,
        Self::VerticalCenter,
        Self::Bottom,
        Self::DistributeHorizontalCenters,
        Self::DistributeVerticalCenters,
    ];
    fn distribution(self) -> bool {
        matches!(
            self,
            Self::DistributeHorizontalCenters | Self::DistributeVerticalCenters
        )
    }
    fn vertical(self) -> bool {
        matches!(
            self,
            Self::Top | Self::VerticalCenter | Self::Bottom | Self::DistributeVerticalCenters
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reference {
    Targets,
    Canvas,
}

pub(crate) fn schema(_op: Operation) -> Value {
    json!({"type":"object","properties":{
        "layer_ids":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"integer","minimum":1,"maximum":4294967295u64},
            "description":"Exact raster IDs in caller order; never implicitly expand persistent links. Carry each owned linked mask."},
        "mode":{"type":"string","enum":Mode::ALL},
        "reference":{"type":"string","enum":["targets","canvas"],"default":"targets"}},
        "required":["layer_ids","mode"],"additionalProperties":false})
}
fn invalid(message: impl Into<String>) -> ActionError {
    ActionError::new("invalid_arguments", message)
}
fn error(message: impl Into<String>) -> ActionError {
    ActionError::new("arrangement", message)
}
fn inputs(args: &Value) -> Result<(Vec<u64>, Mode, Reference), ActionError> {
    let ids = super::layer_links::ids(&args["layer_ids"], "invalid_arguments")?;
    let mode: Mode = serde_json::from_value(args["mode"].clone())
        .map_err(|_| invalid("Unknown layer arrangement mode"))?;
    let reference = match args.get("reference") {
        None => Reference::Targets,
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| invalid("Unknown layer arrangement reference"))?,
    };
    if mode.distribution() {
        if reference != Reference::Targets || ids.len() < 3 {
            return Err(invalid(
                "Center distribution requires at least three targets and the targets reference",
            ));
        }
    } else if reference == Reference::Targets && ids.len() < 2 {
        return Err(invalid(
            "Aligning to targets requires at least two rasters; canvas alignment accepts one",
        ));
    }
    Ok((ids, mode, reference))
}
pub(crate) fn validate_literal(args: &Value) -> Result<(), ActionError> {
    inputs(args).map(|_| ())
}

#[derive(Clone, Debug, Serialize)]
struct TargetPlan {
    layer_id: u64,
    mask_ids: Vec<u64>,
    bounds: [usize; 4],
    dx: i32,
    dy: i32,
    moved_bounds: [i64; 4],
}
#[derive(Debug, Serialize)]
struct Plan {
    revision: u64,
    document_identity: u64,
    layer_ids: Vec<u64>,
    linked_mask_ids: Vec<u64>,
    mode: Mode,
    reference: Reference,
    reference_bounds: [usize; 4],
    targets: Vec<TargetPlan>,
    changed: bool,
    contact_active: bool,
    can_apply: bool,
    staged_bytes: usize,
    staging_limit_bytes: usize,
}

/// Exact signed nearest integer; ties are away from zero. The layout domain
/// stays small, but i128 intermediates also avoid abs(i64::MIN) overflow.
fn round_ratio(numerator: i64, denominator: i64) -> i64 {
    assert!(denominator > 0);
    let n = i128::from(numerator);
    let d = i128::from(denominator);
    let magnitude = (n.abs() * 2 + d) / (d * 2);
    i64::try_from(if n < 0 { -magnitude } else { magnitude })
        .expect("Rounded layout ratio fits its input domain")
}
fn layout(
    ids: &[u64],
    bounds: &[[usize; 4]],
    mode: Mode,
    reference: Reference,
    frame: [usize; 2],
) -> Result<([usize; 4], Vec<(i32, i32)>), String> {
    if ids.is_empty()
        || ids.len() > 16
        || ids.len() != bounds.len()
        || bounds
            .iter()
            .any(|b| b[0] >= b[2] || b[1] >= b[3] || b[2] > frame[0] || b[3] > frame[1])
    {
        return Err("Layer arrangement requires nonempty in-frame raw alpha bounds".into());
    }
    if mode.distribution() && (ids.len() < 3 || reference != Reference::Targets)
        || !mode.distribution() && reference == Reference::Targets && ids.len() < 2
    {
        return Err("Layer arrangement count and reference do not match the operation".into());
    }
    let reference_bounds = if reference == Reference::Canvas {
        [0, 0, frame[0], frame[1]]
    } else {
        bounds.iter().skip(1).fold(bounds[0], |a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        })
    };
    let mut offsets = vec![(0, 0); ids.len()];
    let axis = usize::from(mode.vertical());
    if mode.distribution() {
        let center = |index: usize| bounds[index][axis] as i64 + bounds[index][axis + 2] as i64;
        let mut order: Vec<_> = (0..ids.len()).collect();
        order.sort_by_key(|index| (center(*index), ids[*index]));
        let first = center(order[0]);
        let span = center(*order.last().unwrap()) - first;
        let divisor = (ids.len() - 1) as i64;
        for (position, index) in order.into_iter().enumerate() {
            let delta = round_ratio(
                first * divisor + span * position as i64 - center(index) * divisor,
                2 * divisor,
            );
            let delta = i32::try_from(delta).map_err(|_| "Layer arrangement offset overflow")?;
            offsets[index] = if axis == 0 { (delta, 0) } else { (0, delta) };
        }
    } else {
        for (index, bounds) in bounds.iter().enumerate() {
            let delta = match mode {
                Mode::Left | Mode::Top => reference_bounds[axis] as i64 - bounds[axis] as i64,
                Mode::Right | Mode::Bottom => {
                    reference_bounds[axis + 2] as i64 - bounds[axis + 2] as i64
                }
                Mode::HorizontalCenter | Mode::VerticalCenter => round_ratio(
                    reference_bounds[axis] as i64 + reference_bounds[axis + 2] as i64
                        - bounds[axis] as i64
                        - bounds[axis + 2] as i64,
                    2,
                ),
                _ => unreachable!("Distribution handled above"),
            };
            let delta = i32::try_from(delta).map_err(|_| "Layer arrangement offset overflow")?;
            offsets[index] = if axis == 0 { (delta, 0) } else { (0, delta) };
        }
    }
    Ok((reference_bounds, offsets))
}
fn prepare(s: &Session, args: &Value, mutating: bool) -> Result<Plan, ActionError> {
    let (ids, mode, reference) = inputs(args)?;
    let contact_active = s.stroke.active() || s.pointer_checkpoint.is_some();
    if mutating && (contact_active || s.doc.history_group_active()) {
        return Err(ActionError::new(
            "stroke_active",
            "Finish the current contact or history group before arranging layers",
        ));
    }
    if s.doc.has_adjustments() {
        super::adjustments::validate_document(&s.registry, &s.doc)?;
    }
    let pairs = if mutating {
        s.doc.validate_owner_moves(&ids)
    } else {
        s.doc.inspect_owner_moves(&ids)
    }
    .map_err(error)?;
    super::native::require_movement(s, true, false).map_err(error)?;
    let linked_mask_ids: Vec<_> = pairs.iter().filter_map(|(_, mask)| *mask).collect();
    if !linked_mask_ids.is_empty() && !super::mask_geometry::available(s) {
        return Err(error(
            "Painting DLL does not support raw raster-mask geometry",
        ));
    }
    let staged_bytes = s
        .doc
        .owner_move_staging_bytes(ids.len(), linked_mask_ids.len())
        .map_err(error)?;
    let mut bounds = Vec::with_capacity(ids.len());
    for id in &ids {
        bounds.push(
            super::native::affine_layer_bounds(s, *id)
                .map_err(error)?
                .ok_or_else(|| {
                    error(format!(
                        "Layer {id} has no raw positive-alpha content to arrange"
                    ))
                })?,
        );
    }
    let (reference_bounds, offsets) =
        layout(&ids, &bounds, mode, reference, [s.doc.width, s.doc.height]).map_err(error)?;
    let targets = pairs
        .into_iter()
        .zip(bounds)
        .zip(offsets)
        .map(|(((layer_id, mask), bounds), (dx, dy))| TargetPlan {
            layer_id,
            mask_ids: mask.into_iter().collect(),
            bounds,
            dx,
            dy,
            moved_bounds: [
                bounds[0] as i64 + i64::from(dx),
                bounds[1] as i64 + i64::from(dy),
                bounds[2] as i64 + i64::from(dx),
                bounds[3] as i64 + i64::from(dy),
            ],
        })
        .collect::<Vec<_>>();
    Ok(Plan {
        revision: s.revision,
        document_identity: s.doc.identity,
        layer_ids: ids,
        linked_mask_ids,
        mode,
        reference,
        reference_bounds,
        changed: targets
            .iter()
            .any(|target| target.dx != 0 || target.dy != 0),
        targets,
        contact_active,
        can_apply: !contact_active && !s.doc.history_group_active(),
        staged_bytes,
        staging_limit_bytes: crate::engine::document::OWNER_MOVE_STAGING_LIMIT,
    })
}
pub(crate) fn describe(s: &Session, args: &Value) -> Result<Value, ActionError> {
    Ok(serde_json::to_value(prepare(s, args, false)?)?)
}
pub(crate) fn execute(s: &mut Session, op: Operation, args: Value) -> ActionResult {
    if op == Operation::ArrangeLayersPlan {
        return describe(s, &args);
    }
    if op != Operation::ArrangeLayers {
        return Err(error("Operation routed to the wrong arrangement handler"));
    }
    let mut plan = prepare(s, &args, true)?;
    if plan.changed {
        let mut outputs = Vec::with_capacity(plan.targets.len());
        for target in plan
            .targets
            .iter()
            .filter(|target| target.dx != 0 || target.dy != 0)
        {
            let index = s
                .doc
                .layer_index(target.layer_id)
                .ok_or_else(|| error("Layer does not exist"))?;
            let pixels = super::native::move_layer_pixels(
                s,
                index,
                &abi::Move {
                    kind: 0,
                    dx: target.dx,
                    dy: target.dy,
                },
            )
            .map_err(error)?;
            let mask = target
                .mask_ids
                .first()
                .map(|id| {
                    super::owner_movement::stage_mask(s, *id, target.dx, target.dy)
                        .map(|plane| (*id, plane))
                })
                .transpose()
                .map_err(error)?;
            outputs.push((target.layer_id, pixels, mask));
        }
        plan.changed = s.doc.apply_owner_moves(outputs).map_err(error)?;
    }
    s.doc.compose();
    if let Some(message) = &s.doc.render_error {
        return Err(error(message.clone()));
    }
    Ok(serde_json::to_value(plan)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_ratio_ties_round_away_from_zero_without_float_drift() {
        for (n, d, want) in [
            (1, 2, 1),
            (-1, 2, -1),
            (3, 2, 2),
            (-3, 2, -2),
            (2, 3, 1),
            (-2, 3, -1),
            (1, 3, 0),
            (-1, 3, 0),
            (i64::MIN, 1, i64::MIN),
            (i64::MAX, 1, i64::MAX),
        ] {
            assert_eq!(round_ratio(n, d), want);
        }
    }
    #[test]
    fn target_alignments_use_one_union_and_keep_caller_order() {
        let ids = [3, 1, 2];
        let boxes = [[2, 5, 4, 6], [1, 1, 3, 3], [5, 2, 8, 5]];
        for (mode, wanted) in [
            (Mode::Left, vec![(-1, 0), (0, 0), (-4, 0)]),
            (Mode::Right, vec![(4, 0), (5, 0), (0, 0)]),
            (Mode::HorizontalCenter, vec![(2, 0), (3, 0), (-2, 0)]),
            (Mode::Top, vec![(0, -4), (0, 0), (0, -1)]),
            (Mode::Bottom, vec![(0, 0), (0, 3), (0, 1)]),
            (Mode::VerticalCenter, vec![(0, -2), (0, 2), (0, 0)]),
        ] {
            let (reference, offsets) =
                layout(&ids, &boxes, mode, Reference::Targets, [10, 8]).unwrap();
            assert_eq!(reference, [1, 1, 8, 6]);
            assert_eq!(offsets, wanted, "{mode:?}");
        }
    }
    #[test]
    fn canvas_center_accepts_one_and_retains_signed_half_ties() {
        let (_, offsets) = layout(
            &[7],
            &[[7, 4, 10, 7]],
            Mode::HorizontalCenter,
            Reference::Canvas,
            [10, 8],
        )
        .unwrap();
        assert_eq!(offsets, [(-4, 0)]);
        let (_, offsets) = layout(
            &[7],
            &[[0, 0, 3, 3]],
            Mode::VerticalCenter,
            Reference::Canvas,
            [10, 8],
        )
        .unwrap();
        assert_eq!(offsets, [(0, 3)]);
    }
    #[test]
    fn distribution_orders_centers_then_ids_and_keeps_endpoints_fixed() {
        let ids = [9, 1, 5, 2];
        let boxes = [[1, 1, 3, 3], [1, 1, 3, 3], [7, 3, 9, 5], [1, 1, 3, 3]];
        let (_, offsets) = layout(
            &ids,
            &boxes,
            Mode::DistributeHorizontalCenters,
            Reference::Targets,
            [10, 8],
        )
        .unwrap();
        assert_eq!(offsets, [(4, 0), (0, 0), (0, 0), (2, 0)]);
        let (_, vertical) = layout(
            &[3, 1, 2],
            &[[2, 5, 4, 6], [1, 1, 3, 3], [5, 2, 8, 5]],
            Mode::DistributeVerticalCenters,
            Reference::Targets,
            [10, 8],
        )
        .unwrap();
        assert_eq!(vertical, [(0, 0), (0, 0), (0, 0)]);
    }
    #[test]
    fn distribution_matches_independent_exhaustive_small_rational_reference() {
        fn reference(n: i64, d: i64) -> i32 {
            let mut k = n / d;
            let remainder = n % d;
            if remainder.abs() * 2 >= d {
                k += n.signum();
            }
            k as i32
        }
        for first in 0..5usize {
            for middle in 0..5usize {
                for last in 0..5usize {
                    let ids = [3, 1, 2];
                    let boxes = [
                        [first, 0, first + 1, 1],
                        [middle, 0, middle + 1, 1],
                        [last, 0, last + 1, 1],
                    ];
                    let (_, offsets) = layout(
                        &ids,
                        &boxes,
                        Mode::DistributeHorizontalCenters,
                        Reference::Targets,
                        [8, 1],
                    )
                    .unwrap();
                    let mut order = [0usize, 1, 2];
                    order.sort_by_key(|i| (boxes[*i][0] + boxes[*i][2], ids[*i]));
                    let centers = boxes.map(|b| (b[0] + b[2]) as i64);
                    let lo = centers[order[0]];
                    let hi = centers[order[2]];
                    let mut expected = [(0, 0); 3];
                    for (rank, index) in order.into_iter().enumerate() {
                        expected[index] = (
                            reference(2 * lo + (hi - lo) * rank as i64 - 2 * centers[index], 4),
                            0,
                        );
                    }
                    assert_eq!(offsets, expected, "{boxes:?}");
                }
            }
        }
    }
    #[test]
    fn literal_counts_domains_and_normalized_duplicates_reject_before_execution() {
        for args in [
            json!({"layer_ids":[1],"mode":"left"}),
            json!({"layer_ids":[1,1.0],"mode":"left"}),
            json!({"layer_ids":[1,2],"mode":"distribute_horizontal_centers"}),
            json!({"layer_ids":[1,2,3],"mode":"distribute_horizontal_centers","reference":"canvas"}),
            json!({"layer_ids":[1],"mode":"unknown","reference":"canvas"}),
        ] {
            assert_eq!(
                validate_literal(&args).unwrap_err().code,
                "invalid_arguments"
            );
        }
        validate_literal(
            &json!({"layer_ids":[2.0],"mode":"horizontal_center","reference":"canvas"}),
        )
        .unwrap();
        assert!(layout(&[1], &[[0, 0, 0, 1]], Mode::Left, Reference::Canvas, [1, 1]).is_err());
        assert!(layout(
            &[1],
            &[[0, 0, 1, 1]],
            Mode::DistributeHorizontalCenters,
            Reference::Targets,
            [1, 1]
        )
        .is_err());
    }
}
