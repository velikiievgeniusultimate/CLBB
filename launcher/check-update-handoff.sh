#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --manifest-path launcher/Cargo.toml --locked --offline
fixture=$(mktemp -d /tmp/clbb-handoff-test-XXXXXX)
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/runtime/bin" "$fixture/state/game"
touch "$fixture/state/game/base.apk"
printf 'v0.1.0-fixture\n' > "$fixture/runtime/VERSION"
cat > "$fixture/runtime/run-mlbb.sh" <<'GAME'
#!/bin/sh
printf '[CLBB_EXIT] update_requested\n'
GAME
cat > "$fixture/runtime/bin/clbb-update-ui" <<'UI'
#!/bin/sh
flock -n "$CLBB_STATE_ROOT/logs/clbb-launch.lock" -c "printf ready > '$CLBB_STATE_ROOT/window-opened'"
UI
chmod +x "$fixture/runtime/run-mlbb.sh" "$fixture/runtime/bin/clbb-update-ui"
env DISPLAY=:fixture CLBB_ROOT="$fixture/runtime" CLBB_STATE_ROOT="$fixture/state" CLBB_MANAGED_INSTALL=1 \
 launcher/target/debug/clbb-launcher
[[ $(cat "$fixture/state/window-opened") == ready && -f "$fixture/state/game/base.apk" ]]
printf '%s\n' 'CLBB_HANDOFF_OK explicit-close,lock-released,updater-started,state-preserved'
