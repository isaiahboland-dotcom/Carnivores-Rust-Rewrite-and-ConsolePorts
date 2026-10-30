#include "atlas_mips.h"
/* Copy a linear RGBA8 atlas into a 2D-tiled R5G5B5A1 one with
 * ATLAS_MIP_LEVELS mip levels (built on the CPU from level 0 in RGBA8, then
 * each level narrowed to 16 bits, see atlas_mips.h): half the texture
 * traffic, and level 0 loses nothing. Conversion and validation happen only
 * during loading, never in the measured gameplay loop. `flat` gets a
 * one-level view of the same texture for the in-hunt A/B (R3). On any
 * failure the linear atlas stays in use. */
static int atlas_tiled_copy(const GX2Texture *linear, GX2Texture *tiled, GX2Texture *flat) {
    *tiled = *linear;
    tiled->surface.image = NULL; tiled->surface.mipmaps = NULL;
    tiled->surface.tileMode = GX2_TILE_MODE_TILED_2D_THIN1;
    tiled->surface.format = GX2_SURFACE_FORMAT_UNORM_R5_G5_B5_A1;
    tiled->surface.mipLevels = ATLAS_MIP_LEVELS;
    tiled->surface.swizzle = 0;
    GX2CalcSurfaceSizeAndAlignment(&tiled->surface);
    tiled->surface.image = memalign(tiled->surface.alignment, tiled->surface.imageSize);
    tiled->surface.mipmaps = memalign(tiled->surface.alignment, tiled->surface.mipmapSize);
    GX2Surface level[ATLAS_MIP_LEVELS], narrow[ATLAS_MIP_LEVELS], check[ATLAS_MIP_LEVELS];
    memset(level, 0, sizeof(level)); memset(narrow, 0, sizeof(narrow)); memset(check, 0, sizeof(check));
    int valid = tiled->surface.image && tiled->surface.mipmaps;
    if (valid) {
        // A fresh allocation can contain dirty CPU cache lines from its prior owner.
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, tiled->surface.image, tiled->surface.imageSize);
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, tiled->surface.mipmaps, tiled->surface.mipmapSize);
    }
    level[0] = linear->surface;
    // Each smaller level is its own linear surface, copied into the tiled
    // chain by GX2CopySurface (which knows the chain's layout).
    for (uint32_t k = 1; valid && k < ATLAS_MIP_LEVELS; ++k) {
        level[k] = linear->surface;
        level[k].width = linear->surface.width >> k; level[k].height = linear->surface.height >> k;
        level[k].image = NULL;
        GX2CalcSurfaceSizeAndAlignment(&level[k]);
        level[k].image = memalign(level[k].alignment, level[k].imageSize);
        if (!level[k].image) { valid = 0; break; }
        atlas_mip_halve(level[k - 1].image, level[k - 1].pitch, level[k].image, level[k].pitch,
                        level[k].width, level[k].height);
    }
    // Every level narrowed to a linear 16-bit surface of its own.
    for (uint32_t k = 0; valid && k < ATLAS_MIP_LEVELS; ++k) {
        narrow[k] = level[k];
        narrow[k].format = GX2_SURFACE_FORMAT_UNORM_R5_G5_B5_A1;
        narrow[k].image = NULL;
        GX2CalcSurfaceSizeAndAlignment(&narrow[k]);
        narrow[k].image = memalign(narrow[k].alignment, narrow[k].imageSize);
        if (!narrow[k].image) { valid = 0; break; }
        atlas_to_rgb5a1(level[k].image, level[k].pitch, narrow[k].image, narrow[k].pitch,
                        narrow[k].width, narrow[k].height);
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, narrow[k].image, narrow[k].imageSize);
    }
    if (valid) {
        for (uint32_t k = 0; k < ATLAS_MIP_LEVELS; ++k) GX2CopySurface(&narrow[k], 0, 0, &tiled->surface, k, 0);
        GX2DrawDone();
        GX2Invalidate(GX2_INVALIDATE_MODE_TEXTURE, tiled->surface.image, tiled->surface.imageSize);
        GX2Invalidate(GX2_INVALIDATE_MODE_TEXTURE, tiled->surface.mipmaps, tiled->surface.mipmapSize);
        // Read every level back to a linear surface and verify every texel.
        for (uint32_t k = 0; valid && k < ATLAS_MIP_LEVELS; ++k) {
            check[k] = narrow[k];
            check[k].image = memalign(check[k].alignment, check[k].imageSize);
            if (!check[k].image) { valid = 0; break; }
            DCFlushRange(check[k].image, check[k].imageSize);
            GX2CopySurface(&tiled->surface, k, 0, &check[k], 0, 0);
        }
        GX2DrawDone();
        for (uint32_t k = 0; valid && k < ATLAS_MIP_LEVELS; ++k) {
            DCInvalidateRange(check[k].image, check[k].imageSize);
            for (uint32_t y = 0; y < check[k].height; ++y) {
                if (memcmp((const uint16_t *)narrow[k].image + y * narrow[k].pitch,
                           (const uint16_t *)check[k].image + y * check[k].pitch, check[k].width * 2)) { valid = 0; break; }
            }
        }
    }
    for (uint32_t k = 0; k < ATLAS_MIP_LEVELS; ++k) { free(check[k].image); free(narrow[k].image); if (k) free(level[k].image); }
    if (valid) {
        tiled->viewNumMips = ATLAS_MIP_LEVELS;
        GX2InitTextureRegs(tiled);
        *flat = *tiled; flat->viewNumMips = 1;
        GX2InitTextureRegs(flat);
    } else {
        free(tiled->surface.image); free(tiled->surface.mipmaps);
        tiled->surface.image = NULL; tiled->surface.mipmaps = NULL;
    }
    OSReport("C2 atlas %ux%u tiled %u, R5G5B5A1, %u mip levels, roundtrip %s\n", linear->surface.width,
        linear->surface.height, tiled->surface.tileMode, ATLAS_MIP_LEVELS, valid ? "PASS" : "FAIL (linear fallback)");
    return valid;
}
