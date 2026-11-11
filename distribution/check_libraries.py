# SPDX-License-Identifier: GPL-3.0-or-later
"""Check runtime ELF dependencies before activating a downloaded release."""
import os
from pathlib import Path
import subprocess
import sys

def check(root):
    root=Path(root).resolve();env=os.environ.copy();env.pop('LD_PRELOAD',None)
    env['LC_ALL']='C'
    env['LD_LIBRARY_PATH']=':'.join(str(root/p)for p in ['prefix/lib','prefix/lib/art','prefix/lib/java/dex/art/natives'])
    missing=[]
    for path in sorted(root.rglob('*')):
        if path.is_symlink() or not path.is_file():continue
        with path.open('rb')as stream:header=stream.read(20)
        if header[:6]!=b'\x7fELF\x02\x01' or header[18:20]!=b'\x3e\x00':continue
        result=subprocess.run(['ldd',str(path)],env=env,capture_output=True,text=True,timeout=30)
        output=result.stdout+'\n'+result.stderr
        if 'not found' in output:
            missing.extend(str(path.relative_to(root))+': '+line.strip()for line in output.splitlines()if 'not found'in line)
        elif result.returncode and 'not a dynamic executable' not in output and 'statically linked' not in output:
            raise ValueError('ELF dependency check failed: '+str(path.relative_to(root)))
    if missing:raise ValueError('Missing runtime libraries / Отсутствуют библиотеки: '+'; '.join(missing))
if __name__=='__main__':check(sys.argv[1])
