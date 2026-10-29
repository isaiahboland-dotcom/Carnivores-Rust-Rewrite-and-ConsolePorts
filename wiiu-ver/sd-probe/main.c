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

extern uint32_t c2_sd_parse(const uint8_t *, size_t);
void *c2_sd_alloc(size_t size, size_t align) {
    if (align < sizeof(void *)) align = sizeof(void *);
    return memalign(align, size);
}
void c2_sd_free(void *ptr) { free(ptr); }
void c2_sd_panic(void) { OSFatal("Carnivores Wii U: Rust SD probe panic"); for (;;) {} }

static FSClient client;
static FSCmdBlock cmd;
static uint8_t data[64] __attribute__((aligned(64)));
static const uint8_t tail[8] = {0x34, 0x12, 0xcd, 0xab, 0x02, 0x01, 0x78, 0x56};

int main(void) {
    char path[256];
    FSFileHandle file;
    FSStatus status;
    int mounted = 0, added = 0, opened = 0;
    WHBProcInit();
    WHBLogConsoleInit();
    WHBLogPrint("Carnivores Wii U SD and parser probe");
    WHBLogPrint("Running SD checks...");
    WHBLogConsoleDraw();
    if (!WHBMountSdCard()) { WHBLogPrint("SD mount: FAIL (check SD setup)"); goto done; }
    mounted = 1;
    WHBLogPrint("SD mount: PASS");
    status = FSAddClient(&client, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) { WHBLogPrintf("FS client: FAIL (%d)", status); goto done; }
    added = 1;
    FSInitCmdBlock(&cmd);
    if (snprintf(path, sizeof(path), "%s/wiiu/apps/carnivores-probe/fixture.tga", WHBGetSdCardMountPath()) >= (int)sizeof(path)) {
        WHBLogPrint("Fixture path too long"); goto done;
    }
    status = FSOpenFile(&client, &cmd, path, "r", &file, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) { WHBLogPrintf("Open fixture: FAIL (%d)", status); goto done; }
    opened = 1;
    WHBLogPrint("Open fixture: PASS");
    status = FSReadFile(&client, &cmd, data, 1, 26, file, 0, FS_ERROR_FLAG_ALL);
    if (status != 26) { WHBLogPrintf("Read 26 bytes: FAIL (%d)", status); goto done; }
    WHBLogPrint("Read 26 bytes: PASS");
    WHBLogPrint(c2_sd_parse(data, 26) ? "Rust TGA parse: PASS" : "Rust TGA parse: FAIL");
    status = FSSetPosFile(&client, &cmd, file, 18, FS_ERROR_FLAG_ALL);
    if (status != FS_STATUS_OK) { WHBLogPrintf("Seek: FAIL (%d)", status); goto done; }
    status = FSReadFile(&client, &cmd, data, 1, 8, file, 0, FS_ERROR_FLAG_ALL);
    if (status != 8) { WHBLogPrintf("Seek read: FAIL (%d)", status); goto done; }
    WHBLogPrint(memcmp(data, tail, 8) == 0 ? "Seek and read: PASS" : "Seek and read: FAIL (bytes)");
done:
    if (opened) { status = FSCloseFile(&client, &cmd, file, FS_ERROR_FLAG_ALL); if (status != FS_STATUS_OK) WHBLogPrintf("Close: FAIL (%d)", status); }
    if (added) { status = FSDelClient(&client, FS_ERROR_FLAG_ALL); if (status != FS_STATUS_OK) WHBLogPrintf("Client removal: FAIL (%d)", status); }
    if (mounted && !WHBUnmountSdCard()) WHBLogPrint("Unmount: FAIL");
    WHBLogPrint("Use HOME to exit.");
    while (WHBProcIsRunning()) { WHBLogConsoleDraw(); OSSleepTicks(OSMillisecondsToTicks(16)); }
    WHBLogConsoleFree();
    WHBProcShutdown();
    return 0;
}
