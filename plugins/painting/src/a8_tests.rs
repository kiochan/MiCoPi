//! Native-only A8 conformance; this does not imply application mask support.
use super::*;

struct Canvas {
    width: usize,
    height: usize,
    rgba: Vec<u32>,
    a8: Vec<u8>,
    rgba_calls: usize,
    a8_calls: usize,
}
impl Canvas {
    fn new(width: usize, height: usize, value: u8) -> Self {
        let pixel = 0xff000000 | value as u32 * 0x010101;
        Self {
            width,
            height,
            rgba: vec![pixel; width * height],
            a8: vec![value; width * height],
            rgba_calls: 0,
            a8_calls: 0,
        }
    }
    fn host(&mut self) -> abi::ToolHostA8 {
        abi::ToolHostA8 {
            base: abi::ToolHost {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::ToolHostA8>() as u32,
                context: (self as *mut Self).cast(),
                pixels: rgba_loan,
                pick,
            },
            a8: Some(byte_loan),
        }
    }
    fn valid(&self, r: abi::Region) -> bool {
        r.width > 0
            && r.height > 0
            && r.x
                .checked_add(r.width)
                .is_some_and(|x| x <= self.width as u32)
            && r.y
                .checked_add(r.height)
                .is_some_and(|y| y <= self.height as u32)
    }
}
unsafe extern "C" fn rgba_loan(
    context: *mut c_void,
    r: abi::Region,
    transform: Option<abi::Transform>,
    user: *mut c_void,
) -> i32 {
    let canvas = &mut *context.cast::<Canvas>();
    if !canvas.valid(r) {
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        return abi::ERROR;
    };
    canvas.rgba_calls += 1;
    let mut pixels = abi::Pixels {
        data: canvas
            .rgba
            .as_mut_ptr()
            .add(r.y as usize * canvas.width + r.x as usize),
        len: (r.height as usize - 1) * canvas.width + r.width as usize,
        width: r.width,
        height: r.height,
        stride: canvas.width as u32,
        origin_x: r.x,
        origin_y: r.y,
    };
    transform(user, &mut pixels)
}
unsafe extern "C" fn byte_loan(
    context: *mut c_void,
    r: abi::Region,
    transform: Option<abi::A8Transform>,
    user: *mut c_void,
) -> i32 {
    let canvas = &mut *context.cast::<Canvas>();
    if !canvas.valid(r) {
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        return abi::ERROR;
    };
    canvas.a8_calls += 1;
    let mut pixels = abi::A8Pixels {
        data: canvas
            .a8
            .as_mut_ptr()
            .add(r.y as usize * canvas.width + r.x as usize),
        len: (r.height as usize - 1) * canvas.width + r.width as usize,
        width: r.width,
        height: r.height,
        stride: canvas.width as u32,
        origin_x: r.x,
        origin_y: r.y,
    };
    transform(user, &mut pixels)
}
unsafe extern "C" fn pick(_: *mut c_void, _: f32, _: f32, _: *mut u32) -> i32 {
    abi::ERROR
}
fn options(selection: &abi::Coverage, a8: bool) -> abi::ToolOptionsSelection {
    abi::ToolOptionsSelection {
        base: abi::ToolOptionsExt {
            base: abi::ToolOptions {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::ToolOptionsSelection>() as u32,
                spacing: 0.17,
                minimum_opacity: 0.13,
            },
            flags: if a8 { abi::TOOL_OPTION_A8_TARGET } else { 0 }
                | if selection.data.is_null() {
                    0
                } else {
                    abi::TOOL_OPTION_SELECTION
                },
        },
        selection: *selection,
    }
}
fn parameters(color: u32, stabilizer: f32) -> abi::ToolParameters {
    abi::ToolParameters {
        radius: 6.5,
        opacity: 0.72,
        hardness: 0.45,
        color,
        stabilizer,
        pressure_gamma: 1.5,
        minimum_radius: 0.08,
        flags: abi::PRESSURE_SIZE | abi::PRESSURE_OPACITY,
    }
}
fn point(i: u32, phase: u32, pen: bool, eraser: bool, width: u32, height: u32) -> abi::ToolSample {
    abi::ToolSample {
        phase,
        flags: if pen { abi::PEN } else { 0 } | if eraser { abi::ERASER } else { 0 },
        x: 12. + i as f32 * 3.5,
        y: 25. + (i as f32 * 0.43).sin() * 11.,
        pressure: if i == 0 || phase == abi::UP {
            0.
        } else {
            0.05 + ((i * 7) % 13) as f32 / 14.
        },
        time: (u32::MAX - 15).wrapping_add(i * 4),
        view_x: 0.,
        view_y: 0.,
        width,
        height,
        zoom: 1.,
    }
}

