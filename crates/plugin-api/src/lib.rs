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
/// Mutable one-byte coverage region. Stride and length are measured in bytes;
/// write only width bytes per row. The borrow lasts for one transform callback.
#[repr(C)]
pub struct A8Pixels {
    pub data: *mut u8,
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}
/// Immutable contact-origin RGBA samples from an already captured undo tile.
/// Stride may differ from the paired current region. Read only width samples
/// per row; no pointer or descriptor may outlive the synchronous callback.
#[repr(C)]
pub struct OriginPixels {
    pub data: *const u32,
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}
/// Immutable raw A8 contact-origin samples; never effective mask coverage.
#[repr(C)]
pub struct OriginA8Pixels {
    pub data: *const u8,
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub origin_x: u32,
    pub origin_y: u32,
}
pub type Transform = unsafe extern "C" fn(*mut c_void, *mut Pixels) -> i32;
pub type A8Transform = unsafe extern "C" fn(*mut c_void, *mut A8Pixels) -> i32;
pub type OriginTransform =
    unsafe extern "C" fn(*mut c_void, *const OriginPixels, *mut Pixels) -> i32;
pub type A8OriginTransform =
    unsafe extern "C" fn(*mut c_void, *const OriginA8Pixels, *mut A8Pixels) -> i32;
pub type OriginPixelsLoan =
    unsafe extern "C" fn(*mut c_void, Region, Option<OriginTransform>, *mut c_void) -> i32;
