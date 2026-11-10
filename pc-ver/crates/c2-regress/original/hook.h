/* Shared by hook.c (imports, clock, capture) and script.c (input script). */
#ifndef C2HOOK_H
#define C2HOOK_H
#include <windows.h>

/* hook.c */
extern HWND game_window;
extern DWORD frame;
void logs(const char *s);
void save_screen(const char *path);
void quit(UINT code);

/* script.c: a script line per action, by frame (see script.c). */
BOOL script_load(const char *path);
/* Runs the actions due at the end of `frame`. */
void script_step(void);
/* The virtual input the game reads instead of the real devices. */
UINT WINAPI hook_get_keyboard_state(PBYTE out);
SHORT WINAPI hook_get_async_key_state(int vk);
SHORT WINAPI hook_get_key_state(int vk);
BOOL WINAPI hook_get_cursor_pos(LPPOINT p);
BOOL WINAPI hook_set_cursor_pos(int x, int y);

/* kill.c: kill runs (globals from the script's `addr` lines), and the
 * game's characters and hunter for behave.c. */
void kill_addr(const char *name, DWORD v);
void kill_release(int ai, int dist);
void kill_step(void);
DWORD kill_death_frame(void);
BOOL game_ready(void);
int game_count(void);
int *ch_int(int i, int off);
float *ch_float(int i, int off);
float player_x(void);
float player_y(void);
int player_health(void);
float player_z(void);
void set_player(float x, float y, float z);
BOOL set_player_alpha(float a);
int to_int(float x);

/* TCharacter field offsets (Hunt.h; 156 bytes in both shipped renderers). */
enum {
    CH_SIZE = 156,
    CH_AI = 4,
    CH_STATE = 16,
    CH_NOFIND = 24,
    CH_AFRAID = 28,
    CH_TGTIME = 32,
    CH_PHASE = 48,
    CH_FTIME = 52,
    CH_VSPEED = 56,
    CH_SCALE = 68,
    CH_TGX = 84,
    CH_TGZ = 88,
    CH_POSX = 92,
    CH_POSY = 96,
    CH_POSZ = 100,
    CH_HEALTH = 144,
};

/* behave.c: behaviour runs (an animal alarmed or slain, a subject logged
 * every frame). */
void behave_place(int x, int y, int z, int yaw_mrad);
void behave_alarm(int ai, BOOL at, int x, int z);
void behave_slay(int ai, BOOL at, int x, int z);
void behave_track_hunter(void);
void behave_pacify(void);
void behave_untrack(void);
void behave_step(void);

/* texture.c: isolated captures through native animation and rasterization. */
void texture_run(const char *path);

void *__cdecl memset(void *d, int c, size_t n);
#endif
