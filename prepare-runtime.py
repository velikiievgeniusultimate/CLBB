#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Run both existing preparations, sharing interpreter startup and imports only."""
import runpy
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
original_argv = sys.argv
original_path = sys.path[:]
try:
    for name in ('prepare-native-plugins.py', 'prepare-system-fonts.py'):
        sys.argv = [str(root / name)]
        sys.path[:] = [str(root), *original_path[1:]]
        runpy.run_path(str(root / name), run_name='__main__')
finally:
    sys.argv = original_argv
    sys.path[:] = original_path

import os
sys.path.insert(0, str(root / "distribution"))
from guest_root import prepare
prepare(os.environ.get("CLBB_STATE_ROOT", str(root)), root / "prefix")
