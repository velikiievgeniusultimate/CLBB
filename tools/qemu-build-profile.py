#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Install GCC counters from a local startup sample or locate them for a rebuild."""
import hashlib
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BRIDGE = ROOT / 'src/nb-qemu'
CORE_BUILD = BRIDGE / 'qemu/_build'
BRIDGE_BUILD = BRIDGE / 'builddir/libnb-qemu'
CACHE = Path(os.environ.get('XDG_CACHE_HOME') or Path.home() / '.cache') / 'clbb-build/qemu-pgo'


def signature():
    parts = [os.fsencode(ROOT)]
    compiler = shlex.split(os.environ.get('CC') or 'cc')
    for flag in ('-dumpfullversion', '-dumpmachine'):
        parts.append(subprocess.check_output([*compiler, flag]))
    for path in ('sources.json', 'patches/bridge-runtime.patch', 'patches/qemu-runtime.patch'):
        parts.append((ROOT / path).read_bytes())
    for tree, paths in ((BRIDGE, ['Makefile', 'libnb-qemu']), (BRIDGE / 'qemu', ['.'])):
        parts.append(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=tree))
        parts.append(subprocess.check_output(['git', 'diff', '--no-ext-diff', 'HEAD', '--', *paths], cwd=tree))
    return hashlib.sha256(b'\0'.join(parts)).hexdigest()


def counter_paths(source, core, bridge):
    # GCC mangles relative QEMU object paths with #. The bridge Makefile
    # passes absolute object paths, which GCC stores as a directory tree.
    prefix = str(core.resolve()).replace('/', '#') + '#'
    target = str(CORE_BUILD).replace('/', '#') + '#'
    for entry in source.glob('*.gcda'):
        if entry.name.startswith(prefix):
            yield entry, Path(target + entry.name[len(prefix):])
    tree = source / bridge.resolve().relative_to('/')
    for entry in tree.glob('*.gcda'):
        yield entry, BRIDGE_BUILD.relative_to('/') / entry.name


def counter_digest(tree):
    result = hashlib.sha256()
    for data in sorted(tree.rglob('*.gcda')):
        relative = data.relative_to(tree)
        result.update(os.fsencode(relative) + b'\0' + hashlib.sha256(data.read_bytes()).digest())
    return result.hexdigest()


def install(source, core, bridge):
    cache = CACHE / signature()
    cache.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=cache) as scratch:
        payload = Path(scratch) / 'profile'
        present = set()
        for data, relative in sorted(counter_paths(source, core, bridge)):
            contents = data.read_bytes()
            if contents[:4] != b'adcg':
                raise ValueError(f'Invalid GCC counter file: {data.name}')
            output = payload / relative
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(contents)
            present.add(relative.name.split('#')[-1])
        if not {'translate-a64.gcda', 'cpu-exec.gcda', 'libnb-qemu.cpp.gcda'}.issubset(present):
            raise ValueError('Training sample lacks AArch64 translation, dispatch or native bridge counters')
        name = counter_digest(payload)
        destination = cache / name
        if not destination.is_dir():
            try:
                payload.rename(destination)
            except OSError:
                if not destination.is_dir():
                    raise
        if payload.is_dir() and counter_digest(destination) != name:
            # Repair only this cache entry, replacing complete counters atomically.
            expected = {path.relative_to(payload) for path in payload.rglob('*.gcda')}
            for relative in expected:
                output = destination / relative
                output.parent.mkdir(parents=True, exist_ok=True)
                (payload / relative).replace(output)
            for extra in destination.rglob('*.gcda'):
                if extra.relative_to(destination) not in expected:
                    extra.unlink()
        selected = Path(scratch) / 'selected'
        selected.write_text(name + '\n')
        selected.replace(cache / 'selected')
    return destination


def locate():
    if not CACHE.is_dir():
        return
    cache = CACHE / signature()
    try:
        name = (cache / 'selected').read_text().strip()
    except FileNotFoundError:
        return
    if re.fullmatch('[0-9a-f]{64}', name) and (cache / name).is_dir():
        if counter_digest(cache / name) == name:
            print(cache / name)
        else:
            print('Incomplete local GCC profile, using ordinary QEMU optimization', file=sys.stderr)


if __name__ == '__main__':
    if sys.argv[1:] == ['locate']:
        # The optional profile is local to a compiler, worktree and source revision.
        try:
            locate()
        except (OSError, ValueError, subprocess.CalledProcessError) as error:
            print(f'Using ordinary QEMU optimization: {error}', file=sys.stderr)
    elif len(sys.argv) == 5 and sys.argv[1] == 'install':
        print(install(*(Path(value) for value in sys.argv[2:])))
    else:
        raise SystemExit('Usage: qemu-build-profile.py locate | install COUNTERS TRAINING_CORE TRAINING_BRIDGE')
