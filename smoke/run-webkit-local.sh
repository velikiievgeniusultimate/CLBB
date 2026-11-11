#!/usr/bin/env bash
set -euo pipefail
CLBB_WEBKIT_SMOKE_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$CLBB_WEBKIT_SMOKE_ROOT/env.sh"
CLBB_WEBKIT_SMOKE_OUTPUT="$CLBB_WEBKIT_SMOKE_ROOT/smoke/webkit-local-output"
mkdir -p "$CLBB_WEBKIT_SMOKE_OUTPUT"
CLBB_WEBKIT_SMOKE_TEMP="$(mktemp -d /tmp/clbb-webkit-smoke.XXXXXXXX)"
CLBB_WEBKIT_SMOKE_SERVER_PID=''
cleanup() {
    if [[ -n "$CLBB_WEBKIT_SMOKE_SERVER_PID" ]]; then
        kill "$CLBB_WEBKIT_SMOKE_SERVER_PID" 2>/dev/null || true
        wait "$CLBB_WEBKIT_SMOKE_SERVER_PID" 2>/dev/null || true
    fi
    rm -rf -- "$CLBB_WEBKIT_SMOKE_TEMP"
}
trap cleanup EXIT

[[ -f "$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so" ]]
[[ -x "$CLBB_ATL_PREFIX/lib/webkitgtk-6.0/WebKitWebProcess" ]]
cc -std=c11 -O2 -Wall -Wextra -Werror \
    "$CLBB_WEBKIT_SMOKE_ROOT/smoke/webkit-local.c" \
    -L"$CLBB_ATL_PREFIX/lib" $(pkg-config --cflags --libs webkitgtk-6.0) \
    -o "$CLBB_WEBKIT_SMOKE_ROOT/smoke/webkit-local"

python3 -u - "$CLBB_WEBKIT_SMOKE_TEMP/port" > "$CLBB_WEBKIT_SMOKE_OUTPUT/http.log" 2>&1 <<'PY' &
from http.server import BaseHTTPRequestHandler, HTTPServer
import pathlib, sys
page = '''<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'">
<style>body { background:#234b72; color:white; font:24px sans-serif; padding:30px }
input { display:block; width:600px; font:28px sans-serif; padding:12px; margin:24px 0 }
#result { color:#8affb0; padding:12px; border:3px solid #e8ab45 }</style></head>
<body><h1>Local WebKit smoke — Unicode</h1><label>Test input<input id="field"></label><div id="result">Waiting</div>
<script>window.addEventListener('load',()=>{
 const field=document.querySelector('#field');field.value='Привет, 世界 👋';
 field.dispatchEvent(new Event('input',{bubbles:true}));
 const reply=prompt('CLBB_NATIVE_PROMPT','запрос');
 document.querySelector('#result').textContent=reply;
 requestAnimationFrame(()=>requestAnimationFrame(()=>{
 window.webkit.messageHandlers.smoke.postMessage(field.value+'|'+reply+'|'+document.readyState);
 }));
});</script></body></html>'''.encode('utf-8')
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type','text/html; charset=utf-8')
        self.send_header('Content-Length',str(len(page)))
        self.end_headers()
        self.wfile.write(page)
server=HTTPServer(('127.0.0.1',0),Handler)
pathlib.Path(sys.argv[1]).write_text(str(server.server_port))
server.serve_forever()
PY
CLBB_WEBKIT_SMOKE_SERVER_PID=$!
for ((CLBB_WEBKIT_SMOKE_READY=0; CLBB_WEBKIT_SMOKE_READY<100; ++CLBB_WEBKIT_SMOKE_READY)); do
    [[ -s "$CLBB_WEBKIT_SMOKE_TEMP/port" ]] && break
    kill -0 "$CLBB_WEBKIT_SMOKE_SERVER_PID"
    sleep 0.02
done
[[ -s "$CLBB_WEBKIT_SMOKE_TEMP/port" ]]
CLBB_WEBKIT_SMOKE_PORT="$(cat "$CLBB_WEBKIT_SMOKE_TEMP/port")"
mkdir -m 700 "$CLBB_WEBKIT_SMOKE_TEMP/runtime" "$CLBB_WEBKIT_SMOKE_TEMP/cache" "$CLBB_WEBKIT_SMOKE_TEMP/data"
unset WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS
export CLBB_WEBKIT_PROCESS_DIR="$CLBB_ATL_PREFIX/lib/webkitgtk-6.0"
export WEBKIT_INJECTED_BUNDLE_PATH="$CLBB_WEBKIT_PROCESS_DIR/injected-bundle"
export CLBB_WEBKIT_SMOKE_PREFIX="$CLBB_ATL_PREFIX"
export XDG_RUNTIME_DIR="$CLBB_WEBKIT_SMOKE_TEMP/runtime"
export XDG_CACHE_HOME="$CLBB_WEBKIT_SMOKE_TEMP/cache"
export XDG_DATA_HOME="$CLBB_WEBKIT_SMOKE_TEMP/data"
export GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none GTK_USE_PORTAL=0
set +e
timeout --kill-after=3s 30s xvfb-run -a -s '-screen 0 1024x768x24' dbus-run-session -- \
    env LD_PRELOAD="$CLBB_ATL_PREFIX/lib/libclbb_webkit_launch_adapter.so" \
    "$CLBB_WEBKIT_SMOKE_ROOT/smoke/webkit-local" \
    "http://127.0.0.1:$CLBB_WEBKIT_SMOKE_PORT/" "$CLBB_WEBKIT_SMOKE_OUTPUT/render.png" \
    > "$CLBB_WEBKIT_SMOKE_OUTPUT/runtime.log" 2>&1
CLBB_WEBKIT_SMOKE_RESULT=$?
set -e
cat "$CLBB_WEBKIT_SMOKE_OUTPUT/runtime.log"
exit "$CLBB_WEBKIT_SMOKE_RESULT"
