#ifndef MICOPI_PLUGIN_V1_H
#define MICOPI_PLUGIN_V1_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
#define MICOPI_ABI_VERSION 1u
#define MICOPI_OK 0
#define MICOPI_ERROR 1
/* All pointers are borrowed for the current synchronous call only. */
typedef struct { const uint8_t *data; size_t len; } MicopiBytes;
typedef struct { void *context; int32_t (*write)(void *, MicopiBytes); } MicopiReply;
typedef struct { uint32_t x, y, width, height; } MicopiRegion;
typedef struct {
    uint32_t *data; size_t len;
    uint32_t width, height, stride, origin_x, origin_y;
} MicopiPixels;
typedef int32_t (*MicopiTransform)(void *, MicopiPixels *);
/* Mutable one-byte coverage; length/stride use bytes. Write only width bytes per
   row, and do not retain any pointer beyond its synchronous transform call. */
typedef struct {
    uint8_t *data; size_t len;
    uint32_t width, height, stride, origin_x, origin_y;
} MicopiA8Pixels;
typedef int32_t (*MicopiA8Transform)(void *, MicopiA8Pixels *);
/* Immutable contact-original undo tile samples. Coordinate extents match the
   paired current region; row strides can differ. Never retain these pointers. */
typedef struct {
    const uint32_t *data; size_t len;
    uint32_t width, height, stride, origin_x, origin_y;
} MicopiOriginPixels;
typedef struct {
    const uint8_t *data; size_t len;
    uint32_t width, height, stride, origin_x, origin_y;
} MicopiOriginA8Pixels;
typedef int32_t (*MicopiOriginTransform)(void *, const MicopiOriginPixels *, MicopiPixels *);
typedef int32_t (*MicopiA8OriginTransform)(void *, const MicopiOriginA8Pixels *, MicopiA8Pixels *);
typedef int32_t (*MicopiOriginPixelsLoan)(void *, MicopiRegion, MicopiOriginTransform, void *);
typedef int32_t (*MicopiOriginA8Loan)(void *, MicopiRegion, MicopiA8OriginTransform, void *);
typedef struct {
    uint32_t abi_version, struct_size;
    void *context;
    int32_t (*dispatch)(void *, MicopiBytes, MicopiBytes, MicopiReply);
    int32_t (*edit_region)(void *, MicopiRegion, MicopiTransform, void *);
} MicopiHostApi;
typedef struct {
    uint32_t abi_version, struct_size;
    MicopiBytes (*describe)(void);
    void *(*create)(void);
    void (*destroy)(void *);
    int32_t (*invoke)(void *, MicopiBytes, MicopiBytes, const MicopiHostApi *, MicopiReply);
} MicopiPluginApi;
/* Export this function using the platform's C export attribute. */
const MicopiPluginApi *micopi_plugin_v1(void);
/* Optional additive tool extension (SDK 1.1); the action ABI above is unchanged. */
#define MICOPI_TOOL_ABI_VERSION 1u
#define MICOPI_DOWN 0u
#define MICOPI_MOVE 1u
#define MICOPI_UP 2u
#define MICOPI_CANCEL 3u
#define MICOPI_PEN 1u
#define MICOPI_ERASER 2u
#define MICOPI_PRESSURE_SIZE 1u
#define MICOPI_PRESSURE_OPACITY 2u
#define MICOPI_DRAWING 1u
#define MICOPI_PANNING 2u
#define MICOPI_COLOR_CHANGED 4u
#define MICOPI_VIEW_CHANGED 8u
typedef struct {
    float radius, opacity, hardness; uint32_t color;
    float stabilizer, pressure_gamma, minimum_radius; uint32_t flags;
} MicopiToolParameters;
typedef struct {
    uint32_t phase, flags; float x, y, pressure; uint32_t time;
    float view_x, view_y; uint32_t width, height; float zoom;
} MicopiToolSample;
typedef struct {
    uint32_t flags, color; float zoom, pan_dx, pan_dy;
} MicopiToolOutput;
typedef struct {
    uint32_t abi_version, struct_size; void *context;
    /* Borrowed strided pixels. Touch only width pixels per row; no row gaps.
       Host snapshots undo tiles first. Errors roll back the entire stroke. */
    int32_t (*pixels)(void *, MicopiRegion, MicopiTransform, void *);
    int32_t (*pick)(void *, float, float, uint32_t *);
} MicopiToolHost;
/* Optional complete tail for A8 target contacts. The legacy pixels callback
   stays RGBA-only. Host snapshots byte tiles before loan; failure rolls back
   the entire contact. Check the base struct_size before reading this tail. */