pub type OriginA8Loan =
    unsafe extern "C" fn(*mut c_void, Region, Option<A8OriginTransform>, *mut c_void) -> i32;
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
/// Optional complete tool-host tail for TOOL_OPTION_A8_TARGET contacts. This
/// never redirects the legacy RGBA callback. The host snapshots byte tiles before
/// lending storage and rolls back the entire contact on any callback failure.
#[repr(C)]
pub struct ToolHostA8 {
    pub base: ToolHost,
    pub a8:
        Option<unsafe extern "C" fn(*mut c_void, Region, Option<A8Transform>, *mut c_void) -> i32>,
}
pub const TOOL_HOST_ORIGIN_RGBA: u32 = 1;
pub const TOOL_HOST_ORIGIN_A8: u32 = 2;
/// Optional complete tail for capped stroke contacts. Read only when the base
/// header covers this entire struct, the matching feature is set and its loan
/// is present. The host snapshots the full requested ROI once, then calls the
/// transform for each undo-tile intersection. Origins and current regions have
/// matching coordinates, disjoint storage and potentially different strides.
/// All loans are synchronous, non-reentrant and bound to the admitted target.
#[repr(C)]
pub struct ToolHostStroke {
    pub base: ToolHostA8,
    pub features: u32,
    pub tile_side: u32,
    pub pixels_origin: Option<OriginPixelsLoan>,
    pub a8_origin: Option<OriginA8Loan>,
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
/// ToolOptionsApiExt capability and ToolOptionsExt flag. Freeze the A8 target at
/// DOWN, require a complete ToolHostA8 before touching storage, and reject its
/// combination with alpha lock. Encoded-RGB grayscale is (54R+183G+19B+128)>>8;
/// erasing reveals 255. Dab mode scales each deposit by selection; capped
/// stroke mode applies selection once to cumulative influence before byte mixing.
pub const TOOL_OPTION_A8_TARGET: u32 = 4;
/// ToolOptionsApiExt capability for independent Flow and capped stroke opacity.
/// As an options flag, selects capped stroke mode and requires ToolHostStroke.
/// Nondefault Flow requires the complete ToolOptionsStroke even in dab mode.
pub const TOOL_OPTION_STROKE_OPACITY: u32 = 8;
/// Optional analytic tip geometry in a complete ToolOptionsTip. The flag marks
/// the tail as meaningful; capability absence permits only legacy round tips.
pub const TOOL_OPTION_TIP_GEOMETRY: u32 = 16;
pub const TIP_ROUND: u32 = 0;
pub const TIP_SQUARE: u32 = 1;
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
/// Additive options negotiated by TOOL_OPTION_STROKE_OPACITY capability. Flow
/// is finite 0..1; reserved must be zero. Capped mode is the flags bit, while
/// flow-only dab mode uses this tail without that bit. Legacy calls reset to
/// flow 1 / dab mode. Configure once before DOWN; borrowed selection stays frozen.
#[repr(C)]
pub struct ToolOptionsStroke {
    pub base: ToolOptionsSelection,
    pub flow: f32,
    pub reserved: u32,
}
/// Additive fixed contact geometry. Radius is the major half-diameter; roundness
/// is the minor/major ratio in 0.25..=1, angle is clockwise document degrees in
/// -180..=180, and reserved must be zero. Both axes scale with pressure. Legacy
/// option prefixes reset to round/1/0. This tail adds no host callbacks or loans.
#[repr(C)]
pub struct ToolOptionsTip {
    pub base: ToolOptionsStroke,
    pub tip_shape: u32,
    pub roundness: f32,
    pub angle: f32,
    pub reserved: u32,
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
/// Immutable full-canvas coverage for the optional masked renderer. A null data
/// pointer requires len=stride=0 and constant in 0..=255. Otherwise constant=0,
/// stride is at least canvas width and len covers the final row. Storage must not
/// alias mutable output and no pointer may outlive the render call.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeMask {
    pub data: *const u8,
    pub len: usize,
    pub stride: u32,
    pub constant: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeNode {
    // 0 layer, 1 filter, 2 blend, 3 output. Capability-gated: 4 transparent,
    // 5 premultiplied mix (a before, b after, mode=0, finite amount in 0..=1).
    // Masked-renderer only: 6 coverage source (a descriptor, b=mode=amount=0),
    // 7 apply coverage (a image, b coverage, mode=0, amount opacity in 0..=1),
    // 8 masked mix (a before, b after, mode prior coverage slot, amount opacity).
    pub kind: u32,
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
/// Optional complete compositor tail. The published v1 prefix remains unchanged.
/// Kind 4 produces transparent zero with a=b=mode=0 and amount=0.
pub const COMPOSITOR_TRANSPARENT: u64 = 1;
/// Kind 5 interpolates premultiplied colors using t=round(amount*255).
/// Endpoints retain the exact input words. Otherwise round weighted alpha/255;
/// zero output alpha yields zero, and RGB rounds half-up against the unrounded
/// weighted alpha. Both input indices must refer to prior instruction slots.
pub const COMPOSITOR_MIX: u64 = 1 << 1;
/// Kinds 6/7/8 require the complete CompositorApiMasks tail and render_masked.
/// Image and coverage slots are distinct types, although both use prior indices.
pub const COMPOSITOR_MASKS: u64 = 1 << 2;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositorApiFeatures {
    pub base: CompositorApi,
    pub capabilities: u64,
}
/// Stable color blend IDs. Existing IDs 0..=7 retain their original arithmetic.
pub const BLEND_NORMAL: u32 = 0;
pub const BLEND_MULTIPLY: u32 = 1;
pub const BLEND_SCREEN: u32 = 2;
pub const BLEND_OVERLAY: u32 = 3;
pub const BLEND_DARKEN: u32 = 4;
pub const BLEND_LIGHTEN: u32 = 5;
pub const BLEND_ADD: u32 = 6;
pub const BLEND_DIFFERENCE: u32 = 7;
pub const BLEND_COLOR_BURN: u32 = 8;
pub const BLEND_LINEAR_BURN: u32 = 9;
pub const BLEND_DARKER_COLOR: u32 = 10;
pub const BLEND_COLOR_DODGE: u32 = 11;
pub const BLEND_LIGHTER_COLOR: u32 = 12;
pub const BLEND_SOFT_LIGHT: u32 = 13;
pub const BLEND_HARD_LIGHT: u32 = 14;
pub const BLEND_VIVID_LIGHT: u32 = 15;
pub const BLEND_LINEAR_LIGHT: u32 = 16;
pub const BLEND_PIN_LIGHT: u32 = 17;
pub const BLEND_HARD_MIX: u32 = 18;
pub const BLEND_EXCLUSION: u32 = 19;
pub const BLEND_SUBTRACT: u32 = 20;
pub const BLEND_DIVIDE: u32 = 21;
pub const BLEND_HUE: u32 = 22;
pub const BLEND_SATURATION: u32 = 23;
pub const BLEND_COLOR: u32 = 24;
pub const BLEND_LUMINOSITY: u32 = 25;
pub const BLEND_MODE_MAX: u32 = BLEND_LUMINOSITY;
pub const BLEND_MODE_COUNT: u32 = BLEND_MODE_MAX + 1;
pub const BLEND_MODES_LEGACY: u64 = (1 << (BLEND_DIFFERENCE + 1)) - 1;
pub const BLEND_MODES_ALL: u64 = (1 << BLEND_MODE_COUNT) - 1;
/// Read this entire optional tail only when the published prefix struct_size
/// covers it. Bit N advertises color blend ID N for both layers and blend nodes.
/// Without this tail, only BLEND_MODES_LEGACY is supported. The older Features
/// tail remains independently readable at its original size and offset.
/// All mandatory BLEND_MODES_LEGACY bits must remain set; new-mode support may
/// be partial. Old v1 hosts ignore this tail and assume the legacy modes exist.
#[repr(C)]
pub struct CompositorApiModes {
    pub base: CompositorApiFeatures,
    pub supported_blend_modes: u64,
}
pub type CompositeRenderMaskedFn = unsafe extern "C" fn(
    *const CompositeNode,
    usize,
    *const CompositeLayer,
    usize,
    *const CompositeMask,
    usize,
    u32,
    u32,
    *mut Pixels,
) -> i32;
/// Read only when the original header advertises this entire tail and the
/// capability bitmap includes COMPOSITOR_MASKS. Prefix render retains its
/// original instruction contract and does not accept kinds 6/7/8.
#[repr(C)]
pub struct CompositorApiMasks {
    pub base: CompositorApiModes,
    pub render_masked: Option<CompositeRenderMaskedFn>,
}
/// Optional effective-A8 region kernel. Requires the complete effects tail and
/// COMPOSITOR_MASKS. The existing 40-byte mask-compositor prefix is unchanged.
pub const COMPOSITOR_MASK_EFFECTS: u64 = 1 << 3;
pub const MASK_EFFECT_MAX_RADIUS: u32 = 256;
/// A raw edit, admitted only with density=255 and feather_radius=0.
pub const MASK_EFFECT_INVERT_RAW: u32 = 1;
/// Normal flags=0. Feather precedes density. With R=feather_radius, apply
/// H(a),V(a),H(b),V(b),H(c),V(c), where a=(R+2)/3,b=(R+1)/3,c=R/3.
/// Each box clamps to the canvas edge and rounds its byte mean half-up.
/// Total spatial support is R per axis. Finally output
/// 255-round_half_up((255-feathered)*density/255). Reserved must be zero.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MaskEffectRequest {
    pub flags: u32,
    pub density: u32,
    pub feather_radius: u32,
    pub reserved: u32,
}
pub type MaskEffectsCreateFn = unsafe extern "C" fn() -> *mut c_void;
pub type MaskEffectsDestroyFn = unsafe extern "C" fn(*mut c_void);
/// Reserve two full-frame A8 capacities and one u32 column row, without changing
/// their lengths or processing pixels. Dimensions 1..=4096, radius 0..=256;
/// maximum scratch capacity is 32 MiB plus 16 KiB.
pub type MaskEffectsReserveFn = unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32;
/// Immutable full-frame source, dimensions 1..=4096. Output is a separate A8
/// region with a canvas-relative origin. Validate every descriptor/alias before
/// writes. The instance owns reusable ROI+halo scratch; pointers never escape.
pub type MaskEffectRegionFn = unsafe extern "C" fn(
    *mut c_void,
    *const MaskEffectRequest,
    *const CompositeMask,
    u32,
    u32,
    *mut A8Pixels,
) -> i32;
#[repr(C)]
pub struct CompositorApiMaskEffects {
    pub base: CompositorApiMasks,
    pub create_mask_effects: Option<MaskEffectsCreateFn>,
    pub destroy_mask_effects: Option<MaskEffectsDestroyFn>,
    pub mask_effect_region: Option<MaskEffectRegionFn>,
    /// Mandatory with COMPOSITOR_MASK_EFFECTS; incomplete experimental tails reject.
    pub reserve_mask_effects: Option<MaskEffectsReserveFn>,
}
pub type CompositorEntry = unsafe extern "C" fn() -> *const CompositorApi;

/// Independent bounded Curves preparation. Existing compositor/Levels tables
/// and symbol contracts are unchanged; this prepares one channel at a time.
pub const CURVES_SYMBOL: &[u8] = b"micopi_curves_v1\0";
pub const CURVES_MAX_POINTS: usize = 32;
pub const CURVES_LINEAR: u32 = 0;
pub const CURVES_SMOOTH: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct CurvePoint {
    pub input: u32,
    pub output: u32,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CurvesParameters {
    pub abi_version: u32,
    pub struct_size: u32,
    pub count: u32,
    pub interpolation: u32,
    pub points: [CurvePoint; CURVES_MAX_POINTS],
}
/// 2..32 strictly input-ordered byte points; clamp outside endpoint inputs.
/// Modes: linear=0, shape-preserving Hermite=1 with endpoint secant tangents.
/// Writes exactly 256 bytes on success; invalid/overlapping spans do not write.
/// No allocation, retained pointer or document mutation.
pub type CurvesPrepareFn = unsafe extern "C" fn(*const CurvesParameters, *mut u8, usize) -> i32;
#[repr(C)]
pub struct CurvesApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub prepare: Option<CurvesPrepareFn>,
}
pub type CurvesEntry = unsafe extern "C" fn() -> *const CurvesApi;

#[cfg(test)]
mod curves_layout_tests {
    use super::*;
    #[test]
    fn independent_curves_api_has_stable_x64_layout() {
        assert_eq!(std::mem::size_of::<CurvePoint>(), 8);
        assert_eq!(std::mem::size_of::<CurvesParameters>(), 272);
        assert_eq!(std::mem::offset_of!(CurvesParameters, count), 8);
        assert_eq!(std::mem::offset_of!(CurvesParameters, interpolation), 12);
        assert_eq!(std::mem::offset_of!(CurvesParameters, points), 16);
        assert_eq!(std::mem::size_of::<CurvesApi>(), 16);
        assert_eq!(std::mem::offset_of!(CurvesApi, prepare), 8);
        assert_eq!(CURVES_SYMBOL, b"micopi_curves_v1\0");
    }
}

/// Optional cold Levels preparation. This does not change compositor tables.
pub const LEVELS_SYMBOL: &[u8] = b"micopi_levels_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct LevelsParameters {
    pub abi_version: u32,
    pub struct_size: u32,
    pub input_black: u32,
    pub input_white: u32,
    pub output_black: u32,
    pub output_white: u32,
    pub gamma: f64,
}
/// Writes exactly 256 bytes on success; rejects invalid or overlapping spans
/// without writes. No allocation, retained pointer or document mutation.
pub type LevelsPrepareFn = unsafe extern "C" fn(*const LevelsParameters, *mut u8, usize) -> i32;
#[repr(C)]
pub struct LevelsApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub prepare: Option<LevelsPrepareFn>,
}
pub type LevelsEntry = unsafe extern "C" fn() -> *const LevelsApi;
/// Kind 9 consumes prior image slot a and LUT index mode; b=0, amount=1.
/// Tables are borrowed only for the render call, at most 64 (16 KiB).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CompositeLut {
    pub values: [u8; 256],
}
pub type LevelsRenderFn = unsafe extern "C" fn(
    *const CompositeNode,
    usize,
    *const CompositeLayer,
    usize,
    *const CompositeMask,
    usize,
    *const CompositeLut,
    usize,
    u32,
    u32,
    *mut Pixels,
) -> i32;
/// Optional complete 24-byte tail. Preparation-only 16-byte tables stay valid.
#[repr(C)]
pub struct LevelsApiRender {
    pub base: LevelsApi,
    pub render: Option<LevelsRenderFn>,
}

