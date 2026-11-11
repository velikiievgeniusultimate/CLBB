#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Prepare the ARM64 application's native library directory, independent of host ABI."""
import fcntl,json,os
from pathlib import Path,PurePosixPath
import shutil,sys,tempfile,zipfile
ABI='arm64-v8a'
def prepare(state, locked=False):
    state=Path(state);apks=[state/'game/base.apk',*sorted((state/'game').glob('split_*.apk'))]
    if not apks[0].is_file():return {'state':'no_game','libraries':0}
    identity=[{'name':p.name,'size':p.stat().st_size,'mtime_ns':p.stat().st_mtime_ns}for p in apks]
    cache=state/'cache/native-apk-libraries.json';directory=state/'data/base.apk_/lib'
    try:
        saved=json.loads(cache.read_text())
        if saved.get('apks')==identity and saved.get('abi')==ABI and saved.get('libraries') and all((directory/name).is_file()and(directory/name).stat().st_size==size for name,size in saved['libraries'].items()):
            return {'state':'current','libraries':len(saved['libraries']),'abi':ABI}
    except (OSError,ValueError,TypeError):pass
    locks=None
    if not locked:
        (state/'logs').mkdir(parents=True,exist_ok=True);locks=(state/'logs/clbb-launch.lock').open('a+b')
        try:fcntl.flock(locks,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:
            locks.close();raise ValueError('Close CLBB before preparing libraries / Сначала закройте CLBB')
    try:
        entries={};total=0
        for apk in apks:
            with zipfile.ZipFile(apk)as archive:
                for info in archive.infolist():
                    if not info.filename.startswith('lib/'+ABI+'/')or info.is_dir():continue
                    name=info.filename.removeprefix('lib/'+ABI+'/');path=PurePosixPath(name)
                    if '\\'in name or len(path.parts)!=1 or '..'in path.parts or not name.endswith('.so'):raise ValueError('Unsafe native library path')
                    if name in entries and entries[name][1].CRC!=info.CRC:raise ValueError('Conflicting native library in APK set: '+name)
                    entries[name]=(apk,info);total+=info.file_size
        if 'libmain.so'not in entries or total>2*1024**3:raise ValueError('A complete ARM64 Mobile Legends APK set is required / В APK отсутствует ARM64 libmain.so')
        apk,info=entries['libmain.so']
        with zipfile.ZipFile(apk)as archive:
            with archive.open(info)as source:header=source.read(20)
        if header[:6]!=b'\x7fELF\x02\x01'or header[18:20]!=b'\xb7\x00':raise ValueError('libmain.so is not an ARM64 ELF library')
        directory.mkdir(parents=True,exist_ok=True);sizes={}
        for position,(name,(apk,info))in enumerate(sorted(entries.items()),1):
            pending=None
            try:
                with zipfile.ZipFile(apk)as archive,archive.open(info)as source,tempfile.NamedTemporaryFile(dir=directory,prefix='.native-',delete=False)as target:
                    pending=Path(target.name);shutil.copyfileobj(source,target,1024*1024)
                pending.chmod(0o644);os.replace(pending,directory/name);pending=None;sizes[name]=info.file_size
                bar='#'*(20*position//len(entries));print(f'[CLBB native {bar:<20} {position}/{len(entries)}] {name}',flush=True)
            finally:
                if pending is not None:pending.unlink(missing_ok=True)
        cache.parent.mkdir(parents=True,exist_ok=True)
        temp=cache.with_suffix('.tmp');temp.write_text(json.dumps({'abi':ABI,'apks':identity,'libraries':sizes}));os.replace(temp,cache)
        return {'state':'prepared','libraries':len(entries),'abi':ABI,'bytes':sum(sizes.values())}
    finally:
        if locks is not None:locks.close()
if __name__=='__main__':
    try:print(json.dumps(prepare(sys.argv[1],locked=os.environ.get('CLBB_NATIVE_LOCK_HELD')=='1')))
    except Exception as error:raise SystemExit(str(error))