typedef struct {
    MicopiToolHost base;
    int32_t (*a8)(void *, MicopiRegion, MicopiA8Transform, void *);
} MicopiToolHostA8;
#define MICOPI_TOOL_HOST_ORIGIN_RGBA 1u
#define MICOPI_TOOL_HOST_ORIGIN_A8 2u
/* SDK1.3 optional complete tail; read only with complete struct_size, matching
   feature and non-null callback. Each synchronous, non-reentrant loan prepares
   the full ROI once then calls the transform for tile intersections. Current
   and immutable original storage are disjoint. tile_side is64. */
typedef struct {
    MicopiToolHostA8 base;
    uint32_t features, tile_side;
    MicopiOriginPixelsLoan pixels_origin;
    MicopiOriginA8Loan a8_origin;
} MicopiToolHostStroke;
typedef struct {
    uint32_t abi_version, struct_size;
    void *(*create)(void);
    void (*destroy)(void *);
    /* Parameters is non-null on DOWN only; freeze it until UP/CANCEL.
       All pointers are scoped to this synchronous call. */
    int32_t (*pointer)(void *, uint32_t, const MicopiToolSample *,
                       const MicopiToolParameters *, const MicopiToolHost *,
                       MicopiToolOutput *);
} MicopiToolApi;
const MicopiToolApi *micopi_tools_v1(void);
/* Optional stroke options. Existing v1 tables and parameter sizes are unchanged. */
typedef struct {
    uint32_t abi_version, struct_size;
    float spacing, minimum_opacity; /* diameter fraction, pressure opacity floor */
} MicopiToolOptions;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*configure)(void *, const MicopiToolOptions *);
} MicopiToolOptionsApi;
const MicopiToolOptionsApi *micopi_tool_options_v1(void);
#define MICOPI_TOOL_OPTION_ALPHA_LOCK 1u
#define MICOPI_TOOL_OPTION_SELECTION 2u
/* Both options capability and flag. Requires complete MicopiToolHostA8, cannot
   combine with alpha lock. Brush value=(54R+183G+19B+128)>>8; eraser reveals 255.
   Dab mode clips each deposit; capped mode clips cumulative influence once
   before rounded byte interpolation. */
#define MICOPI_TOOL_OPTION_A8_TARGET 4u
/* Capability negotiates Flow tail; flag selects capped stroke mode. Nondefault
   Flow also requires the full tail in dab mode. Capped contacts require the
   complete MicopiToolHostStroke, with only their admitted target exposed. */
#define MICOPI_TOOL_OPTION_STROKE_OPACITY 8u
/* Fixed analytic geometry, complete 80-byte MicopiToolOptionsTip required. */
#define MICOPI_TOOL_OPTION_TIP_GEOMETRY 16u
#define MICOPI_TIP_ROUND 0u
#define MICOPI_TIP_SQUARE 1u
typedef struct {
    const uint8_t *data; size_t len;
    uint32_t width, height, stride;
} MicopiCoverage;
/* Optional HostApi tail: immutable action-entry snapshot, valid until invoke returns. */
typedef struct {
    MicopiHostApi base;
    int32_t (*selection)(void *, MicopiCoverage *);
} MicopiHostApiSelection;
/* Additive tails, available only if the prefix struct_size includes the tail. */
typedef struct { MicopiToolOptions base; uint32_t flags; } MicopiToolOptionsExt;
typedef struct { MicopiToolOptionsExt base; MicopiCoverage selection; } MicopiToolOptionsSelection;
/* Legacy configure resets flow1/dab; finite flow0..1 and reserved0 required. */
typedef struct { MicopiToolOptionsSelection base; float flow; uint32_t reserved; } MicopiToolOptionsStroke;
/* Radius is the major half-diameter; finite roundness .25..1, clockwise document
   angle -180..180, reserved0. Pressure scales both axes. Legacy option prefixes
   reset geometry to Round/1/0; no assets or new host loans are introduced. */
