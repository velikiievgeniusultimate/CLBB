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
