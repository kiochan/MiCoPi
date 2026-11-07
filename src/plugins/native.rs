//! Versioned native-code plugin loader and synchronous host capabilities.
use super::PixelAccess;
use crate::application::{Action, ActionError, ActionResult, Session};
use micopi_plugin_api as abi;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    ffi::c_void,
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug)]
struct Module {
    handle: isize,
}
#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
impl Drop for Module {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            FreeLibrary(self.handle as *mut c_void);
        }
    }
}
type SelectionTable = (
    abi::SelectionApi,
    Option<abi::SelectionColorFn>,
    Option<abi::SelectionEdgesFn>,
    Option<abi::SelectionTransferFn>,
);
type CompositorTable = (
    abi::CompositorApi,
    u64,
    u64,
    Option<abi::CompositeRenderMaskedFn>,
);
type MaskEffectsTable = (
    abi::MaskEffectsCreateFn,
    abi::MaskEffectsDestroyFn,
    abi::MaskEffectRegionFn,
    abi::MaskEffectsReserveFn,
);
unsafe fn decode_mask_effects_table(
    pointer: *const abi::CompositorApi,
    capabilities: u64,
) -> Result<Option<MaskEffectsTable>, String> {
    if capabilities & abi::COMPOSITOR_MASK_EFFECTS == 0 {
        return Ok(None);
    }
    if pointer.is_null() {
        return Err("Compositor mask effects returned a null table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if capabilities & abi::COMPOSITOR_MASKS == 0
        || header.struct_size < std::mem::size_of::<abi::CompositorApiMaskEffects>() as u32
    {
        return Err("Compositor mask effects require masks and a complete effects table".into());
    }
    let effects = std::ptr::read_unaligned(pointer.cast::<abi::CompositorApiMaskEffects>());
    match (
        effects.create_mask_effects,
        effects.destroy_mask_effects,
        effects.mask_effect_region,
        effects.reserve_mask_effects,
    ) {
        (Some(create), Some(destroy), Some(region), Some(reserve)) => {
            Ok(Some((create, destroy, region, reserve)))
        }
        _ => Err(
            "Compositor mask effects are missing required lifecycle/reserve/region callbacks"
                .into(),
        ),
    }
}
// The v1 callback prefix remains usable when an optional feature tail is absent.
unsafe fn decode_compositor_table(
    pointer: *const abi::CompositorApi,
) -> Result<CompositorTable, String> {
    if pointer.is_null() {
        return Err("Compositor returned a null ABI table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::CompositorApi>() as u32
    {
        return Err("Incompatible compositor ABI".into());
    }
    let api = std::ptr::read_unaligned(pointer);
    if api.render.is_none() {
        return Err("Compositor is missing render".into());
    }
    let capabilities =
        if header.struct_size >= std::mem::size_of::<abi::CompositorApiFeatures>() as u32 {
            std::ptr::read_unaligned(pointer.cast::<abi::CompositorApiFeatures>()).capabilities
        } else {
            0
        };
    let modes = if header.struct_size >= std::mem::size_of::<abi::CompositorApiModes>() as u32 {
        std::ptr::read_unaligned(pointer.cast::<abi::CompositorApiModes>()).supported_blend_modes
    } else {
        abi::BLEND_MODES_LEGACY
    };
    if modes & abi::BLEND_MODES_LEGACY != abi::BLEND_MODES_LEGACY {
        return Err("Compositor blend mode mask omits legacy modes".into());
    }
    let masked = if capabilities & abi::COMPOSITOR_MASKS != 0 {
        if header.struct_size < std::mem::size_of::<abi::CompositorApiMasks>() as u32 {
            return Err("Compositor mask capability requires a complete table".into());
        }
        let callback =
            std::ptr::read_unaligned(pointer.cast::<abi::CompositorApiMasks>()).render_masked;
        if callback.is_none() {
            return Err("Compositor mask capability is missing render_masked".into());
        }
        callback
    } else {
        None
    };
    decode_mask_effects_table(pointer, capabilities)?;
    Ok((api, capabilities, modes, masked))
}
// Read only advertised, complete optional tails; legacy prefixes remain usable.
unsafe fn decode_selection_table(
    pointer: *const abi::SelectionApi,
) -> Result<SelectionTable, String> {
    if pointer.is_null() {
        return Err("Null selection table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::SelectionApi>() as u32
    {
        return Err("Incompatible selection ABI".into());
    }
    let api = std::ptr::read_unaligned(pointer);
    if api.mix.is_none() {
        return Err("Missing selection mixing callback".into());
    }
    let color = if header.struct_size >= std::mem::size_of::<abi::SelectionApiColor>() as u32 {
        std::ptr::read_unaligned(pointer.cast::<abi::SelectionApiColor>()).color
    } else {
        None
    };
    let edges = if header.struct_size >= std::mem::size_of::<abi::SelectionApiEdges>() as u32 {
        std::ptr::read_unaligned(pointer.cast::<abi::SelectionApiEdges>()).edges
    } else {
        None
    };
    let transfer = if header.struct_size >= std::mem::size_of::<abi::SelectionApiTransfer>() as u32
    {
        std::ptr::read_unaligned(pointer.cast::<abi::SelectionApiTransfer>()).transfer
    } else {
        None
    };
    Ok((api, color, edges, transfer))
}
unsafe fn decode_resample_table(
    pointer: *const abi::ResampleApi,
) -> Result<(abi::ResampleApi, u32), String> {
    if pointer.is_null() {
        return Err("Null resample table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::ResampleApi>() as u32
    {
        return Err("Incompatible resample ABI".into());
    }
    let features = if header.struct_size >= std::mem::size_of::<abi::ResampleApiExt>() as u32 {
        std::ptr::read_unaligned(pointer.cast::<abi::ResampleApiExt>()).features
    } else {
        0
    };
    Ok((std::ptr::read_unaligned(pointer), features))
}
unsafe fn decode_a8_sampler_table(
    pointer: *const abi::A8SamplerApi,
) -> Result<abi::A8SamplerApi, String> {
    if pointer.is_null() {
        return Err("A8 sampler returned a null table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::A8SamplerApi>() as u32
    {
        return Err("Incompatible A8 sampler ABI".into());
    }
    let api = std::ptr::read_unaligned(pointer);
    if api.reframe.is_none() || api.geometry.is_none() || api.resample.is_none() {
        return Err("A8 sampler is missing required reframe/geometry/resample callbacks".into());
    }
    Ok(api)
}
unsafe fn decode_a8_affine_table(
    pointer: *const abi::A8AffineApi,
) -> Result<abi::A8AffineApi, String> {
    if pointer.is_null() {
        return Err("A8 affine sampler returned a null table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != abi::ABI_VERSION
        || header.struct_size < std::mem::size_of::<abi::A8AffineApi>() as u32
    {
        return Err("Incompatible A8 affine sampler ABI".into());
    }
    let api = std::ptr::read_unaligned(pointer);
    if api.sample.is_none() {
        return Err("A8 affine sampler is missing the sample callback".into());
    }
    Ok(api)
}
unsafe fn decode_move_table(pointer: *const abi::MoveApi) -> Result<abi::MoveApi, String> {
    if pointer.is_null() {
        return Err("Null move table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::MoveApi>() as u32 {
        return Err("Incompatible move ABI".into());
    }
    Ok(std::ptr::read_unaligned(pointer))
}
unsafe fn decode_affine_table(pointer: *const abi::AffineApi) -> Result<abi::AffineApi, String> {
    if pointer.is_null() {
        return Err("Null affine table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::AffineApi>() as u32
    {
        return Err("Incompatible affine ABI".into());
    }
    Ok(std::ptr::read_unaligned(pointer))
}
unsafe fn decode_projective_table(
    pointer: *const abi::ProjectiveApi,
) -> Result<abi::ProjectiveApi, String> {
    if pointer.is_null() {
        return Err("Null projective table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::ProjectiveApi>() as u32
    {
        return Err("Incompatible projective ABI".into());
    }
    Ok(std::ptr::read_unaligned(pointer))
}
unsafe fn decode_trim_table(pointer: *const abi::TrimApi) -> Result<abi::TrimApi, String> {
    if pointer.is_null() {
        return Err("Null trim table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1 || header.struct_size < std::mem::size_of::<abi::TrimApi>() as u32 {
        return Err("Incompatible trim ABI".into());
    }
    Ok(std::ptr::read_unaligned(pointer))
}
unsafe fn decode_geometry_table(
    pointer: *const abi::GeometryApi,
) -> Result<abi::GeometryApi, String> {
    if pointer.is_null() {
        return Err("Null geometry table".into());
    }
    let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
    if header.abi_version != 1
        || header.struct_size < std::mem::size_of::<abi::GeometryApi>() as u32
    {
        return Err("Incompatible geometry ABI".into());
    }
    Ok(std::ptr::read_unaligned(pointer))
}
impl Module {
    unsafe fn geometry_api(&self) -> Result<Option<abi::GeometryApi>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::GEOMETRY_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::GeometryEntry>(address);
            decode_geometry_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn trim_api(&self) -> Result<Option<abi::TrimApi>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::TRIM_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::TrimEntry>(address);
            decode_trim_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn move_api(&self) -> Result<Option<abi::MoveApi>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::MOVE_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::MoveEntry>(address);
            decode_move_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    unsafe fn affine_api(&self) -> Result<Option<abi::AffineApi>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::AFFINE_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::AffineEntry>(address);
            decode_affine_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn projective_api(&self) -> Result<Option<abi::ProjectiveApi>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::PROJECTIVE_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::ProjectiveEntry>(address);
            decode_projective_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn resample_api(&self) -> Result<Option<(abi::ResampleApi, u32)>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::RESAMPLE_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::ResampleEntry>(address);
            decode_resample_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn a8_sampler_api(&self) -> Result<Option<abi::A8SamplerApi>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::A8_SAMPLER_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::A8SamplerEntry>(address);
            decode_a8_sampler_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn a8_affine_api(&self) -> Result<Option<abi::A8AffineApi>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::A8_AFFINE_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::A8AffineEntry>(address);
            decode_a8_affine_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    unsafe fn selection_api(&self) -> Result<Option<SelectionTable>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::SELECTION_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::SelectionEntry>(address);
            decode_selection_table(entry()).map(Some)
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    unsafe fn tool_options_api(&self) -> Result<Option<(abi::ToolOptionsApi, u32)>, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(
                self.handle as *mut c_void,
                abi::TOOL_OPTIONS_SYMBOL.as_ptr(),
            );
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::ToolOptionsEntry>(address);
            let pointer = entry();
            if pointer.is_null() {
                return Err("Null tool options table".into());
            }
            let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
            if header.abi_version != 1
                || header.struct_size < std::mem::size_of::<abi::ToolOptionsApi>() as u32
            {
                return Err("Incompatible tool options ABI".into());
            }
            let api = std::ptr::read_unaligned(pointer);
            if api.configure.is_none() {
                return Err("Missing tool options callback".into());
            }
            let features =
                if header.struct_size >= std::mem::size_of::<abi::ToolOptionsApiExt>() as u32 {
                    std::ptr::read_unaligned(pointer.cast::<abi::ToolOptionsApiExt>()).features
                } else {
                    0
                };
            Ok(Some((api, features)))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    unsafe fn compositor_api(
        &self,
    ) -> Result<Option<(CompositorTable, Option<MaskEffectsTable>)>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::COMPOSITOR_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::CompositorEntry>(address);
            let pointer = entry();
            let table = decode_compositor_table(pointer)?;
            let effects = decode_mask_effects_table(pointer, table.1)?;
            Ok(Some((table, effects)))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    fn load(path: &Path) -> Result<Self, String> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let handle =
                unsafe { LoadLibraryExW(name.as_ptr(), std::ptr::null_mut(), 0x100 | 0x1000) };
            if handle.is_null() {
                return Err(format!(
                    "DLL loading failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self {
                handle: handle as isize,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err("Native plugins currently require Windows DLLs".into())
        }
    }
    unsafe fn tool_api(&self) -> Result<Option<abi::ToolApi>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::TOOL_ENTRY_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::ToolEntry>(address);
            let pointer = entry();
            if pointer.is_null() {
                return Err("Tool plugin returned a null ABI table".into());
            }
            let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
            if header.abi_version != abi::TOOL_ABI_VERSION
                || header.struct_size < std::mem::size_of::<abi::ToolApi>() as u32
            {
                return Err("Incompatible tool ABI".into());
            }
            let api = std::ptr::read_unaligned(pointer);
            if api.create.is_none() || api.destroy.is_none() || api.pointer.is_none() {
                return Err("Tool ABI is missing required functions".into());
            }
            Ok(Some(api))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    unsafe fn entry(&self) -> Result<abi::Entry, String> {
        #[cfg(windows)]
        {
            let address = GetProcAddress(self.handle as *mut c_void, abi::ENTRY_SYMBOL.as_ptr());
            if address.is_null() {
                return Err("DLL is missing the micopi_plugin_v1 export".into());
            }
            Ok(std::mem::transmute::<*mut c_void, abi::Entry>(address))
        }
        #[cfg(not(windows))]
        {
            Err("Native plugins are not supported on this platform".into())
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Contributions {
    id: String,
    #[serde(default)]
    pub actions: Vec<super::ActionDefinition>,
    #[serde(default)]
    pub tools: Vec<super::ToolDefinition>,
    #[serde(default)]
    pub panels: Vec<super::PanelDefinition>,
    #[serde(default)]
    pub states: Vec<super::StateDefinition>,
    #[serde(default)]
    pub nodes: Vec<super::nodes::Definition>,
}
pub struct NativePlugin {
    pub path: PathBuf,
    api: abi::PluginApi,
    tools: Option<abi::ToolApi>,
    tool_options: Option<(abi::ToolOptionsApi, u32)>,
    selection_mix: Option<abi::SelectionApi>,
    selection_color: Option<abi::SelectionColorFn>,
    selection_edges: Option<abi::SelectionEdgesFn>,
    selection_transfer: Option<abi::SelectionTransferFn>,
    geometry: Option<abi::GeometryApi>,
    trim: Option<abi::TrimApi>,
    movement: Option<abi::MoveApi>,
    affine: Option<abi::AffineApi>,
    projective: Option<abi::ProjectiveApi>,
    resample: Option<abi::ResampleApi>,
    resample_features: u32,
    a8_sampler: Option<abi::A8SamplerApi>,
    a8_affine: Option<abi::A8AffineApi>,
    compositor: Option<abi::CompositorApi>,
    compositor_capabilities: u64,
    compositor_blend_modes: u64,
    compositor_masked: Option<abi::CompositeRenderMaskedFn>,
    compositor_effects: Option<MaskEffectsTable>,
    mask_effect_instance: std::sync::Mutex<Option<MaskEffectInstance>>,
    _module: Module,
}
struct MaskEffectInstance {
    pointer: usize,
    destroy: abi::MaskEffectsDestroyFn,
}
impl Drop for MaskEffectInstance {
    fn drop(&mut self) {
        // NativePlugin declares this cache before its module, so the callback
        // remains loaded until its instance has been destroyed.
        unsafe {
            (self.destroy)(self.pointer as *mut c_void);
        }
    }
}
impl std::fmt::Debug for NativePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativePlugin")
            .field("path", &self.path)
            .field("abi", &abi::ABI_VERSION)
            .finish()
    }
}
impl NativePlugin {
    pub(super) fn has_compositor(&self) -> bool {
        self.compositor.is_some()
    }
    pub(super) fn has_tools(&self) -> bool {
        self.tools.is_some()
    }
    pub(super) fn has_actions(&self) -> bool {
        self.api.invoke.is_some()
    }
    pub(crate) fn supports_a8_sampler(&self) -> bool {
        self.a8_sampler.is_some()
    }
    pub(crate) fn supports_a8_affine(&self) -> bool {
        self.a8_affine.is_some()
    }
    /// Synchronous borrowed sampling; the caller owns checked output staging.
    pub(crate) fn a8_affine(
        &self,
        request: &abi::A8Affine,
        source: &abi::CompositeMask,
        source_width: u32,
        source_height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        let callback = self
            .a8_affine
            .as_ref()
            .and_then(|api| api.sample)
            .ok_or("Painting DLL does not support raw A8 affine sampling")?;
        let status = unsafe { callback(request, source, source_width, source_height, output) };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Native A8 affine sampling failed: {status}"))
        }
    }
    pub(crate) fn supports_stroke_opacity(&self) -> bool {
        self.tool_options
            .is_some_and(|(_, features)| features & abi::TOOL_OPTION_STROKE_OPACITY != 0)
    }
    #[cfg(test)]
    pub(crate) fn replace_tool_options_features_for_test(&mut self, features: u32) -> Option<u32> {
        self.tool_options
            .as_mut()
            .map(|(_, current)| std::mem::replace(current, features))
    }
    pub(crate) fn a8_reframe(
        &self,
        request: &abi::A8Reframe,
        source: &abi::CompositeMask,
        source_width: u32,
        source_height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        let callback = self
            .a8_sampler
            .as_ref()
            .and_then(|api| api.reframe)
            .ok_or("Painting DLL does not support A8 reframe sampling")?;
        let status = unsafe { callback(request, source, source_width, source_height, output) };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Native A8 reframe sampling failed: {status}"))
        }
    }
    pub(crate) fn a8_geometry(
        &self,
        request: &abi::A8Geometry,
        source: &abi::CompositeMask,
        source_width: u32,
        source_height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        let callback = self
            .a8_sampler
            .as_ref()
            .and_then(|api| api.geometry)
            .ok_or("Painting DLL does not support A8 geometry sampling")?;
        let status = unsafe { callback(request, source, source_width, source_height, output) };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Native A8 geometry sampling failed: {status}"))
        }
    }
    pub(crate) fn a8_resample(
        &self,
        request: &abi::Resample,
        source: &abi::CompositeMask,
        source_width: u32,
        source_height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        let callback = self
            .a8_sampler
            .as_ref()
            .and_then(|api| api.resample)
            .ok_or("Painting DLL does not support A8 resample sampling")?;
        let status = unsafe { callback(request, source, source_width, source_height, output) };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Native A8 resample sampling failed: {status}"))
        }
    }
    pub(super) fn load(
        parent: &Path,
        filename: &str,
        id: &str,
    ) -> Result<(Arc<Self>, Contributions), String> {
        let relative = Path::new(filename);
        if relative.components().count() != 1
            || !matches!(
                relative.components().next(),
                Some(std::path::Component::Normal(_))
            )
            || !relative
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        {
            return Err("library must be a DLL filename inside the plugin directory".into());
        }
        let parent = parent.canonicalize().map_err(|e| e.to_string())?;
        let path = parent
            .join(relative)
            .canonicalize()
            .map_err(|e| format!("DLL not found: {e}"))?;
        if !path.starts_with(&parent) {
            return Err("library cannot point outside the plugin directory".into());
        }
        let module = Module::load(&path)?;
        let api = unsafe {
            let pointer = (module.entry()?)();
            if pointer.is_null() {
                return Err("Plugin returned a null ABI table".into());
            }
            let header = std::ptr::read_unaligned(pointer.cast::<abi::Header>());
            if header.abi_version != abi::ABI_VERSION
                || header.struct_size < std::mem::size_of::<abi::PluginApi>() as u32
            {
                return Err(format!(
                    "Incompatible plugin ABI: version={}, size={}",
                    header.abi_version, header.struct_size
                ));
            }
            std::ptr::read_unaligned(pointer)
        };
        let geometry = unsafe { module.geometry_api()? };
        let trim = unsafe { module.trim_api()? };
        let movement = unsafe { module.move_api()? };
        let affine = unsafe { module.affine_api()? };
        let projective = unsafe { module.projective_api()? };
        let (resample, resample_features) = match unsafe { module.resample_api()? } {
            Some((api, features)) => (Some(api), features),
            None => (None, 0),
        };
        let a8_sampler = unsafe { module.a8_sampler_api()? };
        let a8_affine = unsafe { module.a8_affine_api()? };
        let tools = unsafe { module.tool_api()? };
        let tool_options = unsafe { module.tool_options_api()? };
        let (selection_mix, selection_color, selection_edges, selection_transfer) =
            match unsafe { module.selection_api()? } {
                Some((base, color, edges, transfer)) => (Some(base), color, edges, transfer),
                None => (None, None, None, None),
            };
        if tool_options.is_some() && tools.is_none() {
            return Err("Tool options require a tool API".into());
        }
        let (
            compositor,
            compositor_capabilities,
            compositor_blend_modes,
            compositor_masked,
            compositor_effects,
        ) = match unsafe { module.compositor_api()? } {
            Some(((api, capabilities, modes, masked), effects)) => {
                (Some(api), capabilities, modes, masked, effects)
            }
            None => (None, 0, 0, None, None),
        };
        if api.describe.is_none()
            || (api.invoke.is_none() && tools.is_none() && compositor.is_none())
            || api.create.is_some() != api.destroy.is_some()
        {
            return Err("Plugin ABI is missing required functions or paired create/destroy".into());
        }
        let descriptor = unsafe { (api.describe.unwrap())() };
        if descriptor.len > 1024 * 1024 || descriptor.len == 0 || descriptor.data.is_null() {
            return Err("Plugin descriptor is empty or exceeds 1 MB".into());
        }
        let contributions: Contributions = serde_json::from_slice(unsafe { descriptor.as_slice() })
            .map_err(|e| format!("Invalid plugin descriptor: {e}"))?;
        if contributions.id != id {
            return Err("DLL descriptor ID does not match the manifest".into());
        }
        Ok((
            Arc::new(Self {
                path,
                api,
                tools,
                tool_options,
                selection_mix,
                selection_color,
                selection_edges,
                selection_transfer,
                geometry,
                trim,
                movement,
                affine,
                projective,
                resample,
                resample_features,
                a8_sampler,
                a8_affine,
                compositor,
                compositor_capabilities,
                compositor_blend_modes,
                compositor_masked,
                compositor_effects,
                mask_effect_instance: Default::default(),
                _module: module,
            }),
            contributions,
        ))
    }
}
impl crate::engine::graph::Renderer for NativePlugin {
    fn supports_mask_effects(&self) -> bool {
        self.compositor_effects.is_some()
    }
    fn reserve_mask_effects(&self, width: u32, height: u32, radius: u32) -> Result<(), String> {
        let (create, destroy, _, reserve) = self
            .compositor_effects
            .ok_or("The compositor does not support mask density and feather")?;
        let mut cached = self
            .mask_effect_instance
            .lock()
            .map_err(|_| "Mask effect instance is unavailable")?;
        if cached.is_none() {
            let pointer = unsafe { create() };
            if pointer.is_null() {
                return Err("Compositor mask effect instance creation failed".into());
            }
            *cached = Some(MaskEffectInstance {
                pointer: pointer as usize,
                destroy,
            });
        }
        let status = unsafe {
            reserve(
                cached.as_ref().unwrap().pointer as *mut c_void,
                width,
                height,
                radius,
            )
        };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Mask effects reservation failed: {status}"))
        }
    }
    fn mask_effect_region(
        &self,
        request: &abi::MaskEffectRequest,
        source: &abi::CompositeMask,
        width: u32,
        height: u32,
        output: &mut abi::A8Pixels,
    ) -> Result<(), String> {
        let (create, destroy, region, _) = self
            .compositor_effects
            .ok_or("The compositor does not support mask density and feather")?;
        let mut cached = self
            .mask_effect_instance
            .lock()
            .map_err(|_| "Mask effect instance is unavailable")?;
        if cached.is_none() {
            let pointer = unsafe { create() };
            if pointer.is_null() {
                return Err("Compositor mask effect instance creation failed".into());
            }
            *cached = Some(MaskEffectInstance {
                pointer: pointer as usize,
                destroy,
            });
        }
        let status = unsafe {
            region(
                cached.as_ref().unwrap().pointer as *mut c_void,
                request,
                source,
                width,
                height,
                output,
            )
        };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Mask effects execution failed: {status}"))
        }
    }
    fn supports_masks(&self) -> bool {
        self.compositor_masked.is_some()
    }
    fn supports_mix(&self) -> bool {
        self.compositor_capabilities & abi::COMPOSITOR_MIX != 0
    }
    fn supports_blend_mode(&self, mode: u32) -> bool {
        mode <= abi::BLEND_MODE_MAX && self.compositor_blend_modes & (1u64 << mode) != 0
    }
    fn supports_transparent(&self) -> bool {
        self.compositor_capabilities & abi::COMPOSITOR_TRANSPARENT != 0
    }
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        let api = self.compositor.ok_or("Missing compositor interface")?;
        if nodes.iter().any(|n| n.kind == 4) && !self.supports_transparent() {
            return Err("Compositor does not support transparent image nodes".into());
        }
        if nodes.iter().any(|n| n.kind == 5) && !self.supports_mix() {
            return Err("The compositor does not support backdrop mixing".into());
        }
        if layers
            .iter()
            .any(|layer| !self.supports_blend_mode(layer.mode))
            || nodes
                .iter()
                .any(|node| node.kind == 2 && !self.supports_blend_mode(node.mode))
        {
            return Err("The compositor does not support this blend mode".into());
        }
        let status = unsafe {
            (api.render.unwrap())(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                layers.len(),
                width,
                height,
                output,
            )
        };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Compositor execution failed: {status}"))
        }
    }
    fn render_masked(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        masks: &[abi::CompositeMask],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        if masks.is_empty() && nodes.iter().all(|node| node.kind < 6) {
            return self.render(nodes, layers, width, height, output);
        }
        let render = self
            .compositor_masked
            .ok_or("The compositor does not support mask coverage")?;
        if nodes.iter().any(|node| node.kind == 4) && !self.supports_transparent() {
            return Err("Compositor does not support transparent image nodes".into());
        }
        if nodes.iter().any(|node| node.kind == 5) && !self.supports_mix() {
            return Err("The compositor does not support backdrop mixing".into());
        }
        if layers
            .iter()
            .any(|layer| !self.supports_blend_mode(layer.mode))
            || nodes
                .iter()
                .any(|node| node.kind == 2 && !self.supports_blend_mode(node.mode))
        {
            return Err("The compositor does not support this blend mode".into());
        }
        let status = unsafe {
            render(
                nodes.as_ptr(),
                nodes.len(),
                layers.as_ptr(),
                layers.len(),
                masks.as_ptr(),
                masks.len(),
                width,
                height,
                output,
            )
        };
        if status == abi::OK {
            Ok(())
        } else {
            Err(format!("Masked compositor execution failed: {status}"))
        }
    }
}
pub(crate) struct Instance {
    plugin: Arc<NativePlugin>,
    state: *mut c_void,
}
impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(destroy) = self.plugin.api.destroy {
            unsafe { destroy(self.state) }
        }
    }
}
impl Instance {
    fn new(plugin: Arc<NativePlugin>) -> Result<Self, ActionError> {
        let state = if let Some(create) = plugin.api.create {
            let state = unsafe { create() };
            if state.is_null() {
                return Err(ActionError::new(
                    "native_init",
                    "Plugin instance initialization failed",
                ));
            }
            state
        } else {
            std::ptr::null_mut()
        };
        Ok(Self { plugin, state })
    }
    fn invoke(
        &mut self,
        session: &mut Session,
        handler: &str,
        args: Value,
        access: PixelAccess,
    ) -> ActionResult {
        let args = serde_json::to_vec(&args)?;
        if args.len() > abi::MAX_MESSAGE {
            return Err(ActionError::new(
                "native_arguments",
                "Native action arguments exceed the limit",
            ));
        }
        let selection = session.doc.selection().current.clone();
        let mut context = HostContext {
            session,
            busy: false,
            selection,
            access,
            denied: None,
        };
        let host = abi::HostApiSelection {
            base: abi::HostApi {
                abi_version: abi::ABI_VERSION,
                struct_size: std::mem::size_of::<abi::HostApiSelection>() as u32,
                context: (&mut context as *mut HostContext).cast(),
                dispatch,
                edit_region,
            },
            selection: read_selection,
        };
        let mut output = Output::default();
        let reply = output.reply();
        let status = unsafe {
            (self.plugin.api.invoke.unwrap())(
                self.state,
                abi::Bytes::borrowed(handler.as_bytes()),
                abi::Bytes::borrowed(&args),
                &host.base,
                reply,
            )
        };
        if let Some(error) = context.denied {
            return Err(error);
        }
        if output.overflow {
            return Err(ActionError::new(
                "native_output",
                "Plugin result exceeds the protocol limit",
            ));
        }
        let value: Value = serde_json::from_slice(&output.bytes).map_err(|e| {
            ActionError::new("native_output", format!("Plugin must return JSON: {e}"))
        })?;
        if status != abi::OK {
            return Err(ActionError::new("native_error", value.to_string()));
        }
        Ok(value)
    }
}
pub(crate) fn execute(session: &mut Session, action: &str, args: Value) -> ActionResult {
    let access = session.registry.actions[action].effective_pixel_access();
    access.require(&session.doc, session.doc.active)?;
    if access != PixelAccess::None && session.doc.has_raster_masks() {
        session
            .doc
            .ensure_graph_renderer()
            .map_err(|error| ActionError::new("mask_renderer", error))?;
    }
    let (plugin_id, handler) = session
        .registry
        .native_actions
        .get(action)
        .cloned()
        .ok_or_else(|| ActionError::new("native_missing", "Native action is not bound to a DLL"))?;
    if session.native_stack.contains(&plugin_id) {
        return Err(ActionError::new(
            "native_reentrancy",
            "Plugin call forms a cycle or reenters the same instance",
        ));
    }
    let plugin = session
        .registry
        .native_plugins
        .get(&plugin_id)
        .cloned()
        .ok_or_else(|| ActionError::new("native_missing", "DLL is not loaded"))?;
    if session.read_depth == 0 {
        session.finish_stroke();
    }
    let mut instance = match session.native_instances.remove(&plugin_id) {
        Some(instance) => instance,
        None => Instance::new(plugin)?,
    };
    session.native_stack.push(plugin_id.clone());
    let result = instance.invoke(session, &handler, args, access);
    session.native_stack.pop();
    session.native_instances.insert(plugin_id, instance);
    result
}
#[derive(Default)]
struct Output {
    bytes: Vec<u8>,
    overflow: bool,
}
impl Output {
    fn reply(&mut self) -> abi::Reply {
        abi::Reply {
            context: (self as *mut Self).cast(),
            write: write_output,
        }
    }
}
unsafe extern "C" fn write_output(context: *mut c_void, bytes: abi::Bytes) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let output = &mut *context.cast::<Output>();
    if bytes.len > abi::MAX_MESSAGE
        || (bytes.len > 0 && bytes.data.is_null())
        || output.bytes.len().saturating_add(bytes.len) > abi::MAX_MESSAGE
    {
        output.overflow = true;
        return abi::ERROR;
    }
    output.bytes.extend_from_slice(bytes.as_slice());
    abi::OK
}
struct HostContext {
    session: *mut Session,
    busy: bool,
    selection: Option<crate::engine::selection::Mask>,
    access: PixelAccess,
    denied: Option<ActionError>,
}
unsafe extern "C" fn read_selection(context: *mut c_void, output: *mut abi::Coverage) -> i32 {
    if context.is_null() || output.is_null() {
        return abi::ERROR;
    }
    let context = &*context.cast::<HostContext>();
    if context.busy {
        return abi::ERROR;
    }
    *output = context.selection.as_ref().map(coverage).unwrap_or_default();
    abi::OK
}
fn coverage(mask: &crate::engine::selection::Mask) -> abi::Coverage {
    let (width, height) = mask.dimensions();
    abi::Coverage {
        data: mask.coverage().as_ptr(),
        len: mask.coverage().len(),
        width: width as u32,
        height: height as u32,
        stride: width as u32,
    }
}
pub(crate) fn selection_shape(
    s: &Session,
    shape: &abi::SelectionShape,
) -> Result<crate::engine::selection::Mask, String> {
    let api = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.selection_mix)
        .ok_or("Painting DLL does not support selection geometry")?;
    let rasterize = api
        .rasterize
        .ok_or("Painting DLL does not support selection geometry")?;
    let mut output = vec![0; s.doc.width * s.doc.height];
    let previous = s
        .doc
        .selection()
        .current
        .as_ref()
        .map(coverage)
        .unwrap_or_default();
    if unsafe {
        rasterize(
            shape,
            &previous,
            output.as_mut_ptr(),
            output.len(),
            s.doc.width as u32,
            s.doc.height as u32,
        )
    } != abi::OK
    {
        return Err("Selection geometry failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, output)
}
pub(crate) fn selection_edges(
    s: &Session,
    options: &abi::SelectionEdges,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.selection_edges)
        .ok_or("Painting DLL does not support selection edges")?;
    let mask = s
        .doc
        .selection()
        .current
        .as_ref()
        .ok_or("An active selection is required")?;
    let source = coverage(mask);
    let mut output = vec![0; s.doc.width * s.doc.height];
    if unsafe { kernel(options, &source, output.as_mut_ptr(), output.len()) } != abi::OK {
        return Err("Selection edge processing failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, output)
}
pub(crate) fn selection_color(
    s: &mut Session,
    options: &abi::SelectionColor,
    merged: bool,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.selection_color)
        .ok_or("Painting DLL does not support color selection")?;
    if merged {
        s.doc.compose();
        if let Some(error) = &s.doc.render_error {
            return Err(error.clone());
        }
    }
    let pixels = if merged {
        &s.doc.composite
    } else {
        &s.doc.layers[s.doc.active].pixels
    };
    let source = abi::Pixels {
        data: pixels.as_ptr() as *mut u32,
        len: pixels.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let previous = s
        .doc
        .selection()
        .current
        .as_ref()
        .map(coverage)
        .unwrap_or_default();
    let mut output = vec![0; s.doc.width * s.doc.height];
    if unsafe {
        kernel(
            options,
            &source,
            &previous,
            output.as_mut_ptr(),
            output.len(),
        )
    } != abi::OK
    {
        return Err("Color selection failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, output)
}
/// Cold action boundary: blend scratch output using the independent painting DLL.
/// Typed tool MOVE uses its frozen mask directly and never enters this function.
pub(crate) fn clip_region(
    s: &Session,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    pixels: &mut [u32],
) -> Result<(), String> {
    let Some(mask) = &s.doc.selection().current else {
        return Ok(());
    };
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|v| v > s.doc.width)
        || y.checked_add(h).is_none_or(|v| v > s.doc.height)
        || w.checked_mul(h) != Some(pixels.len())
    {
        return Err("Invalid selection clipping region".into());
    }
    let api = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.selection_mix)
        .ok_or("Painting DLL does not support selection blending")?;
    let mask = coverage(mask);
    let before = abi::Pixels {
        data: unsafe {
            s.doc.layers[s.doc.active]
                .pixels
                .as_ptr()
                .add(y * s.doc.width + x)
        } as *mut u32,
        len: (h - 1) * s.doc.width + w,
        width: w as u32,
        height: h as u32,
        stride: s.doc.width as u32,
        origin_x: x as u32,
        origin_y: y as u32,
    };
    let mut after = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: pixels.len(),
        width: w as u32,
        height: h as u32,
        stride: w as u32,
        origin_x: x as u32,
        origin_y: y as u32,
    };
    if unsafe { (api.mix.unwrap())(&mask, &before, &mut after) } != abi::OK {
        return Err("Selection blending failed".into());
    }
    Ok(())
}
unsafe extern "C" fn dispatch(
    context: *mut c_void,
    id: abi::Bytes,
    args: abi::Bytes,
    reply: abi::Reply,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let host = context.cast::<HostContext>();
    if (*host).busy {
        return abi::ERROR;
    }
    (*host).busy = true;
    let result = (|| -> ActionResult {
        if id.len > 96
            || args.len > abi::MAX_MESSAGE
            || (id.len > 0 && id.data.is_null())
            || (args.len > 0 && args.data.is_null())
        {
            return Err(ActionError::new(
                "native_arguments",
                "Invalid host call buffer",
            ));
        }
        let id = std::str::from_utf8(id.as_slice())
            .map_err(|e| ActionError::new("native_arguments", e.to_string()))?;
        let args = serde_json::from_slice(args.as_slice())?;
        (&mut *(*host).session).apply(Action::new(id, args))
    })();
    let (status, value) = match result {
        Ok(value) => (abi::OK, value),
        Err(error) => (abi::ERROR, json!({ "error": error })),
    };
    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    let written = (reply.write)(reply.context, abi::Bytes::borrowed(&bytes));
    (*host).busy = false;
    if written != abi::OK {
        return abi::ERROR;
    }
    status
}
unsafe extern "C" fn edit_region(
    context: *mut c_void,
    region: abi::Region,
    transform: Option<abi::Transform>,
    user: *mut c_void,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let host = context.cast::<HostContext>();
    if (*host).busy {
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        return abi::ERROR;
    };
    let session = &mut *(*host).session;
    if session.read_depth != 0 {
        return abi::ERROR;
    }
    if let Err(error) = (*host)
        .access
        .require_write(&session.doc, session.doc.active)
    {
        (*host).denied = Some(error);
        return abi::ERROR;
    }
    let [x, y, w, h] = [
        region.x as usize,
        region.y as usize,
        region.width as usize,
        region.height as usize,
    ];
    if w == 0
        || h == 0
        || x.checked_add(w).is_none_or(|v| v > session.doc.width)
        || y.checked_add(h).is_none_or(|v| v > session.doc.height)
        || !session.doc.layer_visible(session.doc.active)
    {
        return abi::ERROR;
    }
    (*host).busy = true;
    session.finish_stroke();
    let mut pixels = Vec::with_capacity(w * h);
    for row in y..y + h {
        pixels.extend_from_slice(
            &session.doc.layers[session.doc.active].pixels
                [row * session.doc.width + x..row * session.doc.width + x + w],
        );
    }
    let mut buffer = abi::Pixels {
        data: pixels.as_mut_ptr(),
        len: pixels.len(),
        width: region.width,
        height: region.height,
        stride: region.width,
        origin_x: region.x,
        origin_y: region.y,
    };
    let status = transform(user, &mut buffer);
    if status == abi::OK && session.doc.alpha_locked(session.doc.active) {
        let valid = pixels.iter().enumerate().all(|(i, after)| {
            let before = session.doc.layers[session.doc.active].pixels
                [(y + i / w) * session.doc.width + x + i % w];
            before >> 24 == *after >> 24 && (before >> 24 != 0 || before == *after)
        });
        if !valid {
            (*host).busy = false;
            return abi::ERROR;
        }
    }
    if status == abi::OK && clip_region(session, x, y, w, h, &mut pixels).is_err() {
        (*host).busy = false;
        return abi::ERROR;
    }
    if status == abi::OK && session.doc.replace_region(x, y, w, h, &pixels) {
        if !session.doc.history_group_active() {
            session.revision += 1;
        }
    }
    (*host).busy = false;
    status
}
#[cfg(test)]
#[path = "native_a8_affine_tests.rs"]
mod a8_affine_tests;
#[cfg(test)]
#[path = "native_a8_sampler_tests.rs"]
mod a8_sampler_tests;
#[cfg(test)]
#[path = "native_mask_effects_tests.rs"]
mod mask_effects_tests;
#[cfg(test)]
#[path = "native_masks_tests.rs"]
mod masks_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compositor_optional_features_require_a_complete_tail_and_preserve_the_prefix() {
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
        let prefix = abi::CompositorApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::CompositorApi>() as u32,
            render: Some(render),
        };
        assert_eq!(unsafe { decode_compositor_table(&prefix) }.unwrap().1, 0);
        assert_eq!(
            unsafe { decode_compositor_table(&prefix) }.unwrap().2,
            abi::BLEND_MODES_LEGACY
        );
        let mut table = abi::CompositorApiFeatures {
            base: prefix,
            capabilities: abi::COMPOSITOR_TRANSPARENT,
        };
        table.base.struct_size = std::mem::size_of::<abi::CompositorApiFeatures>() as u32 - 1;
        assert_eq!(
            unsafe { decode_compositor_table(&table.base) }.unwrap().1,
            0
        );
        table.base.struct_size += 1;
        assert_eq!(
            unsafe { decode_compositor_table(&table.base) }.unwrap().1,
            abi::COMPOSITOR_TRANSPARENT
        );
        assert_eq!(
            unsafe { decode_compositor_table(&table.base) }.unwrap().2,
            abi::BLEND_MODES_LEGACY
        );
        let mut modes = abi::CompositorApiModes {
            base: abi::CompositorApiFeatures {
                base: prefix,
                capabilities: abi::COMPOSITOR_TRANSPARENT,
            },
            supported_blend_modes: abi::BLEND_MODES_ALL,
        };
        modes.base.base.struct_size = std::mem::size_of::<abi::CompositorApiModes>() as u32 - 1;
        assert_eq!(
            unsafe { decode_compositor_table(&modes.base.base) }
                .unwrap()
                .2,
            abi::BLEND_MODES_LEGACY
        );
        modes.base.base.struct_size += 1;
        assert_eq!(
            unsafe { decode_compositor_table(&modes.base.base) }
                .unwrap()
                .2,
            abi::BLEND_MODES_ALL
        );
        modes.supported_blend_modes = abi::BLEND_MODES_LEGACY | (1u64 << abi::BLEND_HUE);
        assert_eq!(
            unsafe { decode_compositor_table(&modes.base.base) }
                .unwrap()
                .2,
            modes.supported_blend_modes
        );
        modes.supported_blend_modes &= !1;
        assert!(unsafe { decode_compositor_table(&modes.base.base) }
            .err()
            .unwrap()
            .contains("legacy"));
        table.base.render = None;
        assert!(unsafe { decode_compositor_table(&table.base) }.is_err());
        table.base.render = prefix.render;
        table.base.abi_version = 99;
        assert!(unsafe { decode_compositor_table(&table.base) }.is_err());
        assert!(unsafe { decode_compositor_table(std::ptr::null()) }.is_err());
    }
    #[test]
    fn edge_tail_is_optional_and_complete_size_checked_after_color_prefix() {
        unsafe extern "C" fn mix(
            _: *const abi::Coverage,
            _: *const abi::Pixels,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::OK
        }
        unsafe extern "C" fn edges(
            _: *const abi::SelectionEdges,
            _: *const abi::Coverage,
            _: *mut u8,
            _: usize,
        ) -> i32 {
            abi::OK
        }
        let mut table = abi::SelectionApiEdges {
            base: abi::SelectionApiColor {
                base: abi::SelectionApi {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::SelectionApiColor>() as u32,
                    mix: Some(mix),
                    rasterize: None,
                },
                color: None,
            },
            edges: Some(edges),
        };
        unsafe {
            assert!(decode_selection_table(&table.base.base)
                .unwrap()
                .2
                .is_none());
            table.base.base.struct_size = std::mem::size_of::<abi::SelectionApiEdges>() as u32 - 1;
            assert!(decode_selection_table(&table.base.base)
                .unwrap()
                .2
                .is_none());
            table.base.base.struct_size += 1;
            assert!(decode_selection_table(&table.base.base)
                .unwrap()
                .2
                .is_some());
        }
    }
    #[test]
    fn resample_optional_table_checks_header_before_reading_fixed_callbacks() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_resample_table(std::ptr::null()).is_err());
            assert!(decode_resample_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut api = abi::ResampleApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::ResampleApi>() as u32 - 1,
            pixels: None,
            mask: None,
        };
        unsafe {
            assert!(decode_resample_table(&api).is_err());
            api.struct_size += 1;
            assert!(decode_resample_table(&api).is_ok());
            api.abi_version = 2;
            assert!(decode_resample_table(&api).is_err());
        }
    }
    #[test]
    fn movement_optional_table_and_missing_callbacks_reject_without_mutation() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_move_table(std::ptr::null()).is_err());
            assert!(decode_move_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut api = abi::MoveApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::MoveApi>() as u32 - 1,
            pixels: None,
            mask: None,
        };
        unsafe {
            assert!(decode_move_table(&api).is_err());
            api.struct_size += 1;
            assert!(decode_move_table(&api).is_ok());
            api.abi_version = 2;
            assert!(decode_move_table(&api).is_err());
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .movement = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        let history = s.doc.history_bytes();
        assert!(s
            .apply(Action::new("canvas.move", json!({"dx":1,"dy":0})))
            .unwrap_err()
            .message
            .contains("does not support integer movement"));
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
        s.apply(Action::new("canvas.rotate_90_cw", json!({})))
            .unwrap();
        assert_eq!((s.doc.width, s.doc.height), (8, 12));
    }
    #[test]
    fn affine_optional_table_and_callback_failures_preserve_sources_and_other_capabilities() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_affine_table(std::ptr::null()).is_err());
            assert!(decode_affine_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut table = abi::AffineApi {
            bounds: None,
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::AffineApi>() as u32 - 1,
            pixels: None,
            mask: None,
        };
        unsafe {
            assert!(decode_affine_table(&table).is_err());
            table.struct_size += 1;
            assert!(decode_affine_table(&table).is_ok());
            table.abi_version = 2;
            assert!(decode_affine_table(&table).is_err());
        }
        unsafe extern "C" fn mask_ok(
            _: *const abi::Affine,
            _: *const abi::Coverage,
            out: *mut u8,
            len: usize,
        ) -> i32 {
            std::slice::from_raw_parts_mut(out, len).fill(0);
            abi::OK
        }
        unsafe extern "C" fn mask_bad(
            _: *const abi::Affine,
            _: *const abi::Coverage,
            _: *mut u8,
            _: usize,
        ) -> i32 {
            abi::ERROR
        }
        unsafe extern "C" fn pixel_bad(
            _: *const abi::Affine,
            _: *const abi::Pixels,
            _: *const abi::Coverage,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::ERROR
        }
        for kind in 0..4 {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
            let mut registry = super::super::Registry::load(&root).unwrap();
            Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
                .unwrap()
                .affine = match kind {
                0 => None,
                1 => Some(abi::AffineApi {
                    bounds: None,
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::AffineApi>() as u32,
                    pixels: None,
                    mask: None,
                }),
                _ => Some(abi::AffineApi {
                    bounds: None,
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::AffineApi>() as u32,
                    pixels: Some(pixel_bad),
                    mask: Some(if kind == 2 { mask_bad } else { mask_ok }),
                }),
            };
            let mut s = Session::new(registry).unwrap();
            s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
                .unwrap();
            s.apply(Action::new("layer.fill_color", json!({"color":"#aa77dd"})))
                .unwrap();
            s.apply(Action::new(
                "selection.shape",
                json!({"kind":"rectangle","x":1,"y":1,"width":4,"height":3}),
            ))
            .unwrap();
            let state = s.state();
            let original = s.doc.layers[0].pixels.clone();
            let composition = s.doc.composition();
            let history = s.doc.history_bytes();
            assert!(s
                .apply(Action::new(
                    "canvas.transform",
                    json!({"translate_x":1,"target":"selection"})
                ))
                .is_err());
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.layers[0].pixels, original);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.history_bytes(), history);
            s.apply(Action::new(
                "canvas.move",
                json!({"dx":1,"dy":0,"target":"layer"}),
            ))
            .unwrap();
        }
    }
    #[test]
    fn projective_optional_table_and_callback_failures_preserve_sources_and_other_capabilities() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_projective_table(std::ptr::null()).is_err());
            assert!(decode_projective_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut table = abi::ProjectiveApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::ProjectiveApi>() as u32 - 1,
            pixels: None,
            mask: None,
        };
        unsafe {
            assert!(decode_projective_table(&table).is_err());
            table.struct_size += 1;
            assert!(decode_projective_table(&table).is_ok());
            table.abi_version = 2;
            assert!(decode_projective_table(&table).is_err());
        }
        unsafe extern "C" fn mask_ok(
            _: *const abi::Projective,
            _: *const abi::Coverage,
            out: *mut u8,
            len: usize,
        ) -> i32 {
            std::slice::from_raw_parts_mut(out, len).fill(0);
            abi::OK
        }
        unsafe extern "C" fn mask_bad(
            _: *const abi::Projective,
            _: *const abi::Coverage,
            out: *mut u8,
            len: usize,
        ) -> i32 {
            if len > 0 {
                out.write(128);
            }
            abi::ERROR
        }
        unsafe extern "C" fn pixel_bad(
            _: *const abi::Projective,
            _: *const abi::Pixels,
            _: *const abi::Coverage,
            out: *mut abi::Pixels,
        ) -> i32 {
            (*out).data.write(0xff112233);
            abi::ERROR
        }
        for kind in 0..4 {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
            let mut registry = super::super::Registry::load(&root).unwrap();
            Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
                .unwrap()
                .projective = match kind {
                0 => None,
                1 => Some(abi::ProjectiveApi {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::ProjectiveApi>() as u32,
                    pixels: None,
                    mask: None,
                }),
                _ => Some(abi::ProjectiveApi {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::ProjectiveApi>() as u32,
                    pixels: Some(pixel_bad),
                    mask: Some(if kind == 2 { mask_bad } else { mask_ok }),
                }),
            };
            let mut s = Session::new(registry).unwrap();
            s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
                .unwrap();
            s.apply(Action::new("layer.fill_color", json!({"color":"#aa77dd"})))
                .unwrap();
            s.apply(Action::new(
                "selection.shape",
                json!({"kind":"rectangle","x":1,"y":1,"width":4,"height":3}),
            ))
            .unwrap();
            let state = s.state();
            let original = s.doc.layers[0].pixels.clone();
            let composition = s.doc.composition();
            let history = s.doc.history_bytes();
            assert!(s
                .apply(Action::new(
                    "canvas.transform",
                    json!({"quad":[[1,1],[6,1],[5,5],[1,5]],"target":"selection"})
                ))
                .is_err());
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.layers[0].pixels, original);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.history_bytes(), history);
            s.apply(Action::new(
                "canvas.move",
                json!({"dx":1,"dy":0,"target":"layer"}),
            ))
            .unwrap();
        }
    }
    #[test]
    fn affine_invalid_native_content_bounds_reject_queries_and_mutations() {
        unsafe extern "C" fn bad(_: *const abi::Pixels, out: *mut abi::TrimBounds) -> i32 {
            *out = abi::TrimBounds {
                x: u32::MAX,
                y: 0,
                width: u32::MAX,
                height: 1,
            };
            abi::OK
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .affine
            .as_mut()
            .unwrap()
            .bounds = Some(bad);
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        s.apply(Action::new("layer.fill_color", json!({"color":"#aa77dd"})))
            .unwrap();
        let state = s.state();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        for id in ["canvas.transform.plan", "canvas.transform"] {
            assert!(s
                .apply(Action::new(id, json!({"target":"layer","angle":90})))
                .unwrap_err()
                .message
                .contains("invalid content bounds"));
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            assert_eq!(s.doc.history_bytes(), history);
        }
    }
    #[test]
    fn movement_callback_failures_discard_staged_masks_and_pixels() {
        unsafe extern "C" fn mask_ok(
            _: *const abi::Move,
            _: *const abi::Coverage,
            out: *mut u8,
            len: usize,
        ) -> i32 {
            std::slice::from_raw_parts_mut(out, len).fill(0);
            abi::OK
        }
        unsafe extern "C" fn mask_bad(
            _: *const abi::Move,
            _: *const abi::Coverage,
            _: *mut u8,
            _: usize,
        ) -> i32 {
            abi::ERROR
        }
        unsafe extern "C" fn pixels_bad(
            _: *const abi::Move,
            _: *const abi::Pixels,
            _: *const abi::Coverage,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::ERROR
        }
        for fail_mask in [false, true] {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
            let mut registry = super::super::Registry::load(&root).unwrap();
            Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
                .unwrap()
                .movement = Some(abi::MoveApi {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::MoveApi>() as u32,
                pixels: Some(pixels_bad),
                mask: Some(if fail_mask { mask_bad } else { mask_ok }),
            });
            let mut s = Session::new(registry).unwrap();
            s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
                .unwrap();
            s.apply(Action::new("layer.fill_color", json!({"color":"#bb88dd"})))
                .unwrap();
            s.apply(Action::new(
                "selection.shape",
                json!({"kind":"rectangle","x":1,"y":1,"width":4,"height":3}),
            ))
            .unwrap();
            let state = s.state();
            let composition = s.doc.composition();
            let pixels = s.doc.layers[0].pixels.clone();
            let history = s.doc.history_bytes();
            assert!(s
                .apply(Action::new(
                    "canvas.move",
                    json!({"dx":2,"dy":1,"target":"selection"})
                ))
                .is_err());
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.composition(), composition);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            assert_eq!(s.doc.history_bytes(), history);
        }
    }
    #[test]
    fn trim_optional_table_and_invalid_outputs_reject_before_structural_changes() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_trim_table(std::ptr::null()).is_err());
            assert!(decode_trim_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut api = abi::TrimApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::TrimApi>() as u32 - 1,
            bounds: None,
        };
        unsafe {
            assert!(decode_trim_table(&api).is_err());
            api.struct_size += 1;
            assert!(decode_trim_table(&api).is_ok());
            api.abi_version = 2;
            assert!(decode_trim_table(&api).is_err());
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .trim = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        for action in ["canvas.trim", "canvas.trim_bounds"] {
            assert!(s
                .apply(Action::new(action, json!({})))
                .unwrap_err()
                .message
                .contains("does not support automatic trim"));
            assert_eq!(s.state(), state);
        }
        s.apply(Action::new(
            "canvas.image_size",
            json!({"width":6,"method":"nearest"}),
        ))
        .unwrap();
        assert_eq!((s.doc.width, s.doc.height), (6, 4));
        unsafe extern "C" fn bad(
            _: *const abi::Trim,
            _: *const abi::Pixels,
            out: *mut abi::TrimBounds,
        ) -> i32 {
            *out = abi::TrimBounds {
                x: u32::MAX,
                y: 0,
                width: u32::MAX,
                height: 1,
            };
            abi::OK
        }
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .trim
            .as_mut()
            .unwrap()
            .bounds = Some(bad);
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        let history = s.doc.history_bytes();
        assert!(s
            .apply(Action::new("canvas.trim", json!({})))
            .unwrap_err()
            .message
            .contains("invalid bounds"));
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.doc.layers[0].pixels, vec![0; 96]);
    }
    #[test]
    fn geometry_optional_table_checks_header_before_reading_fixed_callbacks() {
        let header = abi::Header {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::Header>() as u32,
        };
        unsafe {
            assert!(decode_geometry_table(std::ptr::null()).is_err());
            assert!(decode_geometry_table((&header as *const abi::Header).cast()).is_err());
        }
        let mut api = abi::GeometryApi {
            abi_version: 1,
            struct_size: std::mem::size_of::<abi::GeometryApi>() as u32 - 1,
            pixels: None,
            mask: None,
        };
        unsafe {
            assert!(decode_geometry_table(&api).is_err());
            api.struct_size += 1;
            assert!(decode_geometry_table(&api).is_ok());
            api.abi_version = 2;
            assert!(decode_geometry_table(&api).is_err());
        }
    }
    #[test]
    fn area_capability_requires_a_complete_advertised_optional_tail() {
        let mut api = abi::ResampleApiExt {
            base: abi::ResampleApi {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::ResampleApi>() as u32,
                pixels: None,
                mask: None,
            },
            features: abi::RESAMPLE_AREA,
        };
        unsafe {
            assert_eq!(decode_resample_table(&api.base).unwrap().1, 0);
            api.base.struct_size = std::mem::size_of::<abi::ResampleApiExt>() as u32 - 1;
            assert_eq!(decode_resample_table(&api.base).unwrap().1, 0);
            api.base.struct_size += 1;
            assert_eq!(
                decode_resample_table(&api.base).unwrap().1,
                abi::RESAMPLE_AREA
            );
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .resample_features = 0;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        let history = s.doc.history_bytes();
        let error = s
            .apply(Action::new(
                "canvas.image_size",
                json!({"width":6,"method":"area"}),
            ))
            .unwrap_err();
        assert_eq!(error.code, "image_size");
        assert!(error.message.contains("does not support area"));
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
        s.apply(Action::new(
            "canvas.image_size",
            json!({"width":6,"method":"bilinear"}),
        ))
        .unwrap();
    }
    #[test]
    fn missing_resample_and_failed_later_layer_never_publish_partial_outputs() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .resample = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        assert!(s
            .apply(Action::new("canvas.image_size", json!({"width":24})))
            .is_err());
        assert_eq!(s.state(), state);
        s.apply(Action::new("canvas.image_size", json!({"width":12})))
            .unwrap();
        let mut s = Session::new(super::super::Registry::load(&root).unwrap()).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        s.apply(Action::new("layer.add", json!({}))).unwrap();
        // First blank layer succeeds, second nonblank layer fails after writing output.
        s.apply(Action::new("layer.fill_color", json!({"color":"#ff0000"})))
            .unwrap();
        unsafe extern "C" fn fail(
            _: *const abi::Resample,
            src: *const abi::Pixels,
            out: *mut abi::Pixels,
        ) -> i32 {
            let source = std::slice::from_raw_parts((*src).data, (*src).len);
            std::slice::from_raw_parts_mut((*out).data, (*out).len).fill(0xff00ff00);
            if source.iter().any(|p| *p != 0) {
                abi::ERROR
            } else {
                abi::OK
            }
        }
        Arc::get_mut(s.registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .resample
            .as_mut()
            .unwrap()
            .pixels = Some(fail);
        let state = s.state();
        let history = s.doc.history_bytes();
        let pixels = s.doc.layers[1].pixels.clone();
        assert!(s
            .apply(Action::new("canvas.image_size", json!({"width":24})))
            .is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(s.doc.layers[0].pixels, vec![0; 96]);
        assert_eq!(s.doc.layers[1].pixels, pixels);
    }
    #[test]
    fn missing_geometry_and_oversized_mask_results_leave_document_untouched() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins");
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .geometry = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let state = s.state();
        let history = s.doc.history_bytes();
        for id in [
            "canvas.rotate_90_cw",
            "canvas.rotate_90_ccw",
            "canvas.rotate_180",
            "canvas.flip_horizontal",
            "canvas.flip_vertical",
            "layer.rotate_90_cw",
            "layer.rotate_90_ccw",
        ] {
            assert_eq!(
                s.apply(Action::new(id, json!({}))).unwrap_err().code,
                "geometry"
            );
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.history_bytes(), history);
        }
        unsafe extern "C" fn noise(
            _: *const abi::Geometry,
            _: *const abi::Coverage,
            p: *mut u8,
            len: usize,
            _: u32,
            _: u32,
        ) -> i32 {
            let mut seed = 17u32;
            for i in 0..len {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *p.add(i) = seed as u8;
            }
            abi::OK
        }
        let mut registry = super::super::Registry::load(&root).unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .geometry
            .as_mut()
            .unwrap()
            .mask = Some(noise);
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":512,"height":512})))
            .unwrap();
        s.apply(Action::new("selection.all", json!({}))).unwrap();
        let state = s.state();
        let composition = s.doc.composition();
        let history = s.doc.history_bytes();
        assert!(s
            .apply(Action::new("canvas.rotate_90_cw", json!({})))
            .is_err());
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
    }
    #[test]
    fn transfer_tail_is_optional_and_unsupported_operations_do_not_mutate() {
        unsafe extern "C" fn mix(
            _: *const abi::Coverage,
            _: *const abi::Pixels,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::OK
        }
        unsafe extern "C" fn transfer(
            _: *const abi::SelectionTransfer,
            _: *const abi::Pixels,
            _: *const abi::Coverage,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::OK
        }
        let mut table = abi::SelectionApiTransfer {
            base: abi::SelectionApiEdges {
                base: abi::SelectionApiColor {
                    base: abi::SelectionApi {
                        abi_version: 1,
                        struct_size: std::mem::size_of::<abi::SelectionApiEdges>() as u32,
                        mix: Some(mix),
                        rasterize: None,
                    },
                    color: None,
                },
                edges: None,
            },
            transfer: Some(transfer),
        };
        unsafe {
            assert!(decode_selection_table(&table.base.base.base)
                .unwrap()
                .3
                .is_none());
            table.base.base.base.struct_size =
                std::mem::size_of::<abi::SelectionApiTransfer>() as u32 - 1;
            assert!(decode_selection_table(&table.base.base.base)
                .unwrap()
                .3
                .is_none());
            table.base.base.base.struct_size += 1;
            assert!(decode_selection_table(&table.base.base.base)
                .unwrap()
                .3
                .is_some());
        }
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .selection_transfer = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        let before = s.state();
        let history = s.doc.history_bytes();
        for id in ["clipboard.copy", "clipboard.copy_merged", "clipboard.cut"] {
            assert!(s.apply(Action::new(id, json!({}))).is_err());
            assert!(s.clipboard.is_none());
            assert_eq!(s.state(), before);
            assert_eq!(s.doc.history_bytes(), history);
        }
        s.apply(Action::new("selection.all", json!({}))).unwrap();
        let before = s.state();
        let history = s.doc.history_bytes();
        for id in ["layer.via_copy", "layer.via_cut"] {
            assert!(s.apply(Action::new(id, json!({}))).is_err());
            assert_eq!(s.state(), before);
            assert_eq!(s.doc.history_bytes(), history);
            assert_eq!(s.doc.layers.len(), 1);
        }
    }
    #[test]
    fn unavailable_edge_kernel_preserves_masks_pixels_revision_and_undo() {
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .selection_edges = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        s.apply(Action::new("selection.all", json!({}))).unwrap();
        let before = s.state();
        let metadata = s.doc.composition();
        let pixels = s.doc.layers[0].pixels.clone();
        let history = s.doc.history_bytes();
        for id in [
            "selection.expand",
            "selection.contract",
            "selection.feather",
            "selection.smooth",
        ] {
            assert_eq!(
                s.apply(Action::new(id, json!({"radius":2})))
                    .unwrap_err()
                    .code,
                "selection_edges"
            );
            assert_eq!(s.state(), before);
            assert_eq!(s.doc.composition(), metadata);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            assert_eq!(s.doc.history_bytes(), history);
        }
        s.apply(Action::new("history.undo", json!({}))).unwrap();
        assert!(s.doc.selection().current.is_none());
    }
    #[test]
    fn oversized_edge_result_is_rejected_before_selection_publication() {
        unsafe extern "C" fn noise(
            _: *const abi::SelectionEdges,
            _: *const abi::Coverage,
            out: *mut u8,
            len: usize,
        ) -> i32 {
            let mut seed = 177u32;
            for i in 0..len {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *out.add(i) = (seed >> 24) as u8;
            }
            abi::OK
        }
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .selection_edges = Some(noise);
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":512,"height":512})))
            .unwrap();
        s.apply(Action::new("selection.all", json!({}))).unwrap();
        let state = s.state();
        let composition = s.doc.composition();
        let history = s.doc.history_bytes();
        let error = s
            .apply(Action::new("selection.feather", json!({"radius":2})))
            .unwrap_err();
        assert_eq!(error.code, "selection_edges");
        assert_eq!(s.state(), state);
        assert_eq!(s.doc.composition(), composition);
        assert_eq!(s.doc.history_bytes(), history);
        assert_eq!(
            s.apply(Action::new("replay.verify", json!({}))).unwrap()["result"]["valid"],
            true
        );
    }
    #[test]
    fn selection_tables_read_only_complete_advertised_tails() {
        unsafe extern "C" fn mix(
            _: *const abi::Coverage,
            _: *const abi::Pixels,
            _: *mut abi::Pixels,
        ) -> i32 {
            abi::OK
        }
        unsafe extern "C" fn color(
            _: *const abi::SelectionColor,
            _: *const abi::Pixels,
            _: *const abi::Coverage,
            _: *mut u8,
            _: usize,
        ) -> i32 {
            abi::OK
        }
        let mut table = abi::SelectionApiColor {
            base: abi::SelectionApi {
                abi_version: 1,
                struct_size: std::mem::size_of::<abi::SelectionApi>() as u32,
                mix: Some(mix),
                rasterize: None,
            },
            color: Some(color),
        };
        unsafe {
            assert!(decode_selection_table(std::ptr::null()).is_err());
            let legacy = table.base;
            let (base, tail, edges, transfer) = decode_selection_table(&legacy).unwrap();
            assert!(transfer.is_none());
            assert!(edges.is_none());
            assert!(base.mix.is_some());
            assert!(tail.is_none());
            table.base.struct_size = std::mem::size_of::<abi::SelectionApiColor>() as u32 - 1;
            assert!(decode_selection_table(&table.base).unwrap().1.is_none());
            table.base.struct_size += 1;
            assert!(decode_selection_table(&table.base).unwrap().1.is_some());
            table.base.abi_version = 2;
            assert!(decode_selection_table(&table.base).is_err());
            table.base.abi_version = 1;
            table.base.mix = None;
            assert!(decode_selection_table(&table.base).is_err());
            table.base.struct_size = std::mem::size_of::<abi::Header>() as u32;
            assert!(decode_selection_table(&table.base).is_err());
        }
    }
    #[test]
    fn unavailable_color_selection_preserves_document_and_records_failed_attempts() {
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        Arc::get_mut(registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .selection_color = None;
        let mut s = Session::new(registry).unwrap();
        s.apply(Action::new("canvas.new", json!({"width":12,"height":8})))
            .unwrap();
        s.apply(Action::new("selection.all", json!({}))).unwrap();
        let metadata = s.doc.composition();
        let pixels = s.doc.layers[0].pixels.clone();
        let state = s.state();
        let mut journal = s.replay_journal(false).unwrap().entries.len();
        for (id, args) in [
            ("selection.wand", json!({"x":0,"y":0})),
            ("selection.grow", json!({})),
            ("selection.similar", json!({})),
        ] {
            assert!(s.apply(Action::new(id, args)).is_err());
            assert_eq!(s.state(), state);
            assert_eq!(s.doc.composition(), metadata);
            assert_eq!(s.doc.layers[0].pixels, pixels);
            // Failed attempts are journaled, while their document outcome is unchanged.
            journal += 1;
            assert_eq!(s.replay_journal(false).unwrap().entries.len(), journal);
            assert_eq!(
                s.apply(Action::new("replay.verify", json!({}))).unwrap()["result"]["valid"],
                true
            );
        }
        s.apply(Action::new("history.undo", json!({}))).unwrap();
        assert!(s.doc.selection().current.is_none());
    }
    #[test]
    fn action_selection_snapshot_remains_alive_across_nested_document_changes() {
        use crate::engine::selection::{Mask, SelectionState};
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(2, 2);
        session
            .doc
            .set_selection(SelectionState {
                current: Some(Mask::new(2, 2, vec![0, 32, 128, 255]).unwrap()),
                previous: None,
            })
            .unwrap();
        let selection = session.doc.selection().current.clone();
        let mut context = HostContext {
            session: &mut session,
            busy: false,
            selection,
            access: PixelAccess::None,
            denied: None,
        };
        let mut mask = abi::Coverage::default();
        let pointer = (&mut context as *mut HostContext).cast();
        assert_eq!(unsafe { read_selection(pointer, &mut mask) }, abi::OK);
        session.doc = crate::engine::Document::new(8, 8);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(mask.data, mask.len) },
            [0, 32, 128, 255]
        );
        context.busy = true;
        assert_eq!(unsafe { read_selection(pointer, &mut mask) }, abi::ERROR);
        assert_eq!(
            unsafe { read_selection(std::ptr::null_mut(), &mut mask) },
            abi::ERROR
        );
        context.busy = false;
        context.selection = None;
        assert_eq!(unsafe { read_selection(pointer, &mut mask) }, abi::OK);
        assert!(mask.data.is_null());
    }
    #[test]
    fn locked_tool_validation_uses_strided_edge_tiles_and_rolls_back_bad_pixels() {
        unsafe extern "C" fn recolor(_: *mut c_void, p: *mut abi::Pixels) -> i32 {
            for y in 0..(*p).height as usize {
                for x in 0..(*p).width as usize {
                    let pixel = &mut *(*p).data.add(y * (*p).stride as usize + x);
                    if *pixel >> 24 != 0 {
                        *pixel = (*pixel & 0xff000000) | 0x00abcdef;
                    }
                }
            }
            abi::OK
        }
        unsafe extern "C" fn change_alpha(_: *mut c_void, p: *mut abi::Pixels) -> i32 {
            *(*p)
                .data
                .add(((*p).height as usize - 1) * (*p).stride as usize + (*p).width as usize - 1) =
                0xff123456;
            abi::OK
        }
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(70, 67);
        session.doc.layers[0].pixels.fill(0x80112233);
        session.doc.layers[0].pixels[64 * 70 + 64] = 0x00abcdef;
        session
            .doc
            .set_layer_properties_with_lock(
                0,
                true,
                255,
                crate::engine::graph::BlendMode::Normal,
                true,
            )
            .unwrap();
        let before = session.doc.layers[0].pixels.clone();
        session.doc.begin_plugin_stroke();
        let mut context = ToolContext {
            session: &mut session,
            busy: false,
            failed: false,
            write_pixels: true,
            mask_lease: None,
            stroke_opacity: false,
            denied: None,
        };
        let region = abi::Region {
            x: 63,
            y: 63,
            width: 7,
            height: 4,
        };
        assert_eq!(
            unsafe {
                tool_pixels(
                    (&mut context as *mut ToolContext).cast(),
                    region,
                    Some(recolor),
                    std::ptr::null_mut(),
                )
            },
            abi::OK
        );
        assert!(!context.failed);
        assert_eq!(session.doc.layers[0].pixels[64 * 70 + 64], 0x00abcdef);
        assert_eq!(
            unsafe {
                tool_pixels(
                    (&mut context as *mut ToolContext).cast(),
                    region,
                    Some(change_alpha),
                    std::ptr::null_mut(),
                )
            },
            abi::ERROR
        );
        assert!(context.failed);
        session.doc.rollback_plugin_stroke();
        assert_eq!(session.doc.layers[0].pixels, before);
    }
    #[test]
    fn missing_optional_tool_options_accept_defaults_and_reject_custom_values() {
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        let plugin = registry.native_plugins.get_mut("painting").unwrap();
        Arc::get_mut(plugin).unwrap().tool_options = None;
        let mut session = Session::new(registry).unwrap();
        session
            .apply(Action::new("canvas.new", json!({"width":32,"height":32})))
            .unwrap();
        let stroke = json!({"points":[{"x":10,"y":10},{"x":20,"y":20}]});
        session
            .apply(Action::new("canvas.stroke", stroke.clone()))
            .unwrap();
        session
            .apply(Action::new("brush.set", json!({"spacing":0.5})))
            .unwrap();
        let pixels = session.doc.layers[0].pixels.clone();
        let revision = session.revision;
        let error = session
            .apply(Action::new("canvas.stroke", stroke))
            .unwrap_err();
        assert_eq!(error.code, "tool_options_unsupported");
        assert_eq!(session.doc.layers[0].pixels, pixels);
        assert_eq!(session.revision, revision);
        session
            .apply(Action::new("brush.set", json!({"spacing":0.11})))
            .unwrap();
        session
            .apply(Action::new("layer.set", json!({"alpha_locked":true})))
            .unwrap();
        let before = session.state();
        let error = session
            .apply(Action::new(
                "canvas.stroke",
                json!({"points":[{"x":10,"y":10}]}),
            ))
            .unwrap_err();
        assert_eq!(error.code, "tool_options_unsupported");
        assert_eq!(session.state(), before);
        session
            .apply(Action::new("layer.set", json!({"alpha_locked":false})))
            .unwrap();
        session
            .apply(Action::new("selection.all", json!({})))
            .unwrap();
        let before = session.state();
        assert_eq!(
            session
                .apply(Action::new(
                    "canvas.stroke",
                    json!({"points":[{"x":10,"y":10}]})
                ))
                .unwrap_err()
                .code,
            "tool_options_unsupported"
        );
        assert_eq!(session.state(), before);
        session.tool_instances.clear();
        Arc::get_mut(session.registry.native_plugins.get_mut("painting").unwrap())
            .unwrap()
            .selection_mix = None;
        let before = session.state();
        let pixels = session.doc.layers[0].pixels.clone();
        assert!(session
            .apply(Action::new("native.invert", json!({})))
            .is_err());
        assert!(session
            .apply(Action::new("layer.clear", json!({})))
            .is_err());
        assert_eq!(session.state(), before);
        assert_eq!(session.doc.layers[0].pixels, pixels);
    }
    unsafe extern "C" fn fail(_: *mut c_void, pixels: *mut abi::Pixels) -> i32 {
        *(*pixels).data = 0xff123456;
        abi::ERROR
    }
    unsafe extern "C" fn paint(_: *mut c_void, pixels: *mut abi::Pixels) -> i32 {
        *(*pixels).data = 0xff123456;
        abi::OK
    }
    unsafe extern "C" fn count_paint(context: *mut c_void, pixels: *mut abi::Pixels) -> i32 {
        *context.cast::<usize>() += 1;
        paint(std::ptr::null_mut(), pixels)
    }
    #[test]
    fn legacy_unclassified_action_is_rejected_before_invoking_a_locked_layer() {
        use crate::engine::{document::LayerColor, graph::BlendMode};
        let mut registry = super::super::Registry::load(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"),
        )
        .unwrap();
        registry
            .actions
            .get_mut("native.invert")
            .unwrap()
            .pixel_access = None;
        let mut session = Session::new(registry).unwrap();
        session.doc = crate::engine::Document::new(8, 8);
        session
            .doc
            .set_layer_properties_organized(
                0,
                true,
                255,
                BlendMode::Normal,
                false,
                false,
                true,
                LayerColor::None,
            )
            .unwrap();
        let history = session.doc.history_bytes();
        let error = session
            .apply(Action::new("native.invert", json!({})))
            .unwrap_err();
        assert_eq!(error.code, "position_locked");
        assert!(!session.native_instances.contains_key("native_paint"));
        assert_eq!(session.doc.history_bytes(), history);
        assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
        // A read-only action from the same unchanged DLL remains usable.
        session
            .apply(Action::new("native.stats", json!({})))
            .unwrap();
        assert!(session.native_instances.contains_key("native_paint"));
    }
    #[test]
    fn mutable_action_loans_recheck_current_layer_locks_and_declared_access() {
        use crate::engine::{document::LayerColor, graph::BlendMode};
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(8, 8);
        session.doc.try_add_layer().unwrap();
        session
            .doc
            .set_layer_properties_organized(
                1,
                true,
                255,
                BlendMode::Normal,
                false,
                true,
                false,
                LayerColor::None,
            )
            .unwrap();
        session.doc.active = 0;
        session
            .doc
            .set_layer_properties_organized(
                0,
                true,
                255,
                BlendMode::Normal,
                false,
                false,
                true,
                LayerColor::None,
            )
            .unwrap();
        let mut context = HostContext {
            session: &mut session,
            busy: false,
            selection: None,
            access: PixelAccess::None,
            denied: None,
        };
        let region = abi::Region {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let mut calls = 0usize;
        let pointer = (&mut context as *mut HostContext).cast();
        let user = (&mut calls as *mut usize).cast();
        assert_eq!(
            unsafe { edit_region(pointer, region, Some(count_paint), user) },
            abi::ERROR
        );
        assert_eq!(context.denied.take().unwrap().code, "pixel_access");
        context.access = PixelAccess::Geometry;
        assert_eq!(
            unsafe { edit_region(pointer, region, Some(count_paint), user) },
            abi::ERROR
        );
        assert_eq!(context.denied.take().unwrap().code, "position_locked");
        assert_eq!(calls, 0);
        context.access = PixelAccess::Pixels;
        assert_eq!(
            unsafe { edit_region(pointer, region, Some(count_paint), user) },
            abi::OK
        );
        assert_eq!(calls, 1);
        assert_eq!(session.doc.layers[0].pixels[0], 0xff123456);
        // A nested dispatch may select a different layer after action entry.
        session.doc.active = 1;
        let history = session.doc.history_bytes();
        assert_eq!(
            unsafe { edit_region(pointer, region, Some(count_paint), user) },
            abi::ERROR
        );
        assert_eq!(context.denied.take().unwrap().code, "pixel_locked");
        assert_eq!(calls, 1);
        assert_eq!(session.doc.layers[1].pixels[0], 0);
        assert_eq!(session.doc.history_bytes(), history);
    }
    #[test]
    fn nonpainting_and_readonly_tools_cannot_borrow_mutable_pixels() {
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(8, 8);
        // An already-open document edit must not grant a read tool write access.
        session.doc.begin_plugin_stroke();
        let mut context = ToolContext {
            session: &mut session,
            busy: false,
            failed: false,
            write_pixels: false,
            mask_lease: None,
            stroke_opacity: false,
            denied: None,
        };
        let region = abi::Region {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let mut calls = 0usize;
        let pointer = (&mut context as *mut ToolContext).cast();
        let user = (&mut calls as *mut usize).cast();
        assert_eq!(
            unsafe { tool_pixels(pointer, region, Some(count_paint), user) },
            abi::ERROR
        );
        assert!(context.failed);
        context.failed = false;
        context.write_pixels = true;
        session.read_depth = 1;
        assert_eq!(
            unsafe { tool_pixels(pointer, region, Some(count_paint), user) },
            abi::ERROR
        );
        assert!(context.failed);
        assert_eq!(calls, 0);
        assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
        assert_eq!(session.doc.history_bytes(), 0);
        let mut color = 0x123456;
        assert_eq!(unsafe { tool_pick(pointer, 1., 1., &mut color) }, abi::OK);
        assert_eq!(color, 0x123456);
        session.doc.rollback_plugin_stroke();
    }
    #[test]
    fn pixel_callback_failure_is_atomic_and_success_is_undoable() {
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(8, 8);
        let mut context = HostContext {
            session: &mut session,
            busy: false,
            selection: None,
            access: PixelAccess::Pixels,
            denied: None,
        };
        let region = abi::Region {
            x: 2,
            y: 2,
            width: 3,
            height: 3,
        };
        unsafe {
            assert_eq!(
                edit_region(
                    (&mut context as *mut HostContext).cast(),
                    region,
                    Some(fail),
                    std::ptr::null_mut()
                ),
                abi::ERROR
            );
            assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
            assert_eq!(session.revision, 0);
            assert_eq!(session.doc.history_bytes(), 0);
            session.read_depth = 1;
            assert_eq!(
                edit_region(
                    (&mut context as *mut HostContext).cast(),
                    region,
                    Some(paint),
                    std::ptr::null_mut()
                ),
                abi::ERROR
            );
            assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
            session.read_depth = 0;
            assert_eq!(
                edit_region(
                    (&mut context as *mut HostContext).cast(),
                    region,
                    Some(paint),
                    std::ptr::null_mut()
                ),
                abi::OK
            );
        }
        assert_eq!(session.doc.layers[0].pixels[18], 0xff123456);
        assert_eq!(session.revision, 1);
        session.doc.undo();
        assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
    unsafe extern "C" fn failing_tool(
        _: *mut c_void,
        _: u32,
        sample: *const abi::ToolSample,
        _: *const abi::ToolParameters,
        host: *const abi::ToolHost,
        output: *mut abi::ToolOutput,
    ) -> i32 {
        let host = &*host;
        let down = (*sample).phase == abi::DOWN;
        let region = abi::Region {
            x: if down { 60 } else { 130 },
            y: 60,
            width: 10,
            height: 10,
        };
        (host.pixels)(
            host.context,
            region,
            Some(if down { paint } else { fail }),
            std::ptr::null_mut(),
        );
        (*output).flags = abi::DRAWING;
        // Even a plugin which ignores its callback error cannot commit the stroke.
        abi::OK
    }
    #[test]
    fn tool_callback_error_rolls_back_all_previously_exposed_tiles() {
        use crate::application::{Pointer, PointerPhase};
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(256, 128);
        let plugin = session.registry.native_plugins["painting"].clone();
        let mut instance = ToolInstance::new(plugin).unwrap();
        instance.api.pointer = Some(failing_tool);
        session.tool_instances.insert("painting".into(), instance);
        let p = Pointer {
            phase: PointerPhase::Down,
            x: 65.,
            y: 65.,
            pressure: 1.,
            time: 1,
            pen: true,
            eraser: false,
            view_x: None,
            view_y: None,
            tool_override: None,
        };
        session.pointer(p).unwrap();
        assert_eq!(session.doc.layers[0].pixels[60 * 256 + 60], 0xff123456);
        assert!(session
            .pointer(Pointer {
                phase: PointerPhase::Move,
                time: 2,
                ..p
            })
            .is_err());
        assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
        assert_eq!(session.doc.history_bytes(), 0);
        assert!(!session.doc.dirty);
        assert!(!session.is_drawing());
        assert!(!session.tool_instances.contains_key("painting"));
        // A subsequent contact gets a fresh real DLL state and remains usable.
        session.pointer(p).unwrap();
        session.finish_stroke();
        assert!(session.doc.layers[0].pixels.iter().any(|p| *p != 0));
        session.doc.undo();
        assert!(session.doc.layers[0].pixels.iter().all(|p| *p == 0));
    }
}

#[cfg(test)]
#[path = "native_mask_tools_tests.rs"]
mod mask_tool_tests;
#[cfg(test)]
#[path = "native_origin_tools_tests.rs"]
mod origin_tool_tests;

/// Cached for the document session; moved into a contact for allocation-free moves.
pub(crate) struct ToolInstance {
    selection: Option<crate::engine::selection::Mask>,
    stroke_opacity: bool,
    plugin: Arc<NativePlugin>,
    state: *mut c_void,
    api: abi::ToolApi,
}
impl Drop for ToolInstance {
    fn drop(&mut self) {
        unsafe { (self.api.destroy.unwrap())(self.state) }
    }
}
impl ToolInstance {
    pub(crate) fn new(plugin: Arc<NativePlugin>) -> Result<Self, ActionError> {
        let api = plugin.tools.ok_or_else(|| {
            ActionError::new("tool_plugin", "DLL has no point-by-point tool interface")
        })?;
        let state = unsafe { (api.create.unwrap())() };
        if state.is_null() {
            return Err(ActionError::new(
                "tool_plugin",
                "Tool instance initialization failed",
            ));
        }
        Ok(Self {
            plugin,
            state,
            api,
            selection: None,
            stroke_opacity: false,
        })
    }
    pub(crate) fn prepare_contact(
        &mut self,
        session: &Session,
        painting: bool,
        a8_target: bool,
    ) -> Result<(), ActionError> {
        if painting {
            let stroke_opacity =
                session.parameters.opacity_mode == crate::application::OpacityMode::Stroke;
            if (stroke_opacity || session.parameters.flow != 1.)
                && !self.plugin.supports_stroke_opacity()
            {
                return Err(ActionError::new(
                    "tool_options_unsupported",
                    "Tool DLL does not support Flow or stroke opacity",
                ));
            }
            if a8_target
                && self
                    .plugin
                    .tool_options
                    .is_none_or(|(_, features)| features & abi::TOOL_OPTION_A8_TARGET == 0)
            {
                return Err(ActionError::new(
                    "tool_options_unsupported",
                    "Tool DLL does not support raster-mask painting",
                ));
            }
            let selection = session.doc.selection().current.clone();
            if selection.is_some()
                && self
                    .plugin
                    .tool_options
                    .is_none_or(|(_, features)| features & abi::TOOL_OPTION_SELECTION == 0)
            {
                return Err(ActionError::new(
                    "tool_options_unsupported",
                    "Tool DLL does not support selection-clipped painting",
                ));
            }
            self.selection = selection;
            let p = &session.parameters;
            let locked = !a8_target && session.doc.alpha_locked(session.doc.active);
            if let Some((api, features)) = self.plugin.tool_options {
                if locked && features & abi::TOOL_OPTION_ALPHA_LOCK == 0 {
                    return Err(ActionError::new(
                        "tool_options_unsupported",
                        "Tool DLL does not support alpha-locked painting",
                    ));
                }
                let options = abi::ToolOptions {
                    abi_version: 1,
                    struct_size: std::mem::size_of::<abi::ToolOptions>() as u32,
                    spacing: p.spacing,
                    minimum_opacity: p.minimum_opacity,
                };
                let flags = if locked {
                    abi::TOOL_OPTION_ALPHA_LOCK
                } else {
                    0
                } | if a8_target {
                    abi::TOOL_OPTION_A8_TARGET
                } else {
                    0
                } | if stroke_opacity {
                    abi::TOOL_OPTION_STROKE_OPACITY
                } else {
                    0
                };
                let status = if features & abi::TOOL_OPTION_STROKE_OPACITY != 0 {
                    let extended = abi::ToolOptionsStroke {
                        base: abi::ToolOptionsSelection {
                            base: abi::ToolOptionsExt {
                                base: abi::ToolOptions {
                                    struct_size: std::mem::size_of::<abi::ToolOptionsStroke>()
                                        as u32,
                                    ..options
                                },
                                flags: flags
                                    | if self.selection.is_some() {
                                        abi::TOOL_OPTION_SELECTION
                                    } else {
                                        0
                                    },
                            },
                            selection: self.selection.as_ref().map(coverage).unwrap_or_default(),
                        },
                        flow: p.flow,
                        reserved: 0,
                    };
                    unsafe { (api.configure.unwrap())(self.state, &extended.base.base.base) }
                } else if features & abi::TOOL_OPTION_SELECTION != 0 {
                    let extended = abi::ToolOptionsSelection {
                        base: abi::ToolOptionsExt {
                            base: abi::ToolOptions {
                                struct_size: std::mem::size_of::<abi::ToolOptionsSelection>()
                                    as u32,
                                ..options
                            },
                            flags: flags
                                | if self.selection.is_some() {
                                    abi::TOOL_OPTION_SELECTION
                                } else {
                                    0
                                },
                        },
                        selection: self.selection.as_ref().map(coverage).unwrap_or_default(),
                    };
                    unsafe { (api.configure.unwrap())(self.state, &extended.base.base) }
                } else if features & (abi::TOOL_OPTION_ALPHA_LOCK | abi::TOOL_OPTION_A8_TARGET) != 0
                {
                    let extended = abi::ToolOptionsExt {
                        base: abi::ToolOptions {
                            struct_size: std::mem::size_of::<abi::ToolOptionsExt>() as u32,
                            ..options
                        },
                        flags,
                    };
                    unsafe { (api.configure.unwrap())(self.state, &extended.base) }
                } else {
                    unsafe { (api.configure.unwrap())(self.state, &options) }
                };
                if status != abi::OK {
                    return Err(ActionError::new(
                        "tool_options",
                        "Tool rejected stroke options",
                    ));
                }
            } else if locked || p.spacing != 0.11 || p.minimum_opacity != 0. {
                return Err(ActionError::new(
                    "tool_options_unsupported",
                    if locked {
                        "Tool DLL does not support alpha-locked painting"
                    } else {
                        "Tool DLL does not support custom spacing or pressure opacity floor"
                    },
                ));
            }
            self.stroke_opacity = stroke_opacity;
        } else {
            self.stroke_opacity = false;
        }
        Ok(())
    }
    pub(crate) fn pointer(
        &mut self,
        session: &mut Session,
        handler: u32,
        sample: abi::ToolSample,
        parameters: Option<&abi::ToolParameters>,
        painting: bool,
        mask_lease: Option<crate::engine::raster_masks::MaskLease>,
    ) -> Result<abi::ToolOutput, ActionError> {
        let stroke_opacity = painting && self.stroke_opacity;
        let mut context = ToolContext {
            session,
            busy: false,
            failed: false,
            write_pixels: painting,
            mask_lease,
            stroke_opacity,
            denied: None,
        };
        let host = abi::ToolHostStroke {
            base: abi::ToolHostA8 {
                base: abi::ToolHost {
                    abi_version: abi::TOOL_ABI_VERSION,
                    struct_size: if stroke_opacity {
                        std::mem::size_of::<abi::ToolHostStroke>()
                    } else if mask_lease.is_some() {
                        std::mem::size_of::<abi::ToolHostA8>()
                    } else {
                        std::mem::size_of::<abi::ToolHost>()
                    } as u32,
                    context: (&mut context as *mut ToolContext).cast(),
                    pixels: tool_pixels,
                    pick: tool_pick,
                },
                a8: if mask_lease.is_some() {
                    Some(tool_a8)
                } else {
                    None
                },
            },
            features: if stroke_opacity {
                if mask_lease.is_some() {
                    abi::TOOL_HOST_ORIGIN_A8
                } else {
                    abi::TOOL_HOST_ORIGIN_RGBA
                }
            } else {
                0
            },
            tile_side: crate::engine::TILE as u32,
            pixels_origin: (stroke_opacity && mask_lease.is_none())
                .then_some(tool_pixels_origin as abi::OriginPixelsLoan),
            a8_origin: (stroke_opacity && mask_lease.is_some())
                .then_some(tool_a8_origin as abi::OriginA8Loan),
        };
        let mut output = abi::ToolOutput::default();
        let status = unsafe {
            (self.api.pointer.unwrap())(
                self.state,
                handler,
                &sample,
                parameters.map_or(std::ptr::null(), |p| p as *const _),
                &host.base.base,
                &mut output,
            )
        };
        if let Some(error) = context.denied {
            return Err(error);
        }
        if status != abi::OK || context.failed {
            return Err(ActionError::new(
                "tool_plugin",
                format!(
                    "Point tool failed (status {status}, DLL {})",
                    self.plugin.path.display()
                ),
            ));
        }
        if output.flags & !(abi::DRAWING | abi::PANNING | abi::COLOR_CHANGED | abi::VIEW_CHANGED)
            != 0
            || output.flags & (abi::DRAWING | abi::PANNING) == (abi::DRAWING | abi::PANNING)
            || output.color > 0xffffff
        {
            return Err(ActionError::new(
                "tool_plugin",
                "Tool returned invalid flags or color",
            ));
        }
        if output.flags & abi::VIEW_CHANGED != 0
            && (!output.zoom.is_finite()
                || !(0.02..=32.).contains(&output.zoom)
                || !output.pan_dx.is_finite()
                || !output.pan_dy.is_finite()
                || (session.view.pan_x + output.pan_dx).abs() > 1e6
                || (session.view.pan_y + output.pan_dy).abs() > 1e6)
        {
            return Err(ActionError::new(
                "tool_plugin",
                "Tool returned an invalid view",
            ));
        }
        Ok(output)
    }
}
struct ToolContext {
    session: *mut Session,
    busy: bool,
    failed: bool,
    write_pixels: bool,
    mask_lease: Option<crate::engine::raster_masks::MaskLease>,
    stroke_opacity: bool,
    denied: Option<ActionError>,
}
unsafe extern "C" fn tool_pixels(
    context: *mut c_void,
    region: abi::Region,
    transform: Option<abi::Transform>,
    user: *mut c_void,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let context = context.cast::<ToolContext>();
    if (*context).busy {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        (*context).failed = true;
        return abi::ERROR;
    };
    let session = &mut *(*context).session;
    if !(*context).write_pixels || session.read_depth != 0 {
        (*context).failed = true;
        return abi::ERROR;
    }
    if (*context).mask_lease.is_some() {
        (*context).failed = true;
        (*context).denied = Some(ActionError::new(
            "tool_target",
            "An A8 mask contact cannot borrow mutable RGBA pixels",
        ));
        return abi::ERROR;
    }
    if let Err(error) = PixelAccess::Pixels.require(&session.doc, session.doc.active) {
        (*context).failed = true;
        (*context).denied = Some(error);
        return abi::ERROR;
    }
    let (x, y, w, h) = (
        region.x as usize,
        region.y as usize,
        region.width as usize,
        region.height as usize,
    );
    if session.doc.locked_region_empty(x, y, w, h) {
        return abi::OK;
    }
    if !session.doc.prepare_plugin_region(x, y, w, h) {
        (*context).failed = true;
        return abi::ERROR;
    }
    (*context).busy = true;
    let stride = session.doc.width;
    let offset = y * stride + x;
    let data = session.doc.layers[session.doc.active]
        .pixels
        .as_mut_ptr()
        .add(offset);
    let mut pixels = abi::Pixels {
        data,
        len: (h - 1) * stride + w,
        width: region.width,
        height: region.height,
        stride: stride as u32,
        origin_x: region.x,
        origin_y: region.y,
    };
    let status = transform(user, &mut pixels);
    (*context).busy = false;
    if status != abi::OK || !session.doc.locked_tool_region_valid(x, y, w, h) {
        (*context).failed = true;
        return abi::ERROR;
    }
    status
}
unsafe extern "C" fn tool_a8(
    context: *mut c_void,
    region: abi::Region,
    transform: Option<abi::A8Transform>,
    user: *mut c_void,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let context = context.cast::<ToolContext>();
    if (*context).busy {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        (*context).failed = true;
        return abi::ERROR;
    };
    let session = &mut *(*context).session;
    if !(*context).write_pixels || session.read_depth != 0 {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(lease) = (*context).mask_lease else {
        (*context).failed = true;
        (*context).denied = Some(ActionError::new(
            "tool_target",
            "An RGBA contact cannot borrow a mutable A8 mask",
        ));
        return abi::ERROR;
    };
    let mut pixels = match session.doc.mask_region(
        lease,
        region.x as usize,
        region.y as usize,
        region.width as usize,
        region.height as usize,
    ) {
        Ok(pixels) => pixels,
        Err(error) => {
            (*context).failed = true;
            (*context).denied = Some(ActionError::new("mask_lease", error));
            return abi::ERROR;
        }
    };
    (*context).busy = true;
    let status = transform(user, &mut pixels);
    (*context).busy = false;
    if status != abi::OK {
        (*context).failed = true;
    }
    status
}

unsafe extern "C" fn tool_pixels_origin(
    context: *mut c_void,
    region: abi::Region,
    transform: Option<abi::OriginTransform>,
    user: *mut c_void,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let context = context.cast::<ToolContext>();
    if (*context).busy {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        (*context).failed = true;
        return abi::ERROR;
    };
    let session = &mut *(*context).session;
    if !(*context).stroke_opacity || !(*context).write_pixels || session.read_depth != 0 {
        (*context).failed = true;
        return abi::ERROR;
    }
    if (*context).mask_lease.is_some() {
        (*context).failed = true;
        (*context).denied = Some(ActionError::new(
            "tool_target",
            "An A8 contact cannot borrow RGBA origins",
        ));
        return abi::ERROR;
    }
    if let Err(error) = PixelAccess::Pixels.require(&session.doc, session.doc.active) {
        (*context).failed = true;
        (*context).denied = Some(error);
        return abi::ERROR;
    }
    let (x, y, width, height) = (
        region.x as usize,
        region.y as usize,
        region.width as usize,
        region.height as usize,
    );
    if session.doc.locked_region_empty(x, y, width, height) {
        return abi::OK;
    }
    if !session.doc.prepare_plugin_region(x, y, width, height) {
        (*context).failed = true;
        return abi::ERROR;
    }
    // Snapshot the complete ROI once before any intersection callback. Original
    // Arc tile storage stays owned by the active Edit until the loan returns.
    (*context).busy = true;
    let stride = session.doc.width;
    let tile = crate::engine::TILE;
    let mut status = abi::OK;
    'tiles: for ty in y / tile..=(y + height - 1) / tile {
        for tx in x / tile..=(x + width - 1) / tile {
            let left = x.max(tx * tile);
            let top = y.max(ty * tile);
            let w = (x + width).min((tx + 1) * tile) - left;
            let h = (y + height).min((ty + 1) * tile) - top;
            let original = match session.doc.plugin_origin_region(left, top, w, h) {
                Ok(original) => original,
                Err(error) => {
                    (*context).denied = Some(ActionError::new("tool_origin", error));
                    status = abi::ERROR;
                    break 'tiles;
                }
            };
            let mut current = abi::Pixels {
                data: session.doc.layers[session.doc.active]
                    .pixels
                    .as_mut_ptr()
                    .add(top * stride + left),
                len: (h - 1) * stride + w,
                width: w as u32,
                height: h as u32,
                stride: stride as u32,
                origin_x: left as u32,
                origin_y: top as u32,
            };
            status = transform(user, &original, &mut current);
            if status != abi::OK || (*context).failed {
                status = abi::ERROR;
                break 'tiles;
            }
        }
    }
    (*context).busy = false;
    if status != abi::OK || !session.doc.locked_tool_region_valid(x, y, width, height) {
        (*context).failed = true;
        return abi::ERROR;
    }
    status
}

unsafe extern "C" fn tool_a8_origin(
    context: *mut c_void,
    region: abi::Region,
    transform: Option<abi::A8OriginTransform>,
    user: *mut c_void,
) -> i32 {
    if context.is_null() {
        return abi::ERROR;
    }
    let context = context.cast::<ToolContext>();
    if (*context).busy {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(transform) = transform else {
        (*context).failed = true;
        return abi::ERROR;
    };
    let session = &mut *(*context).session;
    if !(*context).stroke_opacity || !(*context).write_pixels || session.read_depth != 0 {
        (*context).failed = true;
        return abi::ERROR;
    }
    let Some(lease) = (*context).mask_lease else {
        (*context).failed = true;
        (*context).denied = Some(ActionError::new(
            "tool_target",
            "An RGBA contact cannot borrow A8 origins",
        ));
        return abi::ERROR;
    };
    let (x, y, width, height) = (
        region.x as usize,
        region.y as usize,
        region.width as usize,
        region.height as usize,
    );
    let full = match session.doc.mask_region(lease, x, y, width, height) {
        Ok(current) => current,
        Err(error) => {
            (*context).failed = true;
            (*context).denied = Some(ActionError::new("mask_lease", error));
            return abi::ERROR;
        }
    };
    (*context).busy = true;
    let tile = crate::engine::TILE;
    let stride = full.stride as usize;
    let mut status = abi::OK;
    'tiles: for ty in y / tile..=(y + height - 1) / tile {
        for tx in x / tile..=(x + width - 1) / tile {
            let left = x.max(tx * tile);
            let top = y.max(ty * tile);
            let w = (x + width).min((tx + 1) * tile) - left;
            let h = (y + height).min((ty + 1) * tile) - top;
            let original = match session.doc.mask_origin_region(lease, left, top, w, h) {
                Ok(original) => original,
                Err(error) => {
                    (*context).denied = Some(ActionError::new("mask_lease", error));
                    status = abi::ERROR;
                    break 'tiles;
                }
            };
            let mut current = abi::A8Pixels {
                data: full.data.add((top - y) * stride + left - x),
                len: (h - 1) * stride + w,
                width: w as u32,
                height: h as u32,
                stride: full.stride,
                origin_x: left as u32,
                origin_y: top as u32,
            };
            status = transform(user, &original, &mut current);
            if status != abi::OK || (*context).failed {
                status = abi::ERROR;
                break 'tiles;
            }
        }
    }
    (*context).busy = false;
    if status != abi::OK {
        (*context).failed = true;
    }
    status
}
unsafe extern "C" fn tool_pick(context: *mut c_void, x: f32, y: f32, color: *mut u32) -> i32 {
    if context.is_null() || color.is_null() {
        return abi::ERROR;
    }
    let context = context.cast::<ToolContext>();
    if (*context).busy {
        (*context).failed = true;
        return abi::ERROR;
    }
    let session = &mut *(*context).session;
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.
        || y < 0.
        || x >= session.doc.width as f32
        || y >= session.doc.height as f32
    {
        (*context).failed = true;
        return abi::ERROR;
    }
    session.doc.compose();
    let pixel = session.doc.composite[y as usize * session.doc.width + x as usize];
    if pixel >> 24 != 0 {
        *color = pixel & 0xffffff;
    }
    abi::OK
}

/// Cold transfer boundary: source and output are separate, tightly packed buffers.
pub(crate) fn transfer(
    s: &Session,
    options: &abi::SelectionTransfer,
    source: &[u32],
    size: [usize; 2],
    mask: Option<&crate::engine::selection::Mask>,
    output: &mut [u32],
    target: [usize; 2],
) -> Result<(), String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.selection_transfer)
        .ok_or("Painting DLL does not support pixel transfer")?;
    let mut src = abi::Pixels {
        data: source.as_ptr() as *mut u32,
        len: source.len(),
        width: size[0] as u32,
        height: size[1] as u32,
        stride: size[0] as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut dst = abi::Pixels {
        data: output.as_mut_ptr(),
        len: output.len(),
        width: target[0] as u32,
        height: target[1] as u32,
        stride: target[0] as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let coverage = mask.map(coverage).unwrap_or(abi::Coverage {
        data: std::ptr::null(),
        len: 0,
        width: 0,
        height: 0,
        stride: 0,
    });
    if unsafe { kernel(options, &mut src, &coverage, &mut dst) } != abi::OK {
        return Err("Native pixel transfer failed".into());
    }
    Ok(())
}

pub(crate) fn geometry_pixels(
    s: &Session,
    o: &abi::Geometry,
    source: &[u32],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
) -> Result<Vec<u32>, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.geometry)
        .and_then(|api| api.pixels)
        .ok_or("Painting DLL does not support exact geometry")?;
    let src = abi::Pixels {
        data: source.as_ptr() as *mut u32,
        len: source.len(),
        width: sw as u32,
        height: sh as u32,
        stride: sw as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut out = vec![0; w * h];
    let mut dst = abi::Pixels {
        data: out.as_mut_ptr(),
        len: out.len(),
        width: w as u32,
        height: h as u32,
        stride: w as u32,
        origin_x: 0,
        origin_y: 0,
    };
    if unsafe { kernel(o, &src, &mut dst) } != abi::OK {
        return Err("Native pixel geometry failed".into());
    }
    Ok(out)
}
pub(crate) fn geometry_mask(
    s: &Session,
    o: &abi::Geometry,
    mask: &crate::engine::selection::Mask,
    w: usize,
    h: usize,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.geometry)
        .and_then(|api| api.mask)
        .ok_or("Painting DLL does not support selection geometry")?;
    let mut out = vec![0; w * h];
    let src = coverage(mask);
    if unsafe { kernel(o, &src, out.as_mut_ptr(), out.len(), w as u32, h as u32) } != abi::OK {
        return Err("Native selection geometry failed".into());
    }
    crate::engine::selection::Mask::new(w, h, out)
}

