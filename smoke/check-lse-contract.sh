#!/usr/bin/env bash
# Build only by default. Run explicitly after the current measurement window.
set -euo pipefail
LSE_TEST_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$LSE_TEST_ROOT/env.sh"
LSE_TEST_OUT="$LSE_TEST_ROOT/smoke/lse-contract-artifacts"
LSE_TEST_BIONIC="$LSE_TEST_ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != --run && "$1" != --build-only ) ]]; then
    printf '%s\n' 'usage: check-lse-contract.sh [--build-only | --run]' >&2; exit 2
fi
mkdir -p "$LSE_TEST_OUT/classes"
clang -target aarch64-pc-linux -march=armv8.1-a+lse -mno-outline-atomics -fuse-ld=lld \
 -fPIC -shared -O2 -fno-strict-aliasing -fno-builtin -std=gnu11 -Wall -Wextra -Werror \
 -nostdinc -nostdlib -D__ANDROID__ \
 -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
 -I"$LSE_TEST_BIONIC/libc/include" -I"$LSE_TEST_BIONIC/libc/kernel/android/uapi" \
 -I"$LSE_TEST_BIONIC/libc/kernel/uapi" -I"$LSE_TEST_BIONIC/libc/kernel/uapi/asm-arm64" \
 -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" -Wl,-z,defs \
 -o "$LSE_TEST_OUT/libclbb-arm-lse-contract.so" "$LSE_TEST_ROOT/smoke/arm-lse-contract.c" -lc -ldl
javac --release 8 -Xlint:-options -d "$LSE_TEST_OUT/classes" "$LSE_TEST_ROOT/smoke/CLBBArmLseContract.java"
dx --dex --output="$LSE_TEST_OUT/test.jar" "$LSE_TEST_OUT/classes"
printf '%s\n' 'CLBB_ARM64_LSE_CONTRACT_BUILT=1'
if [[ "${1:-}" != --run ]]; then exit 0; fi
ulimit -c 0
NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
 timeout -k 3s 25s dalvikvm -Xforce-nb-testing -XX:NativeBridge="$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
 -cp "$LSE_TEST_OUT/test.jar" CLBBArmLseContract "$LSE_TEST_OUT/libclbb-arm-lse-contract.so" \
 > "$LSE_TEST_OUT/run.log" 2>&1
tr -d '\000' < "$LSE_TEST_OUT/run.log" | rg '^CLBB_ARM64_LSE_'
tr -d '\000' < "$LSE_TEST_OUT/run.log" | rg -q '^CLBB_ARM64_LSE_CONTRACT_OK=1$'
