# Runtime Rust migration update — 2026-11-08

SSSE3 runtime PAC now calls the hidden scalar-ABI Rust entry in
`translator-rust/src/lib.rs`. The C vector implementation described below is
retained as the independently generated reference. This audit explicitly
compiles `CLBB_PAC_C_REFERENCE=1`; the separate
`tools/translator-rust-audit/check.py` checks and benchmarks actual default Rust
dispatch against that reference and pinned upstream. Unsupported hosts still
use the C portable path. One cross-language boundary exists per full PAC;
there are no per-round language transitions. Historical measurements below
refer to the earlier C optimization, not a new whole-game Rust speedup.

# Bit-exact QARMA/PAuth acceleration

The final production implementation is in
`src/nb-qemu/qemu/target/arm/pauth_helper.c`:

- SSSE3-capable GNU-compatible x86-64 hosts keep all sixteen QARMA nibbles in
  sixteen SIMD byte lanes for the entire computation. PSHUFB performs S-box
  and cell/tweak permutations, while fixed shifts, masks and lane shuffles
  implement nibble rotations/matrix mixing. Conversion to/from packed uint64
  happens only at entry/exit.
- Other hosts use portable ANF substitution and SWAR matrix mixing. Neither
  selected S-box implementation uses input-indexed memory tables.
- One `__builtin_cpu_supports("ssse3")` test reads the compiler's immutable
  cached CPU-feature set per full PAC. The targeted SSSE3 function is never
  entered on unsupported hosts. No global `-mssse3`/`-march=native`, per-round
  dispatch, CPUID, mutable lazy flag, Rust/C call or environment override is
  introduced.

The Rust generators emit these C intrinsics/kernels, keeping the hot path in
one compiler's inlined computation. They do not create a runtime language
bridge. The portable core retains the original full QARMA body byte-for-byte;
its S-box function arguments are compile-time known. The vector-state version
is independently checked against the original full computation and every
transformed primitive. The entire source tail from `pauth_addpac` remains
byte-for-byte identical to upstream: keys, signing, authentication, failure
behavior and guest security flags are preserved.

The selected deep-08 profile attributed approximately 9% of sampled user CPU
work to the original `pauth_computepac`/`pac_mult`. This is the optimization
justification, not evidence that the entire launch will improve by the math
kernel's speedup. No resident process, game-file RAM preload, lower quality,
resolution change, APK edit or authentication bypass is involved.

## Reproduce the actual-production oracle

From the ATL checkout root:

```sh
python3 tools/pauth-audit/audit.py --random 2000000 --iterations 500000 --repeats 5
python3 tools/pauth-audit/abi-check.py
```

The script verifies pinned upstream source SHA-256, regenerates the candidate
with Rust and requires the actual production source to match exactly before
extracting/compiling it. It separately verifies the unchanged portable core
body and add/auth tail. It runs GCC O2, O3, UBSan and a separate portable-only
translation unit with all SSSE3 sections excluded. Test forcing consists of
calling static extracted variant functions; no force flag reaches the normal
runtime/game API.

Each O2/O3/portable-only build tests two million deterministic random full
64-bit data/modifier/128-bit-key vectors and 39,200 edge vectors. UBSan tests
200,000 random plus the same edges. Every vector compares upstream with the
real dispatcher and forced portable variant; supported hosts also force the
actual full-state SSSE3 implementation. Kernel checks exhaust all 65,536 inputs
of every independent matrix column, every byte at every byte position, every
nibble at every cell, and the randomized words. The SSSE3 primitive checks
also cover pack/unpack, cell and tweak permutations, substitution and matrix
mixing. Edge cases include zero/all ones, alternating patterns, each set/clear
bit, pointer-shaped masks and independent high/low key halves.

`artifacts/report.json` and per-build JSONL retain validation totals, checksums,
compiler versions, CPU affinity and CPU/wall benchmark samples. Calls have a
real dependency chain or four interleaved streams; variant order alternates
and all checksums must match. Final local Ryzen 9800X3D measurements:

| GCC build / workload | Upstream ns/PAC | Full-state dispatch ns/PAC | Speedup |
|---|---:|---:|---:|
| O2, dependent | 242.33 | 50.81 | 4.77× |
| O2, four streams | 237.79 | 43.82 | 5.43× |
| O3, dependent | 212.59 | 51.49 | 4.13× |
| O3, four streams | 211.92 | 41.62 | 5.09× |
| O2, SSSE3 sections excluded, dependent | 238.32 | 138.61 | 1.72× |