typedef struct {
    MicopiToolOptionsStroke base;
    uint32_t tip_shape; float roundness, angle; uint32_t reserved;
} MicopiToolOptionsTip;
typedef struct {
    uint32_t kind, mode; float bounds[4];
    const float (*points)[2]; size_t count;
} MicopiSelectionShape;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*mix)(const MicopiCoverage *, const MicopiPixels *, MicopiPixels *);
    int32_t (*rasterize)(const MicopiSelectionShape *, const MicopiCoverage *, uint8_t *, size_t, uint32_t, uint32_t);
} MicopiSelectionApi;
typedef struct {
    uint32_t kind, mode, x, y, tolerance, contiguous;
} MicopiSelectionColor;
typedef struct {
    MicopiSelectionApi base;
    int32_t (*color)(const MicopiSelectionColor *, const MicopiPixels *, const MicopiCoverage *, uint8_t *, size_t);
} MicopiSelectionApiColor;
/* Kinds: expand/contract/feather/smooth (0..3). Radius 1..256. Edge: extend=0, zero=1. */
typedef struct { uint32_t kind, radius, edge; } MicopiSelectionEdges;
typedef struct {
    MicopiSelectionApiColor base;
    int32_t (*edges)(const MicopiSelectionEdges *, const MicopiCoverage *, uint8_t *, size_t);
} MicopiSelectionApiEdges;
/* Optional fixed tail; buffers are disjoint and local origins must be zero.
   Kind 0 extracts at x/y with optional coverage; kind 1 places with clipping,
   requires absent coverage and leaves destination pixels outside the overlap unchanged. */
typedef struct { uint32_t kind; int32_t x, y; } MicopiSelectionTransfer;
typedef struct {
    MicopiSelectionApiEdges base;
    int32_t (*transfer)(const MicopiSelectionTransfer *, const MicopiPixels *,
                        const MicopiCoverage *, MicopiPixels *);
} MicopiSelectionApiTransfer;
const MicopiSelectionApi *micopi_selection_v1(void);
typedef struct { MicopiToolOptionsApi base; uint32_t features; } MicopiToolOptionsApiExt;
/* SDK 1.2 additive compositor extension; no existing tables change. */
typedef struct {
    const uint32_t *data; size_t len;
    uint32_t opacity, visible, mode;
} MicopiCompositeLayer;
/* Immutable full-frame A8 coverage. Null data requires len=stride=0 and
   constant<=255; non-null data requires constant=0 and full-frame row storage.
   Input must not alias output; pointers stay scoped to the render call. */
typedef struct {
    const uint8_t *data; size_t len;
    uint32_t stride, constant;
} MicopiCompositeMask;
typedef struct {
    /* Kinds 0 layer, 1 filter, 2 blend, 3 output. Optional kind 4 is transparent
       zero; it requires MICOPI_COMPOSITOR_TRANSPARENT and a=b=mode=amount=0.
       Optional kind 5 requires MICOPI_COMPOSITOR_MIX: a before, b after, mode=0,
       finite amount in [0,1]; both input indices must precede this instruction.
       Masked-renderer kinds: 6 coverage source(a descriptor,b=mode=amount=0);
       7 apply coverage(a image,b coverage,mode=0,amount opacity in[0,1]);
       8 masked mix(a before,b after,mode prior coverage slot,amount opacity).
       Coverage and image instruction slots are distinct types. */
    uint32_t kind, a, b, mode; float amount;
} MicopiCompositeNode;
typedef struct {
    uint32_t abi_version, struct_size;
    /* All nodes are topologically ordered. Source layers are immutable and
       disjoint from output. All pointers are scoped to the synchronous call. */
    int32_t (*render)(const MicopiCompositeNode *, size_t,
                      const MicopiCompositeLayer *, size_t,
                      uint32_t, uint32_t, MicopiPixels *);
} MicopiCompositorApi;
/* Read the complete tail only when struct_size >= sizeof(MicopiCompositorApiFeatures).
   Hosts must check capabilities before sending optional instructions. */
#define MICOPI_COMPOSITOR_TRANSPARENT UINT64_C(1)
/* Kind 5 uses t=round(amount*255), with exact input words at t=0/255.
   Otherwise round weighted alpha/255; zero output alpha yields zero. RGB rounds
   half-up against the unrounded weighted alpha, preserving premultiplied mixing. */
