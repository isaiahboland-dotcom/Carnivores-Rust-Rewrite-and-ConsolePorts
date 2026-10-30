#include <assert.h>
#include <math.h>
#include "../map_view.h"
static void check_uv(float x, float z, int display) {
    float uv[24]; map_view_uv(x, z, display, uv);
    float span = display ? .25f : 1.0f;
    assert(fabsf(uv[4] - uv[0] - span) < .00001f);
    assert(fabsf(uv[9] - uv[1] - span) < .00001f);
    for (int i = 0; i < 6; ++i) {
        assert(uv[i*4] >= 0 && uv[i*4] <= 1);
        assert(uv[i*4+1] >= 0 && uv[i*4+1] <= 1);
        assert(uv[i*4+2] == 1 && uv[i*4+3] == 0);
    }
    // Matching corners across the two triangles prevent a diagonal seam.
    for (int i = 0; i < 4; ++i) {
        assert(uv[i] == uv[12+i]);
        assert(uv[8+i] == uv[16+i]);
    }
}
int main(void) {
    for (int dead = 0; dead < 2; ++dead)
    for (int full = 0; full < 2; ++full)
    for (int game = 0; game < 2; ++game) {
        int tv = map_view_visible(dead, full, game, 0);
        int drc = map_view_visible(dead, full, game, 1);
        if (dead) assert(!tv && !drc); // Death playback always renders the world.
        else if (full) assert(tv && drc);
        else assert(!tv && drc == !game);
    }
    for (int x = -256; x <= 1280; x += 17)
    for (int z = -256; z <= 1280; z += 19)
    for (int display = 0; display < 2; ++display) check_uv(x, z, display);
    float tv[24], drc[24];
    map_view_uv(512, 512, 0, tv); map_view_uv(512, 512, 1, drc);
    assert(tv[0] == 0 && tv[1] == 0 && tv[4] == 1 && tv[9] == 1);
    assert(drc[0] == .375f && drc[1] == .375f && drc[4] == .625f && drc[9] == .625f);
    map_view_uv(1024, 1024, 1, drc);
    assert(drc[0] == .75f && drc[1] == .75f);
    assert(tv[0] == 0 && tv[4] == 1); // Updating DRC leaves TV attributes intact.
    return 0;
}
