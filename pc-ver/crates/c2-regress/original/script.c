/* The input script and the virtual keyboard and mouse the game reads.
 *
 * One action per line, `FRAME ACTION ARGS`; FRAME is absolute, `+N` for
 * N frames after the previous line, or `D+N` for N frames after the
 * hunter's death (kill.c; never run if he does not die). `#` starts a
 * comment.
 *   key VK down|up   hold or release a key (also posts WM_KEYDOWN/UP)
 *   tap VK           down now, up two frames later
 *   text STRING      WM_CHAR per character (names on the register screen)
 *   mouse X Y        move the cursor (screen pixels; posts WM_MOUSEMOVE)
 *   click X Y        move, left button down now and up two frames later
 *   look DX DY       mouse-look: the cursor DX, DY from where the game
 *                    last centred it, for one frame
 *   shot NAME        save the screen as NAME.bmp
 *   mark TEXT        a line in the log (stage and checkpoint names)
 *   exit             quit the game (exit code 0)
 *   addr NAME HEX    a game global for kill.c (health, characters, count,
 *                    player_x, player_y, player_z, camera_alpha)
 *   release AI DIST  kill.c: the first animal of AI set on the hunter
 * VK: a letter or digit, LBUTTON RBUTTON RETURN TAB ESCAPE SPACE SHIFT
 * CONTROL MENU BACK F1..F12 UP DOWN LEFT RIGHT, or a number.
 * The real keyboard and mouse are never read: runs repeat exactly.
 *
 * After a click the game waits for the button to come up in a loop that
 * never ends a frame (Interface.cpp: `while (GetAsyncKeyState(VK_LBUTTON)
 * & 0x80000000);`). A key polled RELEASE_POLLS times within one frame is
 * let go, as a player's finger would. */
#include "hook.h"

enum {
    A_KEY, A_TAP, A_TEXT, A_MOUSE, A_CLICK, A_LOOK, A_SHOT, A_MARK, A_EXIT, A_UP,
    A_ADDR, A_RELEASE
};

typedef struct {
    DWORD frame;
    int kind, a, b;
    char text[96];
} Action;

#define MAX_ACTIONS 2048
static Action acts[MAX_ACTIONS];
static int nacts = 0, next_act = 0;
/* `D+N` actions, by N. */
#define MAX_AFTER 64
static Action after[MAX_AFTER];
static int nafter = 0, next_after = 0;
static BOOL to_after = FALSE;
#define RELEASE_POLLS 256
static BYTE keys[256];
static WORD polls[256];
static POINT cursor = {400, 300}, centre = {400, 300};

static BOOL is_space(char c) { return c == ' ' || c == '\t'; }

static const char *skip(const char *s) {
    while (is_space(*s)) s++;
    return s;
}

/* The next word into `out` (up to `n` bytes); returns the rest. */
static const char *word(const char *s, char *out, int n) {
    int i = 0;
    s = skip(s);
    while (*s && !is_space(*s) && *s != '\r' && *s != '\n') {
        if (i < n - 1) out[i++] = *s;
        s++;
    }
    out[i] = 0;
    return s;
}

static int num(const char *w) {
    int v = 0, neg = (*w == '-');
    if (neg || *w == '+') w++;
    while (*w >= '0' && *w <= '9') v = v * 10 + (*w++ - '0');
    return neg ? -v : v;
}

static DWORD hex(const char *w) {
    DWORD v = 0;
    if (w[0] == '0' && (w[1] == 'x' || w[1] == 'X')) w += 2;
    for (;; w++) {
        int d = (*w >= '0' && *w <= '9')   ? *w - '0'
                : (*w >= 'a' && *w <= 'f') ? *w - 'a' + 10
                : (*w >= 'A' && *w <= 'F') ? *w - 'A' + 10
                                           : -1;
        if (d < 0) return v;
        v = v * 16 + (DWORD)d;
    }
}

