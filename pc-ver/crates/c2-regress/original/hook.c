/* c2hook.dll: loaded into the original game (carn2.exe, IceAge.exe) by
 * c2launch.exe, for regression runs (docs/REGRESSION.md, "Original
 * engines"). Nothing on disk is changed: only the game's own import table
 * entries are pointed here, in memory.
 *
 * - timeGetTime (WINMM): a virtual clock. It starts at a fixed value (the
 *   game seeds rand() from it) and advances by C2H_FRAME_MS per frame.
 * - PeekMessageA (USER32): counts frames (a call that finds the queue
 *   empty ends the message loop of one frame) and runs the script.
 * - Keyboard and mouse (USER32): the script's virtual devices (script.c).
 * - CreateWindowExA: remembers the game's window, for posted messages.
 * - MessageBoxA: logs the text and ends the run (exit code 3) instead of
 *   waiting for a click (the game's error boxes: "ABNORMAL_HALT").
 * - CreateProcessA (KERNEL32): the menu runs the hunt as a second copy of
 *   the game and waits for it. The child is started suspended, this DLL is
 *   loaded into it too, and it runs C2H_HUNT_SCRIPT (log: C2H_LOG with
 *   "-hunt" before the extension). The command line is logged.
 * - HeapCreate (KERNEL32): the game makes one fixed-size heap of at most
 *   64 MB (Game.cpp: HeapCreate(0, 60000000, 64000000)); Ice Age's data
 *   does not fit Wine's fixed-size heaps, so it is made growable.
 *   Memory reservation only; the game's logic is unchanged.
 *
 * Settings come from the environment (inherited from c2launch.exe):
 *   C2H_SCRIPT    input script (script.c); none: no input
 *   C2H_HUNT_SCRIPT  the hunt process's script
 *   C2H_FRAME_MS  ms per frame (default 20)
 *   C2H_EXIT      quit at this frame at the latest (default 60000)
 *   C2H_LOG       log file (default c2hook.log)
 */
#include "hook.h"

#define CLOCK_START 0x00100000u

HWND game_window = NULL;
DWORD frame = 0;
static DWORD vclock = CLOCK_START;
static DWORD frame_ms = 20, exit_frame = 60000, calls_this_frame = 0;
static HANDLE logf = INVALID_HANDLE_VALUE;
static BOOL(WINAPI *real_peek)(LPMSG, HWND, UINT, UINT, UINT);
static HANDLE(WINAPI *real_heap_create)(DWORD, SIZE_T, SIZE_T);
static HINSTANCE self_module;
static BOOL(WINAPI *real_create_process)(LPCSTR, LPSTR, LPSECURITY_ATTRIBUTES,
                                         LPSECURITY_ATTRIBUTES, BOOL, DWORD, LPVOID, LPCSTR,
                                         LPSTARTUPINFOA, LPPROCESS_INFORMATION);
static HWND(WINAPI *real_create_window)(DWORD, LPCSTR, LPCSTR, DWORD, int, int, int, int, HWND,
                                         HMENU, HINSTANCE, LPVOID);

void *__cdecl memset(void *d, int c, size_t n) {
    unsigned char *p = d;
    while (n--) *p++ = (unsigned char)c;
    return d;
}

void logs(const char *s) {
    DWORD n;
    if (logf != INVALID_HANDLE_VALUE) WriteFile(logf, s, lstrlenA(s), &n, NULL);
}

void quit(UINT code) {
    if (logf != INVALID_HANDLE_VALUE) CloseHandle(logf);
    logf = INVALID_HANDLE_VALUE;
    ExitProcess(code);
}

static DWORD env_num(const char *name, DWORD def) {
    char b[32];
    DWORD v = 0, i;
    if (!GetEnvironmentVariableA(name, b, sizeof b)) return def;
    for (i = 0; b[i] >= '0' && b[i] <= '9'; i++) v = v * 10 + (DWORD)(b[i] - '0');
    return i ? v : def;
}

/* The game window's client area as a 24-bit BMP (the whole screen if
 * there is no window yet). */
