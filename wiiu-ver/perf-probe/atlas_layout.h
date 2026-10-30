/* Keep the linear reference for a same-session A/B comparison. Conversion and
 * validation happen only during loading, never in the measured gameplay loop. */
static int atlas_tiled_copy(const GX2Texture *linear, GX2Texture *tiled) {
    *tiled = *linear;
    tiled->surface.image = NULL;
    tiled->surface.tileMode = GX2_TILE_MODE_TILED_2D_THIN1;
    tiled->surface.swizzle = 0;
    GX2CalcSurfaceSizeAndAlignment(&tiled->surface);
    tiled->surface.image = memalign(tiled->surface.alignment, tiled->surface.imageSize);
    if (!tiled->surface.image) return 0;
    // A fresh allocation can contain dirty CPU cache lines from its prior owner.
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, tiled->surface.image, tiled->surface.imageSize);
    GX2CopySurface(&linear->surface, 0, 0, &tiled->surface, 0, 0);
    GX2DrawDone();
    GX2Invalidate(GX2_INVALIDATE_MODE_TEXTURE, tiled->surface.image, tiled->surface.imageSize);
    GX2InitTextureRegs(tiled);

    // Read back to a temporary linear surface and verify every RGBA texel.
    // Allocation/copy failures keep the original linear path usable.
    GX2Surface check = linear->surface;
    check.image = memalign(check.alignment, check.imageSize);
    if (!check.image) { free(tiled->surface.image); tiled->surface.image = NULL; return 0; }
    DCFlushRange(check.image, check.imageSize);
    GX2CopySurface(&tiled->surface, 0, 0, &check, 0, 0);
    GX2DrawDone();
    DCInvalidateRange(check.image, check.imageSize);
    int valid = 1;
    for (uint32_t y = 0; y < check.height; ++y) {
        if (memcmp((const uint8_t *)linear->surface.image + y * linear->surface.pitch * 4,
                   (const uint8_t *)check.image + y * check.pitch * 4, check.width * 4)) { valid = 0; break; }
    }
    free(check.image);
    if (!valid) { free(tiled->surface.image); tiled->surface.image = NULL; }
    OSReport("C2 atlas %ux%u tiled %u roundtrip %s\n", linear->surface.width,
        linear->surface.height, tiled->surface.tileMode, valid ? "PASS" : "FAIL (linear fallback)");
    return valid;
}
