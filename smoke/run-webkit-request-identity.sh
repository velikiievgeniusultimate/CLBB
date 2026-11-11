#!/usr/bin/env bash
set -euo pipefail
CLBB_IDENTITY_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$CLBB_IDENTITY_ROOT/env.sh"
CLBB_IDENTITY_TEMP="$(mktemp -d /tmp/clbb-webkit-identity.XXXXXXXX)"
CLBB_IDENTITY_SERVER=''
cleanup() {
    if [[ -n "$CLBB_IDENTITY_SERVER" ]]; then kill "$CLBB_IDENTITY_SERVER" 2>/dev/null || true; wait "$CLBB_IDENTITY_SERVER" 2>/dev/null || true; fi
    rm -rf -- "$CLBB_IDENTITY_TEMP"
}
trap cleanup EXIT
mkdir -m 700 "$CLBB_IDENTITY_TEMP/runtime" "$CLBB_IDENTITY_TEMP/cache" "$CLBB_IDENTITY_TEMP/data" "$CLBB_IDENTITY_TEMP/extension"
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIC -shared \
    "$CLBB_IDENTITY_ROOT/src/atl/src/web-process-extension/atl-android-request.c" -L"$CLBB_ATL_PREFIX/lib" \
    $(pkg-config --cflags --libs webkitgtk-web-process-extension-6.0) \
    -o "$CLBB_IDENTITY_TEMP/extension/libatl-android-request.so"
cc -std=c11 -O2 -Wall -Wextra -Werror "$CLBB_IDENTITY_ROOT/smoke/webkit-request-identity.c" \
    -L"$CLBB_ATL_PREFIX/lib" $(pkg-config --cflags --libs webkitgtk-6.0) -o "$CLBB_IDENTITY_TEMP/webkit-request-identity"
python3 "$CLBB_IDENTITY_ROOT/smoke/webkit-request-identity-server.py" "$CLBB_IDENTITY_TEMP/port" &
CLBB_IDENTITY_SERVER=$!
for ((CLBB_IDENTITY_TRY=0; CLBB_IDENTITY_TRY<100; CLBB_IDENTITY_TRY++)); do
    [[ -s "$CLBB_IDENTITY_TEMP/port" ]] && break
    sleep .02
done
read -r CLBB_IDENTITY_PORT < "$CLBB_IDENTITY_TEMP/port" || [[ -n "${CLBB_IDENTITY_PORT:-}" ]]
unset WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS
export CLBB_WEBKIT_PROCESS_DIR="$CLBB_ATL_PREFIX/lib/webkitgtk-6.0"
export WEBKIT_INJECTED_BUNDLE_PATH="$CLBB_WEBKIT_PROCESS_DIR/injected-bundle"
export XDG_RUNTIME_DIR="$CLBB_IDENTITY_TEMP/runtime" XDG_CACHE_HOME="$CLBB_IDENTITY_TEMP/cache" XDG_DATA_HOME="$CLBB_IDENTITY_TEMP/data"
export GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none GTK_USE_PORTAL=0
for CLBB_IDENTITY_MODE in valid invalid invalid-type; do
    CLBB_IDENTITY_PACKAGE='com.example.atl_identity'
    [[ "$CLBB_IDENTITY_MODE" == invalid ]] && CLBB_IDENTITY_PACKAGE='bad/package'
    [[ "$CLBB_IDENTITY_MODE" == invalid-type ]] && CLBB_IDENTITY_PACKAGE='invalid-type'
    timeout --kill-after=3s 20s xvfb-run -a -s '-screen 0 800x600x24' dbus-run-session -- \
        env LD_PRELOAD="$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so" \
        "$CLBB_IDENTITY_TEMP/webkit-request-identity" "$CLBB_ATL_PREFIX" "$CLBB_IDENTITY_TEMP/extension" \
        "http://127.0.0.1:$CLBB_IDENTITY_PORT/$CLBB_IDENTITY_MODE" "$CLBB_IDENTITY_PACKAGE" \
        > "$CLBB_IDENTITY_TEMP/result" 2> "$CLBB_IDENTITY_TEMP/diagnostics"
    printf '%s ' "$CLBB_IDENTITY_MODE"
    sed -n '/^REQUEST_IDENTITY /p' "$CLBB_IDENTITY_TEMP/result"
done
