//! Independent scalar oracles and bounded borrowed-buffer/contact probes.
use super::*;
use crate::tip::{probe, Tip};

fn reference(
    shape: u32,
    q: f64,
    degrees: f64,
    radius: f64,
    dx: f64,
    dy: f64,
    hardness: f64,
) -> f64 {
    let angle = degrees * std::f64::consts::PI / 180.;
    let horizontal = angle.cos() * dx + angle.sin() * dy;
    let vertical = angle.cos() * dy - angle.sin() * dx;
    let major = radius.max(0.5);
    let minor = major * q;
    let x = horizontal.abs();
    let y = vertical.abs();
    let (norm, signed) = if shape == abi::TIP_SQUARE {
        let outside_x = (x - major).max(0.);
        let outside_y = (y - minor).max(0.);
        (
            (x / major).max(y / minor),
            (outside_x * outside_x + outside_y * outside_y).sqrt()
                + (x - major).max(y - minor).min(0.),
        )
    } else {
        let norm = ((x / major).powi(2) + (y / minor).powi(2)).sqrt();
        let gradient = ((x / (major * major)).powi(2) + (y / (minor * minor)).powi(2)).sqrt();
        (
            norm,
            if gradient == 0. {
                -minor
            } else {
                (norm - 1.) * norm / gradient
            },
        )
    };
    let edge = (0.5 - signed).clamp(0., 1.);
    let soft = if hardness >= 0.999 {
        1.
    } else {
        ((1. - norm) / (1. - hardness)).clamp(0., 1.)
    };
    edge * soft
}
fn tip_options(shape: u32, q: f32, angle: f32, capped: bool, flow: f32) -> abi::ToolOptionsTip {
    let mut base = super::options(capped, flow);
    base.base.base.base.struct_size = std::mem::size_of::<abi::ToolOptionsTip>() as u32;
    base.base.base.flags |= abi::TOOL_OPTION_TIP_GEOMETRY;
    abi::ToolOptionsTip {
        base,
        tip_shape: shape,
        roundness: q,
        angle,
        reserved: 0,
    }
}
fn host_table(host: &mut Host, a8: bool) -> abi::ToolHostStroke {
    let mut table = host.table(a8);
    table.base.base.pixels = legacy_rgba;
    table.base.a8 = a8.then_some(legacy_a8);
    table
}
fn raster(host: &Host, tip: Tip, a8: bool, capped: bool, selection: abi::Coverage) -> Raster {
    let mut coverage = Coverage::default();
    if capped {
        coverage
            .begin(host.width as u32, host.height as u32)
            .unwrap();
    }
    Raster {
        width: host.width as u32,
        height: host.height as u32,
        spacing: 0.11,
        flow: 0.25,
        tip,
        a8_value: a8.then_some(0),
        stroke: capped.then_some(coverage),
        selection,
        ..Default::default()
    }
}
fn parameters() -> abi::ToolParameters {
    abi::ToolParameters {
        radius: 6.,
        opacity: 0.4,
        hardness: 0.85,
        color: 0x3366cc,
        stabilizer: 0.35,
        pressure_gamma: 1.,
        minimum_radius: 0.08,
        flags: abi::PRESSURE_SIZE | abi::PRESSURE_OPACITY,
    }
}
fn sample(phase: u32, i: u32) -> abi::ToolSample {
    abi::ToolSample {
        phase,
        flags: abi::PEN,
        x: 5. + i as f32 * 1.2,
        y: 32. + ((i % 7) as f32 - 3.) * 1.3,
        pressure: 0.2 + (i % 9) as f32 * 0.09,
        time: 100 + i * 8,
        view_x: 0.,
        view_y: 0.,
        width: 64,
        height: 64,
        zoom: 1.,
    }
}

