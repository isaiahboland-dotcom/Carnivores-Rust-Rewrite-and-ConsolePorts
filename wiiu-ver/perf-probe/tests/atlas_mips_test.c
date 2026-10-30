#include <assert.h>
#include <string.h>
#include "../atlas_mips.h"
int main(void) {
    uint8_t src[4 * 4 * 4], dst[2 * 2 * 4];
    memset(src, 0, sizeof(src));
    // Top-left 2x2: two opaque green texels, two transparent black ones.
    const uint8_t green[4] = {0, 200, 0, 255};
    memcpy(src + (0 * 4 + 0) * 4, green, 4);
    memcpy(src + (1 * 4 + 1) * 4, green, 4);
    // Top-right 2x2: one opaque red texel only.
    const uint8_t red[4] = {250, 0, 0, 255};
    memcpy(src + (0 * 4 + 2) * 4, red, 4);
    // Bottom-left 2x2: opaque greys 0, 100, 100, 200.
    const uint8_t greys[4] = {0, 100, 100, 200};
    for (int i = 0; i < 4; ++i) {
        uint8_t *t = src + ((2 + i / 2) * 4 + i % 2) * 4;
        t[0] = t[1] = t[2] = greys[i]; t[3] = 255;
    }
    // Bottom-right 2x2 fully transparent but coloured: plain average.
    for (int i = 0; i < 4; ++i) { uint8_t *t = src + ((2 + i / 2) * 4 + 2 + i % 2) * 4; t[2] = 40; }
    atlas_mip_halve(src, 4, dst, 2, 2, 2);
    // No black fringe: the colour is the opaque texels' colour.
    assert(dst[0] == 0 && dst[1] == 200 && dst[2] == 0 && dst[3] == 128); // kept by the 0.5 cut
    assert(dst[4] == 250 && dst[5] == 0 && dst[7] == 64);                // dropped by the cut
    assert(dst[8] == 100 && dst[9] == 100 && dst[11] == 255);
    assert(dst[12] == 0 && dst[14] == 40 && dst[15] == 0);
    // A different destination pitch leaves the padding alone.
    uint8_t padded[3 * 2 * 4];
    memset(padded, 0xAB, sizeof(padded));
    atlas_mip_halve(src, 4, padded, 3, 2, 2);
    assert(memcmp(padded, dst, 8) == 0 && padded[8] == 0xAB && padded[20] == 0xAB);
    // 16 bits: every RGB555 value widened as c2_core::color::rgb555_to_rgba8
    // does comes back exactly, in the R5G5B5A1 bit order.
    for (uint32_t c = 0; c < 32; ++c) {
        const uint8_t wide = (uint8_t)(c << 3 | c >> 2);
        const uint8_t texel[4] = {wide, 0, 0, 255}, texel_g[4] = {0, wide, 0, 0}, texel_b[4] = {0, 0, wide, 127};
        assert(atlas_rgb5a1(texel) == (c | 0x8000u));
        assert(atlas_rgb5a1(texel_g) == c << 5);
        assert(atlas_rgb5a1(texel_b) == c << 10);
    }
    // The mip level above: alpha 128 stays opaque, 64 does not.
    uint16_t level[3 * 2];
    memset(level, 0xAB, sizeof(level));
    atlas_to_rgb5a1(dst, 2, level, 3, 2, 2);
    assert(level[0] == (24u << 5 | 0x8000u) && level[1] == 30u && level[3] == (12u | 12u << 5 | 12u << 10 | 0x8000u));
    assert(level[2] == 0xABAB && level[5] == 0xABAB);
    return 0;
}
