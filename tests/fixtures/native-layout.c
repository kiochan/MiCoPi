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
int main(void) {
    MicopiCompositeRenderMaskedFn callback = masked;
    MicopiToolHostA8 host;
    host.a8 = byte_loan;
    return callback == masked && host.a8 == byte_loan ? 0 : 1;
}
#ifdef __cplusplus
}
#endif
