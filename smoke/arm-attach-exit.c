#include <jni.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>

static JavaVM *vm;
static pthread_key_t cleanup_key;

struct worker_state {
    int explicit_detach;
    int repeat_attach;
    int result;
    JNIEnv *attached_env;
};

/* These are guest pthread destructors. JNI must still work here; the host
 * attachment must be released only after guest cleanup has finished. */
static void guest_cleanup(void *pointer) {
    struct worker_state *state = pointer;
    JNIEnv *env = 0;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK ||
        !env || env != state->attached_env)
        return;
    state->result |= 16;
    if ((*env)->GetVersion(env) < JNI_VERSION_1_6)
        return;
    state->result |= 32;
    if (state->explicit_detach) {
        if ((*vm)->DetachCurrentThread(vm) != JNI_OK)
            return;
        env = (JNIEnv *)(uintptr_t)1;
        if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_EDETACHED || env)
            return;
    }
    /* Without explicit detach, leave ownership to the bridge's exit hook. */
    state->result |= 64;
}

static void *worker(void *pointer) {
    struct worker_state *state = pointer;
    JNIEnv *env = (JNIEnv *)(uintptr_t)1;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_EDETACHED || env)
        return pointer;
    state->result |= 1;
    if ((*vm)->AttachCurrentThread(vm, (void **)&env, 0) != JNI_OK || !env)
        return pointer;
    state->attached_env = env;
    state->result |= 2;
    if ((*env)->GetVersion(env) < JNI_VERSION_1_6)
        return pointer;
    state->result |= 4;
    if (pthread_setspecific(cleanup_key, state))
        return pointer;
    state->result |= 8;
    if (state->repeat_attach) {
        JNIEnv *again = 0;
        if ((*vm)->AttachCurrentThread(vm, (void **)&again, 0) != JNI_OK || again != env)
            return pointer;
        state->result |= 128;
    }
    return pointer;
}

/* Return bits 1/2/4 for the three guest pthread exit policies, and bit 8 for
 * preserving the Java parent thread's existing attachment. */
JNIEXPORT jint JNICALL Java_CLBBArmAttachExit_check(JNIEnv *env, jclass cls) {
    if ((*env)->GetJavaVM(env, &vm) != JNI_OK || !vm)
        return -100;
    JNIEnv *parent = 0;
    if ((*vm)->GetEnv(vm, (void **)&parent, JNI_VERSION_1_6) != JNI_OK || parent != env)
        return -101;
    if (pthread_key_create(&cleanup_key, guest_cleanup))
        return -102;
    int result = 0;
    for (int branch = 0; branch < 3; ++branch) {
        struct worker_state state = {
            .explicit_detach = branch == 1,
            .repeat_attach = branch == 2,
            .result = 0,
            .attached_env = 0,
        };
        pthread_t thread;
        if (pthread_create(&thread, 0, worker, &state)) {
            pthread_key_delete(cleanup_key);
            return -110 - branch;
        }
        void *returned = 0;
        if (pthread_join(thread, &returned)) {
            /* A live worker may still reference state on this stack. */
            abort();
        }
        int expected = branch == 2 ? 255 : 127;
        if (returned != &state || state.result != expected) {
            pthread_key_delete(cleanup_key);
            return -1000 - branch * 256 - state.result;
        }
        result |= 1 << branch;
    }
    pthread_key_delete(cleanup_key);
    parent = 0;
    if ((*vm)->GetEnv(vm, (void **)&parent, JNI_VERSION_1_6) == JNI_OK &&
        parent == env && (*env)->GetVersion(env) >= JNI_VERSION_1_6)
        result |= 8;
    return result;
}

/* This entry is called by a Java-created Thread. Repeated attach must not
 * transfer ownership of its pre-existing ART attachment to the bridge. */
JNIEXPORT jint JNICALL Java_CLBBArmAttachExit_javaOwnedProbe(JNIEnv *env, jclass cls) {
    JavaVM *parent_vm = 0;
    if ((*env)->GetJavaVM(env, &parent_vm) != JNI_OK || parent_vm != vm)
        return -200;
    int result = 0;
    JNIEnv *current = 0;
    if ((*vm)->GetEnv(vm, (void **)&current, JNI_VERSION_1_6) != JNI_OK || current != env)
        return result;
    result |= 1;
    current = 0;
    if ((*vm)->AttachCurrentThread(vm, (void **)&current, 0) != JNI_OK || current != env)
        return result;
    result |= 2;
    current = 0;
    if ((*vm)->AttachCurrentThread(vm, (void **)&current, 0) != JNI_OK || current != env)
        return result;
    result |= 4;
    current = 0;
    if ((*vm)->GetEnv(vm, (void **)&current, JNI_VERSION_1_6) == JNI_OK && current == env)
        result |= 8;
    if ((*env)->GetVersion(env) >= JNI_VERSION_1_6)
        result |= 16;
    return result;
}

JNIEXPORT jboolean JNICALL Java_CLBBArmAttachExit_stillAttached(JNIEnv *env, jclass cls) {
    JNIEnv *current = 0;
    return (*vm)->GetEnv(vm, (void **)&current, JNI_VERSION_1_6) == JNI_OK &&
           current == env && (*env)->GetVersion(env) >= JNI_VERSION_1_6;
}
