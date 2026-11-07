//! Startup-only negotiation for optional HSL plan callbacks.
use super::*;
type Callbacks = (abi::HslRenderFn, abi::HslHistogramFn);
unsafe fn decode(pointer: *const abi::HslApi) -> Result<Callbacks, String> {
    if pointer.is_null() {
        return Err("Null HSL API table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::HslApi>() as u32 {
        return Err("Incompatible or incomplete HSL API table".into());
    }
    let table = std::ptr::read_unaligned(pointer);
    Ok((
        table.render.ok_or("Missing HSL render callback")?,
        table.histogram.ok_or("Missing HSL histogram callback")?,
    ))
}
impl Module {
    pub(super) unsafe fn hsl_api(&self) -> Result<Option<Callbacks>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::HSL_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::HslEntry>(address);
            Ok(Some(decode(entry())?))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
impl NativePlugin {
    pub(crate) fn supports_hsl(&self) -> bool {
        self.hsl.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn render(
        _: *const abi::CompositeNode,
        _: usize,
        _: *const abi::CompositeLayer,
        _: usize,
        _: *const abi::CompositeMask,
        _: usize,
        _: *const abi::CompositeLut,
        _: usize,
        _: *const abi::HslParameters,
        _: usize,
        _: u32,
        _: u32,
        _: *mut abi::Pixels,
    ) -> i32 {
        abi::ERROR
    }
    unsafe extern "C" fn histogram(
        _: *const abi::HistogramRequest,
        _: *const abi::CompositeNode,
        _: usize,
        _: *const abi::CompositeLayer,
        _: usize,
        _: *const abi::CompositeMask,
        _: usize,
        _: *const abi::CompositeLut,
        _: usize,
        _: *const abi::HslParameters,
        _: usize,
        _: *mut abi::HistogramResult,
    ) -> i32 {
        abi::ERROR
    }
    #[test]
    fn complete_hsl_callbacks_require_version_size_and_both_functions() {
        let mut table = abi::HslApi {
            abi_version: 1,
            struct_size: 24,
            render: Some(render),
            histogram: Some(histogram),
        };
        assert!(unsafe { decode(&table) }.is_ok());
        for size in [0, 8, 16, 23] {
            table.struct_size = size;
            assert!(unsafe { decode(&table) }.is_err());
        }
        table.struct_size = 24;
        table.abi_version = 2;
        assert!(unsafe { decode(&table) }.is_err());
        table.abi_version = 1;
        table.render = None;
        assert!(unsafe { decode(&table) }.is_err());
        table.render = Some(render);
        table.histogram = None;
        assert!(unsafe { decode(&table) }.is_err());
        assert!(unsafe { decode(std::ptr::null()) }.is_err());
        let header = abi::Header {
            abi_version: 1,
            struct_size: 8,
        };
        assert!(unsafe { decode((&header as *const abi::Header).cast()) }.is_err());
    }
    #[test]
    fn future_hsl_tables_preserve_known_prefix() {
        #[repr(C)]
        struct Future {
            base: abi::HslApi,
            tail: [u8; 24],
        }
        let table = Future {
            base: abi::HslApi {
                abi_version: 1,
                struct_size: 48,
                render: Some(render),
                histogram: Some(histogram),
            },
            tail: [93; 24],
        };
        assert!(unsafe { decode(&table.base) }.is_ok());
        assert_eq!(table.tail, [93; 24]);
    }
    #[cfg(windows)]
    #[test]
    fn live_dll_caches_hsl_callbacks_and_keeps_other_plugin_absence_compatible() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let plugin = &registry.native_plugins["compositing"];
        assert!(plugin.supports_hsl());
        assert!(!registry.native_plugins["native_paint"].supports_hsl());
        let callbacks = plugin.hsl.unwrap();
        let pixel = 0x80ff0000;
        let layer = abi::CompositeLayer {
            data: &pixel,
            len: 1,
            opacity: 255,
            visible: 1,
            mode: 0,
        };
        let nodes = [
            abi::CompositeNode {
                kind: 0,
                a: 0,
                b: 0,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 11,
                a: 0,
                b: 0,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 3,
                a: 1,
                b: 0,
                mode: 0,
                amount: 1.,
            },
        ];
        let parameters = abi::HslParameters {
            hue: 120.,
            saturation: 0.,
            lightness: 0.,
        };
        let mut pixel_out = 0;
        let mut output = abi::Pixels {
            data: &mut pixel_out,
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe {
                callbacks.0(
                    nodes.as_ptr(),
                    3,
                    &layer,
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    &parameters,
                    1,
                    1,
                    1,
                    &mut output,
                )
            },
            abi::OK
        );
        assert_eq!(pixel_out, 0x8000ff00);
        let request = abi::HistogramRequest {
            abi_version: 1,
            struct_size: 24,
            width: 1,
            height: 1,
            stride: 1,
            max_samples: 1,
        };
        let mut result = abi::HistogramResult::default();
        assert_eq!(
            unsafe {
                callbacks.1(
                    &request,
                    nodes.as_ptr(),
                    3,
                    &layer,
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    &parameters,
                    1,
                    &mut result,
                )
            },
            abi::OK
        );
        assert_eq!(result.bins[1][255], 1);
        assert_eq!(pixel, 0x80ff0000);
        assert_eq!(plugin.hsl.unwrap().0 as usize, callbacks.0 as usize);
    }
}