static int vk(const char *w) {
    static const struct { const char *name; int vk; } names[] = {
        {"LBUTTON", VK_LBUTTON}, {"RBUTTON", VK_RBUTTON}, {"RETURN", VK_RETURN},
        {"TAB", VK_TAB}, {"ESCAPE", VK_ESCAPE}, {"SPACE", VK_SPACE},
        {"SHIFT", VK_SHIFT}, {"CONTROL", VK_CONTROL}, {"MENU", VK_MENU},
        {"BACK", VK_BACK}, {"UP", VK_UP}, {"DOWN", VK_DOWN}, {"LEFT", VK_LEFT},
        {"RIGHT", VK_RIGHT}, {"ADD", VK_ADD}, {"SUBTRACT", VK_SUBTRACT},
    };
    unsigned i;
    if (w[0] && !w[1]) return (w[0] >= 'a' && w[0] <= 'z') ? w[0] - 32 : w[0];
    if (w[0] == 'F' && w[1] >= '1' && w[1] <= '9') return VK_F1 + num(w + 1) - 1;
    for (i = 0; i < sizeof names / sizeof names[0]; i++)
        if (!lstrcmpiA(w, names[i].name)) return names[i].vk;
    return num(w);
}

static void add(DWORD f, int kind, int a, int b, const char *text) {
    Action *x;
    if (to_after) {
        if (nafter >= MAX_AFTER) return;
        x = &after[nafter++];
    } else {
        if (nacts >= MAX_ACTIONS) return;
        x = &acts[nacts++];
    }
    x->frame = f;
    x->kind = kind;
    x->a = a;
    x->b = b;
    x->text[0] = 0;
    if (text) lstrcpynA(x->text, text, sizeof x->text);
}

static void sort(Action *v, int n) {
    int i, j;
    for (i = 1; i < n; i++)
        for (j = i; j > 0 && v[j - 1].frame > v[j].frame; j--) {
            Action t = v[j];
            v[j] = v[j - 1];
            v[j - 1] = t;
        }
}

BOOL script_load(const char *path) {
    static char buf[65536];
    DWORD got = 0, f = 0;
    HANDLE h = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    const char *s;
    if (h == INVALID_HANDLE_VALUE) return FALSE;
    ReadFile(h, buf, sizeof buf - 1, &got, NULL);
    CloseHandle(h);
    buf[got] = 0;
    for (s = buf; *s;) {
        char w[96], a[32], b[32];
        const char *line = skip(s), *rest;
        while (*s && *s != '\n') s++;
        if (*s) s++;
        if (*line == '#' || *line == '\r' || *line == '\n' || !*line) continue;
        rest = word(line, w, sizeof w);
        to_after = (w[0] == 'D' && w[1] == '+');
        if (to_after)
            f = (DWORD)num(w + 2);
        else
            f = (w[0] == '+') ? f + (DWORD)num(w) : (DWORD)num(w);
        rest = word(rest, w, sizeof w);
        if (!lstrcmpA(w, "key")) {
            rest = word(word(rest, a, sizeof a), b, sizeof b);
            add(f, A_KEY, vk(a), !lstrcmpA(b, "down"), NULL);
        } else if (!lstrcmpA(w, "tap")) {
            word(rest, a, sizeof a);
            add(f, A_KEY, vk(a), 1, NULL);
            add(f + 2, A_KEY, vk(a), 0, NULL);
        } else if (!lstrcmpA(w, "click")) {
            rest = word(word(rest, a, sizeof a), b, sizeof b);
            add(f, A_CLICK, num(a), num(b), NULL);
            add(f + 2, A_UP, num(a), num(b), NULL);
        } else if (!lstrcmpA(w, "mouse") || !lstrcmpA(w, "look")) {
            int kind = w[0] == 'm' ? A_MOUSE : A_LOOK;
            rest = word(word(rest, a, sizeof a), b, sizeof b);
            add(f, kind, num(a), num(b), NULL);
        } else if (!lstrcmpA(w, "text") || !lstrcmpA(w, "shot") || !lstrcmpA(w, "mark")) {
            char t[96];
            int i = 0, kind = w[0] == 't' ? A_TEXT : w[0] == 's' ? A_SHOT : A_MARK;
            rest = skip(rest);
            while (*rest && *rest != '\r' && *rest != '\n' && i < 95) t[i++] = *rest++;
            t[i] = 0;
            add(f, kind, 0, 0, t);
        } else if (!lstrcmpA(w, "exit")) {
            add(f, A_EXIT, 0, 0, NULL);
        } else if (!lstrcmpA(w, "addr")) {
            rest = word(word(rest, a, sizeof a), b, sizeof b);
            add(f, A_ADDR, (int)hex(b), 0, a);
        } else if (!lstrcmpA(w, "release")) {
            rest = word(word(rest, a, sizeof a), b, sizeof b);
            add(f, A_RELEASE, num(a), num(b), NULL);
        } else {
            char m[160];
            wsprintfA(m, "script: unknown action \"%s\" at frame %lu\r\n", w, f);
            logs(m);
        }
    }
    /* Run in frame order (lines may be out of order); stable. */
    sort(acts, nacts);
    sort(after, nafter);
    return TRUE;
}

