#include <coreinit/debug.h>
#include <gx2/draw.h>
#include <gx2/event.h>
#include <gx2/registers.h>
#include <gx2/shaders.h>
#include <gx2/texture.h>
#include <gx2/sampler.h>
#include <gx2/utils.h>
#include <gx2/mem.h>
#include <gx2r/buffer.h>
#include <gx2r/draw.h>
#include <whb/gfx.h>
#include <coreinit/filesystem.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <malloc.h>
#include <padscore/kpad.h>
#include <padscore/wpad.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vpad/input.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>
#include <whb/sdcard.h>

extern const unsigned char c2_sample_shader[];
extern int32_t c2_sky_texture(void *, uint8_t *, uint32_t);
extern void c2_sky_vertices(void *, float *, float *);
extern int32_t c2_map_texture(void *, uint8_t *, uint32_t);
extern void c2_map_player(void *, float *, float *, float *, float *);
extern uint32_t c2_water_vertices(void *, float *, float *);
extern int32_t c2_scenery_atlas(void *, uint8_t *, uint32_t);
extern uint32_t c2_scenery_vertices(void *, float *, float *, uint32_t);
extern int32_t c2_terrain_atlas(void *, uint8_t *, uint32_t);
extern uint32_t c2_terrain_vertices(void *, float *, float *);
typedef struct { int32_t x, y, z, yaw_milli, speed_milli; uint32_t sprint, crouch; } Snapshot;
extern int32_t c2_move_create(const uint8_t *, size_t, const uint8_t *, size_t, void **);
extern uint32_t c2_move_checks(void *);
extern void c2_move_tick(void *, float, float, float, float, uint32_t, Snapshot *);
extern void c2_move_destroy(void *);
void *c2_move_alloc(size_t n, size_t a) { if (a < sizeof(void *)) a = sizeof(void *); return memalign(a, n); }
void c2_move_free(void *p) { free(p); }
void c2_move_panic(void) { OSFatal("Carnivores Wii U: movement probe Rust panic"); for (;;) {} }
static FSClient client;
static int32_t load_file(const char *name, uint8_t **data, uint32_t *len) {
    char path[256]; FSCmdBlock cmd; FSFileHandle file; FSStat stat;
    if (snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/%s", WHBGetSdCardMountPath(), name) >= (int)sizeof(path)) return -1002;
    FSInitCmdBlock(&cmd);
    FSStatus s = FSOpenFile(&client, &cmd, path, "r", &file, FS_ERROR_FLAG_ALL);
    if (s != FS_STATUS_OK) return s;
    s = FSGetStatFile(&client, &cmd, file, &stat, FS_ERROR_FLAG_ALL);
    uint8_t *p = NULL;
    if (s == FS_STATUS_OK && (stat.size == 0 || stat.size > 64u * 1024u * 1024u)) s = -1002;
    if (s == FS_STATUS_OK) { p = memalign(64, (stat.size + 63u) & ~63u); if (!p) s = -1003; }
    uint32_t pos = 0;
    while (s == FS_STATUS_OK && pos < stat.size) {
        uint32_t n = stat.size - pos; if (n > 4096) n = 4096;
        FSStatus got = FSReadFile(&client, &cmd, p + pos, 1, n, file, 0, FS_ERROR_FLAG_ALL);
        if (got < 0) { s = got; break; }
        if ((uint32_t)got != n) { s = -1001; break; }
        pos += n;
    }
    FSStatus closed = FSCloseFile(&client, &cmd, file, FS_ERROR_FLAG_ALL);
    if (s == FS_STATUS_OK && closed != FS_STATUS_OK) s = closed;
    if (s != FS_STATUS_OK) { free(p); return s; }
    *data = p; *len = stat.size; return 0;
}
int main(void) {
    int mounted = 0, added = 0; void *sim = NULL;
    uint8_t *map = NULL, *rsc = NULL; uint32_t map_len = 0, rsc_len = 0;
    WHBProcInit(); WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U movement simulation");
    WHBLogPrint("Loading base game AREA1 from SD..."); WHBLogConsoleDraw();
    if (!WHBMountSdCard()) { WHBLogPrint("SD mount: FAIL"); goto end_load; }
    mounted = 1;
    FSStatus s = FSAddClient(&client, FS_ERROR_FLAG_ALL);
    if (s != FS_STATUS_OK) { WHBLogPrintf("FS client: FAIL %d", s); goto end_load; }
    added = 1;
    int32_t status = load_file("AREA1.MAP", &map, &map_len);
    if (status != 0) { WHBLogPrintf("AREA1.MAP read: FAIL %d", status); goto end_load; }
    WHBLogPrint("AREA1.MAP loaded"); WHBLogConsoleDraw();
    status = load_file("AREA1.RSC", &rsc, &rsc_len);
    if (status != 0) { WHBLogPrintf("AREA1.RSC read: FAIL %d", status); goto end_load; }
    WHBLogPrint("AREA1.RSC loaded; preparing area..."); WHBLogConsoleDraw();
    status = c2_move_create(map, map_len, rsc, rsc_len, &sim);
    if (status != 0) { WHBLogPrintf("Map parse: FAIL %d", status); goto end_load; }
    WHBLogPrint("AREA1 loaded; checking movement..."); WHBLogConsoleDraw();
    uint32_t checks = c2_move_checks(sim);
    WHBLogPrint(checks & 1 ? "Landing checkpoint: PASS" : "Landing checkpoint: FAIL");
    WHBLogPrint(checks & 2 ? "Walk checkpoint: PASS" : "Walk checkpoint: FAIL");
    WHBLogPrint(checks & 4 ? "Sprint checkpoint: PASS" : "Sprint checkpoint: FAIL");
    WHBLogPrint("AREA1 simulation: READY");
end_load:
    free(map); free(rsc);
    if (added && FSDelClient(&client, FS_ERROR_FLAG_ALL) != FS_STATUS_OK) WHBLogPrint("FS client removal: FAIL");
    if (mounted && !WHBUnmountSdCard()) WHBLogPrint("SD unmount: FAIL");
    if (sim) {
        WHBLogPrint("Text only: L stick move, R stick look");
        WHBLogPrint("A jump; B sprint; D-pad up crouch");
        WHBLogPrint("Coordinates update every second.");
    }
    WHBLogPrint("Use HOME to exit.");
    WHBLogConsoleDraw();
    if (!sim) { while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); } WHBLogConsoleFree(); WHBProcShutdown(); return 1; }
    WHBLogConsoleFree();
    WHBGfxShaderGroup shader = {0}; GX2RBuffer positions = {0}, colors = {0};
    if (!WHBGfxInit() || !WHBGfxLoadGFDShaderGroup(&shader, 0, c2_sample_shader)
        || !WHBGfxInitShaderAttribute(&shader, "aPosition", 0, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitShaderAttribute(&shader, "aTexLight", 1, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitFetchShader(&shader)) OSFatal("Terrain probe: graphics setup failed");
    GX2Texture atlas = {0}; GX2Sampler sampler = {0};
    atlas.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    atlas.surface.width = 1024; atlas.surface.height = 1024; atlas.surface.depth = 1;
    atlas.surface.mipLevels = 1; atlas.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    atlas.surface.aa = GX2_AA_MODE1X; atlas.surface.use = GX2_SURFACE_USE_TEXTURE;
    atlas.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&atlas.surface);
    atlas.surface.image = memalign(atlas.surface.alignment, atlas.surface.imageSize);
    if (!atlas.surface.image) OSFatal("Terrain texture probe: atlas allocation failed");
    memset(atlas.surface.image, 0, atlas.surface.imageSize);
    if (c2_terrain_atlas(sim, atlas.surface.image, atlas.surface.pitch) != 0) OSFatal("Terrain texture probe: atlas data invalid or too many textures");
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, atlas.surface.image, atlas.surface.imageSize);
    atlas.viewNumMips = 1; atlas.viewNumSlices = 1;
    atlas.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&atlas);
    GX2InitSampler(&sampler, GX2_TEX_CLAMP_MODE_CLAMP, GX2_TEX_XY_FILTER_MODE_LINEAR);
    if (shader.pixelShader->samplerVarCount != 1) OSFatal("Terrain texture probe: shader sampler missing");
    uint32_t sampler_location = shader.pixelShader->samplerVars[0].location;
    GX2Texture object_atlas = {0};
    object_atlas.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    object_atlas.surface.width = 2048; object_atlas.surface.height = 2048; object_atlas.surface.depth = 1;
    object_atlas.surface.mipLevels = 1; object_atlas.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    object_atlas.surface.aa = GX2_AA_MODE1X; object_atlas.surface.use = GX2_SURFACE_USE_TEXTURE;
    object_atlas.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&object_atlas.surface);
    object_atlas.surface.image = memalign(object_atlas.surface.alignment, object_atlas.surface.imageSize);
    if (!object_atlas.surface.image) OSFatal("Scenery probe: object atlas allocation failed");
    memset(object_atlas.surface.image, 0, object_atlas.surface.imageSize);
    if (c2_scenery_atlas(sim, object_atlas.surface.image, object_atlas.surface.pitch) != 0) OSFatal("Scenery probe: invalid or too many object textures");
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, object_atlas.surface.image, object_atlas.surface.imageSize);
    object_atlas.viewNumMips = 1; object_atlas.viewNumSlices = 1;
    object_atlas.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&object_atlas);
    GX2RBuffer object_positions = {0}, object_attrs = {0};
    object_positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    object_positions.elemSize = 4 * sizeof(float); object_positions.elemCount = 65535;
    object_attrs.flags = object_positions.flags; object_attrs.elemSize = object_positions.elemSize; object_attrs.elemCount = object_positions.elemCount;
    if (!GX2RCreateBuffer(&object_positions) || !GX2RCreateBuffer(&object_attrs)) OSFatal("Scenery probe: object buffer allocation failed");
    positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    positions.elemSize = 4 * sizeof(float); positions.elemCount = 24 * 24 * 6;
    colors.flags = positions.flags; colors.elemSize = positions.elemSize; colors.elemCount = positions.elemCount;
    if (!GX2RCreateBuffer(&positions) || !GX2RCreateBuffer(&colors)) OSFatal("Terrain probe: buffer creation failed");
    GX2Texture map_tex = {0};
    map_tex.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    map_tex.surface.width = 1024; map_tex.surface.height = 1024; map_tex.surface.depth = 1;
    map_tex.surface.mipLevels = 1; map_tex.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    map_tex.surface.aa = GX2_AA_MODE1X; map_tex.surface.use = GX2_SURFACE_USE_TEXTURE;
    map_tex.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&map_tex.surface);
    map_tex.surface.image = memalign(map_tex.surface.alignment, map_tex.surface.imageSize);
    if (!map_tex.surface.image) OSFatal("Map probe: texture allocation failed");
    memset(map_tex.surface.image, 0, map_tex.surface.imageSize);
    if (c2_map_texture(sim, map_tex.surface.image, map_tex.surface.pitch)) OSFatal("Map probe: texture generation failed");
    map_tex.viewNumMips = 1; map_tex.viewNumSlices = 1;
    map_tex.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&map_tex);
    GX2Sampler map_sampler = {0};
    GX2InitSampler(&map_sampler, GX2_TEX_CLAMP_MODE_CLAMP, GX2_TEX_XY_FILTER_MODE_POINT);
    GX2RBuffer map_pos = {0}, map_attr = {0};
    map_pos.flags = positions.flags; map_pos.elemSize = 4 * sizeof(float); map_pos.elemCount = 6;
    map_attr.flags = map_pos.flags; map_attr.elemSize = map_pos.elemSize; map_attr.elemCount = 6;
    if (!GX2RCreateBuffer(&map_pos) || !GX2RCreateBuffer(&map_attr)) OSFatal("Map probe: buffer allocation failed");
    GX2RBuffer water_pos = {0}, water_attr = {0};
    water_pos.flags = positions.flags; water_pos.elemSize = positions.elemSize; water_pos.elemCount = positions.elemCount;
    water_attr.flags = positions.flags; water_attr.elemSize = positions.elemSize; water_attr.elemCount = positions.elemCount;
    if (!GX2RCreateBuffer(&water_pos) || !GX2RCreateBuffer(&water_attr)) OSFatal("Water probe: buffer allocation failed");
    GX2Texture sky_tex = {0};
    sky_tex.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    sky_tex.surface.width = 256; sky_tex.surface.height = 256; sky_tex.surface.depth = 1;
    sky_tex.surface.mipLevels = 1; sky_tex.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    sky_tex.surface.aa = GX2_AA_MODE1X; sky_tex.surface.use = GX2_SURFACE_USE_TEXTURE;
    sky_tex.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&sky_tex.surface);
    sky_tex.surface.image = memalign(sky_tex.surface.alignment, sky_tex.surface.imageSize);
    if (!sky_tex.surface.image) OSFatal("Sky probe: texture allocation failed");
    memset(sky_tex.surface.image, 0, sky_tex.surface.imageSize);
    if (c2_sky_texture(sim, sky_tex.surface.image, sky_tex.surface.pitch)) OSFatal("Sky probe: invalid sky");
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, sky_tex.surface.image, sky_tex.surface.imageSize);
    sky_tex.viewNumMips = 1; sky_tex.viewNumSlices = 1;
    sky_tex.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&sky_tex);
    GX2Sampler sky_sampler = {0};
    GX2InitSampler(&sky_sampler, GX2_TEX_CLAMP_MODE_WRAP, GX2_TEX_XY_FILTER_MODE_LINEAR);
    GX2RBuffer sky_pos = {0}, sky_attr = {0};
    sky_pos.flags = positions.flags; sky_pos.elemSize = positions.elemSize; sky_pos.elemCount = 6;
    sky_attr.flags = positions.flags; sky_attr.elemSize = positions.elemSize; sky_attr.elemCount = 6;
    if (!GX2RCreateBuffer(&sky_pos) || !GX2RCreateBuffer(&sky_attr)) OSFatal("Sky probe: buffer allocation failed");
    uint8_t saved[27 * 27 * 4]; int old_x = -1, old_z = -1;
    int full_map = 0, gamepad_game = 0;
    Snapshot snap = {0};
    const uint64_t step_ticks = OSMillisecondsToTicks(16);
    uint64_t accumulated = 0; OSTime previous_time = OSGetTime();
    uint32_t pending_toggles = 0;
    int reported_gamepad = 0, reported_pro = 0;
    VPADStatus gamepad = {0}; KPADStatus pro[4] = {0}; int pro_valid[4] = {0};
    if (sim) { KPADInit(); WPADEnableURCC(TRUE); }
    while (WHBProcIsRunning()) {
        if (sim) {
            VPADStatus sample = {0}; VPADReadError error = VPAD_READ_NO_SAMPLES;
            uint32_t flags = 0;
            int32_t n = VPADRead(VPAD_CHAN_0, &sample, 1, &error);
            if (n > 0) {
                gamepad = sample;
                if (!reported_gamepad) { reported_gamepad = 1; }
                if (sample.trigger & VPAD_BUTTON_B) flags |= 2;
                if (sample.trigger & VPAD_BUTTON_UP) flags |= 4;
                if (sample.trigger & VPAD_BUTTON_R) full_map = !full_map;
                if (sample.trigger & VPAD_BUTTON_MINUS) gamepad_game = !gamepad_game;
            } else if (error != VPAD_READ_NO_SAMPLES) { memset(&gamepad, 0, sizeof(gamepad)); }
            float lx = gamepad.leftStick.x, ly = gamepad.leftStick.y;
            float rx = gamepad.rightStick.x, ry = gamepad.rightStick.y;
            if (gamepad.hold & VPAD_BUTTON_A) flags |= 1;
            for (int channel = 0; channel < 4; ++channel) {
                KPADStatus sample_pro = {0}; KPADError kerr = KPAD_ERROR_NO_SAMPLES;
                uint32_t count = KPADReadEx((KPADChan)channel, &sample_pro, 1, &kerr);
                if (count > 0) {
                    pro_valid[channel] = sample_pro.extensionType == WPAD_EXT_PRO_CONTROLLER;
                    if (pro_valid[channel]) {
                        pro[channel] = sample_pro;
                        if (!reported_pro) { reported_pro = 1; }
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_B) flags |= 2;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_UP) flags |= 4;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_R) full_map = !full_map;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_MINUS) gamepad_game = !gamepad_game;
                    }
                } else if (kerr != KPAD_ERROR_NO_SAMPLES) { pro_valid[channel] = 0; }
                if (!pro_valid[channel]) continue;
                if (pro[channel].pro.hold & WPAD_PRO_BUTTON_A) flags |= 1;
                if (pro[channel].pro.leftStick.x > 0.25f || pro[channel].pro.leftStick.x < -0.25f) lx = pro[channel].pro.leftStick.x;
                if (pro[channel].pro.leftStick.y > 0.25f || pro[channel].pro.leftStick.y < -0.25f) ly = pro[channel].pro.leftStick.y;
                if (pro[channel].pro.rightStick.x > 0.25f || pro[channel].pro.rightStick.x < -0.25f) rx = pro[channel].pro.rightStick.x;
                if (pro[channel].pro.rightStick.y > 0.25f || pro[channel].pro.rightStick.y < -0.25f) ry = pro[channel].pro.rightStick.y;
            }
            // Retain edge-triggered toggles across frames with no simulation step.
            pending_toggles ^= flags & 6;
            OSTime now = OSGetTime();
            uint64_t elapsed = now > previous_time ? (uint64_t)(now - previous_time) : 0;
            previous_time = now;
            // Drop excess time after a long pause instead of an unbounded catch-up.
            const uint64_t max_elapsed = OSMillisecondsToTicks(128);
            if (elapsed > max_elapsed) elapsed = max_elapsed;
            accumulated += elapsed;
            if (accumulated > max_elapsed) accumulated = max_elapsed;
            while (accumulated >= step_ticks) {
                c2_move_tick(sim, lx, ly, rx, ry, (flags & 1) | pending_toggles, &snap);
                pending_toggles = 0;
                accumulated -= step_ticks;
            }
            GX2DrawDone();
            float *p = GX2RLockBufferEx(&positions, 0), *c = GX2RLockBufferEx(&colors, 0);
            if (!p || !c) OSFatal("Terrain probe: buffer lock failed");
            uint32_t count = c2_terrain_vertices(sim, p, c);
            GX2RUnlockBufferEx(&positions, 0); GX2RUnlockBufferEx(&colors, 0);
            p = GX2RLockBufferEx(&object_positions, 0); c = GX2RLockBufferEx(&object_attrs, 0);
            if (!p || !c) OSFatal("Scenery probe: object buffer lock failed");
            uint32_t object_count = c2_scenery_vertices(sim, p, c, object_positions.elemCount);
            GX2RUnlockBufferEx(&object_positions, 0); GX2RUnlockBufferEx(&object_attrs, 0);
            p = GX2RLockBufferEx(&water_pos, 0); c = GX2RLockBufferEx(&water_attr, 0);
            if (!p || !c) OSFatal("Water probe: buffer lock failed");
            uint32_t water_count = c2_water_vertices(sim, p, c);
            GX2RUnlockBufferEx(&water_pos, 0); GX2RUnlockBufferEx(&water_attr, 0);
            p = GX2RLockBufferEx(&sky_pos, 0); c = GX2RLockBufferEx(&sky_attr, 0);
            if (!p || !c) OSFatal("Sky probe: buffer lock failed");
            c2_sky_vertices(sim, p, c);
            GX2RUnlockBufferEx(&sky_pos, 0); GX2RUnlockBufferEx(&sky_attr, 0);
            // Restore the underlying map before painting the next mobile-style overlay.
            uint8_t *map_pixels = map_tex.surface.image;
            if (old_x >= 0) for (int dz = 0; dz < 27; ++dz) for (int dx = 0; dx < 27; ++dx)
                memcpy(map_pixels + ((old_z + dz) * map_tex.surface.pitch + old_x + dx) * 4, saved + (dz * 27 + dx) * 4, 4);
            float px, pz, dir_x, dir_z; c2_map_player(sim, &px, &pz, &dir_x, &dir_z);
            int mx = (int)px - 13, mz = (int)pz - 13;
            if (mx < 0) mx = 0; if (mx > 997) mx = 997;
            if (mz < 0) mz = 0; if (mz > 997) mz = 997;
            for (int dz = 0; dz < 27; ++dz) for (int dx = 0; dx < 27; ++dx) {
                uint8_t *pixel = map_pixels + ((mz + dz) * map_tex.surface.pitch + mx + dx) * 4;
                memcpy(saved + (dz * 27 + dx) * 4, pixel, 4);
                float ax = (mx + dx) - px, az = (mz + dz) - pz;
                float forward = ax * dir_x + az * dir_z;
                float side = ax * -dir_z + az * dir_x;
                float radius2 = ax * ax + az * az;
                // The diagnostic terrain window is 12 cells in each direction.
                // Tint a 90-degree forward sector, as on the 3DS mobile map.
                if (radius2 <= 144.0f && forward >= 0 && side >= -forward && side <= forward) {
                    pixel[0] = (uint8_t)((pixel[0] * 55 + 255 * 45) / 100);
                    pixel[1] = (uint8_t)((pixel[1] * 55 + 217 * 45) / 100);
                    pixel[2] = (uint8_t)((pixel[2] * 55 + 26 * 45) / 100);
                }
                if (radius2 >= 132.25f && radius2 <= 156.25f) {
                    pixel[0] = 255; pixel[1] = 217; pixel[2] = 26;
                }
            }
            old_x = mx; old_z = mz;
            GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, map_tex.surface.image, map_tex.surface.imageSize);
            WHBGfxBeginRender();
            for (int display = 0; display < 2; ++display) {
                if (display == 0) WHBGfxBeginRenderTV(); else WHBGfxBeginRenderDRC();
                int show_map = display == 0 ? full_map : (!gamepad_game || full_map);
                if (show_map) WHBGfxClearColor(0.06f, 0.16f, 0.09f, 1.0f);
                else WHBGfxClearColor(0.24f, 0.44f, 0.65f, 1.0f);
                if (show_map) {
                    int zoom = display == 1 && !gamepad_game;
                    float span = zoom ? 128.0f : 1024.0f;
                    float left = zoom ? px - span / 2.0f : 0.0f;
                    float top = zoom ? pz - span / 2.0f : 0.0f;
                    if (left < 0) left = 0; if (left > 1024 - span) left = 1024 - span;
                    if (top < 0) top = 0; if (top > 1024 - span) top = 1024 - span;
                    float u0 = left / 1024.0f, v0 = top / 1024.0f;
                    float u1 = (left + span) / 1024.0f, v1 = (top + span) / 1024.0f;
                    // Square map in a 16:9 display, surrounded by the clear color.
                    float verts[24] = {-0.5625f,1,0,1, 0.5625f,1,0,1, 0.5625f,-1,0,1,
                                       -0.5625f,1,0,1, 0.5625f,-1,0,1, -0.5625f,-1,0,1};
                    float attrs[24] = {u0,v0,1,0, u1,v0,1,0, u1,v1,1,0,
                                       u0,v0,1,0, u1,v1,1,0, u0,v1,1,0};
                    float *mp = GX2RLockBufferEx(&map_pos, 0), *ma = GX2RLockBufferEx(&map_attr, 0);
                    if (!mp || !ma) OSFatal("Map probe: buffer lock failed");
                    memcpy(mp, verts, sizeof(verts)); memcpy(ma, attrs, sizeof(attrs));
                    GX2RUnlockBufferEx(&map_pos, 0); GX2RUnlockBufferEx(&map_attr, 0);
                    GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                    GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                    GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                    GX2SetPixelTexture(&map_tex, sampler_location); GX2SetPixelSampler(&map_sampler, sampler_location);
                    GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                    GX2RSetAttributeBuffer(&map_pos, 0, map_pos.elemSize, 0); GX2RSetAttributeBuffer(&map_attr, 1, map_attr.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                    if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
                    // Both displays may need different map UVs; wait before rewriting the buffer.
                    GX2DrawDone();
                    continue;
                }
                GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                GX2SetPixelTexture(&sky_tex, sampler_location); GX2SetPixelSampler(&sky_sampler, sampler_location);
                GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                GX2RSetAttributeBuffer(&sky_pos, 0, sky_pos.elemSize, 0); GX2RSetAttributeBuffer(&sky_attr, 1, sky_attr.elemSize, 0);
                GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                GX2SetDepthOnlyControl(TRUE, TRUE, GX2_COMPARE_FUNC_LESS);
                GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                GX2SetPixelTexture(&atlas, sampler_location); GX2SetPixelSampler(&sampler, sampler_location);
                GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                GX2RSetAttributeBuffer(&positions, 0, positions.elemSize, 0); GX2RSetAttributeBuffer(&colors, 1, colors.elemSize, 0);
                GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, count, 0, 1);
                if (object_count) {
                    GX2SetPixelTexture(&object_atlas, sampler_location);
                    GX2RSetAttributeBuffer(&object_positions, 0, object_positions.elemSize, 0);
                    GX2RSetAttributeBuffer(&object_attrs, 1, object_attrs.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, object_count, 0, 1);
                }
                if (water_count) {
                    GX2SetPixelTexture(&atlas, sampler_location);
                    GX2RSetAttributeBuffer(&water_pos, 0, water_pos.elemSize, 0);
                    GX2RSetAttributeBuffer(&water_attr, 1, water_attr.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, water_count, 0, 1);
                }
                if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
            }
            WHBGfxFinishRender();
        }
    }
    GX2DrawDone(); GX2RDestroyBufferEx(&positions, 0); GX2RDestroyBufferEx(&colors, 0);
    GX2RDestroyBufferEx(&object_positions, 0); GX2RDestroyBufferEx(&object_attrs, 0);
    GX2RDestroyBufferEx(&sky_pos, 0); GX2RDestroyBufferEx(&sky_attr, 0); free(sky_tex.surface.image);
    GX2RDestroyBufferEx(&water_pos, 0); GX2RDestroyBufferEx(&water_attr, 0);
    GX2RDestroyBufferEx(&map_pos, 0); GX2RDestroyBufferEx(&map_attr, 0); free(map_tex.surface.image);
    free(object_atlas.surface.image); free(atlas.surface.image); WHBGfxFreeShaderGroup(&shader); WHBGfxShutdown();
    if (sim) KPADShutdown();
    c2_move_destroy(sim); WHBProcShutdown(); return 0;
}
