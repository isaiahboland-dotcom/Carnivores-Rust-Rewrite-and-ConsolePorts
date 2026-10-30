/* Shared map-view policy/layout; no mutable state or GX2 dependencies. */
static int map_view_visible(int death_active, int full_map, int gamepad_game, int display) {
    return !death_active && (display == 0 ? full_map : (!gamepad_game || full_map));
}
/* The map texture's `span` cells round the player (256 on the GamePad, the
 * whole 1024 zoomed out or on the TV). */
static void map_view_uv(float px, float pz, float span, float attrs[24]) {
    float left = px - span / 2.0f, top = pz - span / 2.0f;
    if (left < 0) left = 0;
    if (left > 1024 - span) left = 1024 - span;
    if (top < 0) top = 0;
    if (top > 1024 - span) top = 1024 - span;
    float u0 = left / 1024.0f, v0 = top / 1024.0f;
    float u1 = (left + span) / 1024.0f, v1 = (top + span) / 1024.0f;
    const float result[24] = {u0,v0,1,0, u1,v0,1,0, u1,v1,1,0,
                              u0,v0,1,0, u1,v1,1,0, u0,v1,1,0};
    for (int i = 0; i < 24; ++i) attrs[i] = result[i];
}
/* A touch on the GamePad map (854x480; the map fills x 187..667) as map
 * texture cells, from the view's UVs; 0 outside it. */
static int map_view_touch(const float attrs[24], int tx, int ty, float *x, float *z) {
    if (tx < 187 || tx >= 667 || ty < 0 || ty >= 480) return 0;
    *x = (attrs[0] + (tx - 187) / 480.0f * (attrs[4] - attrs[0])) * 1024.0f;
    *z = (attrs[1] + ty / 480.0f * (attrs[9] - attrs[1])) * 1024.0f;
    return 1;
}
