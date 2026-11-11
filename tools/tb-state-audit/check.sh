#!/usr/bin/env bash
set -euo pipefail
CLBB_AUDIT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
source "$CLBB_AUDIT_ROOT/env.sh"
CLBB_AUDIT_QEMU="$CLBB_AUDIT_ROOT/src/nb-qemu/qemu"
CLBB_AUDIT_TMP=$(mktemp -d)
trap 'rm -rf "$CLBB_AUDIT_TMP"' EXIT
cc -O2 -g -fno-strict-aliasing -fsanitize=undefined -fno-sanitize-recover=all -D_GNU_SOURCE -DNEED_CPU_H \
 -I "$CLBB_AUDIT_QEMU/_build/aarch64-linux-user" -I "$CLBB_AUDIT_QEMU/_build" \
 -I "$CLBB_AUDIT_QEMU/target/arm" -I "$CLBB_AUDIT_QEMU/include" -I "$CLBB_AUDIT_QEMU" -I "$CLBB_AUDIT_QEMU/tcg/i386" \
 $(pkg-config --cflags glib-2.0) "$CLBB_AUDIT_ROOT/tools/tb-state-audit/check.c" -ldl -o "$CLBB_AUDIT_TMP/check"
"$CLBB_AUDIT_TMP/check" "${1:-$CLBB_AUDIT_ROOT/prefix/lib/libnb-qemu.so}"
