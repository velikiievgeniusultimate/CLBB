#!/usr/bin/env bash
set -euo pipefail
CLBB_HTTPS_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$CLBB_HTTPS_ROOT/env.sh"
CLBB_HTTPS_TEMP="$(mktemp -d /tmp/clbb-webkit-https.XXXXXXXX)"
trap 'rm -rf -- "$CLBB_HTTPS_TEMP"' EXIT
mkdir -m 700 "$CLBB_HTTPS_TEMP/runtime" "$CLBB_HTTPS_TEMP/cache" "$CLBB_HTTPS_TEMP/data" "$CLBB_HTTPS_TEMP/extension"
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIC -shared \
    "$CLBB_HTTPS_ROOT/smoke/webkit-empty-post-gate.c" -L"$CLBB_ATL_PREFIX/lib" \
    $(pkg-config --cflags --libs webkitgtk-web-process-extension-6.0) \
    -o "$CLBB_HTTPS_TEMP/extension/root-only-gate.so"
cc -std=c11 -O2 -Wall -Wextra -Werror "$CLBB_HTTPS_ROOT/smoke/webkit-empty-post.c" \
    -L"$CLBB_ATL_PREFIX/lib" $(pkg-config --cflags --libs webkitgtk-6.0) -o "$CLBB_HTTPS_ROOT/smoke/webkit-empty-post"
unset WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS
export CLBB_WEBKIT_PROCESS_DIR="$CLBB_ATL_PREFIX/lib/webkitgtk-6.0"
export WEBKIT_INJECTED_BUNDLE_PATH="$CLBB_WEBKIT_PROCESS_DIR/injected-bundle"
export XDG_RUNTIME_DIR="$CLBB_HTTPS_TEMP/runtime" XDG_CACHE_HOME="$CLBB_HTTPS_TEMP/cache" XDG_DATA_HOME="$CLBB_HTTPS_TEMP/data"
export GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none GTK_USE_PORTAL=0
# Direct connection is confined to this isolated child; no proxy values are printed.
unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY no_proxy NO_PROXY
set +e
timeout --kill-after=3s 30s xvfb-run -a -s '-screen 0 800x600x24' dbus-run-session -- \
    env LD_PRELOAD="$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so" \
    "$CLBB_HTTPS_ROOT/smoke/webkit-empty-post" "$CLBB_ATL_PREFIX" "$CLBB_HTTPS_TEMP/extension" \
    > "$CLBB_HTTPS_TEMP/result" 2> "$CLBB_HTTPS_TEMP/diagnostics"
CLBB_HTTPS_RESULT=$?
set -e
sed -n '/^hostname=/p' "$CLBB_HTTPS_TEMP/result"
exit "$CLBB_HTTPS_RESULT"
