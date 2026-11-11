#!/usr/bin/env python3
"""Extract signed Arch dependencies into this experiment's private prefix."""
import concurrent.futures
from pathlib import Path
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parent
PACKAGES = ROOT / 'packages'
PACKAGES.mkdir(exist_ok=True)

def fetch(line):
    name, url = line.split(' ', 1)
    dest = PACKAGES / url.rsplit('/', 1)[-1]
    candidates = [url, f'https://archive.archlinux.org/packages/{name[0]}/{name}/{dest.name}']
    for candidate in candidates:
        try:
            urllib.request.urlretrieve(candidate, dest)
            urllib.request.urlretrieve(candidate + '.sig', str(dest) + '.sig')
            break
        except Exception:
            if candidate == candidates[-1]:
                raise
    subprocess.run(['gpg', '--homedir', '/etc/pacman.d/gnupg', '--no-auto-check-trustdb',
                    '--verify', str(dest) + '.sig', str(dest)], check=True, capture_output=True)
    subprocess.run(['bsdtar', '-xf', str(dest), '-C', str(ROOT / 'prefix'),
                    '--strip-components', '1', 'usr'], check=True)
    return f'{name}: signature verified, extracted'

lines = subprocess.check_output(['pacman', '-Sp', '--print-format', '%n %l', *sys.argv[1:]], text=True).splitlines()
with concurrent.futures.ThreadPoolExecutor(3) as executor:
    for result in executor.map(fetch, lines):
        print(result, flush=True)
for pc in (ROOT / 'prefix/lib/pkgconfig').glob('*.pc'):
    pc.write_text(pc.read_text().replace('prefix=/usr\n', f'prefix={ROOT / "prefix"}\n'))
