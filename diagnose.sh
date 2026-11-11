#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
set -eu
CLBB_DIAG_TMP=$(mktemp -d)
trap 'rm -rf -- "$CLBB_DIAG_TMP"' EXIT HUP INT TERM
CLBB_BASE="${CLBB_INSTALL_ROOT:-${XDG_DATA_HOME:-$HOME/.local/share}/clbb}"
CLBB_RUNTIME=$(readlink -f -- "$CLBB_BASE/current")
printf '%s\n' 'CLBB: загрузка диагностического инструмента / Downloading diagnostics'
curl --fail --location --progress-bar --show-error --proto '=https' --proto-redir '=https' --connect-timeout 15 --max-time 60 \
    https://raw.githubusercontent.com/velikiievgeniusultimate/CLBB/main/distribution/launch-fallback.py -o "$CLBB_DIAG_TMP/diagnose.py"
python3 "$CLBB_DIAG_TMP/diagnose.py" "$CLBB_RUNTIME" "${CLBB_STATE_ROOT:-$CLBB_BASE/state}" 'Проверка после неудачного запуска CLBB'
