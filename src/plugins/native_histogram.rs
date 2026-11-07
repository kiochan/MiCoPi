//! Optional read-only histogram boundary. No state or per-contact discovery.
use super::*;
unsafe fn decode(pointer: *const abi::HistogramApi) -> Result<abi::HistogramFn, String> {
    if pointer.is_null() {
        return Err("Null histogram API table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::HistogramApi>() as u32
    {
        return Err("Incompatible histogram ABI".into());
    }
    std::ptr::read_unaligned(pointer)
        .sample
        .ok_or_else(|| "Missing histogram callback".into())
}
impl Module {
    pub(super) unsafe fn histogram_api(&self) -> Result<Option<abi::HistogramFn>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::HISTOGRAM_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::HistogramEntry>(address);
            decode(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
fn sample(
    callback: Option<abi::HistogramFn>,
    pixels: &[u32],
    mut p: abi::HistogramRequest,
) -> Result<abi::HistogramResult, ActionError> {
    let invalid = || {
        ActionError::new(
            "histogram_parameters",
            "Invalid histogram dimensions, stride or sample budget",
        )
    };
    if !(1..=4096).contains(&p.width)
        || !(1..=4096).contains(&p.height)
        || p.stride < p.width
        || !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&p.max_samples)
        || (p.height as usize - 1)
            .checked_mul(p.stride as usize)
            .and_then(|n| n.checked_add(p.width as usize))
            .is_none_or(|n| n > pixels.len())
    {
        return Err(invalid());
    }
    let callback = callback.ok_or_else(|| {
        ActionError::new(
            "histogram_unavailable",
            "Native histogram sampling is unavailable",
        )
    })?;
    p.abi_version = 1;
    p.struct_size = std::mem::size_of::<abi::HistogramRequest>() as u32;
    let mut output = abi::HistogramResult::default();
    if unsafe { callback(&p, pixels.as_ptr(), pixels.len(), &mut output) } != abi::OK {
        return Err(ActionError::new(
            "histogram_sample",
            "Native histogram sampling failed",
        ));
    }
    let count = output.nontransparent_pixels;
    let area = u64::from(p.width) * u64::from(p.height);
    if output.abi_version != 1
        || output.struct_size != std::mem::size_of::<abi::HistogramResult>() as u32
        || output.grid_width == 0
        || output.grid_height == 0
        || output.grid_width > p.width
        || output.grid_height > p.height
        || output.sampled_pixels != u64::from(output.grid_width) * u64::from(output.grid_height)
        || output.sampled_pixels > u64::from(p.max_samples)
        || count > output.sampled_pixels
        || area <= u64::from(p.max_samples)
            && (output.grid_width != p.width || output.grid_height != p.height)
        || output.bins[..3]
            .iter()
            .any(|bins| bins.iter().any(|n| *n > count) || bins.iter().sum::<u64>() != count)
        || (0..256)
            .any(|i| output.bins[3][i] != output.bins[0][i] + output.bins[1][i] + output.bins[2][i])
    {
        return Err(ActionError::new(
            "histogram_result",
            "Invalid native histogram counts",
        ));
    }
    Ok(output)
}
impl NativePlugin {
    pub(crate) fn supports_histogram(&self) -> bool {
        self.histogram.is_some()
    }
    pub(crate) fn histogram(
        &self,
        pixels: &[u32],
        request: abi::HistogramRequest,
    ) -> Result<abi::HistogramResult, ActionError> {
        sample(self.histogram, pixels, request)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> abi::HistogramRequest {
        abi::HistogramRequest {
            abi_version: 0,
            struct_size: 0,
            width: 1,
            height: 1,
            stride: 1,
            max_samples: 1,
        }
    }
    unsafe extern "C" fn corrupt(
        _: *const abi::HistogramRequest,
        _: *const u32,
        _: usize,
        out: *mut abi::HistogramResult,
    ) -> i32 {
        (*out).grid_width = 1;
        (*out).grid_height = 1;
        (*out).sampled_pixels = 1;
        (*out).bins[0][0] = u64::MAX;
        abi::OK
    }
    unsafe extern "C" fn failed(
        _: *const abi::HistogramRequest,
        _: *const u32,
        _: usize,
        out: *mut abi::HistogramResult,
    ) -> i32 {
        (*out).sampled_pixels = 100;
        abi::ERROR
    }
    #[test]
    fn histogram_decoder_checks_headers_and_mandatory_callback() {
        unsafe {
            assert!(decode(std::ptr::null()).is_err());
            let header = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert!(decode((&header as *const abi::Header).cast()).is_err());
            let missing = abi::HistogramApi {
                abi_version: 1,
                struct_size: 16,
                sample: None,
            };
            assert!(decode(&missing).is_err());
            let wrong = abi::HistogramApi {
                abi_version: 2,
                struct_size: 16,
                sample: Some(failed),
            };
            assert!(decode(&wrong).is_err());
            let future = abi::HistogramApi {
                abi_version: 1,
                struct_size: 32,
                sample: Some(failed),
            };
            assert!(decode(&future).is_ok());
        }
    }
    #[test]
    fn histogram_adapter_preflights_and_never_publishes_failed_or_corrupt_output() {
        assert_eq!(
            sample(None, &[0], request()).err().unwrap().code,
            "histogram_unavailable"
        );
        let mut p = request();
        p.max_samples = 0;
        assert_eq!(
            sample(Some(corrupt), &[0], p).err().unwrap().code,
            "histogram_parameters"
        );
        assert_eq!(
            sample(Some(corrupt), &[], request()).err().unwrap().code,
            "histogram_parameters"
        );
        assert_eq!(
            sample(Some(corrupt), &[0], request()).err().unwrap().code,
            "histogram_result"
        );
        assert_eq!(
            sample(Some(failed), &[0], request()).err().unwrap().code,
            "histogram_sample"
        );
    }
    #[test]
    fn bundled_histogram_is_discovered_once_and_retains_partial_alpha_counts() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let compositor = &registry.native_plugins["compositing"];
        assert!(compositor.supports_histogram());
        assert!(!registry.native_plugins["painting"].supports_histogram());
        let output = compositor.histogram(&[0x01010203], request()).unwrap();
        assert_eq!(output.sampled_pixels, 1);
        assert_eq!(output.nontransparent_pixels, 1);
        assert_eq!(output.bins[0][1], 1);
        assert_eq!(output.bins[3][2], 1);
    }
}
