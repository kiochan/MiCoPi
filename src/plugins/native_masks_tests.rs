//! Optional mask-table negotiation and dispatch, independent of document storage.
use super::*;
use crate::engine::graph::Renderer;

unsafe extern "C" fn legacy(
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

fn table() -> abi::CompositorApiMasks {
    abi::CompositorApiMasks {
        base: abi::CompositorApiModes {
            base: abi::CompositorApiFeatures {
                base: abi::CompositorApi {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::CompositorApiMasks>() as u32,
                    render: Some(legacy),
                },
                capabilities: abi::COMPOSITOR_TRANSPARENT
                    | abi::COMPOSITOR_MIX
                    | abi::COMPOSITOR_MASKS,
            },
            supported_blend_modes: abi::BLEND_MODES_ALL,
        },
        render_masked: Some(masked),
    }
}

#[test]
fn mask_tail_requires_its_advertised_complete_callback_without_reinterpreting_old_prefixes() {
    let mut value = table();
    for size in 16..=40 {
        value.base.base.base.struct_size = size;
        let decoded = unsafe { decode_compositor_table(&value.base.base.base) };
        if size < 24 {
            let (_, capabilities, modes, callback) = decoded.unwrap();
            assert_eq!(capabilities, 0);
            assert_eq!(modes, abi::BLEND_MODES_LEGACY);
            assert!(callback.is_none());
        } else if size < 40 {
            assert!(
                decoded.err().unwrap().contains("complete table"),
                "size {size}"
            );
        } else {
            let (_, capabilities, modes, callback) = decoded.unwrap();
            assert_eq!(capabilities & abi::COMPOSITOR_MASKS, abi::COMPOSITOR_MASKS);
            assert_eq!(modes, abi::BLEND_MODES_ALL);
            assert!(callback.is_some());
        }
    }
    value.render_masked = None;
    assert!(unsafe { decode_compositor_table(&value.base.base.base) }
        .err()
        .unwrap()
        .contains("missing render_masked"));
    value.base.base.capabilities &= !abi::COMPOSITOR_MASKS;
    for size in 16..=40 {
        value.base.base.base.struct_size = size;
        let (_, capabilities, modes, callback) =
            unsafe { decode_compositor_table(&value.base.base.base) }.unwrap();
        assert_eq!(capabilities & abi::COMPOSITOR_MASKS, 0);
        assert_eq!(
            modes,
            if size < 32 {
                abi::BLEND_MODES_LEGACY
            } else {
                abi::BLEND_MODES_ALL
            }
        );
        assert!(callback.is_none());
    }
    value.render_masked = Some(masked);
    assert!(
        unsafe { decode_compositor_table(&value.base.base.base) }
            .unwrap()
            .3
            .is_none(),
        "An unadvertised callback must not enable masks"
    );
    value.base.base.capabilities |= abi::COMPOSITOR_MASKS;
    value.base.supported_blend_modes &= !1;
    assert!(unsafe { decode_compositor_table(&value.base.base.base) }.is_err());
}

struct LegacyRenderer;
impl Renderer for LegacyRenderer {
    fn render(
        &self,
        _: &[abi::CompositeNode],
        _: &[abi::CompositeLayer],
        _: u32,
        _: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        unsafe {
            *output.data = 0xff123456;
        }
        Ok(())
    }
}
fn output(data: &mut [u32], width: u32, height: u32) -> abi::Pixels {
    abi::Pixels {
        data: data.as_mut_ptr(),
        len: data.len(),
        width,
        height,
        stride: width,
        origin_x: 0,
        origin_y: 0,
    }
}
fn node(kind: u32, a: u32, b: u32, mode: u32, amount: f32) -> abi::CompositeNode {
    abi::CompositeNode {
        kind,
        a,
        b,
        mode,
        amount,
    }
}
#[test]
fn legacy_renderer_retains_unmasked_dispatch_and_rejects_mask_input_before_writes() {
    let renderer = LegacyRenderer;
    assert!(!renderer.supports_masks());
    let mut data = [0x11223344];
    renderer
        .render_masked(&[], &[], &[], 1, 1, &mut output(&mut data, 1, 1))
        .unwrap();
    assert_eq!(data, [0xff123456]);
    let masks = [abi::CompositeMask {
        data: std::ptr::null(),
        len: 0,
        stride: 0,
        constant: 255,
    }];
    for (nodes, masks) in [
        (&[][..], &masks[..]),
        (&[node(6, 0, 0, 0, 0.)][..], &[][..]),
    ] {
        data[0] = 0x11223344;
        assert!(renderer
            .render_masked(nodes, &[], masks, 1, 1, &mut output(&mut data, 1, 1))
            .unwrap_err()
            .contains("mask coverage"));
        assert_eq!(data, [0x11223344]);
    }
}

#[test]
fn loaded_masked_compositor_uses_typed_coverage_and_preserves_the_legacy_callback() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins/compositing");
    let (plugin, _) = NativePlugin::load(&root, "micopi_compositing.dll", "compositing").unwrap();
    assert!(
        plugin.supports_masks(),
        "Stage the matching compositor DLL before this check"
    );
    let raw = [0x803399cc, 0xff102030, 0x00776655, 0xc0abcdef];
    let layers = [abi::CompositeLayer {
        data: raw.as_ptr(),
        len: raw.len(),
        opacity: 255,
        visible: 1,
        mode: 0,
    }];
    let coverage = [255u8, 0, 0, 128];
    let masks = [abi::CompositeMask {
        data: coverage.as_ptr(),
        len: coverage.len(),
        stride: 4,
        constant: 0,
    }];
    let nodes = [
        node(0, 0, 0, 0, 0.),
        node(6, 0, 0, 0, 0.),
        node(7, 0, 1, 0, 1.),
        node(3, 2, 0, 0, 0.),
    ];
    let mut data = [0x11223344; 4];
    plugin
        .render_masked(&nodes, &layers, &masks, 4, 1, &mut output(&mut data, 4, 1))
        .unwrap();
    assert_eq!(data, [0x803399cc, 0, 0, 0x60abcdef]);
    let mut old = [0; 4];
    let mut fallback = [0; 4];
    plugin
        .render(&[], &layers, 4, 1, &mut output(&mut old, 4, 1))
        .unwrap();
    plugin
        .render_masked(&[], &layers, &[], 4, 1, &mut output(&mut fallback, 4, 1))
        .unwrap();
    assert_eq!(old, fallback);
    data.fill(0x11223344);
    assert!(plugin
        .render(&nodes, &layers, 4, 1, &mut output(&mut data, 4, 1))
        .is_err());
    assert_eq!(data, [0x11223344; 4]);
}
