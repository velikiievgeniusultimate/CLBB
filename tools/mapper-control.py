#!/usr/bin/env python3
"""Drive this checkout's independent mapper pointer without desktop focus/input."""
import json,os,pathlib,socket,sys,time
ROOT=pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
from runtime_identity import runtime_executable_is_owned

MAX_REPLY_BYTES=4*1024*1024
SOCKET_SECONDS=3.


def read_reply(connection,deadline,max_bytes=MAX_REPLY_BYTES):
    """The native server writes one complete reply and closes its connection."""
    chunks=[];size=0
    while True:
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('Mapper reply deadline expired')
        connection.settimeout(remaining)
        chunk=connection.recv(min(65536,max_bytes-size+1))
        if not chunk:break
        size+=len(chunk)
        if size>max_bytes:raise ValueError('Mapper reply exceeds 4 MiB')
        chunks.append(chunk)
    return b''.join(chunks).decode('utf-8').rstrip()


def main(argv=None):
    command=' '.join(sys.argv[1:] if argv is None else argv)
    if not command or len(command.encode())>900 or '\n' in command:
        raise SystemExit('Expected one bounded mapper command')
    pid=json.loads((ROOT/'logs/current-atl-pid.json').read_text())['pid']
    if not runtime_executable_is_owned(pid,ROOT):
        raise SystemExit('Owned ATL identity changed')
    path=pathlib.Path(os.environ.get('XDG_RUNTIME_DIR',f'/run/user/{os.getuid()}'))/'clbb'/f'mapper-{pid}.sock'
    deadline=time.monotonic()+SOCKET_SECONDS
    with socket.socket(socket.AF_UNIX) as connection:
        connection.settimeout(SOCKET_SECONDS)
        connection.connect(str(path))
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('Mapper command deadline expired')
        connection.settimeout(remaining)
        connection.sendall(command.encode())
        connection.shutdown(socket.SHUT_WR)
        print(read_reply(connection,deadline))


if __name__=='__main__':
    main()
