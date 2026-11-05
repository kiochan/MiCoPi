//! Versioned native-code plugin loader and synchronous host capabilities.
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
);
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
    Ok((api, color, edges))
}
impl Module {
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
    unsafe fn compositor_api(&self) -> Result<Option<abi::CompositorApi>, String> {
        #[cfg(windows)]
        {
            let address =
                GetProcAddress(self.handle as *mut c_void, abi::COMPOSITOR_SYMBOL.as_ptr());
            if address.is_null() {
                return Ok(None);
            }
            let entry = std::mem::transmute::<*mut c_void, abi::CompositorEntry>(address);
            let pointer = entry();
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
            Ok(Some(api))
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
    compositor: Option<abi::CompositorApi>,
    _module: Module,
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
        let tools = unsafe { module.tool_api()? };
        let tool_options = unsafe { module.tool_options_api()? };
        let (selection_mix, selection_color, selection_edges) =
            match unsafe { module.selection_api()? } {
                Some((base, color, edges)) => (Some(base), color, edges),
                None => (None, None, None),
            };
        if tool_options.is_some() && tools.is_none() {
            return Err("Tool options require a tool API".into());
        }
        let compositor = unsafe { module.compositor_api()? };
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
                compositor,
                _module: module,
            }),
            contributions,
        ))
    }
}
impl crate::engine::graph::Renderer for NativePlugin {
    fn render(
        &self,
        nodes: &[abi::CompositeNode],
        layers: &[abi::CompositeLayer],
        width: u32,
        height: u32,
        output: &mut abi::Pixels,
    ) -> Result<(), String> {
        let api = self.compositor.ok_or("Missing compositor interface")?;
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
    fn invoke(&mut self, session: &mut Session, handler: &str, args: Value) -> ActionResult {
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
    let result = instance.invoke(session, &handler, args);
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
        Err(error) => (abi::ERROR, json!({"error":error})),
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
        || !session.doc.layers[session.doc.active].visible
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
        session.revision += 1;
    }
    (*host).busy = false;
    status
}
#[cfg(test)]
mod tests {
    use super::*;
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
            let (base, tail, edges) = decode_selection_table(&legacy).unwrap();
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
    #[test]
    fn pixel_callback_failure_is_atomic_and_success_is_undoable() {
        let mut session = Session::new(super::super::Registry::bundled()).unwrap();
        session.doc = crate::engine::Document::new(8, 8);
        let mut context = HostContext {
            session: &mut session,
            busy: false,
            selection: None,
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

/// Cached for the document session; moved into a contact for allocation-free moves.
pub(crate) struct ToolInstance {
    selection: Option<crate::engine::selection::Mask>,
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
        })
    }
    pub(crate) fn pointer(
        &mut self,
        session: &mut Session,
        handler: u32,
        sample: abi::ToolSample,
        parameters: Option<&abi::ToolParameters>,
        painting: bool,
    ) -> Result<abi::ToolOutput, ActionError> {
        if parameters.is_some() && painting {
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
            let locked = session.doc.alpha_locked(session.doc.active);
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
                };
                let status = if features & abi::TOOL_OPTION_SELECTION != 0 {
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
                } else if features & abi::TOOL_OPTION_ALPHA_LOCK != 0 {
                    let extended = abi::ToolOptionsExt {
                        base: abi::ToolOptions {
                            struct_size: std::mem::size_of::<abi::ToolOptionsExt>() as u32,
                            ..options
                        },
                        flags: if locked {
                            abi::TOOL_OPTION_ALPHA_LOCK
                        } else {
                            0
                        },
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
        }
        let mut context = ToolContext {
            session,
            busy: false,
            failed: false,
        };
        let host = abi::ToolHost {
            abi_version: abi::TOOL_ABI_VERSION,
            struct_size: std::mem::size_of::<abi::ToolHost>() as u32,
            context: (&mut context as *mut ToolContext).cast(),
            pixels: tool_pixels,
            pick: tool_pick,
        };
        let mut output = abi::ToolOutput::default();
        let status = unsafe {
            (self.api.pointer.unwrap())(
                self.state,
                handler,
                &sample,
                parameters.map_or(std::ptr::null(), |p| p as *const _),
                &host,
                &mut output,
            )
        };
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