#define MICOPI_COMPOSITOR_MIX (UINT64_C(1) << 1)
#define MICOPI_COMPOSITOR_MASKS (UINT64_C(1) << 2)
typedef struct { MicopiCompositorApi base; uint64_t capabilities; } MicopiCompositorApiFeatures;
/* Stable color blend IDs; IDs 0..7 keep the original arithmetic. */
#define MICOPI_BLEND_NORMAL UINT32_C(0)
#define MICOPI_BLEND_MULTIPLY UINT32_C(1)
#define MICOPI_BLEND_SCREEN UINT32_C(2)
#define MICOPI_BLEND_OVERLAY UINT32_C(3)
#define MICOPI_BLEND_DARKEN UINT32_C(4)
#define MICOPI_BLEND_LIGHTEN UINT32_C(5)
#define MICOPI_BLEND_ADD UINT32_C(6)
#define MICOPI_BLEND_DIFFERENCE UINT32_C(7)
#define MICOPI_BLEND_COLOR_BURN UINT32_C(8)
#define MICOPI_BLEND_LINEAR_BURN UINT32_C(9)
#define MICOPI_BLEND_DARKER_COLOR UINT32_C(10)
#define MICOPI_BLEND_COLOR_DODGE UINT32_C(11)
#define MICOPI_BLEND_LIGHTER_COLOR UINT32_C(12)
#define MICOPI_BLEND_SOFT_LIGHT UINT32_C(13)
#define MICOPI_BLEND_HARD_LIGHT UINT32_C(14)
#define MICOPI_BLEND_VIVID_LIGHT UINT32_C(15)
#define MICOPI_BLEND_LINEAR_LIGHT UINT32_C(16)
#define MICOPI_BLEND_PIN_LIGHT UINT32_C(17)
#define MICOPI_BLEND_HARD_MIX UINT32_C(18)
#define MICOPI_BLEND_EXCLUSION UINT32_C(19)
#define MICOPI_BLEND_SUBTRACT UINT32_C(20)
#define MICOPI_BLEND_DIVIDE UINT32_C(21)
#define MICOPI_BLEND_HUE UINT32_C(22)
#define MICOPI_BLEND_SATURATION UINT32_C(23)
#define MICOPI_BLEND_COLOR UINT32_C(24)
#define MICOPI_BLEND_LUMINOSITY UINT32_C(25)
#define MICOPI_BLEND_MODE_MAX MICOPI_BLEND_LUMINOSITY
#define MICOPI_BLEND_MODE_COUNT UINT32_C(26)
#define MICOPI_BLEND_MODES_LEGACY UINT64_C(0xff)
#define MICOPI_BLEND_MODES_ALL UINT64_C(0x03ffffff)
/* Read only when struct_size >= sizeof(MicopiCompositorApiModes).
   Bit N supports blend ID N in source descriptors and blend instructions.
   A missing Modes tail implies MICOPI_BLEND_MODES_LEGACY. The existing Features
   tail is still independently readable at its original size and offset.
   All MICOPI_BLEND_MODES_LEGACY bits must remain set; new modes may be partial.
   Old v1 hosts ignore this tail and assume the mandatory legacy modes exist. */
typedef struct {
    MicopiCompositorApiFeatures base;
    uint64_t supported_blend_modes;
} MicopiCompositorApiModes;
typedef int32_t (*MicopiCompositeRenderMaskedFn)(
    const MicopiCompositeNode *, size_t, const MicopiCompositeLayer *, size_t,
    const MicopiCompositeMask *, size_t, uint32_t, uint32_t, MicopiPixels *);
/* Read only when the original header covers this complete tail and capabilities
   includes MICOPI_COMPOSITOR_MASKS. Prefix render never accepts kinds 6/7/8. */
typedef struct {
    MicopiCompositorApiModes base;
    MicopiCompositeRenderMaskedFn render_masked;
} MicopiCompositorApiMasks;
/* Optional effective-A8 kernel. Requires complete tail plus MASKS capability;
   the original 40-byte mask-compositor prefix is unchanged. */
#define MICOPI_COMPOSITOR_MASK_EFFECTS (UINT64_C(1) << 3)
#define MICOPI_MASK_EFFECT_MAX_RADIUS 256u
/* Raw inversion only: requires density=255 and feather_radius=0. */
#define MICOPI_MASK_EFFECT_INVERT_RAW 1u
/* Normal flags=0; reserved=0. For R, a=(R+2)/3,b=(R+1)/3,c=R/3.
   Apply H(a),V(a),H(b),V(b),H(c),V(c), clamping each box to canvas edges
   and rounding each byte mean half-up. Support is exactly R per axis.
   Then coverage=255-round_half_up((255-feathered)*density/255). */