#[cfg(test)]
mod levels_layout_tests {
    use super::*;
    #[test]
    fn cold_levels_api_has_stable_x64_layout() {
        assert_eq!(std::mem::size_of::<LevelsParameters>(), 32);
        assert_eq!(std::mem::offset_of!(LevelsParameters, gamma), 24);
        assert_eq!(std::mem::size_of::<LevelsApi>(), 16);
        assert_eq!(std::mem::size_of::<CompositeLut>(), 256);
        assert_eq!(std::mem::size_of::<LevelsApiRender>(), 24);
        assert_eq!(std::mem::offset_of!(LevelsApiRender, render), 16);
        assert_eq!(std::mem::offset_of!(LevelsApi, prepare), 8);
        assert_eq!(LEVELS_SYMBOL, b"micopi_levels_v1\0");
    }
}

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

/// Independent stateless A8 sampler. All three callbacks are mandatory in v1;
/// existing GeometryApi and ResampleApi prefixes remain unchanged.
pub const A8_SAMPLER_SYMBOL: &[u8] = b"micopi_a8_sampler_v1\0";
/// Place the source at signed destination offsets within +/-4096, clipping
/// excess and filling uncovered pixels with fill (0..=255). Reserved is zero.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct A8Reframe {
    pub x: i32,
    pub y: i32,
    pub fill: u32,
    pub reserved: u32,
}
/// Exact existing Geometry kinds/offsets with explicit uncovered A8 fill.
#[repr(C)]
pub struct A8Geometry {
    pub geometry: Geometry,
    pub fill: u32,
}
/// Source dimensions and strides are in bytes and bounded to 4096. The source
/// is a validated dense or uniform CompositeMask. Output is a disjoint full
/// frame (origin zero), preserving row padding. No pointer survives a callback.
pub type A8ReframeFn =
    unsafe extern "C" fn(*const A8Reframe, *const CompositeMask, u32, u32, *mut A8Pixels) -> i32;
