//! MiCoPi native ABI v1. No Rust-owned types or allocations cross this boundary.
//!
//! All calls are synchronous on the session's thread. Borrowed strings, host APIs,
//! reply contexts and pixel buffers must not be retained, freed or used on another
//! thread. Plugins own their instance allocation and release it in `destroy`.
//! An extension may explicitly grant a longer immutable scoped borrow,
//! as with Coverage in ToolOptionsSelection/HostApiSelection; see their contracts.
//! Every exported callback must catch its own unwinding panics. Native plugins are
//! trusted code in the host process, not a sandbox.
use std::ffi::c_void;
pub const ABI_VERSION: u32 = 1;
pub const ENTRY_SYMBOL: &[u8] = b"micopi_plugin_v1\0";
pub const OK: i32 = 0;
pub const ERROR: i32 = 1;
pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Bytes {
    pub data: *const u8,
    pub len: usize,
}
impl Bytes {
    pub const fn borrowed(bytes: &[u8]) -> Self {
        Self {
            data: bytes.as_ptr(),
            len: bytes.len(),
        }
    }
    /// # Safety
    /// The producer must provide readable bytes for the duration of the borrow.
    pub unsafe fn as_slice<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(self.data, self.len)
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Reply {
    pub context: *mut c_void,
    /// Copies bytes immediately; return nonzero to reject oversized output.
    pub write: unsafe extern "C" fn(*mut c_void, Bytes) -> i32,
}
impl Reply {
    /// # Safety
    /// Reply must be the live callback supplied to the current invocation.
    pub unsafe fn send(self, bytes: &[u8]) -> i32 {
        (self.write)(self.context, Bytes::borrowed(bytes))
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[repr(C)]
pub struct Pixels {
    pub data: *mut u32,
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}
pub type Transform = unsafe extern "C" fn(*mut c_void, *mut Pixels) -> i32;
#[repr(C)]
pub struct HostApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub context: *mut c_void,
    /// Dispatch through the shared pipeline. Cross-plugin calls are supported;
    /// cycles/same-instance reentry and writes from read-only scopes are rejected.
    pub dispatch: unsafe extern "C" fn(*mut c_void, Bytes, Bytes, Reply) -> i32,
    /// Transform a contiguous straight-alpha 0xAARRGGBB region of the active layer.
    /// A successful callback commits one undoable edit; a failed callback commits
    /// nothing. Host calls from inside the transform callback are rejected.
    pub edit_region:
        unsafe extern "C" fn(*mut c_void, Region, Option<Transform>, *mut c_void) -> i32,
}
/// Optional immutable selection snapshot frozen at native action entry. The host
/// retains its coverage allocation until invoke returns, including nested calls.
#[repr(C)]
pub struct HostApiSelection {
    pub base: HostApi,
    pub selection: unsafe extern "C" fn(*mut c_void, *mut Coverage) -> i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Header {
    pub abi_version: u32,
    pub struct_size: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginApi {
    pub abi_version: u32,
    pub struct_size: u32,
    /// Returns a static UTF-8 descriptor: {id, actions, tools, panels}.
    pub describe: Option<unsafe extern "C" fn() -> Bytes>,
    /// Optional paired callbacks; create one instance per document Session.
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    /// Write exactly one JSON result (or JSON error on nonzero status).
    pub invoke:
        Option<unsafe extern "C" fn(*mut c_void, Bytes, Bytes, *const HostApi, Reply) -> i32>,
}
pub type Entry = unsafe extern "C" fn() -> *const PluginApi;

/// Convenience receiver for plugin-side host calls. Memory remains plugin-owned.
pub unsafe extern "C" fn collect(context: *mut c_void, bytes: Bytes) -> i32 {
    if context.is_null() || bytes.len > MAX_MESSAGE || (bytes.len != 0 && bytes.data.is_null()) {
        return ERROR;
    }
    let output = &mut *(context as *mut Vec<u8>);
    if output.len().saturating_add(bytes.len) > MAX_MESSAGE {
        return ERROR;
    }
    output.extend_from_slice(bytes.as_slice());
    OK
}

// Additive tool extension. The action ABI above is unchanged.
pub const TOOL_ENTRY_SYMBOL: &[u8] = b"micopi_tools_v1\0";
pub const TOOL_ABI_VERSION: u32 = 1;
pub const DOWN: u32 = 0;
pub const MOVE: u32 = 1;
pub const UP: u32 = 2;
pub const CANCEL: u32 = 3;
pub const PEN: u32 = 1;
pub const ERASER: u32 = 2;
pub const PRESSURE_SIZE: u32 = 1;
pub const PRESSURE_OPACITY: u32 = 2;
pub const DRAWING: u32 = 1;
pub const PANNING: u32 = 2;
pub const COLOR_CHANGED: u32 = 4;
pub const VIEW_CHANGED: u32 = 8;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ToolParameters {
    pub radius: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub color: u32,
    pub stabilizer: f32,
    pub pressure_gamma: f32,
    pub minimum_radius: f32,
    pub flags: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ToolSample {
    pub phase: u32,
    pub flags: u32,
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub time: u32,
    pub view_x: f32,
    pub view_y: f32,
    pub width: u32,
    pub height: u32,
    pub zoom: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ToolOutput {
    pub flags: u32,
    pub color: u32,
    pub zoom: f32,
    pub pan_dx: f32,
    pub pan_dy: f32,
}
#[repr(C)]
pub struct ToolHost {
    pub abi_version: u32,
    pub struct_size: u32,
    pub context: *mut c_void,
    /// Direct strided pixels, scoped to this callback; undo tiles are captured once
    /// per stroke before exposing memory. No region-sized copies or JSON.
    /// Write only the stated width/height, not the row gaps. Any error aborts the
    /// entire tool stroke and restores captured tiles; do not retain pointers.
    pub pixels: unsafe extern "C" fn(*mut c_void, Region, Option<Transform>, *mut c_void) -> i32,
    pub pick: unsafe extern "C" fn(*mut c_void, f32, f32, *mut u32) -> i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ToolApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub create: Option<unsafe extern "C" fn() -> *mut c_void>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    /// parameters is present on down only; the plugin freezes it until up/cancel.
    pub pointer: Option<
        unsafe extern "C" fn(
            *mut c_void,
            u32,
            *const ToolSample,
            *const ToolParameters,
            *const ToolHost,
            *mut ToolOutput,
        ) -> i32,
    >,
}
pub type ToolEntry = unsafe extern "C" fn() -> *const ToolApi;

/// Optional stroke options; independent of the unchanged v1 tool tables.
pub const TOOL_OPTIONS_SYMBOL: &[u8] = b"micopi_tool_options_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ToolOptions {
    pub abi_version: u32,
    pub struct_size: u32,
    /// Dab distance as a fraction of brush diameter; legacy default is 0.11.
    pub spacing: f32,
    pub minimum_opacity: f32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ToolOptionsApi {
    pub abi_version: u32,
    pub struct_size: u32,
    /// Called once before DOWN, on the state returned by ToolApi::create.
    /// No host callbacks; reject changes during an active contact.
    pub configure: Option<unsafe extern "C" fn(*mut c_void, *const ToolOptions) -> i32>,
}
pub type ToolOptionsEntry = unsafe extern "C" fn() -> *const ToolOptionsApi;
pub const TOOL_OPTION_ALPHA_LOCK: u32 = 1;
pub const TOOL_OPTION_SELECTION: u32 = 2;
/// Borrowed full-canvas 8-bit coverage, frozen until the contact ends.
/// A null pointer with all other fields zero means unrestricted painting.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Coverage {
    pub data: *const u8,
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}
impl Default for Coverage {
    fn default() -> Self {
        Self {
            data: std::ptr::null(),
            len: 0,
            width: 0,
            height: 0,
            stride: 0,
        }
    }
}
/// Additive tails; read only when the prefix struct_size covers the whole tail.
#[repr(C)]
pub struct ToolOptionsExt {
    pub base: ToolOptions,
    pub flags: u32,
}
#[repr(C)]
pub struct ToolOptionsSelection {
    pub base: ToolOptionsExt,
    pub selection: Coverage,
}
/// Optional cold-path selection blending. Both pixel regions share coordinates;
/// `before` is immutable and must not alias mutable `after` storage.
pub const SELECTION_SYMBOL: &[u8] = b"micopi_selection_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectionApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub mix: Option<unsafe extern "C" fn(*const Coverage, *const Pixels, *mut Pixels) -> i32>,
    pub rasterize: Option<
        unsafe extern "C" fn(
            *const SelectionShape,
            *const Coverage,
            *mut u8,
            usize,
            u32,
            u32,
        ) -> i32,
    >,
}
/// Kinds: 0 rectangle, 1 ellipse, 2 polygon, 3 all, 4 invert previous coverage.
/// Modes: 0 replace, 1 maximum/add, 2 saturating subtract, 3 minimum/intersect.
#[repr(C)]
pub struct SelectionShape {
    pub kind: u32,
    pub mode: u32,
    pub bounds: [f32; 4],
    pub points: *const [f32; 2],
    pub count: usize,
}
pub type SelectionEntry = unsafe extern "C" fn() -> *const SelectionApi;
/// Optional tail; the SelectionApi prefix and its function layouts stay unchanged.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectionApiColor {
    pub base: SelectionApi,
    pub color: Option<SelectionColorFn>,
}
/// Optional fixed-size tail after SelectionApiColor; preserve both prefixes.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectionApiEdges {
    pub base: SelectionApiColor,
    pub edges: Option<SelectionEdgesFn>,
}
/// Optional fixed-size pixel transfer tail; all earlier table layouts are retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SelectionApiTransfer {
    pub base: SelectionApiEdges,
    pub transfer: Option<SelectionTransferFn>,
}
/// Kind 0 extracts a bounded source rectangle and multiplies alpha by coverage.
/// Kind 1 places a source rectangle, clipping to output bounds; coverage must be absent.
/// Buffers must not overlap. Pixels use local coordinates (origins must be zero).
#[repr(C)]
pub struct SelectionTransfer {
    pub kind: u32,
    pub x: i32,
    pub y: i32,
}
pub type SelectionTransferFn = unsafe extern "C" fn(
    *const SelectionTransfer,
    *const Pixels,
    *const Coverage,
    *mut Pixels,
) -> i32;
pub type SelectionEdgesFn =
    unsafe extern "C" fn(*const SelectionEdges, *const Coverage, *mut u8, usize) -> i32;
/// Kinds: 0 disk expansion, 1 disk contraction, 2 feather, 3 smooth.
/// Radius: 1..=256. Edge: 0 extend, 1 zero coverage beyond the canvas.
#[repr(C)]
pub struct SelectionEdges {
    pub kind: u32,
    pub radius: u32,
    pub edge: u32,
}
pub type SelectionColorFn = unsafe extern "C" fn(
    *const SelectionColor,
    *const Pixels,
    *const Coverage,
    *mut u8,
    usize,
) -> i32;
/// Kinds: 0 seeded color selection, 1 Grow, 2 Similar. Modes match SelectionShape;
/// Grow/Similar require mode 1 and preserve existing positive coverage.
#[repr(C)]
pub struct SelectionColor {
    pub kind: u32,
    pub mode: u32,
    pub x: u32,
    pub y: u32,
    pub tolerance: u32,
    pub contiguous: u32,
}
#[repr(C)]
pub struct ToolOptionsApiExt {
    pub base: ToolOptionsApi,
    pub features: u32,
}

/// Additive synchronous compositor extension. Input layers are immutable;
/// output is a separate strided ARGB region. No pointer may outlive the call.
pub const COMPOSITOR_SYMBOL: &[u8] = b"micopi_compositor_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeLayer {
    pub data: *const u32,
    pub len: usize,
    pub opacity: u32,
    pub visible: u32,
    pub mode: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeNode {
    pub kind: u32, // 0 layer, 1 filter, 2 blend, 3 output
    pub a: u32,
    pub b: u32,
    pub mode: u32,
    pub amount: f32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositorApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub render: Option<
        unsafe extern "C" fn(
            *const CompositeNode,
            usize,
            *const CompositeLayer,
            usize,
            u32,
            u32,
            *mut Pixels,
        ) -> i32,
    >,
}
pub type CompositorEntry = unsafe extern "C" fn() -> *const CompositorApi;

/// Independent optional exact geometry extension. Existing selection/tool tables are unchanged.
pub const GEOMETRY_SYMBOL: &[u8] = b"micopi_geometry_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct GeometryApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub pixels: Option<GeometryPixelsFn>,
    pub mask: Option<GeometryMaskFn>,
}
/// Kinds: 0 clockwise quarter turn, 1 counterclockwise, 2 half turn,
/// 3 horizontal flip, 4 vertical flip. Offsets are signed destination pixels,
/// bounded to +/-4096. Uncovered target pixels become zero; excess is clipped.
#[repr(C)]
pub struct Geometry {
    pub kind: u32,
    pub x: i32,
    pub y: i32,
}
pub type GeometryPixelsFn =
    unsafe extern "C" fn(*const Geometry, *const Pixels, *mut Pixels) -> i32;
