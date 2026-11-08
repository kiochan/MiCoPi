//! Independent numeric references and bounded workspace/rejection invariants.
use super::*;
fn push(x: f32, y: f32, dx: f32, dy: f32) -> Push {
    Push {
        x,
        y,
        dx,
        dy,
        radius: 8.,
        strength: 0.5,
    }
}
fn field() -> Field {
    Field::new(32, 24, 2, 1024 * 1024).unwrap()
}
#[test]
fn identity_partial_cells_and_single_push_follow_independent_polynomial_reference() {
    let mut f = Field::new(31, 23, 2, 1024 * 1024).unwrap();
    assert_eq!(f.displacement(31., 23.), [0.; 2]);
    assert_eq!(f.displacement(-0.1, 4.), [0.; 2]);
    let p = push(12., 10., 4., -2.);
    let regions = f.push(&[p]).unwrap();
    assert_eq!(regions.len, 1);
    for row in 0..f.rows {
        for col in 0..f.cols {
            let x = (col as u32 * 2).min(31) as f64;
            let y = (row as u32 * 2).min(23) as f64;
            let distance = ((x - 12.).hypot(y - 10.) / 8.).min(1.);
            let weight = (1. - distance * distance).powi(2) * 0.5;
            let expected = [-4. * weight, 2. * weight];
            for axis in 0..2 {
                assert!((f.displacement(x, y)[axis] - expected[axis]).abs() < 0.000001);
            }
        }
    }
    assert_eq!(f.displacement(31.01, 23.), [0.; 2]);
}
#[test]
fn compound_push_samples_the_old_field_and_reuses_allocations() {
    let mut f = field();
    let p = push(12., 10., 4., 0.);
    f.push(&[p]).unwrap();
    let old = f.values.clone();
    let data = f.values.as_ptr();
    let scratch = f.scratch.as_ptr();
    let capacity = f.scratch.capacity();
    // At q=(12,10), the second inverse offset is2 and reads the old grid at(10,10).
    let expected = -2. + f64::from(old[5 * f.cols + 5][0]);
    f.push(&[p]).unwrap();
    assert!((f.displacement(12., 10.)[0] - expected).abs() < 0.000001);
    assert_eq!(f.values.as_ptr(), data);
    assert_eq!(f.scratch.as_ptr(), scratch);
    assert_eq!(f.scratch.capacity(), capacity);
    f.clear();
    assert!(f.values.iter().all(|v| *v == [0.; 2]));
    assert_eq!(f.values.as_ptr(), data);
}
#[test]
fn duplicates_symmetry_and_remote_dirty_regions_do_not_amplify_or_span_arms() {
    let a = push(8., 12., 2., 1.);
    let b = push(24., 12., -2., 1.);
    let mut single = field();
    single.push(&[a]).unwrap();
    let mut duplicate = field();
    duplicate.push(&[a, a]).unwrap();
    assert_eq!(single.values, duplicate.values);
    let mut symmetric = field();
    symmetric.push(&[a, b]).unwrap();
    for y in 0..=24 {
        for x in 0..=32 {
            let left = symmetric.displacement(x as f64, y as f64);
            let right = symmetric.displacement((32 - x) as f64, y as f64);
            assert!((left[0] + right[0]).abs() < 0.000001);
            assert!((left[1] - right[1]).abs() < 0.000001);
        }
    }
    let mut remote = Field::new(1024, 1024, 4, 4 * 1024 * 1024).unwrap();
    let regions = remote
        .push(&[push(32., 32., 2., 0.), push(992., 992., -2., 0.)])
        .unwrap();
    assert_eq!(regions.len, 2);
    for b in &regions.items[..2] {
        assert!(b[2] - b[0] <= 32 && b[3] - b[1] <= 32);
    }
}
#[test]
fn invalid_geometry_and_budgets_reject_without_changing_field() {
    assert!(Field::new(0, 24, 2, 1024).is_err());
    assert!(Field::new(16384, 16384, 1, usize::MAX).is_err());
    assert!(Field::new(32, 24, 2, 1).is_err());
    let mut f = field();
    f.push(&[push(8., 8., 2., 0.)]).unwrap();
    let before = f.values.clone();
    for p in [
        Push {
            radius: 1.,
            ..push(8., 8., 1., 0.)
        },
        Push {
            strength: f32::NAN,
            ..push(8., 8., 1., 0.)
        },
        Push {
            dx: 9.,
            ..push(8., 8., 1., 0.)
        },
        Push {
            x: f32::INFINITY,
            ..push(8., 8., 1., 0.)
        },
    ] {
        assert!(f.push(&[p]).is_err());
        assert_eq!(f.values, before);
    }
    assert!(f.push(&[]).is_err());
    assert!(f.push(&[push(8., 8., 1., 0.); 5]).is_err());
    assert_eq!(f.values, before);
    assert_eq!(
        f.push(&[Push {
            strength: 0.,
            ..push(8., 8., 1., 0.)
        }])
        .unwrap(),
        Regions::default()
    );
    assert_eq!(f.values, before);
}
