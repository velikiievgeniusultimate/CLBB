#define _GNU_SOURCE
#include <glib.h>
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <spawn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

static void child_setup(void *unused) {
    (void)unused;
    sigset_t mask;
    sigemptyset(&mask); sigaddset(&mask, SIGUSR1);
    assert(sigprocmask(SIG_BLOCK, &mask, NULL) == 0);
}

static int helper(int argc, char **argv) {
    char byte, executable[4096];
    assert(argc == 3 && !strcmp(argv[2], "opaque argument"));
    assert(!strcmp(argv[0], getenv("CLBB_EXPECT_ARGV0")));
    assert(!strcmp(getenv("CLBB_TEST_TOKEN"), "opaque environment"));
    assert(read(19, &byte, 1) == 1 && byte == 'X');
    sigset_t mask; assert(sigprocmask(SIG_SETMASK, NULL, &mask) == 0);
    assert(sigismember(&mask, SIGUSR1) == 1);
    ssize_t size = readlink("/proc/self/exe", executable, sizeof(executable) - 1);
    assert(size > 0); executable[size] = 0;
    assert(!strcmp(executable, getenv("CLBB_EXPECT_EXECUTABLE")));
    puts("FAKE_HELPER_OK");
    return 0;
}

int main(int argc, char **argv) {
    if (argc > 1 && !strcmp(argv[1], "--helper")) return helper(argc, argv);
    assert(argc == 3);
    const char *directory = argv[1], *other = argv[2];
    const char *names[] = {"WebKitNetworkProcess", "WebKitWebProcess", "WebKitGPUProcess", "OtherProcess"};
    int channel[2]; assert(pipe(channel) == 0);
    assert(dup2(channel[0], 19) == 19);
    for (unsigned n = 0; n < 4; ++n) {
        char old[4096], mapped[4096], arg0[4200], expected[4200];
        snprintf(old, sizeof old, "%s/%s", n == 3 ? other : "/usr/lib/webkitgtk-6.0", names[n]);
        snprintf(mapped, sizeof mapped, "%s/%s", n == 3 ? other : directory, names[n]);
        snprintf(arg0, sizeof arg0, "CLBB_EXPECT_ARGV0=%s", old);
        snprintf(expected, sizeof expected, "CLBB_EXPECT_EXECUTABLE=%s", mapped);
        char *arguments[] = {old, "--helper", "opaque argument", NULL};
        char *environment[] = {arg0, expected, "CLBB_TEST_TOKEN=opaque environment", NULL};
        assert(setenv("CLBB_EXPECT_ARGV0", old, 1) == 0);
        assert(setenv("CLBB_EXPECT_EXECUTABLE", mapped, 1) == 0);
        assert(setenv("CLBB_TEST_TOKEN", "opaque environment", 1) == 0);
        for (unsigned api = 0; api < 6; ++api) {
            assert(write(channel[1], "X", 1) == 1);
            int status;
            if (api == 5) {
                GError *error = NULL;
                gchar *output = NULL;
                assert(g_spawn_sync(NULL, arguments, environment, G_SPAWN_LEAVE_DESCRIPTORS_OPEN,
                    child_setup, NULL, &output, NULL, &status, &error));
                assert(error == NULL && WIFEXITED(status) && WEXITSTATUS(status) == 0);
                assert(output && !strcmp(output, "FAKE_HELPER_OK\n"));
                g_free(output);
            } else if (api >= 3) {
                posix_spawnattr_t attrs; assert(posix_spawnattr_init(&attrs) == 0);
                sigset_t mask; sigemptyset(&mask); sigaddset(&mask, SIGUSR1);
                assert(posix_spawnattr_setsigmask(&attrs, &mask) == 0);
                assert(posix_spawnattr_setflags(&attrs, POSIX_SPAWN_SETSIGMASK) == 0);
                posix_spawn_file_actions_t actions;
                assert(posix_spawn_file_actions_init(&actions) == 0);
                assert(posix_spawn_file_actions_adddup2(&actions, channel[0], 19) == 0);
                pid_t pid;
                int result = api == 3 ? posix_spawn(&pid, old, &actions, &attrs, arguments, environment)
                    : posix_spawnp(&pid, old, &actions, &attrs, arguments, environment);
                assert(result == 0);
                assert(waitpid(pid, &status, 0) == pid && WIFEXITED(status) && WEXITSTATUS(status) == 0);
                posix_spawn_file_actions_destroy(&actions); posix_spawnattr_destroy(&attrs);
            } else {
                pid_t pid = fork(); assert(pid >= 0);
                if (pid == 0) {
                    child_setup(NULL);
                    if (api == 0) execv(old, arguments);
                    else if (api == 1) execve(old, arguments, environment);
                    else execvp(old, arguments);
                    _exit(120);
                }
                assert(waitpid(pid, &status, 0) == pid && WIFEXITED(status) && WEXITSTATUS(status) == 0);
            }
        }
    }
    puts("CLBB_WEBKIT_SPAWN_OK=24");
    return 0;
}
