//! Real typed A8 contacts and hostile loan failures through the shared Session.
use super::*;
use crate::{
    application::{Pointer, PointerPhase, ToolKind},
    engine::{
        graph::BlendMode,
        groups::GroupItem,
        raster_masks::{MaskOwner, PaintTarget, PlaneView},
        replay::Journal,
        selection::{Mask, SelectionState},
        Document,
    },
};

fn session() -> Session {
    let mut s = Session::new(super::super::Registry::bundled()).unwrap();
    s.doc = Document::new(72, 40);
    s.doc.renderer = s.registry.renderer();
    s.doc.layers[0].pixels.fill(0xff334455);
    s.parameters.radius = 4.;
    s.parameters.opacity = 1.;
    s.parameters.hardness = 1.;
    s.parameters.color = "#FF0000".into();
    s.parameters.stabilizer = 0.;
    s.parameters.pressure_size = false;
    s.parameters.pressure_opacity = false;
    s.parameters.spacing = 0.25;
    s
}

fn masked(initial: u8) -> (Session, u64) {
    let mut s = session();
    let id = s
        .doc
        .create_mask(MaskOwner::Layer { id: 1 }, initial)
        .unwrap();
    s.doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    s.doc.dirty = false;
    (s, id)
}

fn samples(s: &Session, id: u64) -> Vec<u8> {
    match s.doc.mask_plane(id).unwrap() {
        PlaneView::Uniform(value) => vec![value; s.doc.width * s.doc.height],
        PlaneView::Dense(bytes) => bytes.to_vec(),
    }
}

fn pointer(phase: PointerPhase, x: f32, y: f32, time: u32) -> Pointer {
    Pointer {
        phase,
        x,
        y,
        time,
        pressure: 1.,
        pen: false,
        eraser: false,
        view_x: None,
        view_y: None,
        tool_override: None,
    }
}

fn contact(s: &mut Session, p: Pointer) {
    s.pointer(p).unwrap();
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        time: p.time + 1,
        ..p
    })
    .unwrap();
}

fn lock_pixels(s: &mut Session) {
    let label = s.doc.color_label(0);
    s.doc
        .set_layer_properties_organized(0, true, 255, BlendMode::Normal, true, true, true, label)
        .unwrap();
}

#[test]
fn a8_brush_and_eraser_ignore_layer_locks_and_undo_one_contact_at_a_time() {
    let (mut s, id) = masked(255);
    lock_pixels(&mut s);
    let rgba = s.doc.layers[0].pixels.clone();
    let p = pointer(PointerPhase::Down, 12.5, 12.5, 0);
    contact(&mut s, p);
    // The published integer grayscale contract maps encoded red to 54.
    assert_eq!(samples(&s, id)[12 * 72 + 12], 54);
    assert_eq!(s.doc.layers[0].pixels, rgba);
    let painted = samples(&s, id);
    contact(
        &mut s,
        Pointer {
            tool_override: Some(ToolKind::Eraser),
            time: 10,
            ..p
        },
    );
    assert_eq!(samples(&s, id)[12 * 72 + 12], 255);
    s.doc.undo();
    assert_eq!(samples(&s, id), painted);
    contact(
        &mut s,
        Pointer {
            pen: true,
            eraser: true,
            time: 20,
            ..p
        },
    );
    assert_eq!(samples(&s, id)[12 * 72 + 12], 255);
    s.doc.undo();
    assert_eq!(samples(&s, id), painted);
    s.doc.undo();
    assert!(samples(&s, id).iter().all(|value| *value == 255));
    s.doc.redo();
    assert_eq!(samples(&s, id), painted);
    assert_eq!(s.doc.layers[0].pixels, rgba);
    s.doc.select_layer(0);
    assert_eq!(s.pointer(p).unwrap_err().code, "pixel_locked");
}