static LPARAM client_xy(int x, int y) {
    POINT p = {x, y};
    if (game_window) ScreenToClient(game_window, &p);
    return MAKELPARAM(p.x, p.y);
}

static void post(UINT msg, WPARAM w, LPARAM l) {
    if (game_window) PostMessageA(game_window, msg, w, l);
}

static void run(const Action *x) {
    char m[160];
    int i;
    switch (x->kind) {
    case A_KEY:
        keys[x->a & 255] = x->b ? 0x80 : 0;
        if (x->a != VK_LBUTTON && x->a != VK_RBUTTON) {
            LPARAM l = 1 | ((LPARAM)MapVirtualKeyA((UINT)x->a, 0) << 16);
            post(x->b ? WM_KEYDOWN : WM_KEYUP, (WPARAM)x->a, x->b ? l : l | 0xC0000000);
        }
        break;
    case A_TEXT:
        for (i = 0; x->text[i]; i++) post(WM_CHAR, (WPARAM)(BYTE)x->text[i], 1);
        break;
    case A_CLICK:
    case A_MOUSE:
    case A_UP:
        cursor.x = x->a;
        cursor.y = x->b;
        post(WM_MOUSEMOVE, x->kind == A_UP ? MK_LBUTTON : 0, client_xy(x->a, x->b));
        if (x->kind == A_CLICK) {
            keys[VK_LBUTTON] = 0x80;
            post(WM_LBUTTONDOWN, MK_LBUTTON, client_xy(x->a, x->b));
        } else if (x->kind == A_UP) {
            keys[VK_LBUTTON] = 0;
            post(WM_LBUTTONUP, 0, client_xy(x->a, x->b));
        }
        break;
    case A_LOOK:
        cursor.x = centre.x + x->a;
        cursor.y = centre.y + x->b;
        break;
    case A_SHOT:
        wsprintfA(m, "%s.bmp", x->text);
        save_screen(m);
        wsprintfA(m, "frame %lu shot %s.bmp\r\n", frame, x->text);
        logs(m);
        break;
    case A_MARK:
        wsprintfA(m, "frame %lu mark %s\r\n", frame, x->text);
        logs(m);
        break;
    case A_ADDR:
        kill_addr(x->text, (DWORD)x->a);
        break;
    case A_RELEASE:
        kill_release(x->a, x->b);
        break;
    case A_EXIT:
        wsprintfA(m, "frame %lu exit\r\n", frame);
        logs(m);
        quit(0);
        break;
    }
}

void script_step(void) {
    int i;
    DWORD d;
    for (i = 0; i < 256; i++) polls[i] = 0;
    while (next_act < nacts && acts[next_act].frame <= frame) run(&acts[next_act++]);
    kill_step();
    d = kill_death_frame();
    while (d && next_after < nafter && d + after[next_after].frame <= frame)
        run(&after[next_after++]);
}

UINT WINAPI hook_get_keyboard_state(PBYTE out) {
    int i;
    for (i = 0; i < 256; i++) out[i] = keys[i];
    return TRUE;
}

SHORT WINAPI hook_get_async_key_state(int k) {
    k &= 255;
    if ((keys[k] & 0x80) && ++polls[k] >= RELEASE_POLLS) {
        char m[64];
        keys[k] = 0;
        wsprintfA(m, "frame %lu released key %d (busy wait)\r\n", frame, k);
        logs(m);
    }
    return (keys[k] & 0x80) ? (SHORT)0x8000 : 0;
}

SHORT WINAPI hook_get_key_state(int k) { return (keys[k & 255] & 0x80) ? (SHORT)0xFF80 : 0; }

BOOL WINAPI hook_get_cursor_pos(LPPOINT p) {
    *p = cursor;
    return TRUE;
}

/* The game centres the cursor for mouse-look; the real one stays put.
 * A centre at (0,0) is ignored: a Modder's Edition fork centres there once
 * while its window has no size yet, then reads it back against its real
 * centre, a jump of half the screen that throws the view up and left. */
BOOL WINAPI hook_set_cursor_pos(int x, int y) {
    if (x == 0 && y == 0) {
        char m[64];
        wsprintfA(m, "frame %lu: SetCursorPos(0,0) ignored\r\n", frame);
        logs(m);
        return TRUE;
    }
    cursor.x = centre.x = x;
    cursor.y = centre.y = y;
    return TRUE;
}