fn resampler(s: &Session, o: &abi::Resample) -> Result<abi::ResampleApi, String> {
    let plugin = s
        .registry
        .native_plugins
        .get("painting")
        .ok_or("Painting DLL is unavailable")?;
    let api = plugin
        .resample
        .ok_or("Painting DLL does not support image resampling")?;
    if o.method == 2 && plugin.resample_features & abi::RESAMPLE_AREA == 0 {
        return Err(
            "Painting DLL does not support area resampling; rebuild matching plugins".into(),
        );
    }
    Ok(api)
}
pub(crate) fn resample_pixels(
    s: &Session,
    o: &abi::Resample,
    source: &[u32],
    sw: usize,
    sh: usize,
    w: usize,
    h: usize,
) -> Result<Vec<u32>, String> {
    let kernel = resampler(s, o)?
        .pixels
        .ok_or("Painting DLL does not support image resampling")?;
    let src = abi::Pixels {
        data: source.as_ptr() as *mut u32,
        len: source.len(),
        width: sw as u32,
        height: sh as u32,
        stride: sw as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut out = vec![0; w * h];
    let mut dst = abi::Pixels {
        data: out.as_mut_ptr(),
        len: out.len(),
        width: w as u32,
        height: h as u32,
        stride: w as u32,
        origin_x: 0,
        origin_y: 0,
    };
    if unsafe { kernel(o, &src, &mut dst) } != abi::OK {
        return Err("Native pixel resample failed".into());
    }
    Ok(out)
}
pub(crate) fn resample_mask(
    s: &Session,
    o: &abi::Resample,
    mask: &crate::engine::selection::Mask,
    w: usize,
    h: usize,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = resampler(s, o)?
        .mask
        .ok_or("Painting DLL does not support selection resample")?;
    let mut out = vec![0; w * h];
    let src = coverage(mask);
    if unsafe { kernel(o, &src, out.as_mut_ptr(), out.len(), w as u32, h as u32) } != abi::OK {
        return Err("Native selection resample failed".into());
    }
    crate::engine::selection::Mask::new(w, h, out)
}

pub(crate) fn trim_bounds(s: &mut Session, o: &abi::Trim) -> Result<abi::TrimBounds, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.trim)
        .and_then(|api| api.bounds)
        .ok_or("Painting DLL does not support automatic trim")?;
    s.doc.compose();
    if let Some(error) = &s.doc.render_error {
        return Err(format!("Cannot analyze merged artwork: {error}"));
    }
    let source = abi::Pixels {
        data: s.doc.composite.as_ptr() as *mut u32,
        len: s.doc.composite.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut out = abi::TrimBounds::default();
    if unsafe { kernel(o, &source, &mut out) } != abi::OK {
        return Err("Native trim analysis failed".into());
    }
    if out.width == 0 && out.height == 0 && out.x == 0 && out.y == 0 {
        return Ok(out);
    }
    if out.width == 0
        || out.height == 0
        || u64::from(out.x) + u64::from(out.width) > s.doc.width as u64
        || u64::from(out.y) + u64::from(out.height) > s.doc.height as u64
    {
        return Err("Native trim returned invalid bounds".into());
    }
    Ok(out)
}

