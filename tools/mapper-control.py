#!/usr/bin/env python3
"""Drive this checkout's independent mapper pointer without desktop focus/input."""
import json,os,pathlib,socket,sys
root=pathlib.Path(__file__).resolve().parents[1]
pid=json.loads((root/'logs/current-atl-pid.json').read_text())['pid']
if pathlib.Path(f'/proc/{pid}/exe').resolve()!=root/'prefix/bin/android-translation-layer':
    raise SystemExit('Owned ATL identity changed')
path=pathlib.Path(os.environ.get('XDG_RUNTIME_DIR',f'/run/user/{os.getuid()}'))/'clbb'/f'mapper-{pid}.sock'
command=' '.join(sys.argv[1:])
if not command or len(command.encode())>900 or '\n' in command:raise SystemExit('Expected one bounded mapper command')
with socket.socket(socket.AF_UNIX) as connection:
    connection.settimeout(3)
    connection.connect(str(path))
    connection.sendall(command.encode())
    connection.shutdown(socket.SHUT_WR)
    print(connection.recv(4096).decode().rstrip())