pub type GeometryMaskFn =
    unsafe extern "C" fn(*const Geometry, *const Coverage, *mut u8, usize, u32, u32) -> i32;
pub type GeometryEntry = unsafe extern "C" fn() -> *const GeometryApi;

/// Independent optional image resampling extension. All callbacks are synchronous.
pub const RESAMPLE_SYMBOL: &[u8] = b"micopi_resample_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResampleApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub pixels: Option<ResamplePixelsFn>,
    pub mask: Option<ResampleMaskFn>,
}
/// Optional area-filter capability; legacy ResampleApi supports only methods 0/1.
pub const RESAMPLE_AREA: u32 = 1 << 2;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ResampleApiExt {
    pub base: ResampleApi,
    pub features: u32,
}
/// Method: 0 nearest pixel center, 1 bilinear, 2 area on shrinking axes / bilinear otherwise.
/// Interpolating methods use premultiplied alpha in encoded RGB.
#[repr(C)]
pub struct Resample {
    pub method: u32,
}
pub type ResamplePixelsFn =
    unsafe extern "C" fn(*const Resample, *const Pixels, *mut Pixels) -> i32;
pub type ResampleMaskFn =
    unsafe extern "C" fn(*const Resample, *const Coverage, *mut u8, usize, u32, u32) -> i32;
