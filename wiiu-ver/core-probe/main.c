#include <coreinit/debug.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <malloc.h>
#include <stdint.h>
#include <stdlib.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>
extern uint32_t c2_core_run(void);
void *c2_core_alloc(size_t size, size_t align) { if (align < sizeof(void *)) align = sizeof(void *); return memalign(align, size); }
void c2_core_free(void *ptr) { free(ptr); }
void c2_core_panic(void) { OSFatal("Carnivores Wii U: Rust core probe panic"); for (;;) {} }
int main(void) {
    WHBProcInit(); WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U shared core probe");
    WHBLogPrint("Running core checks..."); WHBLogConsoleDraw();
    uint32_t result = c2_core_run();
    WHBLogPrint(result & 1 ? "MSVC random sequence: PASS" : "MSVC random sequence: FAIL");
    WHBLogPrint(result & 2 ? "Unsigned random range: PASS" : "Unsigned random range: FAIL");
    WHBLogPrint(result & 4 ? "Signed random range: PASS" : "Signed random range: FAIL");
    WHBLogPrint(result & 8 ? "RGB555 conversion: PASS" : "RGB555 conversion: FAIL");
    WHBLogPrint(result & 16 ? "Image alpha conversion: PASS" : "Image alpha conversion: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); }
    WHBLogConsoleFree(); WHBProcShutdown(); return 0;
}