The preceding S-box-only SSSE3 production candidate measured approximately
96 ns/PAC. A paired standalone experiment measured a further 1.89× dependent
and 2.19× four-stream improvement from preserving vector state throughout the
whole cipher. `artifacts/vector-state-report.json` preserves that comparison;
it is historical evidence, not the current production oracle report.

Portable-only disassembly contains no SSSE3 instructions. SIMD instructions
appear only in separately targeted functions. The generated kernels avoid
input-dependent memory indexing; this is not a general cryptographic
side-channel certification. An earlier 256-byte S-box table candidate was
rejected because it expanded the secret-indexed memory footprint.

## QEMU helper ABI and real ARM-guest regression

`abi-check.py` checks the actual source contract:

- PAC/AUT helper flags are `TCG_CALL_NO_WG`, meaning no writes to guest globals.
- The x86 TCG target marks `ALL_VECTOR_REGS` as call-clobbered.
- `tcg_reg_alloc_call` frees/spills that register set before global sync and
  the C helper call, independently of NO_WG. `tcg_reg_free` uses `temp_sync`.

Therefore a C PAC helper may use host XMM registers; NO_WG does not promise to
preserve them. No helper flags or register allocator behavior were changed.

The standalone fixture keeps all q0–q31 live through PACIA/PACIB/PACDA/PACDB,
matching AUT instructions, PACGA and corrupted-PAC rejection. It toggles
nontrivial NEON values across the calls, authenticates valid pointers,
requires at least one signing operation to change a pointer (detecting a
no-op/disabled PAC path), and compares all 512 bytes afterward. It executes
64 generated vector states ×64 rounds. Guest q8–q15 are saved/restored to obey
the guest function ABI too.

```sh
bash tools/pauth-audit/check-guest-simd.sh --build-only
bash tools/pauth-audit/check-guest-simd.sh
```

The second command launches only the standalone test ART/ARM guest, not the
game. An optional bridge argument requires its normal sibling thunk layout.
The fixture passed on the earlier installed bridge. The first full-vector
build-directory attempt failed before guest execution because the required
sibling thunk layout was absent; it does not validate or invalidate SIMD math.
The installed-prefix repeat then passed explicitly on the new full-vector
bridge, confirmed in `logs/startup-vector-installed-guest-simd.log`.
`artifacts/guest-simd/run.log` retains the standalone fixture output.

## Artifact map

- `generate.rs`, `vector-state.rs`: dependency-free Rust generators.
- `dispatch.c`: portable/targeted wrappers and feature dispatcher template.
- `harness.c`, `audit.py`: actual-source oracle, UBSan and benchmarks.
- `abi-check.py`, `guest-pauth-simd.{c,S}`, `CLBBArmPauthSimd.java`,
  `check-guest-simd.sh`: source ABI contract and standalone guest fixture.
- `artifacts/original-pauth_helper.c`: exact pinned LGPL oracle.
- `artifacts/candidate-pauth_helper.c`, `generated-kernels.c`,
  `extracted-production-and-oracle.c`, `extracted-portable-only.c`: exact final
  candidate and extractions.
- `artifacts/report.json`, `O2.jsonl`, `O3.jsonl`, `ubsan.jsonl`,
  `portable-only.jsonl`, `portable-only-disassembly.txt`: final evidence.
- `artifacts/pauth-qarma.patch`: focused upstream patch.

The earlier read-only renderer review is `logs/startup-render-audit.md`.
Whole-startup A/B measurements are owned by the parent agent and remain
separate from these offline math measurements.

## Memoization assessment (analysis only)

No PAC memoization was added. Correct reuse would have to match all 64 bits of
data, modifier, both key halves and the algorithm/security context; per-CPU
ownership and key reset/change handling cannot be inferred from a pointer
address alone. A new shared cache could retain key-derived material or add
cross-context timing effects. To assess usefulness first, a separate opt-in
measurement could track exact per-CPU tuples in bounded ephemeral diagnostic
state and emit aggregate hit/miss counts only, never key/input values. That
measurement itself costs time and cannot be a normal startup benchmark.

After the full-vector optimization, even perfect caching can only remove the
remaining fraction of QARMA CPU time, not other startup waits/translation.
Re-profile first. The current fixed-work computation avoids new cache state,
key lifetime rules and hit/miss side channels; no production caching proposal
is justified without a measured substantial hit rate.
