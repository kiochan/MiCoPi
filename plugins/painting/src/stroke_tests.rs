use super::*;
use crate::{configure, Raster, State};
#[path = "tip_tests.rs"]
mod tip_tests;

const GAP: u32 = 0xa1b2c3d4;
struct Host {
    width: usize,
    height: usize,
    stride: usize,
    original: Vec<u32>,
    pixels: Vec<u32>,
    original_a8: Vec<u8>,
    mask: Vec<u8>,
    calls: usize,
    fail_at: Option<usize>,
}
impl Host {
    fn new(width: usize, height: usize, pixel: u32, mask: u8) -> Self {
        let stride = width + 3;
        let mut result = Self {
            width,
            height,
            stride,
            original: vec![pixel; width * height],
            pixels: vec![GAP; stride * height],
            original_a8: vec![mask; width * height],
            mask: vec![17; stride * height],
            calls: 0,
            fail_at: None,
        };
        for y in 0..height {
            for x in 0..width {
                result.pixels[y * stride + x] = pixel;
                result.mask[y * stride + x] = mask;
            }
        }
        result
    }
    fn table(&mut self, a8: bool) -> abi::ToolHostStroke {
        abi::ToolHostStroke {
            base: abi::ToolHostA8 {
                base: abi::ToolHost {
                    abi_version: abi::TOOL_ABI_VERSION,
                    struct_size: std::mem::size_of::<abi::ToolHostStroke>() as u32,
                    context: (self as *mut Self).cast(),
                    pixels: reject_rgba,
                    pick: reject_pick,
                },
                a8: a8.then_some(reject_a8),
            },
            features: if a8 {
                abi::TOOL_HOST_ORIGIN_A8
            } else {
                abi::TOOL_HOST_ORIGIN_RGBA
            },
            tile_side: 64,
            pixels_origin: (!a8).then_some(rgba),
            a8_origin: a8.then_some(a8_loan),
        }
    }
    fn lift(&mut self) {
        for y in 0..self.height {
            self.original[y * self.width..(y + 1) * self.width]
                .copy_from_slice(&self.pixels[y * self.stride..y * self.stride + self.width]);
            self.original_a8[y * self.width..(y + 1) * self.width]
                .copy_from_slice(&self.mask[y * self.stride..y * self.stride + self.width]);
        }
    }
    fn gaps_unchanged(&self) {
        for y in 0..self.height {
            assert_eq!(
                &self.pixels[y * self.stride + self.width..(y + 1) * self.stride],
                &[GAP; 3]
            );
            assert_eq!(
                &self.mask[y * self.stride + self.width..(y + 1) * self.stride],
                &[17; 3]
            );
        }
    }
}
unsafe extern "C" fn reject_rgba(
    _: *mut c_void,
    _: abi::Region,
    _: Option<abi::Transform>,
    _: *mut c_void,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn reject_a8(
    _: *mut c_void,
    _: abi::Region,
    _: Option<abi::A8Transform>,
    _: *mut c_void,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn reject_pick(_: *mut c_void, _: f32, _: f32, _: *mut u32) -> i32 {
    abi::ERROR
}
fn intersections(region: abi::Region) -> impl Iterator<Item = (usize, usize, usize, usize)> {
    let (rx, ry, ex, ey) = (
        region.x as usize,
        region.y as usize,
        (region.x + region.width) as usize,
        (region.y + region.height) as usize,
    );
    // Keep the timed synthetic host free of an allocation per dab. Larger
    // reference fixtures may still gather immutable source samples below.
    (ry / SIDE..=(ey - 1) / SIDE).flat_map(move |ty| {
        (rx / SIDE..=(ex - 1) / SIDE).map(move |tx| {
            let (x, y) = (rx.max(tx * SIDE), ry.max(ty * SIDE));
            (
                x,
                y,
                ex.min((tx + 1) * SIDE) - x,
                ey.min((ty + 1) * SIDE) - y,
            )
        })
    })
}
unsafe extern "C" fn rgba(
    context: *mut c_void,
    region: abi::Region,
    callback: Option<abi::OriginTransform>,
    user: *mut c_void,
) -> i32 {
    let host = &mut *context.cast::<Host>();
    let callback = callback.unwrap();
    for (x, y, w, h) in intersections(region) {
        host.calls += 1;
        if host.fail_at == Some(host.calls) {
            return abi::ERROR;
        }
        // Test-only source gathering models an immutable edge-aware host tile.
        // Production borrows the already captured undo allocation instead.
        let owned;
        let (data, len, stride) = if host.width <= SIDE {
            (
                host.original.as_ptr().add(y * host.width + x),
                (h - 1) * host.width + w,
                host.width,
            )
        } else {
            owned = (y..y + h)
                .flat_map(|row| {
                    host.original[row * host.width + x..row * host.width + x + w]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>();
            (owned.as_ptr(), owned.len(), w)
        };
        let origin = abi::OriginPixels {
            data,
            len,
            width: w as u32,
            height: h as u32,
            stride: stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        };
        let mut output = abi::Pixels {
            data: host.pixels.as_mut_ptr().add(y * host.stride + x),
            len: (h - 1) * host.stride + w,
            width: w as u32,
            height: h as u32,
            stride: host.stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        };
        if callback(user, &origin, &mut output) != abi::OK {
            return abi::ERROR;
        }
    }
    abi::OK
}
unsafe extern "C" fn a8_loan(
    context: *mut c_void,
    region: abi::Region,
    callback: Option<abi::A8OriginTransform>,
    user: *mut c_void,
) -> i32 {
    let host = &mut *context.cast::<Host>();
    let callback = callback.unwrap();
    for (x, y, w, h) in intersections(region) {
        host.calls += 1;
        if host.fail_at == Some(host.calls) {
            return abi::ERROR;
        }
        let owned;
        let (data, len, stride) = if host.width <= SIDE {
            (
                host.original_a8.as_ptr().add(y * host.width + x),
                (h - 1) * host.width + w,
                host.width,
            )
        } else {
            owned = (y..y + h)
                .flat_map(|row| {
                    host.original_a8[row * host.width + x..row * host.width + x + w]
                        .iter()
                        .copied()
                })
                .collect::<Vec<_>>();
            (owned.as_ptr(), owned.len(), w)
        };
        let origin = abi::OriginA8Pixels {
            data,
            len,
            width: w as u32,
            height: h as u32,
            stride: stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        };
        let mut output = abi::A8Pixels {
            data: host.mask.as_mut_ptr().add(y * host.stride + x),
            len: (h - 1) * host.stride + w,
            width: w as u32,
            height: h as u32,
            stride: host.stride as u32,
            origin_x: x as u32,
            origin_y: y as u32,
        };
        if callback(user, &origin, &mut output) != abi::OK {
            return abi::ERROR;
        }
    }
    abi::OK
}
fn brush(opacity: f32, erase: bool) -> Brush {
    Brush {
        radius: 3.,
        opacity,
        hardness: 1.,
        color: 0x3366cc,
        erase,
    }
}

unsafe extern "C" fn legacy_rgba(
    context: *mut c_void,
    region: abi::Region,
    callback: Option<abi::Transform>,
    user: *mut c_void,
) -> i32 {
    let host = &mut *context.cast::<Host>();
    host.calls += 1;
    let mut output = abi::Pixels {
        data: host
            .pixels
            .as_mut_ptr()
            .add(region.y as usize * host.stride + region.x as usize),
        len: (region.height as usize - 1) * host.stride + region.width as usize,
        width: region.width,
        height: region.height,
        stride: host.stride as u32,
        origin_x: region.x,
        origin_y: region.y,
    };
    callback.unwrap()(user, &mut output)
}
unsafe extern "C" fn legacy_a8(
    context: *mut c_void,
    region: abi::Region,
    callback: Option<abi::A8Transform>,
    user: *mut c_void,
) -> i32 {
    let host = &mut *context.cast::<Host>();
    host.calls += 1;
    let mut output = abi::A8Pixels {
        data: host
            .mask
            .as_mut_ptr()
            .add(region.y as usize * host.stride + region.x as usize),
        len: (region.height as usize - 1) * host.stride + region.width as usize,
        width: region.width,
        height: region.height,
        stride: host.stride as u32,
        origin_x: region.x,
        origin_y: region.y,
    };
    callback.unwrap()(user, &mut output)
}

#[test]
fn dab_mode_flow_scales_each_deposit_and_keeps_original_build_up_without_scratch() {
    let mut host = Host::new(1, 1, 0, 0);
    let mut table = host.table(false);
    table.base.base.struct_size = std::mem::size_of::<abi::ToolHost>() as u32;
    table.base.base.pixels = legacy_rgba;
    let mut raster = Raster {
        width: 1,
        height: 1,
        flow: 0.5,
        ..Default::default()
    };
    for alpha in [51, 92, 125] {
        raster.begin(&table.base.base, 0.5, 0.5, brush(0.4, false));
        assert_eq!(host.pixels[0] >> 24, alpha);
        assert!(raster.stroke.is_none());
        assert!(!raster.failed);
    }
    host.gaps_unchanged();
}
fn stamp(
    coverage: &mut Coverage,
    host: &mut Host,
    opacity: f32,
    flow: f32,
    erase: bool,
    a8: bool,
    selection: abi::Coverage,
    locked: bool,
) -> i32 {
    let table = host.table(a8);
    coverage.dab(
        &table.base.base,
        abi::Region {
            x: 0,
            y: 0,
            width: host.width as u32,
            height: host.height as u32,
        },
        brush(opacity, erase),
        0.5,
        0.5,
        flow,
        selection,
        locked,
        a8.then_some(0),
    )
}

#[test]
fn stroke_opacity_one_contact_caps_influence_and_lifting_allows_new_paint() {
    let mut host = Host::new(1, 1, 0, 91);
    let mut coverage = Coverage::default();
    coverage.begin(1, 1).unwrap();
    for _ in 0..32 {
        assert_eq!(
            stamp(
                &mut coverage,
                &mut host,
                0.4,
                1.,
                false,
                false,
                Default::default(),
                false
            ),
            abi::OK
        );
    }
    assert_eq!(host.pixels[0], 0x663366cc);
    let original = host.original.clone();
    assert_eq!(original, [0]);
    host.lift();
    coverage.release();
    coverage.begin(1, 1).unwrap();
    assert_eq!(
        stamp(
            &mut coverage,
            &mut host,
            0.4,
            1.,
            false,
            false,
            Default::default(),
            false
        ),
        abi::OK
    );
    assert_eq!(host.pixels[0], 0xa33366cc);
    host.gaps_unchanged();
}

#[test]
fn stroke_opacity_caps_paint_influence_over_opaque_and_semitransparent_origins() {
    for (original, expected) in [
        (0xff204060, 0xff284f8b),
        (0x80604020, 0xb3465682),
        (0x00604020, 0x663366cc),
    ] {
        let mut host = Host::new(1, 1, original, 0);
        let mut coverage = Coverage::default();
        coverage.begin(1, 1).unwrap();
        for _ in 0..20 {
            assert_eq!(
                stamp(
                    &mut coverage,
                    &mut host,
                    0.4,
                    1.,
                    false,
                    false,
                    Default::default(),
                    false
                ),
                abi::OK
            );
        }
        assert_eq!(host.pixels[0], expected);
        assert_eq!(host.original, [original]);
    }
}

#[test]
fn stroke_opacity_low_flow_builds_toward_cap_without_reapplying_selection() {
    let selected = [128u8];
    let selection = abi::Coverage {
        data: selected.as_ptr(),
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
    };
    let mut host = Host::new(1, 1, 0, 0);
    let mut coverage = Coverage::default();
    coverage.begin(1, 1).unwrap();
    stamp(
        &mut coverage,
        &mut host,
        0.4,
        0.25,
        false,
        false,
        selection,
        false,
    );
    assert_eq!(host.pixels[0] >> 24, 13);
    let mut previous = 13;
    for _ in 0..64 {
        assert_eq!(
            stamp(
                &mut coverage,
                &mut host,
                0.4,
                0.25,
                false,
                false,
                selection,
                false
            ),
            abi::OK
        );
        let a = host.pixels[0] >> 24;
        assert!(a >= previous && a <= 51);
        previous = a;
    }
    assert_eq!(previous, 51);
    assert_eq!(coverage.allocations, 1);
    assert_eq!(host.original, [0]);
}

#[test]
fn stroke_opacity_pressure_cap_is_monotone_and_eraser_alpha_lock_follow_origin() {
    let mut host = Host::new(1, 1, 0, 0);
    let mut coverage = Coverage::default();
    coverage.begin(1, 1).unwrap();
    stamp(
        &mut coverage,
        &mut host,
        0.8,
        1.,
        false,
        false,
        Default::default(),
        false,
    );
    let strong = host.pixels[0];
    for cap in [0., 0.1, 0.4, 0.7] {
        stamp(
            &mut coverage,
            &mut host,
            cap,
            1.,
            false,
            false,
            Default::default(),
            false,
        );
        assert_eq!(host.pixels[0], strong);
    }
    stamp(
        &mut coverage,
        &mut host,
        1.,
        1.,
        false,
        false,
        Default::default(),
        false,
    );
    assert_eq!(host.pixels[0], 0xff3366cc);
    for (origin, erase, locked, expected) in [
        (0x80604020, true, false, 0x4d604020),
        (0x80604020, false, true, 0x804e4f65),
        (0x80604020, true, true, 0x80604020),
        (0x00604020, false, true, 0x00604020),
    ] {
        let mut host = Host::new(1, 1, origin, 0);
        let mut coverage = Coverage::default();
        coverage.begin(1, 1).unwrap();
        for _ in 0..12 {
            stamp(
                &mut coverage,
                &mut host,
                0.4,
                1.,
                erase,
                false,
                Default::default(),
                locked,
            );
        }
        assert_eq!(host.pixels[0], expected);
    }
}

#[test]
fn stroke_opacity_a8_gray_and_reveal_white_apply_selection_once() {
    for (erase, selection_byte, expected) in [
        (false, 255, 55),
        (true, 255, 157),
        (false, 128, 73),
        (true, 128, 124),
    ] {
        let mut host = Host::new(1, 1, 0, 91);
        let mut coverage = Coverage::default();
        coverage.begin(1, 1).unwrap();
        let selected = [selection_byte];
        let selection = abi::Coverage {
            data: selected.as_ptr(),
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
        };
        for _ in 0..20 {
            assert_eq!(
                stamp(
                    &mut coverage,
                    &mut host,
                    0.4,
                    1.,
                    erase,
                    true,
                    selection,
                    false
                ),
                abi::OK
            );
        }
        assert_eq!(host.mask[0], expected);
        assert_eq!(host.original_a8, [91]);
        host.gaps_unchanged();
    }
}

#[test]
fn stroke_opacity_edge_tiles_use_logical_coordinates_preserve_gaps_and_reuse_allocations() {
    let mut host = Host::new(130, 65, 0xff204060, 91);
    let mut coverage = Coverage::default();
    coverage.begin(130, 65).unwrap();
    let table = host.table(false);
    let region = abi::Region {
        x: 0,
        y: 0,
        width: 130,
        height: 65,
    };
    let mut b = brush(0.4, false);
    b.radius = 200.;
    for _ in 0..3 {
        assert_eq!(
            coverage.dab(
                &table.base.base,
                region,
                b,
                65.,
                32.,
                1.,
                Default::default(),
                false,
                None
            ),
            abi::OK
        );
    }
    assert_eq!(coverage.used, 6);
    assert_eq!(coverage.allocations, 6);
    for y in 0..65 {
        assert!(host.pixels[y * host.stride..y * host.stride + 130]
            .iter()
            .all(|pixel| *pixel == 0xff284f8b));
    }
    host.gaps_unchanged();
    host.lift();
    coverage.release();
    coverage.begin(130, 65).unwrap();
    assert_eq!(
        coverage.dab(
            &table.base.base,
            region,
            b,
            65.,
            32.,
            1.,
            Default::default(),
            false,
            None
        ),
        abi::OK
    );
    assert_eq!(coverage.allocations, 6);
}

#[test]
fn stroke_opacity_zero_flow_and_missing_host_tail_never_loan_or_allocate_pixels() {
    let mut host = Host::new(1, 1, 0, 91);
    let mut coverage = Coverage::default();
    coverage.begin(1, 1).unwrap();
    assert_eq!(
        stamp(
            &mut coverage,
            &mut host,
            0.4,
            0.,
            false,
            false,
            Default::default(),
            false
        ),
        abi::OK
    );
    assert_eq!(host.calls, 0);
    assert_eq!(coverage.allocations, 0);
    for size in [0, 32, 40, 63] {
        let mut table = host.table(false);
        table.base.base.struct_size = size;
        assert!(!supported(&table.base.base, false));
        assert_eq!(
            coverage.dab(
                &table.base.base,
                abi::Region {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1
                },
                brush(0.4, false),
                0.5,
                0.5,
                1.,
                Default::default(),
                false,
                None
            ),
            abi::ERROR
        );
    }
    let mut table = host.table(false);
    table.features = abi::TOOL_HOST_ORIGIN_A8;
    assert!(!supported(&table.base.base, false));
    table.features = abi::TOOL_HOST_ORIGIN_RGBA;
    table.tile_side = 32;
    assert!(!supported(&table.base.base, false));
    assert_eq!(host.calls, 0);
    assert_eq!(coverage.allocations, 0);
    assert_eq!(host.pixels[0], 0);
}

#[test]
fn stroke_opacity_mid_region_host_error_is_reported_and_retained_pool_is_bounded() {
    let mut host = Host::new(65, 1, 0, 91);
    host.fail_at = Some(2);
    let table = host.table(false);
    let mut coverage = Coverage::default();
    coverage.begin(65, 1).unwrap();
    let mut b = brush(0.4, false);
    b.radius = 100.;
    assert_eq!(
        coverage.dab(
            &table.base.base,
            abi::Region {
                x: 0,
                y: 0,
                width: 65,
                height: 1
            },
            b,
            32.,
            0.5,
            1.,
            Default::default(),
            false,
            None
        ),
        abi::ERROR
    );
    assert_eq!(host.calls, 2);
    let mut coverage = Coverage::default();
    coverage.begin(4096, 4096).unwrap();
    assert_eq!(coverage.indices.len(), 4096);
    assert!(coverage.begin(4097, 1).is_err());
    assert!(coverage.begin(64 * 65, 1).is_err());
    coverage.begin(4096, 65).unwrap();
    coverage
        .reserve_region(abi::Region {
            x: 0,
            y: 0,
            width: 4096,
            height: 65,
        })
        .unwrap();
    assert_eq!(coverage.used, 128);
    coverage.release();
    assert_eq!(coverage.tiles.len(), 64);
}

fn options(mode: bool, flow: f32) -> abi::ToolOptionsStroke {
    abi::ToolOptionsStroke {
        base: abi::ToolOptionsSelection {
            base: abi::ToolOptionsExt {
                base: abi::ToolOptions {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::ToolOptionsStroke>() as u32,
                    spacing: 0.11,
                    minimum_opacity: 0.,
                },
                flags: if mode {
                    abi::TOOL_OPTION_STROKE_OPACITY
                } else {
                    0
                },
            },
            selection: Default::default(),
        },
        flow,
        reserved: 0,
    }
}
#[test]
fn stroke_options_require_complete_tail_validate_flow_and_reset_legacy_configuration() {
    let mut state = State::default();
    let state_ptr = (&mut state as *mut State).cast();
    let mut op = options(true, 0.25);
    assert_eq!(unsafe { configure(state_ptr, &op.base.base.base) }, abi::OK);
    assert_eq!(state.flow, 0.25);
    assert!(state.stroke_opacity);
    // A 16-byte legacy prefix cannot advertise the stroke flag; the explicit
    // legacy reset below must remain accepted without reading beyond its size.
    for size in [20, 56, 57, 60, 63] {
        op.base.base.base.struct_size = size;
        assert_eq!(
            unsafe { configure(state_ptr, &op.base.base.base) },
            abi::ERROR
        );
        assert_eq!(state.flow, 0.25);
        assert!(state.stroke_opacity);
    }
    op = options(false, 1.);
    op.reserved = 1;
    assert_eq!(
        unsafe { configure(state_ptr, &op.base.base.base) },
        abi::ERROR
    );
    op.reserved = 0;
    for flow in [f32::NAN, -0.1, 1.1, f32::INFINITY] {
        op.flow = flow;
        assert_eq!(
            unsafe { configure(state_ptr, &op.base.base.base) },
            abi::ERROR
        );
    }
    let legacy = abi::ToolOptions {
        abi_version: 1,
        struct_size: 16,
        spacing: 0.11,
        minimum_opacity: 0.,
    };
    assert_eq!(unsafe { configure(state_ptr, &legacy) }, abi::OK);
    assert_eq!(state.flow, 1.);
    assert!(!state.stroke_opacity);
    let mut raster = Raster::default();
    raster.width = 1;
    raster.height = 1;
    assert!(raster.stroke.is_none());
    assert_eq!(raster.flow, 1.);
    let empty = Coverage::default();
    assert!(empty.indices.is_empty() && empty.tiles.is_empty());
}

#[test]
fn stroke_callback_rejects_alias_coordinates_and_torn_spans_before_writes() {
    let mut coverage = Coverage::default();
    coverage.begin(1, 1).unwrap();
    let region = abi::Region {
        x: 0,
        y: 0,
        width: 1,
        height: 1,
    };
    coverage.reserve_region(region).unwrap();
    let mut dab = Dab {
        coverage: &mut coverage,
        geometry: Default::default(),
        region,
        brush: brush(0.4, false),
        x: 0.5,
        y: 0.5,
        flow: 1.,
        selection: Default::default(),
        alpha_locked: false,
        a8_value: 0,
    };
    let mut pixel = 0x80604020u32;
    let original = 0x80604020u32;
    let mut origin = abi::OriginPixels {
        data: &original,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    let mut target = abi::Pixels {
        data: &mut pixel,
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    let user = (&mut dab as *mut Dab).cast();
    for case in 0..8 {
        target.data = &mut pixel;
        origin.data = &original;
        origin.len = 1;
        origin.origin_x = 0;
        origin.stride = 1;
        match case {
            0 => origin.data = target.data,
            1 => origin.len = 0,
            2 => origin.origin_x = 1,
            3 => origin.stride = 0,
            4 => target.data = (&mut coverage as *mut Coverage).cast(),
            5 => target.data = coverage.indices.as_mut_ptr().cast(),
            6 => target.data = coverage.tiles.as_mut_ptr().cast(),
            _ => target.data = coverage.tiles[0].as_mut_ptr().cast(),
        }
        assert_eq!(
            unsafe { render_rgba(user, &origin, &mut target) },
            abi::ERROR
        );
        assert_eq!(pixel, original);
        assert_eq!(coverage.tiles[0][0], 0);
        assert_eq!(coverage.indices, [0]);
    }
    target.data = &mut pixel;
    origin.data = &original;
    assert_eq!(
        unsafe { render_rgba(user.cast::<u8>().add(1).cast(), &origin, &mut target) },
        abi::ERROR
    );
}

#[test]
#[ignore = "Bounded Release-only native contact timing; selected explicitly by CI"]
fn bounded_stroke_cost() {
    use std::time::Instant;
    let parameters = abi::ToolParameters {
        radius: 6.,
        opacity: 0.4,
        hardness: 0.85,
        color: 0x3366cc,
        stabilizer: 0.35,
        pressure_gamma: 1.,
        minimum_radius: 0.08,
        flags: abi::PRESSURE_SIZE | abi::PRESSURE_OPACITY,
    };
    for a8 in [false, true] {
        for capped in [false, true] {
            let mut host = Host::new(64, 64, 0x80604020, 91);
            let mut state = State::default();
            let mut op = options(capped, if capped { 0.25 } else { 1. });
            if a8 {
                op.base.base.flags |= abi::TOOL_OPTION_A8_TARGET;
            }
            assert_eq!(
                unsafe { configure((&mut state as *mut State).cast(), &op.base.base.base) },
                abi::OK
            );
            let mut elapsed = Vec::new();
            for run in 0..22 {
                // Frame initialization and immutable fixture snapshots stay outside
                // timing. A <=64 frame lends them directly, without test-only copies.
                host.lift();
                let mut table = host.table(a8);
                table.base.base.pixels = legacy_rgba;
                if a8 {
                    table.base.a8 = Some(legacy_a8);
                }
                if !capped {
                    table.base.base.struct_size = if a8 { 40 } else { 32 };
                }
                let start = Instant::now();
                for i in 0..41 {
                    let sample = abi::ToolSample {
                        phase: if i == 0 {
                            abi::DOWN
                        } else if i == 40 {
                            abi::UP
                        } else {
                            abi::MOVE
                        },
                        flags: abi::PEN,
                        x: 4. + i as f32 * 1.3,
                        y: 32. + ((i % 9) as f32 - 4.) * 2.,
                        pressure: 0.2 + (i % 11) as f32 * 0.08,
                        time: 100 + i * 8,
                        view_x: 0.,
                        view_y: 0.,
                        width: 64,
                        height: 64,
                        zoom: 1.,
                    };
                    assert_eq!(
                        state.input(
                            1,
                            sample,
                            (i == 0).then_some(parameters),
                            &table.base.base,
                            &mut Default::default()
                        ),
                        abi::OK
                    );
                }
                let ns = start.elapsed().as_nanos();
                if run != 0 {
                    elapsed.push(ns);
                }
                host.gaps_unchanged();
                assert!(state.contact.is_none());
            }
            elapsed.sort_unstable();
            assert_eq!(elapsed.len(), 21);
            assert!(state.stroke_cache.tiles.len() <= RETAINED_TILES);
            if capped {
                assert_eq!(state.stroke_cache.allocations, 1);
                assert_eq!(state.stroke_cache.tiles.len(), 1);
            }
            if !capped {
                assert!(
                    state.stroke_cache.tiles.is_empty() && state.stroke_cache.indices.is_empty()
                );
            }
            println!("stroke_cost domain={} mode={} flow={} opacity={} radius={} stabilizer={} frame=64x64 samples=41 contacts=21 p50_ns={} p95_ns={} retained_coverage_bytes={} coverage_tile_allocations={} scope=native_contact_synthetic excludes=frame_setup,undo_capture,Session,journal,GUI,tablet",
            if a8{"a8"}else{"rgba"},if capped{"stroke"}else{"dab"},op.flow,parameters.opacity,parameters.radius,parameters.stabilizer,elapsed[10],elapsed[19],
            state.stroke_cache.tiles.len()*AREA*2,state.stroke_cache.allocations);
        }
    }
}
