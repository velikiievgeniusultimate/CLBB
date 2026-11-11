/*
 * Tiny Code Generator for QEMU
 *
 * Copyright (c) 2008 Fabrice Bellard
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */
#include "qemu/osdep.h"
#include "qemu/host-utils.h"
#include "cpu.h"
#include "exec/helper-proto.h"
#include "exec/cpu_ldst.h"
#include "exec/exec-all.h"
#include "exec/tb-lookup.h"
#include "disas/disas.h"
#include "exec/log.h"
#include "tcg/tcg.h"

/* 32-bit helpers */

int32_t HELPER(div_i32)(int32_t arg1, int32_t arg2)
{
    return arg1 / arg2;
}

int32_t HELPER(rem_i32)(int32_t arg1, int32_t arg2)
{
    return arg1 % arg2;
}

uint32_t HELPER(divu_i32)(uint32_t arg1, uint32_t arg2)
{
    return arg1 / arg2;
}

uint32_t HELPER(remu_i32)(uint32_t arg1, uint32_t arg2)
{
    return arg1 % arg2;
}

/* 64-bit helpers */

uint64_t HELPER(shl_i64)(uint64_t arg1, uint64_t arg2)
{
    return arg1 << arg2;
}

uint64_t HELPER(shr_i64)(uint64_t arg1, uint64_t arg2)
{
    return arg1 >> arg2;
}

int64_t HELPER(sar_i64)(int64_t arg1, int64_t arg2)
{
    return arg1 >> arg2;
}

int64_t HELPER(div_i64)(int64_t arg1, int64_t arg2)
{
    return arg1 / arg2;
}

int64_t HELPER(rem_i64)(int64_t arg1, int64_t arg2)
{
    return arg1 % arg2;
}

uint64_t HELPER(divu_i64)(uint64_t arg1, uint64_t arg2)
{
    return arg1 / arg2;
}

uint64_t HELPER(remu_i64)(uint64_t arg1, uint64_t arg2)
{
    return arg1 % arg2;
}

uint64_t HELPER(muluh_i64)(uint64_t arg1, uint64_t arg2)
{
    uint64_t l, h;
    mulu64(&l, &h, arg1, arg2);
    return h;
}

int64_t HELPER(mulsh_i64)(int64_t arg1, int64_t arg2)
{
    uint64_t l, h;
    muls64(&l, &h, arg1, arg2);
    return h;
}

uint32_t HELPER(clz_i32)(uint32_t arg, uint32_t zero_val)
{
    return arg ? clz32(arg) : zero_val;
}

uint32_t HELPER(ctz_i32)(uint32_t arg, uint32_t zero_val)
{
    return arg ? ctz32(arg) : zero_val;
}

uint64_t HELPER(clz_i64)(uint64_t arg, uint64_t zero_val)
{
    return arg ? clz64(arg) : zero_val;
}

uint64_t HELPER(ctz_i64)(uint64_t arg, uint64_t zero_val)
{
    return arg ? ctz64(arg) : zero_val;
}

uint32_t HELPER(clrsb_i32)(uint32_t arg)
{
    return clrsb32(arg);
}

uint64_t HELPER(clrsb_i64)(uint64_t arg)
{
    return clrsb64(arg);
}

uint32_t HELPER(ctpop_i32)(uint32_t arg)
{
    return ctpop32(arg);
}

uint64_t HELPER(ctpop_i64)(uint64_t arg)
{
    return ctpop64(arg);
}

static void * __attribute__((noinline)) clbb_lookup_tb_ptr_slow(CPUArchState *env)
{
    CPUState *cpu = env_cpu(env);
    TranslationBlock *tb;
    target_ulong cs_base, pc;
    uint32_t flags;

    tb = tb_lookup__cpu_state(cpu, &pc, &cs_base, &flags, curr_cflags());
    if (tb == NULL) {
        return tcg_ctx->code_gen_epilogue;
    }
    qemu_log_mask_and_addr(CPU_LOG_EXEC, pc,
                           "Chain %d: %p ["
                           TARGET_FMT_lx "/" TARGET_FMT_lx "/%#x] %s\n",
                           cpu->cpu_index, tb->tc.ptr, cs_base, pc, flags,
                           lookup_symbol(pc));
    return tb->tc.ptr;
}

/* GPL-2.0-or-later experiment derived from QEMU tb-lookup.h
 * Copyright (C) 2017 Emilio G. Cota and QEMU ARM state extraction.
 * Original full helper retained verbatim in the noinline slow function.
 * No second cache or new code-pointer lifetime/invalidation protocol. */
void *HELPER(lookup_tb_ptr)(CPUArchState *env)
{
#if defined(CONFIG_USER_ONLY) && defined(TARGET_AARCH64) && !defined(CONFIG_DEBUG_TCG)
    CPUState *cpu = env_cpu(env);
    CPUArchState *state = (CPUArchState *)cpu->env_ptr;
    uint32_t cf_mask = curr_cflags();
    uint32_t flags = state->hflags;

    if (likely(FIELD_EX32(flags, TBFLAG_ANY, AARCH64_STATE))) {
        target_ulong pc = state->pc;
        TranslationBlock *tb;
        uint32_t hash;

        if (cpu_isar_feature(aa64_bti, env_archcpu(state))) {
            flags = FIELD_DP32(flags, TBFLAG_A64, BTYPE, state->btype);
        }
        if (FIELD_EX32(flags, TBFLAG_ANY, SS_ACTIVE) &&
            (state->pstate & PSTATE_SS)) {
            flags = FIELD_DP32(flags, TBFLAG_ANY, PSTATE_SS, 1);
        }
        hash = tb_jmp_cache_hash_func(pc);
        tb = atomic_rcu_read(&cpu->tb_jmp_cache[hash]);
        cf_mask &= ~CF_CLUSTER_MASK;
        cf_mask |= cpu->cluster_index << CF_CLUSTER_SHIFT;
        if (likely(tb && tb->pc == pc && tb->cs_base == 0 &&
                   tb->flags == flags &&
                   tb->trace_vcpu_dstate == *cpu->trace_dstate &&
                   (tb_cflags(tb) & (CF_HASH_MASK | CF_INVALID)) == cf_mask)) {
            qemu_log_mask_and_addr(CPU_LOG_EXEC, pc,
                                  "Chain %d: %p ["
                                  TARGET_FMT_lx "/" TARGET_FMT_lx "/%#x] %s\n",
                                  cpu->cpu_index, tb->tc.ptr,
                                  (target_ulong)0, pc, flags,
                                  lookup_symbol(pc));
            return tb->tc.ptr;
        }
    }
#endif
    return clbb_lookup_tb_ptr_slow(env);
}

void HELPER(exit_atomic)(CPUArchState *env)
{
    cpu_loop_exit_atomic(env_cpu(env), GETPC());
}
