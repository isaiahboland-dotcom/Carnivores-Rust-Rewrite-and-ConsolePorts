#include <coreinit/debug.h>
#include <coreinit/cache.h>
#include <coreinit/screen.h>
#include <sndcore2/core.h>
#include <sndcore2/voice.h>
#include <gx2/draw.h>
#include <gx2/event.h>
#include <gx2/registers.h>
#include <gx2/shaders.h>
#include <gx2/texture.h>
#include <gx2/surface.h>
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
#include <math.h>
#include <padscore/kpad.h>
#include <proc_ui/procui.h>
#include <padscore/wpad.h>
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vpad/input.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>
#include <whb/sdcard.h>

extern const unsigned char c2_sample_shader[];
extern const unsigned char c2_menu_shader[];
extern int32_t c2_sky_texture(void *, uint8_t *, uint32_t);
extern uint32_t c2_sky_vertices(void *, float *, float *);
extern void c2_fog_color(void *, float *);
extern int32_t c2_map_texture(void *, uint8_t *, uint32_t);
extern void c2_map_player(void *, float *, float *, float *, float *);
extern uint32_t c2_water_vertices(void *, float *, float *);
extern int32_t c2_scenery_atlas(void *, uint8_t *, uint32_t);
extern uint32_t c2_scenery_vertices(void *, float *, float *, uint32_t);
typedef struct { uint32_t first, count; } SceneryRange;
extern int32_t c2_scenery_resident_prepare(void *, int32_t, float *, float *, uint32_t, uint32_t *);
extern uint32_t c2_scenery_resident_ranges(void *, SceneryRange *, uint32_t, uint32_t *);
extern uint32_t c2_terrain_resident_update(void *, int32_t, float *, float *, float *, float *, SceneryRange *, uint32_t, uint32_t *);
extern void c2_terrain_resident_ranges(void *, SceneryRange *, SceneryRange *, uint32_t, uint32_t *, uint32_t *, uint32_t *, uint32_t *);
extern int32_t c2_hunt_setup(void *, const uint8_t **, const uint32_t *, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t);
extern uint32_t c2_death_active(void *);
extern int32_t c2_actor_atlas(void *, uint8_t *, uint32_t);
extern uint32_t c2_actor_vertices(void *, float *, float *, uint32_t, uint32_t);
extern int32_t c2_fx_load(void *, uint32_t, const uint8_t *, uint32_t);
extern int32_t c2_call_load(void *, uint32_t, const uint8_t *, uint32_t);
extern int32_t c2_sound_clip(void *, uint32_t, uint32_t, const uint8_t **, uint32_t *);
typedef struct { float x, z; uint32_t rgba; } RadarBlip;
extern void c2_set_radar(void *, uint32_t);
extern uint32_t c2_radar_blips(void *, RadarBlip *, uint32_t);
typedef struct { int32_t ammo; uint32_t weapon_ready; int32_t call; int32_t weapon; } HudState;
extern void c2_hud_state(void *, HudState *);
extern uint32_t c2_hunt_dead(void *, uint32_t *, uint32_t *);
extern int32_t c2_restart_hunt(void *, uint32_t);
extern int32_t c2_menu_picture(const uint8_t *, uint32_t, uint8_t *, uint32_t, uint32_t);
extern int32_t c2_icon_load(void *, const uint8_t *, uint32_t);
extern int32_t c2_icon_atlas(void *, uint8_t *, uint32_t, uint32_t, uint32_t);
extern void c2_scenery_camera(void *, float *);
extern void c2_static_view_key(void *, float *);
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
void c2_hunt_stage(uint32_t stage) { OSReport("C2 hunt diagnostic stage %u\n", stage); }
void c2_hunt_values(uint32_t toggle, uint32_t hidden, uint32_t underwater, int32_t health, uint32_t active) {
    OSReport("C2 binoculars toggle %u hidden %u underwater %u health %d active %u\n", toggle, hidden, underwater, health, active);
}
typedef struct { int16_t *samples; uint32_t count; } AudioClip;
static AudioClip audio_clips[26][64];
static AXVoice *audio_voices[8];
static AXVoice *ambient_voice;
static int32_t audio_owner[8];
static uint32_t audio_kind[8], audio_slot[8];
static bool audio_initialized;
static int ambient_active;
static int32_t ambient_zone;
static float ambient_gain;
static void audio_init(void *sim) {
    AXInitParams params = {0}; params.renderer = AX_INIT_RENDERER_48KHZ; params.pipeline = AX_INIT_PIPELINE_SINGLE;
    AXInitWithParams(&params);
    if (!AXIsInit()) { OSReport("C2 audio initialization failed\n"); return; }
    audio_initialized = true;
    for (uint32_t kind = 0; kind < 26; ++kind) for (uint32_t slot = 0; slot < 64; ++slot) {
        const uint8_t *src = NULL; uint32_t len = 0;
        if (c2_sound_clip(sim, kind, slot, &src, &len) || len < 2) continue;
        int16_t *samples = memalign(64, len);
        if (!samples) continue;
        for (uint32_t n = 0; n < len / 2; ++n) samples[n] = (int16_t)((uint16_t)src[n * 2] | ((uint16_t)src[n * 2 + 1] << 8));
        DCFlushRange(samples, len);
        audio_clips[kind][slot] = (AudioClip){samples, len / 2};
    }
    for (unsigned i = 0; i < 8; ++i) { audio_voices[i] = AXAcquireVoice(31, NULL, NULL); audio_owner[i] = -1; }
    ambient_voice = AXAcquireVoice(31, NULL, NULL);
}
void c2_stop_animal(uint32_t index) {
    if (!audio_initialized) return;
    for (unsigned i = 0; i < 8; ++i) if (audio_voices[i] && audio_owner[i] == (int32_t)index) {
        AXVoiceBegin(audio_voices[i]); AXSetVoiceState(audio_voices[i], AX_VOICE_STATE_STOPPED); AXVoiceEnd(audio_voices[i]);
        audio_owner[i] = -1;
    }
}
void c2_play_sound(uint32_t kind, uint32_t slot, int32_t owner, float left, float right) {
    if (!audio_initialized || kind >= 26 || slot >= 64) return;
    if (left <= 0.0f && right <= 0.0f) return;
    AudioClip *clip = &audio_clips[kind][slot]; if (!clip->samples || !clip->count) return;
    AXVoice *voice = NULL; unsigned voice_index = 0;
    for (unsigned i = 0; i < 8; ++i) if (audio_voices[i] && !AXIsVoiceRunning(audio_voices[i])) { voice = audio_voices[i]; voice_index = i; break; }
    if (!voice) return;
    audio_owner[voice_index] = owner;
    audio_kind[voice_index] = kind; audio_slot[voice_index] = slot;
    AXVoiceOffsets offsets = {0}; offsets.dataType = AX_VOICE_FORMAT_LPCM16; offsets.data = clip->samples; offsets.endOffset = clip->count - 1;
    AXVoiceVeData ve = {0x8000, 0};
    AXVoiceDeviceMixData mix[6] = {0};
    mix[0].bus[0].volume = (uint16_t)(fminf(fmaxf(left, 0.0f), 1.0f) * 32768.0f);
    mix[1].bus[0].volume = (uint16_t)(fminf(fmaxf(right, 0.0f), 1.0f) * 32768.0f);
    AXVoiceBegin(voice);
    AXSetVoiceOffsets(voice, &offsets); AXSetVoiceVe(voice, &ve);
    AXSetVoiceDeviceMix(voice, AX_DEVICE_TYPE_TV, 0, mix); AXSetVoiceDeviceMix(voice, AX_DEVICE_TYPE_DRC, 0, mix);
    AXSetVoiceSrcType(voice, AX_VOICE_SRC_TYPE_LINEAR); AXSetVoiceSrcRatio(voice, 22050.0f / (float)AXGetInputSamplesPerSec());
    AXSetVoiceState(voice, AX_VOICE_STATE_PLAYING); AXVoiceEnd(voice);
}
void c2_set_ambient(int32_t zone, float gain) {
    if (!audio_initialized || !ambient_voice) return;
    AXVoiceBegin(ambient_voice);
    AXSetVoiceState(ambient_voice, AX_VOICE_STATE_STOPPED);
    AudioClip *clip = zone < 0 ? &audio_clips[19][6] : zone < 64 ? &audio_clips[18][zone] : NULL;
    ambient_active = clip && clip->count; ambient_zone = zone; ambient_gain = gain;
    if (clip && clip->count) {
        AXVoiceOffsets offsets = {0}; offsets.dataType = AX_VOICE_FORMAT_LPCM16;
        offsets.loopingEnabled = AX_VOICE_LOOP_ENABLED; offsets.data = clip->samples; offsets.endOffset = clip->count - 1;
        AXVoiceVeData ve = {0x8000, 0}; AXVoiceDeviceMixData mix[6] = {0};
        mix[0].bus[0].volume = mix[1].bus[0].volume = (uint16_t)(fminf(fmaxf(gain, 0.0f), 1.0f) * 32768.0f);
        AXSetVoiceOffsets(ambient_voice, &offsets); AXSetVoiceVe(ambient_voice, &ve);
        AXSetVoiceDeviceMix(ambient_voice, AX_DEVICE_TYPE_TV, 0, mix); AXSetVoiceDeviceMix(ambient_voice, AX_DEVICE_TYPE_DRC, 0, mix);
        AXSetVoiceSrcType(ambient_voice, AX_VOICE_SRC_TYPE_LINEAR); AXSetVoiceSrcRatio(ambient_voice, 22050.0f / (float)AXGetInputSamplesPerSec());
        AXSetVoiceState(ambient_voice, AX_VOICE_STATE_PLAYING);
    }
    AXVoiceEnd(ambient_voice);
}
static void audio_silence(int keep_death_sound) {
    if (!audio_initialized) return;
    for (unsigned i=0;i<8;++i) {
        if (keep_death_sound && audio_kind[i] == 19 && audio_slot[i] >= 7) continue;
        if (audio_voices[i]) { AXVoiceBegin(audio_voices[i]); AXSetVoiceState(audio_voices[i], AX_VOICE_STATE_STOPPED); AXVoiceEnd(audio_voices[i]); }
        audio_owner[i] = -1;
    }
    if (ambient_voice) { AXVoiceBegin(ambient_voice); AXSetVoiceState(ambient_voice, AX_VOICE_STATE_STOPPED); AXVoiceEnd(ambient_voice); }
}
static void audio_shutdown(void) {
    if (!audio_initialized) return;
    for (unsigned i = 0; i < 8; ++i) if (audio_voices[i]) { AXSetVoiceState(audio_voices[i], AX_VOICE_STATE_STOPPED); AXFreeVoice(audio_voices[i]); }
    if (ambient_voice) { AXSetVoiceState(ambient_voice, AX_VOICE_STATE_STOPPED); AXFreeVoice(ambient_voice); ambient_voice = NULL; }
    AXQuit();
    for (unsigned i = 0; i < 26; ++i) for (unsigned j = 0; j < 64; ++j) free(audio_clips[i][j].samples);
    memset(audio_clips, 0, sizeof(audio_clips)); memset(audio_voices, 0, sizeof(audio_voices));
    memset(audio_kind, 0, sizeof(audio_kind)); memset(audio_slot, 0, sizeof(audio_slot));
    audio_initialized = false;
}

