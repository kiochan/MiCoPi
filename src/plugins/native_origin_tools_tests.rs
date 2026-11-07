//! Scoped original loans, negotiated prefixes and whole-contact failure handling.
use super::*;
use crate::{
    application::{OpacityMode, Pointer, PointerPhase},
    engine::{
        raster_masks::{MaskOwner, PaintTarget, PlaneView},
        Document,
    },
};
use std::cell::RefCell;

thread_local! {
    static OPTIONS: RefCell<Option<(u32,u32,f32,u32)>> = const { RefCell::new(None) };
    static HOST: RefCell<Option<(u32,u32,u32,bool,bool)>> = const { RefCell::new(None) };
    static SELECTION: RefCell<abi::Coverage> = RefCell::new(abi::Coverage::default());
}

fn session() -> Session {
    let mut s = Session::new(super::super::Registry::bundled()).unwrap();
    s.doc = Document::new(70, 67);
    s.doc.renderer = s.registry.renderer();
    s.doc.layers[0].pixels = (0..70 * 67).map(|i| 0xff000000 | i as u32).collect();
    s.parameters.flow = 1.;
    s.parameters.opacity_mode = OpacityMode::Dab;
    s.parameters.spacing = 0.11;
    s.parameters.minimum_opacity = 0.;
    s
}

unsafe extern "C" fn record_options(_: *mut c_void, options: *const abi::ToolOptions) -> i32 {
    if options.is_null() {
        return abi::ERROR;
    }
    let size = (*options).struct_size;
    let flags = if size >= std::mem::size_of::<abi::ToolOptionsExt>() as u32 {
        std::ptr::read_unaligned(options.cast::<abi::ToolOptionsExt>()).flags
    } else {
        0
    };
    let (flow, reserved) = if size >= std::mem::size_of::<abi::ToolOptionsStroke>() as u32 {
        let tail = std::ptr::read_unaligned(options.cast::<abi::ToolOptionsStroke>());
        (tail.flow, tail.reserved)
    } else {
        (1., 0)
    };
    OPTIONS.with(|record| *record.borrow_mut() = Some((size, flags, flow, reserved)));
    if size >= std::mem::size_of::<abi::ToolOptionsSelection>() as u32 {
        let selected =
            std::ptr::read_unaligned(options.cast::<abi::ToolOptionsSelection>()).selection;
        SELECTION.with(|record| *record.borrow_mut() = selected);
    }
    abi::OK
}

unsafe extern "C" fn record_host(
    _: *mut c_void,
    _: u32,
    _: *const abi::ToolSample,
    _: *const abi::ToolParameters,
    host: *const abi::ToolHost,
    out: *mut abi::ToolOutput,
) -> i32 {
    if host.is_null() || out.is_null() {
        return abi::ERROR;
    }
    let (features, tile, rgba, a8) =
        if (*host).struct_size >= std::mem::size_of::<abi::ToolHostStroke>() as u32 {
            let tail = std::ptr::read_unaligned(host.cast::<abi::ToolHostStroke>());
            (
                tail.features,
                tail.tile_side,
                tail.pixels_origin.is_some(),
                tail.a8_origin.is_some(),
            )
        } else {
            (0, 0, false, false)
        };
    HOST.with(|record| {
        *record.borrow_mut() = Some(((*host).struct_size, features, tile, rgba, a8))
    });
    *out = abi::ToolOutput::default();
    abi::OK
}

fn inspected_instance(s: &Session, features: u32) -> ToolInstance {
    let path = &s.registry.native_plugins["painting"].path;
    let (mut plugin, _) = NativePlugin::load(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        "painting",
    )
    .unwrap();
    let table = Arc::get_mut(&mut plugin).unwrap();
    let mut options = table.tool_options.unwrap().0;
    options.configure = Some(record_options);
    table.tool_options = Some((options, features));
    let mut instance = ToolInstance::new(plugin).unwrap();
    instance.api.pointer = Some(record_host);
    instance
}

