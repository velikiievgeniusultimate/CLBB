/* Synthetic direct AArch64 branches and precise exceptions after relinking. */
#include <jni.h>
#include <stdint.h>
#include <stdio.h>
#include <signal.h>
#include <setjmp.h>
#include <stdatomic.h>
#include <sys/mman.h>
#include <sys/ucontext.h>
#include <unistd.h>

typedef unsigned (*entry_fn)(unsigned choice, const void *guard);
static volatile sig_atomic_t matched;
static volatile sig_atomic_t wrong;
static _Atomic uintptr_t expected_pc, guard_address;
static sigjmp_buf recovery;
_Static_assert(ATOMIC_POINTER_LOCK_FREE == 2, "signal handler needs lock-free addresses");

static void handle_fault(int signo, siginfo_t *info, void *raw)
{
    ucontext_t *u = raw;
    if (signo != SIGSEGV || (uintptr_t)info->si_addr != guard_address ||
        u->uc_mcontext.pc != expected_pc || u->uc_mcontext.regs[1] != guard_address) {
        wrong = 1;
        siglongjmp(recovery, 1);
    }
    matched++;
    /* Continue the target at the instruction directly after its faulting load. */
    u->uc_mcontext.pc += 4;
}

static int call_entry(entry_fn fn, unsigned which, unsigned expected)
{
    if (sigsetjmp(recovery, 1)) {
        return 0;
    }
    return fn(which, (void *)guard_address) == expected;
}

static void publish_line(const void *p)
{
    __asm__ volatile("dc cvau, %0\n dsb ish\n ic ivau, %0\n dsb ish\n isb"
                     :
                     : "r"(p)
                     : "memory");
}
static uint32_t branch_imm(const uint32_t *from, const uint32_t *to)
{
    return UINT32_C(0x14000000) | ((uint32_t)(to - from) & UINT32_C(0x03ffffff));
}
static uint32_t mov_w0(unsigned value)
{
    return UINT32_C(0x52800000) | ((value & 65535u) << 5);
}

JNIEXPORT jint JNICALL Java_CLBBArmLseContract_check(JNIEnv *env, jclass cls)
{
    (void)env;
    (void)cls;
    const size_t page = sysconf(_SC_PAGESIZE);
    uint32_t *code =
        mmap(NULL, page * 4, PROT_READ | PROT_WRITE, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    if (code == MAP_FAILED) {
        return __LINE__;
    }
    guard_address = (uintptr_t)(code + 3 * page / 4);
    struct sigaction action = {0}, old;
    action.sa_sigaction = handle_fault;
    action.sa_flags = SA_SIGINFO;
    sigemptyset(&action.sa_mask);
    int ok = sigaction(SIGSEGV, &action, &old) == 0;
    if (!ok) {
        munmap(code, 4 * page);
        return __LINE__;
    }

    /* Exercise same-page B and BL, a fault at the target after a skipped
     * instruction, and a final instruction at the exact page boundary. */
    uint32_t *dst = code + 12;
    unsigned checked = 0, expected_faults = 0;
    for (unsigned round = 0; ok && round < 8; round++) {
        if (mprotect(code, 3 * page, PROT_READ | PROT_WRITE)) {
            ok = 0;
            break;
        }
        code[0] = UINT32_C(0xaa1e03ec); /* mov x12,x30 */
        code[1] = branch_imm(code + 1, dst) | ((round & 1) ? UINT32_C(0x80000000) : 0);
        code[2] = UINT32_C(0x11000400); /* add w0,w0,#1, only after BL returns */
        code[3] = UINT32_C(0xd65f0180); /* ret x12 */
        code[4] = UINT32_C(0x00000000); /* never fetch the skipped instruction */
        dst[0] = UINT32_C(0xf9400029);  /* ldr x9,[x1] */
        dst[1] = mov_w0(700 + round);
        dst[2] = UINT32_C(0xd65f03c0); /* ret */
        uint32_t *edge = code + page / 4 - 8;
        edge[0] = branch_imm(edge, edge + 7);
        edge[7] = mov_w0(87 + round);
        edge[8] = UINT32_C(0xd65f03c0);
        publish_line(code);
        publish_line(dst);
        publish_line(edge);
        publish_line(edge + 8);
        if (mprotect(code, 3 * page, PROT_READ | PROT_EXEC) ||
            mprotect((void *)guard_address, page, PROT_NONE)) {
            ok = 0;
            break;
        }
        for (unsigned j = 0; j < 12; j++) {
            expected_pc = (uintptr_t)dst;
            expected_faults++;
            if (!call_entry((entry_fn)code, 0, 700 + round + (round & 1)) ||
                !call_entry((entry_fn)edge, 0, 87 + round) ||
                (unsigned)matched != expected_faults || wrong) {
                ok = 0;
                break;
            }
            checked += 2;
        }
    }
    int cleaned = sigaction(SIGSEGV, &old, NULL) == 0 && munmap(code, 4 * page) == 0;
    if (!ok || !cleaned) {
        return __LINE__;
    }
    fprintf(stderr, "CLBB_FORWARD_BRANCH_OK=1 calls=%u faults=%u relinks=8\n", checked,
            expected_faults);
    return 0;
}