typedef struct {
    uint32_t flags, density, feather_radius, reserved;
} MicopiMaskEffectRequest;
typedef void *(*MicopiMaskEffectsCreateFn)(void);
typedef void (*MicopiMaskEffectsDestroyFn)(void *);
/* Reserve two full-frame A8 capacities plus one u32 column row: dimensions
   1..4096, radius 0..256, at most 32 MiB + 16 KiB. Lengths/pixels stay unchanged. */
typedef int32_t (*MicopiMaskEffectsReserveFn)(void *, uint32_t, uint32_t, uint32_t);
/* Immutable full-canvas CompositeMask, frame dimensions 1..4096; disjoint
   A8Pixels output origin/size requests the ROI. Validate before writes.
   Instances reuse ROI+halo scratch. No source/output pointer outlives the call. */
typedef int32_t (*MicopiMaskEffectRegionFn)(
    void *, const MicopiMaskEffectRequest *, const MicopiCompositeMask *,
    uint32_t, uint32_t, MicopiA8Pixels *);
typedef struct {
    MicopiCompositorApiMasks base;
    MicopiMaskEffectsCreateFn create_mask_effects;
    MicopiMaskEffectsDestroyFn destroy_mask_effects;
    MicopiMaskEffectRegionFn mask_effect_region;
    /* All four callbacks and the complete 72-byte table are required by cap 8. */
    MicopiMaskEffectsReserveFn reserve_mask_effects;
} MicopiCompositorApiMaskEffects;
const MicopiCompositorApi *micopi_compositor_v1(void);
/* Independent histogram: bounded centered grid, unweighted nonzero-alpha ARGB.
   Width/height 1..4096; max_samples 1..65536; stride >= width. Result bins are
   R/G/B and their aggregate. Both headers/capacities and disjoint spans validate
   before writes. No allocation, pointer retention, image writes or callbacks. */
#define MICOPI_HISTOGRAM_MAX_SAMPLES 65536
typedef struct { uint32_t abi_version, struct_size, width, height, stride, max_samples; } MicopiHistogramRequest;
typedef struct {
    uint32_t abi_version, struct_size, grid_width, grid_height;
    uint64_t sampled_pixels, nontransparent_pixels;
    uint64_t bins[4][256];
} MicopiHistogramResult;
typedef int32_t (*MicopiHistogramFn)(const MicopiHistogramRequest *, const uint32_t *, size_t, MicopiHistogramResult *);
typedef struct { uint32_t abi_version, struct_size; MicopiHistogramFn sample; } MicopiHistogramApi;
const MicopiHistogramApi *micopi_histogram_v1(void);

/* Independent cold Curves preparation. count=2..32 strictly input-ordered byte
   points; unused slots ignored. interpolation=0 linear, 1 shape-preserving
   Hermite with endpoint secants. Constant outside endpoint inputs. Exactly 256
   output bytes; invalid/null/overlapping spans reject before writes. No heap
   allocation or retained pointers; no document mutation or renderer capability. */
#define MICOPI_CURVES_MAX_POINTS 32
#define MICOPI_CURVES_LINEAR 0
#define MICOPI_CURVES_SMOOTH 1
typedef struct { uint32_t input, output; } MicopiCurvePoint;
typedef struct {
    uint32_t abi_version, struct_size, count, interpolation;
    MicopiCurvePoint points[MICOPI_CURVES_MAX_POINTS];
} MicopiCurvesParameters;
typedef int32_t (*MicopiCurvesPrepareFn)(const MicopiCurvesParameters *, uint8_t *, size_t);
typedef struct {
    uint32_t abi_version, struct_size;
    MicopiCurvesPrepareFn prepare;
} MicopiCurvesApi;
const MicopiCurvesApi *micopi_curves_v1(void);
/* Independent cold Levels preparation. Byte endpoints, input white > black,
   finite gamma 0.1..10. Writes exactly 256 bytes; rejects invalid or overlapping
   spans without writes. No allocation or retained pointers. */
