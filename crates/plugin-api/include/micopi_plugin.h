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
typedef struct {
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
const MicopiCompositorApi *micopi_compositor_v1(void);
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
/* Independent read-only trim analysis. Modes: 0 alpha <= tolerance, 1 top-left
   RGBA, 2 bottom-right RGBA. Color matching uses max channel difference; two
   zero-alpha colors match regardless of hidden RGB. Tolerance 0..255.
   Sides: top=1, right=2, bottom=4, left=8; a nonzero subset is required. */
typedef struct { uint32_t mode, tolerance, sides; } MicopiTrim;
/* Both dimensions zero mean all pixels match; otherwise an in-bounds frame. */
typedef struct { uint32_t x, y, width, height; } MicopiTrimBounds;
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
#ifdef __cplusplus
}
#endif
#endif
