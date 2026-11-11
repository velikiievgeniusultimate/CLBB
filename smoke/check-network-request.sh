#!/usr/bin/env bash
# Isolated framework value-object regression; no VM/game, installation or network.
set -euo pipefail
CLBB_REQUEST_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
CLBB_REQUEST_CLASSES="$(mktemp -d "${TMPDIR:-/tmp}/clbb-network-request.XXXXXXXX")"
trap 'rm -rf -- "$CLBB_REQUEST_CLASSES"' EXIT
javac --release 8 -Xlint:all,-options -Werror -d "$CLBB_REQUEST_CLASSES" \
    "$CLBB_REQUEST_ROOT/src/atl/src/api-impl/android/net/NetworkCapabilities.java" \
    "$CLBB_REQUEST_ROOT/src/atl/src/api-impl/android/net/NetworkRequest.java" \
    "$CLBB_REQUEST_ROOT/smoke/network-request/NetworkRequestSmoke.java"
java -cp "$CLBB_REQUEST_CLASSES" NetworkRequestSmoke
