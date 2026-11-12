#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Check the actual fixed QEMU descriptor registry without starting a game."""
import os
from pathlib import Path
import shlex
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
flags = shlex.split(subprocess.check_output(
    ['pkg-config', '--cflags', '--libs', 'glib-2.0'], text=True))
with tempfile.TemporaryDirectory(prefix='clbb-fd-trans-') as directory:
    output = str(Path(directory) / 'check')
    subprocess.run([
        *shlex.split(os.environ.get('CC', 'cc')), '-std=gnu11', '-O1', '-g',
        '-Wall', '-Wextra', '-Werror', '-fsanitize=address,undefined',
        '-fno-omit-frame-pointer', str(root / 'tools/fd-trans-race-test.c'),
        *flags, '-pthread', '-o', output,
    ], check=True)
    subprocess.run([output], check=True, timeout=30)
