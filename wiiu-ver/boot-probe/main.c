#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>

extern const char *c2_probe_message(void);

int main(void) {
    WHBProcInit();
    WHBLogConsoleInit();
    WHBLogPrint(c2_probe_message());
    WHBLogPrint("Boot test successful. This is not the game yet.");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) {
        WHBLogConsoleDraw();
        OSSleepTicks(OSMillisecondsToTicks(16));
    }
    WHBLogConsoleFree();
    WHBProcShutdown();
    return 0;
}
