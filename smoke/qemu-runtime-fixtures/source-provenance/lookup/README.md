# Private QEMU A64 indirect lookup helper experiment

This is a **licensed QEMU derivative**, not independently owned ETA execution code. It uses the existing QEMU full-key lookup and A64 state extraction. Existing notices remain in the copied tcg-runtime.c; tb-lookup.h credits Emilio G. Cota (2017), GPL version 2 or later. The new wrapper and fixture are supplied under GPL-2.0-or-later. No canonical source, build, prefix or game file is changed.

## Exact scope

prepare.py copies accel/tcg/tcg-runtime.c into a fresh output directory. The original lookup helper body becomes a static noinline slow function with identical body text. One public helper wrapper is added, only for CONFIG_USER_ONLY + TARGET_AARCH64 + !CONFIG_DEBUG_TCG. Other builds tail-call the unchanged slow implementation.

The A64 hit path reads the same actual cpu->env_ptr, hflags, pc, dynamic BTI/BTYPE and active SS/PSTATE_SS semantics. It uses the same jump-cache hash, atomic_rcu_read, full PC/cs_base-zero/flags/trace/cflags including CF_INVALID and current cluster/parallel/icount bits. Its logging macro and arguments preserve the original behavior. Any non-A64 state, null entry or mismatch calls the original helper; original QHT/physical-page checks, cache fill, epilogue and failure handling remain there. The miss path deliberately may repeat state/key work. It is not a new permission or execution cache, and does not persist pointers beyond the existing cpu_exec RCU interval.

There are no modifications to direct chaining, IRQ/exit prologues, generated TB code, code-pointer lifetime, RCU, invalidation/flush publication or guest ISA features. The wrapper requires the same CPU-owner/existing-QEMU invariants as the original helper. It makes no safety claim for concurrent arbitrary mutation of CPU state outside that contract.

The mechanism is compiler shrink-wrapping: A64 cache hits have no address-taken output locals/A32 fallback and therefore need no stack frame or canary spill. Stack protector remains enabled; slow paths keep their ordinary checks. The complete copied source compiles. The candidate wrapper is 374 bytes plus the 545-byte unchanged slow helper in the isolated object. It adds one shared helper body, not code at each translated TB.

## Functional evidence

logs/eta-qemu-a64-lookup-candidate/functional-03/report.json records:

- 60,030 exact original/candidate comparisons, normal and ASAN/UBSAN: full CPU state/cache-fill result, return target, QHT args/count, logging/symbol callbacks, every key mismatch, A32 route, null/global hit, invalidation-clear-republish sequences and 20,000 random A64 states including BTI/SS/parallel/icount/cluster/trace variations.
- Actual QEMU ARMCPU/TranslationBlock layouts, header hash/atomic macros and the original compiled helper/lookup code are used. QHT, A32 state fallback and log output are authored deterministic seams; no guest or real QHT/RCU reclamation runs.
- Whole private tcg-runtime.c object compiles with O2/PIC/stack-protector-strong. No target-cpu=native or host intrinsics are added. The paired functions use identical flags. This is not a complete production bridge link.

logs/eta-qemu-a64-lookup-candidate/invalidation-01/report.json adds three deterministic test-only interleavings: mark the loaded TB invalid before key check; invalidate plus clear its slot; clear slot while the loaded TB remains valid. Both implementations produce the same return/cache/TB effects. Normal and ASAN/UBSAN each pass 60,033 comparisons. Only test copies contain injected hooks; the production candidate has none. Allocations are retained under the original RCU lifetime assumption. This is not a real QEMU cross-thread reclamation/SMC integration proof, which remains necessary before promotion.

functional-01 retains initial authored fixture declaration mistakes; functional-02 retains successful semantic tests followed by an isolated whole-source compile failure from the omitted accel/tcg include path. Neither failure required a production algorithm correction. functional-03 is the complete authoritative run.

## One bounded ABBA, no startup claim

functional-03/benchmark-summary.json preserves all raw rounds, two million calls each, thread CPU clock, same binary/checksums:

| Workload | Original ns/call | Candidate ns/call |
|---|---:|---:|
| A64 full-key hit | 2.577 | 1.923 |
| A64, one mismatch per 64 calls | 2.883 | 2.341 |
| Authored A32 state seam | 2.809 | 3.013 |
| Logging seam enabled | 4.248 | 4.237 |

These extremely hot synthetic objects do not reproduce the actual game's cache misses, QHT cost, code locality, RCU concurrency or working set. The A64 reduction is approximately 25.4%/18.8%; A32 regresses 7.2%, and logging differences are noise-sized. Only one series was run; system activity was uncontrolled. No isolated-core, whole-game, stable-ready-lobby or five-second speed result is claimed. Even a similar percentage in a helper taking about 10% of sampled CPU would be a small overall improvement, not a solution to the entire startup target.

Recommendation: GO for independent review and a private full-bridge/QOM invalidation experiment. NO canonical promotion or game speed claim from this microbenchmark alone. Keep A32 performance outside the A64-only startup claim; reject the candidate if actual QEMU/SMC integration or clean complete startup regresses. Do not proceed to larger inline-TCG/cache redesign on these results alone.
