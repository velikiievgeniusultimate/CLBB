#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
cc -std=c11 -O2 -Wall -Wextra -Werror clbb-viewport.c check-viewport.c \
  $(pkg-config --cflags --libs gtk4) -lm -o "$work_dir/check-viewport"
# Isolated display: never resize the user's window or move the desktop pointer.
xvfb-run -a -s '-screen 0 1920x1080x24 -nolisten tcp' \
  env GDK_BACKEND=x11 GSK_RENDERER=cairo GTK_A11Y=none "$work_dir/check-viewport"
