#pragma once
#include <stdint.h>
/* Mip levels for the world atlases. Each atlas packs power-of-two tiles on
 * aligned cells (terrain 128, scenery and actors 256), so a 2x2 box never
 * mixes two textures above the tile's own 1x1 level; ATLAS_MIP_LEVELS stops
 * well before that (a 128 tile is 8x8 at the last level). */
#define ATLAS_MIP_LEVELS 5

/* Halve an RGBA8 image (pitches in texels). Colour is averaged over the
 * opaque texels only, so cut-out leaves don't pick up the transparent
 * texels' black; alpha is the plain average, so the shader's 0.5 cut keeps
 * a texel when at least half of its four sources were opaque. */
static void atlas_mip_halve(const uint8_t *src, uint32_t src_pitch, uint8_t *dst, uint32_t dst_pitch,
                            uint32_t dst_width, uint32_t dst_height) {
    for (uint32_t y = 0; y < dst_height; ++y) {
        for (uint32_t x = 0; x < dst_width; ++x) {
            const uint8_t *p[4] = {
                src + ((2 * y) * src_pitch + 2 * x) * 4, src + ((2 * y) * src_pitch + 2 * x + 1) * 4,
                src + ((2 * y + 1) * src_pitch + 2 * x) * 4, src + ((2 * y + 1) * src_pitch + 2 * x + 1) * 4,
            };
            uint32_t weight = 0, alpha = 0, rgb[3] = {0, 0, 0}, flat[3] = {0, 0, 0};
            for (int i = 0; i < 4; ++i) {
                alpha += p[i][3];
                weight += p[i][3];
                for (int c = 0; c < 3; ++c) { rgb[c] += p[i][c] * p[i][3]; flat[c] += p[i][c]; }
            }
            uint8_t *out = dst + (y * dst_pitch + x) * 4;
            for (int c = 0; c < 3; ++c)
                out[c] = (uint8_t)(weight ? (rgb[c] + weight / 2) / weight : (flat[c] + 2) / 4);
            out[3] = (uint8_t)((alpha + 2) / 4);
        }
    }
}

/* One RGBA8 texel as the GPU's R5G5B5A1 (GX2_SURFACE_FORMAT_UNORM_R5_G5_B5_A1:
 * red in the low five bits, alpha the top bit), a native 16-bit word. The
 * atlases are the original's RGB555 texels widened to 8 bits, so they come
 * back exactly; alpha keeps the shader's 0.5 cut (128 and up is opaque). */
static uint16_t atlas_rgb5a1(const uint8_t *p) {
    uint32_t r = (p[0] * 31u + 127u) / 255u, g = (p[1] * 31u + 127u) / 255u, b = (p[2] * 31u + 127u) / 255u;
    return (uint16_t)(r | g << 5 | b << 10 | (p[3] >= 128 ? 0x8000u : 0u));
}
/* An RGBA8 image to R5G5B5A1 (pitches in texels). */
static void atlas_to_rgb5a1(const uint8_t *src, uint32_t src_pitch, uint16_t *dst, uint32_t dst_pitch,
                            uint32_t width, uint32_t height) {
    for (uint32_t y = 0; y < height; ++y)
        for (uint32_t x = 0; x < width; ++x) dst[y * dst_pitch + x] = atlas_rgb5a1(src + (y * src_pitch + x) * 4);
}
