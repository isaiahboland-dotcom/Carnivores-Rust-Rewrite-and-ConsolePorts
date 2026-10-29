// GX2 drawing follows devkitPro WUT's gx2_triangle sample. The bundled
// pos_col_shader.gsh is unmodified from that sample; see WUT-LICENSE.txt.
#include <coreinit/debug.h>
#include <gx2/draw.h>
#include <gx2/registers.h>
#include <gx2/shaders.h>
#include <gx2r/buffer.h>
#include <gx2r/draw.h>
#include <stdint.h>
#include <whb/gfx.h>
#include <whb/proc.h>

extern const unsigned char c2_sample_shader[];
extern void c2_gx2_vertices(float *positions, float *colors, uint32_t frame);
void c2_gx2_panic(void) { OSFatal("Carnivores Wii U GX2 probe: Rust panic"); for (;;) {} }
static void fail(const char *message) { OSFatal(message); for (;;) {} }

static void draw_triangle(WHBGfxShaderGroup *shader, GX2RBuffer *positions, GX2RBuffer *colors) {
    GX2SetFetchShader(&shader->fetchShader);
    GX2SetVertexShader(shader->vertexShader);
    GX2SetPixelShader(shader->pixelShader);
    GX2RSetAttributeBuffer(positions, 0, positions->elemSize, 0);
    GX2RSetAttributeBuffer(colors, 1, colors->elemSize, 0);
    GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLES, 3, 0, 1);
}
int main(void) {
    WHBGfxShaderGroup shader = {0};
    GX2RBuffer positions = {0}, colors = {0};
    WHBProcInit();
    if (!WHBGfxInit()) fail("Carnivores GX2 probe: graphics init failed");
    if (!WHBGfxLoadGFDShaderGroup(&shader, 0, c2_sample_shader)) fail("Carnivores GX2 probe: shader load failed");
    if (!WHBGfxInitShaderAttribute(&shader, "aPosition", 0, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitShaderAttribute(&shader, "aColour", 1, 0, GX2_ATTRIB_FORMAT_FLOAT_32_32_32_32)
        || !WHBGfxInitFetchShader(&shader)) fail("Carnivores GX2 probe: shader setup failed");
    positions.flags = GX2R_RESOURCE_BIND_VERTEX_BUFFER | GX2R_RESOURCE_USAGE_CPU_WRITE | GX2R_RESOURCE_USAGE_GPU_READ;
    positions.elemSize = 4 * sizeof(float); positions.elemCount = 3;
    colors.flags = positions.flags; colors.elemSize = positions.elemSize; colors.elemCount = 3;
    if (!GX2RCreateBuffer(&positions) || !GX2RCreateBuffer(&colors)) fail("Carnivores GX2 probe: buffer creation failed");
    uint32_t frame = 0;
    while (WHBProcIsRunning()) {
        float *p = GX2RLockBufferEx(&positions, 0);
        float *c = GX2RLockBufferEx(&colors, 0);
        if (!p || !c) fail("Carnivores GX2 probe: buffer lock failed");
        c2_gx2_vertices(p, c, frame++);
        GX2RUnlockBufferEx(&positions, 0); GX2RUnlockBufferEx(&colors, 0);
        WHBGfxBeginRender();
        WHBGfxBeginRenderTV(); WHBGfxClearColor(0.0f, 0.0f, 0.35f, 1.0f);
        draw_triangle(&shader, &positions, &colors); WHBGfxFinishRenderTV();
        WHBGfxBeginRenderDRC(); WHBGfxClearColor(0.3f, 0.0f, 0.3f, 1.0f);
        draw_triangle(&shader, &positions, &colors); WHBGfxFinishRenderDRC();
        WHBGfxFinishRender();
    }
    GX2RDestroyBufferEx(&positions, 0); GX2RDestroyBufferEx(&colors, 0);
    WHBGfxFreeShaderGroup(&shader); WHBGfxShutdown(); WHBProcShutdown(); return 0;
}
