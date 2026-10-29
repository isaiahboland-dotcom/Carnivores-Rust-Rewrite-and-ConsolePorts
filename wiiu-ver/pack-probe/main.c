#include <coreinit/debug.h>
#include <coreinit/filesystem.h>
#include <coreinit/mutex.h>
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
extern uint32_t c2_pack_run(void);
void *c2_pack_alloc(size_t n, size_t a) { if (a < sizeof(void *)) a = sizeof(void *); return memalign(a, n); }
void c2_pack_free(void *p) { free(p); }
void c2_pack_panic(void) { OSFatal("Carnivores Wii U: Rust pack probe panic"); for (;;) {} }
static FSClient client;
static OSMutex mutex;
static uint8_t buffer[4096] __attribute__((aligned(64)));
static FSStatus open_file(const char *relative, FSCmdBlock *cmd, FSFileHandle *file) {
    char path[512];
    if (snprintf(path, sizeof(path), "%s/%s", WHBGetSdCardMountPath(), relative) >= (int)sizeof(path)) return -1002;
    return FSOpenFile(&client, cmd, path, "r", file, FS_ERROR_FLAG_ALL);
}
int32_t c2_pack_size(const char *path, uint32_t *size) {
    FSCmdBlock cmd; FSFileHandle file; FSStat stat;
    OSLockMutex(&mutex); FSInitCmdBlock(&cmd);
    FSStatus status = open_file(path, &cmd, &file);
    if (status == FS_STATUS_OK) {
        status = FSGetStatFile(&client, &cmd, file, &stat, FS_ERROR_FLAG_ALL);
        FSStatus closed = FSCloseFile(&client, &cmd, file, FS_ERROR_FLAG_ALL);
        if (status == FS_STATUS_OK && closed != FS_STATUS_OK) status = closed;
        if (status == FS_STATUS_OK) *size = stat.size;
    }
    OSUnlockMutex(&mutex); return status;
}
int32_t c2_pack_read(const char *path, uint32_t offset, uint8_t *out, uint32_t len) {
    FSCmdBlock cmd; FSFileHandle file;
    OSLockMutex(&mutex); FSInitCmdBlock(&cmd);
    FSStatus status = open_file(path, &cmd, &file);
    if (status != FS_STATUS_OK) { OSUnlockMutex(&mutex); return status; }
    status = FSSetPosFile(&client, &cmd, file, offset, FS_ERROR_FLAG_ALL);
    uint32_t done = 0;
    while (status == FS_STATUS_OK && done < len) {
        uint32_t n = len - done; if (n > sizeof(buffer)) n = sizeof(buffer);
        FSStatus got = FSReadFile(&client, &cmd, buffer, 1, n, file, 0, FS_ERROR_FLAG_ALL);
        if (got < 0) { status = got; break; }
        if ((uint32_t)got != n) { status = -1001; break; }
        memcpy(out + done, buffer, n); done += n;
    }
    FSStatus closed = FSCloseFile(&client, &cmd, file, FS_ERROR_FLAG_ALL);
    if (status == FS_STATUS_OK && closed != FS_STATUS_OK) status = closed;
    OSUnlockMutex(&mutex); return status;
}
int main(void) {
    int mounted = 0, added = 0;
    WHBProcInit(); WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U SD pack probe"); WHBLogPrint("Running pack checks..."); WHBLogConsoleDraw();
    if (!WHBMountSdCard()) { WHBLogPrint("SD mount: FAIL"); goto done; }
    mounted = 1; WHBLogPrint("SD mount: PASS");
    FSStatus status = FSAddClient(&client, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) { WHBLogPrintf("FS client: FAIL (%d)", status); goto done; }
    added = 1; OSInitMutex(&mutex);
    uint32_t result = c2_pack_run();
    WHBLogPrint(result & 1 ? "Open SD pack: PASS" : "Open SD pack: FAIL");
    WHBLogPrint(result & 2 ? "Pack index: PASS" : "Pack index: FAIL");
    WHBLogPrint(result & 4 ? "Packed TGA: PASS" : "Packed TGA: FAIL");
    WHBLogPrint(result & 8 ? "Multi-chunk read: PASS" : "Multi-chunk read: FAIL");
    WHBLogPrint(result & 16 ? "Missing entry: PASS" : "Missing entry: FAIL");
    WHBLogPrint(result & 32 ? "Missing pack: PASS" : "Missing pack: FAIL");
done:
    if (added) { FSStatus s = FSDelClient(&client, FS_ERROR_FLAG_ALL); if (s != FS_STATUS_OK) WHBLogPrintf("Client removal: FAIL (%d)", s); }
    if (mounted && !WHBUnmountSdCard()) WHBLogPrint("Unmount: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); }
    WHBLogConsoleFree(); WHBProcShutdown(); return 0;
}