fn sample() -> abi::ToolSample {
    abi::ToolSample {
        phase: abi::DOWN,
        flags: 0,
        x: 1.,
        y: 1.,
        pressure: 1.,
        time: 0,
        view_x: 1.,
        view_y: 1.,
        width: 70,
        height: 67,
        zoom: 1.,
    }
}

#[test]
fn stroke_options_and_host_tail_are_frozen_and_only_advertise_the_admitted_domain() {
    let mut s = session();
    let mut instance = inspected_instance(&s, 15);
    assert!(instance.plugin.supports_stroke_opacity());
    s.parameters.flow = 0.3;
    s.parameters.opacity_mode = OpacityMode::Stroke;
    instance.prepare_contact(&s, true, false).unwrap();
    assert_eq!(
        OPTIONS.with(|record| *record.borrow()),
        Some((64, abi::TOOL_OPTION_STROKE_OPACITY, 0.3, 0))
    );
    s.parameters.flow = 1.;
    s.parameters.opacity_mode = OpacityMode::Dab;
    instance
        .pointer(&mut s, 1, sample(), None, true, None)
        .unwrap();
    assert_eq!(
        HOST.with(|record| *record.borrow()),
        Some((64, abi::TOOL_HOST_ORIGIN_RGBA, 64, true, false))
    );
    instance.prepare_contact(&s, true, false).unwrap();
    instance
        .pointer(&mut s, 1, sample(), None, true, None)
        .unwrap();
    assert_eq!(
        HOST.with(|record| *record.borrow()),
        Some((32, 0, 0, false, false))
    );
    s.parameters.flow = 0.25;
    instance.prepare_contact(&s, true, false).unwrap();
    assert_eq!(
        OPTIONS.with(|record| *record.borrow()),
        Some((64, 0, 0.25, 0))
    );
    instance
        .pointer(&mut s, 1, sample(), None, true, None)
        .unwrap();
    assert_eq!(HOST.with(|record| *record.borrow()).unwrap().0, 32);
    let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 177).unwrap();
    s.parameters.opacity_mode = OpacityMode::Stroke;
    instance.prepare_contact(&s, true, true).unwrap();
    let lease = s.doc.begin_mask_stroke(id).unwrap();
    instance
        .pointer(&mut s, 1, sample(), None, true, Some(lease))
        .unwrap();
    assert_eq!(
        HOST.with(|record| *record.borrow()),
        Some((64, abi::TOOL_HOST_ORIGIN_A8, 64, false, true))
    );
    s.doc.finish();
    instance.prepare_contact(&s, false, false).unwrap();
    instance
        .pointer(&mut s, 4, sample(), None, false, None)
        .unwrap();
    assert_eq!(HOST.with(|record| *record.borrow()).unwrap().0, 32);
}

#[test]
fn older_tool_options_keep_old_prefixes_and_reject_nondefaults_before_materialization() {
    let mut s = session();
    let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 177).unwrap();
    let before = s.doc.composition();
    let mut instance = inspected_instance(&s, 7);
    assert!(!instance.plugin.supports_stroke_opacity());
    instance.prepare_contact(&s, true, false).unwrap();
    assert_eq!(OPTIONS.with(|record| *record.borrow()).unwrap().0, 56);
    for (flow, mode) in [(0.5, OpacityMode::Dab), (1., OpacityMode::Stroke)] {
        s.parameters.flow = flow;
        s.parameters.opacity_mode = mode;
        assert_eq!(
            instance.prepare_contact(&s, true, true).unwrap_err().code,
            "tool_options_unsupported"
        );
        assert!(matches!(
            s.doc.mask_plane(id).unwrap(),
            PlaneView::Uniform(177)
        ));
        assert_eq!(s.doc.composition(), before);
    }
    s.parameters.flow = 1.;
    s.parameters.opacity_mode = OpacityMode::Dab;
    instance.prepare_contact(&s, true, true).unwrap();
    let lease = s.doc.begin_mask_stroke(id).unwrap();
    instance
        .pointer(&mut s, 1, sample(), None, true, Some(lease))
        .unwrap();
    assert_eq!(HOST.with(|record| *record.borrow()).unwrap().0, 40);
}

