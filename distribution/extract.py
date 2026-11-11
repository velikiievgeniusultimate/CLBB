# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate a runtime archive before extraction; persistent state is excluded."""
import pathlib
import sys
import tarfile

ALLOWED = {'prefix', 'bin', 'distribution', 'legal', 'LICENSES', 'VERSION', 'LICENSE',
           'REVISION', 'LICENSE_POLICY.md', 'THIRD_PARTY_NOTICES.txt', 'release.json', 'env.sh',
           'run-mlbb.sh', 'prepare-runtime.py', 'prepare-native-plugins.py',
           'prepare-system-fonts.py', 'README.md', 'README.en.md'}
def extract(archive, destination):
    with tarfile.open(archive, 'r:gz') as tar:
        total = 0
        for item in tar.getmembers():
            path = pathlib.PurePosixPath(item.name)
            if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] not in ALLOWED:
                raise ValueError('Unexpected archive path: ' + item.name)
            if not (item.isfile() or item.isdir() or item.issym() or item.islnk()):
                raise ValueError('Special archive entry rejected')
            total += item.size
            if total > 4 * 1024**3:
                raise ValueError('Runtime archive exceeds size limit')
        tar.extractall(destination, filter='data')
if __name__ == '__main__':
    extract(sys.argv[1], sys.argv[2])