#[test]
fn a8_options_are_additive_resettable_and_conflict_with_alpha_lock() {
    assert_ne!(OPTIONS.features & abi::TOOL_OPTION_A8_TARGET, 0);
    let mut state = State::default();
    let context = (&mut state as *mut State).cast();
    let mut op = options(&abi::Coverage::default(), true);
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::OK);
    assert!(state.a8_target);
    op.base.flags |= abi::TOOL_OPTION_ALPHA_LOCK;
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::ERROR);
    assert!(state.a8_target && !state.alpha_locked);
    op.base.flags = abi::TOOL_OPTION_A8_TARGET | 8;
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::ERROR);
    op.base.flags = abi::TOOL_OPTION_A8_TARGET;
    op.base.base.struct_size = std::mem::size_of::<abi::ToolOptionsExt>() as u32;
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::OK);
    op.base.base.struct_size = 17;
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::ERROR);
    let legacy = abi::ToolOptions {
        abi_version: 1,
        struct_size: std::mem::size_of::<abi::ToolOptions>() as u32,
        spacing: 0.11,
        minimum_opacity: 0.,
    };
    assert_eq!(unsafe { configure(context, &legacy) }, abi::OK);
    assert!(!state.a8_target);
}

#[test]
fn a8_contacts_require_complete_nonnull_host_tail_even_at_zero_pressure() {
    for size in 32..40 {
        let mut canvas = Canvas::new(96, 64, 255);
        let mut host = canvas.host();
        host.base.struct_size = size;
        let mut state = State::default();
        let context = (&mut state as *mut State).cast();
        let op = options(&abi::Coverage::default(), true);
        assert_eq!(unsafe { configure(context, &op.base.base) }, abi::OK);
        let mut out = abi::ToolOutput::default();
        assert_eq!(
            unsafe {
                pointer(
                    context,
                    1,
                    &point(0, abi::DOWN, true, false, 96, 64),
                    &parameters(0, 0.35),
                    &host.base,
                    &mut out,
                )
            },
            abi::ERROR
        );
        assert!(state.contact.is_none());
        assert_eq!((canvas.a8_calls, canvas.rgba_calls), (0, 0));
        assert!(canvas.a8.iter().all(|v| *v == 255));
    }
    let mut canvas = Canvas::new(96, 64, 255);
    let mut host = canvas.host();
    host.a8 = None;
    let mut state = State::default();
    let context = (&mut state as *mut State).cast();
    let op = options(&abi::Coverage::default(), true);
    assert_eq!(unsafe { configure(context, &op.base.base) }, abi::OK);
    assert_eq!(
        unsafe {
            pointer(
                context,
                1,
                &point(1, abi::DOWN, false, false, 96, 64),
                &parameters(0, 0.),
                &host.base,
                &mut abi::ToolOutput::default(),
            )
        },
        abi::ERROR
    );
    assert_eq!((canvas.a8_calls, canvas.rgba_calls), (0, 0));
}