#[test]
fn a8_selection_is_fractional_once_and_down_parameters_and_native_instance_stay_cached() {
    let (mut s, id) = masked(200);
    s.parameters.color = "#000000".into();
    let coverage: Vec<_> = (0..40)
        .flat_map(|_| {
            (0..72).map(|x| {
                if x < 12 {
                    0
                } else if x == 12 {
                    128
                } else {
                    255
                }
            })
        })
        .collect();
    s.doc
        .set_selection(SelectionState {
            current: Some(Mask::new(72, 40, coverage).unwrap()),
            previous: None,
        })
        .unwrap();
    let mut instance = ToolInstance::new(s.registry.native_plugins["painting"].clone()).unwrap();
    let state = instance.state;
    // The real native pointer/configuration callbacks remain attached to this state.
    instance.selection = None;
    s.tool_instances.insert("painting".into(), instance);
    let p = pointer(PointerPhase::Down, 12.5, 12.5, 0);
    s.pointer(p).unwrap();
    let values = samples(&s, id);
    assert_eq!(values[12 * 72 + 11], 200);
    assert_eq!(values[12 * 72 + 12], 100);
    assert_eq!(values[12 * 72 + 13], 0);
    s.parameters.color = "#FFFFFF".into();
    s.parameters.opacity = 0.;
    s.registry.native_tools.clear();
    s.registry.native_plugins.remove("painting");
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        x: 32.5,
        time: 20,
        ..p
    })
    .unwrap();
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        x: 32.5,
        time: 30,
        ..p
    })
    .unwrap();
    assert_eq!(samples(&s, id)[12 * 72 + 32], 0);
    assert_eq!(s.tool_instances["painting"].state, state);
    assert!(s.doc.layers[0]
        .pixels
        .iter()
        .all(|pixel| *pixel == 0xff334455));
}

#[test]
fn a8_pressure_and_stabilizer_tail_keep_verified_bytes_without_dll_playback() {
    let (mut s, id) = masked(255);
    s.parameters.radius = 2.;
    s.parameters.stabilizer = 0.65;
    s.parameters.pressure_opacity = true;
    let p = Pointer {
        pen: true,
        pressure: 0.1,
        ..pointer(PointerPhase::Down, 8.5, 8.5, 0)
    };
    s.pointer(p).unwrap();
    let pointer = match s.doc.mask_plane(id).unwrap() {
        PlaneView::Dense(bytes) => bytes.as_ptr(),
        _ => panic!(),
    };
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        x: 32.5,
        pressure: 0.4,
        time: 20,
        ..p
    })
    .unwrap();
    s.pointer(Pointer {
        phase: PointerPhase::Move,
        x: 60.5,
        y: 26.5,
        pressure: 1.,
        time: 40,
        ..p
    })
    .unwrap();
    let before_tail = samples(&s, id)[26 * 72 + 60];
    s.pointer(Pointer {
        phase: PointerPhase::Up,
        x: 60.5,
        y: 26.5,
        pressure: 0.,
        time: 60,
        ..p
    })
    .unwrap();
    let expected = samples(&s, id);
    assert_eq!(expected[26 * 72 + 60], 54);
    assert!(expected[26 * 72 + 60] < before_tail);
    assert!(expected[8 * 72 + 8] > 54 && expected[8 * 72 + 8] < 255);
    assert!(
        matches!(s.doc.mask_plane(id).unwrap(), PlaneView::Dense(bytes) if bytes.as_ptr() == pointer)
    );
    assert_eq!(
        s.revision, 4,
        "Retain the shared per-successful-sample revision policy"
    );
    let journal = s.replay_journal(false).unwrap();
    assert_eq!(
        journal.entries.len(),
        2,
        "One completed pointer outcome after genesis"
    );
    let rows = journal.events(0, 10).unwrap();
    assert_eq!(rows["events"][1]["source"]["complete"], true);
    assert!(rows["events"][1]["changed_mask_tiles"].as_u64().unwrap() > 0);
    let archive = journal.bytes(None).unwrap();
    let mut imported = Journal::import(&archive).unwrap();
    assert_eq!(imported.verify(None).unwrap()["valid"], true);
    let state = imported.preview(1).unwrap();
    let mut restored = Document::new(72, 40);
    state.restore_document(&mut restored);
    assert!(restored.renderer.is_none());
    assert!(state.matches_document(&restored));
    assert_eq!(restored.mask_plane(id).unwrap().sample(26 * 72 + 60), 54);
    s.doc.undo();
    assert!(samples(&s, id).iter().all(|value| *value == 255));
    s.doc.redo();
    assert_eq!(samples(&s, id), expected);
}

#[test]
fn a8_contact_reconfiguration_restores_rgba_painting_and_read_tools_never_write_masks() {
    let (mut s, id) = masked(255);
    let p = pointer(PointerPhase::Down, 12.5, 12.5, 0);
    contact(&mut s, p);
    let raw = s.doc.layers[0].pixels.clone();
    let mask = samples(&s, id);
    let history = s.doc.history_bytes();
    for (index, kind) in [ToolKind::Picker, ToolKind::Hand, ToolKind::Zoom]
        .into_iter()
        .enumerate()
    {
        contact(
            &mut s,
            Pointer {
                tool_override: Some(kind),
                time: 20 + index as u32 * 10,
                ..p
            },
        );
        assert_eq!(samples(&s, id), mask);
        assert_eq!(s.doc.layers[0].pixels, raw);
        assert_eq!(s.doc.history_bytes(), history);
    }
    s.doc.select_layer(0);
    s.parameters.color = "#00FF00".into();
    contact(&mut s, Pointer { time: 100, ..p });
    assert_eq!(s.doc.layers[0].pixels[12 * 72 + 12], 0xff00ff00);
    assert_eq!(samples(&s, id), mask);
}

