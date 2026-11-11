#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/env.sh"
BIONIC="$ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
clang -target aarch64-pc-linux -fuse-ld=lld -fPIC -shared -O2 -std=gnu11 \
 -nostdinc -nostdlib -D__ANDROID__ \
 -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
 -I"$BIONIC/libc/include" -I"$BIONIC/libc/kernel/android/uapi" \
 -I"$BIONIC/libc/kernel/uapi" -I"$BIONIC/libc/kernel/uapi/asm-arm64" \
 -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" -Wl,-z,defs \
 -o "$ROOT/smoke/libclbb-arm-$1.so" "$ROOT/smoke/arm-$1.c" -lc -ldl