#[test]
fn a8_mid_contact_tail_loss_and_callback_failure_abort_without_rgba_fallback() {
    for fail_callback in [false, true] {
        let mut canvas = Canvas::new(96, 64, 255);
        let mut host = canvas.host();
        let mut state = State::default();
        let context = (&mut state as *mut State).cast();
        let op = options(&abi::Coverage::default(), true);
        assert_eq!(unsafe { configure(context, &op.base.base) }, abi::OK);
        assert_eq!(
            unsafe {
                pointer(
                    context,
                    1,
                    &point(1, abi::DOWN, false, false, 96, 64),
                    &parameters(0, 0.),
                    &host.base,
                    &mut abi::ToolOutput::default(),
                )
            },
            abi::OK
        );
        let before = canvas.a8.clone();
        let calls = canvas.a8_calls;
        unsafe extern "C" fn fail(
            _: *mut c_void,
            _: abi::Region,
            _: Option<abi::A8Transform>,
            _: *mut c_void,
        ) -> i32 {
            abi::ERROR
        }
        if fail_callback {
            host.a8 = Some(fail);
        } else {
            host.base.struct_size = 39;
        }
        assert_eq!(
            unsafe {
                pointer(
                    context,
                    1,
                    &point(10, abi::MOVE, false, false, 96, 64),
                    std::ptr::null(),
                    &host.base,
                    &mut abi::ToolOutput::default(),
                )
            },
            abi::ERROR
        );
        assert!(state.contact.is_none());
        assert_eq!(canvas.rgba_calls, 0);
        assert_eq!(canvas.a8_calls, calls);
        assert_eq!(canvas.a8, before);
    }
}

