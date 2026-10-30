/* Host state-sequence check of the actual foreground polling implementation. */
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdarg.h>
typedef int FSCmdBlock;
typedef int FSFileHandle;
typedef int FSStatus;
typedef enum { PROCUI_STATUS_IN_FOREGROUND, PROCUI_STATUS_IN_BACKGROUND,
    PROCUI_STATUS_RELEASE_FOREGROUND, PROCUI_STATUS_EXITING } ProcUIStatus;
#define TRUE 1
#define FS_ERROR_FLAG_NONE 0
#define FS_STATUS_OK 0
#define OSMillisecondsToTicks(n) (n)
static int client, audio_initialized = 1;
static unsigned polls, acknowledged, sleeps, stops, restores, flushes, closed, gpu_waits;
static ProcUIStatus sequence[] = {PROCUI_STATUS_IN_FOREGROUND,
    PROCUI_STATUS_RELEASE_FOREGROUND, PROCUI_STATUS_IN_BACKGROUND,
    PROCUI_STATUS_IN_FOREGROUND, PROCUI_STATUS_EXITING};
static ProcUIStatus ProcUIProcessMessages(int block) {
    assert(block == TRUE && polls < sizeof(sequence)/sizeof(sequence[0]));
    return sequence[polls++];
}
/* The hunt's last frame is finished before the GPU is released. */
static void GX2DrawDone(void) { ++gpu_waits; }
static void ProcUIDrawDoneRelease(void) { assert(!audio_initialized && gpu_waits == acknowledged + 1); ++acknowledged; }
static void OSSleepTicks(int ticks) { assert(ticks == 16); ++sleeps; }
static void performance_flush(void) { ++flushes; }
static void audio_shutdown(void) { assert(audio_initialized); audio_initialized = 0; ++stops; }
static void audio_resume(void *sim) { assert(sim && !audio_initialized); audio_initialized = 1; ++restores; }
static void OSReport(const char *format, ...) { (void)format; }
static uint64_t OSGetTime(void) { return 123; }
static const char *WHBGetSdCardMountPath(void) { return "/sd"; }
static void FSInitCmdBlock(FSCmdBlock *cmd) { *cmd = 0; }
static int FSOpenFile(int *c, int *cmd, const char *path, const char *mode, int *f, int flags) {
    (void)c; (void)cmd; (void)flags;
    assert(strstr(path, "lifecycle-") && !strcmp(mode, "w")); *f = 1; return 0;
}
static int FSWriteFile(int *c, int *cmd, const void *data, int size, int n, int f, int zero, int flags) {
    (void)c; (void)cmd; (void)zero; (void)flags;
    assert(data && size == 1 && n > 0 && f == 1); return n;
}
static int FSFlushFile(int *c, int *cmd, int f, int flags) { (void)c; (void)cmd; (void)flags; assert(f == 1); return 0; }
static int FSCloseFile(int *c, int *cmd, int f, int flags) { (void)c; (void)cmd; (void)flags; assert(f == 1); ++closed; return 0; }
#include "../lifecycle.h"
int main(void) {
    lifecycle_trace_open(); assert(lifecycle_log_open);
    lifecycle_sim = &client;
    assert(app_running() && polls == 1);
    assert(app_running() && polls == 4); // Release/background never returns a drawable frame.
    assert(acknowledged == 1 && sleeps == 1 && stops == 1 && restores == 1 && flushes == 1 && gpu_waits == 1);
    assert(!lifecycle_suspended_audio && audio_initialized);
    assert(!app_running() && polls == 5);
    assert(lifecycle_exiting && lifecycle_log_open); // Resources still available for cleanup.
    assert(!app_running() && polls == 5); // No repeated ProcUI calls after exit.
    lifecycle_trace_close(); assert(!lifecycle_log_open && closed == 1);
    lifecycle_exiting = 0;
    lifecycle_home_denied(NULL);
    assert(!app_running() && polls == 5); // HBL HOME denied requests clean teardown too.
    return 0;
}
