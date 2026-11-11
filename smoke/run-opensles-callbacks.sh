#!/usr/bin/env bash
set -euo pipefail
CLBB_CALLBACK_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$CLBB_CALLBACK_ROOT/env.sh"
CLBB_CALLBACK_TEMP="$(mktemp -d /tmp/clbb-opensles-callbacks.XXXXXXXX)"
trap 'rm -rf -- "$CLBB_CALLBACK_TEMP"' EXIT
CLBB_CALLBACK_SOURCE="$CLBB_CALLBACK_ROOT/src/nb-qemu/libnb-qemu"
CLBB_CALLBACK_SANITIZER_FLAGS=()
if [[ "${CLBB_CALLBACK_SANITIZE:-0}" == 1 ]]; then
    CLBB_CALLBACK_SANITIZER_FLAGS=(-fsanitize=address,undefined -fno-omit-frame-pointer)
    # Registry closures deliberately live until process exit.
    export ASAN_OPTIONS=detect_leaks=0
fi
c++ "${CLBB_CALLBACK_SANITIZER_FLAGS[@]}" -std=c++17 -O1 -g -pthread -DLOG_NDEBUG=1 -DHAVE_SYS_UIO_H -ffunction-sections -fdata-sections \
    -I"$CLBB_CALLBACK_SOURCE" -I"$CLBB_CALLBACK_SOURCE/_includes" \
    $(pkg-config --cflags libffi) \
    "$CLBB_CALLBACK_ROOT/smoke/check-opensles-callbacks.cpp" \
    "$CLBB_CALLBACK_SOURCE/libOpenSLES.itf.cpp" "$CLBB_CALLBACK_SOURCE/Trampoline.cpp" \
    -Wl,--gc-sections $(pkg-config --libs libffi) -ldl -o "$CLBB_CALLBACK_TEMP/check-opensles-callbacks"
timeout --kill-after=2s 20s "$CLBB_CALLBACK_TEMP/check-opensles-callbacks"
