#include <coreinit/debug.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <malloc.h>
#include <stdint.h>
#include <stdlib.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>
extern uint32_t c2_model_run(void);
void *c2_model_alloc(size_t size, size_t align) { if (align < sizeof(void *)) align = sizeof(void *); return memalign(align, size); }
void c2_model_free(void *ptr) { free(ptr); }
void c2_model_panic(void) { OSFatal("Carnivores Wii U: Rust model probe panic"); for (;;) {} }
int main(void) {
    WHBProcInit(); WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U model and animation probe");
    WHBLogPrint("Running model checks..."); WHBLogConsoleDraw();
    uint32_t result = c2_model_run();
    WHBLogPrint(result & 1 ? "Animation start: PASS" : "Animation start: FAIL");
    WHBLogPrint(result & 2 ? "Animation interpolation: PASS" : "Animation interpolation: FAIL");
    WHBLogPrint(result & 4 ? "Animation wrap and invalid: PASS" : "Animation wrap and invalid: FAIL");
    WHBLogPrint(result & 8 ? "Model transform and radius: PASS" : "Model transform and radius: FAIL");
    WHBLogPrint(result & 16 ? "Vertex lighting: PASS" : "Vertex lighting: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); }
    WHBLogConsoleFree(); WHBProcShutdown(); return 0;
}