#[test]
fn analytic_tip_scalar_coverage_matches_independent_f64_oracle() {
    for shape in [abi::TIP_ROUND, abi::TIP_SQUARE] {
        for q in [0.25, 0.5, 1.] {
            for angle in [-135., 0., 37., 90.] {
                for radius in [0.5, 2.5, 9.5] {
                    let geometry = Tip::new(shape, q, angle).unwrap().geometry(radius);
                    for hardness in [0., 0.5, 1.] {
                        for y in -13..=13 {
                            for x in -13..=13 {
                                let (dx, dy) = (x as f32 * 0.7 + 0.13, y as f32 * 0.7 - 0.21);
                                let actual = geometry.coverage(dx, dy, hardness);
                                let expected = reference(
                                    shape,
                                    q as f64,
                                    angle as f64,
                                    radius as f64,
                                    dx as f64,
                                    dy as f64,
                                    hardness as f64,
                                );
                                assert!((f64::from(actual)-expected).abs()<0.00003,"shape={shape} q={q} angle={angle} r={radius} x={dx} y={dy}: {actual} != {expected}");
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn analytic_tip_regions_and_scanlines_never_skip_positive_coverage() {
    for shape in [abi::TIP_ROUND, abi::TIP_SQUARE] {
        for q in [0.25, 0.5, 0.9] {
            for angle in [-180., -37., 0., 45., 90.] {
                for radius in [0.5, 2.5, 9.5] {
                    let geometry = Tip::new(shape, q, angle).unwrap().geometry(radius);
                    let (cx, cy) = (15.37, 14.81);
                    let region = geometry.region(cx, cy, 32, 31);
                    let mut candidates = 0usize;
                    for y in 0..31 {
                        let span = geometry.row_span(cx, y as f32 + 0.5 - cy, 0, 32);
                        if let Some((lo, hi)) = span {
                            candidates += hi - lo;
                        }
                        for x in 0..32 {
                            if reference(
                                shape,
                                q as f64,
                                angle as f64,
                                radius as f64,
                                x as f64 + 0.5 - cx as f64,
                                y as f64 + 0.5 - cy as f64,
                                1.,
                            ) > 0.00001
                            {
                                assert!(
                                    x >= region.x
                                        && x < region.x + region.width
                                        && y >= region.y
                                        && y < region.y + region.height
                                );
                                let (lo, hi) =
                                    span.expect("Positive coverage must have a candidate row");
                                assert!((lo..hi).contains(&(x as usize)));
                            }
                        }
                    }
                    assert!(candidates <= 32 * 31);
                }
            }
        }
    }
    // Large-tip bounds are metadata only; do not allocate a large image.
    for shape in [abi::TIP_ROUND, abi::TIP_SQUARE] {
        let region = Tip::new(shape, 1., 45.)
            .unwrap()
            .geometry(200.)
            .region(2048., 2048., 4096, 4096);
        assert!(region.width <= 569 && region.height <= 569);
    }
}
#[test]
fn analytic_tip_configuration_rejects_malformed_tails_before_changing_state_and_resets_legacy() {
    assert_ne!(crate::OPTIONS.features & abi::TOOL_OPTION_TIP_GEOMETRY, 0);
    let mut state = State::default();
    let pointer = (&mut state as *mut State).cast();
    let mut valid = tip_options(abi::TIP_SQUARE, 0.25, 37., true, 0.25);
    assert_eq!(
        unsafe { configure(pointer, &valid.base.base.base.base) },
        abi::OK
    );
    let old = state.tip;
    for mode in 0..10 {
        let mut invalid = tip_options(abi::TIP_SQUARE, 0.25, 37., true, 0.25);
        match mode {
            0 => invalid.tip_shape = 2,
            1 => invalid.roundness = 0.24999,
            2 => invalid.roundness = 1.0001,
            3 => invalid.roundness = f32::NAN,
            4 => invalid.angle = 180.01,
            5 => invalid.angle = f32::INFINITY,
            6 => invalid.reserved = 1,
            7 => invalid.base.reserved = 1,
            8 => invalid.base.flow = f32::NAN,
            _ => invalid.base.base.base.flags &= !abi::TOOL_OPTION_TIP_GEOMETRY,
        }
        assert_eq!(
            unsafe { configure(pointer, &invalid.base.base.base.base) },
            abi::ERROR
        );
        assert_eq!(state.tip, old);
        assert_eq!(state.flow, 0.25);
        assert!(state.stroke_opacity);
    }
    for size in 65..80 {
        valid.base.base.base.base.struct_size = size;
        assert_eq!(
            unsafe { configure(pointer, &valid.base.base.base.base) },
            abi::ERROR
        );
        assert_eq!(state.tip, old);
    }
    for size in [16, 20, 24, 56, 64] {
        let full = tip_options(abi::TIP_SQUARE, 0.25, 37., true, 0.25);
        assert_eq!(
            unsafe { configure(pointer, &full.base.base.base.base) },
            abi::OK
        );
        let mut legacy = tip_options(abi::TIP_SQUARE, 0.25, 37., false, 1.);
        legacy.base.base.base.flags = 0;
        legacy.base.base.base.base.struct_size = size;
        assert_eq!(
            unsafe { configure(pointer, &legacy.base.base.base.base) },
            abi::OK
        );
        assert_eq!(state.tip, Tip::default());
        assert_eq!(state.flow, 1.);
        assert!(!state.stroke_opacity);
    }
}
#[test]
fn analytic_tip_square_has_independent_three_row_coverage_and_exact_quarter_turn() {
    let expected = [
        [0u32; 7],
        [0; 7],
        [0, 191, 191, 191, 191, 191, 0],
        [0, 255, 255, 255, 255, 255, 0],
        [0, 191, 191, 191, 191, 191, 0],
        [0; 7],
        [0; 7],
    ];
    for angle in [0., 90., -90., 180.] {
        let mut host = Host::new(7, 7, 0, 255);
        let table = host_table(&mut host, false);
        let mut raster = raster(
            &host,
            Tip::new(abi::TIP_SQUARE, 0.5, angle).unwrap(),
            false,
            false,
            Default::default(),
        );
        raster.flow = 1.;
        raster.begin(
            &table.base.base,
            3.5,
            3.5,
            Brush {
                radius: 2.5,
                opacity: 1.,
                ..brush(1., false)
            },
        );
        assert!(!raster.failed);
        for y in 0..7 {
            for x in 0..7 {
                let expected = if angle.abs() == 90. {
                    expected[x][y]
                } else {
                    expected[y][x]
                };
                assert_eq!(
                    host.pixels[y * host.stride + x] >> 24,
                    expected,
                    "{angle} {x},{y}"
                );
            }
        }
        host.gaps_unchanged();
    }
}
#[test]
fn analytic_tip_rgba_and_a8_share_coverage_selection_flow_and_capped_influence() {
    let selection = vec![128u8; 13 * 11];
    let coverage = abi::Coverage {
        data: selection.as_ptr(),
        len: selection.len(),
        width: 13,
        height: 11,
        stride: 13,
    };
    for shape in [abi::TIP_ROUND, abi::TIP_SQUARE] {
        for capped in [false, true] {
            for angle in [0., 37., 90.] {
                let tip = Tip::new(shape, 0.5, angle).unwrap();
                let mut rgba = Host::new(13, 11, 0, 255);
                let rgba_table = host_table(&mut rgba, false);
                let mut a8 = Host::new(13, 11, 0, 255);
                let a8_table = host_table(&mut a8, true);
                let mut rgba_raster = raster(&rgba, tip, false, capped, coverage);
                let mut a8_raster = raster(&a8, tip, true, capped, coverage);
                let b = Brush {
                    radius: 3.5,
                    opacity: 0.4,
                    hardness: 0.5,
                    color: 0,
                    erase: false,
                };
                for _ in 0..5 {
                    rgba_raster.begin(&rgba_table.base.base, 6.5, 5.5, b);
                    a8_raster.begin(&a8_table.base.base, 6.5, 5.5, b);
                }
                assert!(!rgba_raster.failed && !a8_raster.failed);
                for y in 0..11 {
                    for x in 0..13 {
                        let alpha = rgba.pixels[y * rgba.stride + x] >> 24;
                        let mask = a8.mask[y * a8.stride + x];
                        assert_eq!(
                            alpha + u32::from(mask),
                            255,
                            "{shape} {capped} {angle} {x},{y}"
                        );
                        if capped {
                            assert!(alpha <= 51);
                        }
                    }
                }
                rgba.gaps_unchanged();
                a8.gaps_unchanged();
                let center = 5 * rgba.stride + 6;
                let mut expected = 0u32;
                if capped {
                    let cap = (65535f32 * 0.4).round() as u32;
                    let amount = (65535f32 * 0.25).round() as u32;
                    for _ in 0..5 {
                        expected += ((cap - expected) * amount + 32767) / 65535;
                    }
                    expected = (expected * 128 + 32767) / 65535;
                } else {
                    let a = (255f32 * (0.4 * 0.25)).round() as u32;
                    let selected = (a * 128 + 127) / 255;
                    for _ in 0..5 {
                        expected = selected + (expected * (255 - selected) + 127) / 255;
                    }
                }
                assert_eq!(rgba.pixels[center] >> 24, expected);
            }
        }
    }
}
#[test]
fn analytic_tip_roundness_one_keeps_legacy_bytes_and_nonround_spacing_is_bounded() {
    for a8 in [false, true] {
        for capped in [false, true] {
            for hardness in [0., 0.6, 1.] {
                let mut baseline = Host::new(23, 19, 0x80604020, 91);
                let baseline_table = host_table(&mut baseline, a8);
                let mut first = raster(&baseline, Tip::default(), a8, capped, Default::default());
                let b = Brush {
                    hardness,
                    radius: 4.5,
                    ..brush(0.4, false)
                };
                first.begin(&baseline_table.base.base, 7.2, 8.6, b);
                first.line(&baseline_table.base.base, 17.3, 12.1, b);
                for angle in [-180., 37., 90.] {
                    let mut host = Host::new(23, 19, 0x80604020, 91);
                    let table = host_table(&mut host, a8);
                    let mut other = raster(
                        &host,
                        Tip::new(abi::TIP_ROUND, 1., angle).unwrap(),
                        a8,
                        capped,
                        Default::default(),
                    );
                    other.begin(&table.base.base, 7.2, 8.6, b);
                    other.line(&table.base.base, 17.3, 12.1, b);
                    assert_eq!(host.pixels, baseline.pixels);
                    assert_eq!(host.mask, baseline.mask);
                }
            }
        }
    }
    let mut counts = Vec::new();
    for q in [1., 0.25] {
        let mut host = Host::new(64, 64, 0, 255);
        let table = host_table(&mut host, false);
        let mut raster = raster(
            &host,
            Tip::new(abi::TIP_SQUARE, q, 37.).unwrap(),
            false,
            false,
            Default::default(),
        );
        probe::reset();
        raster.begin(&table.base.base, 8., 32., brush(0.4, false));
        raster.line(&table.base.base, 56., 32., brush(0.4, false));
        counts.push(probe::get().dabs);
        assert!(!raster.failed);
    }
    assert!(counts[1] > counts[0]);
    assert!(counts[1] <= counts[0] * 4 + 1);
}

#[test]
fn analytic_tip_pressure_scales_both_axes_and_configure_never_changes_an_active_contact() {
    for a8 in [false, true] {
        for capped in [false, true] {
            let mut host = Host::new(7, 7, 0, 255);
            let table = host_table(&mut host, a8);
            let mut state = State::default();
            let pointer = (&mut state as *mut State).cast();
            let mut op = tip_options(abi::TIP_SQUARE, 0.5, 0., capped, 1.);
            if a8 {
                op.base.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
            }
            assert_eq!(
                unsafe { configure(pointer, &op.base.base.base.base) },
                abi::OK
            );
            let p = abi::ToolParameters {
                radius: 4.,
                opacity: 1.,
                hardness: 1.,
                color: 0,
                stabilizer: 0.,
                pressure_gamma: 1.,
                minimum_radius: 0.25,
                flags: abi::PRESSURE_SIZE,
            };
            let down = abi::ToolSample {
                phase: abi::DOWN,
                x: 3.5,
                y: 3.5,
                pressure: 0.5,
                width: 7,
                height: 7,
                ..sample(abi::DOWN, 0)
            };
            probe::reset();
            assert_eq!(
                state.input(1, down, Some(p), &table.base.base, &mut Default::default()),
                abi::OK
            );
            let before = state.tip;
            let changed = tip_options(abi::TIP_ROUND, 0.25, 90., false, 1.);
            assert_eq!(
                unsafe { configure(pointer, &changed.base.base.base.base) },
                abi::ERROR
            );
            assert_eq!(state.tip, before);
            for y in 0..7 {
                for x in 0..7 {
                    let expected = if (1..=5).contains(&x) {
                        match y {
                            2 | 4 => 191,
                            3 => 255,
                            _ => 0,
                        }
                    } else {
                        0
                    };
                    let actual = if a8 {
                        255 - u32::from(host.mask[y * host.stride + x])
                    } else {
                        host.pixels[y * host.stride + x] >> 24
                    };
                    assert_eq!(actual, expected, "pressure axes {a8} {capped} {x},{y}");
                }
            }
            assert_eq!(
                state.input(
                    1,
                    abi::ToolSample {
                        phase: abi::UP,
                        pressure: 0.,
                        time: 120,
                        ..down
                    },
                    None,
                    &table.base.base,
                    &mut Default::default()
                ),
                abi::OK
            );
            assert!(state.contact.is_none());
            assert_eq!(probe::get().dabs, 1);
            host.gaps_unchanged();
            // Zero pressure is not a minimum-size paint deposit, on either domain.
            let mut empty = Host::new(7, 7, 0, 255);
            let table = host_table(&mut empty, a8);
            assert_eq!(
                state.input(
                    1,
                    abi::ToolSample {
                        pressure: 0.,
                        time: 140,
                        ..down
                    },
                    Some(p),
                    &table.base.base,
                    &mut Default::default()
                ),
                abi::OK
            );
            assert_eq!(
                state.input(
                    1,
                    abi::ToolSample {
                        phase: abi::UP,
                        pressure: 0.,
                        time: 160,
                        ..down
                    },
                    None,
                    &table.base.base,
                    &mut Default::default()
                ),
                abi::OK
            );
            assert_eq!(empty.calls, 0);
            for y in 0..7 {
                assert!(empty.pixels[y * empty.stride..y * empty.stride + 7]
                    .iter()
                    .all(|v| *v == 0));
                assert!(empty.mask[y * empty.stride..y * empty.stride + 7]
                    .iter()
                    .all(|v| *v == 255));
            }
            empty.gaps_unchanged();
        }
    }
}
#[test]
fn analytic_tip_capped_loan_failure_ends_native_contact_without_retaining_host_pointers() {
    for a8 in [false, true] {
        let mut host = Host::new(64, 64, 0x80604020, 91);
        let table = host_table(&mut host, a8);
        let mut state = State::default();
        let pointer = (&mut state as *mut State).cast();
        let mut op = tip_options(abi::TIP_SQUARE, 0.25, 37., true, 0.25);
        if a8 {
            op.base.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
        }
        assert_eq!(
            unsafe { configure(pointer, &op.base.base.base.base.base) },
            abi::OK
        );
        assert_eq!(
            unsafe {
                crate::pointer(
                    pointer,
                    1,
                    &sample(abi::DOWN, 0),
                    &parameters(),
                    &table.base.base,
                    &mut Default::default(),
                )
            },
            abi::OK
        );
        host.fail_at = Some(host.calls + 1);
        assert_eq!(
            unsafe {
                crate::pointer(
                    pointer,
                    1,
                    &sample(abi::MOVE, 1),
                    std::ptr::null(),
                    &table.base.base,
                    &mut Default::default(),
                )
            },
            abi::ERROR
        );
        assert!(state.contact.is_none());
        assert!(state.stroke_cache.tiles.is_empty());
        assert!(host.original.iter().all(|p| *p == 0x80604020));
        assert!(host.original_a8.iter().all(|p| *p == 91));
        host.gaps_unchanged();
        host.fail_at = None;
        let mut reset = super::options(false, 1.);
        if a8 {
            reset.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
        }
        assert_eq!(
            unsafe { configure(pointer, &reset.base.base.base) },
            abi::OK
        );
        assert_eq!(state.tip, Tip::default());
        assert_eq!(
            unsafe {
                crate::pointer(
                    pointer,
                    1,
                    &sample(abi::DOWN, 2),
                    &parameters(),
                    &table.base.base,
                    &mut Default::default(),
                )
            },
            abi::OK
        );
        assert_eq!(
            unsafe {
                crate::pointer(
                    pointer,
                    1,
                    &sample(abi::CANCEL, 2),
                    std::ptr::null(),
                    &table.base.base,
                    &mut Default::default(),
                )
            },
            abi::OK
        );
        assert!(state.contact.is_none());
    }
}
#[test]
#[ignore = "Bounded Release-only analytic tip contact/candidate probe; selected explicitly by CI"]
fn bounded_tip_cost() {
    use std::time::Instant;
    for a8 in [false, true] {
        for capped in [false, true] {
            for (name, shape, q, angle) in [
                ("round", abi::TIP_ROUND, 1., 0.),
                ("ellipse", abi::TIP_ROUND, 0.25, 37.),
                ("square", abi::TIP_SQUARE, 1., 0.),
                ("chisel", abi::TIP_SQUARE, 0.25, 37.),
            ] {
                let mut host = Host::new(64, 64, 0x80604020, 91);
                let table = host_table(&mut host, a8);
                let mut state = State::default();
                let mut op = tip_options(shape, q, angle, capped, 0.25);
                if a8 {
                    op.base.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
                }
                assert_eq!(
                    unsafe {
                        configure((&mut state as *mut State).cast(), &op.base.base.base.base)
                    },
                    abi::OK
                );
                let mut elapsed = Vec::new();
                let mut counts = probe::Counts::default();
                for run in 0..10 {
                    host.lift();
                    probe::reset();
                    let begin = Instant::now();
                    for i in 0..41 {
                        assert_eq!(
                            state.input(
                                1,
                                sample(
                                    if i == 0 {
                                        abi::DOWN
                                    } else if i == 40 {
                                        abi::UP
                                    } else {
                                        abi::MOVE
                                    },
                                    i
                                ),
                                (i == 0).then_some(parameters()),
                                &table.base.base,
                                &mut Default::default()
                            ),
                            abi::OK
                        );
                    }
                    if run != 0 {
                        elapsed.push(begin.elapsed().as_nanos());
                        counts = probe::get();
                    }
                    assert!(state.contact.is_none());
                    host.gaps_unchanged();
                    assert!(state.stroke_cache.tiles.len() <= 64);
                    if capped {
                        assert_eq!(state.stroke_cache.allocations, 1);
                    } else {
                        assert!(state.stroke_cache.tiles.is_empty());
                    }
                    assert!(probe::get().dabs <= 4096);
                    assert!(probe::get().candidates <= probe::get().loan_pixels);
                    assert!(probe::get().positive_visits <= probe::get().candidates);
                }
                elapsed.sort_unstable();
                println!("tip_cost shape={name} domain={} mode={} frame=64x64 samples=41 contacts=9 p50_ns={} p95_ns={} dabs={} candidate_pixels={} positive_pixel_visits={} loan_regions={} loan_roi_pixel_sum={} retained_coverage_bytes={} coverage_tile_allocations={} scope=instrumented_native_contact_synthetic excludes=frame_setup,undo_capture,Session,journal,GUI,tablet",
                if a8 {"a8"} else {"rgba"},if capped {"stroke"} else {"dab"},elapsed[4],elapsed[8],counts.dabs,counts.candidates,
                counts.positive_visits,counts.loan_regions,counts.loan_pixels,state.stroke_cache.tiles.len()*AREA*2,state.stroke_cache.allocations);
            }
        }
    }
}