// Compact built-in 5x7 font; unknown characters are blank.
static void audio_resume(void *sim) {
    audio_init(sim);
    if (ambient_active) c2_set_ambient(ambient_zone, ambient_gain);
}
static const char glyph_chars[] = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ.:/-";
static const uint8_t glyph_rows[][7] = {
{14,17,19,21,25,17,14},{4,12,4,4,4,4,14},{14,17,1,2,4,8,31},{30,1,1,14,1,1,30},{2,6,10,18,31,2,2},{31,16,16,30,1,1,30},{14,16,16,30,17,17,14},{31,1,2,4,8,8,8},{14,17,17,14,17,17,14},{14,17,17,15,1,1,14},
{14,17,17,31,17,17,17},{30,17,17,30,17,17,30},{14,17,16,16,16,17,14},{30,17,17,17,17,17,30},{31,16,16,30,16,16,31},{31,16,16,30,16,16,16},{14,17,16,23,17,17,15},{17,17,17,31,17,17,17},{14,4,4,4,4,4,14},{7,2,2,2,18,18,12},{17,18,20,24,20,18,17},{16,16,16,16,16,16,31},{17,27,21,21,17,17,17},{17,25,21,19,17,17,17},{14,17,17,17,17,17,14},{30,17,17,30,16,16,16},{14,17,17,17,21,18,13},{30,17,17,30,20,18,17},{15,16,16,14,1,1,30},{31,4,4,4,4,4,4},{17,17,17,17,17,17,14},{17,17,17,17,17,10,4},{17,17,17,21,21,21,10},{17,17,10,4,10,17,17},{17,17,10,4,4,4,4},{31,1,2,4,8,16,31},
{0,0,0,0,0,6,6},{0,6,6,0,6,6,0},{1,2,2,4,8,8,16},{0,0,0,31,0,0,0}
};
static void perf_line(GX2Texture *tex, int row, const char *text) {
    uint8_t *pixels = tex->surface.image;
    for (int c = 0; text[c] && c < 42; ++c) {
        const char *g = strchr(glyph_chars, text[c]);
        if (!g) continue;
        size_t index = (size_t)(g - glyph_chars);
        for (int y = 0; y < 7; ++y) for (int x = 0; x < 5; ++x) {
            if (!(glyph_rows[index][y] & (1 << (4-x)))) continue;
            for (int yy = 0; yy < 2; ++yy) for (int xx = 0; xx < 2; ++xx) {
                uint8_t *p = pixels + ((row * 19 + 5 + y * 2 + yy) * tex->surface.pitch + 5 + c * 12 + x * 2 + xx) * 4;
                p[0] = 255; p[1] = 255; p[2] = 255; p[3] = 255;
            }
        }
    }
}
typedef struct { uint64_t frame, wait, terrain, scenery, water, sky, map, draw, present, simulation, actors; uint32_t frames, terrain_v, scenery_v, water_v, actor_v, tiled_atlases, static_cache_enabled, resident_enabled, resident_active; } Perf;
static double avg_ms(uint64_t ticks, uint32_t frames) { return frames ? (double)OSTicksToMicroseconds(ticks) / (1000.0 * frames) : 0.0; }
static void perf_update(GX2Texture *tex, const Perf *p) {
    uint8_t *pixels = tex->surface.image;
    for (uint32_t y = 0; y < tex->surface.height; ++y) for (uint32_t x = 0; x < tex->surface.width; ++x) {
        uint8_t *q = pixels + (y * tex->surface.pitch + x) * 4;
        q[0] = 8; q[1] = 16; q[2] = 10; q[3] = 255;
    }
    char line[64];
    snprintf(line, sizeof(line), "AVG MS FRAME %.1f WAIT %.1f", avg_ms(p->frame,p->frames), avg_ms(p->wait,p->frames)); perf_line(tex,0,line);
    snprintf(line, sizeof(line), "CPU TERRAIN %.1f SCENERY %.1f", avg_ms(p->terrain,p->frames), avg_ms(p->scenery,p->frames)); perf_line(tex,1,line);
    snprintf(line, sizeof(line), "CPU WATER %.1f SKY %.1f MAP %.1f", avg_ms(p->water,p->frames), avg_ms(p->sky,p->frames), avg_ms(p->map,p->frames)); perf_line(tex,2,line);
    snprintf(line, sizeof(line), "CPU DRAW %.1f PRESENT %.1f", avg_ms(p->draw,p->frames), avg_ms(p->present,p->frames)); perf_line(tex,3,line);
    snprintf(line, sizeof(line), "CPU SIM %.1f ACTORS %.1f", avg_ms(p->simulation,p->frames),avg_ms(p->actors,p->frames)); perf_line(tex,4,line);
    snprintf(line, sizeof(line), "%c%c%c T %u S %u A %u", p->tiled_atlases ? 'T' : 'L', p->static_cache_enabled ? 'C' : 'B', p->resident_enabled ? (p->resident_active ? 'R' : 'F') : 'B', p->terrain_v,p->scenery_v,p->actor_v); perf_line(tex,5,line);
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, tex->surface.image, tex->surface.imageSize);
}
#include "menu.h"
#include "call_picker.h"

static void hud_update(GX2Texture *tex, const HudState *hud, uint32_t selection) {
    memset(tex->surface.image, 0, tex->surface.imageSize);
    char line[40];
    static const char *names[6] = {"PISTOL","SHOTGUN","DB SHOTGUN","X-BOW","RIFLE","SNIPER"};
    int weapon=0, rank=hud->weapon; for(int i=0;i<6;++i) if(selection&(1u<<i)) { if(rank--==0) { weapon=i; break; } }
    snprintf(line, sizeof(line), "%s %d", names[weapon], hud->ammo); perf_line(tex, 0, line);
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, tex->surface.image, tex->surface.imageSize);
}

#include "stream_upload.h"
#include "map_view.h"
#include "atlas_layout.h"
#include "static_view.h"

static FSClient client;
#include "performance.h"
#include "lifecycle.h"
/* Flush startup checkpoints to SD so a hardware GPU hang leaves evidence.
 * OSScreen is no longer usable after the handoff to GX2. */
