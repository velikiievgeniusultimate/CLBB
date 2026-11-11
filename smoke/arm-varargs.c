#include <jni.h>
JNIEXPORT jint JNICALL Java_CLBBArmVarargs_check(JNIEnv *env, jclass cls) {
    jmethodID method = (*env)->GetStaticMethodID(env, cls, "callback", "(IDIDIDIDIDIDIDIDIDID)I");
    if (!method) return -100;
    return (*env)->CallStaticIntMethod(env, cls, method,
        11, 1.25, 22, 2.25, 33, 3.25, 44, 4.25, 55, 5.25,
        66, 6.25, 77, 7.25, 88, 8.25, 99, 9.25, 110, 10.25);
}
