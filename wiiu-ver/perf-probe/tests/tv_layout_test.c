#include <assert.h>
#include "../tv_layout.h"
int main(void) {
    uint32_t offset = 0;
    // 1080p FLOAT_R32 depth memory holds a 720p depth and colour buffer.
    assert(tv_layout(0x10000000u, 1920u * 1088u * 4u, 1280u * 720u * 4u, 0x800u, 1280u * 720u * 4u, 0x800u, &offset));
    assert(offset == 1280u * 720u * 4u);
    // The colour starts at its own alignment.
    assert(tv_layout(0x10000000u, 0x10000u, 0x1001u, 0x100u, 0x100u, 0x1000u, &offset) && offset == 0x2000u);
    // Exactly full fits; one byte over does not.
    assert(tv_layout(0x1000u, 0x3000u, 0x1000u, 0x1000u, 0x2000u, 0x1000u, &offset) && offset == 0x1000u);
    assert(!tv_layout(0x1000u, 0x2fffu, 0x1000u, 0x1000u, 0x2000u, 0x1000u, &offset));
    // A base off the depth's alignment, no memory, or zero alignments.
    assert(!tv_layout(0x1800u, 0x100000u, 0x1000u, 0x1000u, 0x1000u, 0x1000u, &offset));
    assert(!tv_layout(0, 0x100000u, 0x1000u, 0x1000u, 0x1000u, 0x1000u, &offset));
    assert(!tv_layout(0x1000u, 0x100000u, 0x1000u, 0, 0x1000u, 0x1000u, &offset));
    return 0;
}