#[test]
fn up_and_final_stabilizer_tail_failures_propagate_for_a8_and_prefix_rgba() {
    struct FailingCanvas {
        canvas: Canvas,
        calls: usize,
        fail_at: usize,
        failed: bool,
    }
    impl FailingCanvas {
        fn new() -> Self {
            Self {
                canvas: Canvas::new(256, 128, 255),
                calls: 0,
                fail_at: usize::MAX,
                failed: false,
            }
        }
        fn host(&mut self, a8: bool) -> abi::ToolHostA8 {
            abi::ToolHostA8 {
                base: abi::ToolHost {
                    abi_version: 1,
                    struct_size: if a8 {
                        std::mem::size_of::<abi::ToolHostA8>()
                    } else {
                        std::mem::size_of::<abi::ToolHost>()
                    } as u32,
                    context: (self as *mut Self).cast(),
                    pixels: rgba,
                    pick,
                },
                a8: Some(bytes),
            }
        }
        fn reject(&mut self) -> bool {
            self.calls += 1;
            if self.calls == self.fail_at {
                self.failed = true;
                true
            } else {
                false
            }
        }
    }
    unsafe extern "C" fn rgba(
        context: *mut c_void,
        r: abi::Region,
        transform: Option<abi::Transform>,
        user: *mut c_void,
    ) -> i32 {
        let context = &mut *context.cast::<FailingCanvas>();
        if context.reject() {
            return abi::ERROR;
        }
        rgba_loan(
            (&mut context.canvas as *mut Canvas).cast(),
            r,
            transform,
            user,
        )
    }
    unsafe extern "C" fn bytes(
        context: *mut c_void,
        r: abi::Region,
        transform: Option<abi::A8Transform>,
        user: *mut c_void,
    ) -> i32 {
        let context = &mut *context.cast::<FailingCanvas>();
        if context.reject() {
            return abi::ERROR;
        }
        byte_loan(
            (&mut context.canvas as *mut Canvas).cast(),
            r,
            transform,
            user,
        )
    }
    fn sample(i: u32, phase: u32) -> abi::ToolSample {
        abi::ToolSample {
            phase,
            flags: 0,
            x: 20. + i as f32 * 70.,
            y: 40. + i as f32 * 20.,
            pressure: 0.7,
            time: i * 4,
            view_x: 0.,
            view_y: 0.,
            width: 256,
            height: 128,
            zoom: 1.,
        }
    }
    fn deliver(
        state: &mut State,
        host: &abi::ToolHost,
        p: abi::ToolSample,
        parameters: Option<&abi::ToolParameters>,
    ) -> i32 {
        unsafe {
            pointer(
                (state as *mut State).cast(),
                1,
                &p,
                parameters.map_or(std::ptr::null(), |p| p as *const _),
                host,
                &mut abi::ToolOutput::default(),
            )
        }
    }
    fn prepare(canvas: &mut FailingCanvas, state: &mut State, a8: bool) -> abi::ToolHostA8 {
        let host = canvas.host(a8);
        let options = options(&abi::Coverage::default(), a8);
        assert_eq!(
            unsafe { configure((state as *mut State).cast(), &options.base.base) },
            abi::OK
        );
        let p = parameters(0, 1.);
        assert_eq!(
            deliver(state, &host.base, sample(0, abi::DOWN), Some(&p)),
            abi::OK
        );
        assert_eq!(
            deliver(state, &host.base, sample(1, abi::MOVE), None),
            abi::OK
        );
        host
    }
    for a8 in [false, true] {
        // A final MOVE follows the same filtering path as UP with this nonzero
        // pressure, but leaves the final raw-position tail for a separate call.
        let mut baseline = FailingCanvas::new();
        let mut baseline_state = State::default();
        let baseline_host = prepare(&mut baseline, &mut baseline_state, a8);
        let before_up = baseline.calls;
        assert_eq!(
            deliver(
                &mut baseline_state,
                &baseline_host.base,
                sample(2, abi::MOVE),
                None
            ),
            abi::OK
        );
        let before_tail = baseline.calls;
        assert!(before_tail > before_up);
        assert_eq!(
            deliver(
                &mut baseline_state,
                &baseline_host.base,
                sample(2, abi::CANCEL),
                None
            ),
            abi::OK
        );
        assert!(
            baseline.calls > before_tail,
            "fixture must contain a nonempty final tail"
        );
        for (phase, fail_at) in [
            (abi::UP, before_up + 1),
            (abi::UP, before_tail + 1),
            (abi::CANCEL, before_up + 1),
        ] {
            let mut canvas = FailingCanvas::new();
            let mut state = State::default();
            let host = prepare(&mut canvas, &mut state, a8);
            assert_eq!(canvas.calls, before_up);
            canvas.fail_at = fail_at;
            assert_eq!(
                deliver(&mut state, &host.base, sample(2, phase), None),
                abi::ERROR,
                "a8{a8} phase{phase} fail_at{fail_at}"
            );
            assert!(canvas.failed);
            assert_eq!(canvas.calls, fail_at);
            assert!(state.contact.is_none());
            assert!(state.filter.finish().is_none());
            if a8 {
                assert_eq!(canvas.canvas.rgba_calls, 0);
            } else {
                assert_eq!(canvas.canvas.a8_calls, 0);
            }
        }
    }
}

