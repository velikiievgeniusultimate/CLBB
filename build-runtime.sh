#!/usr/bin/env bash
# Rebuild the patched runtime in this existing experiment; no dependency bootstrap.
set -euo pipefail
CLBB_BUILD_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
CLBB_CHECK_ONLY=0
case "${1:-}" in
    --check) CLBB_CHECK_ONLY=1 ;;
    --help)
        printf '%s\n' 'Usage: bash build-runtime.sh [--check]' \
            'CLBB_BUILD_JOBS=N controls parallelism (default: 4).' \
            '--check validates prerequisites and checks for this runtime in use; no build/install.'
        exit 0 ;;
    '') ;;
    *) printf 'Unknown argument: %s\n' "$1" >&2; exit 2 ;;
esac
[[ $# -le 1 ]] || { printf '%s\n' 'Too many arguments.' >&2; exit 2; }
source "$CLBB_BUILD_ROOT/env.sh"
fail() { printf 'build-runtime: %s\n' "$*" >&2; exit 1; }
[[ "$CLBB_ATL_ROOT" == "$CLBB_BUILD_ROOT" ]] || fail 'env.sh points at another checkout; update it first.'
[[ "$CLBB_ATL_PREFIX" == "$CLBB_BUILD_ROOT/prefix" && -d "$CLBB_ATL_PREFIX" && ! -L "$CLBB_ATL_PREFIX" ]] ||
    fail 'Expected an existing, non-symlink experiment prefix/.'
[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] || fail 'This nb-qemu build supports Linux x86_64 hosts only.'
CLBB_BUILD_JOBS="${CLBB_BUILD_JOBS:-4}"
[[ "$CLBB_BUILD_JOBS" =~ ^[1-9][0-9]*$ ]] || fail 'CLBB_BUILD_JOBS must be a positive integer.'
for CLBB_TOOL in cargo rustc make install meson ninja clang clang++ cc c++ pkgconf python3 flock javac dx cmp; do
    command -v "$CLBB_TOOL" >/dev/null || fail "Missing command: $CLBB_TOOL"
done
cmp -s "$CLBB_BUILD_ROOT/dns-adapter/include/clbb_dns.h" \
    "$CLBB_BUILD_ROOT/src/nb-qemu/libnb-qemu/ClbbDnsWire.h" || fail 'Host DNS wire header differs from the adapter.'
cmp -s "$CLBB_BUILD_ROOT/dns-adapter/include/clbb_dns.h" \
    "$CLBB_BUILD_ROOT/src/nb-qemu/libnb-qemu-guest/bionic/libc/private/clbb_dns_wire.h" ||
    fail 'Guest DNS wire header differs from the adapter.'
for CLBB_REQUIRED in execution-core/src/lib.rs execution-core/src/abi.rs execution-core/clbb_execution.h \
    translator-rust/src/lib.rs webkit-launch-adapter/Cargo.lock dns-adapter/Cargo.lock src/nb-qemu/Makefile \
    src/nb-qemu/qemu/configure src/art_standalone/Makefile \
    src/art_standalone/art/runtime/jni/java_vm_ext.cc src/atl/meson.build mapper/runtime/Cargo.lock \
    prefix/lib/pkgconfig/art-standalone.pc prefix/lib/libc_bio.so \
    prefix/lib/art/libart.so \
    prefix/share/java/asm.jar prefix/share/java/asm-tree.jar prefix/share/java/java-getopt.jar; do
    [[ -f "$CLBB_BUILD_ROOT/$CLBB_REQUIRED" ]] || fail "Missing prerequisite: $CLBB_REQUIRED"
done

# Exact local identity only: executable or runtime mappings below this prefix,
# or a run/probe script resolving to this checkout. Never match a generic process
# name such as waydroid, java, timeout or dalvikvm.
check_runtime_idle() {
    python3 - "$CLBB_BUILD_ROOT" "$CLBB_ATL_PREFIX" <<'PY'
import os
from pathlib import Path
import sys

root, prefix = map(os.path.realpath, sys.argv[1:])
executables = {prefix + '/bin/' + name for name in
               ('android-translation-layer', 'dalvikvm', 'dalvikvm32', 'dalvikvm64')}
mapped_files = {prefix + '/lib/libnb-qemu.so', prefix + '/lib/libclbb_dns_adapter.so',
                prefix + '/lib/art/libart.so', prefix + '/lib/libclbb_webkit_launch_adapter.so',
                prefix + '/lib/libclbb_mapper.so', prefix + '/lib/libOpenSLES.so.1.0.1'}
scripts = {str(p.resolve()) for p in Path(root).glob('probe-*.sh')}
scripts.add(root + '/run-mlbb.sh')
busy = []
for process in Path('/proc').iterdir():
    if not process.name.isdecimal():
        continue
    reason = None
    executable = ''
    try:
        executable = os.readlink(process / 'exe').removesuffix(' (deleted)')
        if executable in executables:
            reason = executable
    except OSError:
        pass
    if reason is None:
        try:
            for mapping in (process / 'maps').read_text().splitlines():
                fields = mapping.split(None, 5)
                if len(fields) == 6 and fields[5].removesuffix(' (deleted)') in mapped_files:
                    reason = fields[5]
                    break
        except OSError:
            pass
    if reason is None and os.path.basename(executable) in ('bash', 'sh', 'dash', 'zsh', 'ksh'):
        try:
            arguments = (process / 'cmdline').read_bytes().split(b'\0')
            cwd = os.readlink(process / 'cwd')
            for raw in arguments[1:]:
                argument = os.fsdecode(raw)
                if argument in ('-c', '-lc', '-ic'):
                    break
                if argument.startswith('-'):
                    continue
                if argument.endswith('.sh'):
                    candidate = os.path.realpath(os.path.join(cwd, argument))
                    if candidate in scripts:
                        reason = candidate
                break
        except OSError:
            pass
    if reason:
        busy.append((process.name, reason))
if busy:
    print('build-runtime: this experiment is in use; stop its run/probe before installing:', file=sys.stderr)
    for pid, reason in busy:
        print(f'  PID {pid}: {reason}', file=sys.stderr)
    sys.exit(1)
PY
}

check_runtime_idle
if (( CLBB_CHECK_ONLY )); then
    printf '%s\n' 'build-runtime: preflight passed; no build or install performed.'
    exit 0
fi
# Serializes invocations of this script. Existing launchers do not share the lock;
# repeat the process check before every install, and do not launch during a build.
exec 9>"$CLBB_BUILD_ROOT/.build-runtime.lock"
flock -n 9 || fail 'Another build-runtime.sh invocation is active.'

CLBB_RUST_HOST=''
while IFS= read -r CLBB_RUST_LINE; do
    case "$CLBB_RUST_LINE" in 'host: '*) CLBB_RUST_HOST="${CLBB_RUST_LINE#host: }" ;; esac
done < <(rustc -vV)
[[ "$CLBB_RUST_HOST" == x86_64-*-linux-* ]] || fail 'Rust compiler host does not match Linux x86_64.'
CLBB_DNS_DIR="$CLBB_BUILD_ROOT/dns-adapter"
CLBB_DNS_TARGET="$CLBB_DNS_DIR/target"
CLBB_NB_DIR="$CLBB_BUILD_ROOT/src/nb-qemu"
CLBB_ART_DIR="$CLBB_BUILD_ROOT/src/art_standalone"
CLBB_ATL_DIR="$CLBB_BUILD_ROOT/src/atl"

printf '%s\n' 'Building Rust DNS adapter (offline, locked, release)...'
cargo build --manifest-path "$CLBB_DNS_DIR/Cargo.toml" --offline --locked --release \
    --target "$CLBB_RUST_HOST" --target-dir "$CLBB_DNS_TARGET" -j "$CLBB_BUILD_JOBS"
check_runtime_idle
install -m 0755 "$CLBB_DNS_TARGET/$CLBB_RUST_HOST/release/libclbb_dns_adapter.so" \
    "$CLBB_ATL_PREFIX/lib/libclbb_dns_adapter.so"

printf '%s\n' 'Building Rust WebKit launch adapter (offline, locked, release)...'
cargo build --manifest-path "$CLBB_BUILD_ROOT/webkit-launch-adapter/Cargo.toml" --offline --locked --release \
    --target "$CLBB_RUST_HOST" --target-dir "$CLBB_BUILD_ROOT/webkit-launch-adapter/target" -j "$CLBB_BUILD_JOBS"
check_runtime_idle
install -m 0755 "$CLBB_BUILD_ROOT/webkit-launch-adapter/target/$CLBB_RUST_HOST/release/libclbb_webkit_launch_adapter.so" \
    "$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so"

printf '%s\n' 'Building Rust mapper and common controller (offline, locked, release)...'
cargo build --manifest-path "$CLBB_BUILD_ROOT/mapper/runtime/Cargo.toml" --offline --locked --release \
    --target "$CLBB_RUST_HOST" --target-dir "$CLBB_BUILD_ROOT/mapper/runtime/target" -j "$CLBB_BUILD_JOBS"
check_runtime_idle
install -m 0755 "$CLBB_BUILD_ROOT/mapper/runtime/target/$CLBB_RUST_HOST/release/libclbb_mapper.so" \
    "$CLBB_ATL_PREFIX/lib/libclbb_mapper.so"

printf '%s\n' 'Building shared master-volume OpenSLES adapter...'
CLBB_SLES_SETUP=(setup)
[[ ! -f "$CLBB_BUILD_ROOT/src/libopensles-standalone/build/meson-private/coredata.dat" ]] || CLBB_SLES_SETUP+=(--reconfigure)
meson "${CLBB_SLES_SETUP[@]}" "$CLBB_BUILD_ROOT/src/libopensles-standalone/build" "$CLBB_BUILD_ROOT/src/libopensles-standalone" \
    --prefix "$CLBB_ATL_PREFIX" --libdir lib --buildtype release --wrap-mode nodownload
meson compile -C "$CLBB_BUILD_ROOT/src/libopensles-standalone/build" -j "$CLBB_BUILD_JOBS"
check_runtime_idle
DESTDIR= meson install -C "$CLBB_BUILD_ROOT/src/libopensles-standalone/build" --no-rebuild

printf '%s\n' 'Building patched ART shared runtime incrementally...'
# Use the file target: the phony libart target also builds the static runtime.
make -C "$CLBB_ART_DIR" -j "$CLBB_BUILD_JOBS" out/host/linux-x86/lib64/libart.so
check_runtime_idle
install -m 0755 "$CLBB_ART_DIR/out/host/linux-x86/lib64/libart.so" \
    "$CLBB_ATL_PREFIX/lib/art/libart.so"

printf '%s\n' 'Building QEMU, host bridge and guest bionic...'
# The generated table is deliberately inlined in QEMU. Keep the Rust definition
# and the complete finite-domain C oracle check part of a reproducible rebuild.
"$CLBB_BUILD_ROOT/tools/rsqrt-estimate-audit/check.sh"
# Keep generated C reference and the runtime Rust PAC port against upstream.
bash "$CLBB_BUILD_ROOT/execution-core/check.sh"
python3 "$CLBB_BUILD_ROOT/tools/translator-rust-audit/check.py"
python3 "$CLBB_BUILD_ROOT/tools/pauth-audit/audit.py"
# The upstream Makefile concatenates PREFIX and LIBDIR: the slash is required.
make -C "$CLBB_NB_DIR" -j "$CLBB_BUILD_JOBS" \
    PREFIX="$CLBB_ATL_PREFIX/" LIBDIR=lib CLBB_NATIVE_BRIDGE_DNS=1 \
    NB_BRIDGE_OPT='-O2 -fno-strict-aliasing -fno-omit-frame-pointer' \
    NB_GUEST_OPT='-O2 -fno-strict-aliasing' \
    BIONIC_OPT='-O2 -fno-strict-aliasing -fno-builtin -mno-outline-atomics' \
    NB_BRIDGE_LOG_NDEBUG=1 default
check_runtime_idle
make -C "$CLBB_NB_DIR" PREFIX="$CLBB_ATL_PREFIX/" LIBDIR=lib \
    INSTALL_PREFIX="$CLBB_ATL_PREFIX/" install
"$CLBB_BUILD_ROOT/tools/tb-state-audit/check.sh"
bash "$CLBB_BUILD_ROOT/tools/pauth-audit/check-guest-simd.sh"

printf '%s\n' 'Configuring and building ATL with local Java tool dependencies...'
CLBB_MESON_SETUP=(setup)
[[ ! -f "$CLBB_ATL_DIR/build/meson-private/coredata.dat" ]] || CLBB_MESON_SETUP+=(--reconfigure)
meson "${CLBB_MESON_SETUP[@]}" "$CLBB_ATL_DIR/build" "$CLBB_ATL_DIR" \
    --prefix "$CLBB_ATL_PREFIX" --libdir lib --buildtype release --wrap-mode nodownload \
    -Ddownload_tool_deps=false \
    -Dasm_jar_path="$CLBB_ATL_PREFIX/share/java/asm.jar" \
    -Dasm_tree_jar_path="$CLBB_ATL_PREFIX/share/java/asm-tree.jar" \
    -Dgetopt_jar_path="$CLBB_ATL_PREFIX/share/java/java-getopt.jar"
meson compile -C "$CLBB_ATL_DIR/build" -j "$CLBB_BUILD_JOBS"
check_runtime_idle
DESTDIR= meson install -C "$CLBB_ATL_DIR/build" --no-rebuild
printf '%s\n' 'Runtime installed in the experiment prefix. Run smoke/build-all.sh and smoke/run-checks.sh separately.'
