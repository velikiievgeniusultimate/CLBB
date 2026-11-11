#include <jni.h>
JNIEXPORT jint JNICALL Java_CLBBArmSmoke_answer(JNIEnv *env, jclass cls) {
    (void)env;
    (void)cls;
    return 42;
}
JNIEXPORT jint JNICALL Java_CLBBArmThreads_answer(JNIEnv *env, jclass cls) {
    (void)env;
    (void)cls;
    return 42;
}
