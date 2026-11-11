#!/usr/bin/env bash
set -euo pipefail
CLBB_ZLIB_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
source "$CLBB_ZLIB_ROOT/env.sh"
CLBB_BIONIC="$CLBB_ZLIB_ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
cd "$CLBB_ZLIB_ROOT/src/zlib"
clang -target aarch64-pc-linux -fuse-ld=lld -fPIC -shared -O2 -std=gnu11 \
    -nostdinc -nostdlib -D__ANDROID__ -D_LARGEFILE64_SOURCE \
    -I. -I"$(clang -print-resource-dir)/include" \
    -I"$CLBB_BIONIC/libc/include" \
    -I"$CLBB_BIONIC/libc/kernel/android/uapi" \
    -I"$CLBB_BIONIC/libc/kernel/uapi" \
    -I"$CLBB_BIONIC/libc/kernel/uapi/asm-arm64" \
    -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
    -Wl,-soname,libz.so -Wl,--hash-style=both -Wl,-z,defs \
    -o "$CLBB_ATL_PREFIX/share/libnb-qemu-guest/libz.so" \
    adler32.c compress.c crc32.c deflate.c gzclose.c gzlib.c gzread.c gzwrite.c \
    infback.c inffast.c inflate.c inftrees.c trees.c uncompr.c zutil.c -lc -ldl
