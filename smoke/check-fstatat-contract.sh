#!/usr/bin/env bash
# Each invalid output buffer runs in its own expendable, game-free Dalvik VM.
set -euo pipefail
FSTATAT_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$FSTATAT_ROOT/env.sh"
FSTATAT_OUT="$FSTATAT_ROOT/smoke/fstatat-contract-artifacts"
FSTATAT_BIONIC="$FSTATAT_ROOT/src/nb-qemu/libnb-qemu-guest/bionic"
if [[ $# -gt 1 || ( $# -eq 1 && "$1" != --run && "$1" != --build-only ) ]]; then
    printf '%s\n' 'usage: check-fstatat-contract.sh [--build-only | --run]' >&2; exit 2
fi
mkdir -p "$FSTATAT_OUT/classes"
clang -target aarch64-pc-linux -march=armv8-a -mno-outline-atomics -fuse-ld=lld \
 -fPIC -shared -O2 -fno-strict-aliasing -fno-builtin -std=gnu11 -Wall -Wextra -Werror \
 -nostdinc -nostdlib -D__ANDROID__ \
 -I"$(clang -print-resource-dir)/include" -I"$JAVA_HOME/include" -I"$JAVA_HOME/include/linux" \
 -I"$FSTATAT_BIONIC/libc/include" -I"$FSTATAT_BIONIC/libc/kernel/android/uapi" \
 -I"$FSTATAT_BIONIC/libc/kernel/uapi" -I"$FSTATAT_BIONIC/libc/kernel/uapi/asm-arm64" \
 -L"$CLBB_ATL_PREFIX/share/libnb-qemu-guest" -Wl,-z,defs \
 -o "$FSTATAT_OUT/libclbb-arm-fstatat-contract.so" "$FSTATAT_ROOT/smoke/arm-fstatat-contract.c" -lc -ldl
javac --release 8 -Xlint:-options -d "$FSTATAT_OUT/classes" "$FSTATAT_ROOT/smoke/CLBBArmFstatatContract.java"
dx --dex --output="$FSTATAT_OUT/test.jar" "$FSTATAT_OUT/classes"
printf '%s\n' 'CLBB_ARM64_FSTATAT_CONTRACT_BUILT=1'
if [[ "${1:-}" != --run ]]; then exit 0; fi
ulimit -c 0
: > "$FSTATAT_OUT/status.tsv"
FSTATAT_NAMES=(valid_rw readonly unmapped cross_page write_only valid_cross_page tracked_smc)
for FSTATAT_CASE in 0 1 2 3 4 5 6; do
    FSTATAT_NAME="${FSTATAT_NAMES[$FSTATAT_CASE]}"
    # Preserve the failure status and complete crash log; continue with the
    # remaining cases. Core dumps are disabled to avoid large unrelated files.
    set +e
    NB_QEMU_SYSROOT="$CLBB_ATL_PREFIX/share/libnb-qemu-guest" \
      timeout -k 1s 8s dalvikvm -Xforce-nb-testing -XX:NativeBridge="$CLBB_ATL_PREFIX/lib/libnb-qemu.so" \
      -cp "$FSTATAT_OUT/test.jar" CLBBArmFstatatContract "$FSTATAT_OUT/libclbb-arm-fstatat-contract.so" "$FSTATAT_CASE" \
      > "$FSTATAT_OUT/$FSTATAT_NAME.log" 2>&1
    FSTATAT_STATUS=$?
    set -e
    printf '%s\t%s\n' "$FSTATAT_NAME" "$FSTATAT_STATUS" >> "$FSTATAT_OUT/status.tsv"
done
python - "$FSTATAT_OUT" <<'PY'
import json
from pathlib import Path
import re
import sys
out = Path(sys.argv[1])
rows = []
for line in (out / 'status.tsv').read_text().splitlines():
    name, exit_code = line.split('\t')
    text = (out / (name + '.log')).read_text(errors='replace').replace('\0', '')
    result = re.findall(r'^CLBB_FSTATAT_RESULT=(-?\d+)$', text, re.M)
    result = int(result[-1]) if result else None
    expected = -14 if name in ('readonly', 'unmapped', 'cross_page') else 0
    crash = 'Fatal signal' in text or 'Segmentation fault' in text
    row = dict(case=name, exit_code=int(exit_code), expected_raw_result=expected,
               actual_raw_result=result, crash_reported=crash,
               passed=int(exit_code) == 0 and result == expected,
               log=str((out / (name + '.log')).resolve()))
    rows.append(row)
report = dict(syscall_number=79, operation='newfstatat', fixture_stat_bytes=128,
              scenarios=rows, passed=all(row['passed'] for row in rows))
(out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
for row in rows:
    print('CLBB_FSTATAT_CASE=' + row['case'] + ' EXIT=' + str(row['exit_code']) +
          ' RESULT=' + str(row['actual_raw_result']) + ' EXPECTED=' + str(row['expected_raw_result']) +
          ' CRASH=' + str(int(row['crash_reported'])) + ' PASS=' + str(int(row['passed'])))
print('CLBB_FSTATAT_CONTRACT_OK=' + str(int(report['passed'])))
sys.exit(0 if report['passed'] else 1)
PY
