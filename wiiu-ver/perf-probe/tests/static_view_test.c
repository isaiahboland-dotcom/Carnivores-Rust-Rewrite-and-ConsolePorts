#include <assert.h>
#include "../static_view.h"
int main(void) {
    StaticView view = {0};
    float key[7] = {100, 200, 300, .5f, -.3f, 10, 20};
    assert(!static_view_matches(&view, key, 1));
    static_view_save(&view, key, 12, 24, 0, 6);
    assert(static_view_matches(&view, key, 1));
    assert(view.terrain == 12 && view.scenery == 24 && view.water == 0 && view.sky == 6);
    assert(!static_view_matches(&view, key, 0)); // A/B bypass always rebuilds.
    for (int i = 0; i < 7; ++i) {
        float old = key[i];
        key[i] = nextafterf(old, INFINITY); // Even one ULP of motion must miss.
        assert(!static_view_matches(&view, key, 1));
        key[i] = old;
        assert(static_view_matches(&view, key, 1));
        key[i] = NAN;
        assert(!static_view_matches(&view, key, 1));
        key[i] = old;
    }
    view.valid = 0; // Restart invalidates even if the new spawn has the same key.
    assert(!static_view_matches(&view, key, 1));
    static_view_save(&view, key, 0, 0, 0, 0);
    assert(static_view_matches(&view, key, 1)); // Empty buffers are reusable.
    assert(view.terrain == 0 && view.scenery == 0 && view.water == 0 && view.sky == 0);
    return 0;
}
