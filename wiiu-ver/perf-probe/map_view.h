/* Shared map-view policy/layout; no mutable state or GX2 dependencies. */
static int map_view_visible(int death_active, int full_map, int gamepad_game, int display) {
    return !death_active && (display == 0 ? full_map : (!gamepad_game || full_map));
}
static void map_view_uv(float px, float pz, int display, float attrs[24]) {
    float span = display == 1 ? 256.0f : 1024.0f;
    float left = display == 1 ? px - span / 2.0f : 0.0f;
    float top = display == 1 ? pz - span / 2.0f : 0.0f;
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
