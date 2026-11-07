use super::*;
use crate::engine::graph::Renderer;
use std::sync::atomic::{AtomicUsize, Ordering};
static CREATED: AtomicUsize = AtomicUsize::new(0);
static DESTROYED: AtomicUsize = AtomicUsize::new(0);
static RESERVED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn render(
    _: *const abi::CompositeNode,
    _: usize,
    _: *const abi::CompositeLayer,
    _: usize,
    _: u32,
    _: u32,
    _: *mut abi::Pixels,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn masked(
    _: *const abi::CompositeNode,
    _: usize,
    _: *const abi::CompositeLayer,
    _: usize,
    _: *const abi::CompositeMask,
    _: usize,
    _: u32,
    _: u32,
    _: *mut abi::Pixels,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn create() -> *mut c_void {
    CREATED.fetch_add(1, Ordering::Relaxed);
    Box::into_raw(Box::new(1u32)).cast()
}
unsafe extern "C" fn destroy(pointer: *mut c_void) {
    DESTROYED.fetch_add(1, Ordering::Relaxed);
    drop(Box::from_raw(pointer.cast::<u32>()));
}
unsafe extern "C" fn effect(
    _: *mut c_void,
    _: *const abi::MaskEffectRequest,
    _: *const abi::CompositeMask,
    _: u32,
    _: u32,
    _: *mut abi::A8Pixels,
) -> i32 {
    abi::ERROR
}
unsafe extern "C" fn reserve(pointer: *mut c_void, width: u32, height: u32, radius: u32) -> i32 {
    RESERVED.fetch_add(1, Ordering::Relaxed);
    if pointer.is_null()
        || !(1..=4096).contains(&width)
        || !(1..=4096).contains(&height)
        || radius > 256
    {
        abi::ERROR
    } else {
        abi::OK
    }
}

fn table() -> abi::CompositorApiMaskEffects {
    abi::CompositorApiMaskEffects {
        base: abi::CompositorApiMasks {
            base: abi::CompositorApiModes {
                base: abi::CompositorApiFeatures {
                    base: abi::CompositorApi {
                        abi_version: 1,
                        struct_size: 72,
                        render: Some(render),
                    },
                    capabilities: abi::COMPOSITOR_MASKS | abi::COMPOSITOR_MASK_EFFECTS,
                },
                supported_blend_modes: abi::BLEND_MODES_LEGACY,
            },
            render_masked: Some(masked),
        },
        create_mask_effects: Some(create),
        destroy_mask_effects: Some(destroy),
        mask_effect_region: Some(effect),
        reserve_mask_effects: Some(reserve),
    }
}

#[test]
fn mask_effect_tail_requires_advertisement_complete_size_and_all_callbacks() {
    let mut value = table();
    for size in 16..=72 {
        value.base.base.base.base.struct_size = size;
        let decoded = unsafe { decode_compositor_table(&value.base.base.base.base) };
        if size < 24 {
            assert!(decoded.is_ok());
        } else if size < 72 {
            assert!(decoded.is_err(), "size {size}");
        } else {
            assert!(decoded.is_ok());
        }
    }
    for missing in 0..4 {
        let mut value = table();
        match missing {
            0 => value.create_mask_effects = None,
            1 => value.destroy_mask_effects = None,
            2 => value.mask_effect_region = None,
            _ => value.reserve_mask_effects = None,
        }
        assert!(
            unsafe { decode_compositor_table(&value.base.base.base.base) }
                .err()
                .unwrap()
                .contains("callbacks")
        );
    }
    let mut value = table();
    value.base.base.base.capabilities = abi::COMPOSITOR_MASK_EFFECTS;
    assert!(unsafe { decode_compositor_table(&value.base.base.base.base) }.is_err());
    value.base.base.base.capabilities = abi::COMPOSITOR_MASKS;
    value.base.base.base.base.struct_size = 40;
    let (_, capabilities, _, masked) =
        unsafe { decode_compositor_table(&value.base.base.base.base) }.unwrap();
    assert!(masked.is_some());
    assert!(
        unsafe { decode_mask_effects_table(&value.base.base.base.base, capabilities) }
            .unwrap()
            .is_none()
    );
    assert!(
        unsafe { decode_mask_effects_table(std::ptr::null(), abi::COMPOSITOR_MASK_EFFECTS) }
            .is_err()
    );
    // The unpublished 64-byte effects prototype is not a complete cap-8 tail.
    #[repr(C)]
    struct Prototype64 {
        base: abi::CompositorApiMasks,
        create: Option<abi::MaskEffectsCreateFn>,
        destroy: Option<abi::MaskEffectsDestroyFn>,
        region: Option<abi::MaskEffectRegionFn>,
    }
    let mut complete = table();
    complete.base.base.base.base.struct_size = 64;
    let prototype = Prototype64 {
        base: complete.base,
        create: Some(create),
        destroy: Some(destroy),
        region: Some(effect),
    };
    assert_eq!(std::mem::size_of::<Prototype64>(), 64);
    assert!(unsafe { decode_compositor_table(&prototype.base.base.base.base) }.is_err());
}

#[test]
fn loaded_effect_bridge_reuses_one_instance_and_handles_uniform_and_raw_inversion() {
    let registry =
        crate::plugins::Registry::load(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
            .unwrap();
    let plugin = registry
        .native_plugins
        .get("compositing")
        .expect("Stage the matching compositor DLL");
    assert!(plugin.supports_mask_effects());
    assert!(plugin.mask_effect_instance.lock().unwrap().is_none());
    plugin.reserve_mask_effects(4, 4, 256).unwrap();
    let reserved_pointer = plugin
        .mask_effect_instance
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .pointer;
    let mut bytes = vec![11; 6];
    let mut output = abi::A8Pixels {
        data: bytes.as_mut_ptr(),
        len: bytes.len(),
        width: 2,
        height: 2,
        stride: 3,
        origin_x: 1,
        origin_y: 1,
    };
    let uniform = abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 0,
    };
    let options = abi::MaskEffectRequest {
        flags: 0,
        density: 128,
        feather_radius: 256,
        reserved: 0,
    };
    plugin
        .mask_effect_region(&options, &uniform, 4, 4, &mut output)
        .unwrap();
    assert_eq!(bytes, vec![127, 127, 11, 127, 127, 11]);
    let pointer = plugin
        .mask_effect_instance
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .pointer;
    assert_eq!(pointer, reserved_pointer);
    let invert = abi::MaskEffectRequest {
        flags: abi::MASK_EFFECT_INVERT_RAW,
        density: 255,
        feather_radius: 0,
        reserved: 0,
    };
    plugin
        .mask_effect_region(&invert, &uniform, 4, 4, &mut output)
        .unwrap();
    assert_eq!(bytes, vec![255, 255, 11, 255, 255, 11]);
    assert_eq!(
        plugin
            .mask_effect_instance
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .pointer,
        pointer
    );
    let invalid = abi::MaskEffectRequest {
        feather_radius: 257,
        ..options
    };
    let unchanged = bytes.clone();
    assert!(plugin
        .mask_effect_region(&invalid, &uniform, 4, 4, &mut output)
        .is_err());
    assert_eq!(bytes, unchanged);
}

#[test]
fn cached_effect_instance_is_created_once_even_after_errors_and_destroyed_with_its_owner() {
    let native = Arc::new(NativePlugin {
        path: PathBuf::new(),
        api: abi::PluginApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::PluginApi>() as u32,
            describe: None,
            create: None,
            destroy: None,
            invoke: None,
        },
        tools: None,
        tool_options: None,
        selection_mix: None,
        selection_color: None,
        selection_edges: None,
        selection_transfer: None,
        geometry: None,
        trim: None,
        movement: None,
        affine: None,
        projective: None,
        resample: None,
        resample_features: 0,
        a8_sampler: None,
        a8_affine: None,
        a8_affine_bounds: None,
        compositor: None,
        compositor_capabilities: 0,
        compositor_blend_modes: 0,
        compositor_masked: None,
        compositor_effects: Some((create, destroy, effect, reserve)),
        mask_effect_instance: Default::default(),
        _module: Module { handle: 0 },
    });
    let created = CREATED.load(Ordering::Relaxed);
    let destroyed = DESTROYED.load(Ordering::Relaxed);
    let reserved = RESERVED.load(Ordering::Relaxed);
    let options = abi::MaskEffectRequest {
        flags: 0,
        density: 255,
        feather_radius: 1,
        reserved: 0,
    };
    let source = abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 255,
    };
    let mut byte = [0];
    let mut out = abi::A8Pixels {
        data: byte.as_mut_ptr(),
        len: 1,
        width: 1,
        height: 1,
        stride: 1,
        origin_x: 0,
        origin_y: 0,
    };
    for _ in 0..3 {
        native.reserve_mask_effects(1, 1, 1).unwrap();
        assert!(native
            .mask_effect_region(&options, &source, 1, 1, &mut out)
            .is_err());
    }
    assert!(native.reserve_mask_effects(1, 1, 257).is_err());
    assert_eq!(RESERVED.load(Ordering::Relaxed), reserved + 4);
    assert_eq!(CREATED.load(Ordering::Relaxed), created + 1);
    let other = native.clone();
    drop(native);
    assert_eq!(DESTROYED.load(Ordering::Relaxed), destroyed);
    drop(other);
    assert_eq!(DESTROYED.load(Ordering::Relaxed), destroyed + 1);
}
