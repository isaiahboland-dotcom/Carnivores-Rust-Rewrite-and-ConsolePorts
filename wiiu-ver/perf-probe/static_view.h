#pragma once
#include <math.h>
#include <stdint.h>
#include <string.h>
/* Key: eye XYZ, yaw/pitch, player cell XZ. Matches all current static-world
 * generator inputs. Area/resources/projection are immutable within a session.
 * Invalidate on restart; extend the key if dynamic world/projection is added. */
typedef struct {
    int valid;
    float key[7];
    uint32_t terrain, scenery, water, sky;
} StaticView;
static int static_view_matches(const StaticView *view, const float key[7], int enabled) {
    if (!enabled || !view->valid) return 0;
    for (int i = 0; i < 7; ++i) if (!isfinite(key[i])) return 0;
    return memcmp(view->key, key, sizeof(view->key)) == 0;
}
static void static_view_save(StaticView *view, const float key[7], uint32_t terrain,
                             uint32_t scenery, uint32_t water, uint32_t sky) {
    memcpy(view->key, key, sizeof(view->key));
    view->terrain = terrain; view->scenery = scenery; view->water = water; view->sky = sky;
    view->valid = 1;
}
