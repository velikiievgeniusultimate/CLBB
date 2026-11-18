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
    if (sigsetjmp(recovery, 1)) { return 0; }
    return fn(which, (void *)guard_address) == expected;
}

static void publish_line(const void *p)
{
    __asm__ volatile("dc cvau, %0\n dsb ish\n ic ivau, %0\n dsb ish\n isb"
                     : : "r"(p) : "memory");
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
    uint32_t *code = mmap(NULL, page * 4, PROT_READ | PROT_WRITE,
                          MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
    if (code == MAP_FAILED) { return __LINE__; }
    uint32_t *target[2] = { code + page / 4, code + 2 * page / 4 };
    guard_address = (uintptr_t)(code + 3 * page / 4);
    struct sigaction action = {0}, old;
    action.sa_sigaction = handle_fault;
    action.sa_flags = SA_SIGINFO;
    sigemptyset(&action.sa_mask);
    int ok = sigaction(SIGSEGV, &action, &old) == 0;
    if (!ok) { munmap(code, 4 * page); return __LINE__; }

    /* Both edges lead to different pages. The second edge includes a further
     * unconditional direct branch TB. Both destination pages are invalidated
     * independently while the unchanged source TB remains cached. */
    code[0] = UINT32_C(0x34000000) | (((uint32_t)(target[0] - code) & 0x7ffff) << 5);
    code[1] = branch_imm(code + 1, target[1]);
    for (unsigned i = 0; i < 2; i++) {
        target[i][0] = UINT32_C(0xd503201f); /* nop */
        target[i][1] = mov_w0(i ? 207 : 91);
        target[i][2] = UINT32_C(0xd65f03c0); /* ret */
        publish_line(target[i]);
    }
    publish_line(code);
    ok = mprotect(code, 3 * page, PROT_READ | PROT_EXEC) == 0 &&
         mprotect((void *)guard_address, page, PROT_NONE) == 0;
    unsigned results[2] = {91, 207}, has_fault[2] = {0}, expected_faults = 0, checked = 0;
    for (unsigned round = 0; ok && round < 8; round++) {
        unsigned idx = round & 1u;
        if (mprotect(target[idx], page, PROT_READ | PROT_WRITE)) { ok = 0; break; }
        has_fault[idx] = (round / 2) & 1u;
        results[idx] += 13;
        target[idx][0] = has_fault[idx] ? UINT32_C(0xf9400029) /* ldr x9,[x1] */
                                        : UINT32_C(0xd503201f);
        target[idx][1] = mov_w0(results[idx]);
        publish_line(target[idx]);
        if (mprotect(target[idx], page, PROT_READ | PROT_EXEC)) { ok = 0; break; }
        for (unsigned i = 0; i < 24; i++) {
            unsigned which = (i * 3 + (i >> 1) + round) & 1u;
            expected_pc = (uintptr_t)target[which];
            expected_faults += has_fault[which];
            if (!call_entry((entry_fn)code, which, results[which]) || (unsigned)matched != expected_faults || wrong) {
                ok = 0; break;
            }
            checked++;
        }
    }
    int cleaned = sigaction(SIGSEGV, &old, NULL) == 0 && munmap(code, 4 * page) == 0;
    if (!ok || !cleaned) { return __LINE__; }
    fprintf(stderr, "CLBB_DIRECT_FALLBACK_OK=1 calls=%u faults=%u relinks=8\n", checked, expected_faults);
    return 0;
}