fn limited_instance(s: &Session, features: Option<u32>) -> ToolInstance {
    let path = &s.registry.native_plugins["painting"].path;
    let (mut plugin, _) = NativePlugin::load(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        "painting",
    )
    .unwrap();
    {
        let table = Arc::get_mut(&mut plugin).unwrap();
        table.tool_options = features.map(|features| (table.tool_options.unwrap().0, features));
    }
    ToolInstance::new(plugin).unwrap()
}

#[test]
fn missing_a8_or_selection_capabilities_reject_before_uniform_materialization() {
    for features in [
        None,
        Some(abi::TOOL_OPTION_SELECTION),
        Some(abi::TOOL_OPTION_A8_TARGET),
    ] {
        let (mut s, id) = masked(255);
        if features == Some(abi::TOOL_OPTION_A8_TARGET) {
            s.doc
                .set_selection(SelectionState {
                    current: Some(Mask::new(72, 40, vec![255; 72 * 40]).unwrap()),
                    previous: None,
                })
                .unwrap();
        }
        let instance = limited_instance(&s, features);
        s.tool_instances.insert("painting".into(), instance);
        let metadata = s.doc.composition();
        let history = s.doc.history_bytes();
        let revision = s.revision;
        let error = s
            .pointer(pointer(PointerPhase::Down, 12.5, 12.5, 0))
            .unwrap_err();
        assert_eq!(error.code, "tool_options_unsupported");
        assert!(matches!(
            s.doc.mask_plane(id).unwrap(),
            PlaneView::Uniform(255)
        ));
        assert_eq!(s.doc.composition(), metadata);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.revision, revision);
        assert!(!s.is_drawing());
    }
}

#[test]
fn group_mask_painting_checks_owner_visibility_and_does_not_depend_on_active_pixel_locks() {
    let mut s = session();
    let group = s
        .doc
        .group_create(&[GroupItem::layer(1)], None, None)
        .unwrap();
    let id = s
        .doc
        .create_mask(MaskOwner::Group { id: group }, 255)
        .unwrap();
    s.doc.set_mask_target(PaintTarget::Mask { id }).unwrap();
    lock_pixels(&mut s);
    let p = pointer(PointerPhase::Down, 12.5, 12.5, 0);
    contact(&mut s, p);
    assert_eq!(samples(&s, id)[12 * 72 + 12], 54);
    s.doc
        .group_set(group, None, Some(false), None, None, None)
        .unwrap();
    let before = samples(&s, id);
    let history = s.doc.history_bytes();
    let error = s.pointer(Pointer { time: 20, ..p }).unwrap_err();
    assert_eq!(error.code, "mask_painting");
    assert!(error.to_string().contains("hidden"));
    assert_eq!(samples(&s, id), before);
    assert_eq!(s.doc.history_bytes(), history);

    let (mut hidden, mask) = masked(255);
    hidden
        .doc
        .set_layer_properties(0, false, 255, BlendMode::Normal)
        .unwrap();
    assert!(hidden.pointer(p).is_err());
    assert!(matches!(
        hidden.doc.mask_plane(mask).unwrap(),
        PlaneView::Uniform(255)
    ));
}

unsafe extern "C" fn paint_a8(_: *mut c_void, pixels: *mut abi::A8Pixels) -> i32 {
    let pixels = &mut *pixels;
    for y in 0..pixels.height as usize {
        for x in 0..pixels.width as usize {
            *pixels.data.add(y * pixels.stride as usize + x) = 37;
        }
    }
    abi::OK
}
unsafe extern "C" fn fail_a8(user: *mut c_void, pixels: *mut abi::A8Pixels) -> i32 {
    paint_a8(user, pixels);
    abi::ERROR
}
unsafe extern "C" fn failing_a8_tool(
    _: *mut c_void,
    _: u32,
    sample: *const abi::ToolSample,
    _: *const abi::ToolParameters,
    host: *const abi::ToolHost,
    output: *mut abi::ToolOutput,
) -> i32 {
    let down = (*sample).phase == abi::DOWN;
    if (*host).struct_size != std::mem::size_of::<abi::ToolHostA8>() as u32 {
        return abi::ERROR;
    }
    let extended = &*host.cast::<abi::ToolHostA8>();
    extended.a8.unwrap()(
        (*host).context,
        abi::Region {
            x: if down { 3 } else { 65 },
            y: 3,
            width: 3,
            height: 3,
        },
        Some(if down { paint_a8 } else { fail_a8 }),
        std::ptr::null_mut(),
    );
    (*output).flags = if down { abi::DRAWING } else { 0 };
    // Ignoring a transform error must not make it possible to commit partial bytes.
    abi::OK
}

