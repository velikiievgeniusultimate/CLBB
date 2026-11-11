#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Assemble a per-installation guest namespace, including generated font paths."""
from pathlib import Path


def prepare(state, prefix):
    state = Path(state).resolve()
    source = Path(prefix).resolve() / 'share/libnb-qemu-guest'
    target = state / 'guest-root'
    target.mkdir(parents=True, exist_ok=True)
    entries = {p.name: p.resolve() for p in source.iterdir() if p.name not in ('system', 'etc', 'vendor')}
    entries.update({name: state / name for name in ('system', 'etc', 'vendor')})
    for name, destination in entries.items():
        link = target / name
        if link.is_symlink():
            if link.resolve() == destination:
                continue
            link.unlink()
        elif link.exists():
            raise ValueError('Unexpected guest namespace file: ' + str(link))
        link.symlink_to(destination)
    for name in ('fonts.xml', 'system_fonts.xml', 'fallback_fonts.xml'):
        if not (target / 'system/etc' / name).is_file():
            raise ValueError('Missing guest font configuration: ' + name)
    return target
