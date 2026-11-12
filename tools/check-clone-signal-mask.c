/* SPDX-License-Identifier: GPL-2.0-or-later */
#define _GNU_SOURCE
#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdatomic.h>
#include <stdio.h>
#include <time.h>
#include "../src/nb-qemu/qemu/linux-user/clone-signal-mask.h"

static _Thread_local volatile sig_atomic_t caught;
static atomic_int phase;
static sigset_t wanted;
static void handle(int signal) { assert(signal == SIGUSR1); caught++; }
static void await(int value) {
    struct timespec begin, now;
    assert(clock_gettime(CLOCK_MONOTONIC, &begin) == 0);
    while (atomic_load_explicit(&phase, memory_order_acquire) < value) {
        assert(clock_gettime(CLOCK_MONOTONIC, &now) == 0);
        assert(now.tv_sec - begin.tv_sec < 3);
        sched_yield();
    }
}
static void *child(void *unused) {
    (void)unused;
    sigset_t current, pending, inherited;
    assert(pthread_sigmask(SIG_SETMASK, NULL, &inherited) == 0);
    assert(sigismember(&inherited, SIGUSR1) == 1);
    atomic_store_explicit(&phase, 1, memory_order_release);
    await(2);
    /* This is the failed old clone policy: the GC-like signal remains pending. */
    assert(caught == 0 && sigpending(&pending) == 0);
    assert(sigismember(&pending, SIGUSR1) == 1);
    /* Exercise the exact helper used by production clone_func(). */
    clbb_clone_signal_mask(&wanted, &current);
    assert(pthread_sigmask(SIG_SETMASK, &current, NULL) == 0);
    assert(caught == 1);
    assert(pthread_sigmask(SIG_SETMASK, NULL, &current) == 0);
    assert(sigismember(&current, SIGUSR2) == 1);
    assert(sigismember(&current, SIGSEGV) == 0);
    assert(sigismember(&current, SIGBUS) == 0);
    atomic_store_explicit(&phase, 3, memory_order_release);
    return NULL;
}
int main(void) {
    struct sigaction action = {.sa_handler = handle};
    sigemptyset(&action.sa_mask);
    assert(sigaction(SIGUSR1, &action, NULL) == 0);
    sigemptyset(&wanted);
    sigaddset(&wanted, SIGUSR2);
    sigaddset(&wanted, SIGSEGV);
    sigaddset(&wanted, SIGBUS);
    for (int i = 0; i < 1000; i++) {
        sigset_t transient, original;
        sigfillset(&transient);
        assert(pthread_sigmask(SIG_SETMASK, &transient, &original) == 0);
        atomic_store(&phase, 0);
        pthread_t thread;
        assert(pthread_create(&thread, NULL, child, NULL) == 0);
        await(1);
        assert(pthread_kill(thread, SIGUSR1) == 0);
        atomic_store_explicit(&phase, 2, memory_order_release);
        await(3);
        assert(pthread_join(thread, NULL) == 0);
        assert(pthread_sigmask(SIG_SETMASK, &original, NULL) == 0);
    }
    puts("CLBB_CLONE_SIGNAL_MASK_OK 1000 pending signals released; guest blocking preserved; fault signals unblocked");
}
