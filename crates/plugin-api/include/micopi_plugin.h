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
#ifdef __cplusplus
}
#endif
#endif
