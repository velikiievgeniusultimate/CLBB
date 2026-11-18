/* Architecture fixtures owned by the emulator tests. No game is involved. */
#include <jni.h>
#include <pthread.h>
#include <setjmp.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/auxv.h>
#include <sys/mman.h>
#include <sys/ucontext.h>
#include <unistd.h>

#define CHECK(x) do { if (!(x)) return __LINE__; } while (0)

static uint64_t counter;
static void *increment(void *unused)
{
    (void)unused;
    for (int i = 0; i < 10000; i++) {
        uint64_t old = __atomic_load_n(&counter, __ATOMIC_RELAXED);
        while (!__atomic_compare_exchange_n(&counter, &old, old + 1, 0,
                                             __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)) {
        }
    }
    return NULL;
}

#define WIDTH_TEST(type) do { \
    type cell = 19, expected = 19; \
    CHECK(__atomic_compare_exchange_n(&cell, &expected, (type)0xd3, 0, \
                                      __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)); \
    CHECK(expected == 19 && cell == (type)0xd3); \
    CHECK(!__atomic_compare_exchange_n(&cell, &expected, (type)0x18, 0, \
                                       __ATOMIC_SEQ_CST, __ATOMIC_RELAXED)); \
    CHECK(expected == (type)0xd3 && cell == (type)0xd3); \
} while (0)

extern void llsc_store(uint64_t *cell);
extern const char llsc_store_pc[];
extern void lse_store(uint64_t *cell);
extern const char lse_store_pc[];
extern int raw_aliases(uint64_t *cell);
extern void raw_misaligned(uint64_t *cell);
extern const char misaligned_pc[];
__asm__(
    ".text\n.p2align 2\n"
    ".global llsc_store, llsc_store_pc\n.hidden llsc_store, llsc_store_pc\n"
    ".type llsc_store,%function\n"
    "llsc_store:\n mov x9, #0x4567\n ldaxr x10,[x0]\n"
    "llsc_store_pc:\n stlxr w11,x9,[x0]\n ret\n"
    ".size llsc_store,.-llsc_store\n"
    ".global lse_store, lse_store_pc\n.hidden lse_store, lse_store_pc\n"
    ".type lse_store,%function\n"
    "lse_store:\n mov x9,#0x4567\n ldr x10,[x0]\n .arch_extension lse\n"
    "lse_store_pc:\n casal x10,x9,[x0]\n ret\n"
    ".size lse_store,.-lse_store\n"
    ".global raw_aliases\n.hidden raw_aliases\n.type raw_aliases,%function\n"
    "raw_aliases:\n mov x4,x0\n mov x3,#19\n str x0,[x4]\n"
    "casal x0,x3,[x0]\n cmp x0,x4\n b.ne 1f\n ldr x0,[x4]\n cmp x0,#19\n b.ne 1f\n"
    "casal x0,x4,[x4]\n cmp x0,#19\n b.ne 1f\n ldr x0,[x4]\n cmp x0,x4\n b.ne 1f\n"
    "casal x0,x0,[x0]\n cmp x0,x4\n b.ne 1f\n"
    "mov w0,#1\n ret\n1: mov w0,#0\n ret\n.size raw_aliases,.-raw_aliases\n"
    ".global raw_misaligned, misaligned_pc\n.hidden raw_misaligned, misaligned_pc\n"
    ".type raw_misaligned,%function\n"
    "raw_misaligned:\n mov x9,#0x4567\n mov x10,#19\n"
    "misaligned_pc:\n casal x10,x9,[x0]\n ret\n"
    ".size raw_misaligned,.-raw_misaligned\n");

static sigjmp_buf recover;
static const char *fault_pc;
static void *fault_address;
static volatile sig_atomic_t exact_fault;
static volatile sig_atomic_t last_signal;

static void fault_handler(int signo, siginfo_t *info, void *opaque)
{
    ucontext_t *context = opaque;
    last_signal = signo;
    exact_fault = (signo == SIGSEGV || signo == SIGBUS) &&
                  info->si_addr == fault_address &&
                  context->uc_mcontext.pc == (uintptr_t)fault_pc &&
                  context->uc_mcontext.regs[9] == 0x4567;
    siglongjmp(recover, 1);
}