typedef struct {
    uint32_t abi_version, struct_size;
    uint32_t input_black, input_white, output_black, output_white;
    double gamma;
} MicopiLevelsParameters;
typedef int32_t (*MicopiLevelsPrepareFn)(const MicopiLevelsParameters *, uint8_t *, size_t);
typedef struct {
    uint32_t abi_version, struct_size;
    MicopiLevelsPrepareFn prepare;
} MicopiLevelsApi;
const MicopiLevelsApi *micopi_levels_v1(void);
/* Complete optional 24-byte Levels tail. Kind 9: image a, LUT index mode,
   b=0, amount=1. At most 64 borrowed LUTs; preserve all alpha bytes. Legacy
   compositor entry points continue rejecting kind 9. */
typedef struct { uint8_t values[256]; } MicopiCompositeLut;
typedef int32_t (*MicopiLevelsRenderFn)(
    const MicopiCompositeNode *, size_t, const MicopiCompositeLayer *, size_t,
    const MicopiCompositeMask *, size_t, const MicopiCompositeLut *, size_t,
    uint32_t, uint32_t, MicopiPixels *);
typedef struct { MicopiLevelsApi base; MicopiLevelsRenderFn render; } MicopiLevelsApiRender;
/* Complete optional 40-byte Levels tail. Master is applied before the selected
   R/G/B table, with byte rounding at each step. Nested headers are version 1,
   size exactly 32; the outer record accepts larger known-prefix sizes.
   Output length counts CompositeLut records; exactly three are written only on
   success. Channel rendering accepts kind 10 and kind 9; old callbacks retain
   their previous rejection behavior. */
typedef struct {
    uint32_t abi_version, struct_size;
    MicopiLevelsParameters master, red, green, blue;
} MicopiLevelsChannelsParameters;
typedef int32_t (*MicopiLevelsChannelsPrepareFn)(
    const MicopiLevelsChannelsParameters *, MicopiCompositeLut *, size_t);
typedef struct {
    MicopiLevelsApiRender base;
    MicopiLevelsChannelsPrepareFn prepare_channels;
    MicopiLevelsRenderFn render_channels;
} MicopiLevelsApiChannels;
/* Complete optional Curves rendering tail. Kind 10: prior image a, b=0,
   amount=1; mode starts three consecutive R/G/B LUTs. Kind 9 remains supported.
   At most 64 total LUTs; legacy and Levels-only callbacks reject kind 10. */
typedef struct { MicopiCurvesApi base; MicopiLevelsRenderFn render; } MicopiCurvesApiRender;
/* Complete optional 24-byte histogram plan tail. Dense full-canvas immutable
   inputs; request stride equals width. At most 65,536 centered grid samples,
   64 instructions, 16 layers, 32 masks and 64 LUTs. No image allocation.
   Nonempty image plan; validate every declaration/loan before publication. */
typedef int32_t (*MicopiHistogramPlanFn)(
    const MicopiHistogramRequest *, const MicopiCompositeNode *, size_t,
    const MicopiCompositeLayer *, size_t, const MicopiCompositeMask *, size_t,
    const MicopiCompositeLut *, size_t, MicopiHistogramResult *);
typedef struct { MicopiHistogramApi base; MicopiHistogramPlanFn sample_plan; } MicopiHistogramApiPlan;
/* Independent optional exact geometry table, with immutable source and disjoint
   output. Local origins are zero; dimensions/stride are bounded to 4096.
   Kinds: 0 CW, 1 CCW, 2 half turn, 3 flip horizontal, 4 flip vertical.
   x/y are signed output offsets within +/-4096. Uncovered pixels become zero. */
typedef struct { uint32_t kind; int32_t x, y; } MicopiGeometry;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*pixels)(const MicopiGeometry *, const MicopiPixels *, MicopiPixels *);
    int32_t (*mask)(const MicopiGeometry *, const MicopiCoverage *, uint8_t *, size_t, uint32_t, uint32_t);
} MicopiGeometryApi;
const MicopiGeometryApi *micopi_geometry_v1(void);
/* Independent optional image resampling table. Same bounded disjoint storage as
   geometry; target dimensions determine scaling. Method 0 nearest center,
   1 bilinear with premultiplied alpha in encoded RGB. Equal dimensions copy raw. */
