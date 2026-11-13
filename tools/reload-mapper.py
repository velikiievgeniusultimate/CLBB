#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Build/deploy only Rust mapper code into the current owned game, without restart.

The GTK host must support live ABI v1 (alpha76+). No game files, account data,
selected immutable runtime or persisted bindings are replaced. Busy/recording
sessions are rejected. Use --rollback to stage the preceding mapper again.
"""
from __future__ import annotations
import argparse,hashlib,json,os,socket,subprocess,tempfile,time
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
from runtime_identity import runtime_executable_is_owned

def birth(pid):
    fields=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
    if fields[0] in ('Z','X','x'):raise RuntimeError('Owned game has exited')
    return fields[19]

def immutable_copy(source:Path,directory:Path):
    data=source.read_bytes()
    if not 64<=len(data)<=67108864 or data[:4]!=b'\x7fELF':raise ValueError('Expected bounded ELF mapper library')
    digest=hashlib.sha256(data).hexdigest();directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    target=directory/(digest+'.so')
    if target.exists():
        if hashlib.sha256(target.read_bytes()).hexdigest()!=digest:raise ValueError('Existing live artifact changed; refusing overwrite')
    else:
        fd,name=tempfile.mkstemp(prefix='.mapper-',dir=directory)
        try:
            with os.fdopen(fd,'wb')as stream:stream.write(data);stream.flush();os.fsync(stream.fileno())
            os.chmod(name,0o444);os.replace(name,target)
        finally:
            if os.path.exists(name):os.unlink(name)
    return target,digest

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library',type=Path,help='Existing module instead of building the current source')
    parser.add_argument('--rollback',action='store_true')
    parser.add_argument('--status',action='store_true')
    parser.add_argument('--marker',help='Compile-time diagnostic marker, never a gameplay setting')
    args=parser.parse_args()
    if args.rollback and(args.library or args.marker):parser.error('Rollback does not build a module')
    metadata=json.loads((ROOT/'logs/current-atl-pid.json').read_text());pid=metadata['pid'];expected=birth(pid)
    uid=Path(f'/proc/{pid}').stat().st_uid
    if uid!=os.getuid():raise RuntimeError('Run this tool as the game owner, without sudo')
    endpoint=Path(os.environ.get('XDG_RUNTIME_DIR',f'/run/user/{uid}'))/'clbb'/f'mapper-{pid}.sock'
    def identity():
        if birth(pid)!=expected or not runtime_executable_is_owned(pid,ROOT):raise RuntimeError('Owned game identity changed')
        if json.loads((ROOT/'logs/current-atl-pid.json').read_text())['pid']!=pid:raise RuntimeError('Selected game PID changed')
    def command(value):
        identity()
        with socket.socket(socket.AF_UNIX)as connection:
            connection.settimeout(10);connection.connect(str(endpoint));connection.sendall(value.encode());connection.shutdown(socket.SHUT_WR)
            data=bytearray()
            while True:
                part=connection.recv(8192)
                if not part:break
                data.extend(part)
                if len(data)>1048576:raise RuntimeError('Live control response exceeds bound')
        identity();return data.decode()
    try:before=json.loads(command('read diagnostics.reload'))
    except json.JSONDecodeError:raise RuntimeError('Current host lacks live reloading; install the new host once')from None
    if not isinstance(before,dict) or before.get('supported')is not True:raise RuntimeError('Install the live-reload host once; current game has no compatible loader')
    if args.status:print(json.dumps(before,ensure_ascii=False));return
    record={'pid':pid,'birth':expected,'before':before,'started_ns':time.monotonic_ns()}
    if args.rollback:request='rollback-mapper';expected_sha=before.get('previous_sha256')
    else:
        source=args.library
        if source is None:
            env=os.environ.copy()
            if args.marker:env['CLBB_MAPPER_BUILD_MARKER']=args.marker
            subprocess.run(['cargo','build','--manifest-path','mapper/runtime/Cargo.toml','--release','--lib','--locked','--offline'],cwd=ROOT,env=env,check=True)
            source=ROOT/'mapper/runtime/target/release/libclbb_mapper.so'
        artifact,expected_sha=immutable_copy(source,Path(os.environ.get('XDG_STATE_HOME',str(Path.home()/'.local/state')))/'clbb/live-mapper/artifacts')
        if any(char.isspace()for char in str(artifact)):raise RuntimeError('Native private command currently requires a whitespace-free artifact path')
        record.update(artifact=str(artifact),sha256=expected_sha)
        request=f'reload-mapper {artifact} {expected_sha}'
    reply=command(request);record['reply']=reply
    if not reply.startswith('OK reload'):raise RuntimeError(reply)
    deadline=time.monotonic()+8
    while True:
        status=json.loads(command('read diagnostics.reload'));record['after']=status
        if not status.get('pending'):
            if status.get('sha256')!=expected_sha:raise RuntimeError('Candidate rejected; previous mapper retained: '+str(status))
            break
        if time.monotonic()>deadline:raise RuntimeError('Candidate observation timed out; inspect diagnostics.reload before another request')
        time.sleep(.05)
    record['finished_ns']=time.monotonic_ns();record['build']=command('read diagnostics.mapper_build')
    history=ROOT/'logs/live-mapper';history.mkdir(parents=True,exist_ok=True)
    path=history/f'reload-{record["finished_ns"]}.json';path.write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'pid_unchanged':pid,'generation':record['after']['generation'],'sha256':expected_sha,'report':str(path)},ensure_ascii=False))
if __name__=='__main__':
    try:main()
    except(Exception,KeyboardInterrupt)as error:
        print('CLBB live mapper: '+str(error),file=sys.stderr);raise SystemExit(1)
