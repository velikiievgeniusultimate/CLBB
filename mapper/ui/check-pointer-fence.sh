#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
work_dir=$(mktemp -d)
trap 'rm -rf "$work_dir"' EXIT
cc -std=c11 -O2 -Wall -Wextra -Werror check-pointer-fence.c \
  $(pkg-config --cflags --libs x11 xfixes xtst) -o "$work_dir/check-pointer-fence"
xvfb-run -a -s '-screen 0 1920x1080x24 -nolisten tcp' "$work_dir/check-pointer-fence"
