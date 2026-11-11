#include <jni.h>
#include <pthread.h>
#include <signal.h>
#include <errno.h>
#include <stdint.h>
#include <stdatomic.h>
#include <string.h>
#include <sched.h>

static volatile sig_atomic_t seen;
static volatile sig_atomic_t handled;
static pthread_t receiver;
static atomic_uint requested_epoch;
static atomic_int sender_stop;
static atomic_int sender_error;

static void handler(int signo) {
    seen = signo;
    ++handled;
}

static void *sender(void *unused) {
    unsigned int delivered_epoch = 0;
    while (!atomic_load_explicit(&sender_stop, memory_order_acquire)) {
        unsigned int epoch = atomic_load_explicit(&requested_epoch,
                                                   memory_order_acquire);
        if (epoch == delivered_epoch) {
            sched_yield();
            continue;
        }
        int error = pthread_kill(receiver, SIGPWR);
        if (error) {
            atomic_store_explicit(&sender_error, error, memory_order_release);
            return (void *)(intptr_t)error;
        }
        delivered_epoch = epoch;
    }
    return 0;
}

/* Returning through QEMU's stack trampoline protects its page as translated
 * code. Later signal frames must make every page in their write range writable
 * again, including the reserved context and trampoline near the frame's end.
 * Changing stack depth moves those fields across page boundaries. */
__attribute__((noinline))
static int wait_at_depth(size_t depth, unsigned int epoch,
                         const sigset_t *wait_mask) {
    volatile unsigned char padding[depth + 256];
    volatile uint64_t locals[32];
    size_t length = sizeof padding;
    for (size_t i = 0; i < length; ++i)
        padding[i] = (unsigned char)(epoch * 17u + i);
    for (size_t i = 0; i < 32; ++i)
        locals[i] = UINT64_C(0x1234567800000000) + epoch + i;

    seen = 0;
    sig_atomic_t previous_count = handled;
    atomic_store_explicit(&requested_epoch, epoch, memory_order_release);
    while (!seen) {
        int result = sigsuspend(wait_mask);
        int observed_errno = errno;
        if (result != -1 || observed_errno != EINTR)
            return -200;
    }
    if (seen != SIGPWR || handled != previous_count + 1)
        return -201;
    for (size_t i = 0; i < length; ++i) {
        if (padding[i] != (unsigned char)(epoch * 17u + i))
            return -202;
        /* Exercise writes after rt_sigreturn, not just reads of old locals. */
        padding[i] ^= 0x5a;
    }
    for (size_t i = 0; i < 32; ++i) {
        uint64_t expected = UINT64_C(0x1234567800000000) + epoch + i;
        if (locals[i] != expected)
            return -203;
        locals[i] = expected ^ UINT64_C(0xdeadbeef);
    }
    return 0;
}

JNIEXPORT jint JNICALL Java_CLBBArmSignalReturn_check(JNIEnv *env, jclass cls) {
    struct sigaction action, previous;
    memset(&action, 0, sizeof action);
    action.sa_handler = handler;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGPWR, &action, &previous)) return -1000-errno;
    sigset_t blocked, original;
    sigemptyset(&blocked);
    sigaddset(&blocked, SIGPWR);
    if (pthread_sigmask(SIG_BLOCK, &blocked, &original)) {
        sigaction(SIGPWR, &previous, 0);
        return -100;
    }
    sigset_t wait_mask = original;
    sigdelset(&wait_mask, SIGPWR);
    receiver = pthread_self();
    seen = 0;
    handled = 0;
    atomic_store(&requested_epoch, 0);
    atomic_store(&sender_stop, 0);
    atomic_store(&sender_error, 0);
    pthread_t thread;
    if (pthread_create(&thread, 0, sender, 0)) {
        sigaction(SIGPWR, &previous, 0);
        pthread_sigmask(SIG_SETMASK, &original, 0);
        return -101;
    }
    static const size_t depths[] = {
        0, 16, 240, 1008, 2032, 3056, 4080, 4096, 4112, 8176
    };
    const size_t depth_count = sizeof depths / sizeof depths[0];
    unsigned int epoch = 0;
    int failure = 0;
    for (unsigned int round = 0; round < 8 && !failure; ++round) {
        for (size_t step = 0; step < depth_count && !failure; ++step) {
            size_t index = (round & 1) ? depth_count - step - 1 : step;
            /* The second signal reuses exactly the previous trampoline page. */
            for (unsigned int repeat = 0; repeat < 2 && !failure; ++repeat)
                failure = wait_at_depth(depths[index], ++epoch, &wait_mask);
        }
    }
    atomic_store_explicit(&sender_stop, 1, memory_order_release);
    void *status;
    int join_error = pthread_join(thread, &status);
    int send_error = atomic_load_explicit(&sender_error, memory_order_acquire);
    sigaction(SIGPWR, &previous, 0);
    pthread_sigmask(SIG_SETMASK, &original, 0);
    if (failure) return failure;
    return !join_error && !status && !send_error && handled == 160 ? SIGPWR : -102;
}
