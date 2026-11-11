#!/usr/bin/env python3
"""One geometry-checked CLBB touch through its own Android queue, no X input."""
import argparse
import json
import os
from pathlib import Path
import socket
import stat
import struct

R = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
for name in ('pid','birth-ticks','x','y','width','height'):
    p.add_argument('--'+name,required=True,type=int)
a = p.parse_args()
assert a.pid > 0 and a.birth_ticks > 0 and 0 <= a.x < a.width <= 4096 and 0 <= a.y < a.height <= 4096

def verify():
    proc = Path('/proc',str(a.pid))
    f = (proc/'stat').read_text().rsplit(')',1)[1].split()
    assert f[0] not in ('Z','X','x') and int(f[19]) == a.birth_ticks
    assert (proc/'exe').resolve(strict=True) == R/'prefix/bin/android-translation-layer'

verify()
path = Path('/run/user',str(os.getuid()),'clbb',f'mapper-{a.pid}.sock')
s = path.lstat()
assert stat.S_ISSOCK(s.st_mode) and s.st_uid == os.getuid()
with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
    client.settimeout(2)
    client.connect(str(path))
    pid,uid,gid = struct.unpack('3i',client.getsockopt(socket.SOL_SOCKET,socket.SO_PEERCRED,12))
    assert pid == a.pid and uid == os.getuid()
    verify()
    client.sendall(f'tap-window {a.x} {a.y} {a.width} {a.height}\n'.encode('ascii'))
    reply = client.recv(128).decode('ascii').strip()
verify()
print(json.dumps({'pid':a.pid,'ack':reply,'transport_only':True,'desktop_input_sent':False}))
raise SystemExit(0 if reply == 'OK tap' else 1)
