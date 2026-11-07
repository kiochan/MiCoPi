//! Optional cold Curves preparation. No typed contact or document integration.
use super::*;

unsafe fn decode(pointer: *const abi::CurvesApi) -> Result<abi::CurvesPrepareFn, String> {
    if pointer.is_null() {
        return Err("Null Curves API table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::CurvesApi>() as u32
    {
        return Err("Incompatible Curves preparation ABI".into());
    }
    std::ptr::read_unaligned(pointer)
        .prepare
        .ok_or_else(|| "Missing Curves preparation callback".into())
}
impl Module {
    pub(super) unsafe fn curves_api(&self) -> Result<Option<abi::CurvesPrepareFn>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::CURVES_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::CurvesEntry>(address);
            decode(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
fn prepare(
    callback: Option<abi::CurvesPrepareFn>,
    mut parameters: abi::CurvesParameters,
) -> Result<[u8; 256], ActionError> {
    let count = parameters.count as usize;
    if !(2..=abi::CURVES_MAX_POINTS).contains(&count)
        || !matches!(
            parameters.interpolation,
            abi::CURVES_LINEAR | abi::CURVES_SMOOTH
        )
        || parameters.points[..count]
            .iter()
            .any(|p| p.input > 255 || p.output > 255)
        || parameters.points[..count]
            .windows(2)
            .any(|p| p[0].input >= p[1].input)
    {
        return Err(ActionError::new(
            "curves_parameters",
            "Invalid Curves points or interpolation",
        ));
    }
    let callback = callback.ok_or_else(|| {
        ActionError::new(
            "curves_unavailable",
            "Native Curves preparation is unavailable",
        )
    })?;
    parameters.abi_version = 1;
    parameters.struct_size = std::mem::size_of::<abi::CurvesParameters>() as u32;
    let mut output = [0; 256];
    if unsafe { callback(&parameters, output.as_mut_ptr(), output.len()) } != abi::OK {
        return Err(ActionError::new(
            "curves_prepare",
            "Native Curves preparation failed",
        ));
    }
    Ok(output)
}
impl NativePlugin {
    pub(crate) fn supports_curves_preparation(&self) -> bool {
        self.curves_prepare.is_some()
    }
    #[allow(dead_code)] // The next source-node increment consumes this cold adapter.
    pub(crate) fn prepare_curves(
        &self,
        parameters: abi::CurvesParameters,
    ) -> Result<[u8; 256], ActionError> {
        prepare(self.curves_prepare, parameters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parameters() -> abi::CurvesParameters {
        let mut value = abi::CurvesParameters {
            abi_version: 0, // The host supplies normalized ABI headers.
            struct_size: 0,
            count: 2,
            interpolation: abi::CURVES_SMOOTH,
            points: [abi::CurvePoint::default(); abi::CURVES_MAX_POINTS],
        };
        value.points[1] = abi::CurvePoint {
            input: 255,
            output: 255,
        };
        value
    }
    unsafe extern "C" fn identity(
        p: *const abi::CurvesParameters,
        out: *mut u8,
        len: usize,
    ) -> i32 {
        assert_eq!((*p).abi_version, 1);
        assert_eq!(
            (*p).struct_size as usize,
            std::mem::size_of::<abi::CurvesParameters>()
        );
        assert_eq!(len, 256);
        for i in 0..256 {
            *out.add(i) = i as u8;
        }
        abi::OK
    }
    unsafe extern "C" fn failure(_: *const abi::CurvesParameters, out: *mut u8, _: usize) -> i32 {
        *out = 99;
        abi::ERROR
    }
    #[test]
    fn decoder_admits_complete_and_future_tables_rejects_missing_or_short() {
        unsafe {
            assert!(decode(std::ptr::null()).is_err());
            let short = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert!(decode((&short as *const abi::Header).cast()).is_err());
            for size in [16, 32] {
                let table = abi::CurvesApi {
                    abi_version: 1,
                    struct_size: size,
                    prepare: Some(identity),
                };
                assert!(decode(&table).is_ok());
            }
            for (version, callback) in [(2, Some(identity as abi::CurvesPrepareFn)), (1, None)] {
                let table = abi::CurvesApi {
                    abi_version: version,
                    struct_size: 16,
                    prepare: callback,
                };
                assert!(decode(&table).is_err());
            }
        }
    }
    #[test]
    fn adapter_validates_before_callback_and_keeps_failed_output_private() {
        assert_eq!(
            prepare(None, parameters()).unwrap_err().code,
            "curves_unavailable"
        );
        for case in 0..6 {
            let mut p = parameters();
            match case {
                0 => p.count = 1,
                1 => p.count = u32::MAX,
                2 => p.interpolation = 3,
                3 => p.points[1].input = 0,
                4 => p.points[1].input = 256,
                _ => p.points[1].output = 256,
            }
            assert_eq!(
                prepare(Some(identity), p).unwrap_err().code,
                "curves_parameters"
            );
        }
        assert_eq!(
            prepare(Some(failure), parameters()).unwrap_err().code,
            "curves_prepare"
        );
        assert_eq!(
            prepare(Some(identity), parameters()).unwrap(),
            std::array::from_fn(|i| i as u8)
        );
    }
    #[test]
    fn bundled_curves_are_cached_and_inventory_does_not_claim_document_support() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let compositor = &registry.native_plugins["compositing"];
        assert!(compositor.supports_curves_preparation());
        assert!(!registry.native_plugins["painting"].supports_curves_preparation());
        assert_eq!(
            compositor.prepare_curves(parameters()).unwrap(),
            std::array::from_fn(|i| i as u8)
        );
        let description = registry.describe();
        let item = description["native"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "compositing")
            .unwrap();
        assert_eq!(item["filters"]["curves_prepare"], true);
        assert!(!registry.nodes.contains_key("compositing.curves"));
    }
}