static int check_readonly(uint64_t *cell, void (*write_probe)(uint64_t *),
                          const char *instruction)
{
    fault_pc = instruction;
    fault_address = cell;
    exact_fault = 0;
    if (!sigsetjmp(recover, 1)) {
        write_probe(cell);
        return 0;
    }
    return exact_fault && last_signal == SIGSEGV;
}

/* Some QEMU revisions replay an unaligned CAS in single-step mode.
 * Record both observable outcomes for comparison with this build's control. */
static int check_unaligned(unsigned char *bytes)
{
    uint64_t value = 19;
    memcpy(bytes, &value, sizeof(value));
    fault_pc = misaligned_pc;
    fault_address = bytes;
    exact_fault = 0;
    if (!sigsetjmp(recover, 1)) {
        raw_misaligned((uint64_t *)(void *)bytes);
        memcpy(&value, bytes, sizeof(value));
        return value == 0x4567 ? 1 : 0;
    }
    return exact_fault ? (last_signal == SIGBUS ? 2 : 3) : 0;
}

static int check_code_invalidation(size_t page)
{
    typedef int (*code_fn)(void);
    uint32_t *p = mmap(NULL, page, PROT_READ | PROT_WRITE | PROT_EXEC,
                       MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) {
        return 0;
    }
    uint32_t old = 0x52800540; /* mov w0, #42 */
    p[0] = old;
    p[1] = 0xd65f03c0;        /* ret */
    code_fn volatile fn = (code_fn)(uintptr_t)p;
    int ok = fn() == 42 && __atomic_compare_exchange_n(
        p, &old, 0x52800560, 0, __ATOMIC_SEQ_CST, __ATOMIC_RELAXED) &&
        fn() == 43;
    return !munmap(p, page) && ok;
}

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    (void)env;
    (void)cls;
    WIDTH_TEST(uint8_t);
    WIDTH_TEST(uint16_t);
    WIDTH_TEST(uint32_t);
    WIDTH_TEST(uint64_t);
    pthread_t workers[4];
    for (size_t k = 0; k < 4; k++) CHECK(!pthread_create(&workers[k], NULL, increment, NULL));
    for (size_t k = 0; k < 4; k++) CHECK(!pthread_join(workers[k], NULL));
    CHECK(counter == 40000);

    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    uint64_t *cell = mmap(NULL, page, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    CHECK(cell != MAP_FAILED);
    *cell = 19;
    CHECK(!mprotect(cell, page, PROT_READ));
    struct sigaction old, old_bus, action = { 0 };
    action.sa_flags = SA_SIGINFO;
    action.sa_sigaction = fault_handler;
    sigemptyset(&action.sa_mask);
    CHECK(!sigaction(SIGSEGV, &action, &old));
    CHECK(!sigaction(SIGBUS, &action, &old_bus));
    int llsc_fault = check_readonly(cell, llsc_store, llsc_store_pc);
    int has_lse = !!(getauxval(AT_HWCAP) & (1UL << 8));
    int lse_fault = !has_lse || check_readonly(cell, lse_store, lse_store_pc);
    int unaligned = -1;
    if (has_lse) {
        unsigned char bytes[24] __attribute__((aligned(8)));
        uint64_t alias_cell;
        CHECK(raw_aliases(&alias_cell));
        unaligned = check_unaligned(bytes + 1);
        CHECK(unaligned != 0);
    }
    CHECK(!sigaction(SIGBUS, &old_bus, NULL));
    CHECK(!sigaction(SIGSEGV, &old, NULL));
    CHECK(*cell == 19);
    CHECK(!munmap(cell, page));
    CHECK(llsc_fault && lse_fault);
    CHECK(check_code_invalidation(page));
    printf("CLBB_ATOMIC_FAULT_PC_OK=1 lse=%d unaligned=%d smc=1 contended=40000 widths=4\n",
           has_lse, unaligned);
    fflush(stdout);
    return 0;
}