pub type A8GeometryFn =
    unsafe extern "C" fn(*const A8Geometry, *const CompositeMask, u32, u32, *mut A8Pixels) -> i32;
/// Methods 0/1/2 use the existing exact u8 nearest/bilinear/area rules.
pub type A8ResampleFn =
    unsafe extern "C" fn(*const Resample, *const CompositeMask, u32, u32, *mut A8Pixels) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct A8SamplerApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub reframe: Option<A8ReframeFn>,
    pub geometry: Option<A8GeometryFn>,
    pub resample: Option<A8ResampleFn>,
}
pub type A8SamplerEntry = unsafe extern "C" fn() -> *const A8SamplerApi;

/// Independent optional scalar affine sampling; the published A8SamplerApi
/// table remains unchanged. Sources are immutable dense/uniform raw bytes.
pub const A8_AFFINE_SYMBOL: &[u8] = b"micopi_a8_affine_v1\0";
/// Finite forward document-edge affine map, in row-major order. Methods are
/// nearest0 / bilinear1; uncovered scalar samples use fill0..255. Sampling maps
/// destination pixel centers through the inverse. Bilinear rounds axis weights
/// to Q16 and the final scalar sum half-up; it does not premultiply coverage.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct A8Affine {
    pub method: u32,
    pub fill: u32,
    pub matrix: [f64; 6],
}
pub type A8AffineFn =
    unsafe extern "C" fn(*const A8Affine, *const CompositeMask, u32, u32, *mut A8Pixels) -> i32;