typedef struct { uint32_t method; } MicopiResample;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*pixels)(const MicopiResample *, const MicopiPixels *, MicopiPixels *);
    int32_t (*mask)(const MicopiResample *, const MicopiCoverage *, uint8_t *, size_t, uint32_t, uint32_t);
} MicopiResampleApi;
/* Optional complete tail; base methods 0/1 remain unchanged. Method 2 averages
   exact source footprints on shrinking axes, bilinear on equal/enlarging axes. */
#define MICOPI_RESAMPLE_AREA (1u << 2)
typedef struct { MicopiResampleApi base; uint32_t features; } MicopiResampleApiExt;
const MicopiResampleApi *micopi_resample_v1(void);
/* Independent stateless A8 sampler. All three v1 callbacks are mandatory.
   The old GeometryApi and ResampleApi layouts remain unchanged. Source/output
   dimensions and strides are bounded to 4096, lengths to 4096*4096 bytes.
   CompositeMask accepts validated dense or uniform sources. Output is a
   disjoint full frame with zero origin; preserve row padding and reject
   invalid options, descriptors and aliases before any write. */
typedef struct { int32_t x, y; uint32_t fill, reserved; } MicopiA8Reframe;
typedef struct { MicopiGeometry geometry; uint32_t fill; } MicopiA8Geometry;
/* Reframe/geometry offsets are signed destination pixels within +/-4096.
   Fill is 0..255; reserved is zero. No pointer outlives any callback. */
typedef int32_t (*MicopiA8ReframeFn)(const MicopiA8Reframe *, const MicopiCompositeMask *, uint32_t, uint32_t, MicopiA8Pixels *);
typedef int32_t (*MicopiA8GeometryFn)(const MicopiA8Geometry *, const MicopiCompositeMask *, uint32_t, uint32_t, MicopiA8Pixels *);
/* Methods 0/1/2 retain the existing exact u8 nearest/bilinear/area arithmetic. */
typedef int32_t (*MicopiA8ResampleFn)(const MicopiResample *, const MicopiCompositeMask *, uint32_t, uint32_t, MicopiA8Pixels *);
typedef struct {
    uint32_t abi_version, struct_size;
    MicopiA8ReframeFn reframe;
    MicopiA8GeometryFn geometry;
    MicopiA8ResampleFn resample;
} MicopiA8SamplerApi;
const MicopiA8SamplerApi *micopi_a8_sampler_v1(void);
/* Independent optional raw-A8 affine table; the existing sampler is unchanged.
   Forward row-major document-edge map. nearest0/bilinear1 sample inverse mapped
   pixel centers; each exterior tap uses fill0..255. Bilinear axis weights are
   Q16, scalar aggregate is rounded half-up. No premultiplication, fullframe
   scratch or retained pointer. Output origin0 and row padding is untouched. */
typedef struct { uint32_t method, fill; double matrix[6]; } MicopiA8Affine;
typedef int32_t (*MicopiA8AffineFn)(const MicopiA8Affine *, const MicopiCompositeMask *, uint32_t, uint32_t, MicopiA8Pixels *);
typedef struct { uint32_t abi_version, struct_size; MicopiA8AffineFn sample; } MicopiA8AffineApi;
typedef const MicopiA8AffineApi *(*MicopiA8AffineEntry)(void);
const MicopiA8AffineApi *micopi_a8_affine_v1(void);
/* Independent read-only trim analysis. Modes: 0 alpha <= tolerance, 1 top-left
   RGBA, 2 bottom-right RGBA. Color matching uses max channel difference; two
   zero-alpha colors match regardless of hidden RGB. Tolerance 0..255.
   Sides: top=1, right=2, bottom=4, left=8; a nonzero subset is required. */
typedef struct { uint32_t mode, tolerance, sides; } MicopiTrim;
/* Both dimensions zero mean all pixels match; otherwise an in-bounds frame. */
typedef struct { uint32_t x, y, width, height; } MicopiTrimBounds;
/* Optional raw-A8 affine tail, declared after its bounds output type.
   Bounds cover samples different from neutral0..255. Neutral uniform input
   returns zero bounds; different uniform input covers the full frame. Output
   must not overlap the source descriptor or dense source bytes. Inspect the
   base struct_size before reading the optional, nullable bounds callback. */
