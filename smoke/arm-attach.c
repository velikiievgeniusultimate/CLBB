#include <jni.h>
#include <pthread.h>
#include <stdint.h>
static JavaVM *vm;
static void *worker(void *unused) {
    JNIEnv *env = 0;
    jint status = (*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6);
    int result = 0;
    if (status != JNI_EDETACHED) return (void *)(intptr_t)(1000 + status);
    result |= 1;
    if (env == 0) result |= 2;
    if ((*vm)->AttachCurrentThread(vm, (void **)&env, 0) != JNI_OK || !env)
        return (void *)(intptr_t)result;
    result |= 4;
    if ((*env)->GetVersion(env) >= JNI_VERSION_1_6) result |= 8;
    JNIEnv *again = 0;
    if ((*vm)->GetEnv(vm, (void **)&again, JNI_VERSION_1_6) == JNI_OK && again == env)
        result |= 16;
    if ((*vm)->DetachCurrentThread(vm) == JNI_OK) result |= 32;
    again = 0;
    if ((*vm)->GetEnv(vm, (void **)&again, JNI_VERSION_1_6) == JNI_EDETACHED)
        result |= 64;
    return (void *)(intptr_t)result;
}
JNIEXPORT jint JNICALL Java_CLBBArmAttach_check(JNIEnv *env, jclass cls) {
    if ((*env)->GetJavaVM(env, &vm) != JNI_OK) return -100;
    JNIEnv *bad_version = (JNIEnv *)(uintptr_t)1;
    if ((*vm)->GetEnv(vm, (void **)&bad_version, 0x7fffffff) != JNI_EVERSION)
        return -103;
    if (bad_version != 0) return -104;
    pthread_t thread;
    if (pthread_create(&thread, 0, worker, 0)) return -101;
    void *result = 0;
    if (pthread_join(thread, &result)) return -102;
    return (jint)(intptr_t)result;
}