pub type ResampleEntry = unsafe extern "C" fn() -> *const ResampleApi;

/// Independent read-only automatic trim analysis. No pointer survives a call.
pub const TRIM_SYMBOL: &[u8] = b"micopi_trim_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TrimApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub bounds: Option<TrimBoundsFn>,
}
/// Mode 0 alpha <= tolerance, 1 top-left RGBA, 2 bottom-right RGBA.
/// Color modes match max per-channel absolute difference <= tolerance (0..255).
/// Both zero-alpha colors match regardless of hidden RGB. Sides: top=1,
/// right=2, bottom=4, left=8; at least one side must be selected.
#[repr(C)]
pub struct Trim {
    pub mode: u32,
    pub tolerance: u32,
    pub sides: u32,
}
/// Zero width AND height means all pixels match. Otherwise an in-bounds frame.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrimBounds {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
pub type TrimBoundsFn = unsafe extern "C" fn(*const Trim, *const Pixels, *mut TrimBounds) -> i32;
pub type TrimEntry = unsafe extern "C" fn() -> *const TrimApi;

/// Optional integer movement; synchronous immutable sources and disjoint outputs.
pub const MOVE_SYMBOL: &[u8] = b"micopi_move_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MoveApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub pixels: Option<MovePixelsFn>,
    pub mask: Option<MoveMaskFn>,
}
/// Kind 0 whole layer (absent coverage), 1 selected pixels, 2 mask only.
/// Offsets are integer document pixels within +/-4096; excess is clipped.
#[repr(C)]
pub struct Move {
    pub kind: u32,
    pub dx: i32,
    pub dy: i32,
}
pub type MovePixelsFn =
    unsafe extern "C" fn(*const Move, *const Pixels, *const Coverage, *mut Pixels) -> i32;
