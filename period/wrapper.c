/*
 * Fast-path wrapper for period.exe.
 *
 * For trivial programs such as `show "Hello, World!".` this executable
 * prints the output directly and exits without loading the full Rust
 * interpreter. All other programs are forwarded to period-core.exe.
 */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <io.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

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

    for (const char *p = s; p < end; p++) {
        if (*p == '{' || *p == '}') return 0;
    }

    fwrite(s, 1, end - s, stdout);
    putchar('\n');
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
    if (try_fast_show((const char *)buf)) {
        result = 0;
    } else {
        result = run_core(argc, argv);
    }

    free(buf);
    return result;
}