/// Complete independent optional table. Inputs/output are disjoint and scoped
/// to one synchronous call. Output origin is zero; row padding is untouched.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct A8AffineApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub sample: Option<A8AffineFn>,
}
/// Read-only support bounds relative to neutral0..255. Uniform neutral has no
/// support (zero bounds); a different uniform sample covers the complete frame.
/// Output must be disjoint from source bytes and the source descriptor.
pub type A8AffineBoundsFn =
    unsafe extern "C" fn(*const CompositeMask, u32, u32, u32, *mut TrimBounds) -> i32;
/// Optional tail. A complete base table remains a supported sample-only API;
/// inspect struct_size before reading bounds, which may itself be absent.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct A8AffineApiExt {
    pub base: A8AffineApi,
    pub bounds: Option<A8AffineBoundsFn>,
}
pub type A8AffineEntry = unsafe extern "C" fn() -> *const A8AffineApi;

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

/// Independent optional homography sampler. Existing affine layouts remain unchanged.
pub const PROJECTIVE_SYMBOL: &[u8] = b"micopi_projective_v1\0";
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProjectiveApi {
    pub abi_version: u32,
    pub struct_size: u32,
    pub pixels: Option<ProjectivePixelsFn>,
    pub mask: Option<ProjectiveMaskFn>,
}
/// Row-major finite forward homography in document-edge coordinates.
/// Kinds/methods match Affine. Inverse samples behind the forward horizon are transparent.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Projective {
    pub kind: u32,
    pub method: u32,
    pub matrix: [f64; 9],
}
pub type ProjectivePixelsFn =
    unsafe extern "C" fn(*const Projective, *const Pixels, *const Coverage, *mut Pixels) -> i32;
pub type ProjectiveMaskFn =
    unsafe extern "C" fn(*const Projective, *const Coverage, *mut u8, usize) -> i32;
