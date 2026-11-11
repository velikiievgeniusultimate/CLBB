#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --manifest-path mapper/runtime/Cargo.toml --locked --offline --bin clbb-update-ui
fixture=$(mktemp -d /tmp/clbb-update-ui-test-XXXXXX)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/old/bin" "$fixture/new/bin" "$fixture/install"
ln -s "$fixture/new" "$fixture/install/current"
printf '#!/bin/sh\nprintf wrong > "%s"\n' "$fixture/wrong" > "$fixture/old/bin/clbb-launcher"
printf '#!/bin/sh\nprintf restarted > "%s"\n' "$fixture/restarted" > "$fixture/new/bin/clbb-launcher"
chmod +x "$fixture/"{old,new}/bin/clbb-launcher
cat > "$fixture/manager" <<'HELPER'
#!/bin/sh
printf '%s\r' '######## 42.0%' >&2
sleep .4
printf '%s\n' '{"state":"installed","version":"v0.1.0-fixture"}'
HELPER
chmod +x "$fixture/manager"
run() {
 xvfb-run -a env GTK_A11Y=none GDK_BACKEND=x11 GSK_RENDERER=cairo \
  CLBB_MANAGED_INSTALL=1 CLBB_UPDATE_MANAGER="$fixture/manager" \
  CLBB_ROOT="$fixture/old" CLBB_INSTALL_ROOT="$fixture/install" CLBB_RELEASE_VERSION=v0.1.0-fixture \
  CLBB_UPDATE_UI_TEST_RESTART="${restart:-0}" timeout 12 mapper/runtime/target/debug/clbb-update-ui "$@"
}
run --self-test-cancel
restart=1 run --self-test
for i in {1..100}; do [[ -f "$fixture/restarted" ]] && break; sleep .01; done
[[ -f "$fixture/restarted" && ! -f "$fixture/wrong" ]]
printf '%s\n' 'CLBB_UPDATE_RESTART_OK selected current release'
cat > "$fixture/manager" <<'HELPER'
#!/bin/sh
printf '%s\r' '######## 42.0%' >&2
sleep .4
printf '%s\n' '{"error":"fixture download failure"}'
exit 1
HELPER
run --self-test-error
