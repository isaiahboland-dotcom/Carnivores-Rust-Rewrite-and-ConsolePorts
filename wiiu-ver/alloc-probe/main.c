#include <coreinit/debug.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <malloc.h>
#include <stdint.h>
#include <stdlib.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>

extern uint32_t c2_probe_run(void);
static uint32_t alloc_count;
static uint32_t free_count;

void *c2_probe_alloc(size_t size, size_t alignment) {
    if (alignment < sizeof(void *)) alignment = sizeof(void *);
    void *ptr = memalign(alignment, size);
    if (ptr) ++alloc_count;
    return ptr;
}
void c2_probe_free(void *ptr) { ++free_count; free(ptr); }
uint32_t c2_probe_alloc_count(void) { return alloc_count; }
uint32_t c2_probe_free_count(void) { return free_count; }
void c2_probe_panic(void) {
    OSFatal("Carnivores Wii U: Rust probe panic");
    for (;;) {}
}

int main(void) {
    WHBProcInit();
    WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U allocation and parser probe");
    WHBLogPrint("Running Rust checks...");
    WHBLogConsoleDraw();
    uint32_t result = c2_probe_run();
    WHBLogPrint(result & 1 ? "Allocation and growth: PASS" : "Allocation and growth: FAIL");
    WHBLogPrint(result & 2 ? "64-byte alignment: PASS" : "64-byte alignment: FAIL");
    WHBLogPrint(result & 4 ? "TGA parse and endian: PASS" : "TGA parse and endian: FAIL");
    WHBLogPrint(result & 8 ? "TGA write round trip: PASS" : "TGA write round trip: FAIL");
    WHBLogPrint(result & 16 ? "Truncated TGA error: PASS" : "Truncated TGA error: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) {
        WHBLogConsoleDraw();
        OSSleepTicks(OSMillisecondsToTicks(16));
    }
    WHBLogConsoleFree();
    WHBProcShutdown();
    return 0;
}