pub type ProjectiveEntry = unsafe extern "C" fn() -> *const ProjectiveApi;

#[cfg(all(test, target_pointer_width = "64"))]
mod transform_layout_tests {
    use super::*;
    use std::mem::{offset_of, size_of};
    #[test]
    fn independent_a8_sampler_preserves_existing_geometry_and_resample_tables() {
        assert_eq!(size_of::<Geometry>(), 12);
        assert_eq!(size_of::<GeometryApi>(), 24);
        assert_eq!(size_of::<Resample>(), 4);
        assert_eq!(size_of::<ResampleApi>(), 24);
        assert_eq!(size_of::<ResampleApiExt>(), 32);
        assert_eq!(size_of::<A8Reframe>(), 16);
        assert_eq!(offset_of!(A8Reframe, fill), 8);
        assert_eq!(offset_of!(A8Reframe, reserved), 12);
        assert_eq!(size_of::<A8Geometry>(), 16);
        assert_eq!(offset_of!(A8Geometry, fill), 12);
        assert_eq!(size_of::<A8SamplerApi>(), 32);
        assert_eq!(offset_of!(A8SamplerApi, reframe), 8);
        assert_eq!(offset_of!(A8SamplerApi, geometry), 16);
        assert_eq!(offset_of!(A8SamplerApi, resample), 24);
    }
    #[test]
    fn a8_extensions_preserve_published_tool_and_compositor_prefixes() {
        assert_eq!(size_of::<Pixels>(), 40);
        assert_eq!(size_of::<A8Pixels>(), 40);
        assert_eq!(offset_of!(A8Pixels, data), 0);
        assert_eq!(offset_of!(A8Pixels, len), 8);
        assert_eq!(offset_of!(A8Pixels, width), 16);
        assert_eq!(offset_of!(A8Pixels, origin_y), 32);
        assert_eq!(size_of::<ToolParameters>(), 32);
        assert_eq!(size_of::<ToolSample>(), 44);
        assert_eq!(size_of::<ToolOutput>(), 20);
        assert_eq!(size_of::<ToolHost>(), 32);
        assert_eq!(offset_of!(ToolHost, pixels), 16);
        assert_eq!(offset_of!(ToolHost, pick), 24);
        assert_eq!(size_of::<ToolHostA8>(), 40);
        assert_eq!(offset_of!(ToolHostA8, base), 0);
        assert_eq!(offset_of!(ToolHostA8, a8), 32);
        assert_eq!(size_of::<ToolApi>(), 32);
        assert_eq!(size_of::<ToolOptions>(), 16);
        assert_eq!(size_of::<ToolOptionsExt>(), 20);
        assert_eq!(size_of::<ToolOptionsSelection>(), 56);
        assert_eq!(size_of::<ToolOptionsApi>(), 16);
        assert_eq!(size_of::<ToolOptionsApiExt>(), 24);
        assert_eq!(TOOL_OPTION_A8_TARGET, 4);
        assert_eq!(
            TOOL_OPTION_A8_TARGET & (TOOL_OPTION_ALPHA_LOCK | TOOL_OPTION_SELECTION),
            0
        );
        assert_eq!(size_of::<CompositeMask>(), 24);
        assert_eq!(offset_of!(CompositeMask, len), 8);
        assert_eq!(offset_of!(CompositeMask, stride), 16);
        assert_eq!(offset_of!(CompositeMask, constant), 20);
        assert_eq!(size_of::<CompositorApiMasks>(), 40);
        assert_eq!(offset_of!(CompositorApiMasks, base), 0);
        assert_eq!(offset_of!(CompositorApiMasks, render_masked), 32);
        assert_eq!(COMPOSITOR_MASKS, 4);
        assert_eq!(size_of::<MaskEffectRequest>(), 16);
        assert_eq!(offset_of!(MaskEffectRequest, density), 4);
        assert_eq!(offset_of!(MaskEffectRequest, feather_radius), 8);
        assert_eq!(size_of::<CompositorApiMaskEffects>(), 72);
        assert_eq!(
            offset_of!(CompositorApiMaskEffects, create_mask_effects),
            40
        );
        assert_eq!(
            offset_of!(CompositorApiMaskEffects, destroy_mask_effects),
            48
        );
        assert_eq!(offset_of!(CompositorApiMaskEffects, mask_effect_region), 56);
        assert_eq!(
            offset_of!(CompositorApiMaskEffects, reserve_mask_effects),
            64
        );
        assert_eq!(COMPOSITOR_MASK_EFFECTS, 8);
        assert_eq!(
            COMPOSITOR_MASKS & (COMPOSITOR_TRANSPARENT | COMPOSITOR_MIX),
            0
        );
    }
    #[test]
    fn stroke_opacity_tails_preserve_all_published_prefixes_and_const_views() {
        assert_eq!(TOOL_ABI_VERSION, 1);
        assert_eq!(ABI_VERSION, 1);
        assert_eq!(size_of::<ToolOptionsStroke>(), 64);
        assert_eq!(offset_of!(ToolOptionsStroke, base), 0);
        assert_eq!(offset_of!(ToolOptionsStroke, flow), 56);
        assert_eq!(offset_of!(ToolOptionsStroke, reserved), 60);
        assert_eq!(size_of::<ToolHostStroke>(), 64);
        assert_eq!(offset_of!(ToolHostStroke, base), 0);
        assert_eq!(offset_of!(ToolHostStroke, features), 40);
        assert_eq!(offset_of!(ToolHostStroke, tile_side), 44);
        assert_eq!(offset_of!(ToolHostStroke, pixels_origin), 48);
        assert_eq!(offset_of!(ToolHostStroke, a8_origin), 56);
        assert_eq!(size_of::<OriginPixels>(), size_of::<Pixels>());
        assert_eq!(size_of::<OriginA8Pixels>(), size_of::<A8Pixels>());
        assert_eq!(offset_of!(OriginPixels, data), 0);
        assert_eq!(offset_of!(OriginPixels, len), 8);
        assert_eq!(offset_of!(OriginPixels, width), 16);
        assert_eq!(offset_of!(OriginPixels, stride), 24);
        assert_eq!(offset_of!(OriginPixels, origin_x), 28);
        assert_eq!(offset_of!(OriginPixels, origin_y), 32);
        assert_eq!(offset_of!(OriginA8Pixels, origin_y), 32);
        assert_eq!(TOOL_OPTION_STROKE_OPACITY, 8);
        assert_eq!(
            TOOL_OPTION_STROKE_OPACITY
                & (TOOL_OPTION_ALPHA_LOCK | TOOL_OPTION_SELECTION | TOOL_OPTION_A8_TARGET),
            0
        );
        assert_eq!(TOOL_HOST_ORIGIN_RGBA, 1);
        assert_eq!(TOOL_HOST_ORIGIN_A8, 2);
        assert_eq!(TOOL_HOST_ORIGIN_RGBA & TOOL_HOST_ORIGIN_A8, 0);
    }
    #[test]
    fn independent_a8_affine_table_preserves_existing_sampling_tables() {
        assert_eq!(A8_AFFINE_SYMBOL, b"micopi_a8_affine_v1\0");
        assert_eq!(size_of::<A8Affine>(), 56);
        assert_eq!(offset_of!(A8Affine, method), 0);
        assert_eq!(offset_of!(A8Affine, fill), 4);
        assert_eq!(offset_of!(A8Affine, matrix), 8);
        assert_eq!(size_of::<A8AffineApi>(), 16);
        assert_eq!(offset_of!(A8AffineApi, sample), 8);
        assert_eq!(size_of::<A8AffineApiExt>(), 24);
        assert_eq!(offset_of!(A8AffineApiExt, base), 0);
        assert_eq!(offset_of!(A8AffineApiExt, bounds), 16);
        assert_eq!(size_of::<A8SamplerApi>(), 32);
        assert_eq!(offset_of!(A8SamplerApi, reframe), 8);
        assert_eq!(offset_of!(A8SamplerApi, geometry), 16);
        assert_eq!(offset_of!(A8SamplerApi, resample), 24);
        assert_eq!(size_of::<Affine>(), 56);
        assert_eq!(size_of::<AffineApi>(), 32);
    }
    #[test]
    fn analytic_tip_tail_preserves_stroke_and_legacy_options() {
        assert_eq!(size_of::<ToolOptionsTip>(), 80);
        assert_eq!(offset_of!(ToolOptionsTip, base), 0);
        assert_eq!(offset_of!(ToolOptionsTip, tip_shape), 64);
        assert_eq!(offset_of!(ToolOptionsTip, roundness), 68);
        assert_eq!(offset_of!(ToolOptionsTip, angle), 72);
        assert_eq!(offset_of!(ToolOptionsTip, reserved), 76);
        assert_eq!(TOOL_OPTION_TIP_GEOMETRY, 16);
        assert_eq!(
            TOOL_OPTION_TIP_GEOMETRY
                & (TOOL_OPTION_ALPHA_LOCK
                    | TOOL_OPTION_SELECTION
                    | TOOL_OPTION_A8_TARGET
                    | TOOL_OPTION_STROKE_OPACITY),
            0
        );
        assert_eq!((TIP_ROUND, TIP_SQUARE), (0, 1));
        assert_eq!(size_of::<ToolOptions>(), 16);
        assert_eq!(size_of::<ToolOptionsSelection>(), 56);
        assert_eq!(size_of::<ToolOptionsStroke>(), 64);
        assert_eq!(size_of::<ToolHostStroke>(), 64);
    }
    #[test]
    fn optional_a8_bounds_callback_uses_the_published_scalar_descriptor_and_output() {
        unsafe extern "C" fn callback(
            source: *const CompositeMask,
            width: u32,
            height: u32,
            neutral: u32,
            output: *mut TrimBounds,
        ) -> i32 {
            if (*source).constant != neutral {
                output.write(TrimBounds {
                    x: 0,
                    y: 0,
                    width,
                    height,
                });
            } else {
                output.write(TrimBounds::default());
            }
            OK
        }
        let callback: A8AffineBoundsFn = callback;
        let api = A8AffineApiExt {
            base: A8AffineApi {
                abi_version: 1,
                struct_size: size_of::<A8AffineApiExt>() as u32,
                sample: None,
            },
            bounds: Some(callback),
        };
        let source = CompositeMask {
            data: std::ptr::null(),
            len: 0,
            stride: 0,
            constant: 73,
        };
        let mut output = TrimBounds::default();
        assert_eq!(
            unsafe { api.bounds.unwrap()(&source, 5, 3, 255, &mut output) },
            OK
        );
        assert_eq!(
            output,
            TrimBounds {
                x: 0,
                y: 0,
                width: 5,
                height: 3
            }
        );
        assert_eq!(unsafe { callback(&source, 5, 3, 73, &mut output) }, OK);
        assert_eq!(output, TrimBounds::default());
    }
    #[test]
    fn independent_projective_table_preserves_the_published_affine_layout() {
        assert_eq!(size_of::<Affine>(), 56);
        assert_eq!(size_of::<AffineApi>(), 32);
        assert_eq!(offset_of!(AffineApi, bounds), 24);
        assert_eq!(size_of::<Projective>(), 80);
        assert_eq!(offset_of!(Projective, matrix), 8);
        assert_eq!(size_of::<ProjectiveApi>(), 24);
        assert_eq!(offset_of!(ProjectiveApi, pixels), 8);
        assert_eq!(offset_of!(ProjectiveApi, mask), 16);
    }
    #[test]
    fn optional_compositor_capabilities_preserve_the_v1_prefix() {
        assert_eq!(size_of::<CompositeNode>(), 20);
        assert_eq!(offset_of!(CompositeNode, amount), 16);
        assert_eq!(size_of::<CompositeLayer>(), 32);
        assert_eq!(size_of::<CompositorApi>(), 16);
        assert_eq!(offset_of!(CompositorApi, render), 8);
        assert_eq!(size_of::<CompositorApiFeatures>(), 24);
        assert_eq!(offset_of!(CompositorApiFeatures, base), 0);
        assert_eq!(offset_of!(CompositorApiFeatures, capabilities), 16);
        assert_eq!(COMPOSITOR_TRANSPARENT, 1);
        assert_eq!(COMPOSITOR_MIX, 2);
        assert_eq!(COMPOSITOR_MIX & COMPOSITOR_TRANSPARENT, 0);
        assert_eq!(size_of::<CompositorApiModes>(), 32);
        assert_eq!(offset_of!(CompositorApiModes, base), 0);
        assert_eq!(offset_of!(CompositorApiModes, supported_blend_modes), 24);
        assert_eq!(BLEND_MODE_MAX, 25);
        assert_eq!(BLEND_MODE_COUNT, 26);
        assert_eq!(BLEND_MODES_LEGACY, 0xff);
        assert_eq!(BLEND_MODES_ALL, 0x03ff_ffff);
    }
}
