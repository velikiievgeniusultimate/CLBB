#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cargo build --offline --manifest-path "$ROOT/Cargo.toml"
cc -std=c11 -Wall -Wextra -Werror -I"$ROOT/include" "$ROOT/tests/abi.c" \
  -L"$ROOT/target/debug" -Wl,-rpath,"$ROOT/target/debug" -lclbb_dns_adapter \
  -o "$ROOT/target/abi-test"
LD_LIBRARY_PATH="$ROOT/target/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" "$ROOT/target/abi-test"
