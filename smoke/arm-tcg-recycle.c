/* Exercise repeated whole-cache recycling with our own AArch64 functions. */
#include <jni.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

typedef uint32_t (*probe_fn)(void);

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    (void)env;
    (void)cls;
    const unsigned count = 320000;
    const size_t page = (size_t)sysconf(_SC_PAGESIZE);
    const size_t size = (count * 8 + page - 1) / page * page;
    uint32_t *code = mmap(NULL, size, PROT_READ | PROT_WRITE,
                          MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    if (code == MAP_FAILED) return __LINE__;
    for (unsigned i = 0; i < count; i++) {
        code[2 * i] = 0x52800000u | ((i & 65535u) << 5); /* movz w0,#i */
        code[2 * i + 1] = 0xd65f03c0u;                    /* ret */
    }
    if (mprotect(code, size, PROT_READ | PROT_EXEC)) return __LINE__;
    for (unsigned pass = 0; pass < 2; pass++) {
        for (unsigned n = 0; n < count; n++) {
            unsigned i = pass == 0 ? n : count - n - 1;
            if (((probe_fn)(code + 2 * i))() != (i & 65535u)) return __LINE__;
        }
    }
    if (mprotect(code, page, PROT_READ | PROT_WRITE)) return __LINE__;
    code[0] = 0x52800000u | (129u << 5);
    if (mprotect(code, page, PROT_READ | PROT_EXEC)) return __LINE__;
    if (((probe_fn)code)() != 129u) return __LINE__;
    if (munmap(code, size)) return __LINE__;
    printf("CLBB_TB_RECYCLE_OK blocks=%u visits=%u smc=1\n", count, 2 * count);
    fflush(stdout);
    return 0;
}
