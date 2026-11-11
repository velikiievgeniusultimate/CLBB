#!/usr/bin/env python3
"""Stop only this checkout's recorded ATL instance; retire only its private diagnostic XI2 seat."""
from pathlib import Path
import json,os,signal,time,subprocess
ROOT=Path(__file__).resolve().parents[1]
pid=json.loads((ROOT/'logs/current-atl-pid.json').read_text())['pid']
proc=Path('/proc',str(pid))
from runtime_identity import runtime_executable_is_owned
def identity():
 try:
  if not (proc/'exe').exists():return None
  if not runtime_executable_is_owned(pid,ROOT):raise RuntimeError('Recorded PID is not owned ATL')
  stat=(proc/'stat').read_text().rsplit(')',1)[1].split()
  if stat[0] in ('Z','X','x'):return None
  return stat[19]
 except FileNotFoundError:return None
birth=identity()
if birth is not None:
 for sig,seconds in [(signal.SIGTERM,4),(signal.SIGKILL,3)]:
  current=identity()
  if current is None:break
  if current!=birth:raise RuntimeError('Recorded PID was reused')
  os.kill(pid,sig);deadline=time.monotonic()+seconds
  while time.monotonic()<deadline and identity()==birth:time.sleep(.025)
 if identity() is not None:raise RuntimeError('Owned runtime did not stop; refusing replacement')
# The private XI2 seat must survive while GTK is alive, then be removed so
# repeated fresh launches do not exhaust legacy XTEST device-ID limits.
# --cleanup refuses live PIDs and selects only our exact diagnostic seat name.
cleanup = 'helper_absent'
helper=ROOT/'tools/independent-pointer-tap'
if helper.is_file():
 result=subprocess.run([str(helper),'--cleanup',str(pid)],env=dict(os.environ,DISPLAY=':1'),capture_output=True,text=True,timeout=5)
 if result.returncode not in (0,9):raise RuntimeError('Owned diagnostic seat cleanup failed: '+result.stderr[-1000:])
 cleanup='completed' if result.returncode==0 else 'skipped_live_pid'
print(json.dumps(dict(pid=pid,already_exited=birth is None,stopped=True,private_diagnostic_seat_cleanup=cleanup)))
