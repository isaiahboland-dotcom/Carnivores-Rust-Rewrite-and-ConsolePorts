#include <coreinit/debug.h>
#include <gx2/draw.h>
#include <gx2/event.h>
#include <gx2/registers.h>
#include <gx2/shaders.h>
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
        || !WHBGfxInitShaderAttribute(&shader, "aColour", 1, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitFetchShader(&shader)) OSFatal("Terrain probe: graphics setup failed");
    positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    positions.elemSize = 4 * sizeof(float); positions.elemCount = 24 * 24 * 6;
    colors.flags = positions.flags; colors.elemSize = positions.elemSize; colors.elemCount = positions.elemCount;
    if (!GX2RCreateBuffer(&positions) || !GX2RCreateBuffer(&colors)) OSFatal("Terrain probe: buffer creation failed");
    unsigned frame = 0; Snapshot snap = {0};
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
                    }
                } else if (kerr != KPAD_ERROR_NO_SAMPLES) { pro_valid[channel] = 0; }
                if (!pro_valid[channel]) continue;
                if (pro[channel].pro.hold & WPAD_PRO_BUTTON_A) flags |= 1;
                if (pro[channel].pro.leftStick.x > 0.25f || pro[channel].pro.leftStick.x < -0.25f) lx = pro[channel].pro.leftStick.x;
                if (pro[channel].pro.leftStick.y > 0.25f || pro[channel].pro.leftStick.y < -0.25f) ly = pro[channel].pro.leftStick.y;
                if (pro[channel].pro.rightStick.x > 0.25f || pro[channel].pro.rightStick.x < -0.25f) rx = pro[channel].pro.rightStick.x;
                if (pro[channel].pro.rightStick.y > 0.25f || pro[channel].pro.rightStick.y < -0.25f) ry = pro[channel].pro.rightStick.y;
            }
            c2_move_tick(sim, lx, ly, rx, ry, flags, &snap);
            GX2DrawDone();
            float *p = GX2RLockBufferEx(&positions, 0), *c = GX2RLockBufferEx(&colors, 0);
            if (!p || !c) OSFatal("Terrain probe: buffer lock failed");
            uint32_t count = c2_terrain_vertices(sim, p, c);
            GX2RUnlockBufferEx(&positions, 0); GX2RUnlockBufferEx(&colors, 0);
            WHBGfxBeginRender();
            for (int display = 0; display < 2; ++display) {
                if (display == 0) WHBGfxBeginRenderTV(); else WHBGfxBeginRenderDRC();
                WHBGfxClearColor(0.24f, 0.44f, 0.65f, 1.0f);
                GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                GX2RSetAttributeBuffer(&positions, 0, positions.elemSize, 0); GX2RSetAttributeBuffer(&colors, 1, colors.elemSize, 0);
                GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, count, 0, 1);
                if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
            }
            WHBGfxFinishRender(); frame++;
        }
    }
    GX2DrawDone(); GX2RDestroyBufferEx(&positions, 0); GX2RDestroyBufferEx(&colors, 0);
    WHBGfxFreeShaderGroup(&shader); WHBGfxShutdown();
    if (sim) KPADShutdown();
    c2_move_destroy(sim); WHBProcShutdown(); return 0;
}
