#include <jni.h>

typedef int (*operation)(int);

static int add_one(int value) { return value + 1; }
static int add_two(int value) { return value + 2; }
static int add_three(int value) { return value + 3; }
static int add_four(int value) { return value + 4; }

static const operation targets[] = {add_one, add_two, add_three, add_four};

/* Both call sites see mono-, bi- and polymorphic traffic during the same run.
 * Keep the BLR's return distinct from the BR tail call. */
__attribute__((noinline)) static int indirect_call(operation op, int input) {
    return op(input) + 7;
}

__attribute__((noinline)) static int indirect_tail(operation op, int input) {
    return op(input);
}

JNIEXPORT jboolean JNICALL Java_CLBBArmIndirect_check(JNIEnv *env, jclass cls, jint lane) {
    (void)env;
    (void)cls;
    for (int i = 0; i < 12000; ++i) {
        int mask = i < 4000 ? 0 : (i < 8000 ? 1 : 3);
        int select = (i + lane) & mask;
        int expected = i + select + 1;
        if (indirect_call(targets[select], i) != expected + 7 ||
            indirect_tail(targets[select], i) != expected) {
            return JNI_FALSE;
        }
    }
    return JNI_TRUE;
}
