/* Kill runs in the original (docs/REGRESSION.md, "Kill animations in the
 * original"): an animal is set on the hunter, his death is noticed, and the
 * body and the killer are logged every frame after it.
 *
 * The game's globals are found by c2-regress in the hunt executable's own
 * code (src/original/addrs.rs) and given here by the script's `addr` lines.
 * Field offsets are TCharacter's (Hunt.h), which both shipped renderers
 * use (156 bytes each). Every address is checked against the executable's
 * image before it is read or written.
 *
 * Log lines (parsed by src/original/kills.rs):
 *   frame F release AI A index I of N            the animal set down
 *   frame F release AI A not found (N characters)
 *   frame F death body B killer I count N        MyHealth reached 0
 *   frame F pose B PHASE FTIME X Z SCALE K PHASE FTIME X Z SCALE
 */
#include "hook.h"

enum {
    CH_SIZE = 156,
    CH_AI = 4,
    CH_STATE = 16,
    CH_NOFIND = 24,
    CH_AFRAID = 28,
    CH_TGTIME = 32,
    CH_PHASE = 48,
    CH_FTIME = 52,
    CH_SCALE = 68,
    CH_TGX = 84,
    CH_TGZ = 88,
    CH_POSX = 92,
    CH_POSY = 96,
    CH_POSZ = 100,
    CH_HEALTH = 144,
};

/* MSVC's marker that floating point is used; no C runtime provides it. */
int _fltused = 0;

static DWORD a_health, a_chars, a_count, a_px, a_py, a_pz, a_cam;
static DWORD image_lo, image_hi;
static int released = -1;
static DWORD death = 0;
static int body = -1;

static float fsin(float x) {
    float r;
    __asm__("fsin" : "=t"(r) : "0"(x));
    return r;
}

static float fcos(float x) {
    float r;
    __asm__("fcos" : "=t"(r) : "0"(x));
    return r;
}

/* Rounded towards zero, without the C runtime's conversion helper. */
static int to_int(float x) {
    int r;
    short cw, trunc;
    __asm__ volatile("fnstcw %0" : "=m"(cw));
    trunc = cw | 0x0C00;
    __asm__ volatile("fldcw %0" ::"m"(trunc));
    __asm__ volatile("fistpl %0" : "=m"(r) : "t"(x) : "st");
    __asm__ volatile("fldcw %0" ::"m"(cw));
    return r;
}

static BOOL ok(DWORD a, DWORD n) {
    if (!image_hi) {
        BYTE *base = (BYTE *)GetModuleHandleA(NULL);
        IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + ((IMAGE_DOS_HEADER *)base)->e_lfanew);
        image_lo = (DWORD)base;
        image_hi = image_lo + nt->OptionalHeader.SizeOfImage;
    }
    return a >= image_lo && a + n <= image_hi;
}

static BOOL ready(void) {
    return a_health && a_chars && a_count && a_px && a_pz && a_cam && ok(a_health, 4) &&
           ok(a_count, 4) && ok(a_px, 4) && ok(a_py, 4) && ok(a_pz, 4) && ok(a_cam, 4) &&
           ok(a_chars, CH_SIZE);
}

static int *ch_int(int i, int off) { return (int *)(a_chars + (DWORD)i * CH_SIZE + off); }
static float *ch_float(int i, int off) { return (float *)(a_chars + (DWORD)i * CH_SIZE + off); }

static int count(void) {
    int n = *(int *)a_count;
    /* Characters[] has room for 256 in both games; never read past it. */
    if (n < 0) return 0;
    while (n > 0 && !ok(a_chars, (DWORD)n * CH_SIZE)) n--;
    return n;
}

void kill_addr(const char *name, DWORD v) {
    static const struct { const char *n; DWORD *a; } names[] = {
        {"health", &a_health}, {"characters", &a_chars}, {"count", &a_count},
        {"player_x", &a_px},   {"player_y", &a_py},      {"player_z", &a_pz},
        {"camera_alpha", &a_cam},
    };
    unsigned i;
    for (i = 0; i < sizeof names / sizeof names[0]; i++)
        if (!lstrcmpA(name, names[i].n)) *names[i].a = v;
}

/* The first living animal of `ai`, `dist` units ahead of the hunter,
 * hunting him (as our engine's kill script and `c2-pc --die`). */
void kill_release(int ai, int dist) {
    char m[128];
    int i, n;
    float a, px, pz;
    if (!ready()) {
        logs("release: addresses missing or outside the executable\r\n");
        return;
    }
    n = count();
    for (i = 0; i < n; i++)
        if (*ch_int(i, CH_AI) == ai && *ch_int(i, CH_HEALTH) > 0) break;
    if (i == n) {
        wsprintfA(m, "frame %lu release AI %d not found (%d characters)\r\n", frame, ai, n);
        logs(m);
        return;
    }
    a = *(float *)a_cam;
    px = *(float *)a_px;
    pz = *(float *)a_pz;
    *ch_float(i, CH_POSX) = px + fsin(a) * (float)dist;
    *ch_float(i, CH_POSZ) = pz - fcos(a) * (float)dist;
    *ch_float(i, CH_POSY) = *(float *)a_py;
    *ch_float(i, CH_TGX) = px;
    *ch_float(i, CH_TGZ) = pz;
    *ch_int(i, CH_TGTIME) = 0;
    *ch_int(i, CH_STATE) = 1;
    *ch_int(i, CH_AFRAID) = 20000;
    *ch_int(i, CH_NOFIND) = 0;
    released = i;
    wsprintfA(m, "frame %lu release AI %d index %d of %d\r\n", frame, ai, i, n);
    logs(m);
}

DWORD kill_death_frame(void) { return death; }

static void pose_text(char *out, int i) {
    wsprintfA(out, "%d %d %d %d %d %d", i, *ch_int(i, CH_PHASE), *ch_int(i, CH_FTIME),
              to_int(*ch_float(i, CH_POSX)), to_int(*ch_float(i, CH_POSZ)),
              to_int(*ch_float(i, CH_SCALE) * 1000.0f));
}

/* At the end of every frame, before the script's actions. */
void kill_step(void) {
    char m[200], b[80], k[80];
    int n;
    if (released < 0 || !ready()) return;
    n = count();
    if (!death) {
        if (*(int *)a_health != 0) return;
        death = frame;
        body = n - 1;
        wsprintfA(m, "frame %lu death body %d killer %d count %d\r\n", frame, body, released, n);
        logs(m);
    }
    if (body < 0 || body >= n || released >= n) return;
    pose_text(b, body);
    pose_text(k, released);
    wsprintfA(m, "frame %lu pose %s %s\r\n", frame, b, k);
    logs(m);
}