typedef int32_t (*MicopiA8AffineBoundsFn)(const MicopiCompositeMask *, uint32_t, uint32_t, uint32_t, MicopiTrimBounds *);
typedef struct { MicopiA8AffineApi base; MicopiA8AffineBoundsFn bounds; } MicopiA8AffineApiExt;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*bounds)(const MicopiTrim *, const MicopiPixels *, MicopiTrimBounds *);
} MicopiTrimApi;
const MicopiTrimApi *micopi_trim_v1(void);
/* Independent integer movement. Kind 0 whole layer (absent coverage),
   1 selected pixels, 2 mask only. Offsets within +/-4096; excess is clipped.
   Immutable scoped sources, disjoint outputs; mask output is dense. */
typedef struct { uint32_t kind; int32_t dx, dy; } MicopiMove;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*pixels)(const MicopiMove *, const MicopiPixels *, const MicopiCoverage *, MicopiPixels *);
    int32_t (*mask)(const MicopiMove *, const MicopiCoverage *, uint8_t *, size_t);
} MicopiMoveApi;
const MicopiMoveApi *micopi_move_v1(void);
/* Optional affine sampling. Kind 0 layer, 1 selected pixels, 2 mask only.
   Method 0 nearest, 1 bilinear. Forward document-edge matrix:
   x'=m[0]*x+m[1]*y+m[2], y'=m[3]*x+m[4]*y+m[5]. */
typedef struct { uint32_t kind, method; double matrix[6]; } MicopiAffine;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*pixels)(const MicopiAffine *, const MicopiPixels *, const MicopiCoverage *, MicopiPixels *);
    int32_t (*mask)(const MicopiAffine *, const MicopiCoverage *, uint8_t *, size_t);
    int32_t (*bounds)(const MicopiPixels *, MicopiTrimBounds *);
} MicopiAffineApi;
const MicopiAffineApi *micopi_affine_v1(void);
/* Optional finite forward homography. Same kinds/methods as affine.
   x'=(m0*x+m1*y+m2)/(m6*x+m7*y+m8), likewise y' with m3..m5.
   Samples behind the forward horizon are transparent. */
typedef struct { uint32_t kind, method; double matrix[9]; } MicopiProjective;
typedef struct {
    uint32_t abi_version, struct_size;
    int32_t (*pixels)(const MicopiProjective *, const MicopiPixels *, const MicopiCoverage *, MicopiPixels *);
    int32_t (*mask)(const MicopiProjective *, const MicopiCoverage *, uint8_t *, size_t);
} MicopiProjectiveApi;
const MicopiProjectiveApi *micopi_projective_v1(void);

/* Optional independent HSL plan API. Kind 11 uses a=image input, b=0,
   mode=parameter index, amount=1. Fixed-stride finite parameter records;
   hue is [-180,180] degrees; saturation/lightness are [-100,100] percent.
   All declarations validate before writes. Both callbacks are mandatory. */
#define MICOPI_HSL_MAX_PARAMETERS 64
typedef struct { double hue, saturation, lightness; } MicopiHslParameters;
typedef int32_t (*MicopiHslRenderFn)(const MicopiCompositeNode *, size_t,
    const MicopiCompositeLayer *, size_t, const MicopiCompositeMask *, size_t,
    const MicopiCompositeLut *, size_t, const MicopiHslParameters *, size_t,
    uint32_t, uint32_t, MicopiPixels *);
typedef int32_t (*MicopiHslHistogramFn)(const MicopiHistogramRequest *,
    const MicopiCompositeNode *, size_t, const MicopiCompositeLayer *, size_t,
    const MicopiCompositeMask *, size_t, const MicopiCompositeLut *, size_t,
    const MicopiHslParameters *, size_t, MicopiHistogramResult *);
typedef struct { uint32_t abi_version, struct_size;
    MicopiHslRenderFn render; MicopiHslHistogramFn histogram;
} MicopiHslApi;
const MicopiHslApi *micopi_hsl_v1(void);

/* Optional independent superset with unchanged callback signatures.
   Kind 12: Threshold, mode=level 0..255. Kind 13: Posterize, mode=levels
   2..256. Both: a=earlier image, b=0, amount=1. Existing tables reject them.
   Both callbacks are mandatory; use size/version checks before reading. */
typedef MicopiHslApi MicopiQuantizationApi;
const MicopiQuantizationApi *micopi_quantization_v1(void);

#ifdef __cplusplus
}
#endif
#endif
