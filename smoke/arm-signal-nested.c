#include <jni.h>
#include <pthread.h>
#include <signal.h>
#include <errno.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>
static volatile sig_atomic_t stage;
static pthread_t receiver;
static void resume_handler(int signo) { stage = 2; }
static void suspend_handler(int signo) {
    sigset_t wait_mask;
    sigfillset(&wait_mask);
    sigdelset(&wait_mask, SIGXCPU);
    stage = 1;
    while (stage != 2) sigsuspend(&wait_mask);
    volatile uint64_t stack_writes[32];
    for (int i=0; i<32; ++i) stack_writes[i] = 0x12345678 + i;
    stage = stack_writes[31] == 0x12345697 ? 3 : -1;
}
static void *sender(void *unused) {
    while (stage != 1) usleep(1000);
    return (void *)(intptr_t)pthread_kill(receiver, SIGXCPU);
}
JNIEXPORT jint JNICALL Java_CLBBArmSignalNested_check(JNIEnv *env, jclass cls) {
    struct sigaction action, previous_suspend, previous_resume;
    memset(&action, 0, sizeof action);
    sigfillset(&action.sa_mask);
    action.sa_handler = suspend_handler;
    if (sigaction(SIGPWR, &action, &previous_suspend)) return -1000-errno;
    action.sa_handler = resume_handler;
    if (sigaction(SIGXCPU, &action, &previous_resume)) return -2000-errno;
    receiver = pthread_self();
    stage = 0;
    pthread_t thread;
    if (pthread_create(&thread, 0, sender, 0)) return -101;
    raise(SIGPWR);
    void *status;
    pthread_join(thread, &status);
    sigaction(SIGPWR, &previous_suspend, 0);
    sigaction(SIGXCPU, &previous_resume, 0);
    return !status && stage == 3 ? SIGPWR : -102;
}
