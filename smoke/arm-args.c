#include <jni.h>
JNIEXPORT jint JNICALL Java_CLBBArmArgs_check(JNIEnv *env, jclass cls,
    jstring a, jstring b, jint c, jstring d, jint e, jstring f, jint g,
    jstring h, jstring i, jstring j, jstring k) {
    jint result = 0;
    if (c == 17) result |= 1;
    if (e == 31) result |= 2;
    if (g == 47) result |= 4;
    if ((*env)->IsSameObject(env, a, h)) result |= 8;
    if ((*env)->IsSameObject(env, b, i)) result |= 16;
    if ((*env)->IsSameObject(env, d, j)) result |= 32;
    if ((*env)->IsSameObject(env, f, k)) result |= 64;
    return result;
}
