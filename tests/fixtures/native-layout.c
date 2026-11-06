/* Compile-only C/C++ checks for the published x64 ABI and optional mask tails. */
#include "micopi_plugin.h"
#ifdef __cplusplus
#define ABI_ASSERT static_assert
extern "C" {
#else
#define ABI_ASSERT _Static_assert
#endif

ABI_ASSERT(sizeof(void *) == 8, "The supported native SDK target is x64");
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
int main(void) {
    MicopiCompositeRenderMaskedFn callback = masked;
    MicopiToolHostA8 host;
    host.a8 = byte_loan;
    MicopiCompositorApiMaskEffects effects;
    effects.create_mask_effects = create_effects;
    effects.destroy_mask_effects = destroy_effects;
    effects.mask_effect_region = effect_region;
    effects.reserve_mask_effects = reserve_effects;
    MicopiA8SamplerApi sampler;
    sampler.reframe = a8_reframe;
    sampler.geometry = a8_geometry;
    sampler.resample = a8_resample;
    return callback == masked && host.a8 == byte_loan
        && effects.create_mask_effects == create_effects
        && effects.destroy_mask_effects == destroy_effects
        && effects.mask_effect_region == effect_region
        && effects.reserve_mask_effects == reserve_effects
        && sampler.reframe == a8_reframe
        && sampler.geometry == a8_geometry
        && sampler.resample == a8_resample ? 0 : 1;
}
#ifdef __cplusplus
}
#endif
