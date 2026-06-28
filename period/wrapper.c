/*
 * Fast-path wrapper for period.exe.
 *
 * For trivial programs such as `show "Hello, World!".` this executable
 * prints the output directly and exits without loading the full Rust
 * interpreter.
 *
 * For other inputs it looks for a cached JIT DLL in %TEMP%\period_c_cache\.
 * If found, the DLL is loaded in-process and its exported period_run()
 * function is called, avoiding the cost of spawning the Rust binary and
 * another child process. If no cached DLL exists, the wrapper falls back
 * to period-core.exe, which will compile and run the program and create
 * the cache for next time.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <io.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

static char core_path[MAX_PATH];

static void find_core_exe(void) {
    DWORD len = GetModuleFileNameA(NULL, core_path, MAX_PATH);
    if (len == 0 || len >= MAX_PATH) {
        strcpy(core_path, "period-core.exe");
        return;
    }
    char *slash = strrchr(core_path, '\\');
    char *name = slash ? slash + 1 : core_path;
    strcpy(name, "period-core.exe");
}

static int run_core(int argc, char *argv[]) {
    find_core_exe();

    char cmdline[8192];
    int pos = snprintf(cmdline, sizeof(cmdline), "\"%s\"", core_path);
    for (int i = 1; i < argc; i++) {
        pos += snprintf(cmdline + pos, sizeof(cmdline) - pos, " %s", argv[i]);
        if (pos >= (int)sizeof(cmdline)) {
            fprintf(stderr, "period: command line too long\n");
            return 1;
        }
    }

    STARTUPINFOA si = { sizeof(si) };
    PROCESS_INFORMATION pi = { 0 };

    if (!CreateProcessA(core_path, cmdline, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi)) {
        fprintf(stderr, "period: could not run %s\n", core_path);
        return 1;
    }

    WaitForSingleObject(pi.hProcess, INFINITE);
    DWORD code = 1;
    GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hProcess);
    CloseHandle(pi.hThread);
    return (int)code;
}

/* Returns 1 and prints the literal if the source is only `show "...".` */
static int try_fast_show(const char *src) {
    const char *s = src;

    while (*s == ' ' || *s == '\t' || *s == '\r' || *s == '\n') s++;

    if (strncmp(s, "show", 4) != 0) return 0;
    s += 4;

    while (*s == ' ' || *s == '\t') s++;
    if (*s != '"') return 0;
    s++;

    const char *end = strrchr(s, '"');
    if (!end) return 0;

    const char *after = end + 1;
    while (*after == ' ' || *after == '\t' || *after == '\r' || *after == '\n') after++;
    if (after[0] != '.' || after[1] != '\0') return 0;

    fwrite(s, 1, end - s, stdout);
    putchar('\n');
    return 1;
}

static uint64_t fnv1a_64(const unsigned char *data, size_t len) {
    uint64_t hash = 0xcbf29ce484222325ULL;
    for (size_t i = 0; i < len; i++) {
        hash ^= (uint64_t)data[i];
        hash *= 0x100000001b3ULL;
    }
    return hash;
}

static int run_cached_dll(const unsigned char *data, size_t len, int *out_code) {
    uint64_t hash = fnv1a_64(data, len);

    char temp[MAX_PATH];
    DWORD temp_len = GetTempPathA(MAX_PATH, temp);
    if (temp_len == 0 || temp_len >= MAX_PATH) return 0;

    char dll_path[MAX_PATH];
    snprintf(dll_path, sizeof(dll_path), "%speriod_c_cache\\period_%016llx.dll", temp, hash);

    DWORD attribs = GetFileAttributesA(dll_path);
    if (attribs == INVALID_FILE_ATTRIBUTES || (attribs & FILE_ATTRIBUTE_DIRECTORY)) return 0;

    HMODULE h = LoadLibraryA(dll_path);
    if (!h) return 0;

    typedef int (*period_run_t)(void);
    period_run_t run = (period_run_t)GetProcAddress(h, "period_run");
    if (!run) {
        FreeLibrary(h);
        return 0;
    }

    *out_code = run();
    FreeLibrary(h);
    return 1;
}

int main(int argc, char *argv[]) {
    if (argc != 2) {
        return run_core(argc, argv);
    }

    if (strcmp(argv[1], "--version") == 0 || strcmp(argv[1], "-v") == 0 ||
        strcmp(argv[1], "--lsp") == 0) {
        return run_core(argc, argv);
    }

    HANDLE file = CreateFileA(
        argv[1],
        GENERIC_READ,
        FILE_SHARE_READ,
        NULL,
        OPEN_EXISTING,
        FILE_ATTRIBUTE_NORMAL,
        NULL
    );
    if (file == INVALID_HANDLE_VALUE) {
        return run_core(argc, argv);
    }

    DWORD size = GetFileSize(file, NULL);
    if (size == INVALID_FILE_SIZE || size > 1024 * 1024) {
        CloseHandle(file);
        return run_core(argc, argv);
    }

    unsigned char *buf = (unsigned char *)malloc(size + 1);
    if (!buf) {
        CloseHandle(file);
        return run_core(argc, argv);
    }

    DWORD read = 0;
    ReadFile(file, buf, size, &read, NULL);
    CloseHandle(file);
    buf[read] = '\0';

    int result;
    int cached_code = 0;
    if (try_fast_show((const char *)buf)) {
        result = 0;
    } else if (run_cached_dll(buf, read, &cached_code)) {
        result = cached_code;
    } else {
        result = run_core(argc, argv);
    }

    free(buf);
    return result;
}
