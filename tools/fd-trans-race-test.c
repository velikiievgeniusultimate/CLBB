/* Exercise the actual embedded-QEMU fd-trans.h, not a model of its table.
 * Build with glib-2.0, pthreads, and optionally ASan/UBSan. */
#define _GNU_SOURCE
#include <assert.h>
#include <glib.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/wait.h>
#include <unistd.h>

typedef intptr_t abi_long;
typedef uintptr_t abi_ulong;
static void *test_renew(void *memory, size_t bytes);
#undef g_renew
#define g_renew(type, memory, count) \
    ((type *)test_renew((memory), sizeof(type) * (count)))
#include "../src/nb-qemu/qemu/linux-user/fd-trans.h"

TargetFdTrans **target_fd_trans;
unsigned int target_fd_max;
GMutex target_fd_trans_mutex;
static atomic_int window_armed, window_entered, reader_entered;
static atomic_bool churn_stop;

static void *test_renew(void *memory, size_t bytes)
{
    if (atomic_exchange(&window_armed, 0)) {
        /* Readers must not observe the table during an allocation change. */
        assert(!g_mutex_trylock(&target_fd_trans_mutex));
        atomic_store(&window_entered, 1);
        while (!atomic_load(&reader_entered)) {
            sched_yield();
        }
    }
    return g_realloc(memory, bytes);
}

static abi_long data_a(void *data, size_t length)
{
    (void)data;
    return length;
}

static abi_long data_b(void *data, size_t length)
{
    (void)data;
    return length + 1;
}

static abi_long data_reenter(void *data, size_t length)
{
    (void)data;
    fd_trans_unregister(7);
    (void)fd_trans_host_to_target_data(7);
    return length + 2;
}

static abi_long addr_a(void *data, abi_ulong address, socklen_t length)
{
    (void)data;
    (void)address;
    return length;
}

static TargetFdTrans trans_a = {data_a, data_b, addr_a};
static TargetFdTrans trans_b = {data_b, data_a, addr_a};
static TargetFdTrans trans_reenter = {data_reenter, data_reenter, addr_a};

static void *window_reader(void *unused)
{
    TargetFdDataFunc callback;
    (void)unused;
    while (!atomic_load(&window_entered)) {
        sched_yield();
    }
    atomic_store(&reader_entered, 1);
    callback = fd_trans_host_to_target_data(168);
    assert(callback == NULL);
    return NULL;
}

static void test_growth_window(void)
{
    pthread_t reader;
    fd_trans_register(127, &trans_a);
    assert(target_fd_max == 128);
    assert(pthread_create(&reader, NULL, window_reader, NULL) == 0);
    atomic_store(&window_armed, 1);
    fd_trans_register(169, &trans_b);
    assert(pthread_join(reader, NULL) == 0);
    assert(target_fd_max == 192);
    assert(fd_trans_host_to_target_data(169) == data_b);
}

static void test_semantics(void)
{
    TargetFdDataFunc callback;
    unsigned int before = target_fd_max;
    fd_trans_register(-1, &trans_a);
    assert(target_fd_max == before);
    assert(fd_trans_host_to_target_data(-1) == NULL);
    assert(fd_trans_target_to_host_data(-1) == NULL);
    assert(fd_trans_target_to_host_addr(-1) == NULL);
    fd_trans_register(5, &trans_a);
    assert(fd_trans_host_to_target_data(5) == data_a);
    assert(fd_trans_target_to_host_data(5) == data_b);
    assert(fd_trans_target_to_host_addr(5) == addr_a);
    fd_trans_dup(5, 5); /* dup2(fd, fd) must retain its translation. */
    assert(fd_trans_host_to_target_data(5) == data_a);
    fd_trans_dup(5, 257);
    assert(fd_trans_host_to_target_data(257) == data_a);
    fd_trans_dup(-1, 257);
    assert(fd_trans_host_to_target_data(257) == NULL);
    fd_trans_dup(5, -1);
    assert(fd_trans_host_to_target_data(5) == data_a);
    fd_trans_register(6, &trans_reenter);
    callback = fd_trans_host_to_target_data(6);
    fd_trans_unregister(6);
    assert(fd_trans_host_to_target_data(6) == NULL);
    /* Copy remains callable after clear and may reenter the table. */
    assert(callback(NULL, 4) == 6);
}

static void check_data_callback(TargetFdDataFunc callback)
{
    if (callback) {
        abi_long result;
        assert(callback == data_a || callback == data_b ||
               callback == data_reenter);
        result = callback(NULL, 4);
        assert(result >= 4 && result <= 6);
    }
}

static void *churn(void *argument)
{
    uintptr_t lane = (uintptr_t)argument;
    uint32_t random = 0x9e3779b9U ^ lane;
    for (unsigned int i = 0; i < 80000 && !atomic_load(&churn_stop); ++i) {
        int fd, other;
        random = random * 1664525U + 1013904223U;
        /* Grow repeatedly while the other threads read and duplicate. */
        fd = (random >> 8) % (256U + i / 2U);
        other = (fd + 193) % 65536;
        switch ((i + lane) % 5) {
        case 0:
            fd_trans_register(fd, (i & 1) ? &trans_a : &trans_b);
            break;
        case 1:
            fd_trans_dup(fd, other);
            break;
        case 2:
            fd_trans_unregister(fd);
            break;
        default:
            check_data_callback(fd_trans_host_to_target_data(fd));
            check_data_callback(fd_trans_target_to_host_data(fd));
            {
                TargetFdAddrFunc callback = fd_trans_target_to_host_addr(fd);
                assert(callback == NULL || callback == addr_a);
            }
        }
    }
    return NULL;
}

static void test_concurrency_and_fork(void)
{
    pthread_t threads[6];
    for (uintptr_t i = 0; i < G_N_ELEMENTS(threads); ++i) {
        assert(pthread_create(&threads[i], NULL, churn, (void *)i) == 0);
    }
    for (int i = 0; i < 12; ++i) {
        pid_t child;
        int status;
        fd_trans_fork_start();
        child = fork();
        assert(child >= 0);
        fd_trans_fork_end();
        if (child == 0) {
            alarm(5);
            fd_trans_register(65535, &trans_a);
            assert(fd_trans_host_to_target_data(65535) == data_a);
            fd_trans_dup(65535, 65534);
            assert(fd_trans_host_to_target_data(65534) == data_a);
            fd_trans_unregister(65535);
            _exit(0);
        }
        assert(waitpid(child, &status, 0) == child);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }
    for (unsigned int i = 0; i < G_N_ELEMENTS(threads); ++i) {
        assert(pthread_join(threads[i], NULL) == 0);
    }
}

int main(int argc, char **argv)
{
    (void)argc;
    (void)argv;
    test_growth_window();
    test_semantics();
    test_concurrency_and_fork();
    g_mutex_lock(&target_fd_trans_mutex);
    g_free(target_fd_trans);
    target_fd_trans = NULL;
    target_fd_max = 0;
    g_mutex_unlock(&target_fd_trans_mutex);
    g_mutex_clear(&target_fd_trans_mutex);
    puts("fd-trans: growth snapshot, bounds/dup/clear/reentry, 480000 concurrent operations and 12 forks PASS");
    return 0;
}