#[test]
fn stroke_options_freeze_selection_in_complete_tail_until_the_instance_reconfigures() {
    use crate::engine::selection::{Mask, SelectionState};
    let mut s = session();
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(70, 67, vec![128; 70 * 67]).unwrap()),
            previous: None,
        })
        .unwrap();
    s.parameters.opacity_mode = OpacityMode::Stroke;
    let mut instance = inspected_instance(&s, 15);
    instance.prepare_contact(&s, true, false).unwrap();
    let loan = SELECTION.with(|record| *record.borrow());
    assert_eq!(
        loan.data,
        coverage(instance.selection.as_ref().unwrap()).data
    );
    assert_eq!(
        (loan.width, loan.height, loan.stride, loan.len),
        (70, 67, 70, 70 * 67)
    );
    assert_eq!(
        OPTIONS.with(|record| *record.borrow()).unwrap().1,
        abi::TOOL_OPTION_STROKE_OPACITY | abi::TOOL_OPTION_SELECTION
    );
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(70, 67, vec![37; 70 * 67]).unwrap()),
            previous: None,
        })
        .unwrap();
    assert_eq!(unsafe { *loan.data }, 128);
    assert_ne!(
        loan.data,
        coverage(s.doc.selection().current.as_ref().unwrap()).data
    );
    assert_eq!(
        loan.data,
        coverage(instance.selection.as_ref().unwrap()).data
    );
}

#[derive(Default)]
struct Probe {
    views: Vec<(u32, u32, u32, u32, u32, u32, usize)>,
    fail_after: usize,
}
unsafe extern "C" fn rgba_probe(
    user: *mut c_void,
    original: *const abi::OriginPixels,
    current: *mut abi::Pixels,
) -> i32 {
    let probe = &mut *user.cast::<Probe>();
    let (original, current) = (&*original, &mut *current);
    probe.views.push((
        original.origin_x,
        original.origin_y,
        original.width,
        original.height,
        original.stride,
        current.stride,
        original.data as usize,
    ));
    if (
        original.origin_x,
        original.origin_y,
        original.width,
        original.height,
    ) != (
        current.origin_x,
        current.origin_y,
        current.width,
        current.height,
    ) || original.data == current.data.cast_const()
    {
        return abi::ERROR;
    }
    for y in 0..original.height as usize {
        for x in 0..original.width as usize {
            let wanted = 0xff000000
                | ((original.origin_y as usize + y) * 70 + original.origin_x as usize + x) as u32;
            if *original.data.add(y * original.stride as usize + x) != wanted {
                return abi::ERROR;
            }
            *current.data.add(y * current.stride as usize + x) = 0xffabcdef;
        }
    }
    if probe.fail_after == probe.views.len() {
        abi::ERROR
    } else {
        abi::OK
    }
}
unsafe extern "C" fn a8_probe(
    user: *mut c_void,
    original: *const abi::OriginA8Pixels,
    current: *mut abi::A8Pixels,
) -> i32 {
    let probe = &mut *user.cast::<Probe>();
    let (original, current) = (&*original, &mut *current);
    probe.views.push((
        original.origin_x,
        original.origin_y,
        original.width,
        original.height,
        original.stride,
        current.stride,
        original.data as usize,
    ));
    if original.data == current.data.cast_const() {
        return abi::ERROR;
    }
    for y in 0..original.height as usize {
        for x in 0..original.width as usize {
            if *original.data.add(y * original.stride as usize + x) != 177 {
                return abi::ERROR;
            }
            *current.data.add(y * current.stride as usize + x) = 7;
        }
    }
    if probe.fail_after == probe.views.len() {
        abi::ERROR
    } else {
        abi::OK
    }
}