void save_screen(const char *path) {
    int w = GetSystemMetrics(SM_CXSCREEN), h = GetSystemMetrics(SM_CYSCREEN);
    POINT at = {0, 0};
    RECT rc;
    HDC screen = GetDC(NULL), mem = CreateCompatibleDC(screen);
    if (game_window && GetClientRect(game_window, &rc) && rc.right > 0 && rc.bottom > 0) {
        ClientToScreen(game_window, &at);
        w = rc.right;
        h = rc.bottom;
    }
    BITMAPINFO bi;
    void *bits = NULL;
    HBITMAP bmp;
    memset(&bi, 0, sizeof bi);
    bi.bmiHeader.biSize = sizeof bi.bmiHeader;
    bi.bmiHeader.biWidth = w;
    bi.bmiHeader.biHeight = h;
    bi.bmiHeader.biPlanes = 1;
    bi.bmiHeader.biBitCount = 24;
    bmp = CreateDIBSection(mem, &bi, DIB_RGB_COLORS, &bits, NULL, 0);
    if (bmp && bits) {
        DWORD row = ((DWORD)w * 3 + 3) & ~3u, size = row * (DWORD)h, n;
        BITMAPFILEHEADER fh;
        HANDLE f;
        HGDIOBJ old = SelectObject(mem, bmp);
        BitBlt(mem, 0, 0, w, h, screen, at.x, at.y, SRCCOPY);
        GdiFlush();
        memset(&fh, 0, sizeof fh);
        fh.bfType = 0x4d42;
        fh.bfOffBits = sizeof fh + sizeof bi.bmiHeader;
        fh.bfSize = fh.bfOffBits + size;
        f = CreateFileA(path, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, 0, NULL);
        if (f != INVALID_HANDLE_VALUE) {
            WriteFile(f, &fh, sizeof fh, &n, NULL);
            WriteFile(f, &bi.bmiHeader, sizeof bi.bmiHeader, &n, NULL);
            WriteFile(f, bits, size, &n, NULL);
            CloseHandle(f);
        }
        SelectObject(mem, old);
        DeleteObject(bmp);
    }
    DeleteDC(mem);
    ReleaseDC(NULL, screen);
}

static DWORD WINAPI fake_time(void) {
    /* A busy wait (Interface.cpp: `while (t > timeGetTime())`) calls
     * without ending a frame: let time pass for it. */
    if (++calls_this_frame > 4096) vclock++;
    return vclock;
}

static void end_frame(void) {
    char b[96];
    frame++;
    vclock += frame_ms;
    calls_this_frame = 0;
    if (frame % 500 == 0) {
        wsprintfA(b, "frame %lu clock %lu\r\n", frame, vclock - CLOCK_START);
        logs(b);
    }
    script_step();
    if (frame >= exit_frame) {
        logs("frame ceiling reached: exit\r\n");
        quit(4);
    }
}

static BOOL WINAPI hook_peek(LPMSG m, HWND w, UINT a, UINT b, UINT r) {
    BOOL got = real_peek(m, w, a, b, r);
    if (!got) end_frame();
    return got;
}

static HANDLE WINAPI hook_heap_create(DWORD flags, SIZE_T initial, SIZE_T max) {
    (void)max;
    return real_heap_create(flags, initial, 0);
}

static HWND WINAPI hook_create_window(DWORD ex, LPCSTR cls, LPCSTR name, DWORD style, int x, int y,
                                      int w, int h, HWND parent, HMENU menu, HINSTANCE inst,
                                      LPVOID param) {
    HWND r = real_create_window(ex, cls, name, style, x, y, w, h, parent, menu, inst, param);
    if (!game_window && !parent) game_window = r;
    return r;
}

static int WINAPI hook_message_box(HWND w, LPCSTR text, LPCSTR caption, UINT type) {
    char b[400];
    (void)w;
    (void)type;
    wsprintfA(b, "frame %lu message box \"%.120s\": %.240s\r\n", frame, caption ? caption : "",
              text ? text : "");
    logs(b);
    quit(3);
    return IDOK;
}

/* Loads this DLL into a suspended process (as c2launch.exe does). */
static BOOL inject(HANDLE process) {
    char dll[MAX_PATH];
    void *remote, *load;
    HANDLE t;
    DWORD code = 0;
    GetModuleFileNameA(self_module, dll, MAX_PATH);
    remote = VirtualAllocEx(process, NULL, MAX_PATH, MEM_COMMIT, PAGE_READWRITE);
    load = (void *)GetProcAddress(GetModuleHandleA("kernel32.dll"), "LoadLibraryA");
    if (!remote || !load) return FALSE;
    WriteProcessMemory(process, remote, dll, (SIZE_T)lstrlenA(dll) + 1, NULL);
    t = CreateRemoteThread(process, NULL, 0, (LPTHREAD_START_ROUTINE)load, remote, 0, NULL);
    if (!t) return FALSE;
    WaitForSingleObject(t, INFINITE);
    GetExitCodeThread(t, &code);
    CloseHandle(t);
    return code != 0;
}

static BOOL WINAPI hook_create_process(LPCSTR app, LPSTR cmd, LPSECURITY_ATTRIBUTES pa,
                                       LPSECURITY_ATTRIBUTES ta, BOOL inherit, DWORD flags,
                                       LPVOID env, LPCSTR dir, LPSTARTUPINFOA si,
                                       LPPROCESS_INFORMATION pi) {
    char b[600];
    BOOL ok;
    SetEnvironmentVariableA("C2H_CHILD", "1");
    ok = real_create_process(app, cmd, pa, ta, inherit, flags | CREATE_SUSPENDED, env, dir, si, pi);
    SetEnvironmentVariableA("C2H_CHILD", NULL);
    wsprintfA(b, "frame %lu create process \"%.200s\" \"%.300s\" %s\r\n", frame, app ? app : "",
              cmd ? cmd : "", ok ? "started" : "FAILED");
    logs(b);
    if (!ok) return FALSE;
    logs(inject(pi->hProcess) ? "hunt process hooked\r\n" : "hunt process NOT hooked\r\n");
    if (!(flags & CREATE_SUSPENDED)) ResumeThread(pi->hThread);
    return TRUE;
}

