/* Test code only. The driver prepends extracted, separately renamed production
 * and pinned upstream functions. No guest or account data is read. */
static uint64_t random_word(uint64_t *state)
{
    uint64_t z = (*state += UINT64_C(0x9e3779b97f4a7c15));
    z = (z ^ (z >> 30)) * UINT64_C(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)) * UINT64_C(0x94d049bb133111eb);
    return z ^ (z >> 31);
}

static unsigned long long full_cases;
static unsigned long long kernel_cases;

#if defined(__x86_64__) && defined(__GNUC__)
__attribute__((target("ssse3")))
static void simd_kernels(uint64_t value)
{
    __m128i v = production_vector_unpack(value);
    if (production_vector_pack(v) != value ||
        production_vector_pack(_mm_shuffle_epi8(production_vector_load(production_vector_sbox), v)) != oracle_pac_sub(value) ||
        production_vector_pack(_mm_shuffle_epi8(production_vector_load(production_vector_inv_sbox), v)) != oracle_pac_inv_sub(value) ||
        production_vector_pack(production_vector_shuffle(v)) != oracle_pac_cell_shuffle(value) ||
        production_vector_pack(production_vector_inv_shuffle(v)) != oracle_pac_cell_inv_shuffle(value) ||
        production_vector_pack(production_vector_mult(v)) != oracle_pac_mult(value) ||
        production_vector_pack(production_vector_tweak(v)) != oracle_tweak_shuffle(value) ||
        production_vector_pack(production_vector_inv_tweak(v)) != oracle_tweak_inv_shuffle(value)) {
        fputs("Forced full-state SSSE3 primitive mismatch\n", stderr);
        exit(1);
    }
}

#endif

static void kernels(uint64_t value)
{
    if (oracle_pac_sub(value) != production_pac_sub(value) ||
        oracle_pac_inv_sub(value) != production_pac_inv_sub(value) ||
        oracle_pac_mult(value) != production_pac_mult(value)) {
        fprintf(stderr, "Kernel mismatch at %016" PRIx64 "\n", value);
        exit(1);
    }
#if defined(__x86_64__) && defined(__GNUC__)
    if (__builtin_cpu_supports("ssse3")) {
        simd_kernels(value);
    }
#endif
    ++kernel_cases;
}

static void full(uint64_t data, uint64_t modifier, ARMPACKey key)
{
    uint64_t a = oracle_pauth_computepac(data, modifier, key);
    uint64_t b = production_pauth_computepac(data, modifier, key);
    uint64_t c = production_pauth_computepac_portable(data, modifier, key);
    if (a != b || a != c) {
        fprintf(stderr, "Full QARMA mismatch in deterministic test %llu\n",
                full_cases);
        exit(1);
    }
#if defined(__x86_64__) && defined(__GNUC__)
    if (__builtin_cpu_supports("ssse3") &&
        a != production_pauth_computepac_ssse3(data, modifier, key)) {
        fputs("Forced SSSE3 full QARMA mismatch\n", stderr);
        exit(1);
    }
#endif
    ++full_cases;
}

static void validate(unsigned random_count)
{
    /* Every input byte in each position and every input nibble in each cell. */
    for (unsigned shift = 0; shift < 64; shift += 8) {
        for (unsigned byte = 0; byte < 256; ++byte) {
            kernels((uint64_t)byte << shift);
        }
    }
    for (unsigned shift = 0; shift < 64; shift += 4) {
        for (unsigned nibble = 0; nibble < 16; ++nibble) {
            kernels((uint64_t)nibble << shift);
        }
    }
    /* Exhaust each 16-bit matrix column. Four columns cover every possible
     * input to the independently mixed 4-nibble groups. */
    for (unsigned column = 0; column < 4; ++column) {
        for (unsigned value = 0; value < 65536; ++value) {
            uint64_t word = 0;
            for (unsigned row = 0; row < 4; ++row) {
                word |= (uint64_t)((value >> (row * 4)) & 15) <<
                        (row * 16 + column * 4);
            }
            kernels(word);
        }
    }
    uint64_t edge[140] = { 0, UINT64_MAX, UINT64_C(0xaaaaaaaaaaaaaaaa),
        UINT64_C(0x5555555555555555), UINT64_C(0x0123456789abcdef),
        UINT64_C(0xfedcba9876543210), UINT64_C(0x0000ffffffffffff),
        UINT64_C(0xffff000000000000), UINT64_C(0x7fffffffffffffff),
        UINT64_C(0x8000000000000000), UINT64_C(0x00ffffffffffffff),
        UINT64_C(0xff00000000000000) };
    for (unsigned bit = 0; bit < 64; ++bit) {
        edge[12 + bit * 2] = UINT64_C(1) << bit;
        edge[13 + bit * 2] = ~(UINT64_C(1) << bit);
    }
    /* Cartesian data/modifier edges and independently chosen hi/lo key edges. */
    for (unsigned a = 0; a < 140; ++a) {
        for (unsigned b = 0; b < 140; ++b) {
            ARMPACKey key = { edge[(a + b) % 140],
                              edge[(a * 3 + b * 11) % 140] };
            full(edge[a], edge[b], key);
        }
    }
    for (unsigned a = 0; a < 140; ++a) {
        for (unsigned b = 0; b < 140; ++b) {
            ARMPACKey key = { edge[a], edge[b] };
            full(UINT64_C(0x9876543210abcdef),
                 UINT64_C(0xfedcba9876543210), key);
        }
    }
    uint64_t state = UINT64_C(0xdedababa12345678);
    for (unsigned i = 0; i < random_count; ++i) {
        uint64_t data = random_word(&state), modifier = random_word(&state);
        ARMPACKey key = { random_word(&state), random_word(&state) };
        kernels(data);
        full(data, modifier, key);
    }
    printf("{\"validation\":true,\"random_full_cases\":%u,"
           "\"full_cases\":%llu,\"kernel_cases\":%llu}\n",
           random_count, full_cases, kernel_cases);
}

