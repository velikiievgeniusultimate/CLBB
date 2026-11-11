#include <jni.h>
#include <stdint.h>
extern int clbb_pauth_simd_stress(const uint64_t *, uint64_t *, uint64_t);
JNIEXPORT jint JNICALL Java_CLBBArmPauthSimd_check(JNIEnv *env, jclass cls)
{
    (void)env; (void)cls;
    uint64_t seed = UINT64_C(0x8abcd0123456789f);
    for (unsigned test = 0; test < 64; ++test) {
        uint64_t input[64], output[64];
        for (unsigned i = 0; i < 64; ++i) {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            input[i] = seed; output[i] = 0;
        }
        int status = clbb_pauth_simd_stress(input, output, 64);
        if (status != 1) return 1000 + status;
        for (unsigned i = 0; i < 64; ++i) {
            if (input[i] != output[i]) return 2000 + i;
        }
    }
    return 0;
}
