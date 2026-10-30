/* Bounded, buffered per-frame CPU samples. Included after Perf/HuntChoice/client.
 * SD work happens after frame timing; its cost is reported in the next row. */
#define PERFORMANCE_LIMIT (16u * 1024u * 1024u)
static struct {
    FSCmdBlock cmd;
    FSFileHandle file;
    int open;
    uint32_t used, written, frame;
    uint64_t origin, last_flush, overhead;
    char data[65536] __attribute__((aligned(64)));
} performance;
static void performance_flush(void) {
    if (!performance.open || !performance.used) return;
    FSStatus n = FSWriteFile(&client, &performance.cmd, performance.data, 1,
        performance.used, performance.file, 0, FS_ERROR_FLAG_NONE);
    if (n != (FSStatus)performance.used ||
        FSFlushFile(&client, &performance.cmd, performance.file, FS_ERROR_FLAG_NONE) != FS_STATUS_OK) {
        OSReport("C2 performance.csv write failed; capture disabled\n");
        FSCloseFile(&client, &performance.cmd, performance.file, FS_ERROR_FLAG_NONE);
        performance.open = 0;
    }
    performance.written += performance.used;
    performance.used = 0;
    performance.last_flush = OSGetTime();
}
static void performance_close(void) {
    performance_flush();
    if (performance.open) FSCloseFile(&client, &performance.cmd, performance.file, FS_ERROR_FLAG_NONE);
    performance.open = 0;
}
static void performance_open(const HuntChoice *menu, uint32_t seed, int tiled_available, int tv_target) {
    static uint32_t session;
    memset(&performance, 0, sizeof(performance));
    FSInitCmdBlock(&performance.cmd);
    char path[256];
    /* Keep the previous capture across restarts/return-to-menu and app launches.
     * Timebase plus session counter avoids reliance on the console calendar. */
    performance.origin = performance.last_flush = OSGetTime();
    snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/performance-%llx-%u.csv",
        WHBGetSdCardMountPath(), (unsigned long long)performance.origin, ++session);
    performance.open = FSOpenFile(&client, &performance.cmd, path, "w", &performance.file, FS_ERROR_FLAG_NONE) == FS_STATUS_OK;
    if (!performance.open) { OSReport("C2 performance capture could not open SD file\n"); return; }
    GX2ColorBuffer *tv = WHBGfxGetTVColourBuffer(), *drc = WHBGfxGetDRCColourBuffer();
    performance.used = snprintf(performance.data, sizeof(performance.data),
        "# schema=11 resident_scenery=1 static_view_cache=1 terrain_precompute=1 scenery_candidates=1 tiled_available=%d tv_target=%d map_streams=2 hidden_geometry_skip=1 range_upload=%d build=%s %s area=%d animals=%u weapons=%u radar=%u blood_off=%u kill_anim_off=%u time=%u camo=%u scent=%u double_ammo=%u tranq=%u observer=%u run_pct=%u ambient=%u population=%u mode=%u survival=%u seed=%u tv=%ux%u drc=%ux%u limit_bytes=%u\n"
        "frame,start_ms,seed,tv_map,drc_game,ticks,frame_ms,terrain_ms,scenery_ms,water_ms,sky_ms,map_paint_ms,map_cache_ms,sim_ms,animals_ms,gun_ms,draw_ms,geometry_wait_ms,map_wait_ms,present_ms,overlay_ms,unaccounted_ms,previous_profiler_ms,terrain_v,scenery_v,animal_v,gun_v,water_v,sky_v,geometry_stream_bytes,stream_lock_ms,stream_cache_ms,stream_cache_bytes,death_playback,tiled_atlases,static_cache_enabled,static_reused,resident_enabled,resident_active,scenery_ranges,scenery_uploaded_v,gpu_tv_ms,gpu_drc_ms,vsync,tv_world\n",
        tiled_available, tv_target, C2_RANGE_UPLOAD, __DATE__, __TIME__, menu->area + 1, menu->animals, menu->weapons, menu->radar,
        (unsigned)!menu->blood, (unsigned)!menu->death_anim, menu->time, menu->camo, menu->scent, menu->double_ammo,
        menu->tranq, menu->observer, menu->run_pct, menu->ambient, menu->ambient ? 66u : 48u, menu->mode, menu->survival, seed, tv->surface.width, tv->surface.height,
        drc->surface.width, drc->surface.height, PERFORMANCE_LIMIT);
    performance_flush();
}
static double performance_ms(uint64_t ticks) { return (double)OSTicksToMicroseconds(ticks) / 1000.0; }
static void performance_frame(const Perf *p, const Perf *b, uint64_t start, uint32_t seed,
    int tv_map, int drc_game, uint32_t ticks, uint64_t gun, uint64_t map_cache,
    uint64_t overlay, uint64_t geometry_wait, uint64_t map_wait, uint64_t gpu_tv, uint64_t gpu_drc, int vsync,
    uint32_t terrain_v, uint32_t scenery_v, uint32_t animal_v, uint32_t gun_v,
    uint32_t water_v, uint32_t sky_v, uint32_t death_playback, uint32_t tiled_atlases, uint32_t static_cache_enabled, uint32_t static_reused, uint32_t resident_enabled, uint32_t resident_active, uint32_t scenery_ranges, uint32_t scenery_uploaded, uint32_t tv_world) {
    if (!performance.open) return;
    uint64_t begin = OSGetTime();
    /* Each bucket is disjoint. Buffer locks/unlocks remain in geometry buckets.
     * Draw includes render setup; PRESENT and waits are CPU elapsed times.
     * Since schema 8 frames overlap the GPU: WAIT is the wait for the previous
     * frame's GPU work; gpu_tv/gpu_drc (GPU timestamps) proved unreliable.
     * Schema 9: vsync 0 while the D-pad Left diagnostic turns it off.
     * Schema 10: tiled_atlases 2 = tiled with mip levels, 1 = without (R3).
     * Schema 11: the atlases are R5G5B5A1; terrain and water draw with the
     * opaque shader; tv_world = the height the TV's world was drawn at (720
     * on the 720p target, the TV's own with L3 or without it; 0 when the TV
     * shows the map); 32-byte vertices (the fog is a uniform). */
    uint64_t accounted = (p->terrain-b->terrain) + (p->scenery-b->scenery) +
        (p->water-b->water) + (p->sky-b->sky) + (p->map-b->map) +
        (p->simulation-b->simulation) + (p->actors-b->actors) +
        (p->draw-b->draw) + (p->wait-b->wait) + (p->present-b->present) + overlay;
    uint64_t frame = p->frame-b->frame;
    char row[1024];
    int n = snprintf(row, sizeof(row),
        "%u,%.3f,%u,%d,%d,%u,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%u,%u,%u,%u,%u,%u,%u,%.3f,%.3f,%u,%u,%u,%u,%u,%u,%u,%u,%u,%.3f,%.3f,%d,%u\n",
        ++performance.frame, performance_ms(start-performance.origin), seed, tv_map, drc_game, ticks,
        performance_ms(frame), performance_ms(p->terrain-b->terrain), performance_ms(p->scenery-b->scenery),
        performance_ms(p->water-b->water), performance_ms(p->sky-b->sky),
        performance_ms(p->map-b->map-map_cache), performance_ms(map_cache),
        performance_ms(p->simulation-b->simulation), performance_ms(p->actors-b->actors-gun), performance_ms(gun),
        performance_ms(p->draw-b->draw), performance_ms(geometry_wait), performance_ms(map_wait),
        performance_ms(p->present-b->present), performance_ms(overlay), performance_ms(frame > accounted ? frame-accounted : 0),
        performance_ms(performance.overhead), terrain_v, scenery_v, animal_v, gun_v, water_v, sky_v,
        (animal_v+gun_v+scenery_uploaded+(static_reused ? 0 : terrain_v+water_v+sky_v))*32u,
        performance_ms(stream_upload.lock), performance_ms(stream_upload.cache), stream_upload.bytes, death_playback, tiled_atlases, static_cache_enabled, static_reused, resident_enabled, resident_active, scenery_ranges, scenery_uploaded,
        performance_ms(gpu_tv), performance_ms(gpu_drc), vsync, tv_world);
    if (n <= 0 || n >= (int)sizeof(row)) return;
    if (performance.written + performance.used + (uint32_t)n > PERFORMANCE_LIMIT) {
        performance_close();
        OSReport("C2 performance capture reached 16 MiB limit\n");
        return;
    }
    if (performance.used + (uint32_t)n > sizeof(performance.data)) performance_flush();
    if (!performance.open) return;
    memcpy(performance.data + performance.used, row, n);
    performance.used += n;
    if (begin-performance.last_flush >= (uint64_t)OSSecondsToTicks(5)) performance_flush();
    performance.overhead = OSGetTime()-begin;
}
