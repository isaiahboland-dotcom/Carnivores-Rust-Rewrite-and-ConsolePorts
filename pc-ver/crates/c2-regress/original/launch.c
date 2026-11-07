/* c2launch.exe GAME.EXE [ARGS]: starts the original game suspended, loads
 * c2hook.dll (beside this program) into it, then lets it run and waits.
 * The game's files are not changed. See hook.c. */
#include <windows.h>

static void say(const char *s) {
    DWORD n;
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), s, lstrlenA(s), &n, NULL);
}

/* The command line after this program's own name. */
static char *args_after_self(void) {
    char *c = GetCommandLineA();
    if (*c == '"') {
        for (c++; *c && *c != '"'; c++) {}
        if (*c) c++;
    } else {
        while (*c && *c != ' ') c++;
    }
    while (*c == ' ') c++;
    return c;
}

void start(void) {
    char dll[MAX_PATH], *cmd = args_after_self(), *slash = NULL, *p, msg[MAX_PATH + 64];
    STARTUPINFOA si;
    PROCESS_INFORMATION pi;
    void *remote, *load;
    HANDLE t;
    DWORD code = 1, i;
    if (!*cmd) {
        say("usage: c2launch GAME.EXE [ARGS]\r\n");
        ExitProcess(2);
    }
    GetModuleFileNameA(NULL, dll, MAX_PATH);
    for (p = dll; *p; p++)
        if (*p == '\\' || *p == '/') slash = p;
    lstrcpyA(slash ? slash + 1 : dll, "c2hook.dll");
    for (i = 0; i < sizeof si; i++) ((char *)&si)[i] = 0;
    si.cb = sizeof si;
    if (!CreateProcessA(NULL, cmd, NULL, NULL, FALSE, CREATE_SUSPENDED, NULL, NULL, &si, &pi)) {
        wsprintfA(msg, "c2launch: cannot start %s (error %lu)\r\n", cmd, GetLastError());
        say(msg);
        ExitProcess(2);
    }
    remote = VirtualAllocEx(pi.hProcess, NULL, MAX_PATH, MEM_COMMIT, PAGE_READWRITE);
    load = (void *)GetProcAddress(GetModuleHandleA("kernel32.dll"), "LoadLibraryA");
    WriteProcessMemory(pi.hProcess, remote, dll, (SIZE_T)lstrlenA(dll) + 1, NULL);
    t = CreateRemoteThread(pi.hProcess, NULL, 0, (LPTHREAD_START_ROUTINE)load, remote, 0, NULL);
    if (!t) {
        say("c2launch: cannot load c2hook.dll into the game\r\n");
        TerminateProcess(pi.hProcess, 2);
        ExitProcess(2);
    }
    WaitForSingleObject(t, INFINITE);
    GetExitCodeThread(t, &code);
    wsprintfA(msg, "c2launch: %s %s\r\n", dll, code ? "loaded" : "FAILED to load");
    say(msg);
    if (!code) {
        TerminateProcess(pi.hProcess, 2);
        ExitProcess(2);
    }
    ResumeThread(pi.hThread);
    WaitForSingleObject(pi.hProcess, INFINITE);
    GetExitCodeProcess(pi.hProcess, &code);
    ExitProcess(code);
}
