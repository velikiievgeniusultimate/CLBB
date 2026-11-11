#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Import unchanged, user-provided MLBB APKs. Never import account data."""
import fcntl
import json
import os
from pathlib import Path
import re
import shutil
import struct
import sys
import tempfile
import time
import zipfile

def u16(b, p): return struct.unpack_from('<H', b, p)[0]
def u32(b, p): return struct.unpack_from('<I', b, p)[0]
def manifest_attributes(data):
    if len(data) < 8 or u16(data, 0) != 3 or u32(data, 4) > len(data):
        raise ValueError('Unsupported Android manifest')
    strings = []
    pos = u16(data, 2)
    while pos + 8 <= len(data):
        kind, header, size = u16(data, pos), u16(data, pos+2), u32(data, pos+4)
        if size < header or header < 8 or pos + size > len(data):
            raise ValueError('Malformed Android manifest chunk')
        if kind == 1:
            count, flags, start = u32(data,pos+8), u32(data,pos+16), u32(data,pos+20)
            if count > 100000 or header + count*4 > size: raise ValueError('Invalid string pool')
            def length8(at):
                n=data[at]
                return (((n&127)<<8)|data[at+1], at+2) if n&128 else (n,at+1)
            def length16(at):
                n=u16(data,at)
                return (((n&32767)<<16)|u16(data,at+2),at+4) if n&32768 else (n,at+2)
            for i in range(count):
                at=pos+start+u32(data,pos+header+4*i)
                if flags & 256:
                    _,at=length8(at);n,at=length8(at);text=data[at:at+n].decode('utf-8')
                else:
                    n,at=length16(at);text=data[at:at+2*n].decode('utf-16le')
                strings.append(text)
        elif kind == 0x102 and strings:
            if strings[u32(data,pos+20)] == 'manifest':
                first, stride, count = u16(data,pos+24),u16(data,pos+26),u16(data,pos+28)
                if stride < 20 or 16+first+stride*count > size: raise ValueError('Invalid manifest attributes')
                result={}
                for i in range(count):
                    at=pos+16+first+stride*i;name=strings[u32(data,at+4)];raw=u32(data,at+8)
                    value=strings[raw] if raw!=0xffffffff else (strings[u32(data,at+16)] if data[at+15]==3 else u32(data,at+16))
                    result[name]=value
                return result
        pos += size
    raise ValueError('Android manifest root not found')

def import_game(state, files):
    state=Path(state);state.mkdir(parents=True,exist_ok=True)
    if not 1 <= len(files) <= 64: raise ValueError('Select a base APK and its split APKs')
    (state/'logs').mkdir(exist_ok=True)
    with (state/'logs/clbb-launch.lock').open('a+b') as lock:
        try: fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError: raise ValueError('Close CLBB before importing a game / Сначала закройте CLBB')
        entries=[];names=set();versions=set()
        for file in files:
            path=Path(file)
            if not path.is_file() or path.suffix.lower()!='.apk': raise ValueError('Select APK files only')
            with zipfile.ZipFile(path) as apk:
                info=apk.getinfo('AndroidManifest.xml')
                if info.file_size>4*1024**2: raise ValueError('Manifest too large')
                attrs=manifest_attributes(apk.read(info))
            if attrs.get('package')!='com.mobile.legends': raise ValueError('This runtime requires com.mobile.legends')
            split=attrs.get('split','');name='base.apk' if not split else 'split_'+re.sub(r'[^A-Za-z0-9_.-]','_',str(split))+'.apk'
            if name in names: raise ValueError('Duplicate base or split APK')
            names.add(name);versions.add(attrs.get('versionCode'));entries.append((path,name))
        if 'base.apk' not in names or len(versions)!=1: raise ValueError('Select one base APK and splits from the same game version')
        work=Path(tempfile.mkdtemp(prefix='.game-import-',dir=state));backup=None
        try:
            for source,name in entries: shutil.copyfile(source,work/name)
            game=state/'game'
            if game.exists():
                backups=state/'game-backups';backups.mkdir(exist_ok=True)
                backup=backups/str(time.time_ns());game.rename(backup)
            try: work.rename(game)
            except BaseException:
                if backup is not None: backup.rename(game)
                raise
        finally:
            if work.exists(): shutil.rmtree(work)
    return {'state':'imported','apk_count':len(entries),'version_code':next(iter(versions)), 'account_data_imported':False}
if __name__=='__main__':
    try: print(json.dumps(import_game(sys.argv[1],sys.argv[2:])))
    except Exception as error: raise SystemExit(str(error))
