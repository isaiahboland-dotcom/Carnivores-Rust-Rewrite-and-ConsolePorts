#pragma once
#include <stdint.h>
/* Where the TV's 720p world target goes inside the memory of WHB's 1080p TV
 * depth buffer (tv_target.h): its depth buffer at the start, its colour
 * buffer after it at the colour's alignment. 1 if both fit in `available`
 * bytes from `base` and `base` suits the depth's alignment. */
static int tv_layout(uintptr_t base, uint32_t available, uint32_t depth_size, uint32_t depth_align,
                     uint32_t color_size, uint32_t color_align, uint32_t *color_offset) {
    if (!base || !depth_align || !color_align || base % depth_align) return 0;
    uint64_t color = ((uint64_t)base + depth_size + color_align - 1) / color_align * color_align - base;
    if (color + color_size > available) return 0;
    *color_offset = (uint32_t)color;
    return 1;
}
