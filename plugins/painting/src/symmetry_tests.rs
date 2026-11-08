//! Native reflection contracts, configuration admission and shared contact math.
use super::*;
use crate::{
    symmetry::Symmetry,
    tip::{probe, Tip},
};

fn options_symmetry(axes: u32, capped: bool, a8: bool) -> abi::ToolOptionsSymmetry {
    let mut base = super::options(capped, 0.4);
    base.base.base.base.struct_size = std::mem::size_of::<abi::ToolOptionsSymmetry>() as u32;
    base.base.base.flags |= abi::TOOL_OPTION_TIP_GEOMETRY | abi::TOOL_OPTION_SYMMETRY;
    if a8 {
        base.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
    }
    abi::ToolOptionsSymmetry {
        base: abi::ToolOptionsTip {
            base,
            tip_shape: abi::TIP_SQUARE,
            roundness: 0.5,
            angle: 30.,
            reserved: 0,
        },
        axes,
        anchor_x: 0.5,
        anchor_y: 0.5,
        reserved: 0,
    }
}
fn host_table(host: &mut Host, a8: bool) -> abi::ToolHostStroke {
    let mut table = host.table(a8);
    table.base.base.pixels = legacy_rgba;
    table.base.a8 = a8.then_some(legacy_a8);
    table
}
#[test]
fn symmetry_configuration_is_optional_strict_frozen_and_legacy_resettable() {
    assert_ne!(crate::OPTIONS.features & abi::TOOL_OPTION_SYMMETRY, 0);
    let mut state = State::default();
    let pointer = (&mut state as *mut State).cast();
    let valid = options_symmetry(3, true, false);
    assert_eq!(
        unsafe { configure(pointer, &valid.base.base.base.base.base) },
        abi::OK
    );
    let old = state.symmetry;
    for case in 0..11 {
        let mut op = options_symmetry(3, true, false);
        match case {
            0 => op.axes = 4,
            1 => op.anchor_x = f32::NAN,
            2 => op.anchor_y = f32::INFINITY,
            3 => op.anchor_x = -0.01,
            4 => op.anchor_y = 1.01,
            5 => op.reserved = 1,
            6 => op.base.base.base.base.flags &= !abi::TOOL_OPTION_SYMMETRY,
            7 => op.base.reserved = 1,
            8 => op.base.base.reserved = 1,
            9 => op.base.base.flow = f32::NAN,
            _ => op.base.tip_shape = 2,
        }
        assert_eq!(
            unsafe { configure(pointer, &op.base.base.base.base.base) },
            abi::ERROR
        );
        assert_eq!(state.symmetry, old);
        assert!(state.stroke_opacity);
    }
    for size in 80..96 {
        let mut op = options_symmetry(3, true, false);
        op.base.base.base.base.base.struct_size = size;
        assert_eq!(
            unsafe { configure(pointer, &op.base.base.base.base.base) },
            abi::ERROR
        );
        assert_eq!(state.symmetry, old);
    }
    // A genuine old allocation is never read as the new tail.
    let legacy = abi::ToolOptions {
        abi_version: 1,
        struct_size: 16,
        spacing: 0.11,
        minimum_opacity: 0.,
    };
    assert_eq!(unsafe { configure(pointer, &legacy) }, abi::OK);
    assert_eq!(state.symmetry, Symmetry::default());
    let mut old_tip = options_symmetry(3, false, false).base;
    old_tip.base.base.base.base.struct_size = 80;
    old_tip.base.base.base.flags &= !abi::TOOL_OPTION_SYMMETRY;
    assert_eq!(
        unsafe { configure(pointer, &old_tip.base.base.base.base) },
        abi::OK
    );
    assert_eq!(state.symmetry, Symmetry::default());
    assert_eq!(state.tip, Tip::new(abi::TIP_SQUARE, 0.5, 30.).unwrap());
}
#[test]
fn reflection_points_tip_parity_and_axis_dedup_are_fixed_size() {
    let tip = Tip::new(abi::TIP_SQUARE, 0.5, 30.).unwrap();
    let symmetry = Symmetry::new(3, 0.25, 0.75).unwrap().resolve(20, 12);
    let (stamps, count) = symmetry.stamps(2., 3., tip);
    assert_eq!(count, 4);
    assert_eq!(
        stamps.map(|s| (s.x, s.y)),
        [(2., 3.), (8., 3.), (2., 15.), (8., 15.)]
    );
    assert_eq!(stamps[0].tip, stamps[3].tip);
    assert_eq!(stamps[1].tip, Tip::new(abi::TIP_SQUARE, 0.5, -30.).unwrap());
    assert_eq!(stamps[1].tip, stamps[2].tip);
    for tip in [
        Tip::default(),
        Tip::new(abi::TIP_SQUARE, 0.5, 90.).unwrap(),
        Tip::new(abi::TIP_SQUARE, 1., 45.).unwrap(),
    ] {
        assert_eq!(symmetry.stamps(5., 9., tip).1, 1);
        assert_eq!(symmetry.stamps(5., 3., tip).1, 2);
        assert_eq!(symmetry.stamps(2., 9., tip).1, 2);
    }
}
fn paint(
    host: &mut Host,
    axes: u32,
    capped: bool,
    a8: bool,
    selection: Option<&[u8]>,
    handler: u32,
) {
    let mut state = State::default();
    let pointer = (&mut state as *mut State).cast();
    let mut op = options_symmetry(axes, capped, a8);
    if let Some(selection) = selection {
        op.base.base.base.base.flags |= abi::TOOL_OPTION_SELECTION;
        op.base.base.base.selection = abi::Coverage {
            data: selection.as_ptr(),
            len: selection.len(),
            width: 64,
            height: 64,
            stride: 64,
        };
    }
    assert_eq!(
        unsafe { configure(pointer, &op.base.base.base.base.base) },
        abi::OK
    );
    let table = host_table(host, a8);
    let parameters = abi::ToolParameters {
        radius: 3.,
        opacity: 0.65,
        hardness: 0.7,
        color: 0x3366cc,
        stabilizer: 0.35,
        pressure_gamma: 1.,
        minimum_radius: 0.1,
        flags: abi::PRESSURE_SIZE | abi::PRESSURE_OPACITY,
    };
    for i in 0..=8 {
        let sample = abi::ToolSample {
            phase: if i == 0 {
                abi::DOWN
            } else if i == 8 {
                abi::UP
            } else {
                abi::MOVE
            },
            flags: abi::PEN,
            x: 7. + i as f32 * 0.75,
            y: 9. + (i % 3) as f32 * 0.5,
            pressure: if i == 8 { 0. } else { 0.25 + i as f32 * 0.08 },
            time: 100 + i * 8,
            view_x: 0.,
            view_y: 0.,
            width: 64,
            height: 64,
            zoom: 1.,
        };
        assert_eq!(
            unsafe {
                crate::pointer(
                    pointer,
                    handler,
                    &sample,
                    &parameters,
                    &table.base.base,
                    &mut Default::default(),
                )
            },
            abi::OK
        );
        if i == 0 {
            assert_eq!(
                unsafe { configure(pointer, &op.base.base.base.base.base) },
                abi::ERROR
            );
            assert_eq!(state.symmetry.axes, axes);
        }
    }
    assert!(state.contact.is_none());
    assert!(state.filter.finish().is_none());
    host.gaps_unchanged();
}
#[test]
fn mirrored_rgba_a8_pressure_stabilization_and_flow_match_reflected_original_contacts() {
    for a8 in [false, true] {
        for capped in [false, true] {
            for handler in [1, 2] {
                let mut original = Host::new(64, 64, 0x80604020, 91);
                paint(&mut original, 0, capped, a8, None, handler);
                assert!((0..64).any(|y| (0..64).any(|x| if a8 {
                    original.mask[y * original.stride + x] != 91
                } else {
                    original.pixels[y * original.stride + x] != 0x80604020
                })));
                for axes in [1, 2, 3] {
                    let mut mirrored = Host::new(64, 64, 0x80604020, 91);
                    paint(&mut mirrored, axes, capped, a8, None, handler);
                    for y in 0..64 {
                        for x in 0..64 {
                            let sx = if axes & 1 != 0 && x >= 32 { 63 - x } else { x };
                            let sy = if axes & 2 != 0 && y >= 32 { 63 - y } else { y };
                            let actual = if a8 {
                                u32::from(mirrored.mask[y * mirrored.stride + x])
                            } else {
                                mirrored.pixels[y * mirrored.stride + x]
                            };
                            let expected = if a8 {
                                u32::from(original.mask[sy * original.stride + sx])
                            } else {
                                original.pixels[sy * original.stride + sx]
                            };
                            // f32 reflection can move a channel across a byte-rounding
                            // boundary. Compare every stored channel, not just alpha.
                            for shift in [0, 8, 16, 24].into_iter().take(if a8 { 1 } else { 4 }) {
                                let a = (actual >> shift) & 255;
                                let b = (expected >> shift) & 255;
                                assert!(a.abs_diff(b)<=1,"a8={a8} capped={capped} handler={handler} axes={axes} at {x},{y}/{shift}: {a}/{b}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn selection_is_sampled_at_each_destination_without_reflecting_its_plane() {
    let mut selection = vec![0u8; 64 * 64];
    for y in 0..32 {
        for x in 32..64 {
            selection[y * 64 + x] = 128;
        }
    }
    for a8 in [false, true] {
        for capped in [false, true] {
            let mut host = Host::new(64, 64, 0x80604020, 91);
            paint(&mut host, 3, capped, a8, Some(&selection), 1);
            let mut changed = 0;
            for y in 0..64 {
                for x in 0..64 {
                    let changed_here = if a8 {
                        host.mask[y * host.stride + x] != 91
                    } else {
                        host.pixels[y * host.stride + x] != 0x80604020
                    };
                    if changed_here {
                        assert!(x >= 32 && y < 32);
                        changed += 1;
                    }
                }
            }
            assert!(changed > 0);
        }
    }
}
#[test]
fn axis_dedup_and_four_remote_dabs_preserve_bounded_loans_and_capped_storage() {
    for capped in [false, true] {
        let mut host = Host::new(64, 64, 0, 255);
        let table = host_table(&mut host, false);
        let mut raster = Raster {
            width: 64,
            height: 64,
            tip: Tip::default(),
            flow: 0.5,
            symmetry: Symmetry::new(3, 0.5, 0.5).unwrap().resolve(64, 64),
            ..Default::default()
        };
        if capped {
            let mut coverage = Coverage::default();
            coverage.begin(64, 64).unwrap();
            raster.stroke = Some(coverage);
        }
        let brush = Brush {
            radius: 1.,
            opacity: 0.5,
            hardness: 1.,
            color: 0x3366cc,
            erase: false,
        };
        probe::reset();
        raster.begin(&table.base.base, 32., 32., brush);
        assert!(!raster.failed);
        assert_eq!(probe::get().dabs, 1);
        let once = host.pixels.clone();
        let mut single = Host::new(64, 64, 0, 255);
        let single_table = host_table(&mut single, false);
        let mut plain = Raster {
            width: 64,
            height: 64,
            flow: 0.5,
            ..Default::default()
        };
        if capped {
            let mut coverage = Coverage::default();
            coverage.begin(64, 64).unwrap();
            plain.stroke = Some(coverage);
        }
        plain.begin(&single_table.base.base, 32., 32., brush);
        assert_eq!(once, single.pixels);
        probe::reset();
        raster.begin(&table.base.base, 5., 7., brush);
        let counts = probe::get();
        assert_eq!(counts.dabs, 4);
        assert_eq!(counts.loan_regions, 4);
        assert!(counts.loan_pixels <= 4 * 25, "{counts:?}");
        host.gaps_unchanged();
    }
}
#[test]
fn failed_reflected_loan_ends_the_contact_and_releases_shared_coverage() {
    let mut host = Host::new(64, 64, 0, 255);
    host.fail_at = Some(2);
    let table = host_table(&mut host, false);
    let mut state = State::default();
    let pointer = (&mut state as *mut State).cast();
    let op = options_symmetry(3, true, false);
    assert_eq!(
        unsafe { configure(pointer, &op.base.base.base.base.base) },
        abi::OK
    );
    let sample = abi::ToolSample {
        phase: abi::DOWN,
        flags: abi::PEN,
        x: 7.,
        y: 9.,
        pressure: 0.8,
        time: 100,
        view_x: 0.,
        view_y: 0.,
        width: 64,
        height: 64,
        zoom: 1.,
    };
    let parameters = abi::ToolParameters {
        radius: 2.,
        opacity: 1.,
        hardness: 1.,
        color: 0x3366cc,
        stabilizer: 0.35,
        pressure_gamma: 1.,
        minimum_radius: 0.1,
        flags: abi::PRESSURE_SIZE | abi::PRESSURE_OPACITY,
    };
    assert_eq!(
        unsafe {
            crate::pointer(
                pointer,
                1,
                &sample,
                &parameters,
                &table.base.base,
                &mut Default::default(),
            )
        },
        abi::ERROR
    );
    assert!(state.contact.is_none());
    assert!(state.stroke_cache.tiles.is_empty());
    assert_eq!(host.calls, 2);
    assert!(state.filter.finish().is_none());
    host.gaps_unchanged();
}

#[test]
fn simple_four_point_stamp_matches_independent_rgba_and_a8_bytes() {
    for a8 in [false, true] {
        for anchor in [0.5, 0.] {
            let mut host = Host::new(12, 10, 0, 255);
            let table = host_table(&mut host, a8);
            let mut raster = Raster {
                width: 12,
                height: 10,
                a8_value: a8.then_some(0),
                symmetry: Symmetry::new(3, anchor, anchor).unwrap().resolve(12, 10),
                ..Default::default()
            };
            raster.begin(
                &table.base.base,
                2.5,
                3.5,
                Brush {
                    radius: 0.5,
                    opacity: 0.5,
                    hardness: 1.,
                    color: 0x3366cc,
                    erase: false,
                },
            );
            assert!(!raster.failed);
            for y in 0..10 {
                for x in 0..12 {
                    let painted = if anchor == 0. {
                        (x, y) == (2, 3)
                    } else {
                        [(2, 3), (9, 3), (2, 6), (9, 6)].contains(&(x, y))
                    };
                    if a8 {
                        assert_eq!(
                            host.mask[y * host.stride + x],
                            if painted { 127 } else { 255 }
                        );
                    } else {
                        assert_eq!(
                            host.pixels[y * host.stride + x],
                            if painted { 0x803366cc } else { 0 }
                        );
                    }
                }
            }
            host.gaps_unchanged();
        }
    }
}

#[test]
fn overlapping_reflected_shapes_share_one_stroke_opacity_cap_and_alpha_lock() {
    for capped in [false, true] {
        let mut host = Host::new(64, 64, 0, 255);
        let table = host_table(&mut host, false);
        let mut raster = Raster {
            width: 64,
            height: 64,
            tip: Tip::new(abi::TIP_SQUARE, 0.5, 30.).unwrap(),
            symmetry: Symmetry::new(3, 0.5, 0.5).unwrap().resolve(64, 64),
            ..Default::default()
        };
        if capped {
            let mut coverage = Coverage::default();
            coverage.begin(64, 64).unwrap();
            raster.stroke = Some(coverage);
        }
        raster.begin(
            &table.base.base,
            32.,
            32.,
            Brush {
                radius: 4.,
                opacity: 0.5,
                hardness: 1.,
                color: 0x3366cc,
                erase: false,
            },
        );
        assert!(!raster.failed);
        let alpha = (0..64)
            .flat_map(|y| (0..64).map(move |x| (x, y)))
            .map(|(x, y)| host.pixels[y * host.stride + x] >> 24)
            .max()
            .unwrap();
        assert!(alpha > 0);
        if capped {
            assert!(alpha <= 128);
        } else {
            assert!(alpha > 128);
        }
        let mut locked = Host::new(64, 64, 0x80604020, 255);
        let table = host_table(&mut locked, false);
        raster.stroke = None;
        raster.alpha_locked = true;
        raster.begin(
            &table.base.base,
            8.,
            9.,
            Brush {
                radius: 2.,
                opacity: 1.,
                hardness: 1.,
                color: 0x3366cc,
                erase: false,
            },
        );
        for y in 0..64 {
            for x in 0..64 {
                assert_eq!(locked.pixels[y * locked.stride + x] >> 24, 128);
            }
        }
        locked.gaps_unchanged();
    }
}
