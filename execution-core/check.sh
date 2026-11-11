#!/usr/bin/env bash
set -euo pipefail
CLBB_EXEC_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CLBB_EXEC_OUT="$CLBB_EXEC_ROOT/logs/execution-core-contract"
mkdir -p "$CLBB_EXEC_OUT"
rustc --edition=2021 --crate-type lib --emit=obj -C opt-level=3 -C panic=abort -C relocation-model=pic \
 "$CLBB_EXEC_ROOT/execution-core/src/lib.rs" -o "$CLBB_EXEC_OUT/rust.o"
test -z "$(nm -u "$CLBB_EXEC_OUT/rust.o")"
for CLBB_EXEC_MODE in normal sanitized; do
 CLBB_EXEC_FLAGS=(-O2)
 if [[ "$CLBB_EXEC_MODE" == sanitized ]]; then CLBB_EXEC_FLAGS+=(-fsanitize=address,undefined -fno-sanitize-recover=all); fi
 c++ -std=c++17 -pthread "${CLBB_EXEC_FLAGS[@]}" \
  -I"$CLBB_EXEC_ROOT/execution-core" -I"$CLBB_EXEC_ROOT/src/nb-qemu/libnb-qemu" \
  "$CLBB_EXEC_ROOT/execution-core/tests/contract.cpp" \
  "$CLBB_EXEC_ROOT/src/nb-qemu/libnb-qemu/GuestCpu.cpp" "$CLBB_EXEC_OUT/rust.o" \
  -o "$CLBB_EXEC_OUT/$CLBB_EXEC_MODE"
 "$CLBB_EXEC_OUT/$CLBB_EXEC_MODE"
done
