/* Poll ProcUI without allowing WHBProcIsRunning to shut it down before the
 * session has released its resources. Never return a drawable frame while the
 * application is releasing foreground or in the background. */
static int lifecycle_exiting, lifecycle_suspended_audio;
static void *lifecycle_sim;
static FSCmdBlock lifecycle_cmd;
static FSFileHandle lifecycle_file;
static int lifecycle_log_open;
static uint32_t lifecycle_home_denied(void *context) {
    (void)context;
    lifecycle_exiting = 1;
    return 0;
}
static void lifecycle_checkpoint(const char *message) {
    OSReport("C2 lifecycle: %s\n", message);
    if (!lifecycle_log_open) return;
    char line[192] __attribute__((aligned(64)));
    int n = snprintf(line, sizeof(line), "%s\n", message);
    if (n <= 0 || n >= (int)sizeof(line)) return;
    FSStatus wrote = FSWriteFile(&client, &lifecycle_cmd, line, 1, n, lifecycle_file, 0, FS_ERROR_FLAG_NONE);
    if (wrote == n) FSFlushFile(&client, &lifecycle_cmd, lifecycle_file, FS_ERROR_FLAG_NONE);
}
static void lifecycle_trace_open(void) {
    char path[256];
    snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/lifecycle-%llx.log",
        WHBGetSdCardMountPath(), (unsigned long long)OSGetTime());
    FSInitCmdBlock(&lifecycle_cmd);
    lifecycle_log_open = FSOpenFile(&client, &lifecycle_cmd, path, "w", &lifecycle_file, FS_ERROR_FLAG_NONE) == FS_STATUS_OK;
    lifecycle_checkpoint("trace v1 open; ProcUI shutdown deferred until cleanup");
}
static void lifecycle_trace_close(void) {
    if (lifecycle_log_open) FSCloseFile(&client, &lifecycle_cmd, lifecycle_file, FS_ERROR_FLAG_NONE);
    lifecycle_log_open = 0;
}
static int app_running(void) {
    if (lifecycle_exiting) return 0;
    for (;;) {
        ProcUIStatus status = ProcUIProcessMessages(TRUE);
        if (status == PROCUI_STATUS_EXITING) {
            lifecycle_checkpoint("ProcUI EXITING; returning to resource cleanup");
            lifecycle_exiting = 1;
            return 0;
        }
        if (status == PROCUI_STATUS_RELEASE_FOREGROUND) {
            lifecycle_checkpoint("foreground release: flushing profiler");
            performance_flush();
            if (lifecycle_sim && audio_initialized) {
                lifecycle_checkpoint("foreground release: stopping audio");
                audio_shutdown();
                lifecycle_suspended_audio = 1;
            }
            lifecycle_checkpoint("foreground release: acknowledging DrawDoneRelease");
            ProcUIDrawDoneRelease();
            // WHB's graphics release/acquire callbacks manage its foreground
            // buffers. Do not render or poll controllers until reacquired.
            continue;
        }
        if (status == PROCUI_STATUS_IN_BACKGROUND) {
            OSSleepTicks(OSMillisecondsToTicks(16));
            continue;
        }
        if (status == PROCUI_STATUS_IN_FOREGROUND) {
            if (lifecycle_suspended_audio && lifecycle_sim) {
                lifecycle_checkpoint("foreground reacquired: restoring audio");
                audio_resume(lifecycle_sim);
                lifecycle_suspended_audio = 0;
            }
            return !lifecycle_exiting;
        }
    }
}