/* Points `module`'s import of dll!name at `to`; returns the old target. */
static void *patch_import(HMODULE module, const char *dll, const char *name, void *to) {
    BYTE *base = (BYTE *)module;
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + ((IMAGE_DOS_HEADER *)base)->e_lfanew);
    IMAGE_DATA_DIRECTORY d = nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT];
    IMAGE_IMPORT_DESCRIPTOR *imp = (IMAGE_IMPORT_DESCRIPTOR *)(base + d.VirtualAddress);
    for (; d.VirtualAddress && imp->Name; imp++) {
        IMAGE_THUNK_DATA *names, *addrs;
        if (lstrcmpiA((char *)(base + imp->Name), dll)) continue;
        names = (IMAGE_THUNK_DATA *)(base + (imp->OriginalFirstThunk ? imp->OriginalFirstThunk
                                                                     : imp->FirstThunk));
        addrs = (IMAGE_THUNK_DATA *)(base + imp->FirstThunk);
        for (; names->u1.AddressOfData; names++, addrs++) {
            IMAGE_IMPORT_BY_NAME *ibn;
            DWORD old;
            void *prev;
            if (IMAGE_SNAP_BY_ORDINAL(names->u1.Ordinal)) continue;
            ibn = (IMAGE_IMPORT_BY_NAME *)(base + names->u1.AddressOfData);
            if (lstrcmpA((char *)ibn->Name, name)) continue;
            VirtualProtect(&addrs->u1.Function, sizeof(void *), PAGE_READWRITE, &old);
            prev = (void *)addrs->u1.Function;
            addrs->u1.Function = (ULONG_PTR)to;
            VirtualProtect(&addrs->u1.Function, sizeof(void *), old, &old);
            return prev;
        }
    }
    return NULL;
}

static void *hook(HMODULE exe, const char *dll, const char *name, void *to) {
    char b[96];
    void *prev = patch_import(exe, dll, name, to);
    wsprintfA(b, "%s %s\r\n", name, prev ? "hooked" : "NOT FOUND");
    logs(b);
    return prev;
}

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID unused) {
    char path[MAX_PATH], b[MAX_PATH + 32], flag[4];
    HMODULE exe;
    BOOL child;
    (void)unused;
    if (reason != DLL_PROCESS_ATTACH) return TRUE;
    self_module = h;
    child = GetEnvironmentVariableA("C2H_CHILD", flag, sizeof flag) > 0;
    if (!GetEnvironmentVariableA("C2H_LOG", path, sizeof path)) lstrcpyA(path, "c2hook.log");
    if (child) {
        int n = lstrlenA(path), dot = n;
        while (dot > 0 && path[dot - 1] != '.' && path[dot - 1] != '\\') dot--;
        if (dot > 0 && path[dot - 1] == '.') {
            char ext[16];
            lstrcpynA(ext, path + dot - 1, sizeof ext);
            lstrcpyA(path + dot - 1, "-hunt");
            lstrcatA(path, ext);
        } else {
            lstrcatA(path, "-hunt");
        }
    }
    logf = CreateFileA(path, GENERIC_WRITE, FILE_SHARE_READ, NULL, CREATE_ALWAYS, 0, NULL);
    frame_ms = env_num("C2H_FRAME_MS", 20);
    exit_frame = env_num("C2H_EXIT", 60000);
    if (GetEnvironmentVariableA(child ? "C2H_HUNT_SCRIPT" : "C2H_SCRIPT", path, sizeof path)) {
        wsprintfA(b, "script %s %s\r\n", path, script_load(path) ? "loaded" : "NOT READABLE");
        logs(b);
    }
    exe = GetModuleHandleA(NULL);
    hook(exe, "WINMM.dll", "timeGetTime", (void *)fake_time);
    real_peek = hook(exe, "USER32.dll", "PeekMessageA", (void *)hook_peek);
    real_create_window = hook(exe, "USER32.dll", "CreateWindowExA", (void *)hook_create_window);
    hook(exe, "USER32.dll", "MessageBoxA", (void *)hook_message_box);
    hook(exe, "USER32.dll", "GetKeyboardState", (void *)hook_get_keyboard_state);
    hook(exe, "USER32.dll", "GetAsyncKeyState", (void *)hook_get_async_key_state);
    hook(exe, "USER32.dll", "GetKeyState", (void *)hook_get_key_state);
    hook(exe, "USER32.dll", "GetCursorPos", (void *)hook_get_cursor_pos);
    hook(exe, "USER32.dll", "SetCursorPos", (void *)hook_set_cursor_pos);
    real_heap_create = hook(exe, "KERNEL32.dll", "HeapCreate", (void *)hook_heap_create);
    real_create_process = hook(exe, "KERNEL32.dll", "CreateProcessA", (void *)hook_create_process);
    return TRUE;
}
