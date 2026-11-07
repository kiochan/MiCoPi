/* Compile-only C/C++ checks for the published x64 ABI and optional tails. */
#include "micopi_plugin.h"
#ifdef __cplusplus
#include <type_traits>
#define ABI_ASSERT static_assert
extern "C" {
#else
#define ABI_ASSERT _Static_assert
#endif

ABI_ASSERT(sizeof(void *) == 8, "The supported native SDK target is x64");
ABI_ASSERT(sizeof(MicopiHistogramRequest) == 24, "Histogram request");
ABI_ASSERT(offsetof(MicopiHistogramRequest, max_samples) == 20, "Histogram budget offset");
ABI_ASSERT(sizeof(MicopiHistogramResult) == 8224, "Histogram result");
ABI_ASSERT(offsetof(MicopiHistogramResult, sampled_pixels) == 16, "Histogram count offset");
ABI_ASSERT(offsetof(MicopiHistogramResult, bins) == 32, "Histogram bins offset");
ABI_ASSERT(sizeof(MicopiHistogramApi) == 16, "Independent histogram API");
ABI_ASSERT(offsetof(MicopiHistogramApi, sample) == 8, "Histogram callback offset");
ABI_ASSERT(sizeof(MicopiCurvePoint) == 8, "Curves point pair");
ABI_ASSERT(sizeof(MicopiCurvesParameters) == 272, "Bounded Curves parameters");
ABI_ASSERT(offsetof(MicopiCurvesParameters, count) == 8, "Curves count offset");
ABI_ASSERT(offsetof(MicopiCurvesParameters, interpolation) == 12, "Curves mode offset");
ABI_ASSERT(offsetof(MicopiCurvesParameters, points) == 16, "Curves points offset");
ABI_ASSERT(sizeof(MicopiCurvesApi) == 16, "Independent Curves API");
ABI_ASSERT(offsetof(MicopiCurvesApi, prepare) == 8, "Curves preparation callback");
ABI_ASSERT(sizeof(MicopiCurvesApiRender) == 24, "Complete Curves render tail");
ABI_ASSERT(offsetof(MicopiCurvesApiRender, render) == 16, "Curves render offset");
ABI_ASSERT(sizeof(MicopiLevelsParameters) == 32, "Levels preparation parameters");
ABI_ASSERT(offsetof(MicopiLevelsParameters, gamma) == 24, "Levels gamma offset");
ABI_ASSERT(sizeof(MicopiLevelsApi) == 16, "Independent Levels API");
ABI_ASSERT(sizeof(MicopiCompositeLut) == 256, "Levels byte lookup table");
ABI_ASSERT(sizeof(MicopiLevelsApiRender) == 24, "Complete Levels rendering tail");
ABI_ASSERT(offsetof(MicopiLevelsApiRender, render) == 16, "Levels render callback offset");
ABI_ASSERT(offsetof(MicopiLevelsApi, prepare) == 8, "Levels preparation callback");
ABI_ASSERT(sizeof(MicopiLevelsChannelsParameters) == 136, "Channel Levels intent");
ABI_ASSERT(offsetof(MicopiLevelsChannelsParameters, master) == 8, "Master Levels offset");
ABI_ASSERT(offsetof(MicopiLevelsChannelsParameters, red) == 40, "Red Levels offset");
ABI_ASSERT(offsetof(MicopiLevelsChannelsParameters, green) == 72, "Green Levels offset");
ABI_ASSERT(offsetof(MicopiLevelsChannelsParameters, blue) == 104, "Blue Levels offset");
ABI_ASSERT(sizeof(MicopiLevelsApiChannels) == 40, "Complete channel Levels tail");
ABI_ASSERT(offsetof(MicopiLevelsApiChannels, prepare_channels) == 24, "Channel preparation offset");
ABI_ASSERT(offsetof(MicopiLevelsApiChannels, render_channels) == 32, "Channel rendering offset");
ABI_ASSERT(sizeof(MicopiPixels) == 40, "Legacy pixel descriptor");
ABI_ASSERT(sizeof(MicopiToolHost) == 32, "Legacy tool host prefix");
ABI_ASSERT(sizeof(MicopiToolApi) == 32, "Legacy tool API prefix");
ABI_ASSERT(sizeof(MicopiToolParameters) == 32, "Legacy frozen parameters");
ABI_ASSERT(sizeof(MicopiA8Pixels) == 40, "A8 byte descriptor");
ABI_ASSERT(offsetof(MicopiA8Pixels, len) == 8, "A8 length offset");
ABI_ASSERT(offsetof(MicopiA8Pixels, origin_y) == 32, "A8 origin offset");
ABI_ASSERT(sizeof(MicopiToolHostA8) == 40, "Complete A8 host tail");
ABI_ASSERT(offsetof(MicopiToolHostA8, a8) == 32, "A8 callback offset");
ABI_ASSERT(MICOPI_TOOL_OPTION_A8_TARGET == 4, "Independent A8 option flag");
ABI_ASSERT(sizeof(MicopiToolOptions) == 16, "Legacy tool options prefix");
ABI_ASSERT(sizeof(MicopiToolOptionsExt) == 20, "Legacy flagged options prefix");
ABI_ASSERT(sizeof(MicopiCoverage) == 32, "Legacy frozen selection descriptor");
ABI_ASSERT(sizeof(MicopiToolOptionsSelection) == 56, "Legacy selection options tail");
ABI_ASSERT(sizeof(MicopiToolOptionsApi) == 16, "Legacy options API prefix");
ABI_ASSERT(sizeof(MicopiToolOptionsApiExt) == 24, "Legacy options features tail");
ABI_ASSERT(sizeof(MicopiOriginPixels) == 40, "Immutable RGBA origin descriptor");
ABI_ASSERT(sizeof(MicopiOriginA8Pixels) == 40, "Immutable A8 origin descriptor");
ABI_ASSERT(offsetof(MicopiOriginPixels, len) == 8, "RGBA origin length offset");
ABI_ASSERT(offsetof(MicopiOriginPixels, stride) == 24, "RGBA origin stride offset");
ABI_ASSERT(offsetof(MicopiOriginPixels, origin_y) == 32, "RGBA origin coordinate offset");
ABI_ASSERT(offsetof(MicopiOriginA8Pixels, len) == 8, "A8 origin length offset");
ABI_ASSERT(offsetof(MicopiOriginA8Pixels, origin_y) == 32, "A8 origin coordinate offset");
ABI_ASSERT(sizeof(MicopiToolOptionsStroke) == 64, "Complete stroke options tail");
ABI_ASSERT(offsetof(MicopiToolOptionsStroke, base) == 0, "Stroke options preserve selection prefix");
ABI_ASSERT(offsetof(MicopiToolOptionsStroke, flow) == 56, "Stroke flow offset");
ABI_ASSERT(offsetof(MicopiToolOptionsStroke, reserved) == 60, "Stroke reserved offset");
ABI_ASSERT(sizeof(MicopiToolOptionsTip) == 80, "Complete analytic tip options tail");
ABI_ASSERT(offsetof(MicopiToolOptionsTip, base) == 0, "Tip options preserve stroke prefix");
ABI_ASSERT(offsetof(MicopiToolOptionsTip, tip_shape) == 64, "Tip shape offset");
ABI_ASSERT(offsetof(MicopiToolOptionsTip, roundness) == 68, "Tip roundness offset");
ABI_ASSERT(offsetof(MicopiToolOptionsTip, angle) == 72, "Tip angle offset");
ABI_ASSERT(offsetof(MicopiToolOptionsTip, reserved) == 76, "Tip reserved offset");
ABI_ASSERT(MICOPI_TOOL_OPTION_TIP_GEOMETRY == 16, "Independent analytic tip feature");
ABI_ASSERT((MICOPI_TOOL_OPTION_TIP_GEOMETRY & (MICOPI_TOOL_OPTION_ALPHA_LOCK | MICOPI_TOOL_OPTION_SELECTION | MICOPI_TOOL_OPTION_A8_TARGET | MICOPI_TOOL_OPTION_STROKE_OPACITY)) == 0, "Tip preserves existing feature bits");
ABI_ASSERT(MICOPI_TIP_ROUND == 0 && MICOPI_TIP_SQUARE == 1, "Stable analytic shape IDs");
ABI_ASSERT(sizeof(MicopiToolHostStroke) == 64, "Complete origin host tail");
ABI_ASSERT(offsetof(MicopiToolHostStroke, base) == 0, "Stroke host preserves A8 prefix");
ABI_ASSERT(offsetof(MicopiToolHostStroke, features) == 40, "Origin host features offset");
ABI_ASSERT(offsetof(MicopiToolHostStroke, tile_side) == 44, "Origin host tile side offset");
ABI_ASSERT(offsetof(MicopiToolHostStroke, pixels_origin) == 48, "RGBA original loan offset");
ABI_ASSERT(offsetof(MicopiToolHostStroke, a8_origin) == 56, "A8 original loan offset");
ABI_ASSERT(MICOPI_TOOL_OPTION_STROKE_OPACITY == 8, "Independent stroke option flag");
ABI_ASSERT((MICOPI_TOOL_OPTION_STROKE_OPACITY & (MICOPI_TOOL_OPTION_ALPHA_LOCK | MICOPI_TOOL_OPTION_SELECTION | MICOPI_TOOL_OPTION_A8_TARGET)) == 0, "Stroke option preserves old feature bits");
ABI_ASSERT(MICOPI_TOOL_HOST_ORIGIN_RGBA == 1, "RGBA original loan capability");
ABI_ASSERT(MICOPI_TOOL_HOST_ORIGIN_A8 == 2, "A8 original loan capability");
#ifdef __cplusplus
ABI_ASSERT((std::is_same<decltype(((MicopiOriginPixels *)0)->data), const uint32_t *>::value), "RGBA origin samples are immutable");
ABI_ASSERT((std::is_same<decltype(((MicopiOriginA8Pixels *)0)->data), const uint8_t *>::value), "A8 origin samples are immutable");
#else
ABI_ASSERT(_Generic(((MicopiOriginPixels *)0)->data, const uint32_t *: 1, default: 0), "RGBA origin samples are immutable");
ABI_ASSERT(_Generic(((MicopiOriginA8Pixels *)0)->data, const uint8_t *: 1, default: 0), "A8 origin samples are immutable");
#endif
ABI_ASSERT(sizeof(MicopiCompositeNode) == 20, "Legacy instruction layout");
ABI_ASSERT(sizeof(MicopiCompositeLayer) == 32, "Legacy layer descriptor");
ABI_ASSERT(sizeof(MicopiCompositorApi) == 16, "Legacy compositor prefix");
ABI_ASSERT(sizeof(MicopiCompositorApiFeatures) == 24, "Features prefix");
ABI_ASSERT(sizeof(MicopiCompositorApiModes) == 32, "Modes prefix");
ABI_ASSERT(sizeof(MicopiCompositeMask) == 24, "Uniform/strided coverage descriptor");
ABI_ASSERT(offsetof(MicopiCompositeMask, stride) == 16, "Coverage stride offset");
ABI_ASSERT(offsetof(MicopiCompositeMask, constant) == 20, "Uniform byte offset");
ABI_ASSERT(sizeof(MicopiCompositorApiMasks) == 40, "Complete masked compositor tail");
ABI_ASSERT(offsetof(MicopiCompositorApiMasks, render_masked) == 32, "Masked callback offset");
ABI_ASSERT(MICOPI_COMPOSITOR_MASKS == 4, "Independent mask capability");
ABI_ASSERT(sizeof(MicopiMaskEffectRequest) == 16, "Mask effect request layout");
ABI_ASSERT(sizeof(MicopiCompositorApiMaskEffects) == 72, "Complete mask effects and reserve tail");
ABI_ASSERT(offsetof(MicopiCompositorApiMaskEffects, create_mask_effects) == 40, "Mask effects create offset");
ABI_ASSERT(offsetof(MicopiCompositorApiMaskEffects, mask_effect_region) == 56, "Mask effects region offset");
ABI_ASSERT(offsetof(MicopiCompositorApiMaskEffects, reserve_mask_effects) == 64, "Mask effects reserve offset");
ABI_ASSERT(MICOPI_COMPOSITOR_MASK_EFFECTS == 8, "Independent mask effects capability");
ABI_ASSERT(sizeof(MicopiGeometry) == 12, "Legacy geometry request");
ABI_ASSERT(sizeof(MicopiGeometryApi) == 24, "Legacy geometry table");
ABI_ASSERT(sizeof(MicopiResampleApi) == 24, "Legacy resample table");
ABI_ASSERT(sizeof(MicopiResampleApiExt) == 32, "Legacy resample area tail");
ABI_ASSERT(sizeof(MicopiA8Reframe) == 16, "A8 reframe request");
ABI_ASSERT(offsetof(MicopiA8Reframe, fill) == 8, "A8 reframe fill offset");
ABI_ASSERT(offsetof(MicopiA8Reframe, reserved) == 12, "A8 reframe reserved offset");
ABI_ASSERT(sizeof(MicopiA8Geometry) == 16, "A8 exact geometry request");
ABI_ASSERT(offsetof(MicopiA8Geometry, fill) == 12, "A8 geometry fill offset");
ABI_ASSERT(sizeof(MicopiA8SamplerApi) == 32, "Independent A8 sampler table");
ABI_ASSERT(offsetof(MicopiA8SamplerApi, reframe) == 8, "A8 reframe callback offset");
ABI_ASSERT(offsetof(MicopiA8SamplerApi, geometry) == 16, "A8 geometry callback offset");
ABI_ASSERT(offsetof(MicopiA8SamplerApi, resample) == 24, "A8 resample callback offset");
ABI_ASSERT(sizeof(MicopiA8Affine) == 56, "Raw A8 affine request");
ABI_ASSERT(offsetof(MicopiA8Affine, method) == 0, "A8 affine method offset");
ABI_ASSERT(offsetof(MicopiA8Affine, fill) == 4, "A8 affine fill offset");
ABI_ASSERT(offsetof(MicopiA8Affine, matrix) == 8, "A8 affine matrix offset");
ABI_ASSERT(sizeof(MicopiA8AffineApi) == 16, "Independent A8 affine table");
ABI_ASSERT(offsetof(MicopiA8AffineApi, sample) == 8, "A8 affine callback offset");
ABI_ASSERT(sizeof(MicopiA8AffineApiExt) == 24, "Optional A8 bounds tail");
ABI_ASSERT(offsetof(MicopiA8AffineApiExt, base) == 0, "A8 bounds base prefix");
ABI_ASSERT(offsetof(MicopiA8AffineApiExt, bounds) == 16, "A8 bounds callback offset");

static int32_t masked(const MicopiCompositeNode *nodes, size_t count,
                      const MicopiCompositeLayer *layers, size_t layer_count,
                      const MicopiCompositeMask *masks, size_t mask_count,
                      uint32_t width, uint32_t height, MicopiPixels *output) {
    (void)nodes; (void)count; (void)layers; (void)layer_count;
    (void)masks; (void)mask_count; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static int32_t byte_loan(void *context, MicopiRegion region,
                         MicopiA8Transform transform, void *user) {
    (void)context; (void)region; (void)transform; (void)user;
    return MICOPI_OK;
}
static int32_t origin_transform(void *user, const MicopiOriginPixels *origin, MicopiPixels *current) {
    (void)user; (void)origin; (void)current;
    return MICOPI_OK;
}
static int32_t a8_origin_transform(void *user, const MicopiOriginA8Pixels *origin, MicopiA8Pixels *current) {
    (void)user; (void)origin; (void)current;
    return MICOPI_OK;
}
static int32_t origin_loan(void *context, MicopiRegion region, MicopiOriginTransform transform, void *user) {
    (void)context; (void)region; (void)transform; (void)user;
    return MICOPI_OK;
}
static int32_t a8_origin_loan(void *context, MicopiRegion region, MicopiA8OriginTransform transform, void *user) {
    (void)context; (void)region; (void)transform; (void)user;
    return MICOPI_OK;
}
static void *create_effects(void) { return NULL; }
static void destroy_effects(void *context) { (void)context; }
static int32_t effect_region(void *context, const MicopiMaskEffectRequest *request,
                             const MicopiCompositeMask *source, uint32_t width,
                             uint32_t height, MicopiA8Pixels *output) {
    (void)context; (void)request; (void)source; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static int32_t reserve_effects(void *context, uint32_t width, uint32_t height, uint32_t radius) {
    (void)context; (void)width; (void)height; (void)radius;
    return MICOPI_OK;
}
static int32_t a8_reframe(const MicopiA8Reframe *request, const MicopiCompositeMask *source,
                           uint32_t width, uint32_t height, MicopiA8Pixels *output) {
    (void)request; (void)source; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static int32_t a8_geometry(const MicopiA8Geometry *request, const MicopiCompositeMask *source,
                            uint32_t width, uint32_t height, MicopiA8Pixels *output) {
    (void)request; (void)source; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static int32_t a8_resample(const MicopiResample *request, const MicopiCompositeMask *source,
                            uint32_t width, uint32_t height, MicopiA8Pixels *output) {
    (void)request; (void)source; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static int32_t a8_affine(const MicopiA8Affine *request, const MicopiCompositeMask *source,
                          uint32_t width, uint32_t height, MicopiA8Pixels *output) {
    (void)request; (void)source; (void)width; (void)height; (void)output;
    return MICOPI_OK;
}
static const MicopiA8AffineApi *a8_affine_entry(void) { return NULL; }
static int32_t a8_affine_bounds(const MicopiCompositeMask *source, uint32_t width,
                               uint32_t height, uint32_t neutral, MicopiTrimBounds *output) {
    (void)source; (void)width; (void)height; (void)neutral; (void)output;
    return MICOPI_OK;
}
#ifdef __cplusplus
}
#endif

int main(void) {
    MicopiCompositeRenderMaskedFn callback = masked;
    MicopiToolHostA8 host;
    host.a8 = byte_loan;
    MicopiOriginTransform rgba_original_callback = origin_transform;
    MicopiA8OriginTransform a8_original_callback = a8_origin_transform;
    MicopiOriginPixelsLoan rgba_original_loan = origin_loan;
    MicopiOriginA8Loan a8_original_loan_callback = a8_origin_loan;
    MicopiToolHostStroke stroke_host;
    stroke_host.pixels_origin = rgba_original_loan;
    stroke_host.a8_origin = a8_original_loan_callback;
    stroke_host.features = MICOPI_TOOL_HOST_ORIGIN_RGBA | MICOPI_TOOL_HOST_ORIGIN_A8;
    stroke_host.tile_side = 64;
    MicopiToolOptionsStroke stroke_options;
    stroke_options.flow = 1.0f;
    stroke_options.reserved = 0;
    MicopiToolOptionsTip tip_options;
    tip_options.base.flow = 1.0f;
    tip_options.tip_shape = MICOPI_TIP_SQUARE;
    tip_options.roundness = 0.25f;
    tip_options.angle = 45.0f;
    tip_options.reserved = 0;
    MicopiToolOptions *tip_prefix = &tip_options.base.base.base.base;
    MicopiCompositorApiMaskEffects effects;
    effects.create_mask_effects = create_effects;
    effects.destroy_mask_effects = destroy_effects;
    effects.mask_effect_region = effect_region;
    effects.reserve_mask_effects = reserve_effects;
    MicopiA8SamplerApi sampler;
    sampler.reframe = a8_reframe;
    sampler.geometry = a8_geometry;
    sampler.resample = a8_resample;
    MicopiA8AffineFn affine_callback = a8_affine;
    MicopiA8AffineEntry affine_entry_callback = a8_affine_entry;
    MicopiA8AffineApi affine_sampler;
    affine_sampler.abi_version = 1;
    affine_sampler.struct_size = sizeof(MicopiA8AffineApi);
    affine_sampler.sample = affine_callback;
    MicopiA8AffineBoundsFn bounds_callback = a8_affine_bounds;
    MicopiA8AffineApiExt affine_sampler_ext;
    affine_sampler_ext.base = affine_sampler;
    affine_sampler_ext.base.struct_size = sizeof(MicopiA8AffineApiExt);
    affine_sampler_ext.bounds = bounds_callback;
    return callback == masked && host.a8 == byte_loan
        && rgba_original_callback == origin_transform
        && a8_original_callback == a8_origin_transform
        && stroke_host.pixels_origin == origin_loan
        && stroke_host.a8_origin == a8_origin_loan
        && stroke_host.features == 3 && stroke_host.tile_side == 64
        && stroke_options.flow == 1.0f && stroke_options.reserved == 0
        && tip_options.tip_shape == MICOPI_TIP_SQUARE && tip_options.roundness == 0.25f
        && tip_options.angle == 45.0f && tip_options.reserved == 0
        && tip_prefix == (MicopiToolOptions *)&tip_options
        && effects.create_mask_effects == create_effects
        && effects.destroy_mask_effects == destroy_effects
        && effects.mask_effect_region == effect_region
        && effects.reserve_mask_effects == reserve_effects
        && sampler.reframe == a8_reframe
        && sampler.geometry == a8_geometry
        && sampler.resample == a8_resample
        && affine_sampler.sample == a8_affine
        && affine_sampler_ext.base.sample == a8_affine
        && affine_sampler_ext.bounds == a8_affine_bounds
        && affine_entry_callback == a8_affine_entry ? 0 : 1;
}
