/* Isolated texture captures INSIDE the shipped software renderer. No game
 * source is bundled or recompiled. The host runner discovers and validates the
 * two native entry points and viewport globals from the executable's code.
 * Geometry is posed by its CreateMorphedModel and drawn by RenderModelClip.
 * A private 1024-pixel-stride surface replaces the game's DIB only for this
 * call. The process exits after the batch; gameplay rendering never resumes. */
#include "hook.h"

typedef struct { float x, y, z; short owner, hide; } Vertex;
typedef struct {
    int v[3], tx[3], ty[3];
    WORD flags, dmask;
    int distant, next, group;
    char reserved[12];
} Face;
typedef struct {
    int vc, fc, texture_size, texture_height;
    Vertex vertices[1024];
    Face faces[1024];
    WORD *texture, *texture2, *texture3;
    float lights[4][1024];
} Model;
typedef struct {
    int vc, fc, texture_size, texture_height;
    Vertex *vertices; Face *faces;
    WORD *texture, *texture2, *texture3;
    float *lights[4];
} DynamicModel;
typedef struct { char name[32]; int kps, frames, duration; short *data; } Animation;
typedef void (__cdecl *Morph)(Model *, Animation *, int, float);
typedef void (__cdecl *Render)(Model *, float, float, float, int, int, float, float);

/* Genesis/MEE's link-time optimizer specialized these two internal calls.
 * Model/animation use ECX/EDX; the morph time is caller-cleaned on the stack.
 * The renderer takes light in EDX and xyz in XMM1..3, followed by VT/yaw/pitch.
 * The PE recognizer checks this exact ABI before allowing either wrapper. */
static void dynamic_morph(void *entry, DynamicModel *model, Animation *animation, DWORD time) {
    __asm__ volatile("push %[time]\n\tcall *%[entry]\n\taddl $4, %%esp"
        : "+c"(model), "+d"(animation)
        : [entry] "r"(entry), [time] "r"(time)
        : "eax", "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "cc", "memory");
}
static void dynamic_render(void *entry, DynamicModel *model) {
    int light = 240;
    __asm__ volatile("push $0\n\tpush $0\n\tpush $0\n\txorps %%xmm1, %%xmm1\n\txorps %%xmm2, %%xmm2\n\txorps %%xmm3, %%xmm3\n\tcall *%[entry]\n\taddl $12, %%esp"
        : "+c"(model), "+d"(light)
        : [entry] "r"(entry)
        : "eax", "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "cc", "memory");
}

static void **video_slot;
static DWORD image_size;
static BYTE *image;
static HANDLE input;

static void fail(const char *why) {
    logs("texture: ERROR "); logs(why); logs("\r\n"); quit(5);
}
static void read_bytes(void *to, DWORD n) {
    DWORD got;
    if (!ReadFile(input, to, n, &got, NULL) || got != n) fail("truncated job");
}
static DWORD integer(void) { DWORD n; read_bytes(&n, 4); return n; }
static void *allocate(DWORD n) {
    void *p = HeapAlloc(GetProcessHeap(), HEAP_ZERO_MEMORY, n);
    if (!p) fail("allocation failed");
    return p;
}
static void *address(DWORD rva) {
    if (rva < 4096 || rva > image_size - 4) fail("address outside executable");
    return image + rva;
}
static float fsin(float x) { float r; __asm__("fsin" : "=t"(r) : "0"(x)); return r; }
static float fcos(float x) { float r; __asm__("fcos" : "=t"(r) : "0"(x)); return r; }
static HANDLE output(const char *name, const char *suffix) {
    char path[180]; int i;
    /* Job names are single safe stems, never paths. */
    for (i = 0; name[i]; i++) {
        char c = name[i];
        if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
              (c >= '0' && c <= '9') || c == '-' || c == '_')) fail("unsafe capture name");
    }
    lstrcpyA(path, name); lstrcatA(path, suffix);
    HANDLE f = CreateFileA(path, GENERIC_WRITE, 0, NULL, CREATE_NEW, 0, NULL);
    if (f == INVALID_HANDLE_VALUE) fail("output exists or cannot be created");
    return f;
}
static void write_bytes(HANDLE f, const void *p, DWORD n) {
    DWORD wrote;
    if (!WriteFile(f, p, n, &wrote, NULL) || wrote != n) fail("output write failed");
}

