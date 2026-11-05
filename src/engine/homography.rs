//! Finite document-edge geometry. Pixel sampling belongs to the native plugin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    pub matrix: [f64; 6],
    pub denominator: [f64; 3],
}
impl Mapping {
    pub fn identity(&self) -> bool {
        let i = self.denominator[2];
        i > 0.
            && self.denominator[0] == 0.
            && self.denominator[1] == 0.
            && self.matrix == [i, 0., 0., 0., i, 0.]
    }
    pub fn validate(&self) -> Result<(), String> {
        let [a, b, c, d, e, f] = self.matrix;
        let [g, h, i] = self.denominator;
        let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if self
            .matrix
            .iter()
            .chain(&self.denominator)
            .any(|v| !v.is_finite())
            || [a, b, d, e].iter().any(|v| v.abs() > 256.)
            || [c, f].iter().any(|v| v.abs() > 4_194_304.)
            || [g, h, i].iter().any(|v| v.abs() > 16.)
            || determinant.abs() < 1e-8
        {
            return Err(if self.denominator == [0., 0., 1.] {
                "Combined transform exceeds affine limits"
            } else {
                "Invalid or unsupported projective matrix"
            }
            .into());
        }
        Ok(())
    }
    pub fn at(&self, p: [f64; 2]) -> Option<[f64; 2]> {
        let [a, b, c, d, e, f] = self.matrix;
        let den = self.denominator[2] + self.denominator[0] * p[0] + self.denominator[1] * p[1];
        if den < 1e-6 || !den.is_finite() {
            return None;
        }
        let q = [
            (a * p[0] + b * p[1] + c) / den,
            (d * p[0] + e * p[1] + f) / den,
        ];
        q.iter().all(|v| v.is_finite()).then_some(q)
    }
    pub fn corners(&self, b: [usize; 4]) -> Result<[[f64; 2]; 4], String> {
        let mut q = [[0.; 2]; 4];
        for (to, p) in q
            .iter_mut()
            .zip([[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]])
        {
            *to = self
                .at([p[0] as f64, p[1] as f64])
                .ok_or("Transform crosses the projective horizon")?;
        }
        Ok(q)
    }
    pub fn full(&self) -> [f64; 9] {
        let [a, b, c, d, e, f] = self.matrix;
        [
            a,
            b,
            c,
            d,
            e,
            f,
            self.denominator[0],
            self.denominator[1],
            self.denominator[2],
        ]
    }
}
/// Solve the unit-square homography, then change from document-edge source coordinates.
pub fn from_quad(bounds: [usize; 4], q: [[f64; 2]; 4]) -> Result<Mapping, String> {
    if bounds[2] <= bounds[0]
        || bounds[3] <= bounds[1]
        || q.iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 8192.)
    {
        return Err("Invalid perspective corner coordinates".into());
    }
    let cross: [f64; 4] = std::array::from_fn(|i| {
        let a = q[i];
        let b = q[(i + 1) % 4];
        let c = q[(i + 2) % 4];
        (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
    });
    if cross.iter().any(|v| v.abs() < 1e-6)
        || !(cross.iter().all(|v| *v > 0.) || cross.iter().all(|v| *v < 0.))
    {
        return Err("Perspective corners must form a nondegenerate convex quadrilateral".into());
    }
    let dx1 = q[1][0] - q[2][0];
    let dx2 = q[3][0] - q[2][0];
    let dx3 = q[0][0] - q[1][0] + q[2][0] - q[3][0];
    let dy1 = q[1][1] - q[2][1];
    let dy2 = q[3][1] - q[2][1];
    let dy3 = q[0][1] - q[1][1] + q[2][1] - q[3][1];
    let det = dx1 * dy2 - dx2 * dy1;
    if det.abs() < 1e-12 {
        return Err("Degenerate perspective mapping".into());
    }
    // Stabilize affine frames baked from floating-point controls before corner editing.
    let epsilon = q.iter().flatten().fold(1_f64, |a, v| a.max(v.abs())) * 1e-12;
    let (g, h) = if dx3.abs() <= epsilon && dy3.abs() <= epsilon {
        (0., 0.)
    } else {
        ((dx3 * dy2 - dx2 * dy3) / det, (dx1 * dy3 - dx3 * dy1) / det)
    };
    let (w, t) = (
        (bounds[2] - bounds[0]) as f64,
        (bounds[3] - bounds[1]) as f64,
    );
    let (x, y) = (bounds[0] as f64, bounds[1] as f64);
    let a = (q[1][0] - q[0][0] + g * q[1][0]) / w;
    let b = (q[3][0] - q[0][0] + h * q[3][0]) / t;
    let d = (q[1][1] - q[0][1] + g * q[1][1]) / w;
    let e = (q[3][1] - q[0][1] + h * q[3][1]) / t;
    let (g, h) = (g / w, h / t);
    let n = 1. - g * x - h * y;
    let norm = n.abs().max(g.abs()).max(h.abs());
    if norm < 1e-12 {
        return Err("Degenerate perspective mapping".into());
    }
    let result = Mapping {
        matrix: [
            a / norm,
            b / norm,
            (q[0][0] - a * x - b * y) / norm,
            d / norm,
            e / norm,
            (q[0][1] - d * x - e * y) / norm,
        ],
        denominator: [g / norm, h / norm, n / norm],
    };
    result.validate()?;
    result.corners(bounds)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corners_solve_trapezoids_reflections_offsets_and_a_horizon_at_the_origin() {
        for (bounds, quad) in [
            ([0, 0, 4, 4], [[0., 0.], [2., 0.], [2., 2.], [0., 4.]]),
            (
                [20, 30, 40, 50],
                [[50., 10.], [20., 15.], [25., 40.], [45., 50.]],
            ),
            (
                [2, 2, 4, 4],
                [[15., 10.], [12.5, 5.], [12.5, 10.], [15., 20.]],
            ),
            ([0, 0, 10, 8], [[0., 0.], [10., 0.], [10., 8.], [0., 8.]]),
        ] {
            let m = from_quad(bounds, quad).unwrap();
            for (got, want) in m.corners(bounds).unwrap().into_iter().zip(quad) {
                assert!(
                    (got[0] - want[0]).abs() < 1e-8 && (got[1] - want[1]).abs() < 1e-8,
                    "{m:?}: {got:?} vs {want:?}"
                );
            }
        }
        let m = from_quad([0, 0, 4, 4], [[0., 0.], [2., 0.], [2., 2.], [0., 4.]]).unwrap();
        assert_eq!(m.matrix, [1., 0., 0., 0., 1., 0.]);
        assert_eq!(m.denominator, [0.25, 0., 1.]);
        assert!(!m.identity());
    }
    #[test]
    fn concave_crossed_collapsed_nonfinite_and_out_of_range_corners_reject() {
        for quad in [
            [[0., 0.], [4., 4.], [4., 0.], [0., 4.]],
            [[0., 0.], [4., 0.], [1., 1.], [0., 4.]],
            [[0., 0.], [0., 0.], [4., 4.], [0., 4.]],
            [[0., 0.], [4., 0.], [4., 4.], [0., f64::NAN]],
            [[0., 0.], [8193., 0.], [4., 4.], [0., 4.]],
        ] {
            assert!(from_quad([0, 0, 4, 4], quad).is_err());
        }
    }
}
