#include <jni.h>
#include <signal.h>
#include <setjmp.h>
#include <string.h>
#include <errno.h>
static sigjmp_buf escape;
static volatile sig_atomic_t caught;
static void handler(int signo) {
    caught = signo;
    siglongjmp(escape, 1);
}
JNIEXPORT jint JNICALL Java_CLBBArmSignal_check(JNIEnv *env, jclass cls) {
    struct sigaction action, previous;
    memset(&action, 0, sizeof action);
    action.sa_handler = handler;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGSEGV, &action, &previous)) return -1000-errno;
    caught = 0;
    if (sigsetjmp(escape, 1) == 0) {
        /* Deliberately fault inside translated guest code, then recover using
         * the guest application's own POSIX signal handler. */
        __asm__ volatile("mov x9, #0; ldr w9, [x9]" ::: "x9", "memory");
    }
    sigaction(SIGSEGV, &previous, 0);
    return caught;
}
