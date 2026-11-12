#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-or-later
set -euo pipefail
cd "$(dirname "$0")/.."
bin=$(mktemp /tmp/clbb-clone-signal-mask.XXXXXX)
trap 'rm -f "$bin"' EXIT
cc -std=c11 -O2 -Wall -Wextra -Werror -pthread tools/check-clone-signal-mask.c -o "$bin"
timeout 15 "$bin"
