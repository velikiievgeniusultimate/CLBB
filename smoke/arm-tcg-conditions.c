#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include <jni.h>
#include "arm-tcg-conditions.inc"

static uint64_t update(uint64_t value, uint64_t word)
{
    return (value ^ word) * UINT64_C(0x100000001b3);
}

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    unsigned cases = 0;
    (void)env;
    (void)cls;
    for (size_t i = 0; i < sizeof(specs) / sizeof(specs[0]); ++i) {
        uint64_t digest = UINT64_C(0xcbf29ce484222325);
        for (size_t n = 0; n < sizeof(input) / sizeof(input[0]); ++n) {
            uint64_t output[2];
            specs[i].fn(input[n][0], input[n][1], output);
            digest = update(update(digest, output[0]), output[1]);
            ++cases;
        }
        if (digest != specs[i].digest) {
            fprintf(stderr, "BAD COMPARE %s want=%" PRIx64 " got=%" PRIx64 "\n", specs[i].name,
                    specs[i].digest, digest);
            return 1;
        }
    }
    fprintf(stderr, "CLBB_ARM64_CONDITION_CONTRACT_OK=1 cases=%u\n", cases);
    return 0;
}
