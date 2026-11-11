# Rust migration of the CLBB translator

> Current status (2026-11-09): QEMU is the active backend. ETA experiments and
> their historical plans below are retired and stored outside this checkout;
> see [archive index](../ARCHIVES.md). This directory remains a required runtime
> Rust component and has not been removed. Old ETA relative paths below refer
> to archived history, not active dependencies.

This is the first **runtime Rust module**, not a full QEMU rewrite. The ARM64
decoder, TCG IR/compiler and guest memory/syscall machinery remain in QEMU C.

`src/lib.rs` implements complete QARMA PAC arithmetic for SSSE3 x86_64 hosts.
It ports QEMU `target/arm/pauth_helper.c` (Copyright 2019 Linaro, Ltd.) and the
CLBB vector-state optimization. It retains the source's LGPL-2.0-or-later
license. The original notice/license is in that C file and
`../src/nb-qemu/qemu/COPYING.LIB`; translation does not change provenance.

## Boundary and safety

Four u64 values enter and one u64 leaves through the platform C ABI. No raw
pointer, CPUARMState, allocation, file access, guest memory, mutable global or
Rust runtime crosses the boundary. The no_std object must have **no unresolved
symbols**, checked by the Makefile. Fixed array iteration produces no reachable
panic/bounds-failure routine. Rust is compiled with panic=abort and PIC, without
march=native or a global SSSE3 requirement.

QEMU retains the existing cached CPU feature dispatch. Only supported hosts
enter the targeted Rust function; other hosts retain the C portable fallback.
The Rust entry is hidden in the linked bridge, avoiding interposition/PLT in the
hot path. Unsafe is needed for target-feature calls and fixed 16-byte value
conversions; these are not arbitrary memory dereferences. Rust does not make
the rest of QEMU/FFI or guest-memory syscalls memory safe. In particular, the
existing timespec copyout crash is not fixed by this arithmetic migration.

## Build and checks

The native bridge Makefile compiles this file using rustc and links its PIC
object into libnb-qemu.so; no Cargo download or additional runtime shared
library is required. `build-runtime.sh` runs the differential contract first.

From the project root:

```sh
python3 tools/translator-rust-audit/check.py
python3 tools/pauth-audit/audit.py
python3 tools/pauth-audit/abi-check.py
bash tools/pauth-audit/check-guest-simd.sh
```

The Rust check compares pinned upstream, retained C reference, **actual default
Rust dispatch**, and direct Rust on 2,000,000 random + 34,848 edge cases per
O2/O3 build. Benchmarks use the full C/Rust dispatch boundary, alternating
variant order, pinned CPU, dependent and four independent chains, nine repeats,
and matched output checksums. Results are in `logs/translator-rust-pac/`.
The older pauth audit explicitly tests the generated C reference and portable
fallback; it is not evidence that runtime dispatch still selects C.

The ARM64 fixture verifies signing/authentication, rejection of corrupted PAC
and preservation of all 32 live q-registers through the real native bridge.
Full smoke separately checks guest ABI, threads, signals, SMC and syscalls.
Kernel microbenchmarks are not proof of a whole-game startup improvement.

## Current direction — 2026-11-08

The project decision is Rust with assembly where exact CPU/register/stack
control is needed. QEMU stays as a working execution backend and comparison
oracle while a project-owned ARM64 engine is developed. A literal rewrite of
the entire QEMU tree is no longer the required architecture. Functional
parity comes before optimization, and each enabled replacement must also be
checked for performance regression. Closed game/Unity code remains ARM64.

The authoritative implementation status and next-step sequence are in
`../execution-core/README.md`: first a small Rust reference executor and memory/
platform contracts, then x86-64 code generation, then carefully bounded game
integration. The first standalone reference interpreter is now in `../eta-core/`, with
270 synthetic programs / 34560 differential executions against QEMU. Neither
a game-capable JIT nor a Windows backend is implemented yet. This directory currently contains only runtime PAC arithmetic;
the independent execution interface lives in `../execution-core/`.

C reference code is retained for independent validation and fallback; original
license/provenance remains applicable to translated implementations.
