#!/usr/bin/env bash
set -euo pipefail
CLBB_ADAPTER_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
cargo build --manifest-path "$CLBB_ADAPTER_ROOT/Cargo.toml" --offline --locked --release
CLBB_TEST_DIR="$(mktemp -d "$CLBB_ADAPTER_ROOT/target/test.XXXXXXXX")"
trap 'rm -rf -- "$CLBB_TEST_DIR"' EXIT
mkdir "$CLBB_TEST_DIR/helpers" "$CLBB_TEST_DIR/other"
cc -O2 -Wall -Wextra "$CLBB_ADAPTER_ROOT/tests/spawn.c" $(pkgconf --cflags --libs glib-2.0) -o "$CLBB_TEST_DIR/spawn"
for CLBB_HELPER in WebKitNetworkProcess WebKitWebProcess WebKitGPUProcess; do
    cp "$CLBB_TEST_DIR/spawn" "$CLBB_TEST_DIR/helpers/$CLBB_HELPER"
done
cp "$CLBB_TEST_DIR/spawn" "$CLBB_TEST_DIR/other/OtherProcess"
CLBB_ADAPTER_SO="$CLBB_ADAPTER_ROOT/target/release/libclbb_webkit_launch_adapter.so"
CLBB_WEBKIT_PROCESS_DIR="$CLBB_TEST_DIR/helpers" LD_PRELOAD="$CLBB_ADAPTER_SO" \
    timeout -k 2s 20s "$CLBB_TEST_DIR/spawn" "$CLBB_TEST_DIR/helpers" "$CLBB_TEST_DIR/other"
cc -O2 -Wall -Wextra "$CLBB_ADAPTER_ROOT/tests/bwrap.c" -o "$CLBB_TEST_DIR/bwrap"
cc -O2 -Wall -Wextra -shared -fPIC -DCAPTURE_BACKEND "$CLBB_ADAPTER_ROOT/tests/bwrap.c" -o "$CLBB_TEST_DIR/capture.so"
CLBB_WEBKIT_PROCESS_DIR="$CLBB_TEST_DIR/helpers" LD_PRELOAD="$CLBB_ADAPTER_SO:$CLBB_TEST_DIR/capture.so" \
    "$CLBB_TEST_DIR/bwrap"
