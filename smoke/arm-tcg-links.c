/* Exercise AArch64 indirect and conditional links against an invalidated page. */
#include <jni.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/mman.h>
#include <unistd.h>

typedef unsigned (*target_fn)(unsigned);
extern unsigned through_lr(uintptr_t);
extern unsigned through_br(uintptr_t);
extern unsigned through_tagged_ret(uintptr_t);
__asm__(
    ".text\n"
    ".type through_lr,%function\nthrough_lr:\n"
    "mov x12, x30\nmov x30, x0\nblr x30\nret x12\n"
    ".type through_br,%function\nthrough_br:\nbr x0\n"
    ".type through_tagged_ret,%function\nthrough_tagged_ret:\nret x0\n");

static uint32_t mov_result(unsigned value)
{
    return 0x52800000u | (value << 5); /* movz w0,#value */
}

/* Each changed sequence lies wholly in the first cache line of its page. */
static void publish_line(const void *address)
{
    __asm__ volatile("dc cvau, %0\n dsb ish\n ic ivau, %0\n dsb ish\n isb"
                     : : "r"(address) : "memory");
}

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    (void)env;
    (void)cls;
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    uint32_t *code = mmap(NULL, page * 2, PROT_READ | PROT_WRITE,
                          MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    if (code == MAP_FAILED) return __LINE__;
    uint32_t *other = code + page / sizeof(*code);
    code[0] = 0x34000000u | ((page / 4) << 5); /* cbz w0,other */
    code[1] = mov_result(31);
    code[2] = 0xd65f03c0u; /* ret */
    other[0] = mov_result(17);
    other[1] = 0xd65f03c0u;
    publish_line(code);
    publish_line(other);
    if (mprotect(code, 2 * page, PROT_READ | PROT_EXEC)) return __LINE__;
    for (unsigned round = 0; round < 16; round++) {
        unsigned changed = 17 + round;
        if (mprotect(other, page, PROT_READ | PROT_WRITE)) return __LINE__;
        other[0] = mov_result(changed);
        publish_line(other);
        if (mprotect(other, page, PROT_READ | PROT_EXEC)) return __LINE__;
        for (unsigned i = 0; i < 256; i++) {
            unsigned expected = (i & 1) ? 31 : changed;
            uintptr_t address = (uintptr_t)((i & 1) ? code + 1 : other);
            address |= (uintptr_t)(0x80u | (i & 0x7fu)) << 56;
            if (((target_fn)code)(i & 1) != expected) return __LINE__;
            if (through_br(address) != expected) return __LINE__;
            if (through_tagged_ret(address) != expected) return __LINE__;
            if (through_lr(address) != expected) return __LINE__;
        }
    }
    if (munmap(code, 2 * page)) return __LINE__;
    puts("CLBB_TB_LINKS_OK checks=16384 invalidations=16");
    fflush(stdout);
    return 0;
}
