#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Private parallel CLBB workbench; separate login/config/cache with COW assets.

Never copies Android preferences, login_data, web cookies or databases. Runtime
and APKs are read-only shared. fuse-overlayfs redirects resource writes to this
instance; bubblewrap masks the primary checkout and isolates /tmp. No desktop
mouse or focus operations. The normal shortcut and runtime selection are untouched.
"""
import argparse,fcntl,hashlib,importlib.util,json,os,re,shutil,socket,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
RESOURCES={
 'assets':'data/base.apk_/files/dragon2017/assets',
 'plugins':'data/base.apk_/files/libs',
 'libraries':'data/base.apk_/lib',
 'obb':'data/base.apk_/Android/obb',
}
BASE=Path.home()/'.local/share/clbb-debug-instances'

def write(path,value):
 path=Path(path);path.parent.mkdir(parents=True,exist_ok=True);tmp=path.with_name(path.name+'.pending');tmp.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n');tmp.chmod(0o600);os.replace(tmp,path)
def profile(name):
 if not re.fullmatch(r'[a-z][a-z0-9_-]{0,31}',name):raise ValueError('Use a short Latin profile name, e.g. secondary')
 return BASE/name

def selection():
 current=json.loads((ROOT/'.clbb-local-runtime.json').read_text())
 pending=ROOT/'logs/wind-item-cancel-2026-11-16/pending-activation.json'
 if pending.exists():
  activation=json.loads(pending.read_text())
  if current==activation['old']:return activation['new']
 return current

def sync_runtime(state):
 path=state/'.clbb-local-runtime.json'
 selected={**selection(),'state':str(state)}
 if path.exists() and json.loads(path.read_text())==selected:return
 candidate=Path(selected['directory'])
 subprocess.run([sys.executable,str(ROOT/'distribution/release.py'),'verify',str(candidate)],check=True)
 write(path,selected)

def prepare(name, *, interactive=False):
 state=profile(name)
 if state.exists():
  marker=json.loads((state/'.clbb-debug-instance.json').read_text())
  if marker.get('source_root')!=str(ROOT) or marker.get('state_root')!=str(state):raise ValueError('Foreign existing debug profile')
  return state
 selected=selection();candidate=Path(selected['directory']);subprocess.run([sys.executable,str(ROOT/'distribution/release.py'),'verify',str(candidate)],check=True)
 state.mkdir(parents=True,mode=0o700);marker={'schema':1,'name':name,'source_root':str(ROOT),'state_root':str(state),'account_state':'fresh; never imported from primary','shared_resources':RESOURCES,'created_ns':time.time_ns()};write(state/'.clbb-debug-instance.json',marker)
 for folder in ['game','logs','run','config/clbb','state','userdata','data/base.apk_/files','cache']:(state/folder).mkdir(parents=True,exist_ok=True)
 (state/'run').chmod(0o700)
 for resource in RESOURCES.values():(state/resource).mkdir(parents=True,exist_ok=True)
 # Only the developer mapper configuration is cloned, never Android/game data.
 settings_path=Path.home()/'.config/clbb/settings.json'
 settings=json.loads(settings_path.read_text()) if settings_path.is_file() else {'values':{}}
 if not interactive:
  settings['values'].update({'display.resolution':'1280x720','display.fps_limit':'60','display.vsync':'true','audio.muted':'true','debug.recording':'true'})
  settings['values'].pop('display.imported_resolution',None)
 write(state/'config/clbb/settings.json',settings)
 cached=ROOT/'cache/native-apk-libraries.json'
 if cached.is_file():shutil.copy2(cached,state/'cache/native-apk-libraries.json')
 write(state/'.clbb-local-runtime.json',{**selected,'state':str(state)})
 # Prepare private fonts/native metadata in the sandbox at launch. No primary
 # account folder, cookie store, device identity, or game file is copied here.
 return state

def mounts(state):
 for name,relative in RESOURCES.items():
  lower=ROOT/relative;target=state/relative
  if not lower.is_dir():continue
  if subprocess.run(['mountpoint','-q',str(target)]).returncode==0:
   kind=subprocess.check_output(['findmnt','-n','-o','FSTYPE','--target',str(target)],text=True).strip()
   if kind!='fuse.fuse-overlayfs':raise ValueError('Foreign mount at '+str(target))
   continue
  layer=state/'.overlays'/name
  for folder in ['upper','work']:(layer/folder).mkdir(parents=True,exist_ok=True)
  subprocess.run(['fuse-overlayfs','-o',f'lowerdir={lower},upperdir={layer}/upper,workdir={layer}/work',str(target)],check=True)

def sandbox(state,command):
 selected=json.loads((state/'.clbb-local-runtime.json').read_text());runtime=Path(selected['directory'])/'runtime';host_run=os.environ.get('XDG_RUNTIME_DIR',f'/run/user/{os.getuid()}')
 (state/'code_cache').mkdir(exist_ok=True)
 args=['bwrap','--ro-bind','/','/','--bind',str(state),str(state)]
 cache_mount=runtime/'code_cache'
 if not cache_mount.exists():
  # Older packages lack the mount point. Construct the top level in this namespace
  # so a fresh profile can use its own ART cache even on a read-only installation.
  args+=['--tmpfs',str(runtime)]
  for entry in sorted(runtime.iterdir()):
   args+=['--ro-bind',str(entry),str(entry)]
  args+=['--dir',str(cache_mount),'--remount-ro',str(runtime)]
 args+=['--bind',str(state/'code_cache'),str(cache_mount),'--ro-bind',str(ROOT/'game'),str(state/'game'),'--tmpfs','/tmp','--ro-bind','/tmp/.X11-unix','/tmp/.X11-unix','--proc','/proc','--dev-bind','/dev','/dev','--tmpfs',str(ROOT),'--chdir',str(runtime)]
 env={
  'CLBB_ROOT':str(runtime),'CLBB_STATE_ROOT':str(state),'CLBB_MANAGED_INSTALL':'0',
  'CLBB_WRAPPED':'1', # File reports only: never spawn Zenity for private tests.
  # The entire debug game is already in the outer filesystem sandbox. Avoid
  # WebKit trying to build an incompatible second nested mount sandbox.
  'WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS':'1',
  'CLBB_BACKGROUND_STARTUP':'1','XDG_CONFIG_HOME':str(state/'config'),
  'XDG_DATA_HOME':str(state/'userdata'),'XDG_STATE_HOME':str(state/'state'),
  'XDG_RUNTIME_DIR':str(state/'run'),'PIPEWIRE_RUNTIME_DIR':host_run,
  'QEMU_TEMPDIR':str(state/'code_cache'),'TMPDIR':'/tmp',
  'PULSE_SERVER':'unix:'+host_run+'/pulse/native','DISPLAY':os.environ.get('DISPLAY',':1'),
 }
 for key,value in env.items():args+=['--setenv',key,value]
 return args+['--',*command]

def probe(state):
 spec=importlib.util.spec_from_file_location('clbb_debug_probe',ROOT/'tools/probe-hero-control.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
 meta=json.loads((state/'logs/current-atl-pid.json').read_text());return m.Probe(0,{'commands':[],'samples':[]},meta['pid'],state_root=state)

def control(state,command):
 p=probe(state)
 if command.startswith(('pointer ','key ','mouse ')) or command=='begin':
  p.hero=p.read('hero.id')
  if p.hero<=0:raise ValueError('No active debug match')
 return p.command(command)

def live(state):
 try:p=probe(state);p.identity();return p.pid
 except (FileNotFoundError,ProcessLookupError,RuntimeError):return None

def start_supervisor(name):
 state=prepare(name)
 if live(state):raise ValueError('This debug profile is already running')
 sync_runtime(state)
 mounts(state)
 # Test actual namespace boundaries before any game process is started.
 check="import os,json; from pathlib import Path; p=Path(os.environ['CLBB_STATE_ROOT']); q=p/'.namespace-write-check'; q.write_text('private'); q.unlink(); print(json.dumps({'primary_hidden':not Path("+repr(str(ROOT/'game/base.apk'))+").exists(),'private_tmp':not Path('/tmp/atl_cache').exists(),'config':os.environ['XDG_CONFIG_HOME']}))"
 test=json.loads(subprocess.check_output(sandbox(state,[sys.executable,'-c',check]),text=True));assert test['primary_hidden']and test['private_tmp'];write(state/'logs/namespace-check.json',test)
 runtime=Path(json.loads((state/'.clbb-local-runtime.json').read_text())['directory'])/'runtime'
 subprocess.run(['systemctl','--user','start','clbbdirect.slice'],check=True)
 subprocess.run(['sudo','-n','/usr/local/sbin/clbb-direct-route','--sync'],check=True)
 command=['systemd-run','--user','--scope','--quiet','--slice=clbbdirect.slice','--property=CPUWeight=10','--property=IOWeight=10','nice','-n','10',*sandbox(state,[str(runtime/'bin/clbb-launcher')])]
 with (state/'logs/supervisor.log').open('ab')as log:
  child=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=log,stderr=log,start_new_session=True)
 write(state/'logs/supervisor.json',{'pid':child.pid,'started_ns':time.monotonic_ns(),'background_focus':True})
 return child

def start(name):
 child=start_supervisor(name)
 return {'profile':name,'state':str(profile(name)),'supervisor_pid':child.pid,'account':'private persistent profile; primary login is never imported'}

def window(name,show):
 import ctypes as C,ctypes.util
 state=profile(name);owned=probe(state);owned.identity();pid=owned.pid
 display_name=os.environ.get('DISPLAY',':1')
 if not re.fullmatch(r':[0-9]{1,5}(?:\.[0-9]{1,3})?',display_name):raise ValueError('Explicit local X11 display required')
 X=C.CDLL(ctypes.util.find_library('X11'));D=C.c_void_p;W=C.c_ulong;I=C.c_int
 X.XOpenDisplay.argtypes=[C.c_char_p];X.XOpenDisplay.restype=D
 X.XDefaultRootWindow.argtypes=[D];X.XDefaultRootWindow.restype=W
 X.XDefaultScreen.argtypes=[D];X.XDefaultScreen.restype=I
 X.XInternAtom.argtypes=[D,C.c_char_p,I];X.XInternAtom.restype=W
 X.XGetWindowProperty.argtypes=[D,W,W,C.c_long,C.c_long,I,W,C.POINTER(W),C.POINTER(I),C.POINTER(W),C.POINTER(W),C.POINTER(D)]
 X.XFree.argtypes=[D];X.XCloseDisplay.argtypes=[D];X.XSync.argtypes=[D,I]
 X.XIconifyWindow.argtypes=[D,W,I];X.XMapWindow.argtypes=[D,W]
 X.XMoveWindow.argtypes=[D,W,I,I]
 X.XChangeProperty.argtypes=[D,W,W,W,I,I,C.c_void_p,I]
 d=X.XOpenDisplay(display_name.encode())
 if not d:raise ValueError('X11 display unavailable')
 def values(w,key):
  actual=W();fmt=I();count=W();remaining=W();data=D()
  X.XGetWindowProperty(d,w,X.XInternAtom(d,key.encode(),0),0,4096,0,0,C.byref(actual),C.byref(fmt),C.byref(count),C.byref(remaining),C.byref(data))
  result=list(C.cast(data,C.POINTER(W))[:count.value])if data and fmt.value==32 else []
  if data:X.XFree(data)
  return result
 try:
  candidates=[w for w in values(X.XDefaultRootWindow(d),'_NET_CLIENT_LIST')if values(w,'_NET_WM_PID')==[pid]]
  if not candidates:raise ValueError('No owned debug windows available')
  owned.identity()
  for index,w in enumerate(candidates):
   label=('CLBB · DEBUG · '+name).encode();X.XChangeProperty(d,w,X.XInternAtom(d,b'_NET_WM_NAME',0),X.XInternAtom(d,b'UTF8_STRING',0),8,0,label,len(label))
   if show:
    X.XMoveWindow(d,w,1940+index*15,30+index*15) # Observed Philips HDMI-A-1.
    X.XMapWindow(d,w) # No activation, raising, or desktop input.
   else:X.XIconifyWindow(d,w,X.XDefaultScreen(d))
  X.XSync(d,0)
  hidden_atom=X.XInternAtom(d,b'_NET_WM_STATE_HIDDEN',0);end=time.monotonic()+.5
  while True:
   hidden={hex(w):hidden_atom in values(w,'_NET_WM_STATE')for w in candidates}
   if all(value==(not show) for value in hidden.values()) or time.monotonic()>=end:break
   time.sleep(.02)
  owned.identity();return {'profile':name,'pid':pid,'windows':[hex(w)for w in candidates],'visible_requested':show,'hidden_observed':hidden,'focus_requested':False}
 finally:X.XCloseDisplay(d)

def status(name):
 state=profile(name);pid=live(state)
 out={'profile':name,'state':str(state),'pid':pid,'running':bool(pid)}
 if pid:
  for key in ['hero.id','diagnostics.contacts','diagnostics.mapper_build']:
   raw=control(state,'read '+key)
   try:out[key]=json.loads(raw)
   except ValueError:out[key]=raw
 return out

def stop(name,keep_resources=False):
 state=profile(name)
 if live(state):control(state,'close')
 # Do not unmount until the exact owned secondary process has actually exited.
 end=time.monotonic()+3
 while live(state) and time.monotonic()<end:time.sleep(.05)
 if live(state):raise ValueError('Secondary still live; keeping its resource mounts')
 for relative in ([] if keep_resources else reversed(list(RESOURCES.values()))):
  target=state/relative
  if subprocess.run(['mountpoint','-q',str(target)]).returncode==0:subprocess.run(['fusermount3','-u',str(target)],check=True)
 return {'profile':name,'stopped':True,'account_preserved':True}

def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('action',choices=['prepare','start','status','stop','restart','show','hide','command']);parser.add_argument('profile');parser.add_argument('command',nargs='*');args=parser.parse_args()
 if args.action=='prepare':result={'state':str(prepare(args.profile))}
 elif args.action=='start':result=start(args.profile)
 elif args.action=='status':result=status(args.profile)
 elif args.action=='stop':result=stop(args.profile)
 elif args.action=='restart':
  stop(args.profile,keep_resources=True);result=start(args.profile)
 elif args.action in ['show','hide']:result=window(args.profile,args.action=='show')
 else:result=control(profile(args.profile),' '.join(args.command))
 print(result if isinstance(result,str)else json.dumps(result,ensure_ascii=False))
if __name__=='__main__':
 try:main()
 except (OSError,ValueError,RuntimeError,subprocess.SubprocessError)as error:raise SystemExit('CLBB debug instance: '+str(error))
