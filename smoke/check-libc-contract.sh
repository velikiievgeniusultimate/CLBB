#!/usr/bin/env bash
# Build by default. Execution is explicit: ./smoke/check-libc-contract.sh --run [bridge.so]
set -euo pipefail
LIBC_TEST_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$LIBC_TEST_ROOT/env.sh"
LIBC_TEST_OUT="$LIBC_TEST_ROOT/smoke/libc-contract-artifacts"
LIBC_TEST_BIONIC="$LIBC_TEST_ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
if [[ $# -gt 2 || ( $# -gt 0 && "$1" != --run && "$1" != --build-only ) ]]; then
    printf '%s\n' 'usage: check-libc-contract.sh [--build-only | --run [bridge.so]]' >&2
    exit 2
fi
mkdir -p "$LIBC_TEST_OUT/classes"
clang -target aarch64-pc-linux -march=armv8-a -mno-outline-atomics -fuse-ld=lld \
 -fPIC -shared -O2 -fno-strict-aliasing -fno-builtin -std=gnu11 -Wall -Wextra -Werror \
 -nostdinc -nostdlib -D__ANDROID__ \
 -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
 -I"$LIBC_TEST_BIONIC/libc/include" -I"$LIBC_TEST_BIONIC/libc/kernel/android/uapi" \
 -I"$LIBC_TEST_BIONIC/libc/kernel/uapi" -I"$LIBC_TEST_BIONIC/libc/kernel/uapi/asm-arm64" \
 -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" -Wl,-z,defs \
 -o "$LIBC_TEST_OUT/libclbb-arm-libc-contract.so" "$LIBC_TEST_ROOT/smoke/arm-libc-contract.c" -lc -ldl
javac --release 8 -Xlint:-options -d "$LIBC_TEST_OUT/classes" "$LIBC_TEST_ROOT/smoke/CLBBArmLibcContract.java"
dx --dex --output="$LIBC_TEST_OUT/test.jar" "$LIBC_TEST_OUT/classes"
printf '%s\n' 'CLBB_ARM64_LIBC_CONTRACT_BUILT=1'
if [[ "${1:-}" != --run ]]; then exit 0; fi
ulimit -c 0
LIBC_TEST_BRIDGE="${2:-$CLBB_ATL_PREFIX/lib/libnb-qemu.so}"
# No APK, game state, prefix writes, or changes to settings are involved.
NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
 timeout -k 3s 45s dalvikvm -Xforce-nb-testing -XX:NativeBridge="$LIBC_TEST_BRIDGE" \
 -cp "$LIBC_TEST_OUT/test.jar" CLBBArmLibcContract "$LIBC_TEST_OUT/libclbb-arm-libc-contract.so" \
 > "$LIBC_TEST_OUT/run.log" 2>&1
tr -d '\000' < "$LIBC_TEST_OUT/run.log" | rg '^CLBB_(LIBC_|ARM64_LIBC_CONTRACT_OK=)'
tr -d '\000' < "$LIBC_TEST_OUT/run.log" | rg -q '^CLBB_ARM64_LIBC_CONTRACT_OK=1$'
