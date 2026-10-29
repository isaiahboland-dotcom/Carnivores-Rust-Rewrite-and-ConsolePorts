#include <coreinit/debug.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <stdint.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>

extern int32_t c2_runtime_integer(uint32_t a, int32_t b);
extern float c2_runtime_float(float a, float b);
extern uint32_t c2_runtime_decode(const uint8_t *bytes);

void c2_runtime_panic(void) {
    OSFatal("Carnivores Wii U: Rust panic");
    for (;;) {}
}

int main(void) {
    const uint8_t bytes[] = {0x78, 0x56, 0x34, 0x12};
    int integer_ok = c2_runtime_integer(17, -9) == 42;
    int float_ok = c2_runtime_float(1.25f, -0.5f) == 2.0f;
    int endian_ok = c2_runtime_decode(bytes) == 0x12345678u;

    WHBProcInit();
    WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U Rust runtime probe");
    WHBLogPrint(integer_ok ? "Integer ABI: PASS" : "Integer ABI: FAIL");
    WHBLogPrint(float_ok ? "Float ABI: PASS" : "Float ABI: FAIL");
    WHBLogPrint(endian_ok ? "Little-endian decode: PASS" : "Little-endian decode: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) {
        WHBLogConsoleDraw();
        OSSleepTicks(OSMillisecondsToTicks(16));
    }
    WHBLogConsoleFree();
    WHBProcShutdown();
    return 0;
}