#[test]
fn a8_move_up_and_cancel_callback_failures_restore_the_complete_contact() {
    for phase in [PointerPhase::Move, PointerPhase::Up, PointerPhase::Cancel] {
        let (mut s, id) = masked(255);
        let mut instance =
            ToolInstance::new(s.registry.native_plugins["painting"].clone()).unwrap();
        instance.api.pointer = Some(failing_a8_tool);
        s.tool_instances.insert("painting".into(), instance);
        let rgba = s.doc.layers[0].pixels.clone();
        let metadata = s.doc.composition();
        let history = s.doc.history_bytes();
        let revision = s.revision;
        let p = pointer(PointerPhase::Down, 4., 4., 0);
        s.pointer(p).unwrap();
        assert_eq!(samples(&s, id)[3 * 72 + 3], 37);
        assert!(s
            .pointer(Pointer {
                phase,
                time: 10,
                ..p
            })
            .is_err());
        assert!(samples(&s, id).iter().all(|value| *value == 255));
        assert_eq!(s.doc.layers[0].pixels, rgba);
        assert_eq!(s.doc.composition(), metadata);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.revision, revision);
        assert!(!s.doc.dirty);
        assert!(!s.is_drawing());
        assert!(!s.tool_instances.contains_key("painting"));
        assert_eq!(
            s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
            true
        );
    }
}

#[test]
fn a8_journal_failure_rolls_back_raw_bytes_history_and_target() {
    let (mut s, id) = masked(255);
    let metadata = s.doc.composition();
    let history = s.doc.history_bytes();
    let revision = s.revision;
    let p = pointer(PointerPhase::Down, 12.5, 12.5, 0);
    s.pointer(p).unwrap();
    assert_eq!(samples(&s, id)[12 * 72 + 12], 54);
    s.journal.as_mut().unwrap().fail_next_write_after(7);
    assert!(s
        .pointer(Pointer {
            phase: PointerPhase::Up,
            time: 10,
            ..p
        })
        .is_err());
    assert!(samples(&s, id).iter().all(|value| *value == 255));
    assert_eq!(s.doc.composition(), metadata);
    assert_eq!(s.doc.history_bytes(), history);
    assert_eq!(s.revision, revision);
    assert!(!s.doc.dirty);
    assert!(!s.is_drawing());
    assert_eq!(
        s.journal.as_mut().unwrap().verify(None).unwrap()["valid"],
        true
    );
}

unsafe extern "C" fn count_a8(user: *mut c_void, _: *mut abi::A8Pixels) -> i32 {
    *user.cast::<usize>() += 1;
    abi::OK
}
unsafe extern "C" fn count_rgba(user: *mut c_void, _: *mut abi::Pixels) -> i32 {
    *user.cast::<usize>() += 1;
    abi::OK
}

