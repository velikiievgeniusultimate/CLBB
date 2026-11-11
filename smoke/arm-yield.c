#include <jni.h>
__attribute__((noinline)) static int yielding(int value) {
    __asm__ volatile("yield" ::: "memory");
    return value + 17;
}
JNIEXPORT jint JNICALL Java_CLBBArmYield_check(JNIEnv *env, jclass cls) {
    volatile int value=15;
    for (int i=0;i<32;++i) value=yielding(value);
    return value;
}