/// Cold all-output preflight, before allocating either half of an owner move.
pub(crate) fn require_movement(s: &Session, pixels: bool, selection: bool) -> Result<(), String> {
    let movement = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|plugin| plugin.movement);
    if pixels && movement.and_then(|api| api.pixels).is_none() {
        return Err("Painting DLL does not support integer movement".into());
    }
    if selection && movement.and_then(|api| api.mask).is_none() {
        return Err("Painting DLL does not support selection movement".into());
    }
    Ok(())
}

pub(crate) fn move_pixels(s: &Session, o: &abi::Move) -> Result<Vec<u32>, String> {
    move_layer_pixels(s, s.doc.active, o)
}
/// Cold immutable-source sampling without changing active or selected targets.
pub(crate) fn move_layer_pixels(
    s: &Session,
    index: usize,
    o: &abi::Move,
) -> Result<Vec<u32>, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.movement)
        .and_then(|a| a.pixels)
        .ok_or("Painting DLL does not support integer movement")?;
    let pixels = &s
        .doc
        .layers
        .get(index)
        .ok_or("Layer does not exist")?
        .pixels;
    let source = abi::Pixels {
        data: pixels.as_ptr() as *mut u32,
        len: pixels.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut result = vec![0; pixels.len()];
    let mut output = abi::Pixels {
        data: result.as_mut_ptr(),
        len: result.len(),
        ..source
    };
    let mask = if o.kind == 1 {
        coverage(
            s.doc
                .selection()
                .current
                .as_ref()
                .ok_or("Selected movement requires an active selection")?,
        )
    } else {
        abi::Coverage {
            data: std::ptr::null(),
            len: 0,
            width: 0,
            height: 0,
            stride: 0,
        }
    };
    if unsafe { kernel(o, &source, &mask, &mut output) } != abi::OK {
        return Err("Native pixel movement failed".into());
    }
    Ok(result)
}
pub(crate) fn move_mask(
    s: &Session,
    o: &abi::Move,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.movement)
        .and_then(|a| a.mask)
        .ok_or("Painting DLL does not support selection movement")?;
    let mask = coverage(
        s.doc
            .selection()
            .current
            .as_ref()
            .ok_or("Selection movement requires an active selection")?,
    );
    let mut result = vec![0; s.doc.width * s.doc.height];
    if unsafe { kernel(o, &mask, result.as_mut_ptr(), result.len()) } != abi::OK {
        return Err("Native selection movement failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, result)
}

pub(crate) fn affine_source(
    s: &Session,
    o: &abi::Affine,
    pixels: &[u32],
) -> Result<Vec<u32>, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.affine)
        .and_then(|a| a.pixels)
        .ok_or("Painting DLL does not support affine sampling")?;
    if pixels.len() != s.doc.width * s.doc.height {
        return Err("Invalid affine source dimensions".into());
    }
    let source = abi::Pixels {
        data: pixels.as_ptr() as *mut u32,
        len: pixels.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut result = vec![0; pixels.len()];
    let mut output = abi::Pixels {
        data: result.as_mut_ptr(),
        len: result.len(),
        ..source
    };
    let mask = if o.kind == 1 {
        coverage(
            s.doc
                .selection()
                .current
                .as_ref()
                .ok_or("Selected movement requires an active selection")?,
        )
    } else {
        abi::Coverage {
            data: std::ptr::null(),
            len: 0,
            width: 0,
            height: 0,
            stride: 0,
        }
    };
    if unsafe { kernel(o, &source, &mask, &mut output) } != abi::OK {
        return Err("Native affine pixel sampling failed".into());
    }
    Ok(result)
}
pub(crate) fn projective_source(
    s: &Session,
    o: &abi::Projective,
    pixels: &[u32],
) -> Result<Vec<u32>, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.projective)
        .and_then(|a| a.pixels)
        .ok_or("Painting DLL does not support projective sampling")?;
    if pixels.len() != s.doc.width * s.doc.height {
        return Err("Invalid projective source dimensions".into());
    }
    let source = abi::Pixels {
        data: pixels.as_ptr() as *mut u32,
        len: pixels.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut result = vec![0; pixels.len()];
    let mut output = abi::Pixels {
        data: result.as_mut_ptr(),
        len: result.len(),
        ..source
    };
    let mask = if o.kind == 1 {
        coverage(
            s.doc
                .selection()
                .current
                .as_ref()
                .ok_or("Selected movement requires an active selection")?,
        )
    } else {
        abi::Coverage {
            data: std::ptr::null(),
            len: 0,
            width: 0,
            height: 0,
            stride: 0,
        }
    };
    if unsafe { kernel(o, &source, &mask, &mut output) } != abi::OK {
        return Err("Native projective pixel sampling failed".into());
    }
    Ok(result)
}
pub(crate) fn affine_mask(
    s: &Session,
    o: &abi::Affine,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.affine)
        .and_then(|a| a.mask)
        .ok_or("Painting DLL does not support affine mask sampling")?;
    let mask = coverage(
        s.doc
            .selection()
            .current
            .as_ref()
            .ok_or("Selection movement requires an active selection")?,
    );
    let mut result = vec![0; s.doc.width * s.doc.height];
    if unsafe { kernel(o, &mask, result.as_mut_ptr(), result.len()) } != abi::OK {
        return Err("Native affine mask sampling failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, result)
}
pub(crate) fn projective_mask(
    s: &Session,
    o: &abi::Projective,
) -> Result<crate::engine::selection::Mask, String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.projective)
        .and_then(|a| a.mask)
        .ok_or("Painting DLL does not support projective mask sampling")?;
    let mask = coverage(
        s.doc
            .selection()
            .current
            .as_ref()
            .ok_or("Selection movement requires an active selection")?,
    );
    let mut result = vec![0; s.doc.width * s.doc.height];
    if unsafe { kernel(o, &mask, result.as_mut_ptr(), result.len()) } != abi::OK {
        return Err("Native projective mask sampling failed".into());
    }
    crate::engine::selection::Mask::new(s.doc.width, s.doc.height, result)
}