static LONG WINAPI texture_exception(EXCEPTION_POINTERS *e) {
    char message[240];
    wsprintfA(message, "texture: native exception %lx at %lx eax=%lx ecx=%lx edx=%lx edi=%lx esi=%lx\r\n",
        e->ExceptionRecord->ExceptionCode, e->ContextRecord->Eip,
        e->ContextRecord->Eax, e->ContextRecord->Ecx, e->ContextRecord->Edx,
        e->ContextRecord->Edi, e->ContextRecord->Esi);
    logs(message); quit(5); return EXCEPTION_CONTINUE_SEARCH;
}

void texture_run(const char *path) {
    DWORD profile[18], count, job;
    char magic[8];
    IMAGE_NT_HEADERS *nt;
    image = (BYTE *)GetModuleHandleA(NULL);
    nt = (IMAGE_NT_HEADERS *)(image + ((IMAGE_DOS_HEADER *)image)->e_lfanew);
    image_size = nt->OptionalHeader.SizeOfImage;
    input = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    if (input == INVALID_HANDLE_VALUE) fail("job cannot be opened");
    read_bytes(magic, 8);
    if (lstrcmpA(magic, "C2TXRV3")) fail("unsupported job schema");
    read_bytes(profile, sizeof profile);
    if (profile[0] != nt->FileHeader.TimeDateStamp || profile[1] != image_size)
        fail("job belongs to a different executable");
    video_slot = address(profile[17]);
    if (!*video_slot) fail("the original video surface is not initialized");
    AddVectoredExceptionHandler(1, texture_exception);
    Morph morph = (Morph)address(profile[3]);
    Render render = (Render)address(profile[2]);
    if (*(BYTE *)render != 0x55 || *(BYTE *)morph != (profile[12] == 1024 ? 0x8b : 0x51)) fail("entry point signature mismatch");
    float *camera_w = address(profile[4]), *camera_h = address(profile[5]);
    int *cx = address(profile[6]), *cy = address(profile[7]);
    int *ex = address(profile[8]), *ey = address(profile[9]);
    int *water = address(profile[10]);
    int *glass = address(profile[11]);
    BOOL dynamic = profile[12] == 8192;
    if (profile[12] != 1024 && !dynamic) fail("unsupported model layout");
    if (dynamic) {
        *(int *)address(profile[15]) = 2048;
        *(void **)address(profile[13]) = allocate(8192 * 12);
        *(void **)address(profile[14]) = allocate(8192 * 8);
    }
    WORD *surface = allocate(1024 * 768 * 2);
    *video_slot = surface;
    count = integer();
    if (!count || count > 100000) fail("invalid capture count");
    for (job = 0; job < count; job++) {
        Model *m = allocate(sizeof(Model));
        Animation a; DynamicModel dm; Vertex *vertices; Face *faces;
        DWORD w, h, t, frames, vc, fc, texels, i, y;
        float camera[7];
        WORD background;
        char name[128], message[180];
        read_bytes(name, sizeof name); name[127] = 0;
        w = integer(); h = integer(); t = integer();
        read_bytes(camera, sizeof camera); /* target xyz, distance, yaw, pitch, half-angle tangent */
        background = (WORD)integer();
        vc = integer(); fc = integer(); texels = integer();
        if (w < 128 || w > 1024 || h < 128 || h > 768 || !vc || vc > profile[12] || !fc || fc > profile[12] ||
            texels > 256 * 256 || !texels || texels % 256) fail("unsupported model or resolution");
        m->vc = vc; m->fc = fc; m->texture_size = texels * 2; m->texture_height = texels / 256;
        vertices = dynamic ? allocate(vc * sizeof(Vertex)) : m->vertices;
        faces = dynamic ? allocate(fc * sizeof(Face)) : m->faces;
        read_bytes(faces, fc * sizeof(Face));
        read_bytes(vertices, vc * sizeof(Vertex));
        m->texture = allocate(256 * 256 * 2);
        read_bytes(m->texture, texels * 2);
        /* Software LoadCharacterInfo: DATASHIFT, neutral brightness/day.
         * CorrectModel's UV offsets and culling bit; stable face ordering is
         * already supplied by the host, matching the original loader. */
        for (i = 0; i < texels; i++) m->texture[i] = (WORD)((m->texture[i] & 0x7fff) * 2);
        for (i = 0; i < fc; i++) {
            Face *f = &faces[i]; int k;
            for (k = 0; k < 3; k++) {
                if (f->v[k] < 0 || (DWORD)f->v[k] >= vc || f->tx[k] < -32768 || f->tx[k] > 32767 ||
                    f->ty[k] < -32768 || f->ty[k] > 32767) fail("invalid face");
                f->tx[k] = (int)(((DWORD)f->tx[k] << 16) + 0x8000);
                f->ty[k] = (int)(((DWORD)f->ty[k] << 16) + 0x8000);
            }
            if (!(f->flags & 1)) f->flags |= 0x80;
        }
        memset(&dm, 0, sizeof dm);
        dm.vc = vc; dm.fc = fc; dm.texture_size = m->texture_size; dm.texture_height = m->texture_height;
        dm.vertices = vertices; dm.faces = faces; dm.texture = m->texture;
        frames = integer();
        memset(&a, 0, sizeof a);
        if (frames) {
            if (frames > 10000) fail("too many animation frames");
            a.frames = frames; a.kps = (int)integer(); a.duration = (int)integer();
            if (a.kps <= 0 || a.duration <= 0 || frames > 0x7fffffff / vc / 6 ||
                t >= (DWORD)a.duration || (frames - 1) * t > 0x7fffffff / 256)
                fail("invalid or overflowing native animation time");
            /* Duplicate the final frame: the native interpolator reads its
             * successor even when its interpolation weight is zero. */
            a.data = allocate((frames + 1) * vc * 6);
            read_bytes(a.data, frames * vc * 6);
            for (i = 0; i < vc * 3; i++) a.data[frames * vc * 3 + i] = a.data[(frames - 1) * vc * 3 + i];
            if (dynamic) dynamic_morph((void *)morph, &dm, &a, t);
            else morph(m, &a, (int)t, 1.0f);
        } else {
            for (i = 0; i < vc; i++) { vertices[i].x *= 2; vertices[i].y *= 2; vertices[i].z *= -2; }
        }
        HANDLE pose = output(name, ".pose");
        for (i = 0; i < vc; i++) write_bytes(pose, &vertices[i], 12);
        CloseHandle(pose);
        float sy = fsin(camera[4]), c_y = fcos(camera[4]);
        float sp = fsin(camera[5]), cp = fcos(camera[5]);
        *camera_w = *camera_h = (float)h / (2 * camera[6]);
        *(int *)address(profile[16]) = (int)w;
        *cx = (int)w / 2; *cy = (int)h / 2; *ex = (int)w - 1; *ey = (int)h - 1;
        *water = 0; *glass = 0;
        for (i = 0; i < vc; i++) {
            float x = vertices[i].x - camera[0], yv = vertices[i].y - camera[1];
            float z = vertices[i].z - camera[2];
            vertices[i].x = c_y * x - sy * z;
            vertices[i].y = -sy * sp * x + cp * yv - c_y * sp * z;
            vertices[i].z = sy * cp * x + sp * yv + c_y * cp * z - camera[3];
            /* Isolated captures deliberately avoid the game's different
             * near plane and gameplay clipping planes. Refuse unmatched
             * framing rather than quietly use a different projection. */
            float px = *cx + vertices[i].x / -vertices[i].z * *camera_w;
            float py = *cy - vertices[i].y / -vertices[i].z * *camera_h;
            if (vertices[i].z > -64 || px < 1 || px >= *ex || py < 1 || py >= *ey)
                fail("recipe intersects native clipping planes; widen the shared framing");
        }
        for (i = 0; i < 1024 * 768; i++) surface[i] = background;
        char diagnostics[180];
        wsprintfA(diagnostics, "texture: drawing %s surface=%lx slot=%lx w=%d h=%d cx=%d cy=%d vc=%lu fc=%lu\r\n",
            name, (DWORD)surface, (DWORD)video_slot, *ex+1, *ey+1, *cx, *cy, vc, fc);
        logs(diagnostics);
        if (dynamic) dynamic_render((void *)render, &dm);
        else render(m, 0, 0, 0, 240, 0, 0, 0); /* FadeTab[60]: full light, no scene shading. */
        HANDLE pixels = output(name, ".555");
        write_bytes(pixels, &w, 4); write_bytes(pixels, &h, 4);
        for (y = 0; y < h; y++) write_bytes(pixels, surface + y * 1024, w * 2);
        CloseHandle(pixels);
        wsprintfA(message, "texture: captured %s native-pose native-render\r\n", name); logs(message);
        if (a.data) HeapFree(GetProcessHeap(), 0, a.data);
        if (dynamic) { HeapFree(GetProcessHeap(), 0, vertices); HeapFree(GetProcessHeap(), 0, faces); }
        HeapFree(GetProcessHeap(), 0, m->texture); HeapFree(GetProcessHeap(), 0, m);
    }
    CloseHandle(input);
    logs("texture: complete\r\n"); quit(0);
}
