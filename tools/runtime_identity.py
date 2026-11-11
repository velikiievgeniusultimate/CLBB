#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Recognize only the checkout runtime or its verified selected workbench binary."""
import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def runtime_executable_is_owned(pid,root=ROOT):
    root=Path(root)
    try:actual=(Path('/proc')/str(int(pid))/'exe').resolve(strict=True)
    except (OSError,ValueError):return False
    if actual==root/'prefix/bin/android-translation-layer':return True
    try:
        selected=json.loads((root/'.clbb-local-runtime.json').read_text())
        directory=Path(selected['directory']).resolve();target=directory/'runtime'
        if actual!=target/'prefix/bin/android-translation-layer':return False
        candidate=json.loads((directory/'candidate.json').read_text())
        if candidate['assets'][candidate['runtime']]!=selected['runtime_sha256']:return False
        manifest=json.loads((target/'release.json').read_text())
        expected=manifest['files']['prefix/bin/android-translation-layer']['sha256']
        with actual.open('rb')as stream:return hashlib.file_digest(stream,'sha256').hexdigest()==expected
    except (OSError,ValueError,KeyError,TypeError):return False
