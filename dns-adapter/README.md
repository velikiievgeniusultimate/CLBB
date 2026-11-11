# Rootless host DNS adapter

This Rust std-only Linux adapter is integrated into the experiment's patched
nb-qemu and standalone AArch64 bionic. It uses the host OS getaddrinfo, with
no Android netd dependency or hardcoded DNS server. APKs and game libraries are
unchanged. Windows is **not implemented**: this crate explicitly rejects
unsupported targets. The adapter supports 64-bit little-endian Linux; the whole
nb-qemu build currently targets a Linux x86_64 host and AArch64 guest.

## Rebuild the existing runtime

From the experiment directory:

```bash
bash build-runtime.sh --check
CLBB_BUILD_JOBS=4 bash build-runtime.sh
```

The script reads env.sh and requires its checkout/prefix paths to match the
script location. It builds Cargo with --offline --locked --release and an
explicit host target, installs libclbb_dns_adapter.so into prefix/lib, then
incrementally builds only ART's shared runtime using the exact file target:

```bash
make -C src/art_standalone -j4 out/host/linux-x86/lib64/libart.so
```

It checks that the runtime is idle before installing that one file into
prefix/lib/art/libart.so. This includes the patched NativeBridge failure/error
reporting in art/runtime/jni/java_vm_ext.cc: NativeBridge failures now read
NativeBridgeGetError(), with a fallback message for NULL, instead of reading
the unrelated host bionic_dlerror() and passing its NULL result to strlen. The phony libart
target is deliberately not used because it also builds the static runtime.
No other ART binaries, Java artifacts or libraries are reinstalled by this step.

Next it builds and installs patched QEMU/nb-qemu/guest bionic with PREFIX=.../ and
LIBDIR=lib. The trailing slash matters because upstream concatenates these
values. Guest DNS is enabled with CLBB_NATIVE_BRIDGE_DNS=1. Finally it sets up
or reconfigures src/atl/build, compiles and installs ATL with Meson release
settings, --libdir lib, --wrap-mode nodownload, and local Java tool jars.
Host and guest wire header copies must match this adapter's C header exactly.

This rebuild workflow assumes the existing dependencies, source trees and local
patches are already present. It does not fetch packages, apply patches, bootstrap
the full ART stack/bionic_translation/wolfSSL/OpenSLES/libglvnd or rebuild the separate guest
zlib. Source revisions are recorded in sources.json; patches live in patches.
It is a repeatable build command, not a claim of bit-identical artifacts or a
pinned compiler toolchain. GNU Make's existing incremental dependency tracking
still applies; after header-only upstream changes, rebuild the affected objects
explicitly because the handwritten Makefiles do not track every header.

Before installation the script checks /proc for exact executable/runtime
mapping paths under this prefix, plus shell execution of this checkout's
run-mlbb.sh/probe-*.sh. It does not match generic Waydroid/Java/process names
and never kills a process. The check repeats before each install. A flock
serializes this build script, but existing launchers do not share that lock:
**do not start a game/probe while rebuilding**. Process scanning cannot make
multiple installation steps atomic, and inaccessible /proc entries cannot be
inspected. A failed later stage can leave an earlier component already installed.
--check runs preflight only; sourcing env.sh can create its cache/data dirs.
Python 3.9+ is needed for the process check. Validation during authoring covered
Bash/Python syntax, the help path and eight synthetic process-identity cases;
the build/install stages were not executed.

## Verification

Run separately after rebuilding:

```bash
bash dns-adapter/test.sh
./smoke/build-all.sh
./smoke/run-checks.sh
```

The isolated C ABI harness tests numeric IPv4/IPv6, numeric scope, ports/protocol,
localhost canonical name, passive wildcard and errors/output reset. Its small
test-only seccomp filter denies socket creation, connection and send syscalls.
It requires no external DNS access. No such filter is installed in the runtime.

smoke/arm-dns.c and smoke/CLBBArmDns.java exercise actual guest
getaddrinfo/freeaddrinfo, canonical name, numeric IPv4/IPv6, error handling,
gethostbyname2_r, unaligned buffers, ERANGE and two concurrent guest pthreads.
Localhost IPv6 is optional when the host lacks it; numeric IPv6 remains required.
The existing logs/check-Dns.log records CLBB_ARM64_DNS_OK=125 for this
integration. These checks use localhost/numeric addresses only and do not prove
external DNS, game-server connectivity or a successful game login.

NB_QEMU_TRACE_DNS=1 enables bridge diagnostics containing the requested
node/service, result status and record count. Use it only when needed:
query names become part of the log. Individual names are bounded by transport
limits, but the number of log entries is not capped.

## Wire contract

include/clbb_dns.h and src/wire.rs define the v1 pointer-free payload.
Families use 0/4/6 and flags use normalized values, independent of native AF/AI
constants. Records contain TCP/UDP type/protocol, address bytes, host-order port,
IPv6 scope and flowinfo. At most 64 records and 1023 canonical-name bytes are
accepted. Overflow fails the request without silent truncation. Integers use
little endian on the supported hosts. Native addrinfo/sockaddr pointers and
allocator ownership never cross into the guest.

The C API arguments themselves are host pointers and require valid, aligned,
non-overlapping buffers. Native structures stay inside src/linux.rs; errors
are translated from Linux's negative EAI values to this guest's positive EAI
values. Output resets on errors, retaining system_errno only for EAI_SYSTEM.
Resolution is synchronous and follows the host NSS/hosts/DNS configuration and
OS timeouts. There is no additional cache, cancellation or timeout mechanism.

## Integrated path

* src/nb-qemu/libnb-qemu/OsBridge.cpp dispatches SVC 0x2800 directly to
  DnsBridge.cpp. Its arguments are x0=node, x1=service, x2=wire hints,
  x3=wire response. x0 returns a positive guest EAI code, or zero on success.
  Guest strings and payloads are copied through QEMU memory access helpers;
  node/service limits are 1024/256 bytes. JNI keeps its existing syscall handler.
* libnb-qemu-guest/bionic/libc/dns/net/clbb_dns.c under that same source tree
  issues the SVC and allocates results with the guest allocator. Each addrinfo
  has contiguous sockaddr storage compatible with guest freeaddrinfo. Legacy
  hostent results use aligned pointer tables in the caller's buffer and follow
  existing _r ENOSPC-to-ERANGE handling; aliases are empty.
* Guest getaddrinfo and gethostbyname_internal use this path when netd proxy
  opening is unavailable and the network context is completely default: all
  netid/mark fields unset, uid invalid, flags zero and no qhook. Other contexts
  retain the Android resolver path. Guest numeric/null getaddrinfo fast paths
  remain local. Only the standalone bionic Makefile enables the helper;
  Android.bp/full Android builds retain their existing behavior.

The original guest resolver ignored resolv.conf and had no Android daemon to
provision per-network name servers. Its default server addresses were 0.0.0.0/::;
setting Android properties alone would not fix this source version. The host
resolver adapter removes that missing configuration dependency for default
network forward lookups. Reverse lookup/getnameinfo are not implemented by this
adapter. The separate host-architecture wrapper in
src/bionic_translation/libc/libc.c is not used as a guest wire bridge.

A future Windows adapter must initialize Winsock, handle UTF encoding, translate
native flags/errors/families, and emit the same pointer-free wire payload.
It cannot reuse Linux addrinfo or sockaddr layouts.
