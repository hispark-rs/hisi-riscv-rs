#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

/* Process-local SEGGER configuration, before any probe is opened. Build:
 * cc -shared -fPIC -Wall -Wextra -Werror -o guard.so jlink-no-update-guard.c -ldl
 * WS53_JLINK_LIBRARY must name the libjlinkarm.so belonging to JLinkExe.
 * This is not an interposer that bypasses licenses or alters probe firmware.
 */
__attribute__((constructor)) static void prepare_jlink(void)
{
    const char *path = getenv("WS53_JLINK_LIBRARY");
    if (!path || !*path) {
        fprintf(stderr, "GUARD requires WS53_JLINK_LIBRARY\n");
        _exit(125);
    }
    void *handle = dlopen(path, RTLD_NOW | RTLD_GLOBAL);
    if (!handle) {
        fprintf(stderr, "GUARD dlopen failed: %s\n", dlerror());
        _exit(125);
    }
    int (*command)(const char *, char *, int) = dlsym(handle, "JLINKARM_ExecCommand");
    if (!command) {
        fprintf(stderr, "GUARD missing JLINKARM_ExecCommand\n");
        _exit(125);
    }
    const char *commands[] = {"DisableAutoUpdateFW", "DisableFlashDL", "DisableFlashBPs"};
    for (unsigned i = 0; i < sizeof(commands) / sizeof(commands[0]); ++i) {
        char error[512] = {0};
        int result = command(commands[i], error, sizeof(error));
        fprintf(stderr, "GUARD %s => %d %s\n", commands[i], result, error);
        if (result < 0)
            _exit(125);
    }
}
