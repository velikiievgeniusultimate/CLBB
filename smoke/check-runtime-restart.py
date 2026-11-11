#!/usr/bin/env python3
"""Exercise the production exec/restart code without starting the game."""
import os
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
fixture = r'''
#include "restart.h"
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    atl_restart_init(argv);
    assert(argc == 4);
    assert(!strcmp(argv[1], "path with spaces/game.apk"));
    assert(!strcmp(argv[2], "Кагура"));
    assert(!strcmp(argv[3], "--literal=$value;`text`"));
    if (getenv("TEST_LIMIT")) {
        assert(!atl_exec_restart());
        puts("RESTART_LIMIT_OK");
        return 0;
    }
    if (getenv("TEST_BEFORE_PID")) {
        assert(getpid() == atoi(getenv("TEST_BEFORE_PID")));
        errno = 0;
        assert(fcntl(atoi(getenv("TEST_BEFORE_FD")), F_GETFD) == -1);
        assert(errno == EBADF);
        assert(!strcmp(getenv("CLBB_RESTART_COUNT"), "1"));
        assert(fcntl(STDOUT_FILENO, F_GETFD) >= 0);
        puts("RESTART_EXEC_OK same_pid=1 argv_preserved=1 descriptors_closed=1");
        return 0;
    }
    int fd = open("/dev/null", O_RDONLY);
    assert(fd >= 3);
    char value[32];
    snprintf(value, sizeof value, "%d", getpid());
    setenv("TEST_BEFORE_PID", value, 1);
    snprintf(value, sizeof value, "%d", fd);
    setenv("TEST_BEFORE_FD", value, 1);
    assert(atl_exec_restart()); /* successful exec never returns */
    return 1;
}
'''
with tempfile.TemporaryDirectory(prefix="clbb-restart-") as directory:
    directory = pathlib.Path(directory)
    source = directory / "fixture.c"
    binary = directory / "restart-fixture"
    source.write_text(fixture)
    flags = subprocess.check_output(["pkg-config", "--cflags", "--libs", "glib-2.0"], text=True).split()
    subprocess.run(["cc", "-Wall", "-Wextra", "-Werror", "-I", str(root / "src/atl/src/main-executable"),
                    str(source), str(root / "src/atl/src/main-executable/restart.c"), "-o", str(binary), *flags], check=True)
    args = [str(binary), "path with spaces/game.apk", "Кагура", "--literal=$value;`text`"]
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(("TEST_", "CLBB_RESTART_"))}
    subprocess.run(args, env=env, check=True, timeout=10)
    import time
    env.update(TEST_LIMIT="1", CLBB_RESTART_COUNT="5", CLBB_RESTART_WINDOW=str(int(time.monotonic())))
    subprocess.run(args, env=env, check=True, timeout=10)
