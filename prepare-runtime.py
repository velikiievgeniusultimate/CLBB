#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Prepare one profile for launch, using one interpreter for the startup helpers."""
import os
from pathlib import Path
import runpy
import sys


def prepare_runtime(root, state):
    original_argv = sys.argv
    original_path = sys.path[:]
    try:
        for name in ('prepare-native-plugins.py', 'prepare-system-fonts.py'):
            sys.argv = [str(root / name)]
            sys.path[:] = [str(root), *original_path]
            runpy.run_path(str(root / name), run_name='__main__')
    finally:
        sys.argv = original_argv
        sys.path[:] = original_path
    from guest_root import prepare
    prepare(state, root / 'prefix')


def main():
    if len(sys.argv) < 2 or sys.argv[2:] not in ([], ['--print-android-id']):
        raise SystemExit('Usage: prepare-runtime.py ROOT [--print-android-id]')
    root = Path(sys.argv[1]).resolve()
    state = Path(os.environ.get('CLBB_STATE_ROOT', root))
    # Packaged files are immutable, including with a different host Python version.
    sys.dont_write_bytecode = True
    sys.path.insert(0, str(root / 'distribution'))
    if len(sys.argv) == 2:
        prepare_runtime(root, state)
        return
    from device_identity import identity
    # stdout carries only the ID to the shell. Preparation and child processes
    # continue writing progress to the launch log, including on failure.
    with os.fdopen(os.dup(sys.stdout.fileno()), 'w') as result:
        sys.stdout.flush()
        os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
        value = identity(state)
        prepare_runtime(root, state)
        print(value, file=result)


if __name__ == '__main__':
    main()
