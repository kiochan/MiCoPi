//! Nominal analytic tip contours; painting antialiasing is owned by the DLL.
use super::*;
use crate::application::TipShape;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) enum Contour {
    Circle,
    Polygon(Arc<[Vec2]>),
}
impl Contour {
    pub(crate) fn outline(&self, painter: &Painter, center: Pos2, radius: f32, stroke: Stroke) {
        match self {
            Self::Circle => {
                painter.circle_stroke(center, radius, stroke);
            }
            Self::Polygon(points) => {
                painter.add(Shape::closed_line(
                    points
                        .iter()
                        .map(|point| center + *point * radius)
                        .collect(),
                    stroke,
                ));
            }
        }
    }
    pub(crate) fn filled(&self, painter: &Painter, center: Pos2, radius: f32, color: Color32) {
        match self {
            Self::Circle => {
                painter.circle_filled(center, radius, color);
            }
            Self::Polygon(points) => {
                painter.add(Shape::convex_polygon(
                    points
                        .iter()
                        .map(|point| center + *point * radius)
                        .collect(),
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}
#[derive(Clone)]
struct Cached {
    key: (u8, u32, u32, usize),
    contour: Contour,
}

pub(crate) fn contour(shape: TipShape, roundness: f32, angle: f32, segments: usize) -> Contour {
    if shape == TipShape::Round && roundness == 1. {
        return Contour::Circle;
    }
    let (sin, cos) = match angle {
        0. => (0., 1.),
        90. => (1., 0.),
        -90. => (-1., 0.),
        180. | -180. => (0., -1.),
        _ => angle.to_radians().sin_cos(),
    };
    let rotate = |x: f32, y: f32| vec2(cos * x - sin * y, sin * x + cos * y);
    let points = if shape == TipShape::Square {
        [
            (-1., -roundness),
            (1., -roundness),
            (1., roundness),
            (-1., roundness),
        ]
        .into_iter()
        .map(|(x, y)| rotate(x, y))
        .collect::<Vec<_>>()
    } else {
        (0..segments)
            .map(|i| {
                let (sin, cos) = (i as f32 * std::f32::consts::TAU / segments as f32).sin_cos();
                rotate(cos, sin * roundness)
            })
            .collect()
    };
    Contour::Polygon(points.into())
}
pub(crate) fn cached(
    ctx: &Context,
    id: &'static str,
    shape: TipShape,
    roundness: f32,
    angle: f32,
    radius: f32,
) -> Contour {
    if shape == TipShape::Round && roundness == 1. {
        return Contour::Circle;
    }
    // Screen-space detail is bounded; repeated frames reuse trigonometry until
    // the geometry or required contour resolution changes.
    let segments = if shape == TipShape::Square {
        4
    } else {
        ((std::f32::consts::PI * radius.max(1.).sqrt()).ceil() as usize)
            .next_power_of_two()
            .clamp(32, 256)
    };
    let key = (
        if shape == TipShape::Round { 0 } else { 1 },
        roundness.to_bits(),
        angle.to_bits(),
        segments,
    );
    if let Some(cached) = ctx
        .data_mut(|data| data.get_temp::<Cached>(Id::new(id)))
        .filter(|cached| cached.key == key)
    {
        return cached.contour;
    }
    let contour = contour(shape, roundness, angle, segments);
    ctx.data_mut(|data| {
        data.insert_temp(
            Id::new(id),
            Cached {
                key,
                contour: contour.clone(),
            },
        )
    });
    contour
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brush_tip_contour_uses_document_clockwise_axes_and_reuses_normalized_storage() {
        let ctx = Context::default();
        for shape in [TipShape::Round, TipShape::Square] {
            let a = cached(&ctx, "test-tip", shape, 0.25, 90., 100.);
            let b = cached(&ctx, "test-tip", shape, 0.25, 90., 100.);
            let (Contour::Polygon(a), Contour::Polygon(b)) = (a, b) else {
                panic!("noncircular contour required")
            };
            assert!(Arc::ptr_eq(&a, &b));
            let max_x = a.iter().map(|p| p.x.abs()).fold(0., f32::max);
            let max_y = a.iter().map(|p| p.y.abs()).fold(0., f32::max);
            assert!((max_x - 0.25).abs() < 1e-5);
            assert!((max_y - 1.).abs() < 1e-5);
            if shape == TipShape::Round {
                assert!((a[0].x).abs() < 1e-5 && a[0].y > 0.99);
            } else {
                assert_eq!(a.len(), 4);
            }
        }
        assert!(matches!(
            cached(&ctx, "test-tip", TipShape::Round, 1., 173., 6400.),
            Contour::Circle
        ));
    }
}
