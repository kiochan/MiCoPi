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
unsafe fn decode_plan(
    pointer: *const abi::HistogramApi,
) -> Result<Option<abi::HistogramPlanFn>, String> {
    decode(pointer)?;
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.struct_size == std::mem::size_of::<abi::HistogramApi>() as u32 {
        return Ok(None);
    }
    if header.struct_size < std::mem::size_of::<abi::HistogramApiPlan>() as u32 {
        return Err("Incomplete histogram plan tail".into());
    }
    std::ptr::read_unaligned(pointer.cast::<abi::HistogramApiPlan>())
        .sample_plan
        .map(Some)
        .ok_or_else(|| "Missing histogram plan callback".into())
}
impl Module {
    pub(super) unsafe fn histogram_api(
        &self,
    ) -> Result<Option<(abi::HistogramFn, Option<abi::HistogramPlanFn>)>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::HISTOGRAM_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::HistogramEntry>(address);
            let pointer = entry();
            Ok(Some((decode(pointer)?, decode_plan(pointer)?)))
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
    validate_result(output, &p)
}
pub(super) fn validate_result(
    output: abi::HistogramResult,
    p: &abi::HistogramRequest,
) -> Result<abi::HistogramResult, ActionError> {
    let count = output.nontransparent_pixels;
    let area = u64::from(p.width) * u64::from(p.height);
    let expected_grid = if area <= u64::from(p.max_samples) {
        (p.width, p.height)
    } else {
        let width = ((f64::from(p.max_samples) * f64::from(p.width) / f64::from(p.height))
            .sqrt()
            .floor() as u32)
            .clamp(1, p.width.min(p.max_samples));
        (width, p.height.min(p.max_samples / width).max(1))
    };
    if output.abi_version != 1
        || output.struct_size != std::mem::size_of::<abi::HistogramResult>() as u32
        || output.grid_width == 0
        || output.grid_height == 0
        || output.grid_width > p.width
        || output.grid_height > p.height
        || (output.grid_width, output.grid_height) != expected_grid
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
    pub(crate) fn supports_histogram_plan(&self) -> bool {
        self.histogram_plan.is_some() || self.hsl.is_some()
    }
    /// Cold read of the same immutable loans accepted by prepared rendering.
    pub(crate) fn histogram_plan(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        luts: &[abi::CompositeLut],
        request: abi::HistogramRequest,
    ) -> Result<abi::HistogramResult, ActionError> {
        if self.histogram_plan.is_none() && self.hsl.is_some() {
            return self
                .histogram_hsl_plan(nodes, layers, masks, luts, &[], request)
                .map_err(|e| ActionError::new("histogram_plan_sample", e));
        }
        sample_plan(self.histogram_plan, nodes, layers, masks, luts, request)
    }
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

fn sample_plan(
    callback: Option<abi::HistogramPlanFn>,
    nodes: &[abi::CompositeNode],
    layers: &[abi::CompositeLayer],
    masks: &[abi::CompositeMask],
    luts: &[abi::CompositeLut],
    mut request: abi::HistogramRequest,
) -> Result<abi::HistogramResult, ActionError> {
    if !(1..=4096).contains(&request.width)
        || !(1..=4096).contains(&request.height)
        || request.stride != request.width
        || !(1..=abi::HISTOGRAM_MAX_SAMPLES).contains(&request.max_samples)
        || !(1..=64).contains(&nodes.len())
        || !(1..=16).contains(&layers.len())
        || masks.len() > 32
        || luts.len() > 64
        || layers
            .iter()
            .any(|l| l.data.is_null() || l.len < (request.width as usize * request.height as usize))
    {
        return Err(ActionError::new(
            "histogram_parameters",
            "Invalid histogram plan bounds or loans",
        ));
    }
    let callback = callback.ok_or_else(|| {
        ActionError::new(
            "histogram_plan_unavailable",
            "Native histogram plan sampling is unavailable",
        )
    })?;
    request.abi_version = 1;
    request.struct_size = std::mem::size_of::<abi::HistogramRequest>() as u32;
    let mut result = abi::HistogramResult::default();
    if unsafe {
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
            &mut result,
        )
    } != abi::OK
    {
        return Err(ActionError::new(
            "histogram_plan_sample",
            "Native histogram plan sampling failed",
        ));
    }
    validate_result(result, &request)
}
#[cfg(test)]
#[path = "native_histogram_plan_tests.rs"]
mod plan_tests;
