/* The common QARMA body above is unchanged, and is inlined into each wrapper
 * with compile-time-known S-box functions. There is no per-round indirect call. */
static uint64_t pauth_computepac_portable(uint64_t data, uint64_t modifier,
                                         ARMPACKey key)
{
    return pauth_computepac_core(data, modifier, key, pac_sub, pac_inv_sub);
}

#if defined(__x86_64__) && defined(__GNUC__)
#ifndef CLBB_PAC_C_REFERENCE
/* Rust entry takes scalar values only; this code retains CPU feature dispatch.
 * No guest pointer, env layout or mutable state crosses this boundary. */
__attribute__((visibility("hidden")))
extern uint64_t clbb_rust_pac_ssse3(uint64_t data, uint64_t modifier,
                                     uint64_t key_hi, uint64_t key_lo);
#endif
__attribute__((target("ssse3")))
static uint64_t pauth_computepac_ssse3(uint64_t data, uint64_t modifier,
                                      ARMPACKey key)
{
#ifdef CLBB_PAC_C_REFERENCE
    return vector_state_pauth_computepac(data, modifier, key);
#else
    return clbb_rust_pac_ssse3(data, modifier, key.hi, key.lo);
#endif
}
#endif

static uint64_t pauth_computepac(uint64_t data, uint64_t modifier,
                                 ARMPACKey key)
{
#if defined(__x86_64__) && defined(__GNUC__)
    /* GCC initializes this immutable cached feature set before ordinary code
     * runs. This is one feature test per full PAC, not CPUID or mutable lazy
     * initialization, and unsupported hosts never enter the SSSE3 function. */
    if (__builtin_cpu_supports("ssse3")) {
        return pauth_computepac_ssse3(data, modifier, key);
    }
#endif
    return pauth_computepac_portable(data, modifier, key);
}
