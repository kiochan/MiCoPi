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
impl Module {
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
        let mut context = HostContext {
            session,
            busy: false,
        };
        let host = abi::HostApi {
            abi_version: abi::ABI_VERSION,
            struct_size: std::mem::size_of::<abi::HostApi>() as u32,
            context: (&mut context as *mut HostContext).cast(),
            dispatch,
            edit_region,
        };
        let mut output = Output::default();
        let reply = output.reply();
        let status = unsafe {
            (self.plugin.api.invoke.unwrap())(
                self.state,
                abi::Bytes::borrowed(handler.as_bytes()),
                abi::Bytes::borrowed(&args),
                &host,
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
    if status == abi::OK && session.doc.replace_region(x, y, w, h, &pixels) {
        session.revision += 1;
    }
    (*host).busy = false;
    status
}
#[cfg(test)]
mod tests {
    use super::*;
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
        Ok(Self { plugin, state, api })
    }
    pub(crate) fn pointer(
        &mut self,
        session: &mut Session,
        handler: u32,
        sample: abi::ToolSample,
        parameters: Option<&abi::ToolParameters>,
    ) -> Result<abi::ToolOutput, ActionError> {
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
