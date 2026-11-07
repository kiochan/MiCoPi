//! Cold optional Levels preparation; never called by typed pointer MOVE.
use super::*;

unsafe fn decode(pointer: *const abi::LevelsApi) -> Result<abi::LevelsPrepareFn, String> {
    if pointer.is_null() {
        return Err("Null Levels API table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::LevelsApi>() as u32
    {
        return Err("Incompatible Levels preparation ABI".into());
    }
    std::ptr::read_unaligned(pointer)
        .prepare
        .ok_or_else(|| "Missing Levels preparation callback".into())
}
impl Module {
    pub(super) unsafe fn levels_api(&self) -> Result<Option<abi::LevelsPrepareFn>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::LEVELS_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::LevelsEntry>(address);
            decode(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
fn prepare(
    callback: Option<abi::LevelsPrepareFn>,
    mut p: abi::LevelsParameters,
) -> Result<[u8; 256], ActionError> {
    if [p.input_black, p.input_white, p.output_black, p.output_white]
        .iter()
        .any(|v| *v > 255)
        || p.input_black >= p.input_white
        || !p.gamma.is_finite()
        || !(0.1..=10.).contains(&p.gamma)
    {
        return Err(ActionError::new(
            "levels_parameters",
            "Invalid Levels endpoints or gamma",
        ));
    }
    let callback = callback.ok_or_else(|| {
        ActionError::new(
            "levels_unavailable",
            "Native Levels preparation is unavailable",
        )
    })?;
    p.abi_version = 1;
    p.struct_size = std::mem::size_of::<abi::LevelsParameters>() as u32;
    let mut output = [0; 256];
    if unsafe { callback(&p, output.as_mut_ptr(), output.len()) } != abi::OK {
        return Err(ActionError::new(
            "levels_prepare",
            "Native Levels preparation failed",
        ));
    }
    Ok(output)
}
impl NativePlugin {
    pub(crate) fn supports_levels_preparation(&self) -> bool {
        self.levels_prepare.is_some()
    }
    pub(crate) fn prepare_levels(
        &self,
        parameters: abi::LevelsParameters,
    ) -> Result<[u8; 256], ActionError> {
        prepare(self.levels_prepare, parameters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parameters() -> abi::LevelsParameters {
        abi::LevelsParameters {
            abi_version: 0,
            struct_size: 0,
            input_black: 0,
            input_white: 255,
            output_black: 0,
            output_white: 255,
            gamma: 1.,
        }
    }
    unsafe extern "C" fn identity(
        p: *const abi::LevelsParameters,
        out: *mut u8,
        len: usize,
    ) -> i32 {
        assert_eq!((*p).abi_version, 1);
        assert_eq!((*p).struct_size, 32);
        assert_eq!(len, 256);
        for i in 0..256 {
            *out.add(i) = i as u8;
        }
        abi::OK
    }
    unsafe extern "C" fn failure(_: *const abi::LevelsParameters, out: *mut u8, _: usize) -> i32 {
        *out = 77;
        abi::ERROR
    }
    #[test]
    fn levels_table_decoder_rejects_null_short_version_and_missing_callback() {
        unsafe {
            assert!(decode(std::ptr::null()).is_err());
            let short = abi::Header {
                abi_version: 1,
                struct_size: 8,
            };
            assert!(decode((&short as *const abi::Header).cast()).is_err());
            for (version, callback) in [(2, Some(identity as abi::LevelsPrepareFn)), (1, None)] {
                let table = abi::LevelsApi {
                    abi_version: version,
                    struct_size: 16,
                    prepare: callback,
                };
                assert!(decode(&table).is_err());
            }
            let table = abi::LevelsApi {
                abi_version: 1,
                struct_size: 16,
                prepare: Some(identity),
            };
            assert!(decode(&table).is_ok());
        }
    }
    #[test]
    fn levels_adapter_validates_before_callback_and_never_publishes_failed_output() {
        assert_eq!(
            prepare(None, parameters()).unwrap_err().code,
            "levels_unavailable"
        );
        let mut invalid = parameters();
        invalid.input_white = 256;
        assert_eq!(
            prepare(Some(identity), invalid).unwrap_err().code,
            "levels_parameters"
        );
        assert_eq!(
            prepare(Some(failure), parameters()).unwrap_err().code,
            "levels_prepare"
        );
        assert_eq!(
            prepare(Some(identity), parameters()).unwrap(),
            std::array::from_fn(|i| i as u8)
        );
    }
    #[test]
    fn bundled_levels_preparation_is_cached_and_independent_of_legacy_tools() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let compositor = &registry.native_plugins["compositing"];
        assert!(compositor.supports_levels_preparation());
        assert!(!registry.native_plugins["painting"].supports_levels_preparation());
        assert_eq!(
            compositor.prepare_levels(parameters()).unwrap(),
            std::array::from_fn(|i| i as u8)
        );
        let mut p = parameters();
        p.gamma = 2.;
        assert_eq!(compositor.prepare_levels(p).unwrap()[64], 128);
        assert_eq!(
            registry.describe()["native"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "compositing")
                .unwrap()["filters"]["levels_prepare"],
            true
        );
    }
}