#[test]
fn paired_origin_loans_split_edges_reuse_originals_and_restore_all_regions_on_error() {
    let mut s = session();
    let before = s.doc.layers[0].pixels.clone();
    s.doc.begin_plugin_stroke();
    let mut context = ToolContext {
        session: &mut s,
        busy: false,
        failed: false,
        write_pixels: true,
        mask_lease: None,
        stroke_opacity: true,
        denied: None,
    };
    let mut probe = Probe::default();
    let ctx = (&mut context as *mut ToolContext).cast();
    let region = abi::Region {
        x: 62,
        y: 63,
        width: 8,
        height: 4,
    };
    let user = (&mut probe as *mut Probe).cast();
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(rgba_probe), user) },
        abi::OK
    );
    assert_eq!(
        probe
            .views
            .iter()
            .map(|view| (view.0, view.1, view.2, view.3, view.4, view.5))
            .collect::<Vec<_>>(),
        vec![
            (62, 63, 2, 1, 64, 70),
            (64, 63, 6, 1, 6, 70),
            (62, 64, 2, 3, 64, 70),
            (64, 64, 6, 3, 6, 70)
        ]
    );
    let first = probe.views.clone();
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(rgba_probe), user) },
        abi::OK
    );
    assert_eq!(&probe.views[4..], &first);
    probe.fail_after = 9;
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(rgba_probe), user) },
        abi::ERROR
    );
    assert!(context.failed && !context.busy);
    s.doc.rollback_plugin_stroke();
    assert_eq!(s.doc.layers[0].pixels, before);
}

#[test]
fn paired_a8_origins_keep_raw_bytes_and_invalid_domain_readonly_or_stale_loans_do_not_run() {
    let mut s = session();
    let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 177).unwrap();
    let lease = s.doc.begin_mask_stroke(id).unwrap();
    let mut context = ToolContext {
        session: &mut s,
        busy: false,
        failed: false,
        write_pixels: true,
        mask_lease: Some(lease),
        stroke_opacity: true,
        denied: None,
    };
    let ctx = (&mut context as *mut ToolContext).cast();
    let mut probe = Probe::default();
    let user = (&mut probe as *mut Probe).cast();
    let region = abi::Region {
        x: 62,
        y: 63,
        width: 8,
        height: 4,
    };
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(rgba_probe), user) },
        abi::ERROR
    );
    assert!(probe.views.is_empty());
    context.failed = false;
    context.denied = None;
    assert_eq!(
        unsafe { tool_a8_origin(ctx, region, Some(a8_probe), user) },
        abi::OK
    );
    let first = probe.views.clone();
    assert_eq!(
        unsafe { tool_a8_origin(ctx, region, Some(a8_probe), user) },
        abi::OK
    );
    assert_eq!(&probe.views[4..], &first);
    s.read_depth = 1;
    assert_eq!(
        unsafe { tool_a8_origin(ctx, region, Some(a8_probe), user) },
        abi::ERROR
    );
    s.read_depth = 0;
    context.failed = false;
    context.stroke_opacity = false;
    assert_eq!(
        unsafe { tool_a8_origin(ctx, region, Some(a8_probe), user) },
        abi::ERROR
    );
    context.stroke_opacity = true;
    context.failed = false;
    context.mask_lease.as_mut().unwrap().storage_epoch += 1;
    assert_eq!(
        unsafe { tool_a8_origin(ctx, region, Some(a8_probe), user) },
        abi::ERROR
    );
    assert_eq!(probe.views.len(), 8);
    s.doc.rollback_plugin_stroke();
    assert!(
        matches!(s.doc.mask_plane(id).unwrap(),PlaneView::Dense(values) if values.iter().all(|v| *v==177))
    );
}

