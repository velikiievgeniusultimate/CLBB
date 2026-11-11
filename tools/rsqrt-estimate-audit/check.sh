#!/usr/bin/env bash
# Standalone correctness/build check. Does not build, relink or install QEMU.
set -euo pipefail
audit_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cargo test --offline --manifest-path "$audit_dir/Cargo.toml"
cargo build --release --offline --manifest-path "$audit_dir/Cargo.toml"
"$audit_dir/target/release/clbb-rsqrt-estimate-audit" --check --output "$audit_dir/target/check-artifacts"
