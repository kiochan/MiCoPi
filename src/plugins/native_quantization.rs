//! Startup-only optional quantization plan discovery; no document integration.
use super::*;
type Callbacks = (abi::HslRenderFn, abi::HslHistogramFn);
unsafe fn decode(pointer: *const abi::QuantizationApi) -> Result<Callbacks, String> {
    super::hsl::decode(pointer).map_err(|error| error.replace("HSL", "Quantization"))
}
impl Module {
    pub(super) unsafe fn quantization_api(&self) -> Result<Option<Callbacks>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(
                self.handle as *mut c_void,
                abi::QUANTIZATION_SYMBOL.as_ptr(),
            );
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::QuantizationEntry>(address);
            Ok(Some(decode(entry())?))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
impl NativePlugin {
    pub(crate) fn supports_quantization(&self) -> bool {
        self.quantization.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn optional_quantization_callbacks_are_cached_and_tables_reject_incomplete_prefixes() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let plugin = &registry.native_plugins["compositing"];
        assert!(plugin.supports_quantization());
        assert!(!registry.native_plugins["native_paint"].supports_quantization());
        let (render, histogram) = plugin.quantization.unwrap();
        let source = [0x80ff0000];
        let layers = [abi::CompositeLayer {
            data: source.as_ptr(),
            len: 1,
            opacity: 255,
            visible: 1,
            mode: 0,
        }];
        let nodes = [
            abi::CompositeNode {
                kind: 0,
                a: 0,
                b: 0,
                mode: 0,
                amount: 1.,
            },
            abi::CompositeNode {
                kind: 12,
                a: 0,
                b: 0,
                mode: 53,
                amount: 1.,
            },
        ];
        let mut pixel = 93;
        let mut target = abi::Pixels {
            data: &mut pixel,
            len: 1,
            width: 1,
            height: 1,
            stride: 1,
            origin_x: 0,
            origin_y: 0,
        };
        assert_eq!(
            unsafe {
                render(
                    nodes.as_ptr(),
                    2,
                    layers.as_ptr(),
                    1,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    0,
                    1,
                    1,
                    &mut target,
                )
            },
            abi::OK
        );
        assert_eq!(pixel, 0x80ffffff);
        assert_eq!(source, [0x80ff0000]);
        let mut table = abi::QuantizationApi {
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
        #[repr(C)]
        struct Future {
            base: abi::QuantizationApi,
            tail: [u8; 24],
        }
        let future = Future {
            base: abi::QuantizationApi {
                abi_version: 1,
                struct_size: 48,
                render: Some(render),
                histogram: Some(histogram),
            },
            tail: [93; 24],
        };
        assert!(unsafe { decode(&future.base) }.is_ok());
        assert_eq!(future.tail, [93; 24]);
    }
}
