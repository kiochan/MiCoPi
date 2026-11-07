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
    pub(crate) fn render_hsl_plan(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        hsl: &[abi::HslParameters],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        validate_parameters(hsl)?;
        if nodes.len() > 64
            || !(1..=16).contains(&layers.len())
            || masks.len() > 32
            || luts.len() > 64
            || !(1..=4096).contains(&width)
            || !(1..=4096).contains(&height)
        {
            return Err("Invalid HSL plan bounds".into());
        }
        if layers
            .iter()
            .any(|l| !<Self as crate::engine::graph::Renderer>::supports_blend_mode(self, l.mode))
            || nodes.iter().any(|n| {
                n.kind == 2
                    && !<Self as crate::engine::graph::Renderer>::supports_blend_mode(self, n.mode)
            })
        {
            return Err("The compositor does not support this blend mode".into());
        }
        let callback = self.hsl.ok_or("Native HSL rendering is unavailable")?.0;
        let status = unsafe {
            callback(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                layers.len(),
                masks.as_ptr(),
                masks.len(),
                luts.as_ptr(),
                luts.len(),
                hsl.as_ptr(),
                hsl.len(),
                width,
                height,
                output,
            )
        };
        if status == abi::OK {
            Ok(())
        } else {
            Err("Native HSL rendering failed".into())
        }
    }
    pub(crate) fn histogram_hsl_plan(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        hsl: &[abi::HslParameters],
        mut request: abi::HistogramRequest,
    ) -> Result<abi::HistogramResult, String> {
        validate_parameters(hsl)?;
        if !(1..=4096).contains(&request.width)
            || !(1..=4096).contains(&request.height)
            || request.stride != request.width
            || !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&request.max_samples)
            || !(1..=64).contains(&nodes.len())
            || !(1..=16).contains(&layers.len())
            || masks.len() > 32
            || luts.len() > 64
            || layers.iter().any(|l| {
                l.data.is_null() || l.len < (request.width as usize * request.height as usize)
            })
        {
            return Err("Invalid HSL histogram bounds or loans".into());
        }
        let callback = self
            .hsl
            .ok_or("Native HSL histogram sampling is unavailable")?
            .1;
        request.abi_version = 1;
        request.struct_size = std::mem::size_of::<abi::HistogramRequest>() as u32;
        let mut result = abi::HistogramResult::default();
        let status = unsafe {
            callback(
                &request,
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                layers.len(),
                masks.as_ptr(),
                masks.len(),
                luts.as_ptr(),
                luts.len(),
                hsl.as_ptr(),
                hsl.len(),
                &mut result,
            )
        };
        if status != abi::OK {
            return Err("Native HSL histogram sampling failed".into());
        }
        super::histogram::validate_result(result, &request).map_err(|e| e.to_string())
    }
}

fn validate_parameters(parameters: &[abi::HslParameters]) -> Result<(), String> {
    if parameters.len() > abi::HSL_MAX_PARAMETERS {
        return Err("HSL parameter limit exceeded".into());
    }
    for p in parameters {
        crate::engine::hsl::Hsl {
            hue: p.hue,
            saturation: p.saturation,
            lightness: p.lightness,
        }
        .validate()?;
    }
    Ok(())
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
