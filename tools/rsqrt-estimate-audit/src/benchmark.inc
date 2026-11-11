extern int clbb_rust_rsqrt_lookup(int a);
#if defined(__GNUC__) && !defined(__clang__)
#define NOINLINE __attribute__((noinline, noclone))
#elif defined(__clang__)
#define NOINLINE __attribute__((noinline))
#else
#define NOINLINE
#endif
static NOINLINE int original_native_call(int a) {
  return do_recip_sqrt_estimate(a);
}
static NOINLINE int lookup_native_call(int a) {
  return clbb_exact_rsqrt_lookup(a);
}
static volatile uint64_t sink;
static uint16_t values[8192];
static uint64_t timestamp(void) {
  struct timespec ts;
  if (clock_gettime(CLOCK_MONOTONIC, &ts)) {
    perror("clock_gettime");
    exit(2);
  }
  return (uint64_t)ts.tv_sec * UINT64_C(1000000000) + (uint64_t)ts.tv_nsec;
}
static void fill(unsigned pattern) {
  uint32_t rng = UINT32_C(0x7b51d04f);
  for (unsigned i = 0; i < 8192; i++) {
    rng ^= rng << 13;
    rng ^= rng >> 17;
    rng ^= rng << 5;
    values[i] = pattern == 0   ? (uint16_t)(128 + rng % 384)
                : pattern == 1 ? 128
                               : 511;
  }
}
static double measure(unsigned method, uint64_t iterations) {
  volatile const uint16_t *input = values;
  uint64_t sum = 0, start = timestamp();
  switch (method) {
  case 0:
    for (uint64_t i = 0; i < iterations; i++)
      sum += (unsigned)do_recip_sqrt_estimate(input[i & 8191]);
    break;
  case 1:
    for (uint64_t i = 0; i < iterations; i++)
      sum += (unsigned)clbb_exact_rsqrt_lookup(input[i & 8191]);
    break;
  case 2:
    for (uint64_t i = 0; i < iterations; i++)
      sum += (unsigned)original_native_call(input[i & 8191]);
    break;
  case 3:
    for (uint64_t i = 0; i < iterations; i++)
      sum += (unsigned)lookup_native_call(input[i & 8191]);
    break;
  case 4:
    for (uint64_t i = 0; i < iterations; i++)
      sum += (unsigned)clbb_rust_rsqrt_lookup(input[i & 8191]);
    break;
  default:
    exit(3);
  }
  uint64_t elapsed = timestamp() - start;
  sink += sum;
  return (double)elapsed / (double)iterations;
}
static int compare_double(const void *a, const void *b) {
  double x = *(const double *)a, y = *(const double *)b;
  return (x > y) - (x < y);
}
int main(int argc, char **argv) {
  if (argc != 3)
    return 2;
  uint64_t iterations = strtoull(argv[1], NULL, 10);
  unsigned repeats = (unsigned)strtoul(argv[2], NULL, 10);
  if (!iterations || repeats < 1 || repeats > 31)
    return 2;
  unsigned valid = 0, prefix_cases = 0;
  for (int a = 128; a < 512; a++) {
    int reference = do_recip_sqrt_estimate(a);
    if (reference != production_current_rsqrt_estimate(a) ||
        reference != clbb_exact_rsqrt_lookup(a) ||
        reference != clbb_rust_rsqrt_lookup(a)) {
      fprintf(stderr, "EXHAUSTIVE_MISMATCH a=%d original=%d c=%d rust=%d\n", a,
              reference, clbb_exact_rsqrt_lookup(a), clbb_rust_rsqrt_lookup(a));
      return 1;
    }
    valid++;
  }
  /* For rsqrte_u32, low 23 bits do not affect the estimate. Test both
   * endpoints of all 512 upper-bit equivalence classes, including sentinel. */
  for (uint32_t prefix = 0; prefix < 512; prefix++)
    for (unsigned endpoint = 0; endpoint < 2; endpoint++) {
      uint32_t value = (prefix << 23) | (endpoint ? UINT32_C(0x7fffff) : 0);
      uint32_t original =
          (value & UINT32_C(0xc0000000))
              ? (uint32_t)do_recip_sqrt_estimate((int)((value >> 23) & 511))
                    << 23
              : UINT32_MAX;
      uint32_t table =
          (value & UINT32_C(0xc0000000))
              ? (uint32_t)clbb_exact_rsqrt_lookup((int)((value >> 23) & 511))
                    << 23
              : UINT32_MAX;
      if (original != table)
        return 1;
      prefix_cases++;
    }
  const char *patterns[] = {"uniform_384", "input_128_max_loop",
                            "input_511_min_loop"};
  const char *methods[] = {"production_c_direct", "generated_c_inline",
                           "production_c_native_call",
                           "generated_c_native_call", "rust_native_abi_call"};
  printf("{\"verified_inputs\":%u,\"u32_prefix_endpoint_checks\":%u,"
         "\"iterations_per_trial\":%llu,\"repeats\":%u,\"patterns\":[",
         valid, prefix_cases, (unsigned long long)iterations, repeats);
  for (unsigned pattern = 0; pattern < 3; pattern++) {
    fill(pattern);
    double results[5][31];
    for (unsigned method = 0; method < 5; method++)
      (void)measure(method, 100000);
    for (unsigned trial = 0; trial < repeats; trial++)
      for (unsigned step = 0; step < 5; step++) {
        unsigned method = (step + trial) % 5;
        results[method][trial] = measure(method, iterations);
      }
    if (pattern)
      putchar(',');
    printf("{\"pattern\":\"%s\",\"methods\":[", patterns[pattern]);
    for (unsigned method = 0; method < 5; method++) {
      qsort(results[method], repeats, sizeof(double), compare_double);
      if (method)
        putchar(',');
      printf("{\"method\":\"%s\",\"ns_per_call_median\":%.6f,\"ns_per_call_"
             "min\":%.6f,\"ns_per_call_max\":%.6f,\"trials_ns_per_call\":[",
             methods[method], results[method][repeats / 2], results[method][0],
             results[method][repeats - 1]);
      for (unsigned trial = 0; trial < repeats; trial++) {
        if (trial)
          putchar(',');
        printf("%.6f", results[method][trial]);
      }
      printf("]}");
    }
    printf("]}");
  }
  printf("],\"checksum\":%llu}\n", (unsigned long long)sink);
  return 0;
}
