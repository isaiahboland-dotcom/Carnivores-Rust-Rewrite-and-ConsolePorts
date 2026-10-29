#include <coreinit/debug.h>
#include <coreinit/filesystem.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <malloc.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <whb/log.h>
#include <whb/log_console.h>
#include <whb/proc.h>
#include <whb/sdcard.h>
extern uint32_t c2_vfs_run(void);
void *c2_vfs_alloc(size_t size, size_t align) { if (align < sizeof(void *)) align = sizeof(void *); return memalign(align, size); }
void c2_vfs_free(void *ptr) { free(ptr); }
void c2_vfs_panic(void) { OSFatal("Carnivores Wii U: Rust VFS probe panic"); for (;;) {} }
static FSClient client;
static FSCmdBlock cmd;
static uint8_t aligned_data[64] __attribute__((aligned(64)));
// Called synchronously from one Rust thread, with an output buffer of 26 bytes.
int32_t c2_vfs_read_fixture(uint32_t which, uint8_t *output) {
    char path[256];
    FSFileHandle file;
    const char *name = which == 0 ? "fixture.tga" : "missing.tga";
    if (snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/%s", WHBGetSdCardMountPath(), name) >= (int)sizeof(path)) return -1000;
    FSStatus status = FSOpenFile(&client, &cmd, path, "r", &file, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) return status;
    status = FSReadFile(&client, &cmd, aligned_data, 1, 26, file, 0, FS_ERROR_FLAG_ALL);
    FSStatus close_status = FSCloseFile(&client, &cmd, file, FS_ERROR_FLAG_ALL);
    if (status < 0) return status;
    if (status != 26) return -1001;
    if (close_status != FS_STATUS_OK) return close_status;
    memcpy(output, aligned_data, 26);
    return 0;
}
int main(void) {
    int mounted = 0, added = 0;
    WHBProcInit(); WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U Rust VFS probe");
    WHBLogPrint("Running VFS checks..."); WHBLogConsoleDraw();
    if (!WHBMountSdCard()) { WHBLogPrint("SD mount: FAIL"); goto done; }
    mounted = 1; WHBLogPrint("SD mount: PASS");
    FSStatus status = FSAddClient(&client, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) { WHBLogPrintf("FS client: FAIL (%d)", status); goto done; }
    added = 1; FSInitCmdBlock(&cmd);
    uint32_t result = c2_vfs_run();
    WHBLogPrint(result & 1 ? "Rust VFS SD read: PASS" : "Rust VFS SD read: FAIL");
    WHBLogPrint(result & 2 ? "Shared parser: PASS" : "Shared parser: FAIL");
    WHBLogPrint(result & 4 ? "SD missing-file error: PASS" : "SD missing-file error: FAIL");
    WHBLogPrint(result & 8 ? "VFS missing-file error: PASS" : "VFS missing-file error: FAIL");
    WHBLogPrint(result & 16 ? "Layer override: PASS" : "Layer override: FAIL");
done:
    if (added) { FSStatus s = FSDelClient(&client, FS_ERROR_FLAG_ALL); if (s != FS_STATUS_OK) WHBLogPrintf("Client removal: FAIL (%d)", s); }
    if (mounted && !WHBUnmountSdCard()) WHBLogPrint("Unmount: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); }
    WHBLogConsoleFree(); WHBProcShutdown(); return 0;
}
