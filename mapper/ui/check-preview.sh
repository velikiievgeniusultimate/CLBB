#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cc -std=c11 -O2 -Wall -Wextra -Werror -Wno-misleading-indentation clbb-settings.c preview.c -o /tmp/clbb-settings-preview $(pkg-config --cflags --libs gtk4)
if [[ ${1:-} == --run ]]; then
  shift
  exec /tmp/clbb-settings-preview "$@"
fi
# Isolated X server: no focus changes or synthetic input on the user's display.
work_dir=$(mktemp -d)
Xvfb -displayfd 3 -screen 0 "${CLBB_PREVIEW_SCREEN:-1060x760}x24" -nolisten tcp 3>"$work_dir/display" >"$work_dir/xvfb.log" 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" 2>/dev/null || true; rm -rf "$work_dir"' EXIT
for ((i=0;i<100;i++)); do
  [[ -s "$work_dir/display" ]] && break
  sleep .03
done
export DISPLAY=":$(cat "$work_dir/display")"
export GDK_BACKEND=x11
export GSK_RENDERER=cairo
export GTK_A11Y=none
/tmp/clbb-settings-preview --self-test
if [[ ${1:-} == --screenshots ]]; then
  mkdir -p preview
  for page in main controls hero sound hotkeys updates; do
    /tmp/clbb-settings-preview --tab "$page" --timeout 3 >"$work_dir/$page.log" 2>&1 &
    preview_pid=$!
    sleep 1
    import -window root "preview/$page.png"
    wait "$preview_pid"
  done
fi