#[test]
fn a8_loan_rejects_wrong_targets_read_tools_reentry_and_stale_regions_before_callbacks() {
    let (mut s, id) = masked(255);
    let lease = s.doc.begin_mask_stroke(id).unwrap();
    let mut context = ToolContext {
        session: &mut s,
        busy: false,
        failed: false,
        write_pixels: false,
        mask_lease: Some(lease),
        denied: None,
    };
    let pointer = (&mut context as *mut ToolContext).cast();
    let region = abi::Region {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    };
    let mut calls = 0usize;
    let user = (&mut calls as *mut usize).cast();
    assert_eq!(
        unsafe { tool_a8(pointer, region, Some(count_a8), user) },
        abi::ERROR
    );
    context.write_pixels = true;
    context.failed = false;
    context.mask_lease = None;
    assert_eq!(
        unsafe { tool_a8(pointer, region, Some(count_a8), user) },
        abi::ERROR
    );
    context.mask_lease = Some(lease);
    context.failed = false;
    assert_eq!(
        unsafe { tool_pixels(pointer, region, Some(count_rgba), user) },
        abi::ERROR
    );
    context.failed = false;
    context.busy = true;
    assert_eq!(
        unsafe { tool_a8(pointer, region, Some(count_a8), user) },
        abi::ERROR
    );
    context.busy = false;
    context.failed = false;
    s.read_depth = 1;
    assert_eq!(
        unsafe { tool_a8(pointer, region, Some(count_a8), user) },
        abi::ERROR
    );
    s.read_depth = 0;
    context.failed = false;
    assert_eq!(
        unsafe {
            tool_a8(
                pointer,
                abi::Region { width: 0, ..region },
                Some(count_a8),
                user,
            )
        },
        abi::ERROR
    );
    context.failed = false;
    s.doc.finish();
    assert_eq!(
        unsafe { tool_a8(pointer, region, Some(count_a8), user) },
        abi::ERROR
    );
    assert_eq!(calls, 0);
    assert!(samples(&s, id).iter().all(|value| *value == 255));
}

#[test]
fn zero_pressure_mask_contact_deposits_nothing_and_adds_no_pixel_history() {
    let (mut s, id) = masked(255);
    let history = s.doc.history_bytes();
    contact(
        &mut s,
        Pointer {
            pen: true,
            pressure: 0.,
            ..pointer(PointerPhase::Down, 12.5, 12.5, 0)
        },
    );
    assert!(samples(&s, id).iter().all(|value| *value == 255));
    assert_eq!(s.doc.history_bytes(), history);
    assert!(!s.doc.dirty);
}

thread_local! {
    static ORIGINAL_TOOL: std::cell::Cell<Option<abi::ToolApi>> = const { std::cell::Cell::new(None) };
}
unsafe extern "C" fn reject_ending_a8(
    _: *mut c_void,
    _: abi::Region,
    _: Option<abi::A8Transform>,
    _: *mut c_void,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn native_tail_failure(
    state: *mut c_void,
    handler: u32,
    sample: *const abi::ToolSample,
    parameters: *const abi::ToolParameters,
    host: *const abi::ToolHost,
    output: *mut abi::ToolOutput,
) -> i32 {
    let Some(original) = ORIGINAL_TOOL.with(|slot| slot.get().and_then(|api| api.pointer)) else {
        return abi::ERROR;
    };
    if (*sample).phase == abi::UP || (*sample).phase == abi::CANCEL {
        let original_host = &*host;
        let ending = abi::ToolHostA8 {
            base: abi::ToolHost {
                abi_version: original_host.abi_version,
                struct_size: std::mem::size_of::<abi::ToolHostA8>() as u32,
                context: original_host.context,
                pixels: original_host.pixels,
                pick: original_host.pick,
            },
            a8: Some(reject_ending_a8),
        };
        original(state, handler, sample, parameters, &ending.base, output)
    } else {
        original(state, handler, sample, parameters, host, output)
    }
}

#[test]
fn real_native_up_and_cancel_stabilizer_tail_errors_roll_back_and_discard_the_instance() {
    for phase in [PointerPhase::Up, PointerPhase::Cancel] {
        let (mut s, id) = masked(255);
        s.parameters.stabilizer = 0.9;
        let mut instance =
            ToolInstance::new(s.registry.native_plugins["painting"].clone()).unwrap();
        ORIGINAL_TOOL.with(|slot| slot.set(Some(instance.api)));
        instance.api.pointer = Some(native_tail_failure);
        s.tool_instances.insert("painting".into(), instance);
        let history = s.doc.history_bytes();
        let p = pointer(PointerPhase::Down, 8.5, 8.5, 0);
        s.pointer(p).unwrap();
        s.pointer(Pointer {
            phase: PointerPhase::Move,
            x: 60.5,
            y: 26.5,
            time: 20,
            ..p
        })
        .unwrap();
        assert!(samples(&s, id).iter().any(|value| *value != 255));
        assert!(s
            .pointer(Pointer {
                phase,
                x: 60.5,
                y: 26.5,
                time: 40,
                ..p
            })
            .is_err());
        assert!(samples(&s, id).iter().all(|value| *value == 255));
        assert_eq!(s.doc.history_bytes(), history);
        assert!(!s.is_drawing());
        assert!(!s.tool_instances.contains_key("painting"));
        contact(&mut s, Pointer { time: 100, ..p });
        assert!(samples(&s, id).iter().any(|value| *value != 255));
        ORIGINAL_TOOL.with(|slot| slot.set(None));
    }
}
