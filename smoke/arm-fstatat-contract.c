/* Self-authored raw AArch64 syscall 79 write-access regression fixture. */
#include <jni.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

_Static_assert(sizeof(struct stat) == 128, "fixture requires AArch64 Linux stat layout");

static long raw_newfstatat(void *output) {
    register long x0 __asm__("x0") = -100; /* AT_FDCWD */
    register long x1 __asm__("x1") = (long)"/dev/null";
    register long x2 __asm__("x2") = (long)output;
    register long x3 __asm__("x3") = 0;
    register long x8 __asm__("x8") = 79; /* __NR_newfstatat */
    __asm__ volatile("svc #0" : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x8) : "memory", "cc");
    return x0; /* Raw Linux negative errno, not libc's -1/errno conversion. */
}

JNIEXPORT jlong JNICALL Java_CLBBArmFstatatContract_run(JNIEnv *env, jclass klass, jint scenario) {
    (void)env; (void)klass;
    if (scenario < 0 || scenario > 6) return -1001;
    long size = sysconf(_SC_PAGESIZE);
    if (size < 4096) return -1002;
    size_t page = (size_t)size;
    unsigned char *region = mmap(NULL, 3 * page, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (region == MAP_FAILED) return -1003;
    unsigned char *middle = region + page;
    if (mprotect(middle, page, PROT_READ | PROT_WRITE)) return -1004;
    memset(middle, 0xa5, page);
    void *output = middle;
    int (*smc_probe)(void) = NULL;
    if (scenario == 4 && mprotect(middle, page, PROT_WRITE)) return -1009;
    if (scenario == 5) {
        if (mprotect(middle + page, page, PROT_READ | PROT_WRITE)) return -1010;
        memset(middle + page, 0xa5, page);
        output = middle + page - sizeof(struct stat) / 2;
    }
    if (scenario == 6) {
        if (mprotect(middle, page, PROT_READ | PROT_WRITE | PROT_EXEC)) return -1011;
        uint32_t *instructions = (uint32_t *)middle;
        instructions[0] = UINT32_C(0x52800160); /* mov w0, #11 */
        instructions[1] = UINT32_C(0xd65f03c0); /* ret */
        smc_probe = (int (*)(void))middle;
        if (smc_probe() != 11) return -1012;
        output = middle + 128; /* Same translated page, without replacing code. */
    }
    if (scenario == 1 && mprotect(middle, page, PROT_READ)) return -1005;
    if (scenario == 2 && munmap(middle, page)) return -1006;
    if (scenario == 3) output = middle + page - sizeof(struct stat) / 2;
    printf("CLBB_FSTATAT_ENTER_CASE=%d\nCLBB_FSTATAT_OUTPUT_BYTES=%zu\n", scenario, sizeof(struct stat));
    fflush(stdout);
    long result = raw_newfstatat(output);
    if (scenario == 4 && mprotect(middle, page, PROT_READ | PROT_WRITE)) return -1013;
    /* Successful copies retain the ABI layout and exact output length. */
    if ((scenario == 0 || scenario >= 4) && result == 0) {
        struct stat *converted = output;
        if (!S_ISCHR(converted->st_mode) || converted->st_size != 0) result = -1007;
        if (((unsigned char *)output)[sizeof(struct stat)] != 0xa5) result = -1014;
        if (smc_probe && smc_probe() != 11) result = -1015;
    }
    if (munmap(region, 3 * page)) return -1008;
    return (jlong)result;
}
