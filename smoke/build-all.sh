#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/env.sh"
for CHECK in smoke args signal attach varargs smc signal-return signal-nested proc yield attach-exit dns properties; do
    "$ROOT/smoke/build-guest-test.sh" "$CHECK"
done
# Deliberately unresolved relocation tests native-loader error propagation.
# Keep it separate from build-guest-test.sh, whose -z defs rejects this fixture.
clang -target aarch64-pc-linux -fuse-ld=lld -fPIC -shared -O2 \
    -nostdinc -nostdlib -Wl,-z,relro,-z,now \
    -o "$ROOT/smoke/libclbb-arm-load-failure.so" "$ROOT/smoke/arm-load-failure.c"
mkdir -p "$ROOT/smoke/classes"
javac --release 8 -Xlint:-options -d "$ROOT/smoke/classes" "$ROOT"/smoke/*.java
dx --dex --output="$ROOT/smoke/smoke.jar" "$ROOT/smoke/classes"
