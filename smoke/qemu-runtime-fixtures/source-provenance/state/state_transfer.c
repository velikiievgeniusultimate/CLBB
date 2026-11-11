/* SPDX-License-Identifier: GPL-2.0-or-later */
#include "state_transfer.h"
#ifdef HOST_WORDS_BIGENDIAN
#error This audit currently supports little-endian hosts only
#endif

static bool execution_context_valid(CPUARMState *env)
{
    return env->aarch64 == 1 && arm_current_el(env) == 0
        && !arm_cpu_data_is_big_endian(env);
}

_Static_assert(sizeof(ClbbA64StateV1) == 816, "V1 size mismatch");
_Static_assert(_Alignof(ClbbA64StateV1) == 8, "V1 alignment mismatch");
_Static_assert(offsetof(ClbbA64StateV1, x) == 8, "V1 X offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, sp) == 256, "V1 SP offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, pc) == 264, "V1 PC offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, v) == 272, "V1 V offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, nzcv) == 784, "V1 NZCV offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, fpcr) == 788, "V1 FPCR offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, fpsr) == 792, "V1 FPSR offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, reserved) == 796, "V1 reserved offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, tpidr_el0) == 800, "V1 TLS offset mismatch");
_Static_assert(offsetof(ClbbA64StateV1, tpidrro_el0) == 808, "V1 RO TLS offset mismatch");

static bool fields_valid(const ClbbA64StateV1 *state)
{
    return state->reserved == 0 && (state->nzcv & ~UINT32_C(0xf0000000)) == 0
        && (state->fpcr & ~UINT32_C(0x03c00000)) == 0
        && (state->fpsr & ~UINT32_C(0xf800009f)) == 0;
}

int32_t clbb_qemu_state_export_v1(CPUARMState *env, ClbbA64StateV1 *state)
{
    ClbbA64StateV1 next = {0};
    if (!env || !state) {
        return CLBB_A64_BAD_ARGUMENT;
    }
    if (!execution_context_valid(env)) {
        return CLBB_A64_BAD_STATE;
    }
    next.abi = CLBB_A64_ABI_V1;
    next.size = sizeof(next);
    next.nzcv = pstate_read(env) & PSTATE_NZCV;
    next.fpcr = vfp_get_fpcr(env);
    next.fpsr = vfp_get_fpsr(env);
    if (!fields_valid(&next)) {
        return CLBB_A64_BAD_STATE;
    }
    for (unsigned i = 0; i < 31; ++i) {
        next.x[i] = env->xregs[i];
    }
    next.sp = env->xregs[31];
    next.pc = env->pc;
    for (unsigned i = 0; i < 32; ++i) {
        next.v[i][0] = env->vfp.zregs[i].d[0];
        next.v[i][1] = env->vfp.zregs[i].d[1];
    }
    next.tpidr_el0 = env->cp15.tpidr_el[0];
    next.tpidrro_el0 = env->cp15.tpidrro_el[0];
    *state = next;
    return CLBB_A64_OK;
}

int32_t clbb_qemu_state_import_v1(CPUARMState *env, const ClbbA64StateV1 *state)
{
    if (!env || !state) {
        return CLBB_A64_BAD_ARGUMENT;
    }
    if (state->abi != CLBB_A64_ABI_V1 || state->size != sizeof(*state)) {
        return CLBB_A64_BAD_ABI;
    }
    if (!execution_context_valid(env) || !fields_valid(state)) {
        return CLBB_A64_BAD_STATE;
    }
    /* Validation above is complete; the following non-failing helpers update
     * QEMU's cached NZCV and softfloat representations, not just raw fields. */
    for (unsigned i = 0; i < 31; ++i) {
        env->xregs[i] = state->x[i];
    }
    env->xregs[31] = state->sp;
    env->pc = state->pc;
    for (unsigned i = 0; i < 32; ++i) {
        env->vfp.zregs[i].d[0] = state->v[i][0];
        env->vfp.zregs[i].d[1] = state->v[i][1];
    }
    pstate_write(env, (pstate_read(env) & ~PSTATE_NZCV) | state->nzcv);
    vfp_set_fpcr(env, state->fpcr);
    vfp_set_fpsr(env, state->fpsr);
    env->cp15.tpidr_el[0] = state->tpidr_el0;
    env->cp15.tpidrro_el[0] = state->tpidrro_el0;
    return CLBB_A64_OK;
}
