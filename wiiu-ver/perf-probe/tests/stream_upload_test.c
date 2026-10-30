/* Host test: cc -std=c11 -Wall -Wextra -Werror tests/stream_upload_test.c -o /tmp/stream-upload-test */
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
typedef struct { uint32_t elemSize, elemCount; void *buffer; } GX2RBuffer;
#define GX2R_RESOURCE_DISABLE_CPU_INVALIDATE (1u << 20)
#define GX2R_RESOURCE_DISABLE_GPU_INVALIDATE (1u << 21)
#define GX2_INVALIDATE_MODE_CPU_ATTRIBUTE_BUFFER 17
static uint64_t timebase;
static uint32_t lock_flags, unlock_flags, flush_bytes, flush_count;
static void *flush_address;
static uint64_t OSGetTime(void) { return ++timebase; }
static void OSFatal(const char *message) { fputs(message, stderr); abort(); }
static void *GX2RLockBufferEx(GX2RBuffer *b, uint32_t flags) { lock_flags = flags; return b->buffer; }
static void GX2RUnlockBufferEx(GX2RBuffer *b, uint32_t flags) { (void)b; unlock_flags = flags; }
static void GX2Invalidate(int mode, void *buffer, uint32_t size) {
    assert(mode == GX2_INVALIDATE_MODE_CPU_ATTRIBUTE_BUFFER);
    flush_address = buffer; flush_bytes = size; ++flush_count;
}
#include "../stream_upload.h"
int main(void) {
    float memory[64];
    GX2RBuffer b = {16, 16, memory};
    assert(stream_lock(&b) == memory);
    stream_unlock(&b, 3);
    if (C2_RANGE_UPLOAD) {
        uint32_t disabled = GX2R_RESOURCE_DISABLE_CPU_INVALIDATE | GX2R_RESOURCE_DISABLE_GPU_INVALIDATE;
        assert(lock_flags == disabled && unlock_flags == disabled);
        assert(flush_count == 1 && flush_bytes == 48 && flush_address == memory);
        assert(stream_upload.bytes == 48);
    } else {
        assert(lock_flags == 0 && unlock_flags == 0 && flush_count == 0);
        assert(stream_upload.bytes == 256);
    }
    uint32_t previous_flushes = flush_count;
    stream_unlock(&b, 0);
    assert(flush_count == previous_flushes);
    stream_unlock(&b, 16);
    if (C2_RANGE_UPLOAD) assert(flush_bytes == sizeof(memory));
    assert(stream_upload.lock > 0 && stream_upload.cache > 0);
    StreamRange ranges[2] = {{2, 3}, {10, 4}};
    uint32_t bytes = stream_upload.bytes, flushes = flush_count;
    stream_unlock_ranges(&b, ranges, 2);
    if (C2_RANGE_UPLOAD) {
        assert(flush_count == flushes + 2 && flush_address == memory + 40 && flush_bytes == 64);
        assert(stream_upload.bytes == bytes + 112);
    } else {
        assert(flush_count == flushes && stream_upload.bytes == bytes + 256);
    }
    return 0;
}
