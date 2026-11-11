#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Create a persistent identity only in this installation's private state."""
import fcntl
import os
from pathlib import Path
import re
import secrets
import sys


def identity(state):
    directory = Path(state) / 'device'
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (directory / '.lock').open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = directory / 'android-id'
        if path.exists():
            value = path.read_text().strip()
            if not re.fullmatch('[0-9a-f]{16}', value):
                raise ValueError('Invalid local Android identity; refusing a shared fallback')
            return value
        value = secrets.token_hex(8)
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'w') as stream:
            stream.write(value + '\n')
            stream.flush()
            os.fsync(stream.fileno())
        return value


if __name__ == '__main__':
    print(identity(sys.argv[1]))
