/* CPU writes happen only after the frame's GX2DrawDone. GX2R owns allocation
 * and binding; these helpers replace its whole-allocation cache maintenance.
 * Every written prefix is explicitly made visible before GPU submission. */
#ifndef C2_RANGE_UPLOAD
#define C2_RANGE_UPLOAD 1
#endif
static struct { uint64_t lock, cache; uint32_t bytes; } stream_upload;
static float *stream_lock(GX2RBuffer *buffer) {
    uint64_t start = OSGetTime();
    float *data = GX2RLockBufferEx(buffer, C2_RANGE_UPLOAD ?
        GX2R_RESOURCE_DISABLE_CPU_INVALIDATE | GX2R_RESOURCE_DISABLE_GPU_INVALIDATE : 0);
    stream_upload.lock += OSGetTime() - start;
    return data;
}
static void stream_unlock(GX2RBuffer *buffer, uint32_t vertices) {
    if (vertices > buffer->elemCount) OSFatal("Geometry stream exceeds GPU buffer");
    uint64_t start = OSGetTime();
    GX2RUnlockBufferEx(buffer, C2_RANGE_UPLOAD ?
        GX2R_RESOURCE_DISABLE_CPU_INVALIDATE | GX2R_RESOURCE_DISABLE_GPU_INVALIDATE : 0);
    if (C2_RANGE_UPLOAD && vertices) {
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_ATTRIBUTE_BUFFER, buffer->buffer, vertices * buffer->elemSize);
    }
    stream_upload.cache += OSGetTime() - start;
    stream_upload.bytes += (C2_RANGE_UPLOAD ? vertices : buffer->elemCount) * buffer->elemSize;
}
