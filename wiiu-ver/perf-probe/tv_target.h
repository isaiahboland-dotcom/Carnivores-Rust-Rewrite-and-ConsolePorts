#include "tv_layout.h"
/* The TV's world at 1280x720, 2.25 times fewer pixels than 1080p. It is
 * drawn into the memory of WHB's 1080p TV depth buffer (fast MEM1), which
 * nothing needs once the world is done: the picture is then scaled up onto
 * the TV's colour buffer (bilinear), and the HUD, the overlay and the
 * performance text go on top at full resolution without depth. WHB frees and
 * reallocates that memory across a foreground release, so tv_target_bind
 * finds it again every frame. Only when the TV buffer is taller than 720. */
#define TV_WORLD_WIDTH 1280
#define TV_WORLD_HEIGHT 720
typedef struct { GX2ColorBuffer color; GX2DepthBuffer depth; GX2Texture texture; int available; } TvTarget;
static void tv_target_init(TvTarget *t) {
    memset(t, 0, sizeof(*t));
    GX2DepthBuffer *whb = WHBGfxGetTVDepthBuffer();
    if (WHBGfxGetTVColourBuffer()->surface.height <= TV_WORLD_HEIGHT) return;
    GX2Surface *c = &t->color.surface, *d = &t->depth.surface;
    c->use = GX2_SURFACE_USE_TEXTURE | GX2_SURFACE_USE_COLOR_BUFFER;
    c->dim = GX2_SURFACE_DIM_TEXTURE_2D; c->width = TV_WORLD_WIDTH; c->height = TV_WORLD_HEIGHT;
    c->depth = 1; c->mipLevels = 1; c->format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    c->aa = GX2_AA_MODE1X; c->tileMode = GX2_TILE_MODE_DEFAULT;
    t->color.viewNumSlices = 1;
    GX2CalcSurfaceSizeAndAlignment(c);
    GX2InitColorBufferRegs(&t->color);
    // As WHB's own depth buffer, at the smaller size.
    *d = whb->surface; d->image = NULL; d->mipmaps = NULL; d->swizzle = 0;
    d->width = TV_WORLD_WIDTH; d->height = TV_WORLD_HEIGHT;
    t->depth.viewNumSlices = 1; t->depth.depthClear = 1.0f;
    GX2CalcSurfaceSizeAndAlignment(d);
    GX2InitDepthBufferRegs(&t->depth);
    t->texture.surface = *c;
    t->texture.viewNumMips = 1; t->texture.viewNumSlices = 1;
    t->texture.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&t->texture);
    uint32_t offset = 0;
    t->available = tv_layout((uintptr_t)whb->surface.image, whb->surface.imageSize, d->imageSize, d->alignment,
                             c->imageSize, c->alignment, &offset);
    OSReport("C2 TV world target %ux%u in the TV depth memory (%u of %u bytes): %s\n", TV_WORLD_WIDTH, TV_WORLD_HEIGHT,
             offset + c->imageSize, whb->surface.imageSize, t->available ? "PASS" : "does not fit, native resolution");
}
/* Point the target at this frame's TV depth memory; 0 to draw at native
 * resolution instead. */
static int tv_target_bind(TvTarget *t) {
    if (!t->available) return 0;
    GX2DepthBuffer *whb = WHBGfxGetTVDepthBuffer();
    uint32_t offset;
    if (!tv_layout((uintptr_t)whb->surface.image, whb->surface.imageSize, t->depth.surface.imageSize,
                   t->depth.surface.alignment, t->color.surface.imageSize, t->color.surface.alignment, &offset)) return 0;
    t->depth.surface.image = whb->surface.image;
    t->color.surface.image = (uint8_t *)whb->surface.image + offset;
    t->texture.surface.image = t->color.surface.image;
    return 1;
}