#[test]
fn a8_pressure_stabilizer_spacing_selection_and_erase_match_independent_opaque_reference() {
    for (handler, eraser, color, initial) in [
        (1, false, 0x2468da, 255u8),
        (2, false, 0x2468da, 0),
        (1, true, 0x2468da, 103),
        (1, false, 0, 255),
        (1, false, 0xffffff, 0),
    ] {
        // 0x2468da independently resolves to98 under the published integer weights.
        let value = if handler == 2 || eraser {
            255
        } else if color == 0x2468da {
            98
        } else if color == 0 {
            0
        } else {
            255
        };
        for (pen, strength, selected, cancel) in [
            (true, 0., false, false),
            (true, 0.75, true, false),
            (true, 1., true, true),
            (false, 0.35, true, false),
        ] {
            let mut mask_canvas = Canvas::new(96, 64, initial);
            let mut rgba_canvas = Canvas::new(96, 64, initial);
            let mut mask_host = mask_canvas.host();
            let mut rgba_host = rgba_canvas.host();
            rgba_host.base.struct_size = std::mem::size_of::<abi::ToolHost>() as u32;
            let coverage: Vec<u8> = (0..96 * 64)
                .map(|i| [0, 64, 128, 255][(i / 7) % 4])
                .collect();
            let selection = if selected {
                abi::Coverage {
                    data: coverage.as_ptr(),
                    len: coverage.len(),
                    width: 96,
                    height: 64,
                    stride: 96,
                }
            } else {
                abi::Coverage::default()
            };
            let mut mask_state = State::default();
            let mut rgba_state = State::default();
            let mask_context = (&mut mask_state as *mut State).cast();
            let rgba_context = (&mut rgba_state as *mut State).cast();
            let mask_options = options(&selection, true);
            let rgba_options = options(&selection, false);
            assert_eq!(
                unsafe { configure(mask_context, &mask_options.base.base) },
                abi::OK
            );
            assert_eq!(
                unsafe { configure(rgba_context, &rgba_options.base.base) },
                abi::OK
            );
            let mask_parameters = parameters(color, strength);
            let rgba_parameters = parameters(value as u32 * 0x010101, strength);
            for i in 0..20 {
                let phase = if i == 0 {
                    abi::DOWN
                } else if i == 19 {
                    if cancel {
                        abi::CANCEL
                    } else {
                        abi::UP
                    }
                } else {
                    abi::MOVE
                };
                let sample = point(i, phase, pen, eraser, 96, 64);
                let mut reference = sample;
                reference.flags &= !abi::ERASER;
                let mut out = abi::ToolOutput::default();
                let mut reference_out = abi::ToolOutput::default();
                assert_eq!(
                    unsafe {
                        pointer(
                            mask_context,
                            handler,
                            &sample,
                            if i == 0 {
                                &mask_parameters
                            } else {
                                std::ptr::null()
                            },
                            &mask_host.base,
                            &mut out,
                        )
                    },
                    abi::OK
                );
                assert_eq!(
                    unsafe {
                        pointer(
                            rgba_context,
                            1,
                            &reference,
                            if i == 0 {
                                &rgba_parameters
                            } else {
                                std::ptr::null()
                            },
                            &rgba_host.base,
                            &mut reference_out,
                        )
                    },
                    abi::OK
                );
                assert_eq!(out.flags, reference_out.flags);
                if i == 8 {
                    assert_eq!(
                        unsafe { configure(mask_context, &rgba_options.base.base) },
                        abi::ERROR
                    );
                }
            }
            assert_eq!(mask_canvas.rgba_calls, 0);
            assert_eq!(rgba_canvas.a8_calls, 0);
            assert!(mask_canvas.a8_calls > 0 && rgba_canvas.rgba_calls > 0);
            assert_eq!(mask_canvas.a8_calls, rgba_canvas.rgba_calls);
            assert!(
                mask_canvas
                    .a8
                    .iter()
                    .zip(&rgba_canvas.rgba)
                    .all(|(m, p)| *m == (*p & 255) as u8),
                "handler{handler} pen{pen} strength{strength} selected{selected}"
            );
            assert!(mask_canvas
                .rgba
                .iter()
                .all(|p| *p == 0xff000000 | initial as u32 * 0x010101));
            assert!(mask_state.contact.is_none() && rgba_state.contact.is_none());
            // A later prefix-only configuration returns the cached instance to RGBA.
            assert_eq!(
                unsafe { configure(mask_context, &rgba_options.base.base) },
                abi::OK
            );
            assert!(!mask_state.a8_target);
            mask_host.base.struct_size = 32;
            assert_eq!(
                unsafe {
                    pointer(
                        mask_context,
                        1,
                        &point(1, abi::DOWN, false, false, 96, 64),
                        &rgba_parameters,
                        &mask_host.base,
                        &mut abi::ToolOutput::default(),
                    )
                },
                abi::OK
            );
            assert!(mask_canvas.rgba_calls > 0);
        }
    }
}