static double timestamp(clockid_t clock)
{
    struct timespec t;
    if (clock_gettime(clock, &t)) { perror("clock_gettime"); exit(1); }
    return t.tv_sec + t.tv_nsec * 1e-9;
}

typedef uint64_t (*pac_fn)(uint64_t, uint64_t, ARMPACKey);
static uint64_t bench(pac_fn fn, unsigned count, unsigned lanes)
{
    uint64_t v[4] = { 1, 2, 3, 4 };
    for (unsigned i = 0; i < count; ++i) {
        unsigned lane = i & (lanes - 1);
        ARMPACKey key = { UINT64_C(0xa123456789abcdef) + i,
                         UINT64_C(0xbeaf876543210fed) ^ i };
        v[lane] = fn(v[lane], i, key);
    }
    return v[0] ^ v[1] ^ v[2] ^ v[3];
}

int main(int argc, char **argv)
{
    unsigned random_count = argc > 1 ? strtoul(argv[1], NULL, 10) : 2000000;
    unsigned iterations = argc > 2 ? strtoul(argv[2], NULL, 10) : 1000000;
    unsigned repeats = argc > 3 ? strtoul(argv[3], NULL, 10) : 7;
    validate(random_count);
    pac_fn variants[4] = { oracle_pauth_computepac,
        production_pauth_computepac, production_pauth_computepac_portable, NULL };
    const char *names[] = { "oracle", "production", "forced-portable", "forced-ssse3" };
    unsigned variant_count = 3;
#if defined(__x86_64__) && defined(__GNUC__)
    if (__builtin_cpu_supports("ssse3")) {
        variants[3] = production_pauth_computepac_ssse3;
        variant_count = 4;
    }
#endif
    for (unsigned lanes = 1; lanes <= 4; lanes *= 4) {
        for (unsigned repeat = 0; repeat < repeats; ++repeat) {
            uint64_t checksums[4] = { 0 };
            /* Alternate order to reduce systematic thermal/order bias. */
            for (unsigned position = 0; position < variant_count; ++position) {
                unsigned which = (position + repeat) % variant_count;
                double wall = timestamp(CLOCK_MONOTONIC);
                double cpu = timestamp(CLOCK_THREAD_CPUTIME_ID);
                checksums[which] = bench(variants[which], iterations, lanes);
                cpu = timestamp(CLOCK_THREAD_CPUTIME_ID) - cpu;
                wall = timestamp(CLOCK_MONOTONIC) - wall;
                printf("{\"benchmark\":\"%s\",\"lanes\":%u,\"repeat\":%u,"
                       "\"iterations\":%u,\"wall_ns_per_pac\":%.6f,"
                       "\"cpu_ns_per_pac\":%.6f,\"checksum\":\"%016" PRIx64 "\"}\n",
                       names[which], lanes, repeat,
                       iterations, wall * 1e9 / iterations,
                       cpu * 1e9 / iterations, checksums[which]);
            }
            for (unsigned which = 1; which < variant_count; ++which) {
                if (checksums[0] != checksums[which]) {
                    fputs("Benchmark checksum mismatch\n", stderr);
                    return 1;
                }
            }
        }
    }
    return 0;
}
