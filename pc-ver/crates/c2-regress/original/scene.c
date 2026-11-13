/* Diagnostic scene/sky batches in the shipped Genesis software renderer.
 * The host validates instruction operands and supplies RVAs, never guessed
 * addresses. Calls happen after initialization, without simulation between
 * captures. The process exits afterwards; normal gameplay never resumes. */
#include "hook.h"

static HANDLE input;
static BYTE *image;
static DWORD size;
static void fail(const char *s) { logs("scene: ERROR "); logs(s); logs("\r\n"); quit(5); }
static void read_bytes(void *p, DWORD n) {
    DWORD got;
    if (!ReadFile(input, p, n, &got, NULL) || got != n) fail("truncated job");
}
static void *at(DWORD rva) {
    if (rva < 4096 || rva > size - 4) fail("RVA outside image");
    return image + rva;
}
static void write_bytes(HANDLE f, const void *p, DWORD n) {
    DWORD got;
    if (!WriteFile(f, p, n, &got, NULL) || got != n) fail("write failed");
}
static void pixels(const char *name, WORD *surface, int stride, int w, int h) {
    HANDLE f = CreateFileA(name, GENERIC_WRITE, 0, NULL, CREATE_NEW, 0, NULL);
    if (f == INVALID_HANDLE_VALUE) fail("capture already exists or cannot be written");
    write_bytes(f, &w, 4); write_bytes(f, &h, 4);
    for (int y = 0; y < h; y++) write_bytes(f, surface + y * stride, w * 2);
    CloseHandle(f);
}

void scene_run(const char *path) {
    DWORD base[18], p[14], count;
    char magic[8], name[64];
    image = (BYTE *)GetModuleHandleA(NULL);
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(image + ((IMAGE_DOS_HEADER *)image)->e_lfanew);
    size = nt->OptionalHeader.SizeOfImage;
    input = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    if (input == INVALID_HANDLE_VALUE) fail("cannot open job");
    read_bytes(magic, 8); read_bytes(base, sizeof base); read_bytes(p, sizeof p);
    if (magic[7] || lstrcmpA(magic, "C2SCN01") || base[0] != nt->FileHeader.TimeDateStamp || base[1] != size)
        fail("schema/executable mismatch");
    if (base[12] != 8192) fail("only validated dynamic Genesis renderer supported");
    void (__cdecl *draw)(void) = at(p[0]);
    void (__cdecl *sky)(void) = at(p[1]);
    if (*(BYTE *)draw != 0x55 || *(BYTE *)sky != 0x55) fail("entry mismatch");
    WORD *surface = *(WORD **)at(base[17]);
    int stride = *(int *)at(base[15]) / 2;
    int w = *(int *)at(base[8]) + 1, h = *(int *)at(base[9]) + 1;
    if (!surface || w != 800 || h != 600 || stride < w || stride > 4096)
        fail("unexpected native surface");
    /* Remove animals, not static scenery: the Rust area viewer has no herd. */
    *(int *)at(p[13]) = 0;
    read_bytes(&count, 4);
    if (!count || count > 100) fail("invalid batch size");
    for (DWORD i = 0; i < count; i++) {
        float cam[5]; DWORD clock, radius;
        read_bytes(cam, sizeof cam); read_bytes(&clock, 4); read_bytes(&radius, 4);
        if (radius < 16 || radius > 128) fail("unsupported radius");
        for (int k = 0; k < 5; k++) *(float *)at(p[2+k]) = cam[k];
        *(DWORD *)at(p[9]) = clock;
        *(DWORD *)at(p[10]) = radius;
        *(float *)at(base[4]) = 500.0f; *(float *)at(base[5]) = 500.0f;
        draw();
        wsprintfA(name, "scene-%03lu.555", i);
        pixels(name, surface, stride, w, h);
        /* DrawScene has initialized yaw sin/cos and the visible ground min.
         * RenderSkyPlane clears the native surface before drawing the sky. */
        sky();
        wsprintfA(name, "sky-%03lu.555", i);
        pixels(name, surface, stride, w, h);
        wsprintfA(name, "state-%03lu.bin", i);
        HANDLE state = CreateFileA(name, GENERIC_WRITE, 0, NULL, CREATE_NEW, 0, NULL);
        if (state == INVALID_HANDLE_VALUE) fail("state output exists");
        for (int k = 2; k < 14; k++) write_bytes(state, at(p[k]), 4);
        CloseHandle(state);
        logs("scene: captured native scene and sky\r\n");
    }
    CloseHandle(input); logs("scene: complete\r\n"); quit(0);
}
