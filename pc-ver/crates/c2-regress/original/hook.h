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

void *__cdecl memset(void *d, int c, size_t n);
#endif