static FSCmdBlock startup_cmd;
static FSFileHandle startup_file;
static int startup_open, startup_console;
static void startup_checkpoint(const char *message) {
    OSReport("C2 startup: %s\n", message);
    if (startup_console) { WHBLogPrint(message); WHBLogConsoleDraw(); }
    if (startup_open) {
        uint8_t line[256] __attribute__((aligned(64)));
        int n = snprintf((char *)line, sizeof(line), "%s\n", message);
        if (n > 0 && n < (int)sizeof(line)) {
            FSStatus written = FSWriteFile(&client, &startup_cmd, line, 1, n, startup_file, 0, FS_ERROR_FLAG_NONE);
            if (written == n) FSFlushFile(&client, &startup_cmd, startup_file, FS_ERROR_FLAG_NONE);
        }
    }
}
static void startup_trace_open(void) {
    char path[256];
    snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/startup.log", WHBGetSdCardMountPath());
    FSInitCmdBlock(&startup_cmd);
    startup_open = FSOpenFile(&client, &startup_cmd, path, "w", &startup_file, FS_ERROR_FLAG_NONE) == FS_STATUS_OK;
    startup_checkpoint("Hardware startup trace v3; SD mount and FS client: PASS");
    if (!startup_open) startup_checkpoint("WARNING: startup.log could not be opened for writing");
}
static void startup_trace_close(void) {
    if (startup_open) {
        FSCloseFile(&client, &startup_cmd, startup_file, FS_ERROR_FLAG_NONE);
        startup_open = 0;
    }
}
static int32_t load_file(const char *name, uint8_t **data, uint32_t *len) {
    char path[256]; FSCmdBlock cmd; FSFileHandle file; FSStat stat;
    char checkpoint[128]; snprintf(checkpoint, sizeof(checkpoint), "Reading %s...", name);
    startup_checkpoint(checkpoint);
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
    *data = p; *len = stat.size;
    snprintf(checkpoint, sizeof(checkpoint), "%s read: PASS (%u bytes)", name, stat.size);
    startup_checkpoint(checkpoint);
    return 0;
}
static void init_gpu_fog(GX2RBuffer *buffer, const float color[3], float mode) {
    float *dst = GX2RLockBufferEx(buffer, 0);
    if (!dst) OSFatal("Fog probe: buffer lock failed");
    for (uint32_t i = 0; i < buffer->elemCount; ++i) {
        dst[i * 4] = color[0]; dst[i * 4 + 1] = color[1]; dst[i * 4 + 2] = color[2]; dst[i * 4 + 3] = mode;
    }
    GX2RUnlockBufferEx(buffer, 0);
}
static void make_fog_buffer(GX2RBuffer *buffer, const GX2RBuffer *source) {
    buffer->flags = source->flags; buffer->elemSize = source->elemSize; buffer->elemCount = source->elemCount;
    if (!GX2RCreateBuffer(buffer)) OSFatal("Fog probe: buffer allocation failed");
}
static void init_quad(GX2RBuffer *pos, GX2RBuffer *attr, const GX2RBuffer *template, const float verts[24], const float uvs[24]) {
    memset(pos, 0, sizeof(*pos)); memset(attr, 0, sizeof(*attr));
    pos->flags = template->flags; pos->elemSize = template->elemSize; pos->elemCount = template->elemCount;
    attr->flags = template->flags; attr->elemSize = template->elemSize; attr->elemCount = template->elemCount;
    if (!GX2RCreateBuffer(pos) || !GX2RCreateBuffer(attr)) OSFatal("HUD: buffer allocation failed");
    float *p = GX2RLockBufferEx(pos, 0), *a = GX2RLockBufferEx(attr, 0);
    if (!p || !a) OSFatal("HUD: buffer lock failed");
    memcpy(p, verts, 24 * sizeof(float)); memcpy(a, uvs, 24 * sizeof(float));
    GX2RUnlockBufferEx(pos, 0); GX2RUnlockBufferEx(attr, 0);
}
static void init_weapon_grid(GX2RBuffer *pos, GX2RBuffer *attr, const GX2RBuffer *template) {
    memset(pos, 0, sizeof(*pos)); memset(attr, 0, sizeof(*attr));
    pos->flags = template->flags; pos->elemSize = template->elemSize; pos->elemCount = 36;
    attr->flags = template->flags; attr->elemSize = template->elemSize; attr->elemCount = 36;
    if (!GX2RCreateBuffer(pos) || !GX2RCreateBuffer(attr)) OSFatal("HUD: weapon grid allocation failed");
    float *p = GX2RLockBufferEx(pos, 0), *a = GX2RLockBufferEx(attr, 0);
    if (!p || !a) OSFatal("HUD: weapon grid lock failed");
    for (int slot=0; slot<6; ++slot) {
        float x0=-0.98f+(slot%2)*0.19f, x1=x0+0.19f;
        float y1=0.86f-(slot/2)*(1.72f/3.0f), y0=y1-1.72f/3.0f;
        float u0=(slot%2)*0.25f, u1=u0+0.25f;
        float v0=(slot/2)*(1.0f/3.0f), v1=v0+1.0f/3.0f;
        float verts[24]={x0,y1,0,1,x1,y1,0,1,x1,y0,0,1,x0,y1,0,1,x1,y0,0,1,x0,y0,0,1};
        float uvs[24]={u0,v0,1,0,u1,v0,1,0,u1,v1,1,0,u0,v0,1,0,u1,v1,1,0,u0,v1,1,0};
        memcpy(p+slot*24,verts,sizeof(verts)); memcpy(a+slot*24,uvs,sizeof(uvs));
    }
    GX2RUnlockBufferEx(pos, 0); GX2RUnlockBufferEx(attr, 0);
}
static int run_menu(uint8_t **menu_art, MenuState *selection, DeathMenu *death) {
    MenuState menu = *selection;
    WHBGfxShaderGroup shader = {0};
    startup_checkpoint("Menu: initializing GX2...");
    if (!death && !WHBGfxInit()) OSFatal("Menu: GX2 initialization failed");
    if (!death) {
        /* WHBLogConsoleFree calls OSScreenShutdown, which disables the
         * hardware display outputs. Restore them after GX2 owns its buffers;
         * swap/flip progress alone does not prove that output is enabled. */
        OSScreenEnableEx(SCREEN_TV, TRUE);
        OSScreenEnableEx(SCREEN_DRC, TRUE);
        startup_checkpoint("Display handoff: TV and GamePad re-enabled after console shutdown");
        /* Separate scanout from shader/texture drawing on physical hardware. */
        startup_checkpoint("Display check: blue TV / magenta GamePad for 2 seconds...");
        for (int frame = 0; frame < 120 && app_running(); ++frame) {
            WHBGfxBeginRender();
            WHBGfxBeginRenderTV(); WHBGfxClearColor(0.0f, 0.15f, 1.0f, 1.0f); WHBGfxFinishRenderTV();
            WHBGfxBeginRenderDRC(); WHBGfxClearColor(0.8f, 0.0f, 0.65f, 1.0f); WHBGfxFinishRenderDRC();
            WHBGfxFinishRender();
            if (frame == 0) startup_checkpoint("Display check: first clear-only frame returned PASS");
        }
        startup_checkpoint("Display check: clear-only loop returned; starting textured menu");
    }
    startup_checkpoint("Menu: GX2 initialized; loading shader...");
    if (!WHBGfxLoadGFDShaderGroup(&shader, 0, c2_menu_shader)
        || !WHBGfxInitShaderAttribute(&shader, "aPosition", 0, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitShaderAttribute(&shader, "aTexLight", 1, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitFetchShader(&shader)) OSFatal("Menu: graphics setup failed");
    startup_checkpoint("Menu: shader setup PASS; allocating texture...");
    GX2Texture menu_tex = {0};
    menu_tex.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    menu_tex.surface.width = 512; menu_tex.surface.height = 256; menu_tex.surface.depth = 1;
    menu_tex.surface.mipLevels = 1; menu_tex.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    menu_tex.surface.aa = GX2_AA_MODE1X; menu_tex.surface.use = GX2_SURFACE_USE_TEXTURE;
    menu_tex.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&menu_tex.surface);
    menu_tex.surface.image = memalign(menu_tex.surface.alignment, menu_tex.surface.imageSize);
    if (!menu_tex.surface.image) OSFatal("Menu: texture allocation failed");
    menu_tex.viewNumMips = 1; menu_tex.viewNumSlices = 1;
    menu_tex.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&menu_tex);
    GX2Sampler map_sampler = {0}; GX2InitSampler(&map_sampler, GX2_TEX_CLAMP_MODE_CLAMP, GX2_TEX_XY_FILTER_MODE_POINT);
    uint32_t sampler_location = shader.pixelShader->samplerVars[0].location;
    GX2RBuffer template = {0}, menu_pos = {0}, menu_attr = {0};
    template.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    template.elemSize = 4*sizeof(float); template.elemCount = 6;
    const float mv[24] = {-1,1,0,1, 1,1,0,1, 1,-1,0,1, -1,1,0,1, 1,-1,0,1, -1,-1,0,1};
    const float ma[24] = {0,0,1,0, 1,0,1,0, 1,1,1,0, 0,0,1,0, 1,1,1,0, 0,1,1,0};
    init_quad(&menu_pos, &menu_attr, &template, mv, ma);
    startup_checkpoint("Menu: texture and vertex buffers PASS; initializing controllers...");
    if (!death) { KPADInit(); WPADEnableURCC(TRUE); }
    startup_checkpoint("Menu: controllers initialized; entering frame loop...");
    OSReport("C2 menu: standalone UI shader ready\n");
    int menu_touch = 0;
    OSTime input_ready = OSGetTime() + (death ? OSMillisecondsToTicks(400) : 0);
    int first_frame = 1;
    uint32_t menu_frames = 0;
    while (app_running() && !(death ? death->action : menu.done)) {
        if (menu_frames == 1) startup_checkpoint("Menu: second frame entered");
        if (first_frame) startup_checkpoint("Menu: first frame entered; polling controllers...");
        int prev = 0, next = 0, activate = 0, back = 0;
        VPADStatus v = {0}; VPADReadError verr = VPAD_READ_NO_SAMPLES;
        if (VPADRead(VPAD_CHAN_0, &v, 1, &verr) > 0) {
            prev = !!(v.trigger & (VPAD_BUTTON_UP | VPAD_BUTTON_LEFT)); next = !!(v.trigger & (VPAD_BUTTON_DOWN | VPAD_BUTTON_RIGHT));
            activate = !!(v.trigger & VPAD_BUTTON_A); back = !!(v.trigger & VPAD_BUTTON_B);
            VPADTouchData t = {0}; VPADGetTPCalibratedPointEx(VPAD_CHAN_0, VPAD_TP_854X480, &t, &v.tpNormal);
            int touched = t.touched && t.validity == VPAD_VALID;
            if (touched && !menu_touch) {
                int tx = (int)t.x * 512 / 854, ty = (int)t.y * 256 / 480;
                int hit = death ? death_hit(tx, ty) : menu_hit(&menu, tx, ty);
                if (hit >= 0) { if (death) death->focus = hit; else menu.focus = hit; activate = 1; }
            }
            menu_touch = touched;
        }
        for (int channel = 0; channel < 4; ++channel) {
            KPADStatus p = {0}; KPADError err = KPAD_ERROR_NO_SAMPLES;
            if (KPADReadEx((KPADChan)channel, &p, 1, &err) && p.extensionType == WPAD_EXT_PRO_CONTROLLER) {
                prev |= !!(p.pro.trigger & (WPAD_PRO_BUTTON_UP | WPAD_PRO_BUTTON_LEFT)); next |= !!(p.pro.trigger & (WPAD_PRO_BUTTON_DOWN | WPAD_PRO_BUTTON_RIGHT));
                activate |= !!(p.pro.trigger & WPAD_PRO_BUTTON_A); back |= !!(p.pro.trigger & WPAD_PRO_BUTTON_B);
            }
        }
        if (OSGetTime() < input_ready) prev = next = activate = back = 0;
        if (death) death_input(death, prev, next, activate); else menu_input(&menu, prev, next, activate, back);
        if (first_frame) startup_checkpoint("Menu: controller polling PASS; waiting for GPU...");
        GX2DrawDone();
        if (first_frame) startup_checkpoint("Menu: GPU wait PASS; drawing menu texture...");
        if (death) death_draw(&menu_tex, death); else menu_draw(&menu_tex, menu_art, &menu);
        if (!death && menu.done) {
            menu_box(&menu_tex, 240, 65, 270, 125, 16, 22, 16);
            menu_text(&menu_tex, 270, 100, "LOADING HUNT...", 2, 240, 200, 70);
            GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, menu_tex.surface.image, menu_tex.surface.imageSize);
        }
        if (first_frame) startup_checkpoint("Menu: texture drawing PASS; submitting first frame...");
        WHBGfxBeginRender();
        for (int display = 0; display < 2; ++display) {
            if (display == 0) WHBGfxBeginRenderTV(); else WHBGfxBeginRenderDRC();
            WHBGfxClearColor(0.06f, 0.16f, 0.09f, 1.0f);
            GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
            /* Set output state explicitly rather than inheriting it from the
             * console/loader context on hardware. Blending is disabled. */
            GX2SetColorControl(GX2_LOGIC_OP_COPY, 0, FALSE, TRUE);
            GX2SetTargetChannelMasks(GX2_CHANNEL_MASK_RGBA, 0, 0, 0, 0, 0, 0, 0);
            GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE); GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
            GX2SetPixelTexture(&menu_tex, sampler_location); GX2SetPixelSampler(&map_sampler, sampler_location);
            GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
            GX2RSetAttributeBuffer(&menu_pos, 0, menu_pos.elemSize, 0); GX2RSetAttributeBuffer(&menu_attr, 1, menu_attr.elemSize, 0);
            GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
            if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
        }
        WHBGfxFinishRender();
        if (first_frame) { startup_checkpoint("Menu: first frame presented PASS"); first_frame = 0; }
        ++menu_frames;
        if (menu_frames == 2 || menu_frames % 120 == 0) {
            uint32_t swaps, flips; OSTime last_flip, last_vsync;
            GX2GetSwapStatus(&swaps, &flips, &last_flip, &last_vsync);
            char checkpoint[128];
            snprintf(checkpoint, sizeof(checkpoint), "Menu: frame %u returned; swaps %u flips %u; page %d", menu_frames, swaps, flips, menu.page);
            startup_checkpoint(checkpoint);
        }
    }
    /* Each menu frame ends with WHBGfxFinishRender completion. */
    GX2RDestroyBufferEx(&menu_pos, 0); GX2RDestroyBufferEx(&menu_attr, 0);
    free(menu_tex.surface.image); WHBGfxFreeShaderGroup(&shader); if (!death) KPADShutdown();
    OSReport("C2 menu: exit done %d area %d targets %d\n", menu.done, menu.area + 1, menu.animal);
    *selection = menu;
    return death ? death->action : menu.done;
}
// Fold the live Wii U timebase and a session counter into the game's PRNG seed.
static uint32_t next_hunt_seed(void) {
    static uint32_t serial;
    uint64_t ticks = OSGetTime();
    uint32_t seed = (uint32_t)ticks ^ (uint32_t)(ticks >> 32) ^ (++serial * 0x9e3779b9u);
    OSReport("C2 population seed %u: 48 targets, 18 ambient animals\n", seed);
    return seed;
}
static int run_session(MenuState *selection) {
    int session_result = HUNT_EXIT;
    uint32_t hunt_seed = 0;
    int mounted = 0, added = 0, graphics_ready = 0; void *sim = NULL;
    uint8_t *map = NULL, *rsc = NULL; uint32_t map_len = 0, rsc_len = 0;
    uint8_t *menu_art[22] = {0};
    MenuState menu = *selection; menu.page = 0; menu.focus = 0; menu.done = 0; menu.warning = 0;
    uint8_t *hunt_data[23] = {0}; uint32_t hunt_lengths[23] = {0};
    WHBLogConsoleInit();
    startup_console = 1;
    WHBLogPrint("Carnivores Wii U stock hunt");
    WHBLogPrint("Loading hunt menu from SD..."); WHBLogConsoleDraw();
    if (!WHBMountSdCard()) { WHBLogPrint("SD mount: FAIL"); goto end_load; }
    mounted = 1;
    startup_checkpoint("SD mount: PASS; adding FS client...");
    FSStatus s = FSAddClient(&client, FS_ERROR_FLAG_ALL);
    if (s != FS_STATUS_OK) { WHBLogPrintf("FS client: FAIL %d", s); goto end_load; }
    added = 1;
    startup_trace_open();
    lifecycle_trace_open();
    int32_t status = 0;
    const char *art_names[22] = {"MENUM.TGA", "AREA1.TGA", "AREA2.TGA", "AREA3.TGA", "AREA4.TGA", "AREA5.TGA", "DINO1.TGA", "DINO2.TGA", "DINO3.TGA", "DINO4.TGA", "DINO5.TGA", "DINO6.TGA", "DINO7.TGA", "DINO8.TGA", "DINO9.TGA", "WEAPON1.TGA", "WEAPON2.TGA", "WEAPON3.TGA", "WEAPON4.TGA", "WEAPON5.TGA", "WEAPON6.TGA", "EQUIP2.TGA"};
    for (int i = 0; i < 22; ++i) {
        if (!app_running()) goto end_load;
        uint8_t *data = NULL; uint32_t len = 0;
        status = load_file(art_names[i], &data, &len);
        if (!status) {
            menu_art[i] = malloc(512*256*4);
            status = menu_art[i] ? c2_menu_picture(data, len, menu_art[i], 512, 256) : -1000;
        }
        free(data);
        if (status) { WHBLogPrintf("%s: FAIL %d", art_names[i], status); goto end_load; }
        char checkpoint[96]; snprintf(checkpoint, sizeof(checkpoint), "%s decode: PASS (%d/22)", art_names[i], i + 1);
        startup_checkpoint(checkpoint);
    }
    startup_checkpoint("All 22 menu pictures decoded; switching from console to GX2...");
    WHBLogConsoleFree();
    startup_console = 0;
    int start_hunt = run_menu(menu_art, &menu, NULL);
    *selection = menu;
    graphics_ready = 1;
    for (int i = 0; i < 22; ++i) { free(menu_art[i]); menu_art[i] = NULL; }
    if (!start_hunt) goto end_load;
    char area_file[24];
    snprintf(area_file, sizeof(area_file), "AREA%d.MAP", menu.area + 1);
    WHBLogPrintf("Loading %s...", menu_areas[menu.area]); WHBLogConsoleDraw();
    status = load_file(area_file, &map, &map_len);
    if (status) { WHBLogPrintf("%s read: FAIL %d", area_file, status); goto end_load; }
    snprintf(area_file, sizeof(area_file), "AREA%d.RSC", menu.area + 1);
    status = load_file(area_file, &rsc, &rsc_len);
    if (status) { WHBLogPrintf("%s read: FAIL %d", area_file, status); goto end_load; }
    status = c2_move_create(map, map_len, rsc, rsc_len, &sim);
    free(map); map = NULL; free(rsc); rsc = NULL;
    if (status) { WHBLogPrintf("Map parse: FAIL %d", status); goto end_load; }
    const char *hunt_files[23] = {"_RES.TXT", "MOSH.CAR", "GALL.CAR", "DIMOR2.CAR", "PTERA.CAR", "DIMET.CAR", "BRACHI1.CAR", "PAR2.CAR", "ANKYLO1.CAR", "STEGO.CAR", "ALLO.CAR", "CHASMOSA.CAR", "VELO2.CAR", "SPINO.CAR", "CERATO1.CAR", "TIREX.CAR", "PISTOL.CAR", "SHOTGUN.CAR", "DBSGUN.CAR", "X_BOW.CAR", "RIFLE.CAR", "SNIPER.CAR", "HUNTER1.CAR"};
    for (int i = 0; i < 23; ++i) {
        status = load_file(hunt_files[i], &hunt_data[i], &hunt_lengths[i]);
        if (status != 0) { WHBLogPrintf("%s read: FAIL %d", hunt_files[i], status); c2_move_destroy(sim); sim = NULL; goto end_load; }
    }
    hunt_seed = next_hunt_seed();
    status = c2_hunt_setup(sim, (const uint8_t **)hunt_data, hunt_lengths, menu.animal, menu.radar, (menu.blood_off ? 1u : 0u) | (menu.kill_anim_off ? 2u : 0u), menu.weapon, hunt_seed);
    if (status != 0) { WHBLogPrintf("Hunt assets: FAIL %d", status); c2_move_destroy(sim); sim = NULL; goto end_load; }
    const int calls[9] = {1,2,3,4,5,6,7,8,9};
    for (int i = 0; i < 27; ++i) {
        char name[24]; snprintf(name, sizeof(name), "CALL%d_%c.WAV", calls[i/3], 'A'+i%3);
        uint8_t *data = NULL; uint32_t len = 0;
        status = load_file(name, &data, &len);
        if (!status) status = c2_call_load(sim, i, data, len);
        free(data);
        if (status) { WHBLogPrintf("%s: FAIL %d", name, status); c2_move_destroy(sim); sim = NULL; goto end_load; }
    }
    const char *icon_names[15] = {"WEAPON1.TGA", "WEAPON2.TGA", "WEAPON3.TGA", "WEAPON4.TGA", "WEAPON5.TGA", "WEAPON6.TGA", "CALL1.TGA", "CALL2.TGA", "CALL3.TGA", "CALL4.TGA", "CALL5.TGA", "CALL6.TGA", "CALL7.TGA", "CALL8.TGA", "CALL9.TGA"};
    for (int i = 0; i < 15; ++i) {
        uint8_t *data = NULL; uint32_t len = 0;
        status = load_file(icon_names[i], &data, &len);
        if (!status) status = c2_icon_load(sim, data, len);
        free(data);
        if (status) { WHBLogPrintf("%s: FAIL %d", icon_names[i], status); c2_move_destroy(sim); sim = NULL; goto end_load; }
    }
    const char *fx_names[11] = {"HWALK1.WAV", "HWALK2.WAV", "HWALK3.WAV", "FOOTW1.WAV", "FOOTW2.WAV", "FOOTW3.WAV", "A_UNDERW.WAV", "HUM_DIE1.WAV", "HUM_DIE2.WAV", "HUM_DIE3.WAV", "HUM_DIE4.WAV"};
    for (int i = 0; i < 11; ++i) {
        uint8_t *data = NULL; uint32_t len = 0;
        status = load_file(fx_names[i], &data, &len);
        if (status == 0) status = c2_fx_load(sim, i, data, len);
        free(data);
        if (status != 0) { WHBLogPrintf("%s: FAIL %d", fx_names[i], status); c2_move_destroy(sim); sim = NULL; goto end_load; }
    }
    WHBLogPrint("Stock hunt and audio: READY"); WHBLogConsoleDraw();
    if (menu.area == 0) {
        uint32_t checks = c2_move_checks(sim);
        WHBLogPrintf("AREA1 movement checkpoints: %s", checks == 7 ? "PASS" : "FAIL");
    }
end_load:
    startup_checkpoint(sim ? "Hunt loading complete; releasing SD client" : "Session loading/menu ended without a hunt; releasing SD client");
    startup_trace_close();
    free(map); free(rsc);
    for (int i = 0; i < 23; ++i) free(hunt_data[i]);
    if (!sim) lifecycle_trace_close();
    if (!sim && added && FSDelClient(&client, FS_ERROR_FLAG_ALL) != FS_STATUS_OK) WHBLogPrint("FS client removal: FAIL");
    if (!sim && mounted && !WHBUnmountSdCard()) WHBLogPrint("SD unmount: FAIL");
    if (sim) {
        WHBLogPrint("L stick move, R stick look");
        WHBLogPrint("A jump; B sprint; D-pad up crouch");
        WHBLogPrint("D-pad down changes the selected call.");
    }
    WHBLogPrint("Use HOME to exit.");
    WHBLogConsoleDraw();
    if (!sim) { if (graphics_ready) { WHBGfxShutdown(); if (!lifecycle_exiting) WHBLogConsoleInit(); } for (int i = 0; i < 22; ++i) free(menu_art[i]); while (app_running()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); } WHBLogConsoleFree(); return HUNT_EXIT; }
    WHBLogConsoleFree();
    ambient_active = 0;
    audio_init(sim);
    lifecycle_sim = sim;
    WHBGfxShaderGroup shader = {0}; GX2RBuffer positions = {0}, colors = {0};
    if (!WHBGfxLoadGFDShaderGroup(&shader, 0, c2_sample_shader)
        || !WHBGfxInitShaderAttribute(&shader, "aPosition", 0, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitShaderAttribute(&shader, "aTexLight", 1, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitShaderAttribute(&shader, "aFog", 2, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitFetchShader(&shader)) OSFatal("Terrain probe: graphics setup failed");
    GX2UniformVar *eye_uniform = GX2GetVertexUniformVar(shader.vertexShader, "uEye");
    GX2UniformVar *angles_uniform = GX2GetVertexUniformVar(shader.vertexShader, "uAngles");
    if (!eye_uniform || !angles_uniform) OSFatal("Scenery probe: camera uniforms missing");
    GX2Texture atlas = {0}; GX2Sampler sampler = {0};
    atlas.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    atlas.surface.width = 2048; atlas.surface.height = 2048; atlas.surface.depth = 1;
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
    GX2Texture actor_atlas = {0};
    actor_atlas.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D;
    actor_atlas.surface.width = 2048; actor_atlas.surface.height = 1024; actor_atlas.surface.depth = 1;
    actor_atlas.surface.mipLevels = 1; actor_atlas.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    actor_atlas.surface.aa = GX2_AA_MODE1X; actor_atlas.surface.use = GX2_SURFACE_USE_TEXTURE;
    actor_atlas.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&actor_atlas.surface);
    actor_atlas.surface.image = memalign(actor_atlas.surface.alignment, actor_atlas.surface.imageSize);
    if (!actor_atlas.surface.image) OSFatal("Hunt probe: actor atlas allocation failed");
    memset(actor_atlas.surface.image, 0, actor_atlas.surface.imageSize);
    if (c2_actor_atlas(sim, actor_atlas.surface.image, actor_atlas.surface.pitch)) OSFatal("Hunt probe: invalid actor atlas");
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, actor_atlas.surface.image, actor_atlas.surface.imageSize);
    actor_atlas.viewNumMips = 1; actor_atlas.viewNumSlices = 1;
    actor_atlas.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2InitTextureRegs(&actor_atlas);
    GX2Texture tiled_atlas = {0}, tiled_object_atlas = {0}, tiled_actor_atlas = {0};
    int tiled_available = atlas_tiled_copy(&atlas, &tiled_atlas)
        && atlas_tiled_copy(&object_atlas, &tiled_object_atlas)
        && atlas_tiled_copy(&actor_atlas, &tiled_actor_atlas);
    int tiled_atlases = tiled_available;
    OSReport("C2 tiled atlas A/B available %d; D-pad LEFT switches layouts\n", tiled_available);
    GX2RBuffer object_positions = {0}, object_attrs = {0};
    object_positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    object_positions.elemSize = 4 * sizeof(float); object_positions.elemCount = 524288;
    object_attrs.flags = object_positions.flags; object_attrs.elemSize = object_positions.elemSize; object_attrs.elemCount = object_positions.elemCount;
    if (!GX2RCreateBuffer(&object_positions) || !GX2RCreateBuffer(&object_attrs)) OSFatal("Scenery probe: object buffer allocation failed");
    GX2RBuffer actor_positions = {0}, actor_attrs = {0}, actor_fog = {0}, weapon_positions = {0}, weapon_attrs = {0}, weapon_fog = {0};
    actor_positions.flags = object_positions.flags; actor_positions.elemSize = object_positions.elemSize; actor_positions.elemCount = 65536;
    actor_attrs = actor_positions; weapon_positions = actor_positions; weapon_attrs = actor_positions;
    if (!GX2RCreateBuffer(&actor_positions) || !GX2RCreateBuffer(&actor_attrs) || !GX2RCreateBuffer(&weapon_positions) || !GX2RCreateBuffer(&weapon_attrs)) OSFatal("Hunt probe: actor buffer allocation failed");
    make_fog_buffer(&actor_fog, &actor_positions); make_fog_buffer(&weapon_fog, &weapon_positions);
    positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    positions.elemSize = 4 * sizeof(float); positions.elemCount = 148 * 148 * 6;
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
    GX2RBuffer map_pos = {0}, map_attr[2] = {{0}};
    map_pos.flags = positions.flags; map_pos.elemSize = 4 * sizeof(float); map_pos.elemCount = 6;
    if (!GX2RCreateBuffer(&map_pos)) OSFatal("Map probe: buffer allocation failed");
    // Immutable positions, independent UV streams: neither display rewrites a
    // buffer already submitted by the other display in this frame.
    const float map_verts[24] = {-0.5625f,1,0,1, 0.5625f,1,0,1, 0.5625f,-1,0,1,
                                 -0.5625f,1,0,1, 0.5625f,-1,0,1, -0.5625f,-1,0,1};
    float *map_vertex_data = GX2RLockBufferEx(&map_pos, 0);
    if (!map_vertex_data) OSFatal("Map probe: position lock failed");
    memcpy(map_vertex_data, map_verts, sizeof(map_verts));
    GX2RUnlockBufferEx(&map_pos, 0);
    for (int display = 0; display < 2; ++display) {
        map_attr[display].flags = map_pos.flags;
        map_attr[display].elemSize = map_pos.elemSize; map_attr[display].elemCount = 6;
        if (!GX2RCreateBuffer(&map_attr[display])) OSFatal("Map probe: UV allocation failed");
    }
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
    sky_pos.flags = positions.flags; sky_pos.elemSize = positions.elemSize; sky_pos.elemCount = 8 * 8 * 6;
    sky_attr.flags = positions.flags; sky_attr.elemSize = positions.elemSize; sky_attr.elemCount = 8 * 8 * 6;
    if (!GX2RCreateBuffer(&sky_pos) || !GX2RCreateBuffer(&sky_attr)) OSFatal("Sky probe: buffer allocation failed");
    GX2RBuffer terrain_fog = {0}, object_fog = {0}, water_fog = {0}, sky_fog = {0}, map_fog = {0};
    make_fog_buffer(&terrain_fog, &positions); make_fog_buffer(&object_fog, &object_positions);
    make_fog_buffer(&water_fog, &water_pos); make_fog_buffer(&sky_fog, &sky_pos); make_fog_buffer(&map_fog, &map_pos);
    float *map_fog_data = GX2RLockBufferEx(&map_fog, 0);
    if (!map_fog_data) OSFatal("Fog probe: map buffer lock failed");
    memset(map_fog_data, 0, 6 * 4 * sizeof(float)); GX2RUnlockBufferEx(&map_fog, 0);
    GX2Texture perf_tex = {0};
    perf_tex.surface.dim = GX2_SURFACE_DIM_TEXTURE_2D; perf_tex.surface.width = 512; perf_tex.surface.height = 128; perf_tex.surface.depth = 1;
    perf_tex.surface.mipLevels = 1; perf_tex.surface.format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    perf_tex.surface.aa = GX2_AA_MODE1X; perf_tex.surface.use = GX2_SURFACE_USE_TEXTURE; perf_tex.surface.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    GX2CalcSurfaceSizeAndAlignment(&perf_tex.surface);
    perf_tex.surface.image = memalign(perf_tex.surface.alignment, perf_tex.surface.imageSize);
    if (!perf_tex.surface.image) OSFatal("Perf probe: texture allocation failed");
    perf_tex.viewNumMips = 1; perf_tex.viewNumSlices = 1;
    perf_tex.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A); GX2InitTextureRegs(&perf_tex);
    GX2RBuffer perf_pos = {0}, perf_attr = {0};
    perf_pos.flags = map_pos.flags; perf_pos.elemSize = map_pos.elemSize; perf_pos.elemCount = 6;
    perf_attr = perf_pos;
    if (!GX2RCreateBuffer(&perf_pos) || !GX2RCreateBuffer(&perf_attr)) OSFatal("Perf probe: buffer allocation failed");
    const float pv[24] = {-0.98f,0.96f,0,1, -0.10f,0.96f,0,1, -0.10f,0.57f,0,1, -0.98f,0.96f,0,1, -0.10f,0.57f,0,1, -0.98f,0.57f,0,1};
    const float pa[24] = {0,0,1,0, 1,0,1,0, 1,1,1,0, 0,0,1,0, 1,1,1,0, 0,1,1,0};
    float *pp = GX2RLockBufferEx(&perf_pos,0), *pt = GX2RLockBufferEx(&perf_attr,0);
    if (!pp || !pt) OSFatal("Perf probe: buffer lock failed");
    memcpy(pp,pv,sizeof(pv)); memcpy(pt,pa,sizeof(pa)); GX2RUnlockBufferEx(&perf_pos,0); GX2RUnlockBufferEx(&perf_attr,0);
    Perf perf = {0}; perf_update(&perf_tex, &perf);
    GX2Texture hud_tex = perf_tex;
    hud_tex.surface.image = memalign(hud_tex.surface.alignment, hud_tex.surface.imageSize);
    if (!hud_tex.surface.image) OSFatal("HUD: texture allocation failed");
    GX2InitTextureRegs(&hud_tex);
    GX2Texture icons_tex = perf_tex;
    icons_tex.surface.height = 256;
    GX2CalcSurfaceSizeAndAlignment(&icons_tex.surface);
    int call_page = 0, prev_call_page = -1;
    icons_tex.surface.image = memalign(icons_tex.surface.alignment, icons_tex.surface.imageSize);
    if (!icons_tex.surface.image) OSFatal("HUD: icon texture allocation failed");
    memset(icons_tex.surface.image, 0, icons_tex.surface.imageSize);
    HudState hud = {0}; c2_hud_state(sim, &hud);
    if (c2_icon_atlas(sim, icons_tex.surface.image, icons_tex.surface.pitch, call_page, hud.weapon)) OSFatal("HUD: invalid icon images");
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, icons_tex.surface.image, icons_tex.surface.imageSize);
    GX2InitTextureRegs(&icons_tex);
    GX2RBuffer hud_pos = {0}, hud_attr = {0};
    GX2RBuffer panel_ammo_pos = {0}, panel_ammo_attr = {0}, gun_icon_pos = {0}, gun_icon_attr = {0}, call_icon_pos = {0}, call_icon_attr = {0};
    const float hv[24] = {0.54f,-0.76f,0,1, 0.98f,-0.76f,0,1, 0.98f,-0.96f,0,1, 0.54f,-0.76f,0,1, 0.98f,-0.96f,0,1, 0.54f,-0.96f,0,1};
    const float ha[24] = {0,0,1,1, .5f,0,1,1, .5f,.3125f,1,1, 0,0,1,1, .5f,.3125f,1,1, 0,.3125f,1,1};
    const float tav[24] = {-0.98f,0.32f,0,1, -0.60f,0.32f,0,1, -0.60f,0.22f,0,1, -0.98f,0.32f,0,1, -0.60f,0.22f,0,1, -0.98f,0.22f,0,1};
    const float taa[24] = {0,0,1,1, .5f,0,1,1, .5f,.15625f,1,1, 0,0,1,1, .5f,.15625f,1,1, 0,.15625f,1,1};
    const float civ[24] = {0.60f,0.86f,0,1, 0.98f,0.86f,0,1, 0.98f,-0.86f,0,1, 0.60f,0.86f,0,1, 0.98f,-0.86f,0,1, 0.60f,-0.86f,0,1};
    const float cia[24] = {.5f,0,1,0, 1,0,1,0, 1,1,1,0, .5f,0,1,0, 1,1,1,0, .5f,1,1,0};
    init_quad(&hud_pos, &hud_attr, &map_pos, hv, ha);
    init_quad(&panel_ammo_pos, &panel_ammo_attr, &map_pos, tav, taa);
    init_weapon_grid(&gun_icon_pos, &gun_icon_attr, &map_pos); init_quad(&call_icon_pos, &call_icon_attr, &map_pos, civ, cia);
    HudState prev_hud = {-1, 2, -1, -1};
    float fog_color[3]; c2_fog_color(sim, fog_color);
    init_gpu_fog(&terrain_fog, fog_color, -3.0f); init_gpu_fog(&water_fog, fog_color, -3.0f);
    init_gpu_fog(&object_fog, fog_color, -2.0f);
    init_gpu_fog(&actor_fog, fog_color, -2.0f); init_gpu_fog(&weapon_fog, fog_color, 0.0f);
    static uint8_t saved[151 * 151 * 4]; int old_x = -1, old_z = -1;
    static struct { uint32_t offset; uint8_t rgba[4]; } old_blips[256 * 9];
    uint32_t old_blip_count = 0;
    int full_map = 0, gamepad_game = 0, static_cache_enabled = 1;
    StaticView static_view = {0};
    int resident_enabled = 1;
    struct { int attempted, active; float cell[2]; uint32_t range_count; } resident = {0};
    static SceneryRange scenery_ranges[149 * 149];
    /* Resident terrain/water follows the PLUS residency toggle. Blocks of 4x4
     * cells: 37x37 ranges at most. reset forces a full rewrite. */
    struct { int active, reset; float cell[2]; uint32_t terrain_ranges, water_ranges; } terrain_resident = {0, 1};
    static SceneryRange terrain_ranges[37 * 37], water_ranges[37 * 37], terrain_written[37 * 37];
    Snapshot snap = {0};
    const uint64_t step_ticks = OSMillisecondsToTicks(16);
    uint64_t accumulated = 0; OSTime previous_time = OSGetTime();
    uint32_t pending_toggles = 0;
    int reported_gamepad = 0, reported_pro = 0, gamepad_connected = 0, touch_down = 0;
    VPADStatus gamepad = {0}; KPADStatus pro[4] = {0}; int pro_valid[4] = {0};
    if (sim) { KPADInit(); WPADEnableURCC(TRUE); }
    performance_open(&menu, hunt_seed, tiled_available);
    while (app_running()) {
        if (sim) {
            DeathMenu death = {0};
            OSTime frame_start = OSGetTime(), mark, draw_start;
            uint64_t frame_wait = 0, draw_wait = 0, gun_ticks = 0, map_cache_ticks = 0;
            uint32_t sim_ticks = 0;
            uint64_t overlay_start = OSGetTime();
            if (perf.frames >= 30) { perf_update(&perf_tex, &perf); memset(&perf, 0, sizeof(perf)); }
            uint64_t overlay_ticks = OSGetTime() - overlay_start;
            Perf before = perf;
            VPADStatus sample = {0}; VPADReadError error = VPAD_READ_NO_SAMPLES;
            uint32_t flags = 0;
            int32_t n = VPADRead(VPAD_CHAN_0, &sample, 1, &error);
            if (n > 0) {
                gamepad_connected = 1;
                gamepad = sample;
                if (!reported_gamepad) { reported_gamepad = 1; }
                if (sample.trigger & VPAD_BUTTON_B) flags |= 2;
                if (sample.trigger & VPAD_BUTTON_UP) flags |= 4;
                if (sample.trigger & VPAD_BUTTON_ZL) flags |= 8;
                if (sample.trigger & VPAD_BUTTON_DOWN) flags |= 128;
                if (sample.trigger & VPAD_BUTTON_Y) flags |= 4096;
                if (sample.trigger & VPAD_BUTTON_X) flags |= 32;
                if (sample.trigger & VPAD_BUTTON_PLUS) {
                    resident_enabled = !resident_enabled; resident.attempted = 0;
                    resident.active = 0; static_view.valid = 0; terrain_resident.reset = 1;
                }
                if (sample.trigger & VPAD_BUTTON_RIGHT) static_cache_enabled = !static_cache_enabled;
                if ((sample.trigger & VPAD_BUTTON_LEFT) && tiled_available) tiled_atlases = !tiled_atlases;
                if (sample.trigger & VPAD_BUTTON_R) full_map = !full_map;
                if (sample.trigger & VPAD_BUTTON_MINUS) gamepad_game = !gamepad_game;
                VPADTouchData touch = {0};
                VPADGetTPCalibratedPointEx(VPAD_CHAN_0, VPAD_TP_854X480, &touch, &sample.tpNormal);
                int down = touch.touched && touch.validity == VPAD_VALID;
                if (down && !touch_down && (!gamepad_game || full_map)) {
                    if (touch.x < 187) {
                        int slot=(touch.y/160)*2+(touch.x>=94);
                        int carried=0; for(int i=0;i<6;++i) if(menu.weapon&(1u<<i)) ++carried;
                        if(slot<carried) flags |= 64 | ((uint32_t)slot<<16);
                    }
                    else {
                        int hit = call_hit(touch.x, touch.y);
                        if (hit == 6 && call_page > 0) --call_page;
                        else if (hit == 7 && (call_page+1)*6 < call_count(menu.animal)) ++call_page;
                        else if (hit >= 0 && hit < 6) {
                            int target = call_index(menu.animal, call_page, hit);
                            if (target >= 0) flags |= 32 | ((uint32_t)(target+1) << 8);
                        }
                    }
                }
                touch_down = down;
            } else if (error == VPAD_READ_INVALID_CONTROLLER || error == VPAD_READ_UNINITIALIZED) {
                memset(&gamepad, 0, sizeof(gamepad)); gamepad_connected = 0; touch_down = 0;
            }
            float lx = gamepad.leftStick.x, ly = gamepad.leftStick.y;
            float rx = gamepad.rightStick.x, ry = gamepad.rightStick.y;
            if (gamepad.hold & VPAD_BUTTON_A) flags |= 1;
            if (gamepad.hold & VPAD_BUTTON_ZR) flags |= 16;
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
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_ZL) flags |= 8;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_DOWN) flags |= 128;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_Y) flags |= 4096;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_X) flags |= 32;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_PLUS) {
                            resident_enabled = !resident_enabled; resident.attempted = 0;
                            resident.active = 0; static_view.valid = 0; terrain_resident.reset = 1;
                        }
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_RIGHT) static_cache_enabled = !static_cache_enabled;
                        if ((sample_pro.pro.trigger & WPAD_PRO_BUTTON_LEFT) && tiled_available) tiled_atlases = !tiled_atlases;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_R) full_map = !full_map;
                        if (sample_pro.pro.trigger & WPAD_PRO_BUTTON_MINUS) gamepad_game = !gamepad_game;
                    }
                } else if (kerr != KPAD_ERROR_NO_SAMPLES) { pro_valid[channel] = 0; }
                if (!pro_valid[channel]) continue;
                if (pro[channel].pro.hold & WPAD_PRO_BUTTON_A) flags |= 1;
                if (pro[channel].pro.hold & WPAD_PRO_BUTTON_ZR) flags |= 16;
                if (pro[channel].pro.leftStick.x > 0.25f || pro[channel].pro.leftStick.x < -0.25f) lx = pro[channel].pro.leftStick.x;
                if (pro[channel].pro.leftStick.y > 0.25f || pro[channel].pro.leftStick.y < -0.25f) ly = pro[channel].pro.leftStick.y;
                if (pro[channel].pro.rightStick.x > 0.25f || pro[channel].pro.rightStick.x < -0.25f) rx = pro[channel].pro.rightStick.x;
                if (pro[channel].pro.rightStick.y > 0.25f || pro[channel].pro.rightStick.y < -0.25f) ry = pro[channel].pro.rightStick.y;
            }
            // Retain edge-triggered toggles across frames with no simulation step.
            pending_toggles ^= flags & 4334;
            pending_toggles |= flags & 32;
            if (flags & 0xf00) pending_toggles = (pending_toggles & ~0xf00u) | (flags & 0xf00);
            pending_toggles |= flags & 32;
            if (flags & 0xf00) pending_toggles = (pending_toggles & ~0xf00u) | (flags & 0xf00);
            OSTime now = OSGetTime();
            uint64_t elapsed = now > previous_time ? (uint64_t)(now - previous_time) : 0;
            previous_time = now;
            // Drop excess time after a long pause instead of an unbounded catch-up.
            const uint64_t max_elapsed = OSMillisecondsToTicks(128);
            if (elapsed > max_elapsed) elapsed = max_elapsed;
            accumulated += elapsed;
            if (accumulated > max_elapsed) accumulated = max_elapsed;
            mark = OSGetTime();
            while (accumulated >= step_ticks) {
                ++sim_ticks;
                c2_move_tick(sim, lx, ly, rx, ry, (flags & 17) | pending_toggles, &snap);
                pending_toggles = 0;
                accumulated -= step_ticks;
                if (c2_hunt_dead(sim, &death.shots, &death.kills)) break;
            }
            perf.simulation += OSGetTime() - mark;
            if (c2_hunt_dead(sim, &death.shots, &death.kills)) {
                audio_silence(1);
                int action = run_menu(NULL, &menu, &death);
                audio_silence(0);
                if (action == HUNT_RESTART) {
                    hunt_seed = next_hunt_seed();
                    if (c2_restart_hunt(sim, hunt_seed)) OSFatal("Hunt restart failed");
                    static_view.valid = 0; resident.attempted = 0; resident.active = 0; terrain_resident.reset = 1;
                    pending_toggles = 0; accumulated = 0; previous_time = OSGetTime();
                    memset(&gamepad, 0, sizeof(gamepad)); memset(pro, 0, sizeof(pro)); memset(pro_valid, 0, sizeof(pro_valid));
                    memset(&perf, 0, sizeof(perf)); touch_down = 0; full_map = 0;
                    prev_hud = (HudState){-1, 2, -1};
                    continue;
                }
                session_result = action;
                break;
            }
            mark = OSGetTime(); GX2DrawDone(); frame_wait += OSGetTime() - mark;
            c2_hud_state(sim, &hud);
            if (hud.call != prev_hud.call) call_page = call_page_for(menu.animal, hud.call);
            if (hud.call != prev_hud.call || hud.weapon != prev_hud.weapon || call_page != prev_call_page) {
                if (c2_icon_atlas(sim, icons_tex.surface.image, icons_tex.surface.pitch, call_page, hud.weapon)) OSFatal("HUD: call icon update failed");
                call_labels(&icons_tex, menu.animal, call_page, hud.call);
                prev_call_page = call_page;
                GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, icons_tex.surface.image, icons_tex.surface.imageSize);
            }
            if (hud.call != prev_hud.call || hud.weapon != prev_hud.weapon || hud.ammo != prev_hud.ammo || hud.weapon_ready != prev_hud.weapon_ready) { hud_update(&hud_tex, &hud, menu.weapon); prev_hud = hud; }
            memset(&stream_upload, 0, sizeof(stream_upload));
            perf.tiled_atlases = tiled_atlases; perf.static_cache_enabled = static_cache_enabled;
            int death_active = c2_death_active(sim);
            int show_map[2] = {map_view_visible(death_active, full_map, gamepad_game, 0),
                              map_view_visible(death_active, full_map, gamepad_game, 1)};
            int world_visible = !show_map[0] || !show_map[1];
            uint32_t count = 0, object_count = 0, actor_count = 0, weapon_count = 0, water_count = 0, sky_count = 0;
            int static_reused = 0;
            uint32_t scenery_uploaded = 0;
            if (world_visible) {
                float static_key[7]; c2_static_view_key(sim, static_key);
                if (resident_enabled && (!resident.attempted || resident.cell[0] != static_key[5] || resident.cell[1] != static_key[6])) {
                    mark = OSGetTime();
                    float *rp = stream_lock(&object_positions), *ra = stream_lock(&object_attrs);
                    if (!rp || !ra) OSFatal("Scenery: resident buffer lock failed");
                    SceneryRange written = {0, 0};
                    int32_t uploaded = c2_scenery_resident_prepare(sim, !resident.attempted || !resident.active, rp, ra,
                                                                   object_positions.elemCount, &written.first);
                    written.count = scenery_uploaded = uploaded < 0 ? 0u : (uint32_t)uploaded;
                    stream_unlock_ranges(&object_positions, &written, 1); stream_unlock_ranges(&object_attrs, &written, 1);
                    resident.attempted = 1; resident.active = uploaded >= 0;
                    resident.cell[0] = static_key[5]; resident.cell[1] = static_key[6];
                    static_view.valid = 0;
                    perf.scenery += OSGetTime() - mark;
                }
                if (resident_enabled != terrain_resident.active) {
                    float mode = resident_enabled ? -4.0f : -3.0f;
                    init_gpu_fog(&terrain_fog, fog_color, mode); init_gpu_fog(&water_fog, fog_color, mode);
                    terrain_resident.active = resident_enabled; terrain_resident.reset = 1;
                }
                if (terrain_resident.active && (terrain_resident.reset || terrain_resident.cell[0] != static_key[5] || terrain_resident.cell[1] != static_key[6])) {
                    mark = OSGetTime();
                    float *tp = stream_lock(&positions), *ta = stream_lock(&colors);
                    float *wp = stream_lock(&water_pos), *wa = stream_lock(&water_attr);
                    if (!tp || !ta || !wp || !wa) OSFatal("Terrain: resident buffer lock failed");
                    uint32_t written = 0;
                    if (c2_terrain_resident_update(sim, terrain_resident.reset, tp, ta, wp, wa, terrain_written, 37 * 37, &written) == UINT32_MAX)
                        OSFatal("Terrain: resident range overflow");
                    GX2RBuffer *streams[4] = {&positions, &colors, &water_pos, &water_attr};
                    for (int i = 0; i < 4; ++i) stream_unlock_ranges(streams[i], terrain_written, written);
                    terrain_resident.reset = 0;
                    terrain_resident.cell[0] = static_key[5]; terrain_resident.cell[1] = static_key[6];
                    static_view.valid = 0;
                    perf.terrain += OSGetTime() - mark;
                }
                static_reused = static_view_matches(&static_view, static_key, static_cache_enabled);
                if (static_reused) {
                    count = static_view.terrain; object_count = static_view.scenery;
                    water_count = static_view.water; sky_count = static_view.sky;
                }
                mark = OSGetTime();
                float *p, *c;
                if (!static_reused) {
                    if (terrain_resident.active) {
                        c2_terrain_resident_ranges(sim, terrain_ranges, water_ranges, 37 * 37, &terrain_resident.terrain_ranges, &count,
                                                   &terrain_resident.water_ranges, &water_count);
                    } else {
                        p = stream_lock(&positions); c = stream_lock(&colors);
                        if (!p || !c) OSFatal("Terrain probe: buffer lock failed");
                        count = c2_terrain_vertices(sim, p, c);
                        stream_unlock(&positions, count); stream_unlock(&colors, count);
                    }
                    perf.terrain += OSGetTime() - mark; mark = OSGetTime();
                    if (resident.active) {
                        resident.range_count = c2_scenery_resident_ranges(sim, scenery_ranges, 149*149, &object_count);
                        if (resident.range_count == UINT32_MAX) OSFatal("Scenery: resident range overflow");
                    } else {
                        p = stream_lock(&object_positions); c = stream_lock(&object_attrs);
                        if (!p || !c) OSFatal("Scenery probe: object buffer lock failed");
                        object_count = c2_scenery_vertices(sim, p, c, object_positions.elemCount);
                        scenery_uploaded = object_count;
                        stream_unlock(&object_positions, object_count); stream_unlock(&object_attrs, object_count);
                    }
                    perf.scenery += OSGetTime() - mark; mark = OSGetTime();
                }
                mark = OSGetTime();
                p = stream_lock(&actor_positions); c = stream_lock(&actor_attrs);
                if (!p || !c) OSFatal("Hunt probe: actor buffer lock failed");
                actor_count = c2_actor_vertices(sim, p, c, actor_positions.elemCount, 0);
                stream_unlock(&actor_positions, actor_count); stream_unlock(&actor_attrs, actor_count);
                uint64_t gun_start = OSGetTime();
                p = stream_lock(&weapon_positions); c = stream_lock(&weapon_attrs);
                if (!p || !c) OSFatal("Hunt probe: weapon buffer lock failed");
                weapon_count = c2_actor_vertices(sim, p, c, weapon_positions.elemCount, 1);
                stream_unlock(&weapon_positions, weapon_count); stream_unlock(&weapon_attrs, weapon_count);
                gun_ticks = OSGetTime() - gun_start;
                perf.actors += OSGetTime() - mark; perf.actor_v = actor_count; mark = OSGetTime();
                if (!static_reused) {
                    if (!terrain_resident.active) {
                        p = stream_lock(&water_pos); c = stream_lock(&water_attr);
                        if (!p || !c) OSFatal("Water probe: buffer lock failed");
                        water_count = c2_water_vertices(sim, p, c);
                        stream_unlock(&water_pos, water_count); stream_unlock(&water_attr, water_count);
                    }
                    perf.water += OSGetTime() - mark; mark = OSGetTime();
                    p = stream_lock(&sky_pos); c = stream_lock(&sky_attr);
                    if (!p || !c) OSFatal("Sky probe: buffer lock failed");
                    sky_count = c2_sky_vertices(sim, p, c);
                    float *f = stream_lock(&sky_fog); if (!f) OSFatal("Fog probe: buffer lock failed");
                    for (uint32_t i = 0; i < sky_count; ++i) {
                        f[i * 4] = fog_color[0]; f[i * 4 + 1] = fog_color[1];
                        f[i * 4 + 2] = fog_color[2]; f[i * 4 + 3] = -1.0f;
                    }
                    stream_unlock(&sky_fog, sky_count);
                    stream_unlock(&sky_pos, sky_count); stream_unlock(&sky_attr, sky_count);
                    perf.sky += OSGetTime() - mark;
                    static_view_save(&static_view, static_key, count, object_count, water_count, sky_count);
                }
            }
            mark = OSGetTime();
            perf.resident_enabled = resident_enabled; perf.resident_active = resident.active;
            perf.terrain_v = count; perf.scenery_v = object_count; perf.water_v = water_count; perf.actor_v = actor_count;
            float px, pz, dir_x, dir_z; c2_map_player(sim, &px, &pz, &dir_x, &dir_z);
            if (show_map[0] || show_map[1]) {
                // Restore the underlying map before painting the next mobile-style overlay.
                uint8_t *map_pixels = map_tex.surface.image;
                while (old_blip_count) { --old_blip_count; memcpy(map_pixels + old_blips[old_blip_count].offset, old_blips[old_blip_count].rgba, 4); }
                if (old_x >= 0) for (int dz = 0; dz < 151; ++dz) for (int dx = 0; dx < 151; ++dx)
                    memcpy(map_pixels + ((old_z + dz) * map_tex.surface.pitch + old_x + dx) * 4, saved + (dz * 151 + dx) * 4, 4);
                int mx = (int)px - 75, mz = (int)pz - 75;
                if (mx < 0) mx = 0; if (mx > 873) mx = 873;
                if (mz < 0) mz = 0; if (mz > 873) mz = 873;
                for (int dz = 0; dz < 151; ++dz) for (int dx = 0; dx < 151; ++dx) {
                    uint8_t *pixel = map_pixels + ((mz + dz) * map_tex.surface.pitch + mx + dx) * 4;
                    memcpy(saved + (dz * 151 + dx) * 4, pixel, 4);
                    float ax = (mx + dx) - px, az = (mz + dz) - pz;
                    float forward = ax * dir_x + az * dir_z;
                    float side = ax * -dir_z + az * dir_x;
                    float radius2 = ax * ax + az * az;
                    // The diagnostic terrain window is 74 cells in each direction.
                    // Tint a 90-degree forward sector, as on the 3DS mobile map.
                    if (radius2 <= 5476.0f && forward >= 0 && side >= -forward && side <= forward) {
                        pixel[0] = (uint8_t)((pixel[0] * 55 + 255 * 45) / 100);
                        pixel[1] = (uint8_t)((pixel[1] * 55 + 217 * 45) / 100);
                        pixel[2] = (uint8_t)((pixel[2] * 55 + 26 * 45) / 100);
                    }
                    if (radius2 >= 5402.25f && radius2 <= 5550.25f) {
                        pixel[0] = 255; pixel[1] = 217; pixel[2] = 26;
                    }
                }
                RadarBlip blips[256]; uint32_t blip_count = c2_radar_blips(sim, blips, 256);
                for (uint32_t i = 0; i < blip_count; ++i) {
                    int x = (int)blips[i].x, z = (int)blips[i].z;
                    for (int dz = -1; dz <= 1; ++dz) for (int dx = -1; dx <= 1; ++dx) {
                        int bx = x + dx, bz = z + dz;
                        if (bx < 0 || bx >= 1024 || bz < 0 || bz >= 1024) continue;
                        uint32_t offset = (bz * map_tex.surface.pitch + bx) * 4;
                        old_blips[old_blip_count].offset = offset;
                        memcpy(old_blips[old_blip_count++].rgba, map_pixels + offset, 4);
                        map_pixels[offset] = blips[i].rgba >> 24; map_pixels[offset + 1] = blips[i].rgba >> 16;
                        map_pixels[offset + 2] = blips[i].rgba >> 8; map_pixels[offset + 3] = blips[i].rgba;
                    }
                }
                old_x = mx; old_z = mz;
                uint64_t cache_start = OSGetTime();
                GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, map_tex.surface.image, map_tex.surface.imageSize);
                map_cache_ticks = OSGetTime() - cache_start;
            }
            // The preceding frame wait protects these CPU writes. Both streams
            // are prepared before either display can submit map draws.
            for (int display = 0; display < 2; ++display) if (show_map[display]) {
                float attrs[24]; map_view_uv(px, pz, display, attrs);
                float *ma = GX2RLockBufferEx(&map_attr[display], 0);
                if (!ma) OSFatal("Map probe: UV lock failed");
                memcpy(ma, attrs, sizeof(attrs));
                GX2RUnlockBufferEx(&map_attr[display], 0);
            }
            perf.map += OSGetTime() - mark;
            draw_start = OSGetTime();
            WHBGfxBeginRender();
            for (int display = 0; display < 2; ++display) {
                if (display == 0) WHBGfxBeginRenderTV(); else WHBGfxBeginRenderDRC();
                if (show_map[display]) WHBGfxClearColor(0.06f, 0.16f, 0.09f, 1.0f);
                else WHBGfxClearColor(fog_color[0], fog_color[1], fog_color[2], 1.0f);
                if (show_map[display]) {
                    GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                    GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                    GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                    GX2SetPixelTexture(&map_tex, sampler_location); GX2SetPixelSampler(&map_sampler, sampler_location);
                    GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                    GX2RSetAttributeBuffer(&map_pos, 0, map_pos.elemSize, 0); GX2RSetAttributeBuffer(&map_attr[display], 1, map_attr[display].elemSize, 0); GX2RSetAttributeBuffer(&map_fog, 2, map_fog.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                    if (display == 1) {
                        GX2SetPixelTexture(&icons_tex, sampler_location);
                        GX2RSetAttributeBuffer(&gun_icon_pos, 0, gun_icon_pos.elemSize, 0); GX2RSetAttributeBuffer(&gun_icon_attr, 1, gun_icon_attr.elemSize, 0);
                        GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 36, 0, 1);
                        GX2RSetAttributeBuffer(&call_icon_pos, 0, call_icon_pos.elemSize, 0); GX2RSetAttributeBuffer(&call_icon_attr, 1, call_icon_attr.elemSize, 0);
                        GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                        GX2SetPixelTexture(&hud_tex, sampler_location);
                        GX2RSetAttributeBuffer(&panel_ammo_pos, 0, panel_ammo_pos.elemSize, 0); GX2RSetAttributeBuffer(&panel_ammo_attr, 1, panel_ammo_attr.elemSize, 0);
                        GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                    }
                    if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
                    continue;
                }
                GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                GX2SetPixelTexture(&sky_tex, sampler_location); GX2SetPixelSampler(&sky_sampler, sampler_location);
                GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                GX2RSetAttributeBuffer(&sky_pos, 0, sky_pos.elemSize, 0); GX2RSetAttributeBuffer(&sky_attr, 1, sky_attr.elemSize, 0); GX2RSetAttributeBuffer(&sky_fog, 2, sky_fog.elemSize, 0);
                GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, sky_count, 0, 1);
                GX2SetDepthOnlyControl(TRUE, TRUE, GX2_COMPARE_FUNC_LESS);
                GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
                GX2SetShaderMode(GX2_SHADER_MODE_UNIFORM_REGISTER);
                GX2SetPixelTexture(tiled_atlases ? &tiled_atlas : &atlas, sampler_location); GX2SetPixelSampler(&sampler, sampler_location);
                GX2SetFetchShader(&shader.fetchShader); GX2SetVertexShader(shader.vertexShader); GX2SetPixelShader(shader.pixelShader);
                GX2RSetAttributeBuffer(&positions, 0, positions.elemSize, 0); GX2RSetAttributeBuffer(&colors, 1, colors.elemSize, 0); GX2RSetAttributeBuffer(&terrain_fog, 2, terrain_fog.elemSize, 0);
                if (terrain_resident.active) {
                    float camera[8]; c2_scenery_camera(sim, camera);
                    GX2SetVertexUniformReg(eye_uniform->offset, 4, camera);
                    GX2SetVertexUniformReg(angles_uniform->offset, 4, camera + 4);
                    for (uint32_t range = 0; range < terrain_resident.terrain_ranges; ++range)
                        GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, terrain_ranges[range].count, terrain_ranges[range].first, 1);
                } else GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, count, 0, 1);
                if (object_count) {
                    float camera[8]; c2_scenery_camera(sim, camera);
                    GX2SetVertexUniformReg(eye_uniform->offset, 4, camera);
                    GX2SetVertexUniformReg(angles_uniform->offset, 4, camera + 4);
                    GX2SetPixelTexture(tiled_atlases ? &tiled_object_atlas : &object_atlas, sampler_location);
                    GX2RSetAttributeBuffer(&object_positions, 0, object_positions.elemSize, 0);
                    GX2RSetAttributeBuffer(&object_attrs, 1, object_attrs.elemSize, 0);
                    GX2RSetAttributeBuffer(&object_fog, 2, object_fog.elemSize, 0);
                    if (resident.active) {
                        for (uint32_t range = 0; range < resident.range_count; ++range)
                            GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, scenery_ranges[range].count, scenery_ranges[range].first, 1);
                    } else GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, object_count, 0, 1);
                }
                if (actor_count) {
                    float camera[8]; c2_scenery_camera(sim, camera);
                    GX2SetVertexUniformReg(eye_uniform->offset, 4, camera);
                    GX2SetVertexUniformReg(angles_uniform->offset, 4, camera + 4);
                    GX2SetPixelTexture(tiled_atlases ? &tiled_actor_atlas : &actor_atlas, sampler_location);
                    GX2RSetAttributeBuffer(&actor_positions, 0, actor_positions.elemSize, 0);
                    GX2RSetAttributeBuffer(&actor_attrs, 1, actor_attrs.elemSize, 0);
                    GX2RSetAttributeBuffer(&actor_fog, 2, actor_fog.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, actor_count, 0, 1);
                }
                if (water_count) {
                    GX2SetPixelTexture(tiled_atlases ? &tiled_atlas : &atlas, sampler_location);
                    GX2RSetAttributeBuffer(&water_pos, 0, water_pos.elemSize, 0);
                    GX2RSetAttributeBuffer(&water_attr, 1, water_attr.elemSize, 0);
                    GX2RSetAttributeBuffer(&water_fog, 2, water_fog.elemSize, 0);
                    if (terrain_resident.active) {
                        float camera[8]; c2_scenery_camera(sim, camera);
                        GX2SetVertexUniformReg(eye_uniform->offset, 4, camera);
                        GX2SetVertexUniformReg(angles_uniform->offset, 4, camera + 4);
                        for (uint32_t range = 0; range < terrain_resident.water_ranges; ++range)
                            GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, water_ranges[range].count, water_ranges[range].first, 1);
                    } else GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, water_count, 0, 1);
                }
                if (weapon_count) {
                    GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                    GX2SetPixelTexture(tiled_atlases ? &tiled_actor_atlas : &actor_atlas, sampler_location);
                    GX2RSetAttributeBuffer(&weapon_positions, 0, weapon_positions.elemSize, 0);
                    GX2RSetAttributeBuffer(&weapon_attrs, 1, weapon_attrs.elemSize, 0);
                    GX2RSetAttributeBuffer(&weapon_fog, 2, weapon_fog.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, weapon_count, 0, 1);
                }
                GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                GX2SetPixelTexture(&hud_tex, sampler_location); GX2SetPixelSampler(&map_sampler, sampler_location);
                GX2RSetAttributeBuffer(&map_fog, 2, map_fog.elemSize, 0);
                if (display == 1 || !gamepad_connected) {
                    GX2RSetAttributeBuffer(&hud_pos, 0, hud_pos.elemSize, 0); GX2RSetAttributeBuffer(&hud_attr, 1, hud_attr.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                }
                if (display == 0) {
                    GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
                    GX2SetPixelTexture(&perf_tex, sampler_location); GX2SetPixelSampler(&sampler, sampler_location);
                    GX2RSetAttributeBuffer(&perf_pos, 0, perf_pos.elemSize, 0); GX2RSetAttributeBuffer(&perf_attr, 1, perf_attr.elemSize, 0); GX2RSetAttributeBuffer(&map_fog, 2, map_fog.elemSize, 0);
                    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 6, 0, 1);
                }
                if (display == 0) WHBGfxFinishRenderTV(); else WHBGfxFinishRenderDRC();
            }
            mark = OSGetTime();
            perf.draw += (uint64_t)(mark - draw_start) - draw_wait;
            WHBGfxFinishRender();
            perf.present += OSGetTime() - mark;
            perf.wait += frame_wait + draw_wait; perf.frame += OSGetTime() - frame_start; perf.frames++;
            performance_frame(&perf, &before, frame_start, hunt_seed, full_map, gamepad_game,
                sim_ticks, gun_ticks, map_cache_ticks, overlay_ticks, frame_wait, draw_wait,
                count, object_count, actor_count, weapon_count, water_count, sky_count, death_active, tiled_atlases, static_cache_enabled, static_reused, resident_enabled, resident.active && world_visible,
                world_visible ? (resident.active ? resident.range_count : (object_count ? 1u : 0u)) : 0u, scenery_uploaded);
        }
    }
    lifecycle_checkpoint("cleanup: profiler close begin");
    performance_close();
    lifecycle_checkpoint("cleanup: profiler close complete; audio stop begin");
    lifecycle_sim = NULL; lifecycle_suspended_audio = 0;
    audio_shutdown();
    lifecycle_checkpoint("cleanup: audio stopped; geometry free begin");
    // Last rendered frame already completed in WHBGfxFinishRender. Avoid a
    // redundant foreground-only wait after ProcUI has released the display.
    GX2RDestroyBufferEx(&positions, 0); GX2RDestroyBufferEx(&colors, 0);
    GX2RDestroyBufferEx(&object_positions, 0); GX2RDestroyBufferEx(&object_attrs, 0);
    GX2RDestroyBufferEx(&actor_positions, 0); GX2RDestroyBufferEx(&actor_attrs, 0); GX2RDestroyBufferEx(&actor_fog, 0);
    GX2RDestroyBufferEx(&weapon_positions, 0); GX2RDestroyBufferEx(&weapon_attrs, 0); GX2RDestroyBufferEx(&weapon_fog, 0); free(actor_atlas.surface.image);
    GX2RDestroyBufferEx(&terrain_fog, 0); GX2RDestroyBufferEx(&object_fog, 0); GX2RDestroyBufferEx(&water_fog, 0); GX2RDestroyBufferEx(&sky_fog, 0); GX2RDestroyBufferEx(&map_fog, 0);
    GX2RDestroyBufferEx(&sky_pos, 0); GX2RDestroyBufferEx(&sky_attr, 0); free(sky_tex.surface.image);
    GX2RDestroyBufferEx(&water_pos, 0); GX2RDestroyBufferEx(&water_attr, 0);
    GX2RDestroyBufferEx(&perf_pos, 0); GX2RDestroyBufferEx(&perf_attr, 0); free(perf_tex.surface.image);
    GX2RDestroyBufferEx(&hud_pos, 0); GX2RDestroyBufferEx(&hud_attr, 0); free(hud_tex.surface.image);
    GX2RDestroyBufferEx(&panel_ammo_pos, 0); GX2RDestroyBufferEx(&panel_ammo_attr, 0);
    GX2RDestroyBufferEx(&gun_icon_pos, 0); GX2RDestroyBufferEx(&gun_icon_attr, 0); GX2RDestroyBufferEx(&call_icon_pos, 0); GX2RDestroyBufferEx(&call_icon_attr, 0); free(icons_tex.surface.image);
    GX2RDestroyBufferEx(&map_pos, 0); GX2RDestroyBufferEx(&map_attr[0], 0); GX2RDestroyBufferEx(&map_attr[1], 0); free(map_tex.surface.image);
    free(tiled_atlas.surface.image); free(tiled_object_atlas.surface.image); free(tiled_actor_atlas.surface.image);
    free(object_atlas.surface.image); free(atlas.surface.image); WHBGfxFreeShaderGroup(&shader);
    lifecycle_checkpoint("cleanup: geometry/textures freed; graphics shutdown begin");
    WHBGfxShutdown();
    lifecycle_checkpoint("cleanup: graphics shutdown complete; controllers/simulation free begin");
    if (sim) KPADShutdown();
    c2_move_destroy(sim);
    lifecycle_checkpoint("cleanup: simulation freed; closing trace and releasing SD next");
    lifecycle_trace_close();
    if (added) FSDelClient(&client, FS_ERROR_FLAG_NONE);
    if (mounted) WHBUnmountSdCard();
    lifecycle_checkpoint("cleanup: SD released; session complete");
    return session_result;
}

int main(void) {
    WHBProcInit();
    // WHB registers the HBL HOME-denied handler; keep our deferred-shutdown
    // loop informed as well, without polling WHBProcIsRunning again.
    ProcUIRegisterCallback(PROCUI_CALLBACK_HOME_BUTTON_DENIED, lifecycle_home_denied, NULL, 100);
    MenuState selection = {0}; selection.animal = 1; selection.weapon = 1; selection.radar = 1;
    while (app_running()) {
        if (run_session(&selection) != HUNT_MAIN_MENU) break;
    }
    lifecycle_checkpoint("main: resource cleanup complete; ProcUI shutdown begin");
    // Graphics/audio have already been dismantled. Do not leave callbacks that
    // can touch their released resources during final ProcUI shutdown.
    ProcUIClearCallbacks();
    ProcUIShutdown();
    OSReport("C2 lifecycle: ProcUI shutdown complete; returning from main\n");
    WHBProcShutdown();
    return 0;
}
