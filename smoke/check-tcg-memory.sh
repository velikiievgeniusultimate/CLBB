#!/usr/bin/env bash
# Exercise guest atomics, precise faults, invalidation and a full TCG cache.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bridge=${1:?pass the absolute path of the NativeBridge}
out=$(mktemp -d)
trap 'rm -rf -- "$out"' EXIT
export CLBB_STATE_ROOT="$out/state"
mkdir -p "$CLBB_STATE_ROOT" "$out/classes"
source "$root/env.sh"
bionic="$root/src/nb-qemu/libnb-qemu-guest/bionic/libc"
guest="$CLBB_ATL_PREFIX/share/libnb-qemu-guest"
javac --release 8 -Xlint:-options -d "$out/classes" "$root/smoke/CLBBArmLseContract.java"
dx --dex --output="$out/test.jar" "$out/classes"
ulimit -c 0
for fixture in arm-tcg-atomics arm-tcg-recycle; do
    clang -target aarch64-pc-linux -march=armv8-a -mno-outline-atomics -fuse-ld=lld \
     -shared -fPIC -O2 -Wall -Wextra -Werror -std=gnu11 -nostdinc -nostdlib -D__ANDROID__ \
     -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
     -I"$bionic/include" -I"$bionic/kernel/android/uapi" -I"$bionic/kernel/uapi" \
     -I"$bionic/kernel/uapi/asm-arm64" -L"$guest" -Wl,-z,defs \
     -o "$out/$fixture.so" "$root/smoke/$fixture.c" -lc -ldl
    models=(cortex-a72)
    if [[ $fixture == arm-tcg-atomics ]]; then models+=(max); fi
    for model in "${models[@]}"; do
        CLBB_TCG_CACHE_MIB=32 QEMU_CPU="$model" NB_QEMU_SYSROOT="$guest" \
            timeout -k 2s 120s dalvikvm -Xforce-nb-testing "-XX:NativeBridge=$bridge" \
            -cp "$out/test.jar" CLBBArmLseContract "$out/$fixture.so"
    done
done
