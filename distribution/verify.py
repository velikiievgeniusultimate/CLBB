# SPDX-License-Identifier: GPL-3.0-or-later
"""Verify the exact immutable payload described by a verified release manifest."""
import hashlib
import json
import os
from pathlib import Path,PurePosixPath
import sys

def verify(root, manifest):
    root=Path(root);info=json.loads(Path(manifest).read_text())
    entries=info['files']
    if not entries:raise ValueError('Empty payload manifest')
    for name,expected in entries.items():
        path=PurePosixPath(name)
        if path.is_absolute() or '..' in path.parts:raise ValueError('Unsafe payload path')
        target=root/name
        if expected['type']=='symlink':
            if not target.is_symlink() or os.readlink(target)!=expected['target']:raise ValueError('Symlink mismatch: '+name)
        elif expected['type']=='file':
            if target.is_symlink() or not target.is_file():raise ValueError('Missing payload file: '+name)
            with target.open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
            if digest!=expected['sha256']:raise ValueError('Payload hash mismatch: '+name)
        else:raise ValueError('Unknown payload type')
    if (root/'VERSION').read_text().strip()!=info['version']:raise ValueError('Payload version mismatch')
if __name__=='__main__':verify(sys.argv[1],sys.argv[2])