#[test]
#[ignore = "Release typed A8/raster diagnostic; no CI timing threshold"]
fn a8_raster_cost() {
    assert!(
        !cfg!(debug_assertions),
        "Run this diagnostic with --release"
    );
    use std::time::Instant;
    const WIDTH: usize = 512;
    const HEIGHT: usize = 384;
    const INITIAL: u8 = 211;

    // Full-frame source, destinations and every input sample are prepared before
    // timing. Alternating stroke kinds compare hide-gray and reveal-white dabs.
    let samples: Vec<(u32, abi::ToolSample)> = (0..80u32)
        .flat_map(|stroke| {
            (0..64u32).map(move |i| {
                (
                    if stroke % 2 == 0 { 1 } else { 2 },
                    abi::ToolSample {
                        phase: if i == 0 {
                            abi::DOWN
                        } else if i == 63 {
                            abi::UP
                        } else {
                            abi::MOVE
                        },
                        flags: abi::PEN,
                        x: 40. + i as f32 * 6.,
                        y: 60. + ((stroke * 17) % 250) as f32 + (i as f32 * 0.2).sin() * 15.,
                        pressure: if i == 0 || i == 63 {
                            0.
                        } else {
                            0.1 + ((i * 7) % 29) as f32 / 32.
                        },
                        time: (u32::MAX - 10).wrapping_add((stroke * 64 + i) * 4),
                        view_x: 0.,
                        view_y: 0.,
                        width: WIDTH as u32,
                        height: HEIGHT as u32,
                        zoom: 1.,
                    },
                )
            })
        })
        .collect();
    let coverage: Vec<u8> = (0..WIDTH * HEIGHT)
        .map(|i| [0, 64, 128, 255][(i % WIDTH / 32 + i / WIDTH / 16) % 4])
        .collect();
    let selection = abi::Coverage {
        data: coverage.as_ptr(),
        len: coverage.len(),
        width: WIDTH as u32,
        height: HEIGHT as u32,
        stride: WIDTH as u32,
    };
    let mask_options = options(&selection, true);
    let rgba_options = options(&selection, false);
    // Red resolves to exactly54 under the published encoded-RGB weights.
    let mask_parameters = [parameters(0xff0000, 0.35), parameters(0xff0000, 0.35)];
    let rgba_parameters = [parameters(0x363636, 0.35), parameters(0xffffff, 0.35)];

    fn execute(
        state: &mut State,
        host: &abi::ToolHost,
        samples: &[(u32, abi::ToolSample)],
        options: &abi::ToolOptionsSelection,
        parameters: &[abi::ToolParameters; 2],
        a8: bool,
    ) -> i32 {
        let context = (state as *mut State).cast();
        let mut output = abi::ToolOutput::default();
        for (kind, sample) in samples {
            if sample.phase == abi::DOWN {
                let status = unsafe { configure(context, &options.base.base) };
                if status != abi::OK {
                    return status;
                }
            }
            let status = unsafe {
                pointer(
                    context,
                    if a8 { *kind } else { 1 },
                    sample,
                    if sample.phase == abi::DOWN {
                        &parameters[*kind as usize - 1]
                    } else {
                        std::ptr::null()
                    },
                    host,
                    &mut output,
                )
            };
            if status != abi::OK {
                return status;
            }
        }
        abi::OK
    }

    // Generate the opaque-grayscale expected outcome outside every timer. The
    // existing unit matrix separately covers its pressure/filtering invariants.
    let mut expected_canvas = Canvas::new(WIDTH, HEIGHT, INITIAL);
    let mut expected_host = expected_canvas.host();
    expected_host.base.struct_size = std::mem::size_of::<abi::ToolHost>() as u32;
    assert_eq!(
        execute(
            &mut State::default(),
            &expected_host.base,
            &samples,
            &rgba_options,
            &rgba_parameters,
            false,
        ),
        abi::OK
    );
    let expected_calls = expected_canvas.rgba_calls;
    let expected: Vec<u8> = expected_canvas
        .rgba
        .iter()
        .map(|p| (*p & 255) as u8)
        .collect();
    assert!(expected.iter().any(|value| *value != INITIAL));
    assert!(expected.iter().any(|value| *value == INITIAL));

    let mut mask_canvas = Canvas::new(WIDTH, HEIGHT, INITIAL);
    let mut rgba_canvas = Canvas::new(WIDTH, HEIGHT, INITIAL);
    let mut mask_times = Vec::with_capacity(5);
    let mut rgba_times = Vec::with_capacity(5);
    for repeat in 0..5 {
        mask_canvas.a8.fill(INITIAL);
        rgba_canvas
            .rgba
            .fill(0xff000000 | INITIAL as u32 * 0x010101);
        mask_canvas.a8_calls = 0;
        rgba_canvas.rgba_calls = 0;
        let mut mask_state = State::default();
        let mut rgba_state = State::default();
        let mask_host = mask_canvas.host();
        let mut rgba_host = rgba_canvas.host();
        rgba_host.base.struct_size = std::mem::size_of::<abi::ToolHost>() as u32;
        let mut mask_status = abi::ERROR;
        let mut rgba_status = abi::ERROR;
        // Alternate the order of each pair rather than measuring two blocks.
        for a8 in [repeat % 2 == 0, repeat % 2 != 0] {
            let start = Instant::now();
            if a8 {
                mask_status = execute(
                    &mut mask_state,
                    &mask_host.base,
                    &samples,
                    &mask_options,
                    &mask_parameters,
                    true,
                );
                mask_times.push(start.elapsed().as_secs_f64() * 1000.);
            } else {
                rgba_status = execute(
                    &mut rgba_state,
                    &rgba_host.base,
                    &samples,
                    &rgba_options,
                    &rgba_parameters,
                    false,
                );
                rgba_times.push(start.elapsed().as_secs_f64() * 1000.);
            }
        }
        assert_eq!((mask_status, rgba_status), (abi::OK, abi::OK));
        assert!(mask_state.contact.is_none() && rgba_state.contact.is_none());
        assert_eq!(mask_canvas.a8, expected);
        assert_eq!(rgba_canvas.rgba, expected_canvas.rgba);
        assert_eq!(
            (mask_canvas.a8_calls, rgba_canvas.rgba_calls),
            (expected_calls, expected_calls)
        );
        assert_eq!((mask_canvas.rgba_calls, rgba_canvas.a8_calls), (0, 0));
    }
    let mask_samples = mask_times.clone();
    let rgba_samples = rgba_times.clone();
    mask_times.sort_by(f64::total_cmp);
    rgba_times.sort_by(f64::total_cmp);
    println!(
        "a8_raster_cost: 512x384, 5120 prepared pen samples/80 alternating gray-brush and reveal-white eraser strokes, pressure size/opacity, stabilizer0.35 and fractional selection; five interleaved medians A8={:.3}ms, opaque grayscale RGBA={:.3}ms, callbacks/run={expected_calls}; A8_samples_ms={mask_samples:?}, RGBA_samples_ms={rgba_samples:?}. Timed native configure/DOWN/MOVE/UP, typed scoped loans and raster kernels, with preallocated reused destinations. Excludes fixture/sample allocation, source/reference preparation, buffer resets, verification, Session/history/journal, DLL loading, encoding, I/O, GUI and physical pen input. No CI threshold or latency/speedup guarantee.",
        mask_times[2], rgba_times[2]
    );
}
