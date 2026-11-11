# Exact ARM reciprocal-square-root estimate lookup

The production `do_recip_sqrt_estimate(int)` has exactly **384 valid inputs**,
128–511. Its result is always 256–511. The audit stores `result - 256` as one
byte per input: **384 bytes of read-only data**, with no runtime generation.
`src/lib.rs` computes that table using integer-only Rust `const fn` evaluation.
The product uses the generated inline C lookup, avoiding a Rust FFI call on each
estimate. Rust is required for regeneration/checking, not for this QEMU hot path.

The production changes are limited to including `rsqrt-estimate-table.h` inside
`CONFIG_TCG` and replacing the helper's integer loop with a table load. Both
original assertions are retained. The f16/f32/f64 NaN, infinity, sign, zero,
denormal, exception-state, exponent and packing paths, and the u32 sentinel
check remain unchanged. There is no floating-point calculation, host rsqrt
instruction, approximation, fast-math flag, or altered guest ARM behavior.

## Finite-domain proof and oracle

- Odd exponent: the floating wrapper prepends binary `01` to seven fraction
  bits, so input is 128–255.
- Even exponent: it prepends binary `1` to eight fraction bits, so input is
  256–511. Subnormal normalization does not change this prefix invariant.
- The u32 helper rejects inputs with both top bits clear before extracting its
  upper nine bits. All remaining extracted inputs are 128–511. Its low 23 bits
  do not affect the result.
- All 384 inputs are checked against `src/oracle-vfp-helper.c`, frozen unchanged
  from the pre-optimization production source. The original whole-file SHA-256
  was `e9855e05426ee18d39aaec17a03c36bed8f2bbb2231c4aa20f1de030d9abb04a`.
- The generated test compiles that original function, the **current production
  function extracted from QEMU**, the generated C lookup, and a native Rust ABI
  lookup. Every valid input must match exactly. Another 1,024 checks cover both
  low-bit endpoints of all 512 u32 upper-bit equivalence classes, including the
  sentinel classes.
- UBSan runs with assertions enabled. The largest original signed multiply over
  the complete domain is 269,397,130, below `INT_MAX`. No signed-overflow behavior
  is being substituted.

The oracle and generated header preserve the upstream LGPL 2.1-or-later notice
and Fabrice Bellard's original copyright notice. `artifacts/provenance.json`
records pre/post-source and header hashes. No new third-party crate is used.

## Reproducible check before a QEMU build

From the ATL workspace root:

```sh
tools/rsqrt-estimate-audit/check.sh
```

This builds only the standalone audit tool and a small C oracle executable. It
checks the committed generated header byte-for-byte, requires the exact audited
production helper, runs all Rust tests and the exhaustive C/UBSan comparison,
and **does not build, relink, install or restart QEMU/the game**.

To regenerate after reviewing changes to the Rust definition:

```sh
cargo build --release --offline --manifest-path tools/rsqrt-estimate-audit/Cargo.toml
tools/rsqrt-estimate-audit/target/release/clbb-rsqrt-estimate-audit --generate-only
cp tools/rsqrt-estimate-audit/artifacts/rsqrt-table.h src/nb-qemu/qemu/target/arm/rsqrt-estimate-table.h
tools/rsqrt-estimate-audit/check.sh
```

Patch: `artifacts/qemu-rsqrt-estimate-lookup.patch`. Its reverse dry-run was
checked against the modified QEMU source. It contains only the helper change
and the generated header.

After startup/performance baselines finish, the existing archive can be rebuilt
without rerunning configure:

```sh
source env.sh
tools/rsqrt-estimate-audit/check.sh
make -C src/nb-qemu/qemu/_build/aarch64-linux-user -j6 qemu-aarch64.a
export TOPDIR="$PWD/src/nb-qemu/"
export BUILDDIR="$TOPDIR/builddir"
export HOSTARCH=i386 HOSTARCH_FULL=x86_64 TARGETARCH=arm TARGETARCH_FULL=aarch64
export QEMU_BUILDDIR="$TOPDIR/qemu/_build/"
export QEMU_STATIC_LIB="$QEMU_BUILDDIR/aarch64-linux-user/qemu-aarch64.a"
export LIBQEMUUTIL="$QEMU_BUILDDIR/libqemuutil.a"
export LIBDIR_PATH="$CLBB_ATL_PREFIX/lib"
make -C src/nb-qemu/libnb-qemu -j6 host-libs
```

Preserve the currently selected bridge build directory/optimization flags for a
controlled A/B comparison; changing bridge optimization at the same time would
confound the lookup measurement. The initial isolated audit did not build QEMU. A later authorized stage rebuilt
this archive and the release-AB bridge/thunks with the flags below, passed all
14 ARM smoke checks and real Mesa thunk readback, and refreshed the six-library
install plan. No libraries were installed and the game was not restarted.
Installation and runtime measurements remain separate.

## Isolated hot benchmark

```sh
cargo build --release --offline --manifest-path tools/rsqrt-estimate-audit/Cargo.toml
tools/rsqrt-estimate-audit/target/release/clbb-rsqrt-estimate-audit --iterations 2000000 --repeats 7
```

The C harness uses the extracted production oracle, volatile runtime inputs and
an observable checksum. Five methods run in rotating order to reduce ordering
bias: direct production C, inline generated C, native-call production C,
native-call generated C, and native Rust ABI. Flags are `-O2`/`-O3`, `-DNDEBUG`,
`-fno-lto`, with no fast-math. Trials are repeated for a pseudorandom near-uniform
input set and the maximum/minimum loop-work inputs.

The initial report (`artifacts/report.json`) on Ryzen 7 9800X3D/GCC 16.2.1:

| Input distribution | Original C direct, ns/call | Inline C table | Native Rust ABI |
| --- | ---: | ---: | ---: |
| Near-uniform 128–511 | 47.064 | 0.383 | 0.767 |
| Input 128, 510 iterations | 128.781 | 0.383 | 0.767 |
| Input 511, zero iterations | 0.896 | 0.382 | 0.765 |

These are medians of **hot-loop throughput** for independent calls, including
input loading/accumulation; they are not dependent-operation latency or game FPS.
`-O3` gave similar numbers. Original total loop work over all 384 inputs is
65,340 iterations (170.15625 on average). Native Rust ABI and native C lookup
calls cost about the same; inlining avoids that small call overhead. Actual
input distribution and cache behavior matter. A function accounting for 15% of
CPU time cannot yield a 123× whole-game speedup: eliminating its cost would at
most reduce that measured CPU work by roughly 15%, before other bottlenecks.

## Authorized build result

- Archive: `src/nb-qemu/qemu/_build/aarch64-linux-user/qemu-aarch64.a`.
- Release artifacts: `src/nb-qemu/builddir-release-ab/libnb-qemu/`.
- Flags: `-O2 -fno-strict-aliasing -fno-omit-frame-pointer -DLOG_NDEBUG=1`.
- Updated hashes and exact destinations: `logs/release-ab-install-plan.json`.
- ARM logs: `logs/release-ab-rsqrt-*.log`; real Mesa readback:
  `logs/release-ab-rsqrt-gles.log`.
- The six installed prefix files retained their original baseline hashes.
- Dynamic symbol comparison: `logs/release-ab-rsqrt-dynamic-symbols.json`;
  no newly unresolved symbols relative to the installed baseline.

The main QEMU object disassembly shows the old estimate loop eliminated and a
byte table load in `recip_sqrt_estimate`. This is a build artifact check; it does
not establish end-to-end startup or FPS improvements before installation.