pub(crate) fn affine_bounds(s: &Session) -> Result<[usize; 4], String> {
    let kernel = s
        .registry
        .native_plugins
        .get("painting")
        .and_then(|p| p.affine)
        .and_then(|p| p.bounds)
        .ok_or("Painting DLL does not support affine content bounds")?;
    let data = &s.doc.layers[s.doc.active].pixels;
    let source = abi::Pixels {
        data: data.as_ptr() as *mut u32,
        len: data.len(),
        width: s.doc.width as u32,
        height: s.doc.height as u32,
        stride: s.doc.width as u32,
        origin_x: 0,
        origin_y: 0,
    };
    let mut out = abi::TrimBounds::default();
    if unsafe { kernel(&source, &mut out) } != abi::OK {
        return Err("Native transform bounds failed".into());
    }
    if out == abi::TrimBounds::default() {
        return Err("Layer transform requires nontransparent pixels".into());
    }
    if out.width == 0
        || out.height == 0
        || u64::from(out.x) + u64::from(out.width) > s.doc.width as u64
        || u64::from(out.y) + u64::from(out.height) > s.doc.height as u64
    {
        return Err("Native transform returned invalid content bounds".into());
    }
    Ok([
        out.x as usize,
        out.y as usize,
        (out.x + out.width) as usize,
        (out.y + out.height) as usize,
    ])
}

#[cfg(test)]
impl NativePlugin {
    pub(crate) fn replace_a8_sampler_for_test(
        &mut self,
        api: Option<abi::A8SamplerApi>,
    ) -> Option<abi::A8SamplerApi> {
        std::mem::replace(&mut self.a8_sampler, api)
    }

    #[cfg(test)]
    pub(crate) fn replace_movement_pixels_for_test(
        &mut self,
        pixels: Option<abi::MovePixelsFn>,
    ) -> Option<abi::MovePixelsFn> {
        self.movement
            .as_mut()
            .and_then(|api| std::mem::replace(&mut api.pixels, pixels))
    }
}