pub type MoveMaskFn = unsafe extern "C" fn(*const Move, *const Coverage, *mut u8, usize) -> i32;
pub type MoveEntry = unsafe extern "C" fn() -> *const MoveApi;

/// Optional frame-bounded affine sampling with immutable sources and disjoint outputs.
pub const AFFINE_SYMBOL: &[u8] = b"micopi_affine_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AffineApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub pixels: Option<AffinePixelsFn>,
    pub mask: Option<AffineMaskFn>,
    pub bounds: Option<AffineBoundsFn>,
}
/// Kind 0 layer, 1 selected pixels, 2 mask only. Method 0 nearest, 1 bilinear.
/// Forward edge coordinates: x'=m0*x+m1*y+m2; y'=m3*x+m4*y+m5.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Affine {
    pub kind: u32,
    pub method: u32,
    pub matrix: [f64; 6],
}
pub type AffinePixelsFn =
    unsafe extern "C" fn(*const Affine, *const Pixels, *const Coverage, *mut Pixels) -> i32;
pub type AffineMaskFn = unsafe extern "C" fn(*const Affine, *const Coverage, *mut u8, usize) -> i32;
pub type AffineEntry = unsafe extern "C" fn() -> *const AffineApi;
pub type AffineBoundsFn = unsafe extern "C" fn(*const Pixels, *mut TrimBounds) -> i32;
