#!/usr/bin/env bash
set -euo pipefail
PAUTH_AUDIT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
PAUTH_AUDIT_ROOT="$(cd -- "$PAUTH_AUDIT_DIR/../.." && pwd)"
source "$PAUTH_AUDIT_ROOT/env.sh"
ulimit -c 0
PAUTH_AUDIT_OUT="$PAUTH_AUDIT_DIR/artifacts/guest-simd"
PAUTH_AUDIT_BIONIC="$PAUTH_AUDIT_ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
mkdir -p "$PAUTH_AUDIT_OUT/classes"
clang -target aarch64-pc-linux -march=armv8.3-a+simd -fuse-ld=lld -fPIC -shared -O2 -std=gnu11 \
 -nostdinc -nostdlib -D__ANDROID__ \
 -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
 -I"$PAUTH_AUDIT_BIONIC/libc/include" -I"$PAUTH_AUDIT_BIONIC/libc/kernel/android/uapi" \
 -I"$PAUTH_AUDIT_BIONIC/libc/kernel/uapi" -I"$PAUTH_AUDIT_BIONIC/libc/kernel/uapi/asm-arm64" \
 -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" -Wl,-z,defs \
 -o "$PAUTH_AUDIT_OUT/libclbb-pauth-simd.so" \
 "$PAUTH_AUDIT_DIR/guest-pauth-simd.c" "$PAUTH_AUDIT_DIR/guest-pauth-simd.S" -lc -ldl
javac --release 8 -Xlint:-options -d "$PAUTH_AUDIT_OUT/classes" "$PAUTH_AUDIT_DIR/CLBBArmPauthSimd.java"
dx --dex --output="$PAUTH_AUDIT_OUT/test.jar" "$PAUTH_AUDIT_OUT/classes"
if [[ "${1:-}" == "--build-only" ]]; then
  printf '%s\n' 'CLBB_ARM64_PAUTH_SIMD_FIXTURE_BUILT=1'
  exit 0
fi
PAUTH_AUDIT_BRIDGE="${1:-$CLBB_ATL_PREFIX/lib/libnb-qemu.so}"
NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
 timeout -k 3s 25s dalvikvm -Xforce-nb-testing \
 -XX:NativeBridge="$PAUTH_AUDIT_BRIDGE" \
 -cp "$PAUTH_AUDIT_OUT/test.jar" CLBBArmPauthSimd \
 "$PAUTH_AUDIT_OUT/libclbb-pauth-simd.so" > "$PAUTH_AUDIT_OUT/run.log" 2>&1
rg -a 'CLBB_ARM64_PAUTH_SIMD_OK=' "$PAUTH_AUDIT_OUT/run.log"
