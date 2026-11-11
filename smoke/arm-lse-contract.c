/* Self-authored ARM LSE ISA fixture, independent of the game and candidate libc. */
#include <jni.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/auxv.h>

#define CHECK(c) do { if (!(c)) return __LINE__; } while (0)
#define CAS_TEST(type, width, instruction) do { \
    type cell = 7; type expected = 7; type desired = 13; \
    __asm__ volatile(instruction " " width "[old], " width "[new], [%[ptr]]" \
        : [old] "+&r"(expected) : [new] "r"(desired), [ptr] "r"(&cell) : "memory"); \
    CHECK(expected == 7 && cell == 13); \
    expected = 7; desired = 19; \
    __asm__ volatile(instruction " " width "[old], " width "[new], [%[ptr]]" \
        : [old] "+&r"(expected) : [new] "r"(desired), [ptr] "r"(&cell) : "memory"); \
    CHECK(expected == 13 && cell == 13); \
} while (0)
#define ADD_TEST(type, width, instruction) do { \
    type cell = 11; type old; type add = 5; \
    __asm__ volatile(instruction " " width "[add], " width "[old], [%[ptr]]" \
        : [old] "=&r"(old) : [add] "r"(add), [ptr] "r"(&cell) : "memory"); \
    CHECK(old == 11 && cell == 16); \
} while (0)

struct worker { uint64_t *counter; int loops; };
static void *increment(void *opaque) {
    struct worker *w = opaque;
    for (int i = 0; i < w->loops; ++i) {
        uint64_t old, add = 1;
        __asm__ volatile("ldaddal %[add], %[old], [%[ptr]]"
            : [old] "=&r"(old) : [add] "r"(add), [ptr] "r"(w->counter) : "memory");
    }
    return NULL;
}

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass klass) {
    (void)env; (void)klass;
    /* AArch64 Linux HWCAP_ATOMICS is bit 8: require truthful CPU advertising. */
    CHECK((getauxval(AT_HWCAP) & (1ul << 8)) != 0);
    CAS_TEST(uint32_t, "%w", "casal");
    CAS_TEST(uint64_t, "%", "casal");
    CAS_TEST(uint32_t, "%w", "casalb");
    CAS_TEST(uint32_t, "%w", "casalh");
    ADD_TEST(uint32_t, "%w", "ldaddal");
    ADD_TEST(uint64_t, "%", "ldaddal");
    ADD_TEST(uint32_t, "%w", "ldaddalb");
    ADD_TEST(uint32_t, "%w", "ldaddalh");
    uint64_t counter = 0;
    struct worker state = { &counter, 10000 };
    pthread_t threads[4];
    for (int i = 0; i < 4; ++i) CHECK(pthread_create(&threads[i], NULL, increment, &state) == 0);
    for (int i = 0; i < 4; ++i) CHECK(pthread_join(threads[i], NULL) == 0);
    CHECK(counter == 40000);
    printf("CLBB_ARM64_LSE_WIDTH_CASES=8\nCLBB_ARM64_LSE_CONTENDED_COUNT=%llu\n", (unsigned long long)counter);
    fflush(stdout);
    return 0;
}
