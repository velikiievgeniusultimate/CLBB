#include <stdint.h>
#include <stdio.h>
#include <jni.h>

struct result {
    uint64_t value;
    uint32_t nzcv, carry, conditions, greater;
};
typedef void probe(uint64_t, uint64_t, struct result *);
extern probe add32, sub32, add64, sub64, add32_alias_rhs, sub64_alias_rhs;
extern probe cmp32, cmp64, cmn32, cmn64;
extern probe add32_one, sub64_one, sub32_zero, add64_page, sub64_self;
static probe *const probes[] = {
    add32, sub32, add64, sub64, add32_alias_rhs, sub64_alias_rhs,
    cmp32, cmp64, cmn32, cmn64, add32_one, sub64_one, sub32_zero, add64_page, sub64_self
};
struct vector {
    uint64_t a, b, expected;
    uint32_t nzcv, conditions;
    unsigned probe;
};
#include "vectors.inc"

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    unsigned i;
    (void)env;
    (void)cls;
    for (i = 0; i < sizeof(vectors) / sizeof(vectors[0]); ++i) {
        const struct vector *v = &vectors[i];
        struct result actual = {0};
        probes[v->probe](v->a, v->b, &actual);
        if (actual.value != v->expected || actual.nzcv != v->nzcv ||
            actual.carry != ((v->nzcv >> 1) & 1) || actual.conditions != v->conditions ||
            actual.greater != ((v->conditions >> 12) & 1)) {
            fprintf(stderr, "NZCV mismatch vector=%u probe=%u expected=%lx/%x/%x actual=%lx/%x/%x carry=%u gt=%u\n",
                    i, v->probe, (unsigned long)v->expected, v->nzcv, v->conditions,
                    (unsigned long)actual.value, actual.nzcv, actual.conditions,
                    actual.carry, actual.greater);
            return 1;
        }
    }
    fprintf(stderr, "CLBB_ARM64_FLAGS_CONTRACT_OK=1 vectors=%u\n", i);
    return 0;
}