unsafe extern "C" fn reenter(
    user: *mut c_void,
    _: *const abi::OriginPixels,
    _: *mut abi::Pixels,
) -> i32 {
    let context = user;
    let _ = tool_pixels_origin(
        context,
        abi::Region {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        },
        None,
        std::ptr::null_mut(),
    );
    abi::OK
}
unsafe extern "C" fn change_alpha(
    _: *mut c_void,
    _: *const abi::OriginPixels,
    current: *mut abi::Pixels,
) -> i32 {
    *(*current).data = 0;
    abi::OK
}
#[test]
fn paired_rgba_origins_latch_ignored_reentry_and_enforce_alpha_lock() {
    let mut s = session();
    let before = s.doc.layers[0].pixels.clone();
    s.doc.begin_plugin_stroke();
    let mut context = ToolContext {
        session: &mut s,
        busy: false,
        failed: false,
        write_pixels: true,
        mask_lease: None,
        stroke_opacity: true,
        denied: None,
    };
    let ctx = (&mut context as *mut ToolContext).cast();
    let region = abi::Region {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    };
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(reenter), ctx) },
        abi::ERROR
    );
    assert!(context.failed && !context.busy);
    s.doc.rollback_plugin_stroke();
    assert_eq!(s.doc.layers[0].pixels, before);
    s.doc
        .set_layer_properties_with_lock(0, true, 255, crate::engine::graph::BlendMode::Normal, true)
        .unwrap();
    s.doc.begin_plugin_stroke();
    context.failed = false;
    assert_eq!(
        unsafe { tool_pixels_origin(ctx, region, Some(change_alpha), std::ptr::null_mut()) },
        abi::ERROR
    );
    assert!(context.failed);
    s.doc.rollback_plugin_stroke();
    assert_eq!(s.doc.layers[0].pixels, before);
}

unsafe extern "C" fn failed_contact(
    _: *mut c_void,
    _: u32,
    _: *const abi::ToolSample,
    _: *const abi::ToolParameters,
    host: *const abi::ToolHost,
    out: *mut abi::ToolOutput,
) -> i32 {
    if (*host).struct_size < 64 {
        return abi::ERROR;
    }
    let tail = &*host.cast::<abi::ToolHostStroke>();
    let mut probe = Probe {
        views: Vec::new(),
        fail_after: 2,
    };
    let user = (&mut probe as *mut Probe).cast();
    for region in [
        abi::Region {
            x: 1,
            y: 1,
            width: 2,
            height: 2,
        },
        abi::Region {
            x: 65,
            y: 1,
            width: 2,
            height: 2,
        },
    ] {
        if let Some(loan) = tail.pixels_origin {
            let _ = loan((*host).context, region, Some(rgba_probe), user);
        } else if let Some(loan) = tail.a8_origin {
            let _ = loan((*host).context, region, Some(a8_probe), user);
        } else {
            return abi::ERROR;
        }
    }
    (*out).flags = abi::DRAWING;
    // Host must reject even a DLL that ignores its second failed region loan.
    abi::OK
}
#[test]
fn failed_original_loan_rolls_back_complete_rgba_and_a8_session_contacts() {
    for a8 in [false, true] {
        let mut s = session();
        let id = if a8 {
            let id = s.doc.create_mask(MaskOwner::Layer { id: 1 }, 177).unwrap();
            s.doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
            Some(id)
        } else {
            None
        };
        s.parameters.opacity_mode = OpacityMode::Stroke;
        let mut instance =
            ToolInstance::new(s.registry.native_plugins["painting"].clone()).unwrap();
        instance.api.pointer = Some(failed_contact);
        s.tool_instances.insert("painting".into(), instance);
        let before = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let bytes = s.doc.history_bytes();
        assert!(s
            .pointer(Pointer {
                phase: PointerPhase::Down,
                x: 1.,
                y: 1.,
                pressure: 1.,
                time: 0,
                pen: false,
                eraser: false,
                view_x: None,
                view_y: None,
                tool_override: None
            })
            .is_err());
        assert_eq!(s.state(), before);
        assert_eq!(s.doc.layers[0].pixels, pixels);
        assert_eq!(s.doc.history_bytes(), bytes);
        assert!(!s.stroke.active() && s.pointer_checkpoint.is_none());
        if let Some(id) = id {
            assert!(matches!(
                s.doc.mask_plane(id).unwrap(),
                PlaneView::Uniform(177)
            ));
        }
        assert!(
            s.apply(Action::new("replay.verify", json!({}))).unwrap()["result"]["valid"]
                .as_bool()
                .unwrap()
        );
    }
}
