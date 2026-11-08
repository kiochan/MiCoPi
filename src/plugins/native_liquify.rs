//! Startup negotiation and a single-owner cached deformation handle.
use super::*;

unsafe fn decode(pointer: *const abi::LiquifyApi) -> Result<abi::LiquifyApi, String> {
    if pointer.is_null() || (pointer as usize) % std::mem::align_of::<abi::LiquifyApi>() != 0 {
        return Err("Invalid Liquify API pointer".into());
    }
    // Inspect the header before reading the optional table's complete prefix.
    let version = std::ptr::read(pointer.cast::<u32>());
    let size = std::ptr::read(pointer.cast::<u32>().add(1));
    if version != 1 || size < std::mem::size_of::<abi::LiquifyApi>() as u32 {
        return Err("Unsupported Liquify API prefix".into());
    }
    let api = std::ptr::read(pointer);
    if api.create.is_none()
        || api.destroy.is_none()
        || api.clear.is_none()
        || api.push.is_none()
        || api.pixels.is_none()
        || api.mask.is_none()
    {
        return Err("Incomplete Liquify API callbacks".into());
    }
    Ok(api)
}
impl Module {
    pub(super) unsafe fn liquify_api(&self) -> Result<Option<abi::LiquifyApi>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::LIQUIFY_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::LiquifyEntry>(address);
            Ok(Some(decode(entry())?))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
}
pub(crate) struct NativeLiquify {
    // Keep the DLL loaded until after destroy. No source images are retained.
    _plugin: Arc<NativePlugin>,
    api: abi::LiquifyApi,
    state: usize,
    width: u32,
    height: u32,
}
impl Drop for NativeLiquify {
    fn drop(&mut self) {
        unsafe {
            (self.api.destroy.unwrap())(self.state as *mut c_void);
        }
    }
}
impl NativePlugin {
    #[cfg(test)]
    pub(crate) fn replace_liquify_for_test(
        &mut self,
        api: Option<abi::LiquifyApi>,
    ) -> Option<abi::LiquifyApi> {
        std::mem::replace(&mut self.liquify, api)
    }
    pub(crate) fn supports_liquify(&self) -> bool {
        self.liquify.is_some()
    }
    pub(crate) fn create_liquify(
        self: &Arc<Self>,
        width: u32,
        height: u32,
        spacing: u32,
        budget: u64,
    ) -> Result<NativeLiquify, String> {
        let api = self.liquify.ok_or("Native Liquify is unavailable")?;
        let config = abi::LiquifyConfig {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::LiquifyConfig>() as u32,
            width,
            height,
            spacing,
            reserved: 0,
            budget_bytes: budget,
        };
        let state = unsafe { (api.create.unwrap())(&config) };
        if state.is_null() {
            return Err("Cannot create Liquify field within the admitted budget".into());
        }
        Ok(NativeLiquify {
            _plugin: Arc::clone(self),
            api,
            state: state as usize,
            width,
            height,
        })
    }
}
impl NativeLiquify {
    pub(crate) fn pixels_frame_region(
        &self,
        source: &[u32],
        method: u32,
        region: abi::Region,
        output: &mut [u32],
    ) -> Result<(), String> {
        let area = self.width as usize * self.height as usize;
        if source.len() != area
            || output.len() != area
            || region.width == 0
            || region.height == 0
            || region
                .x
                .checked_add(region.width)
                .is_none_or(|n| n > self.width)
            || region
                .y
                .checked_add(region.height)
                .is_none_or(|n| n > self.height)
        {
            return Err("Invalid Liquify frame region".into());
        }
        let offset = region.y as usize * self.width as usize + region.x as usize;
        let src = abi::Pixels {
            data: source.as_ptr().cast_mut(),
            len: area,
            width: self.width,
            height: self.height,
            stride: self.width,
            origin_x: 0,
            origin_y: 0,
        };
        let mut dst = abi::Pixels {
            data: output[offset..].as_mut_ptr(),
            len: area - offset,
            width: region.width,
            height: region.height,
            stride: self.width,
            origin_x: region.x,
            origin_y: region.y,
        };
        if unsafe {
            (self.api.pixels.unwrap())(self.state as *const c_void, method, &src, &mut dst)
        } != abi::OK
        {
            return Err("Native Liquify region sampling failed".into());
        }
        Ok(())
    }
    pub(crate) fn mask_frame_region(
        &self,
        source: crate::engine::raster_masks::PlaneView<'_>,
        method: u32,
        region: abi::Region,
        output: &mut [u8],
    ) -> Result<(), String> {
        use crate::engine::raster_masks::PlaneView;
        let area = self.width as usize * self.height as usize;
        if output.len() != area
            || region.width == 0
            || region.height == 0
            || region
                .x
                .checked_add(region.width)
                .is_none_or(|n| n > self.width)
            || region
                .y
                .checked_add(region.height)
                .is_none_or(|n| n > self.height)
        {
            return Err("Invalid Liquify mask region".into());
        }
        let src = match source {
            PlaneView::Uniform(v) => abi::CompositeMask {
                data: std::ptr::null(),
                len: 0,
                stride: 0,
                constant: u32::from(v),
            },
            PlaneView::Dense(v) => {
                if v.len() != area {
                    return Err("Invalid Liquify mask source extent".into());
                }
                abi::CompositeMask {
                    data: v.as_ptr(),
                    len: v.len(),
                    stride: self.width,
                    constant: 0,
                }
            }
        };
        let offset = region.y as usize * self.width as usize + region.x as usize;
        let mut dst = abi::A8Pixels {
            data: output[offset..].as_mut_ptr(),
            len: area - offset,
            width: region.width,
            height: region.height,
            stride: self.width,
            origin_x: region.x,
            origin_y: region.y,
        };
        if unsafe { (self.api.mask.unwrap())(self.state as *const c_void, method, &src, &mut dst) }
            != abi::OK
        {
            return Err("Native Liquify mask region sampling failed".into());
        }
        Ok(())
    }
    pub(crate) fn clear(&mut self) -> Result<(), String> {
        if unsafe { (self.api.clear.unwrap())(self.state as *mut c_void) } != abi::OK {
            return Err("Cannot clear Liquify field".into());
        }
        Ok(())
    }
    pub(crate) fn push(
        &mut self,
        pushes: &[abi::LiquifyPush],
    ) -> Result<abi::LiquifyUpdate, String> {
        if !(1..=4).contains(&pushes.len()) {
            return Err("Liquify requires one through four joint arms".into());
        }
        let mut update = abi::LiquifyUpdate::default();
        if unsafe {
            (self.api.push.unwrap())(
                self.state as *mut c_void,
                pushes.as_ptr(),
                pushes.len(),
                &mut update,
            )
        } != abi::OK
        {
            return Err("Native Liquify push failed".into());
        }
        if update.reserved != 0
            || update.count > 4
            || update.regions[..update.count as usize].iter().any(|r| {
                r.width == 0
                    || r.height == 0
                    || r.x.checked_add(r.width).is_none_or(|v| v > self.width)
                    || r.y.checked_add(r.height).is_none_or(|v| v > self.height)
            })
        {
            return Err("Native Liquify returned invalid dirty regions".into());
        }
        Ok(update)
    }
    pub(crate) fn pixels(
        &self,
        source: &[u32],
        method: u32,
        region: abi::Region,
        output: &mut [u32],
    ) -> Result<(), String> {
        if source.len() != self.width as usize * self.height as usize
            || output.len() != region.width as usize * region.height as usize
        {
            return Err("Liquify pixel storage does not match its extent".into());
        }
        let src = abi::Pixels {
            data: source.as_ptr().cast_mut(),
            len: source.len(),
            width: self.width,
            height: self.height,
            stride: self.width,
            origin_x: 0,
            origin_y: 0,
        };
        let mut dst = abi::Pixels {
            data: output.as_mut_ptr(),
            len: output.len(),
            width: region.width,
            height: region.height,
            stride: region.width,
            origin_x: region.x,
            origin_y: region.y,
        };
        if unsafe {
            (self.api.pixels.unwrap())(self.state as *const c_void, method, &src, &mut dst)
        } != abi::OK
        {
            return Err("Native Liquify pixel sampling failed".into());
        }
        Ok(())
    }
    pub(crate) fn mask(
        &self,
        source: &abi::CompositeMask,
        method: u32,
        region: abi::Region,
        output: &mut [u8],
    ) -> Result<(), String> {
        if output.len() != region.width as usize * region.height as usize {
            return Err("Liquify mask storage does not match its extent".into());
        }
        let mut dst = abi::A8Pixels {
            data: output.as_mut_ptr(),
            len: output.len(),
            width: region.width,
            height: region.height,
            stride: region.width,
            origin_x: region.x,
            origin_y: region.y,
        };
        if unsafe {
            (self.api.mask.unwrap())(self.state as *const c_void, method, source, &mut dst)
        } != abi::OK
        {
            return Err("Native Liquify mask sampling failed".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn optional_liquify_negotiation_and_cached_lifecycle_reject_incomplete_tables() {
        let registry = crate::plugins::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let plugin = &registry.native_plugins["painting"];
        assert!(plugin.supports_liquify());
        assert!(!registry.native_plugins["compositing"].supports_liquify());
        let mut api = plugin.liquify.unwrap();
        assert!(unsafe { decode(&api) }.is_ok());
        for size in [0, 8, 48, 55] {
            api.struct_size = size;
            assert!(unsafe { decode(&api) }.is_err());
        }
        api = plugin.liquify.unwrap();
        api.abi_version = 2;
        assert!(unsafe { decode(&api) }.is_err());
        for i in 0..6 {
            api = plugin.liquify.unwrap();
            match i {
                0 => api.create = None,
                1 => api.destroy = None,
                2 => api.clear = None,
                3 => api.push = None,
                4 => api.pixels = None,
                _ => api.mask = None,
            }
            assert!(unsafe { decode(&api) }.is_err());
        }
        let mut field = plugin.create_liquify(8, 6, 1, 1024 * 1024).unwrap();
        let original = vec![0x00332211; 48];
        let mut out = vec![0; 48];
        let roi = abi::Region {
            x: 0,
            y: 0,
            width: 8,
            height: 6,
        };
        field.pixels(&original, 1, roi, &mut out).unwrap();
        assert_eq!(out, original);
        field
            .push(&[abi::LiquifyPush {
                x: 4.,
                y: 3.,
                dx: 1.,
                dy: 0.,
                radius: 4.,
                strength: 1.,
            }])
            .unwrap();
        field.clear().unwrap();
        field.pixels(&original, 0, roi, &mut out).unwrap();
        assert_eq!(out, original);
        let source = abi::CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: 91,
        };
        let mut bytes = vec![0; 48];
        field.mask(&source, 1, roi, &mut bytes).unwrap();
        assert_eq!(bytes, vec![91; 48]);
        assert!(plugin.create_liquify(8, 6, 1, 1).is_err());
    }
}
